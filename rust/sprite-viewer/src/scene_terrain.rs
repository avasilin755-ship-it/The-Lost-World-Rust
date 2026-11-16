//! Textured isometric ground using actual .os images extracted from Die Völker.
//!
//! Source: docs/formats/os-dvf-tdb.md + docs/rendering/terrain-drawing.md.
//! This experiment uses original pixels and transition artwork; the test map's
//! land/water classification is synthetic, not a port of the original generator.

use game_core::os::{TerrainSet, parse_os};
use macroquad::prelude::*;
use std::fs;
use std::path::Path;

struct GpuSet {
    tiles: Vec<Texture2D>,
    pick: [u8; 256],
}
impl GpuSet {
    fn load(dir: &Path, basename: &str) -> Result<Self, String> {
        let path = dir.join(format!("{basename}.os"));
        let data = fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let TerrainSet { tiles, pick } = parse_os(&data)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        let mut textures = Vec::with_capacity(tiles.len());
        for tile in &tiles {
            let tex = Texture2D::from_rgba8(tile.width as u16, tile.height as u16, &tile.rgba);
            tex.set_filter(FilterMode::Nearest);
            textures.push(tex);
        }
        println!("Original terrain set: {} ({} textures)", path.display(), textures.len());
        Ok(Self { tiles: textures, pick })
    }

    fn choose(&self, x: i32, y: i32) -> &Texture2D {
        let byte = location_hash(x, y);
        &self.tiles[self.pick[byte as usize] as usize]
    }

    fn index(&self, idx: usize) -> &Texture2D {
        &self.tiles[idx % self.tiles.len()]
    }
}

pub struct TerrainArt {
    meadow: GpuSet,
    path: GpuSet,
    water: Vec<GpuSet>,
    shoreline: Vec<GpuSet>,
}

fn location_hash(x: i32, y: i32) -> u8 {
    let mut v = (x as u32).wrapping_mul(0x9e37_79b9)
        ^ (y as u32).wrapping_mul(0x85eb_ca6b)
        ^ 0x1eaf_b04d;
    v ^= v >> 16;
    v = v.wrapping_mul(0x7feb_352d);
    v ^= v >> 15;
    (v >> 8) as u8
}

/// Four corners are encoded like the original engine (terrain-drawing.md):
/// bit 0 = c0 bottom, bit 1 = c1 right,
/// bit 2 = c2 left, bit 3 = c3 top.
const SHORE_MASKS: [u8; 14] = [
    0b1100, 0b1010, 0b0011, 0b0101,
    0b1000, 0b0010, 0b0001, 0b0100,
    0b1110, 0b1011, 0b0111, 0b1101,
    0b0110, 0b1001,
];

fn transition_index(water_mask: u8) -> Option<usize> {
    SHORE_MASKS.iter().position(|&p| p == water_mask)
}

impl TerrainArt {
    pub fn load(dir: &Path) -> Result<Self, String> {
        let meadow = GpuSet::load(dir, "wiese")?;
        let path = GpuSet::load(dir, "weg")?;
        let mut water = Vec::new();
        let mut shoreline = Vec::new();
        for frame in 1..=4 {
            water.push(GpuSet::load(dir, &format!("wasser{frame}"))?);
            shoreline.push(GpuSet::load(dir, &format!("transition{frame}"))?);
        }
        Ok(Self { meadow, path, water, shoreline })
    }

    /// Choose real game pixels for our temporary land/water geometry.
    /// Every original shoreline set contains 14 direction-specific textures.
    pub fn draw_cell(
        &self, cx: f32, cy: f32, zoom: f32, x: i32, y: i32,
        water_at: impl Fn(i32, i32) -> bool,
        seconds: f64,
        selected: bool,
    ) {
        // The test map treats each (x,y) as a vertex/class sample.
        // This is enough to exercise original transitions until the original
        // DVM terrain map or procedural generator is ported.
        let mask = (water_at(x + 1, y + 1) as u8)
            | ((water_at(x, y + 1) as u8) << 1)
            | ((water_at(x + 1, y) as u8) << 2)
            | ((water_at(x, y) as u8) << 3);
        let frame = (seconds.floor() as usize) % 4;
        let texture: &Texture2D = if mask == 0b1111 {
            self.water[frame].choose(x, y)
        } else if mask == 0 {
            // The original blends meadow into path; this prototype keeps a
            // one-cell path border until per-corner multi-texture blending.
            let near_water = (-1..=1).any(|dx| (-1..=1).any(|dy| water_at(x + dx, y + dy)));
            if near_water { self.path.choose(x, y) } else { self.meadow.choose(x, y) }
        } else {
            let idx = transition_index(mask).unwrap_or(0);
            self.shoreline[frame].index(idx)
        };
        draw_textured_diamond(texture, cx, cy, zoom, selected);
    }
}

/// Full custom UV mapping; a rotated square cannot reproduce this 64x44
/// rhombus without stretching/mirroring original shoreline texture detail.
/// Texture top-left UV (0,0) is the *bottom* diamond vertex, matching source.
fn draw_textured_diamond(texture: &Texture2D, cx: f32, cy: f32, zoom: f32, selected: bool) {
    let half_width = 32.0 * zoom;
    let half_height = 22.0 * zoom;
    let shade = Color::from_rgba(162, 162, 162, 255);
    let mesh = Mesh {
        vertices: vec![
            Vertex::new(cx, cy - half_height, 0.0, 1.0, 1.0, shade), // top
            Vertex::new(cx - half_width, cy, 0.0, 0.0, 1.0, shade), // left
            Vertex::new(cx + half_width, cy, 0.0, 1.0, 0.0, shade), // right
            Vertex::new(cx, cy + half_height, 0.0, 0.0, 0.0, shade), // bottom
        ],
        indices: vec![0, 1, 2, 1, 2, 3],
        texture: Some(texture.clone()),
    };
    draw_mesh(&mesh);

    if selected {
        let glow = Color::from_rgba(238, 227, 104, 105);
        draw_triangle(
            vec2(cx, cy - half_height),
            vec2(cx - half_width, cy),
            vec2(cx + half_width, cy),
            glow,
        );
        draw_triangle(
            vec2(cx, cy + half_height),
            vec2(cx - half_width, cy),
            vec2(cx + half_width, cy),
            glow,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_fourteen_shoreline_masks_are_mapped() {
        for mask in 1..15 {
            assert!(transition_index(mask).is_some(), "missing shoreline {mask:04b}");
        }
        assert_eq!(transition_index(0), None);
        assert_eq!(transition_index(15), None);
        assert_eq!(transition_index(0b1100), Some(0));
        assert_eq!(transition_index(0b1001), Some(13));
    }

    #[test]
    fn location_hash_is_repeatable() {
        assert_eq!(location_hash(7, 8), location_hash(7, 8));
        assert_ne!(location_hash(7, 8), location_hash(8, 7));
    }
}
