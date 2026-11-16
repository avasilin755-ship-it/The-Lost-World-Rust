//! Experimental isometric scene: procedurally shaded ground with actual GD world sprites.
//!
//! The map is intentionally an engineering testbed, NOT original terrain generation.
//! Original buildings can have extra (door/work/random) layers in the OD descriptors;
//! this scene draws an example house's named layers but does not yet simulate jobs.

use game_core::gd::{GdEntry, OpacityMode, decode_gd_frames, gd_index_size, parse_gd_index};
use game_core::od::{OdAnchor, OdIndex, parse_od};
use crate::scene_terrain::TerrainArt;
use macroquad::prelude::*;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;

const SIDE: i32 = 18;
// The original engine uses a 64x44 isometric cell at 1x zoom.
const TILE_W: f32 = 64.0;
const TILE_H: f32 = 44.0;
const TOP_BAR: f32 = 90.0;
const BOTTOM_BAR: f32 = 48.0;

struct Archive {
    path: PathBuf,
    entries: Vec<GdEntry>,
    od: OdIndex,
}

impl Archive {
    fn open(path: PathBuf) -> Result<Self, String> {
        let mut file = File::open(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let size = usize::try_from(file.metadata().map_err(|e| e.to_string())?.len())
            .map_err(|e| e.to_string())?;
        let mut head = [0u8; 24];
        file.read_exact(&mut head).map_err(|e| e.to_string())?;
        let index_size = gd_index_size(&head).map_err(|e| e.to_string())?;
        if !(24..=64 * 1024 * 1024).contains(&index_size) || index_size > size {
            return Err("Invalid GD index size".into());
        }
        let mut bytes = vec![0u8; index_size];
        file.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
        file.read_exact(&mut bytes).map_err(|e| e.to_string())?;
        let index = parse_gd_index(&bytes, size).map_err(|e| e.to_string())?;
        // Companion OD files preserve the original ground anchor and layer
        // descriptors. They sit next to dv.gd / ADDON.GD in the extracted data.
        let od_path = path.with_extension("OD");
        let od_bytes = fs::read(&od_path).map_err(|e| format!(
            "Missing original object descriptions {}: {e}", od_path.display()
        ))?;
        let od = parse_od(&od_bytes)
            .map_err(|e| format!("Cannot parse {}: {e}", od_path.display()))?;
        println!(
            "Scene archive: {} ({} GD sprites; {} OD objects; {} anchor records)",
            path.display(), index.entries.len(), od.objects.len(), od.anchors.len()
        );
        Ok(Self { path, entries: index.entries, od })
    }

    fn find(&self, name: &str) -> Option<&GdEntry> {
        self.entries.iter().find(|entry| entry.name.eq_ignore_ascii_case(name))
    }

    fn nature_name(&self) -> Option<String> {
        // Try known neutral/nature categories. Their exact names vary by release.
        // Prefer a tree standing action (CA), then an idle nature sprite.
        let neutral = self.entries.iter().filter(|entry| entry.name.starts_with("RS\\VE\\"));
        let keywords = ["BAUM", "TREE", "PALM", "TANNE", "BUSCH", "BUSH", "PALME"];
        for suffix in ["\\CA0", "\\MS0", "\\ID0"] {
            if let Some(entry) = neutral.clone().find(|e| {
                let n = e.name.to_ascii_uppercase();
                keywords.iter().any(|token| n.contains(token)) && n.ends_with(suffix)
                    && e.width >= 20 && e.height >= 20
            }) {
                return Some(entry.name.clone());
            }
        }
        // Fallback: a neutral world object; report its actual name in the UI.
        for suffix in ["\\CA0", "\\MS0"] {
            if let Some(entry) = neutral.clone().find(|e| {
                e.name.ends_with(suffix) && e.width >= 35 && e.height >= 45
                    && e.width <= 200 && e.height <= 240
            }) {
                return Some(entry.name.clone());
            }
        }
        None
    }

    fn sprite(&self, name: &str, max_frames: usize) -> Result<SpriteAsset, String> {
        let entry = self.find(name).ok_or_else(|| format!("Sprite not found: {name}"))?;
        if entry.data_length == 0 || entry.data_length > 64 * 1024 * 1024
            || entry.frames == 0 || entry.width > u16::MAX as usize
            || entry.height > u16::MAX as usize
        {
            return Err(format!("Invalid sprite dimensions/frame block: {}", entry.name));
        }
        let count = entry.frames.min(max_frames);
        let total = entry.width.checked_mul(entry.height)
            .and_then(|v| v.checked_mul(count))
            .and_then(|v| v.checked_mul(4))
            .ok_or("Sprite too large")?;
        if total > 96 * 1024 * 1024 {
            return Err(format!("Sprite exceeds preview budget: {}", entry.name));
        }
        let mut block = vec![0u8; entry.data_length];
        let mut file = File::open(&self.path).map_err(|e| e.to_string())?;
        file.seek(SeekFrom::Start(entry.data_offset as u64)).map_err(|e| e.to_string())?;
        file.read_exact(&mut block).map_err(|e| e.to_string())?;
        let decoded = decode_gd_frames(entry, &block, Some(count - 1), OpacityMode::Preserve)
            .map_err(|e| format!("{}: {e}", entry.name))?;
        let mut frames = Vec::new();
        for rgba in decoded {
            let texture = Texture2D::from_rgba8(entry.width as u16, entry.height as u16, &rgba);
            texture.set_filter(FilterMode::Nearest);
            frames.push(texture);
        }
        let anchor = self.od.anchor(&entry.name).ok_or_else(|| format!(
            "No original OD anchor for {} in {}", entry.name, self.path.display()
        ))?;
        Ok(SpriteAsset {
            name: entry.name.clone(),
            left: entry.left, top: entry.top, anchor,
            frames,
        })
    }
}

struct SpriteAsset {
    name: String,
    left: i32,
    top: i32,
    anchor: OdAnchor,
    frames: Vec<Texture2D>,
}

/// Source-of-truth coordinate formula from docs/formats/od-odb.md:
/// GD image rectangle and OD object anchor use the same sprite canvas.
fn sprite_top_left(cell: Vec2, left: i32, top: i32, anchor: OdAnchor, zoom: f32) -> Vec2 {
    cell + vec2((left - anchor.x) as f32, (top - anchor.y) as f32) * zoom
}

impl SpriteAsset {
    fn draw(&self, x: f32, y: f32, zoom: f32, time: f64, animate: bool) {
        let index = if animate && self.frames.len() > 1 {
            ((time * 7.0) as usize) % self.frames.len()
        } else {
            0
        };
        let texture = &self.frames[index];
        let top_left = sprite_top_left(vec2(x, y), self.left, self.top, self.anchor, zoom);
        draw_texture_ex(
            texture,
            top_left.x.floor(),
            top_left.y.floor(),
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(texture.width() * zoom, texture.height() * zoom)),
                ..Default::default()
            },
        );
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Tree,
    House,
    School,
    Person,
    Ship,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Self::Tree => "NATURE",
            Self::House => "HOUSE",
            Self::School => "SCHOOL",
            Self::Person => "PERSON",
            Self::Ship => "SHIP",
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Object {
    x: i32,
    y: i32,
    kind: Kind,
}

struct Assets {
    tree: Option<SpriteAsset>,
    house: Option<SpriteAsset>,
    house_door: Option<SpriteAsset>,
    house_random: Option<SpriteAsset>,
    school: Option<SpriteAsset>,
    person: Option<SpriteAsset>,
    ship: Option<SpriteAsset>,
    nature_name: String,
}

fn optional_sprite(archive: &Archive, name: &str, frames: usize) -> Option<SpriteAsset> {
    match archive.sprite(name, frames) {
        Ok(value) => Some(value),
        Err(error) => {
            eprintln!("Scene optional asset: {error}");
            None
        }
    }
}

impl Assets {
    fn load(base: &Archive, addon: &Archive) -> Self {
        let nature_name = base.nature_name().unwrap_or_default();
        let tree = if nature_name.is_empty() {
            None
        } else {
            optional_sprite(base, &nature_name, 1)
        };
        let house = optional_sprite(base, r"AZ\BU\WOHN\MS0", 1);
        let house_door = optional_sprite(base, r"AZ\BU\WOHN\DO", 1);
        let house_random = optional_sprite(base, r"AZ\BU\WOHN\RA0", 1);
        let school = optional_sprite(base, r"PI\BU\SCHULE\MS0", 1);
        let person = optional_sprite(base, r"SA\VE\WIRT\MG7", 12);
        let ship = optional_sprite(addon, r"SA\VE\TRANSPORTSCHIFF\MG7", 8);
        Self {
            tree, house, house_door, house_random,
            school, person, ship, nature_name,
        }
    }

    fn draw(&self, kind: Kind, sx: f32, sy: f32, zoom: f32, time: f64) {
        match kind {
            Kind::House => {
                for asset in [&self.house, &self.house_door, &self.house_random] {
                    if let Some(sprite) = asset {
                        sprite.draw(sx, sy, zoom, time, false);
                    }
                }
            }
            Kind::Tree => if let Some(sprite) = &self.tree {
                sprite.draw(sx, sy, zoom, time, false);
            },
            Kind::School => if let Some(sprite) = &self.school {
                sprite.draw(sx, sy, zoom, time, false);
            },
            Kind::Person => if let Some(sprite) = &self.person {
                sprite.draw(sx, sy, zoom, time, true);
            },
            Kind::Ship => if let Some(sprite) = &self.ship {
                sprite.draw(sx, sy, zoom, time, true);
            },
        }
    }
}

fn water(x: i32, y: i32) -> bool {
    (x >= 13 && y <= 10) || (x >= 15 && y <= 13)
}

fn project(x: f32, y: f32) -> Vec2 {
    vec2((x - y) * TILE_W * 0.5, (x + y) * TILE_H * 0.5)
}

fn unproject(v: Vec2) -> Vec2 {
    vec2(v.y / TILE_H + v.x / TILE_W, v.y / TILE_H - v.x / TILE_W)
}

fn diamond(cx: f32, cy: f32, zoom: f32, color: Color) {
    let hw = TILE_W * 0.5 * zoom;
    let hh = TILE_H * 0.5 * zoom;
    draw_triangle(vec2(cx, cy - hh), vec2(cx + hw, cy), vec2(cx, cy + hh), color);
    draw_triangle(vec2(cx, cy - hh), vec2(cx - hw, cy), vec2(cx, cy + hh), color);
}

fn draw_tile(x: i32, y: i32, screen: Vec2, zoom: f32, selected: bool) {
    let is_water = water(x, y);
    let coast = !is_water && [(1, 0), (-1, 0), (0, 1), (0, -1)]
        .iter().any(|(dx, dy)| water(x + dx, y + dy));
    let variant = ((x * 13 + y * 37 + x * y * 7).rem_euclid(7)) as u8;
    let mut color = if is_water {
        Color::from_rgba(36 + variant, 91 + variant, 109 + variant, 255)
    } else if coast {
        Color::from_rgba(145 + variant, 137 + variant, 92 + variant, 255)
    } else {
        Color::from_rgba(75 + variant, 111 + variant, 70 + variant / 2, 255)
    };
    if selected {
        color = Color::from_rgba(170, 179, 89, 255);
    }
    diamond(screen.x, screen.y, zoom, color);
    let hw = TILE_W * 0.5 * zoom;
    let hh = TILE_H * 0.5 * zoom;
    draw_line(screen.x - hw, screen.y, screen.x, screen.y + hh, 0.7,
        Color::from_rgba(13, 26, 22, 80));
    draw_line(screen.x, screen.y + hh, screen.x + hw, screen.y, 0.7,
        Color::from_rgba(13, 26, 22, 80));
}

/// Painter order for isometric world objects (furthest anchors first).
///
/// IMPORTANT: the entire opaque ground layer must be drawn BEFORE this list,
/// otherwise later ground diamonds overpaint tall trees, houses, units and ships.
fn object_painter_order(objects: &[Object]) -> Vec<usize> {
    let mut indices: Vec<usize> = (0..objects.len()).collect();
    // In the original, equal-depth anchors render left-to-right, increasing
    // screen X: y - x (docs/rendering/objects.md, section 12.2).
    indices.sort_by_key(|&i| (
        objects[i].x + objects[i].y,
        objects[i].y - objects[i].x,
    ));
    indices
}

fn initial_objects() -> Vec<Object> {
    let mut objects = vec![
        Object { x: 6, y: 7, kind: Kind::House },
        Object { x: 9, y: 9, kind: Kind::House },
        Object { x: 10, y: 6, kind: Kind::School },
        Object { x: 4, y: 9, kind: Kind::Person },
        Object { x: 7, y: 10, kind: Kind::Person },
        Object { x: 11, y: 6, kind: Kind::Person },
        Object { x: 15, y: 5, kind: Kind::Ship },
    ];
    for &(x, y) in &[
        (1, 7), (2, 11), (3, 4), (3, 13),
        (5, 2), (6, 13), (8, 3), (11, 12),
        (12, 2), (14, 11),
    ] {
        objects.push(Object { x, y, kind: Kind::Tree });
    }
    objects
}

fn map_cell_from_mouse(mx: f32, my: f32, camera: Vec2, zoom: f32) -> Option<(i32, i32)> {
    let local = (vec2(mx, my) - camera) / zoom;
    let p = unproject(local);
    // Tiles are anchored at cell centres and have diamond half-extents W/2, H/2.
    // Selecting the closest centre in transformed coordinates matches the grid.
    let x = p.x.round() as i32;
    let y = p.y.round() as i32;
    if x < 0 || x >= SIDE || y < 0 || y >= SIDE {
        return None;
    }
    let centre = project(x as f32, y as f32);
    let dx = (local.x - centre.x).abs() / (TILE_W * 0.5);
    let dy = (local.y - centre.y).abs() / (TILE_H * 0.5);
    (dx + dy <= 1.0).then_some((x, y))
}

fn draw_overlay(
    selected: Option<(i32, i32)>, objects: &[Object], assets: &Assets, zoom: f32,
) {
    draw_rectangle(0.0, 0.0, screen_width(), TOP_BAR, Color::from_rgba(18, 27, 24, 255));
    draw_text("THE LOST WORLD  |  ISOMETRIC SCENE TEST", 18.0, 34.0, 27.0, GOLD);
    draw_text(
        &format!(
            "Rust + Macroquad  |  Original GD/OD objects  |  {} placed  |  zoom {:.1}x  |  64x44 cells",
            objects.len(), zoom
        ),
        18.0, 68.0, 18.0, LIGHTGRAY
    );
    draw_rectangle(
        0.0, screen_height() - BOTTOM_BAR,
        screen_width(), BOTTOM_BAR, Color::from_rgba(18, 27, 24, 255)
    );
    draw_text(
        "WASD Pan | Wheel Zoom | Left Click Select | 1 Tree  2 House  3 Person  4 Ship  5 School | Del Remove",
        12.0, screen_height() - 27.0, 17.0, LIGHTGRAY
    );
    draw_text("ESC Exit | Original animation and textures; map/placement are experimental",
        12.0, screen_height() - 9.0, 14.0, GRAY);
    let selection = match selected {
        Some((x, y)) => {
            let existing = objects.iter().rev().find(|o| o.x == x && o.y == y)
                .map(|o| o.kind.name()).unwrap_or("EMPTY");
            format!("TILE [{x},{y}]   {existing}   {}", if water(x, y) { "WATER" } else { "LAND" })
        }
        None => "CLICK A TILE TO SELECT IT".to_owned(),
    };
    let label_width = measure_text(&selection, None, 18, 1.0).width + 26.0;
    let x = (screen_width() - label_width - 14.0).max(0.0);
    draw_rectangle(x, TOP_BAR + 12.0, label_width, 36.0, Color::from_rgba(19, 28, 24, 242));
    draw_text(&selection, x + 12.0, TOP_BAR + 36.0, 18.0, WHITE);

    if assets.tree.is_none() {
        draw_text("Neutral nature texture missing: using terrain only", 18.0,
            TOP_BAR + 42.0, 18.0, ORANGE);
    } else {
        draw_text(&format!("Nature asset: {}", assets.nature_name),
            18.0, TOP_BAR + 43.0, 15.0, LIGHTGRAY);
    }
}

pub async fn run(base_path: PathBuf, addon_path: PathBuf) -> Result<(), String> {
    let base = Archive::open(base_path)?;
    let addon = Archive::open(addon_path)?;
    let assets = Assets::load(&base, &addon);
    let terrain = base.path.parent()
        .ok_or("The original dv.gd path has no parent directory")?
        .to_path_buf();
    let ground_art = match TerrainArt::load(&terrain) {
        Ok(g) => {
            println!("Loaded original .os ground textures; using animated water and original coastline.");
            Some(g)
        }
        Err(e) => {
            eprintln!("Original .os textures unavailable ({e}); falling back to placeholder terrain.");
            None
        }
    };
    let mut objects = initial_objects();
    let mut camera = vec2(screen_width() * 0.5, 142.0);
    let mut zoom = 1.0f32;
    let mut selected: Option<(i32, i32)> = None;
    let mut drag_from: Option<Vec2> = None;
    let start_time = get_time();

    loop {
        let dt = get_frame_time().min(0.05);
        if is_key_pressed(KeyCode::Escape) { break; }
        let pan = 440.0 * dt;
        if is_key_down(KeyCode::A) || is_key_down(KeyCode::Left) { camera.x += pan; }
        if is_key_down(KeyCode::D) || is_key_down(KeyCode::Right) { camera.x -= pan; }
        if is_key_down(KeyCode::W) || is_key_down(KeyCode::Up) { camera.y += pan; }
        if is_key_down(KeyCode::S) || is_key_down(KeyCode::Down) { camera.y -= pan; }

        let (mx, my) = mouse_position();
        let mouse = vec2(mx, my);
        let (_wx, wy) = mouse_wheel();
        if wy != 0.0 {
            // Zoom around cursor rather than snapping to the origin.
            let before = (mouse - camera) / zoom;
            zoom = (zoom + wy.signum() * 0.1).clamp(0.5, 2.5);
            camera = mouse - before * zoom;
        }
        if is_mouse_button_pressed(MouseButton::Right) {
            drag_from = Some(mouse);
        }
        if is_mouse_button_down(MouseButton::Right) {
            if let Some(old) = drag_from {
                camera += mouse - old;
                drag_from = Some(mouse);
            }
        }
        if is_mouse_button_released(MouseButton::Right) {
            drag_from = None;
        }
        if is_mouse_button_pressed(MouseButton::Left)
            && my >= TOP_BAR && my < screen_height() - BOTTOM_BAR
        {
            selected = map_cell_from_mouse(mx, my, camera, zoom);
        }

        if let Some((x, y)) = selected {
            let pressed = if is_key_pressed(KeyCode::Key1) { Some(Kind::Tree) }
                else if is_key_pressed(KeyCode::Key2) { Some(Kind::House) }
                else if is_key_pressed(KeyCode::Key3) { Some(Kind::Person) }
                else if is_key_pressed(KeyCode::Key4) { Some(Kind::Ship) }
                else if is_key_pressed(KeyCode::Key5) { Some(Kind::School) }
                else { None };
            if let Some(kind) = pressed {
                if (kind == Kind::Ship) == water(x, y) {
                    objects.retain(|o| o.x != x || o.y != y);
                    objects.push(Object { x, y, kind });
                }
            }
            if is_key_pressed(KeyCode::Delete) {
                objects.retain(|o| o.x != x || o.y != y);
            }
        }

        clear_background(Color::from_rgba(28, 45, 42, 255));
        let time = get_time() - start_time;
        // Pass 1: draw ALL opaque ground tiles first. Terrain may never be
        // drawn on top of a unit or tall building in this flat-world prototype.
        for depth in 0..SIDE * 2 - 1 {
            for x in 0..SIDE {
                let y = depth - x;
                if y < 0 || y >= SIDE { continue; }
                let p = project(x as f32, y as f32) * zoom + camera;
                if p.x < -250.0 || p.x > screen_width() + 250.0
                    || p.y < -270.0 || p.y > screen_height() + 140.0 { continue; }
                if let Some(art) = &ground_art {
                    art.draw_cell(
                        p.x, p.y, zoom, x, y, water, time,
                        selected == Some((x, y)),
                    );
                } else {
                    draw_tile(x, y, p, zoom, selected == Some((x, y)));
                }
            }
        }

        // Pass 2: draw sprites above the entire terrain surface. Objects still
        // overlap each other in original isometric painter order by anchor.
        // Do not cull only by the anchor: tall sprites can be visible even when
        // their anchor point is just outside the window.
        for index in object_painter_order(&objects) {
            let obj = &objects[index];
            let p = project(obj.x as f32, obj.y as f32) * zoom + camera;
            assets.draw(obj.kind, p.x, p.y, zoom, time);
        }
        draw_overlay(selected, &objects, &assets, zoom);
        next_frame().await;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn original_od_anchors_align_sprites_to_ground_points() {
        let gd_left = 30;
        let gd_top = 18;
        let anchor = OdAnchor { x: 112, y: 142 };
        assert_eq!(
            sprite_top_left(vec2(350.0, 200.0), gd_left, gd_top, anchor, 1.0),
            vec2(268.0, 76.0),
        );
        assert_eq!(
            sprite_top_left(vec2(350.0, 200.0), gd_left, gd_top, anchor, 2.0),
            vec2(186.0, -48.0),
        );
    }

    #[test]
    fn projection_round_trip_is_stable() {
        for x in 0..SIDE {
            for y in 0..SIDE {
                let p = project(x as f32, y as f32);
                let recovered = unproject(p);
                assert!((recovered.x - x as f32).abs() < 0.0001);
                assert!((recovered.y - y as f32).abs() < 0.0001);
                assert_eq!(
                    map_cell_from_mouse(p.x + 650.0, p.y + 140.0, vec2(650.0, 140.0), 1.0),
                    Some((x, y))
                );
            }
        }
    }

    #[test]
    fn objects_are_depth_sorted_independently_of_ground() {
        let objects = vec![
            Object { x: 6, y: 9, kind: Kind::House },
            Object { x: 2, y: 1, kind: Kind::Tree },
            Object { x: 7, y: 8, kind: Kind::Person },
            Object { x: 3, y: 4, kind: Kind::School },
        ];
        let ordered = object_painter_order(&objects);
        assert_eq!(ordered, vec![1, 3, 2, 0]);
        assert_eq!(
            ordered.iter().map(|&i| objects[i].x + objects[i].y).collect::<Vec<_>>(),
            vec![3, 7, 15, 15],
        );
    }

    #[test]
    fn initial_map_uses_valid_placement() {
        for object in initial_objects() {
            assert!((0..SIDE).contains(&object.x));
            assert!((0..SIDE).contains(&object.y));
            assert_eq!(object.kind == Kind::Ship, water(object.x, object.y));
        }
    }
}
