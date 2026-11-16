//! Extract a data-only ISO-9660 image from the supplied CloneCD .img + .cue files.
 //! This is intentionally a read-only raw-sector converter; it does not execute game programs.
 //!
 //! cargo run -p game-core --example cd_to_iso -- "path/to/disc.cue" "path/to/output.iso"
 //!
 //! Supports a first data track (MODE1/2352 or MODE2/2352 XA Form 1).
 //! For mixed-mode CDs, subsequent CD Audio tracks are excluded.
 //! Rejects XA Form 2 sectors rather than silently truncating their data.

use std::env;
use std::error::Error;
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

const RAW_SECTOR: usize = 2352;
const USER_SECTOR: usize = 2048;
const SYNC: [u8; 12] = [
    0x00, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xff, 0xff, 0xff, 0xff, 0xff, 0x00,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TrackKind {
    Data(u8),
    Audio,
}

#[derive(Debug)]
struct Track {
    kind: TrackKind,
    index0: Option<u64>,
    index1: Option<u64>,
}

fn cue_time(text: &str) -> Result<u64, String> {
    let items: Vec<&str> = text.split(':').collect();
    if items.len() != 3 {
        return Err(format!("invalid cue timestamp: {text}"));
    }
    let mm: u64 = items[0].parse().map_err(|_| "invalid CUE minutes")?;
    let ss: u64 = items[1].parse().map_err(|_| "invalid CUE seconds")?;
    let ff: u64 = items[2].parse().map_err(|_| "invalid CUE frames")?;
    if ss >= 60 || ff >= 75 {
        return Err(format!("invalid cue timestamp: {text}"));
    }
    Ok((mm * 60 + ss) * 75 + ff)
}

fn parse_tracks(bytes: &[u8]) -> Result<Vec<Track>, String> {
    // File names in early CloneCD CUE sheets may be encoded as Windows-1252.
    // Only the ASCII TRACK/INDEX fields are needed, so lossy decoding is fine.
    let text = String::from_utf8_lossy(bytes);
    let mut tracks: Vec<Track> = Vec::new();
    for line in text.lines() {
        let parts: Vec<_> = line.split_ascii_whitespace().collect();
        if parts.is_empty() {
            continue;
        }
        if parts[0].eq_ignore_ascii_case("TRACK") {
            if parts.len() < 3 {
                return Err("incomplete TRACK line".into());
            }
            let kind = if parts[2].eq_ignore_ascii_case("AUDIO") {
                TrackKind::Audio
            } else if parts[2].eq_ignore_ascii_case("MODE1/2352") {
                TrackKind::Data(1)
            } else if parts[2].eq_ignore_ascii_case("MODE2/2352") {
                TrackKind::Data(2)
            } else {
                return Err(format!("unsupported track type: {}", parts[2]));
            };
            tracks.push(Track { kind, index0: None, index1: None });
        } else if parts[0].eq_ignore_ascii_case("INDEX") {
            if parts.len() < 3 {
                return Err("incomplete INDEX line".into());
            }
            let track = tracks.last_mut().ok_or("INDEX before TRACK")?;
            match parts[1] {
                "0" | "00" => track.index0 = Some(cue_time(parts[2])?),
                "1" | "01" => track.index1 = Some(cue_time(parts[2])?),
                _ => {}
            }
        }
    }
    if tracks.is_empty() {
        return Err("no tracks found in cue file".into());
    }
    Ok(tracks)
}

fn data_track_range(tracks: &[Track], total_sectors: u64) -> Result<(u8, u64, u64), String> {
    let first = tracks.first().ok_or("no tracks")?;
    let mode = match first.kind {
        TrackKind::Data(m) => m,
        TrackKind::Audio => return Err("first CD track is audio, not game data".into()),
    };
    let start = first.index1.ok_or("first track has no INDEX 01")?;
    // If the following track has an INDEX 00 pregap, that pregap belongs to
    // the next track and must be excluded from this data image.
    let end = if let Some(next) = tracks.get(1) {
        next.index0.or(next.index1).ok_or("next track has no index")?
    } else {
        total_sectors
    };
    if start >= end || end > total_sectors {
        return Err("track boundary outside of source image".into());
    }
    Ok((mode, start, end))
}

fn user_data<'a>(sector: &'a [u8; RAW_SECTOR], mode: u8) -> Result<&'a [u8], String> {
    if sector[..12] != SYNC {
        return Err("invalid CD sector sync; scrambled or non-data sector".into());
    }
    if sector[15] != mode {
        return Err(format!(
            "unexpected sector mode {}, expected {mode}", sector[15]
        ));
    }
    let offset = match mode {
        1 => 16,
        2 => {
            if sector[16..20] != sector[20..24] {
                return Err("XA subheaders differ".into());
            }
            if sector[18] & 0x20 != 0 {
                return Err("XA Form 2 sector encountered; cannot convert to ISO".into());
            }
            24
        }
        _ => return Err("unsupported CD sector mode".into()),
    };
    Ok(&sector[offset..offset + USER_SECTOR])
}

fn verify_iso9660(input: &mut File, start: u64, mode: u8) -> Result<(), Box<dyn Error>> {
    let lba16 = start
        .checked_add(16)
        .and_then(|n| n.checked_mul(RAW_SECTOR as u64))
        .ok_or("sector offset overflow")?;
    input.seek(SeekFrom::Start(lba16))?;
    let mut sector = [0u8; RAW_SECTOR];
    input.read_exact(&mut sector)?;
    let data = user_data(&sector, mode)?;
    if data[0] != 1 || &data[1..6] != b"CD001" || data[6] != 1 {
        return Err("ISO-9660 primary volume descriptor not found at logical sector 16".into());
    }
    Ok(())
}

fn convert(cue_path: &Path, dest: &Path) -> Result<(), Box<dyn Error>> {
    let tracks = parse_tracks(&fs::read(cue_path)?)?;
    let image_path = cue_path.with_extension("img");
    if !image_path.is_file() {
        return Err(format!("source .img not found: {}", image_path.display()).into());
    }
    let mut source = File::open(&image_path)?;
    let size = source.metadata()?.len();
    if size % RAW_SECTOR as u64 != 0 {
        return Err("raw image size is not a multiple of 2352 bytes".into());
    }
    let (mode, start, end) = data_track_range(&tracks, size / RAW_SECTOR as u64)?;

    // Verify ISO volume descriptor before creating a large output file.
    verify_iso9660(&mut source, start, mode)?;
    source.seek(SeekFrom::Start(start * RAW_SECTOR as u64))?;

    let sector_count = end - start;
    let expected_len = sector_count * USER_SECTOR as u64;
    println!("Source: {}", image_path.display());
    println!("Mode: {mode}, data sectors: {sector_count}");
    println!("Output: {} ({} bytes)", dest.display(), expected_len);

    // Never overwrite existing ISO or a previous incomplete .part file.
    let partial = PathBuf::from(format!("{}.part", dest.display()));
    let output = OpenOptions::new().write(true).create_new(true).open(&partial)?;
    let mut output = BufWriter::new(output);
    let result: Result<(), Box<dyn Error>> = (|| {
        let mut input = BufReader::with_capacity(1024 * 1024, source);
        let mut sector = [0u8; RAW_SECTOR];
        for i in 0..sector_count {
            input.read_exact(&mut sector)?;
            let data = user_data(&sector, mode)
                .map_err(|err| format!("sector {}: {}", start + i, err))?;
            output.write_all(data)?;
        }
        output.flush()?;
        Ok(())
    })();
    drop(output);
    if let Err(error) = result {
        let _ = fs::remove_file(&partial);
        return Err(error);
    }
    if dest.exists() {
        fs::remove_file(&partial)?;
        return Err("destination ISO already exists; refusing to overwrite".into());
    }
    fs::rename(&partial, dest)?;
    println!("Success. ISO-9660 data image created without CD audio tracks.");
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args_os().skip(1);
    let cue = args.next().ok_or(
        "Usage: cargo run -p game-core --example cd_to_iso -- <disc.cue> <output.iso>"
    )?;
    let output = args.next().ok_or("missing output ISO path")?;
    if args.next().is_some() {
        return Err("too many arguments".into());
    }
    convert(Path::new(&cue), Path::new(&output))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_multitrack_disc_and_excludes_audio() {
        let cue = b"FILE \"Volker.img\" BINARY\n TRACK 1 MODE1/2352\n INDEX 1 00:00:00\n TRACK 2 AUDIO\n INDEX 1 54:14:05\n TRACK 3 AUDIO\n INDEX 1 55:46:58\n";
        let tracks = parse_tracks(cue).unwrap();
        assert_eq!(data_track_range(&tracks, 350005).unwrap(), (1, 0, 244055));
    }

    #[test]
    fn parses_single_mode2_disc() {
        let tracks = parse_tracks(
            b"FILE \"Mission.img\" BINARY\n TRACK 1 MODE2/2352\n INDEX 1 00:00:00\n"
        ).unwrap();
        assert_eq!(data_track_range(&tracks, 65993).unwrap(), (2, 0, 65993));
    }

    #[test]
    fn extracts_mode1_and_mode2_form1_payloads() {
        let mut sector = [0u8; RAW_SECTOR];
        sector[..12].copy_from_slice(&SYNC);
        sector[15] = 1;
        sector[16] = 0xab;
        sector[2063] = 0xcd;
        assert_eq!(user_data(&sector, 1).unwrap()[0], 0xab);
        assert_eq!(user_data(&sector, 1).unwrap()[2047], 0xcd);
        sector[15] = 2;
        sector[16..24].fill(0);
        sector[24] = 0x12;
        assert_eq!(user_data(&sector, 2).unwrap()[0], 0x12);
        sector[18] = 0x20;
        sector[22] = 0x20;
        assert!(user_data(&sector, 2).is_err());
    }
}
