# The Lost World — sprite viewer (direct GD support)

The viewer now supports loading original .gd archives directly into GPU textures.
It reads only the sprite index and selected image block, not all artwork.
PNG-folder preview mode is still supported.

## Open Mission Pack from original archive (PowerShell)

    $root = "$HOME\Games\DieVolker_Original"
    cargo run --manifest-path rust/sprite-viewer/Cargo.toml -- "$root\GameAssets\MissionPack\ADDON.GD" 'SA\VE\TRANSPORTSCHIFF\MG7'

## Open base game directly (PowerShell)

    $baseGD = Get-ChildItem "$root\GameAssets\Base" -Recurse -File -Filter "dv.gd" | Select-Object -First 1 -ExpandProperty FullName
    cargo run --manifest-path rust/sprite-viewer/Cargo.toml -- "$baseGD" 'SA\VE\WIRT\MG7'

Local original assets are never committed or uploaded by this viewer.

## Legacy PNG-folder viewer and controls


A temporary standalone desktop prototype using Rust and Macroquad 0.4.16.
It can also preview PNG animations previously exported by `gd_export_png`.

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
