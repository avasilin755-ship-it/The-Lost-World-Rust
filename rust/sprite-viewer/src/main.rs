//! The Lost World: original GD sprite viewer, powered by Macroquad.
//!
//! Read a local original GD archive directly, decode animations in Rust,
//! and upload RGBA frames as GPU textures without any intermediate PNG files.
//! Legacy PNG-folder preview is also supported.
//! Original game assets stay on the user's computer, outside the repository.

use game_core::gd::{OpacityMode, decode_gd_frames, gd_index_size, parse_gd_index};
mod catalog;
mod scene;
mod scene_terrain;

use macroquad::prelude::*;
use std::{
    env, fs,
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

fn window_conf() -> Conf {
    Conf {
        window_title: "The Lost World — Sprite Viewer".to_owned(),
        window_width: 1000,
        window_height: 720,
        window_resizable: true,
        ..Default::default()
    }
}

fn png_files(folder: &PathBuf) -> Result<Vec<PathBuf>, String> {
    let entries = fs::read_dir(folder)
        .map_err(|e| format!("Cannot open {}: {e}", folder.display()))?;
    let mut files = Vec::new();
    for item in entries {
        let path = item.map_err(|e| e.to_string())?.path();
        if path.is_file()
            && path.extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("png"))
        {
            files.push(path);
        }
    }
    files.sort();
    if files.is_empty() {
        return Err(format!("No PNG files in {}", folder.display()));
    }
    Ok(files)
}

fn load_gd_sprite(path: &Path, sprite_name: &str) -> Result<(Vec<Texture2D>, Vec<String>), String> {
    let mut file = File::open(path).map_err(|e| format!("Cannot open {}: {e}", path.display()))?;
    let length = usize::try_from(file.metadata().map_err(|e| e.to_string())?.len())
        .map_err(|e| e.to_string())?;
    let mut header = [0u8; 24];
    file.read_exact(&mut header).map_err(|e| e.to_string())?;
    let index_size = gd_index_size(&header).map_err(|e| e.to_string())?;
    if !(24..=64 * 1024 * 1024).contains(&index_size) || index_size > length {
        return Err("Invalid .gd index size".into());
    }
    let mut index_bytes = vec![0u8; index_size];
    file.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
    file.read_exact(&mut index_bytes).map_err(|e| e.to_string())?;
    let index = parse_gd_index(&index_bytes, length).map_err(|e| e.to_string())?;
    let entry = index.get(sprite_name).ok_or_else(|| {
        format!("Sprite '{sprite_name}' not found. Archive contains {} sprites; use gd_inspect to list names.", index.entries.len())
    })?;
    if entry.data_length == 0 || entry.data_length > 64 * 1024 * 1024 {
        return Err("Invalid or unusually large GD block".into());
    }
    if entry.frames == 0 || entry.frames > 256 {
        return Err("Animation must contain between 1 and 256 frames".into());
    }
    if entry.width > u16::MAX as usize || entry.height > u16::MAX as usize {
        return Err("Sprite dimensions exceed texture limits".into());
    }

    // Load only the selected sprite's pixel block (not the entire large GD archive).
    let mut block = vec![0u8; entry.data_length];
    file.seek(SeekFrom::Start(entry.data_offset as u64)).map_err(|e| e.to_string())?;
    file.read_exact(&mut block).map_err(|e| e.to_string())?;
    let frames = decode_gd_frames(entry, &block, None, OpacityMode::Preserve)
        .map_err(|e| format!("Cannot decode {sprite_name}: {e}"))?;
    let mut textures = Vec::with_capacity(frames.len());
    let mut names = Vec::with_capacity(frames.len());
    for (i, rgba) in frames.iter().enumerate() {
        let texture = Texture2D::from_rgba8(entry.width as u16, entry.height as u16, rgba);
        texture.set_filter(FilterMode::Nearest);
        textures.push(texture);
        names.push(format!("{sprite_name} [{i}]"));
    }
    println!(
        "Loaded '{}' directly from {} ({} x {}, {} frames)",
        sprite_name, path.display(), entry.width, entry.height, frames.len()
    );
    Ok((textures, names))
}

async fn load_png_folder(folder: &PathBuf) -> Result<(Vec<Texture2D>, Vec<String>), String> {
    let paths = png_files(folder)?;
    let mut textures = Vec::new();
    let mut names = Vec::new();
    for file in paths {
        let file_str = file.to_string_lossy();
        match load_texture(&file_str).await {
            Ok(texture) => {
                texture.set_filter(FilterMode::Nearest);
                textures.push(texture);
                names.push(file.file_name().unwrap_or_default().to_string_lossy().into_owned());
            }
            Err(error) => eprintln!("Cannot load {}: {error}", file.display()),
        }
    }
    if textures.is_empty() {
        return Err("None of the PNGs could be loaded".into());
    }
    Ok((textures, names))
}

fn checkerboard(width: f32, height: f32, cell: f32) {
    let dark = Color::from_rgba(58, 63, 63, 255);
    let light = Color::from_rgba(79, 85, 84, 255);
    let cols = (width / cell).ceil() as i32;
    let rows = (height / cell).ceil() as i32;
    for y in 0..rows {
        for x in 0..cols {
            draw_rectangle(
                x as f32 * cell,
                y as f32 * cell,
                cell,
                cell,
                if (x + y) % 2 == 0 { dark } else { light },
            );
        }
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    if env::args_os().nth(1).is_some_and(|v| v.to_string_lossy() == "--scene") {
        let args: Vec<_> = env::args_os().skip(2).collect();
        if args.len() != 2 {
            eprintln!("Usage: cargo run --manifest-path rust/sprite-viewer/Cargo.toml -- --scene <base-dv.gd> <addon.gd>");
            return;
        }
        if let Err(err) = scene::run(PathBuf::from(&args[0]), PathBuf::from(&args[1])).await {
            eprintln!("Scene error: {err}");
        }
        return;
    }
    // Catalog mode preserves the two legacy entry points: direct single GD
    // animation and exported PNG folder.
    if env::args_os().nth(1).is_some_and(|v| v.to_string_lossy() == "--catalog") {
        let args: Vec<_> = env::args_os().skip(2).collect();
        if args.len() != 2 {
            eprintln!("Usage: cargo run --manifest-path rust/sprite-viewer/Cargo.toml -- --catalog <base-dv.gd> <addon.gd>");
            return;
        }
        if let Err(err) = catalog::run(PathBuf::from(&args[0]), PathBuf::from(&args[1])).await {
            eprintln!("Catalog error: {err}");
        }
        return;
    }
    let mut args = env::args_os().skip(1);
    let Some(source_path) = args.next() else {
        eprintln!("Usage:");
        eprintln!("  cargo run --manifest-path rust/sprite-viewer/Cargo.toml -- <archive.gd> <exact-sprite-name>");
        eprintln!("  cargo run --manifest-path rust/sprite-viewer/Cargo.toml -- <PNG-folder>");
        return;
    };
    let source = PathBuf::from(source_path);
    let is_gd = source.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("gd"));
    let result = if is_gd {
        match args.next() {
            Some(name) => match name.into_string() {
                Ok(name) => load_gd_sprite(&source, &name),
                Err(_) => Err("Sprite name must be valid Unicode".into()),
            },
            None => Err("Missing sprite name after GD archive path. Use gd_inspect to list exact names.".into()),
        }
    } else if source.is_dir() {
        load_png_folder(&source).await
    } else {
        Err(format!("Expected a .gd archive or PNG folder, got: {}", source.display()))
    };
    if args.next().is_some() {
        eprintln!("Unexpected additional command line arguments");
        return;
    }
    let (textures, names) = match result {
        Ok(data) => data,
        Err(err) => {
            eprintln!("{err}");
            return;
        }
    };

    let mut frame_index = 0usize;
    let mut playing = true;
    let mut frame_rate = 8.0f32;
    let mut zoom = 5.0f32;
    let mut timer = 0.0f32;
    let mut checker = true;

    loop {
        if is_key_pressed(KeyCode::Escape) {
            break;
        }
        if is_key_pressed(KeyCode::Space) {
            playing = !playing;
            timer = 0.0;
        }
        if is_key_pressed(KeyCode::Right) {
            frame_index = (frame_index + 1) % textures.len();
            playing = false;
        }
        if is_key_pressed(KeyCode::Left) {
            frame_index = (frame_index + textures.len() - 1) % textures.len();
            playing = false;
        }
        if is_key_pressed(KeyCode::B) {
            checker = !checker;
        }
        if is_key_pressed(KeyCode::Up) || is_key_pressed(KeyCode::Equal) {
            zoom = (zoom + 1.0).min(12.0);
        }
        if is_key_pressed(KeyCode::Down) || is_key_pressed(KeyCode::Minus) {
            zoom = (zoom - 1.0).max(1.0);
        }
        if is_key_pressed(KeyCode::Period) {
            frame_rate = (frame_rate + 1.0).min(24.0);
        }
        if is_key_pressed(KeyCode::Comma) {
            frame_rate = (frame_rate - 1.0).max(1.0);
        }
        let (_wheel_x, wheel_y) = mouse_wheel();
        if wheel_y.abs() > 0.01 {
            zoom = (zoom + wheel_y.signum()).clamp(1.0, 12.0);
        }

        if playing && textures.len() > 1 {
            timer += get_frame_time().min(0.1);
            while timer >= 1.0 / frame_rate {
                timer -= 1.0 / frame_rate;
                frame_index = (frame_index + 1) % textures.len();
            }
        }

        if checker {
            checkerboard(screen_width(), screen_height(), 32.0);
        } else {
            clear_background(Color::from_rgba(28, 38, 37, 255));
        }
        let texture = &textures[frame_index];
        // Keep scaling at integer multiples for pixel-art accuracy.
        let size = texture.size() * zoom;
        let x = ((screen_width() - size.x) / 2.0).floor();
        let y = ((screen_height() - size.y) / 2.0).floor() + 18.0;
        draw_texture_ex(
            texture, x, y, WHITE,
            DrawTextureParams {
                dest_size: Some(size),
                ..Default::default()
            },
        );

        draw_rectangle(
            0.0, 0.0, screen_width(), 83.0,
            Color::from_rgba(19, 27, 25, 255),
        );
        draw_text("THE LOST WORLD  |  RUST SPRITE VIEWER", 18.0, 30.0, 27.0, GOLD);
        draw_text(
            &format!(
                "{}   |   Frame {}/{}   |   {}x{}   |   Zoom {}x   |   {} FPS   |   {}",
                names[frame_index], frame_index + 1, textures.len(),
                texture.width() as u32, texture.height() as u32,
                zoom as u32, frame_rate as u32,
                if playing { "PLAY" } else { "PAUSE" }
            ),
            18.0, 64.0, 18.0, WHITE,
        );
        draw_rectangle(
            0.0, screen_height() - 42.0, screen_width(), 42.0,
            Color::from_rgba(19, 27, 25, 255),
        );
        draw_text(
            "SPACE Play/Pause  |  LEFT/RIGHT Frame  |  Wheel/UP/DOWN Zoom  |  ,/. Speed  |  B Background  |  ESC Exit",
            12.0, screen_height() - 15.0, 17.0, LIGHTGRAY,
        );

        next_frame().await;
    }
}
