# The Lost World — Rust prototype tools

## Isometric scene test (NEW)

The scene also loads original .os tile sets from the same extracted Data
directory as dv.gd: wiese.os, weg.os, wasser1..4.os and transition1..4.os.
These supply original grass, gravel, animated water and coastline artwork.
Cells use the original 64x44 isometric proportions. If any files are
missing, the old flat-colour test tiles are used as a fallback.

Limitations: the test land/sea outline is still synthetic; original map
generation, full corner blending, elevations, building footprints and
world simulation are separate later milestones.
Launch a small experimental terrain with original GD building, unit,
ship and neutral-nature sprites. The terrain shading and object placements
are an illustrative testbed, **not yet a recreation of the original maps**.

Run these commands in PowerShell from the repository root:

```powershell
$root = "$HOME\Games\DieVolker_Original"
$baseGD = Get-ChildItem "$root\GameAssets\Base" -Recurse -File -Filter "dv.gd" | Select-Object -First 1 -ExpandProperty FullName
$addonGD = "$root\GameAssets\MissionPack\ADDON.GD"
cargo run --manifest-path rust/sprite-viewer/Cargo.toml -- --scene "$baseGD" "$addonGD"
```

Controls: WASD/arrow keys pan; mouse wheel zooms; right mouse drags;
left click selects a tile; keys 1 tree, 2 house, 3 person,
4 ship (water only), 5 school, Delete removes objects from a selected tile;
Escape exits. Land/water placement restrictions apply.
Original sprite layers, render anchors and painter ordering are early approximations.
There is no pathfinding, production, combat, save system, AI or actual terrain data yet.

---

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
