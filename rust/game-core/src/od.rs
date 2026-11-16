//! Alien Nations OD object descriptions: original sprite anchors and layers.
//!
//! Reader for docs/formats/od-odb.md and src/formats/od.ts.
//! Sprite GD rectangles are canvas-relative; placement is:
//! screen position = cell_screen + (gd.left - od.anchor_x, gd.top - od.anchor_y).
//! No original game data is bundled with the source.

use std::collections::HashMap;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OdError {
    Truncated,
    Invalid(&'static str),
    UnknownKind(u32),
}
impl fmt::Display for OdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for OdError {}
pub type OdResult<T> = Result<T, OdError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OdAnchor {
    pub x: i32,
    pub y: i32,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OdLayer {
    pub name: String,
    pub speed: i32,
    pub flags: u32,
    pub frames: u32,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OdObject {
    pub id: u32,
    pub kind: u32,
    pub anchor: OdAnchor,
    pub slots: Vec<Vec<OdLayer>>,
}
#[derive(Debug)]
pub struct OdIndex {
    pub objects: Vec<OdObject>,
    pub anchors: HashMap<String, OdAnchor>,
}
impl OdIndex {
    pub fn anchor(&self, name: &str) -> Option<OdAnchor> {
        self.anchors.get(name).copied()
    }
    pub fn object_for_sprite(&self, name: &str) -> Option<&OdObject> {
        self.objects.iter().find(|o| o.slots.iter().flatten().any(|l| l.name == name))
    }
}
fn get(bytes: &[u8], offset: usize, len: usize) -> OdResult<&[u8]> {
    let end = offset.checked_add(len).ok_or(OdError::Invalid("offset overflow"))?;
    bytes.get(offset..end).ok_or(OdError::Truncated)
}
fn u32_at(b: &[u8], o: usize) -> OdResult<u32> {
    Ok(u32::from_le_bytes(get(b, o, 4)?.try_into().unwrap()))
}
fn i32_at(b: &[u8], o: usize) -> OdResult<i32> {
    Ok(i32::from_le_bytes(get(b, o, 4)?.try_into().unwrap()))
}
const CP1252_HIGH: [u32; 32] = [
    0x20ac, 0x81, 0x201a, 0x192, 0x201e, 0x2026, 0x2020, 0x2021,
    0x2c6, 0x2030, 0x160, 0x2039, 0x152, 0x8d, 0x17d, 0x8f,
    0x90, 0x2018, 0x2019, 0x201c, 0x201d, 0x2022, 0x2013, 0x2014,
    0x2dc, 0x2122, 0x161, 0x203a, 0x153, 0x9d, 0x17e, 0x178,
];
fn cp1252(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| {
        let cp = if (0x80..=0x9f).contains(&b) {
            CP1252_HIGH[(b - 0x80) as usize]
        } else {
            u32::from(b)
        };
        char::from_u32(cp).unwrap_or('\u{fffd}')
    }).collect()
}

/// Parse an original dv.OD or ADDON.OD file. Invalid offsets and 
/// unusually large descriptor counts are rejected.
pub fn parse_od(bytes: &[u8]) -> OdResult<OdIndex> {
    if u32_at(bytes, 0)? != 28 {
        return Err(OdError::Invalid("unexpected OD header size"));
    }
    let names_off = u32_at(bytes, 4)? as usize;
    let names_index_off = u32_at(bytes, 8)? as usize;
    let records_off = u32_at(bytes, 12)? as usize;
    let anim_base = u32_at(bytes, 16)? as usize;
    let name_count = u32_at(bytes, 20)? as usize;
    let obj_count = u32_at(bytes, 24)? as usize;
    if name_count > 100_000 || obj_count > 100_000 {
        return Err(OdError::Invalid("unreasonable OD object/name count"));
    }

    let read_name = |number: usize| -> OdResult<String> {
        if number >= name_count {
            return Err(OdError::Invalid("invalid OD sprite name number"));
        }
        let index = names_index_off
            .checked_add(number.checked_mul(4)
                .ok_or(OdError::Invalid("name index overflow"))?)
            .ok_or(OdError::Invalid("name index overflow"))?;
        let offset = u32_at(bytes, index)? as usize;
        let pos = names_off.checked_add(offset)
            .ok_or(OdError::Invalid("name offset overflow"))?;
        let tail = bytes.get(pos..).ok_or(OdError::Truncated)?;
        let end = tail.iter().take(4096).position(|&c| c == 0)
            .ok_or(OdError::Invalid("unterminated OD sprite name"))?;
        Ok(cp1252(&tail[..end]))
    };

    let mut objects = Vec::with_capacity(obj_count);
    let mut anchors = HashMap::new();
    let mut pos = records_off;
    for _ in 0..obj_count {
        let kind = u32_at(bytes, pos + 4)?;
        let record_size = match kind {
            0 => 0xc4usize,
            1 => 0x2cusize,
            2 => 0x28usize,
            unknown => return Err(OdError::UnknownKind(unknown)),
        };
        get(bytes, pos, record_size)?;
        let id = u32_at(bytes, pos + 8)?;
        let anchor = OdAnchor { x: i32_at(bytes, pos + 12)?, y: i32_at(bytes, pos + 16)? };
        let nslots = (record_size - 0x24) / 4;
        let mut slots = Vec::with_capacity(nslots);
        for s in 0..nslots {
            let reference = i32_at(bytes, pos + 0x24 + s * 4)?;
            if reference == -1 {
                slots.push(Vec::new());
                continue;
            }
            if reference < 0 {
                return Err(OdError::Invalid("negative OD descriptor reference"));
            }
            let descriptor = anim_base.checked_add(reference as usize)
                .ok_or(OdError::Invalid("descriptor offset overflow"))?;
            let layers_count = u32_at(bytes, descriptor + 0x10)? as usize;
            if layers_count == 0 || layers_count > 32 {
                return Err(OdError::Invalid("OD descriptor layer count outside 1..32"));
            }
            let layers_start = descriptor.checked_add(0x20)
                .ok_or(OdError::Invalid("descriptor layer offset overflow"))?;
            get(bytes, layers_start, layers_count * 32)?;
            let mut layers = Vec::with_capacity(layers_count);
            for k in 0..layers_count {
                let layer = layers_start + k * 32;
                let name = read_name(u32_at(bytes, layer + 0x1c)? as usize)?;
                let od_layer = OdLayer {
                    name: name.clone(),
                    speed: i32_at(bytes, layer + 0x10)?,
                    flags: u32_at(bytes, layer + 0x14)?,
                    frames: u32_at(bytes, layer + 0x18)?,
                };
                anchors.entry(name).or_insert(anchor);
                layers.push(od_layer);
            }
            slots.push(layers);
        }
        objects.push(OdObject { id, kind, anchor, slots });
        pos += record_size;
    }
    Ok(OdIndex { objects, anchors })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u32_write(b: &mut [u8], offset: usize, value: u32) {
        b[offset..offset+4].copy_from_slice(&value.to_le_bytes());
    }
    fn fixture() -> Vec<u8> {
        let mut buf = vec![0u8; 512];
        u32_write(&mut buf, 0, 28);
        u32_write(&mut buf, 4, 28);
        u32_write(&mut buf, 8, 56);
        u32_write(&mut buf, 12, 64);
        u32_write(&mut buf, 16, 128);
        u32_write(&mut buf, 20, 2);
        u32_write(&mut buf, 24, 1);
        let names = b"AZ\\BU\\WOHN\\MS0\0AZ\\BU\\WOHN\\DO\0";
        buf[28..28+names.len()].copy_from_slice(names);
        u32_write(&mut buf, 56, 0);
        u32_write(&mut buf, 60, 15);
        u32_write(&mut buf, 64+4, 1);
        u32_write(&mut buf, 64+8, 37);
        u32_write(&mut buf, 64+12, 112);
        u32_write(&mut buf, 64+16, 142);
        u32_write(&mut buf, 64+0x24, 0xffff_ffff);
        u32_write(&mut buf, 64+0x28, 0);
        u32_write(&mut buf, 128+0x10, 2);
        u32_write(&mut buf, 128+0x20+0x1c, 0);
        u32_write(&mut buf, 128+0x40+0x1c, 1);
        u32_write(&mut buf, 128+0x40+0x14, 0x10);
        buf
    }
    #[test]
    fn anchors_and_layers_are_decoded_from_od() {
        let od = parse_od(&fixture()).unwrap();
        assert_eq!(od.objects.len(), 1);
        assert_eq!(od.anchors.len(), 2);
        assert_eq!(od.anchor("AZ\\BU\\WOHN\\MS0"), Some(OdAnchor { x: 112, y: 142 }));
        assert_eq!(od.anchor("AZ\\BU\\WOHN\\DO"), Some(OdAnchor { x: 112, y: 142 }));
        let desc = od.object_for_sprite("AZ\\BU\\WOHN\\MS0").unwrap();
        assert_eq!(desc.kind, 1);
        assert_eq!(desc.slots[1].len(), 2);
        assert_eq!(desc.slots[1][1].flags, 0x10);
    }
    #[test]
    fn rejects_truncated_or_invalid_od() {
        assert_eq!(parse_od(&[0u8; 12]).unwrap_err(), OdError::Invalid("unexpected OD header size"));
        let mut f = fixture();
        u32_write(&mut f, 64+0x28, 0xffff_fffe);
        assert_eq!(parse_od(&f).unwrap_err(), OdError::Invalid("negative OD descriptor reference"));
    }
    #[test]
    fn original_anchor_formula() {
        let ground = (350, 200);
        let gd = (30, 18);
        let anchor = OdAnchor { x: 112, y: 142 };
        assert_eq!((ground.0 + gd.0 - anchor.x, ground.1 + gd.1 - anchor.y), (268, 76));
    }
}
