//! The Lost World: inspect locally exported original sprite animations.
//!
//! No original assets are committed to or loaded from this repository.
//! Run from the repository root:
//! cargo run --manifest-path rust/sprite-viewer/Cargo.toml -- "C:\Users\Admin\Games\DieVolker_Original\SpritePreview"

use macroquad::prelude::*;
use std::{env, fs, path::PathBuf};

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
    let Some(path) = env::args_os().nth(1) else {
        eprintln!("Usage: cargo run --manifest-path rust/sprite-viewer/Cargo.toml -- <PNG-folder>");
        return;
    };
    let folder = PathBuf::from(path);
    let paths = match png_files(&folder) {
        Ok(paths) => paths,
        Err(err) => {
            eprintln!("{err}");
            return;
        }
    };

    let mut textures = Vec::new();
    let mut names = Vec::new();
    for file in paths {
        let file_str = file.to_string_lossy();
        match load_texture(&file_str).await {
            Ok(texture) => {
                // Keep original pixel edges crisp when magnified.
                texture.set_filter(FilterMode::Nearest);
                textures.push(texture);
                names.push(file.file_name()
                    .unwrap_or_default().to_string_lossy().into_owned());
            }
            Err(error) => eprintln!("Cannot load {}: {error}", file.display()),
        }
    }
    if textures.is_empty() {
        eprintln!("None of the PNGs could be loaded.");
        return;
    }

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
