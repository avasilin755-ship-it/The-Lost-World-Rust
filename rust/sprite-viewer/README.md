# The Lost World — Rust Sprite Viewer and GD Catalog

A local development tool, built with **Rust + Macroquad 0.4.16**.
Original game files are never uploaded or committed.

## Windows 11 — browse all original sprites

Open PowerShell in the repository root and run:

```powershell
$root = "$HOME\Games\DieVolker_Original"
$baseGD = Get-ChildItem "$root\GameAssets\Base" -Recurse -File -Filter "dv.gd" | Select-Object -First 1 -ExpandProperty FullName
$addonGD = "$root\GameAssets\MissionPack\ADDON.GD"

cargo run --manifest-path rust/sprite-viewer/Cargo.toml -- --catalog "$baseGD" "$addonGD"
```

The browser reads both sprite **indexes** at startup (1940 + 289 entries
in the currently tested original archives). It decodes individual sprite
pixel blocks only on selection, allowing live previews without converting
the entire archive to PNG.

### Catalog controls

- Type letters, numbers or backslashes — search by sprite name
- Backspace — erase one character
- Esc — clear search, or close the window if search is empty
- Tab — switch source filter: All / Base / Addon
- Up / Down and PageUp / PageDown — browse entries
- Home / End — jump to beginning / end
- Mouse click — select list entry
- Mouse wheel on list — scroll the list
- Mouse wheel on preview or + / - — integer zoom (1–12x)
- Left / Right — previous / next animation frame
- Space — play / pause
- Comma / period — frames per second, 1–24
- F2 — toggle checkerboard background

## Direct GD sprite preview (existing mode)

```powershell
cargo run --manifest-path rust/sprite-viewer/Cargo.toml -- "$addonGD" 'SA\VE\TRANSPORTSCHIFF\MG7'
```

## Legacy preview of a PNG folder

```powershell
cargo run --manifest-path rust/sprite-viewer/Cargo.toml -- "$root\SpritePreview"
```

These are **asset inspection tools**, not a playable strategy game.
The portable `game-core` remains independent of the graphics runtime.

The archives must be obtained and used in accordance with applicable licenses and copyright law.
