//! Export original Alien Nations .gd sprite animation frames to transparent PNGs.
//!
//! Reads local game assets only; never publishes, uploads, or executes game content.
//!
//! cargo run -p game-core --example gd_export_png -- <archive.gd> <exact-sprite-name> <output-dir> [frame-count]
//! Example: cargo run -p game-core --example gd_export_png -- "C:\Game\ADDON.GD" "SA\VE\TRANSPORTSCHIFF\MG7" "C:\Game\Preview"

use game_core::gd::{OpacityMode, decode_gd_frames, gd_index_size, parse_gd_index};
use game_core::png::encode_rgba_png;
use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args_os().skip(1);
    let path = args.next().ok_or("usage: gd_export_png <archive.gd> <sprite-name> <output-directory> [frame-count]")?;
    let name = args.next().ok_or("missing exact sprite name")?
        .into_string().map_err(|_| "sprite name must be valid Unicode")?;
    let dest = args.next().ok_or("missing output directory")?;
    let requested = args.next()
        .map(|v| v.to_string_lossy().parse::<usize>())
        .transpose()?;
    if args.next().is_some() {
        return Err("too many arguments".into());
    }

    let mut file = File::open(&path)?;
    let file_size = usize::try_from(file.metadata()?.len())?;
    let mut header = [0u8; 24];
    file.read_exact(&mut header)?;
    let index_size = gd_index_size(&header)?;
    if !(24..=64 * 1024 * 1024).contains(&index_size) || index_size > file_size {
        return Err("invalid .gd index size".into());
    }

    let mut prefix = vec![0u8; index_size];
    file.seek(SeekFrom::Start(0))?;
    file.read_exact(&mut prefix)?;
    let index = parse_gd_index(&prefix, file_size)?;
    let entry = index.get(&name).ok_or("sprite name not found; use gd_inspect to list names")?;
    if entry.data_length == 0 || entry.data_length > 64 * 1024 * 1024 {
        return Err("invalid sprite block length".into());
    }
    let frame_count = requested.unwrap_or(entry.frames);
    if frame_count == 0 || frame_count > entry.frames || frame_count > 256 {
        return Err("requested frame count must be between 1 and min(sprite.frames, 256)".into());
    }

    let mut block = vec![0u8; entry.data_length];
    file.seek(SeekFrom::Start(u64::try_from(entry.data_offset)?))?;
    file.read_exact(&mut block)?;
    let frames = decode_gd_frames(
        entry, &block, Some(frame_count - 1), OpacityMode::Preserve,
    )?;

    let safe_name: String = name.chars().map(|c| {
        if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }
    }).collect();
    if safe_name.is_empty() {
        return Err("empty output file name".into());
    }

    fs::create_dir_all(&dest)?;
    println!("Sprite: {name}, {}x{}, {} frame(s)", entry.width, entry.height, frames.len());
    for (frame_no, rgba) in frames.iter().enumerate() {
        let filename = format!("{safe_name}_{frame_no:03}.png");
        let target = Path::new(&dest).join(filename);
        let png = encode_rgba_png(entry.width, entry.height, rgba)?;
        fs::write(&target, png)?;
        println!("Saved frame {frame_no}: {}", target.display());
    }
    println!("Done. Open the PNG files to inspect original sprite transparency.");
    Ok(())
}
