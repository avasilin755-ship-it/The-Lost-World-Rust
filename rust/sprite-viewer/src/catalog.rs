//! In-game catalog of original Alien Nations GD sprites.
//!
//! Reads only archive indices at startup. A sprite's pixel block is fetched and
//! decoded when selected; the original game data never leaves this computer.

use game_core::gd::{GdEntry, OpacityMode, decode_gd_frames, gd_index_size, parse_gd_index};
use macroquad::prelude::*;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;

const LIST_TOP: f32 = 148.0;
const ROW_HEIGHT: f32 = 27.0;
const FOOTER_HEIGHT: f32 = 48.0;
const MAX_INDEX_BYTES: usize = 64 * 1024 * 1024;
const MAX_BLOCK_BYTES: usize = 64 * 1024 * 1024;
const MAX_TOTAL_FRAME_BYTES: usize = 128 * 1024 * 1024;
const MAX_FRAMES: usize = 128;

const PANEL: Color = Color::new(0.085, 0.12, 0.11, 1.0);
const PANEL_ALT: Color = Color::new(0.13, 0.18, 0.16, 1.0);
const TEXT_FAINT: Color = Color::new(0.65, 0.71, 0.68, 1.0);

struct Archive {
    path: PathBuf,
    entries: Vec<GdEntry>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Item {
    archive: usize,
    entry: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SourceFilter {
    All,
    Base,
    Addon,
}

impl SourceFilter {
    fn cycle(self) -> Self {
        match self {
            Self::All => Self::Base,
            Self::Base => Self::Addon,
            Self::Addon => Self::All,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::All => "ALL",
            Self::Base => "BASE",
            Self::Addon => "ADDON",
        }
    }

    fn allows(self, item: Item) -> bool {
        match self {
            Self::All => true,
            Self::Base => item.archive == 0,
            Self::Addon => item.archive == 1,
        }
    }
}

fn read_archive(path: PathBuf) -> Result<Archive, String> {
    let mut file = File::open(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let size = usize::try_from(file.metadata().map_err(|e| e.to_string())?.len())
        .map_err(|e| e.to_string())?;
    let mut header = [0u8; 24];
    file.read_exact(&mut header).map_err(|e| format!("{}: {e}", path.display()))?;
    let index_size = gd_index_size(&header).map_err(|e| e.to_string())?;
    if !(24..=MAX_INDEX_BYTES).contains(&index_size) || index_size > size {
        return Err(format!("Invalid GD index size in {}", path.display()));
    }
    let mut prefix = vec![0u8; index_size];
    file.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
    file.read_exact(&mut prefix).map_err(|e| e.to_string())?;
    let index = parse_gd_index(&prefix, size).map_err(|e| e.to_string())?;
    println!(
        "Catalog loaded: {} ({} sprites, {} index bytes)",
        path.display(), index.entries.len(), index.index_size,
    );
    Ok(Archive { path, entries: index.entries })
}

fn search_items(items: &[Item], archives: &[Archive], query: &str, filter: SourceFilter) -> Vec<usize> {
    let upper = query.to_ascii_uppercase();
    items.iter().enumerate()
        .filter(|(_, item)| filter.allows(**item))
        .filter(|(_, item)| {
            archives[item.archive].entries[item.entry]
                .name.to_ascii_uppercase().contains(&upper)
        })
        .map(|(i, _)| i)
        .collect()
}

fn load_item(archive: &Archive, entry_index: usize) -> Result<Vec<Texture2D>, String> {
    let entry = &archive.entries[entry_index];
    if entry.data_length == 0 || entry.data_length > MAX_BLOCK_BYTES {
        return Err(format!("Invalid block size for {}", entry.name));
    }
    if entry.frames == 0 || entry.frames > MAX_FRAMES {
        return Err(format!("Sprite has {} frames; supported range is 1..={MAX_FRAMES}", entry.frames));
    }
    let total = entry.width.checked_mul(entry.height)
        .and_then(|n| n.checked_mul(entry.frames))
        .and_then(|n| n.checked_mul(4))
        .ok_or("Sprite frame memory calculation overflow")?;
    if total > MAX_TOTAL_FRAME_BYTES || entry.width > u16::MAX as usize
        || entry.height > u16::MAX as usize
    {
        return Err("Sprite is too large for catalog preview".into());
    }
    let mut file = File::open(&archive.path).map_err(|e| e.to_string())?;
    file.seek(SeekFrom::Start(entry.data_offset as u64))
        .map_err(|e| e.to_string())?;
    let mut block = vec![0u8; entry.data_length];
    file.read_exact(&mut block).map_err(|e| e.to_string())?;
    let images = decode_gd_frames(entry, &block, None, OpacityMode::Preserve)
        .map_err(|e| format!("Decode error for {}: {e}", entry.name))?;
    let mut textures = Vec::with_capacity(images.len());
    for rgba in images {
        let texture = Texture2D::from_rgba8(entry.width as u16, entry.height as u16, &rgba);
        texture.set_filter(FilterMode::Nearest);
        textures.push(texture);
    }
    Ok(textures)
}

fn shorten(value: &str, max_len: usize) -> String {
    if value.chars().count() <= max_len {
        return value.to_owned();
    }
    value.chars().take(max_len.saturating_sub(3)).collect::<String>() + "..."
}

fn draw_checker(x: f32, y: f32, width: f32, height: f32) {
    let cell = 28.0;
    let cols = (width / cell).ceil() as usize;
    let rows = (height / cell).ceil() as usize;
    for row in 0..rows {
        for col in 0..cols {
            let x0 = x + col as f32 * cell;
            let y0 = y + row as f32 * cell;
            let w = (width - col as f32 * cell).min(cell).max(0.0);
            let h = (height - row as f32 * cell).min(cell).max(0.0);
            let color = if (row + col) % 2 == 0 {
                Color::from_rgba(68, 73, 70, 255)
            } else {
                Color::from_rgba(87, 93, 89, 255)
            };
            draw_rectangle(x0, y0, w, h, color);
        }
    }
}

/// Browse both original resource archives with one searchable animated preview.
/// File I/O occurs only when a user selects an entry; source archives are read-only.
pub async fn run(base_path: PathBuf, addon_path: PathBuf) -> Result<(), String> {
    let archives = vec![read_archive(base_path)?, read_archive(addon_path)?];
    let items: Vec<Item> = archives.iter().enumerate()
        .flat_map(|(archive, src)| (0..src.entries.len())
            .map(move |entry| Item { archive, entry }))
        .collect();
    let mut filter = SourceFilter::All;
    let mut query = String::new();
    let mut matches = search_items(&items, &archives, &query, filter);
    let mut selected = 0usize;
    let mut scroll = 0usize;
    let mut active: Option<usize> = None;
    let mut textures: Vec<Texture2D> = Vec::new();
    let mut load_error: Option<String> = None;
    let mut frame = 0usize;
    let mut zoom = 4.0f32;
    let mut fps = 8.0f32;
    let mut elapsed = 0.0f32;
    let mut playing = true;
    let mut checkerboard = true;

    println!("Catalog: {} entries (BASE {}, ADDON {})", items.len(),
        archives[0].entries.len(), archives[1].entries.len());

    loop {
        let sw = screen_width();
        let sh = screen_height();
        let sidebar_width = (sw * 0.42).clamp(310.0, 480.0);
        let visible_count = ((sh - LIST_TOP - FOOTER_HEIGHT) / ROW_HEIGHT)
            .floor().max(1.0) as usize;

        let mut refresh = false;
        if is_key_pressed(KeyCode::Tab) {
            filter = filter.cycle();
            refresh = true;
        }
        if is_key_pressed(KeyCode::Backspace) && !query.is_empty() {
            query.pop();
            refresh = true;
        }
        if is_key_pressed(KeyCode::Escape) {
            if query.is_empty() {
                break;
            }
            query.clear();
            refresh = true;
        }
        while let Some(c) = get_char_pressed() {
            if (c.is_ascii_alphanumeric() || matches!(c, '\\' | '/' | '_' | '-' | '.'))
                && query.len() < 90
            {
                query.push(c);
                refresh = true;
            }
        }

        if refresh {
            matches = search_items(&items, &archives, &query, filter);
            selected = 0;
            scroll = 0;
            active = None;
            textures.clear();
            load_error = None;
            frame = 0;
        }

        let (mx, my) = mouse_position();
        let (wheel_x, wheel_y) = mouse_wheel();
        let _ = wheel_x;
        if mx < sidebar_width && my >= LIST_TOP && my < sh - FOOTER_HEIGHT {
            if wheel_y > 0.0 {
                scroll = scroll.saturating_sub((wheel_y.abs().ceil() as usize) * 3);
            } else if wheel_y < 0.0 {
                scroll = scroll.saturating_add((wheel_y.abs().ceil() as usize) * 3);
            }
            scroll = scroll.min(matches.len().saturating_sub(visible_count));
            if is_mouse_button_pressed(MouseButton::Left) {
                let row = ((my - LIST_TOP) / ROW_HEIGHT) as usize;
                let clicked = scroll + row;
                if clicked < matches.len() {
                    selected = clicked;
                }
            }
        } else if wheel_y != 0.0 {
            zoom = (zoom + wheel_y.signum()).clamp(1.0, 12.0);
        }

        if !matches.is_empty() {
            if is_key_pressed(KeyCode::Down) {
                selected = (selected + 1).min(matches.len() - 1);
            }
            if is_key_pressed(KeyCode::Up) {
                selected = selected.saturating_sub(1);
            }
            if is_key_pressed(KeyCode::PageDown) {
                selected = (selected + visible_count).min(matches.len() - 1);
            }
            if is_key_pressed(KeyCode::PageUp) {
                selected = selected.saturating_sub(visible_count);
            }
            if is_key_pressed(KeyCode::Home) {
                selected = 0;
            }
            if is_key_pressed(KeyCode::End) {
                selected = matches.len() - 1;
            }
            if selected < scroll {
                scroll = selected;
            }
            if selected >= scroll + visible_count {
                scroll = selected - visible_count + 1;
            }
        }

        let new_active = matches.get(selected).copied();
        if active != new_active {
            active = new_active;
            textures.clear();
            load_error = None;
            frame = 0;
            elapsed = 0.0;
            if let Some(id) = active {
                let item = items[id];
                match load_item(&archives[item.archive], item.entry) {
                    Ok(images) => textures = images,
                    Err(err) => {
                        eprintln!("{err}");
                        load_error = Some(err);
                    }
                }
            }
        }

        if is_key_pressed(KeyCode::Space) {
            playing = !playing;
            elapsed = 0.0;
        }
        if !textures.is_empty() {
            if is_key_pressed(KeyCode::Right) {
                frame = (frame + 1) % textures.len();
                playing = false;
            }
            if is_key_pressed(KeyCode::Left) {
                frame = (frame + textures.len() - 1) % textures.len();
                playing = false;
            }
        }
        if is_key_pressed(KeyCode::Equal) {
            zoom = (zoom + 1.0).min(12.0);
        }
        if is_key_pressed(KeyCode::Minus) {
            zoom = (zoom - 1.0).max(1.0);
        }
        if is_key_pressed(KeyCode::Period) {
            fps = (fps + 1.0).min(24.0);
        }
        if is_key_pressed(KeyCode::Comma) {
            fps = (fps - 1.0).max(1.0);
        }
        if is_key_pressed(KeyCode::F2) {
            checkerboard = !checkerboard;
        }
        if playing && textures.len() > 1 {
            elapsed += get_frame_time().min(0.1);
            while elapsed >= 1.0 / fps {
                elapsed -= 1.0 / fps;
                frame = (frame + 1) % textures.len();
            }
        }

        // World-space background is rendered first; list and header overlay it.
        clear_background(Color::from_rgba(30, 39, 36, 255));
        let right_x = sidebar_width;
        let view_top = 90.0;
        let view_height = (sh - view_top - FOOTER_HEIGHT).max(0.0);
        let view_width = (sw - right_x).max(0.0);
        if checkerboard {
            draw_checker(right_x, view_top, view_width, view_height);
        } else {
            draw_rectangle(right_x, view_top, view_width, view_height, PANEL_ALT);
        }

        if let Some(texture) = textures.get(frame) {
            let fit = ((view_width - 48.0) / texture.width())
                .min((view_height - 48.0) / texture.height())
                .floor().max(1.0);
            let scale = zoom.min(fit);
            let size = texture.size() * scale;
            let x = right_x + (view_width - size.x) / 2.0;
            let y = view_top + (view_height - size.y) / 2.0;
            draw_texture_ex(
                texture, x.floor(), y.floor(), WHITE,
                DrawTextureParams { dest_size: Some(size), ..Default::default() }
            );
        }

        draw_rectangle(0.0, 0.0, sw, 90.0, PANEL);
        draw_text("THE LOST WORLD  |  RESOURCE CATALOG", 18.0, 34.0, 28.0, GOLD);
        draw_text(
            &format!("Original GD sprites: {}   |   BASE {}   |   ADDON {}",
                items.len(), archives[0].entries.len(), archives[1].entries.len()),
            18.0, 68.0, 19.0, LIGHTGRAY,
        );
        draw_rectangle(0.0, 90.0, sidebar_width, sh - 90.0, PANEL);
        draw_rectangle(8.0, 96.0, sidebar_width - 16.0, 42.0, PANEL_ALT);
        draw_text(
            &format!("Search: {}_", shorten(&query, 35)),
            16.0, 122.0, 21.0, WHITE,
        );
        draw_text(
            &format!("TAB: {}   |   {} result(s)", filter.name(), matches.len()),
            16.0, LIST_TOP - 6.0, 16.0, GOLD,
        );

        for line in 0..visible_count {
            let position = scroll + line;
            if position >= matches.len() {
                break;
            }
            let item = items[matches[position]];
            let entry = &archives[item.archive].entries[item.entry];
            let y = LIST_TOP + line as f32 * ROW_HEIGHT;
            if position == selected {
                draw_rectangle(7.0, y, sidebar_width - 14.0, ROW_HEIGHT - 1.0,
                    Color::from_rgba(82, 91, 62, 255));
            } else if position % 2 == 0 {
                draw_rectangle(7.0, y, sidebar_width - 14.0, ROW_HEIGHT - 1.0, PANEL_ALT);
            }
            let source = if item.archive == 0 { "B" } else { "A" };
            draw_text(
                &format!("{source}  {}", shorten(&entry.name, 43)),
                14.0, y + 19.0, 17.0,
                if position == selected { WHITE } else { LIGHTGRAY },
            );
        }

        draw_rectangle(0.0, sh - FOOTER_HEIGHT, sw, FOOTER_HEIGHT, PANEL);
        draw_text(
            "TYPE Search | TAB Source | UP/DOWN Select | Mouse Wheel List/Zoom | LEFT/RIGHT Frames",
            10.0, sh - 26.0, 16.0, LIGHTGRAY,
        );
        draw_text(
            "SPACE Play | +/- Zoom | ,/. FPS | F2 Background | ESC Clear / Exit",
            10.0, sh - 8.0, 14.0, TEXT_FAINT,
        );

        // Sprite metadata in the preview panel.
        if let Some(id) = active {
            let item = items[id];
            let entry = &archives[item.archive].entries[item.entry];
            draw_rectangle(right_x, 90.0, view_width, 59.0, PANEL_ALT);
            let label = if item.archive == 0 { "BASE" } else { "ADDON" };
            draw_text(
                &format!("{label} / {}", shorten(&entry.name, 54)),
                right_x + 16.0, 114.0, 19.0, GOLD,
            );
            draw_text(
                &format!("{}x{}  |  Frames {}/{}  |  Zoom {}x  |  {} FPS  |  {}",
                    entry.width, entry.height, frame + 1, entry.frames, zoom as u32,
                    fps as u32, if playing { "PLAY" } else { "PAUSE" }),
                right_x + 16.0, 137.0, 16.0, WHITE,
            );
        } else {
            draw_text("No matching sprites", right_x + 24.0, 135.0, 25.0, WHITE);
        }
        if let Some(error) = &load_error {
            draw_rectangle(right_x + 10.0, view_top + 85.0,
                (view_width - 20.0).max(0.0), 62.0, PANEL);
            draw_text(&shorten(error, 68), right_x + 20.0, view_top + 120.0, 17.0, ORANGE);
        }
        next_frame().await;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archive_filter_cycles_and_matches() {
        assert!(SourceFilter::All.allows(Item { archive: 0, entry: 1 }));
        assert!(!SourceFilter::Addon.allows(Item { archive: 0, entry: 1 }));
        assert_eq!(SourceFilter::All.cycle(), SourceFilter::Base);
        assert_eq!(SourceFilter::Base.cycle(), SourceFilter::Addon);
        assert_eq!(SourceFilter::Addon.cycle(), SourceFilter::All);
    }

    #[test]
    fn long_names_are_truncated_safely() {
        assert_eq!(shorten("Hello", 8), "Hello");
        assert_eq!(shorten("ABCDEFGHIJKL", 8), "ABCDE...");
    }
}
