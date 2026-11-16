//! Decoder for original Alien Nations .os terrain tile sets.
//!
//! See docs/formats/os-dvf-tdb.md and src/formats/os.ts.
//! Every tile stores a 64x64 B,G,R,x image; alpha is always opaque.
//! Parses local game resources only; no assets are included in the source.

use std::fmt;

#[derive(Debug, PartialEq, Eq)]
pub enum OsError {
    Truncated,
    Invalid(&'static str),
}
impl fmt::Display for OsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for OsError {}

#[derive(Debug, Clone)]
pub struct TerrainTile {
    pub width: usize,
    pub height: usize,
    pub weight: u32,
    pub rgba: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct TerrainSet {
    pub tiles: Vec<TerrainTile>,
    /// Weighted lookup for selecting an original 64x64 variant using an 8-bit sample.
    pub pick: [u8; 256],
}

fn u32_at(buf: &[u8], offset: usize) -> Result<u32, OsError> {
    let data = buf.get(offset..offset.checked_add(4).ok_or(OsError::Invalid("offset overflow"))?)
        .ok_or(OsError::Truncated)?;
    Ok(u32::from_le_bytes(data.try_into().unwrap()))
}

/// Parses a complete original .os tile set; rejects truncated or oversized data.
pub fn parse_os(buf: &[u8]) -> Result<TerrainSet, OsError> {
    if u32_at(buf, 0)? != 0 {
        return Err(OsError::Invalid("unexpected OS file header"));
    }
    let count = usize::try_from(u32_at(buf, 4)?).map_err(|_| OsError::Invalid("tile count"))?;
    if count == 0 || count > 256 {
        return Err(OsError::Invalid("tile count must be between 1 and 256"));
    }
    let mut tiles = Vec::with_capacity(count);
    let mut off = 8usize;
    for _ in 0..count {
        let header_end = off.checked_add(36).ok_or(OsError::Invalid("tile offset overflow"))?;
        if buf.get(off..header_end).is_none() {
            return Err(OsError::Truncated);
        }
        let width = usize::try_from(u32_at(buf, off + 16)?).map_err(|_| OsError::Invalid("width"))?;
        let height = usize::try_from(u32_at(buf, off + 20)?).map_err(|_| OsError::Invalid("height"))?;
        let weight = u32_at(buf, off + 32)?;
        if width != 64 || height != 64 {
            return Err(OsError::Invalid("unsupported tile dimensions (expected 64x64)"));
        }
        let bytes = width.checked_mul(height).and_then(|n| n.checked_mul(4))
            .ok_or(OsError::Invalid("tile buffer overflow"))?;
        let data_end = header_end.checked_add(bytes)
            .ok_or(OsError::Invalid("tile offset overflow"))?;
        let pixels = buf.get(header_end..data_end).ok_or(OsError::Truncated)?;
        let mut rgba = vec![0u8; bytes];
        for (pixel, bgra) in rgba.chunks_exact_mut(4).zip(pixels.chunks_exact(4)) {
            pixel.copy_from_slice(&[bgra[2], bgra[1], bgra[0], 255]);
        }
        tiles.push(TerrainTile { width, height, weight, rgba });
        off = data_end;
    }
    let pick = build_pick_table(&tiles)?;
    Ok(TerrainSet { tiles, pick })
}

fn build_pick_table(tiles: &[TerrainTile]) -> Result<[u8; 256], OsError> {
    if tiles.is_empty() || tiles.len() > 256 {
        return Err(OsError::Invalid("empty/oversized terrain set"));
    }
    let total: u64 = tiles.iter().map(|tile| u64::from(tile.weight)).sum();
    if total == 0 {
        return Err(OsError::Invalid("all terrain tile weights are zero"));
    }
    let mut table = [0u8; 256];
    let mut start = 0usize;
    let mut cumulative = 0u64;
    for (idx, tile) in tiles.iter().enumerate() {
        cumulative += u64::from(tile.weight);
        let end = ((cumulative * 256) / total) as usize;
        table[start..end].fill(idx as u8);
        start = end;
    }
    Ok(table)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_file(weights: &[u32]) -> Vec<u8> {
        let mut b = vec![0u8; 8];
        b[4..8].copy_from_slice(&(weights.len() as u32).to_le_bytes());
        for (i, &weight) in weights.iter().enumerate() {
            let mut h = [0u8; 36];
            h[16..20].copy_from_slice(&64u32.to_le_bytes());
            h[20..24].copy_from_slice(&64u32.to_le_bytes());
            h[32..36].copy_from_slice(&weight.to_le_bytes());
            b.extend_from_slice(&h);
            let mut p = vec![0u8; 64 * 64 * 4];
            p[0..4].copy_from_slice(&[(i + 3) as u8, 2, 1, 0]);
            b.extend_from_slice(&p);
        }
        b
    }

    #[test]
    fn tiles_are_decoded_as_opaque_rgba() {
        let set = parse_os(&test_file(&[255, 128])).unwrap();
        assert_eq!(set.tiles.len(), 2);
        assert_eq!(set.tiles[0].weight, 255);
        assert_eq!(set.tiles[0].rgba.len(), 64 * 64 * 4);
        assert_eq!(&set.tiles[0].rgba[..4], &[1, 2, 3, 255]);
        assert!(set.pick[..170].iter().all(|&v| v == 0));
        assert!(set.pick[171..].iter().all(|&v| v == 1));
    }

    #[test]
    fn truncated_and_invalid_headers_are_rejected() {
        assert_eq!(parse_os(&[0u8; 7]).unwrap_err(), OsError::Truncated);
        let mut bytes = test_file(&[255]);
        bytes.truncate(40);
        assert_eq!(parse_os(&bytes).unwrap_err(), OsError::Truncated);
        let mut bytes = test_file(&[255]);
        bytes[8 + 16..8 + 20].copy_from_slice(&32u32.to_le_bytes());
        assert_eq!(parse_os(&bytes).unwrap_err(), OsError::Invalid("unsupported tile dimensions (expected 64x64)"));
    }

    #[test]
    fn zero_tile_weights_are_rejected() {
        assert_eq!(parse_os(&test_file(&[0])).unwrap_err(),
            OsError::Invalid("all terrain tile weights are zero"));
    }
}
