//! Minimal, dependency-free PNG encoder for inspecting decoded sprites.
//!
//! Produces valid RGBA8 PNG files using uncompressed DEFLATE blocks.
//! This intentionally prioritizes portability and correctness over file size;
//! game rendering later can use a specialized texture/PNG library.

use std::fmt;

const SIGNATURE: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PngError {
    InvalidDimensions,
    InvalidRgbaLength,
    SizeOverflow,
}

impl fmt::Display for PngError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDimensions => write!(f, "PNG width and height must be positive and fit u32"),
            Self::InvalidRgbaLength => write!(f, "RGBA pixel data length does not match dimensions"),
            Self::SizeOverflow => write!(f, "PNG image is too large to encode"),
        }
    }
}

impl std::error::Error for PngError {}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

fn adler32(bytes: &[u8]) -> u32 {
    const MOD: u32 = 65521;
    let mut a = 1u32;
    let mut b = 0u32;
    for &byte in bytes {
        a += u32::from(byte);
        if a >= MOD {
            a -= MOD;
        }
        b += a;
        if b >= MOD {
            b -= MOD;
        }
    }
    (b << 16) | a
}

fn chunk(out: &mut Vec<u8>, name: &[u8; 4], data: &[u8]) -> Result<(), PngError> {
    let len = u32::try_from(data.len()).map_err(|_| PngError::SizeOverflow)?;
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(name);
    out.extend_from_slice(data);
    // CRC includes the chunk name and chunk data, but not its length.
    let crc = crc32(&out[out.len() - data.len() - 4..]);
    out.extend_from_slice(&crc.to_be_bytes());
    Ok(())
}

/// Encode row-major, straight-alpha RGBA8 pixels as a PNG file.
///
/// The output uses filter 0 on every row and store-only zlib DEFLATE.
/// The original alpha channel is retained, including semitransparent edges.
pub fn encode_rgba_png(width: usize, height: usize, rgba: &[u8]) -> Result<Vec<u8>, PngError> {
    let w = u32::try_from(width).map_err(|_| PngError::InvalidDimensions)?;
    let h = u32::try_from(height).map_err(|_| PngError::InvalidDimensions)?;
    if w == 0 || h == 0 {
        return Err(PngError::InvalidDimensions);
    }
    let row_bytes = width.checked_mul(4).ok_or(PngError::SizeOverflow)?;
    let image_bytes = row_bytes.checked_mul(height).ok_or(PngError::SizeOverflow)?;
    if rgba.len() != image_bytes {
        return Err(PngError::InvalidRgbaLength);
    }

    let row_with_filter = row_bytes.checked_add(1).ok_or(PngError::SizeOverflow)?;
    let raw_len = row_with_filter.checked_mul(height).ok_or(PngError::SizeOverflow)?;
    // Bound allocations for this debugging/export utility.
    if image_bytes > 64 * 1024 * 1024 {
        return Err(PngError::SizeOverflow);
    }
    let mut raw = Vec::with_capacity(raw_len);
    for scanline in rgba.chunks_exact(row_bytes) {
        raw.push(0); // PNG filter type None
        raw.extend_from_slice(scanline);
    }

    // zlib header: CMF 0x78 (32 KiB window), FLG 0x01 (no compression).
    // DEFLATE stored blocks are limited to 65,535 bytes.
    let mut zlib = Vec::with_capacity(
        raw_len
            .checked_add((raw_len / 65535 + 1).saturating_mul(5))
            .and_then(|n| n.checked_add(6))
            .ok_or(PngError::SizeOverflow)?,
    );
    zlib.extend_from_slice(&[0x78, 0x01]);
    let mut pos = 0usize;
    while pos < raw.len() {
        let end = (pos + 65535).min(raw.len());
        let len = (end - pos) as u16;
        zlib.push(if end == raw.len() { 1 } else { 0 }); // BFINAL, BTYPE 00
        zlib.extend_from_slice(&len.to_le_bytes());
        zlib.extend_from_slice(&(!len).to_le_bytes());
        zlib.extend_from_slice(&raw[pos..end]);
        pos = end;
    }
    zlib.extend_from_slice(&adler32(&raw).to_be_bytes());

    let mut png = Vec::new();
    png.extend_from_slice(&SIGNATURE);

    let mut ihdr = [0u8; 13];
    ihdr[0..4].copy_from_slice(&w.to_be_bytes());
    ihdr[4..8].copy_from_slice(&h.to_be_bytes());
    ihdr[8] = 8; // 8 bits per channel
    ihdr[9] = 6; // RGBA true colour with alpha
    // compression, filter and interlace are 0
    chunk(&mut png, b"IHDR", &ihdr)?;
    chunk(&mut png, b"IDAT", &zlib)?;
    chunk(&mut png, b"IEND", &[])?;

    Ok(png)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc_and_adler_known_vectors() {
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
        assert_eq!(adler32(b"Wikipedia"), 0x11e6_0398);
    }

    #[test]
    fn png_has_valid_structure_and_pixel_stream() {
        let rgba = [255u8, 0, 0, 255, 0, 0, 255, 127];
        let png = encode_rgba_png(2, 1, &rgba).unwrap();
        assert_eq!(&png[..8], &SIGNATURE);

        let mut off = 8usize;
        let mut seen = Vec::new();
        let mut zlib = Vec::new();
        while off < png.len() {
            let len = u32::from_be_bytes(png[off..off + 4].try_into().unwrap()) as usize;
            let name = &png[off + 4..off + 8];
            let data = &png[off + 8..off + 8 + len];
            let actual_crc = u32::from_be_bytes(
                png[off + 8 + len..off + 12 + len].try_into().unwrap(),
            );
            assert_eq!(crc32(&png[off + 4..off + 8 + len]), actual_crc);
            seen.push(name.to_vec());
            if name == b"IHDR" {
                assert_eq!(&data[..4], &2u32.to_be_bytes());
                assert_eq!(&data[4..8], &1u32.to_be_bytes());
                assert_eq!(&data[8..10], &[8, 6]);
            }
            if name == b"IDAT" {
                zlib.extend_from_slice(data);
            }
            off += len + 12;
        }
        assert_eq!(seen, [b"IHDR".to_vec(), b"IDAT".to_vec(), b"IEND".to_vec()]);
        assert_eq!(&zlib[..2], &[0x78, 0x01]);
        assert_eq!(zlib[2], 1); // final stored block
        let len = u16::from_le_bytes(zlib[3..5].try_into().unwrap()) as usize;
        assert_eq!(len, 9); // filter byte + 2 RGBA pixels
        assert_eq!(&zlib[7..7 + len], &[&[0][..], &rgba[..]].concat());
        assert_eq!(
            &zlib[7 + len..7 + len + 4],
            &adler32(&zlib[7..7 + len]).to_be_bytes(),
        );
    }

    #[test]
    fn multiple_stored_blocks_and_bad_sizes() {
        let rgba = vec![7u8; 200 * 100 * 4];
        let png = encode_rgba_png(200, 100, &rgba).unwrap();
        assert!(png.len() > rgba.len());
        assert_eq!(encode_rgba_png(0, 1, &[]), Err(PngError::InvalidDimensions));
        assert_eq!(encode_rgba_png(2, 2, &[0; 8]), Err(PngError::InvalidRgbaLength));
        assert_eq!(encode_rgba_png(usize::MAX, 2, &[]), Err(PngError::InvalidDimensions));
    }
}
