//! Validate an original .gd sprite archive without modifying or uploading its contents.
//!
//! Usage:
//! cargo run -p game-core --example gd_inspect -- "C:\path\to\Data\dv.gd"
//! cargo run -p game-core --example gd_inspect -- "C:\path\to\Data\dv.gd" "RS\VE\..."
//!
//! With a second argument, also decode the first animation frame of the named sprite.

use game_core::gd::{OpacityMode, decode_gd_frames, gd_index_size, parse_gd_index};
use std::env;
use std::error::Error;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args_os().skip(1);
    let Some(path) = args.next() else {
        eprintln!("Usage: cargo run -p game-core --example gd_inspect -- <path-to-dv.gd> [sprite-name]");
        return Err("missing .gd file path".into());
    };
    let selected = args.next().map(|v| v.to_string_lossy().into_owned());

    let mut file = File::open(&path)?;
    let file_size = usize::try_from(file.metadata()?.len())?;
    let mut header = [0u8; 24];
    file.read_exact(&mut header)?;
    let index_size = gd_index_size(&header)?;
    if index_size < 24 || index_size > file_size || index_size > 64 * 1024 * 1024 {
        return Err("invalid or unusually large .gd index".into());
    }

    let mut index_bytes = vec![0u8; index_size];
    file.seek(SeekFrom::Start(0))?;
    file.read_exact(&mut index_bytes)?;
    let index = parse_gd_index(&index_bytes, file_size)?;

    println!("Archive: {}", std::path::Path::new(&path).display());
    println!("File size: {file_size} bytes");
    println!("Index: {} bytes", index.index_size);
    println!("Sprites: {}", index.entries.len());
    for (i, entry) in index.entries.iter().take(15).enumerate() {
        println!(
            "{:>3}. {}   {}x{}   {} frame(s), encoding {}",
            i + 1, entry.name, entry.width, entry.height, entry.frames, entry.encoding
        );
    }

    if let Some(name) = selected {
        let entry = index.get(&name).ok_or("sprite name not found in archive")?;
        if entry.data_length == 0 || entry.data_length > 64 * 1024 * 1024 {
            return Err("invalid or unusually large sprite data block".into());
        }
        let mut block = vec![0u8; entry.data_length];
        file.seek(SeekFrom::Start(u64::try_from(entry.data_offset)?))?;
        file.read_exact(&mut block)?;

        // World sprites must preserve their alpha. Do not auto-opaque their door frames.
        let first_frame = decode_gd_frames(entry, &block, Some(0), OpacityMode::Preserve)?;
        let rgba = &first_frame[0];
        let pixels = rgba.chunks_exact(4).count();
        let transparent = rgba.chunks_exact(4).filter(|p| p[3] == 0).count();
        let translucent = rgba.chunks_exact(4)
            .filter(|p| p[3] > 0 && p[3] < 255).count();
        println!("Decoded '{}' frame 0: {pixels} pixels, {transparent} transparent, {translucent} translucent", entry.name);
    } else {
        println!("Index OK. Pass an exact sprite name as the second argument to test frame decoding.");
    }
    Ok(())
}
