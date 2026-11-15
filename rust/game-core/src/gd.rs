//! Reader for Alien Nations .gd sprite archives.
//!
//! Ported from src/formats/gd.ts using docs/formats/gd.md as the format specification.
//! Game artwork is NOT included. Callers supply bytes from a legally obtained installation.
//! All pixel outputs use straight (non-premultiplied) RGBA, row-major.

use std::collections::HashMap;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GdError {
    Truncated,
    Invalid(&'static str),
    UnsupportedEncoding(u16),
    MissingFrame(usize),
    InvalidOpcode(u16),
}

impl fmt::Display for GdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for GdError {}

pub type GdResult<T> = Result<T, GdError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GdEntry {
    pub name: String,
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
    pub width: usize,
    pub height: usize,
    pub frames: usize,
    pub encoding: u16,
    pub data_offset: usize,
    pub data_length: usize,
}

#[derive(Debug)]
pub struct GdIndex {
    pub entries: Vec<GdEntry>,
    /// Name to index in entries.
    pub by_name: HashMap<String, usize>,
    /// Length of the archive metadata prefix, before pixel data.
    pub index_size: usize,
}

impl GdIndex {
    pub fn get(&self, name: &str) -> Option<&GdEntry> {
        self.by_name.get(name).map(|&i| &self.entries[i])
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpacityMode {
    /// Keep the alpha channel as stored; required for world sprites and doors.
    Preserve,
    /// Treat every written pixel as fully opaque.
    ForceOpaque,
    /// For menu/interface sprites only: force opaque if all alpha is zero,
    /// but some nonzero colour was written.
    AutoOpaque,
}

fn section(bytes: &[u8], off: usize, length: usize) -> GdResult<&[u8]> {
    let end = off.checked_add(length).ok_or(GdError::Invalid("offset overflow"))?;
    bytes.get(off..end).ok_or(GdError::Truncated)
}

fn u16_at(bytes: &[u8], off: usize) -> GdResult<u16> {
    Ok(u16::from_le_bytes(section(bytes, off, 2)?.try_into().unwrap()))
}
fn u32_at(bytes: &[u8], off: usize) -> GdResult<u32> {
    Ok(u32::from_le_bytes(section(bytes, off, 4)?.try_into().unwrap()))
}
fn i32_at(bytes: &[u8], off: usize) -> GdResult<i32> {
    Ok(i32::from_le_bytes(section(bytes, off, 4)?.try_into().unwrap()))
}

/// Determine how many bytes to fetch for the index, using the first 20 header bytes.
pub fn gd_index_size(header: &[u8]) -> GdResult<usize> {
    Ok(u32_at(header, 16)? as usize)
}

const CP1252_HIGH: [u32; 32] = [
    0x20ac, 0x81, 0x201a, 0x192, 0x201e, 0x2026, 0x2020, 0x2021,
    0x2c6, 0x2030, 0x160, 0x2039, 0x152, 0x8d, 0x17d, 0x8f,
    0x90, 0x2018, 0x2019, 0x201c, 0x201d, 0x2022, 0x2013, 0x2014,
    0x2dc, 0x2122, 0x161, 0x203a, 0x153, 0x9d, 0x17e, 0x178,
];

fn cp1252(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| {
        let code = if (0x80..=0x9f).contains(&b) {
            CP1252_HIGH[usize::from(b - 0x80)]
        } else {
            u32::from(b)
        };
        char::from_u32(code).unwrap_or('\u{fffd}')
    }).collect()
}

fn cstr(bytes: &[u8], off: usize) -> GdResult<String> {
    let tail = bytes.get(off..).ok_or(GdError::Truncated)?;
    let end = tail.iter().take(4096).position(|&b| b == 0)
        .ok_or(GdError::Invalid("unterminated sprite name"))?;
    Ok(cp1252(&tail[..end]))
}

/// Parse only the metadata prefix (at least gd_index_size bytes).
/// file_size is the size of the *entire* .gd archive, including pixel blocks.
pub fn parse_gd_index(bytes: &[u8], file_size: usize) -> GdResult<GdIndex> {
    let names_off = u32_at(bytes, 0)? as usize;
    let names_index_off = u32_at(bytes, 8)? as usize;
    let records_off = u32_at(bytes, 12)? as usize;
    let pixels_off = gd_index_size(bytes)?;
    let count = u32_at(bytes, 20)? as usize;

    if pixels_off < 24 || pixels_off > file_size || pixels_off > bytes.len() {
        return Err(GdError::Invalid("invalid index boundary"));
    }
    let prefix = &bytes[..pixels_off];
    let mut entries = Vec::new();
    let mut off = records_off;
    for _ in 0..count {
        let size = u32_at(prefix, off)? as usize;
        if size < 12 {
            return Err(GdError::Invalid("entry record smaller than header"));
        }
        let kind = u32_at(prefix, off + 4)?;
        let name_no = u32_at(prefix, off + 8)? as usize;
        let payload = off.checked_add(size)
            .ok_or(GdError::Invalid("record offset overflow"))?;
        section(prefix, off, size)?;
        if kind == 0 {
            section(prefix, payload, 52)?;
            let name_offset_pos = names_index_off
                .checked_add(name_no.checked_mul(4)
                    .ok_or(GdError::Invalid("name index overflow"))?)
                .ok_or(GdError::Invalid("name index overflow"))?;
            let name_rel = u32_at(prefix, name_offset_pos)? as usize;
            let name_off = names_off.checked_add(name_rel)
                .ok_or(GdError::Invalid("name offset overflow"))?;
            let left = i32_at(prefix, payload + 4)?;
            let top = i32_at(prefix, payload + 8)?;
            let right = i32_at(prefix, payload + 12)?;
            let bottom = i32_at(prefix, payload + 16)?;
            let width = i64::from(right) - i64::from(left);
            let height = i64::from(bottom) - i64::from(top);
            if width <= 0 || height <= 0 {
                return Err(GdError::Invalid("invalid sprite dimensions"));
            }
            let frame_count = u32_at(prefix, payload + 36)? as usize;
            if frame_count == 0 {
                return Err(GdError::Invalid("sprite has no frames"));
            }
            let data_rel = u32_at(prefix, payload + 44)? as usize;
            let data_offset = pixels_off.checked_add(data_rel)
                .ok_or(GdError::Invalid("pixel offset overflow"))?;
            if data_offset > file_size {
                return Err(GdError::Invalid("pixel data outside archive"));
            }
            entries.push(GdEntry {
                name: cstr(prefix, name_off)?, left, top, right, bottom,
                width: width as usize, height: height as usize,
                frames: frame_count, encoding: u16_at(prefix, payload + 40)?,
                data_offset, data_length: 0,
            });
            off = payload + 52;
        } else {
            off = payload;
        }
    }

    // Pixel blocks occur in reverse entry order; determine their lengths by
    // sorting absolute offsets and locating the following block.
    let mut ordered: Vec<usize> = (0..entries.len()).collect();
    ordered.sort_by_key(|&i| entries[i].data_offset);
    for (j, &idx) in ordered.iter().enumerate() {
        let end = ordered.get(j + 1)
            .map(|&i| entries[i].data_offset).unwrap_or(file_size);
        entries[idx].data_length = end - entries[idx].data_offset;
    }
    let by_name = entries.iter().enumerate()
        .map(|(i, e)| (e.name.clone(), i)).collect();
    Ok(GdIndex { entries, by_name, index_size: pixels_off })
}

#[derive(Default)]
struct Seen { alpha: u32, colour: u32 }

fn premultiply_clamped(raw: u32) -> u32 {
    let a = raw >> 24;
    let r = ((raw >> 16) & 255).min(a);
    let g = ((raw >> 8) & 255).min(a);
    let b = (raw & 255).min(a);
    (a << 24) | (r << 16) | (g << 8) | b
}

/// Apply a delta stream to the same canvas used by preceding frames.
fn apply_frame(
    block: &[u8], start: usize, canvas: &mut [u32], width: usize, opaque: bool,
) -> GdResult<Seen> {
    let mut p = start;
    let mut row = 0usize;
    let mut cur = 0usize;
    let mut seen = Seen::default();
    loop {
        let opcode = u16_at(block, p)?;
        p += 2;
        let n = usize::from(opcode >> 4);
        let cmd = opcode & 0xf;
        match cmd {
            0 | 1 => {
                let count = if cmd == 0 { n + 1 } else { n + 2 };
                let end = cur.checked_add(count)
                    .ok_or(GdError::Invalid("pixel cursor overflow"))?;
                if end > canvas.len() {
                    return Err(GdError::Invalid("pixel write beyond sprite"));
                }
                let data_bytes = if cmd == 0 { count * 4 } else { 4 };
                section(block, p, data_bytes)?;
                for i in 0..count {
                    let raw = u32_at(block, p + if cmd == 0 { i * 4 } else { 0 })?;
                    seen.alpha |= raw >> 24;
                    seen.colour |= raw & 0x00ff_ffff;
                    canvas[cur + i] = if opaque {
                        raw | 0xff00_0000
                    } else {
                        premultiply_clamped(raw)
                    };
                }
                cur = end;
                p += data_bytes;
            }
            2 => {
                row = row.checked_add(n + 1)
                    .ok_or(GdError::Invalid("row overflow"))?;
                cur = row.checked_mul(width)
                    .ok_or(GdError::Invalid("row offset overflow"))?;
                if cur > canvas.len() {
                    return Err(GdError::Invalid("row outside sprite"));
                }
            }
            3 => {
                cur = cur.checked_add(n + 1)
                    .ok_or(GdError::Invalid("pixel cursor overflow"))?;
                if cur > canvas.len() {
                    return Err(GdError::Invalid("skip outside sprite"));
                }
            }
            4 => return Ok(seen),
            _ => return Err(GdError::InvalidOpcode(cmd)),
        }
    }
}

fn straight_rgba(canvas: &[u32]) -> Vec<u8> {
    let mut out = vec![0u8; canvas.len() * 4];
    for (i, &c) in canvas.iter().enumerate() {
        let a = c >> 24;
        if a == 0 { continue; }
        let convert = |v: u32| ((v * 255 + a / 2) / a).min(255) as u8;
        out[4 * i] = convert((c >> 16) & 255);
        out[4 * i + 1] = convert((c >> 8) & 255);
        out[4 * i + 2] = convert(c & 255);
        out[4 * i + 3] = a as u8;
    }
    out
}

fn decode_pass(
    block: &[u8], starts: &HashMap<usize, usize>, width: usize, pixels: usize,
    last_frame: usize, opaque: bool,
) -> GdResult<(Vec<Vec<u8>>, Seen)> {
    let mut canvas = vec![0u32; pixels];
    let mut images = Vec::with_capacity(last_frame + 1);
    let mut seen = Seen::default();
    for fi in 0..=last_frame {
        let start = *starts.get(&fi).ok_or(GdError::MissingFrame(fi))?;
        let stats = apply_frame(block, start, &mut canvas, width, opaque)?;
        seen.alpha |= stats.alpha;
        seen.colour |= stats.colour;
        images.push(straight_rgba(&canvas));
    }
    Ok((images, seen))
}

/// Decode frames 0..=last_frame. Each frame is an RGBA buffer (width * height * 4).
///
/// block starts at entry.data_offset, NOT at the start of the archive.
/// Use Preserve for dv.gd; AutoOpaque may be used for menu/interface archives.
pub fn decode_gd_frames(
    entry: &GdEntry, block: &[u8], last_frame: Option<usize>, opacity: OpacityMode,
) -> GdResult<Vec<Vec<u8>>> {
    if entry.encoding != 4 {
        return Err(GdError::UnsupportedEncoding(entry.encoding));
    }
    if entry.frames == 0 {
        return Err(GdError::Invalid("sprite has no frames"));
    }
    let last = last_frame.unwrap_or(entry.frames - 1);
    if last >= entry.frames {
        return Err(GdError::MissingFrame(last));
    }
    let pixels = entry.width.checked_mul(entry.height)
        .ok_or(GdError::Invalid("pixel count overflow"))?;
    // Guard against malformed archives allocating excessive memory.
    if pixels == 0 || pixels > 16_777_216 {
        return Err(GdError::Invalid("unsupported sprite dimensions"));
    }
    let mut starts = HashMap::new();
    let mut f = u32_at(block, 12)? as usize;
    for _ in 0..entry.frames {
        let idx = i32_at(block, f)?;
        let next = u32_at(block, f.checked_add(4)
            .ok_or(GdError::Invalid("frame header overflow"))?)? as usize;
        if idx >= 0 {
            let start = f.checked_add(8)
                .ok_or(GdError::Invalid("frame data offset overflow"))?;
            starts.insert(idx as usize, start);
        }
        if next == 0 { break; }
        f = f.checked_add(next).ok_or(GdError::Invalid("frame chain overflow"))?;
    }

    let opaque = opacity == OpacityMode::ForceOpaque;
    let (images, seen) = decode_pass(block, &starts, entry.width, pixels, last, opaque)?;
    if opacity == OpacityMode::AutoOpaque && seen.alpha == 0 && seen.colour != 0 {
        return Ok(decode_pass(block, &starts, entry.width, pixels, last, true)?.0);
    }
    Ok(images)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put32(b: &mut [u8], at: usize, v: u32) {
        b[at..at + 4].copy_from_slice(&v.to_le_bytes());
    }
    fn put16(b: &mut [u8], at: usize, v: u16) {
        b[at..at + 2].copy_from_slice(&v.to_le_bytes());
    }

    fn example_entry(frames: usize) -> GdEntry {
        GdEntry {
            name: "RS\\VE\\TEST".into(), left: 0, top: 0, right: 2, bottom: 1,
            width: 2, height: 1, frames, encoding: 4, data_offset: 0,
            data_length: 0,
        }
    }

    #[test]
    fn parses_index_and_cp1252_name() {
        let mut file = vec![0u8; 180];
        // header, string pool at 24, one name index at 32, record at 36
        put32(&mut file, 0, 24);
        put32(&mut file, 4, 24);
        put32(&mut file, 8, 32);
        put32(&mut file, 12, 36);
        put32(&mut file, 16, 100);
        put32(&mut file, 20, 1);
        file[24..30].copy_from_slice(&[b'T', b'E', b'S', b'T', 0x80, 0]);
        put32(&mut file, 32, 0);
        put32(&mut file, 36, 12);
        put32(&mut file, 40, 0);
        put32(&mut file, 44, 0);
        put32(&mut file, 48, 52);
        put32(&mut file, 52, 0);  // left
        put32(&mut file, 56, 0);  // top
        put32(&mut file, 60, 2);  // right
        put32(&mut file, 64, 1);  // bottom
        put32(&mut file, 84, 2);  // frame_count at descriptor +36
        put16(&mut file, 88, 4);  // encoding at descriptor +40
        put32(&mut file, 92, 12); // data offset +44
        assert_eq!(gd_index_size(&file[..20]).unwrap(), 100);
        let index = parse_gd_index(&file[..100], file.len()).unwrap();
        assert_eq!(index.entries.len(), 1);
        assert_eq!(index.index_size, 100);
        let entry = index.get("TEST€").unwrap();
        assert_eq!((entry.width, entry.height, entry.frames), (2, 1, 2));
        assert_eq!((entry.data_offset, entry.data_length), (112, 68));
    }

    #[test]
    fn decodes_delta_chain_and_straight_rgba() {
        let mut b = vec![0u8; 52];
        put32(&mut b, 12, 16);
        // frame 0: repeat two semitransparent pixels; then end frame
        put32(&mut b, 16, 0);
        put32(&mut b, 20, 18);
        put16(&mut b, 24, 1);
        put32(&mut b, 26, 0x80402010);
        put16(&mut b, 30, 4);
        // frame 1: skip first pixel, replace second
        put32(&mut b, 34, 1);
        put32(&mut b, 38, 0);
        put16(&mut b, 42, 3);
        put16(&mut b, 44, 0);
        put32(&mut b, 46, 0xff010203);
        put16(&mut b, 50, 4);

        let images = decode_gd_frames(&example_entry(2), &b, None, OpacityMode::Preserve).unwrap();
        assert_eq!(images[0], [128, 64, 32, 128, 128, 64, 32, 128]);
        assert_eq!(images[1], [128, 64, 32, 128, 1, 2, 3, 255]);
        assert_eq!(decode_gd_frames(&example_entry(2), &b, Some(0), OpacityMode::Preserve).unwrap().len(), 1);
    }

    #[test]
    fn auto_opaque_is_opt_in_for_ui_only() {
        let mut b = vec![0u8; 32];
        put32(&mut b, 12, 16);
        put32(&mut b, 16, 0);
        put32(&mut b, 20, 0);
        put16(&mut b, 24, 1);
        put32(&mut b, 26, 0x00010203);
        put16(&mut b, 30, 4);
        let e = example_entry(1);
        let preserved = decode_gd_frames(&e, &b, None, OpacityMode::Preserve).unwrap();
        assert_eq!(preserved[0], [0; 8]);
        let forced = decode_gd_frames(&e, &b, None, OpacityMode::AutoOpaque).unwrap();
        assert_eq!(forced[0], [1, 2, 3, 255, 1, 2, 3, 255]);
    }

    #[test]
    fn reports_truncated_and_invalid_data() {
        assert_eq!(gd_index_size(&[0; 8]), Err(GdError::Truncated));
        assert!(parse_gd_index(&[0; 24], 24).is_err());
        let mut e = example_entry(1);
        e.encoding = 2;
        assert_eq!(
            decode_gd_frames(&e, &[], None, OpacityMode::Preserve),
            Err(GdError::UnsupportedEncoding(2))
        );
        e.encoding = 4;
        assert_eq!(
            decode_gd_frames(&e, &[0; 16], None, OpacityMode::Preserve),
            Err(GdError::Truncated)
        );
    }
}
