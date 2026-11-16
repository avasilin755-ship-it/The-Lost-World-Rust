# The Lost World — local sprite viewer

A temporary standalone desktop prototype using Rust and Macroquad 0.4.16.
It previews original `.gd` animation frames **after** exporting them to PNG using
`cargo run -p game-core --example gd_export_png`.

The game assets are read locally only and are **not** part of this repository.

## Windows 11 usage

From the repository root:

```powershell
cargo run --manifest-path rust/sprite-viewer/Cargo.toml -- "$HOME\Games\DieVolker_Original\SpritePreview"
```

For the first run, Cargo downloads Macroquad and compiles the graphical application.

Controls:

- Space — play / pause
- Left / Right — previous / next animation frame (pauses)
- Mouse wheel, Up / Down — zoom from 1x to 12x with nearest filtering
- Comma / Period — animation speed from 1 to 24 frames/second
- B — checkerboard / dark background
- Esc — close

This is a **sprite animation viewer**, not a complete game. It is a separate Cargo
workspace so that the portable `game-core` remains dependency-free for now.
