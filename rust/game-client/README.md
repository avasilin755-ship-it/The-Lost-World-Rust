# The Lost World — Rust Game Client

This is the first **integrated** strategy client, not just a sprite viewer.
It combines original game archive readers, missions, maps, seeded terrain,
object rules, a portable simulation and a Macroquad GUI.

The original game is copyright-protected. We do NOT include its data files.
Provide resources from a copy you are allowed to use. No old EXEs are executed,
and assets are loaded locally from read-only original archives.

## Windows 11 — start a base game campaign

Open PowerShell in the project root:

~~~powershell
cd "$HOME\Projects\The-Lost-World-Rust"
git pull origin rust-port-foundation

$root = "$HOME\Games\DieVolker_Original"
$dataDir = (Get-ChildItem "$root\GameAssets\Base" -Filter dv.gd -Recurse -File |
  Select-Object -First 1 -ExpandProperty DirectoryName)
$addonDir = "$root\GameAssets\MissionPack"

cargo run --release --manifest-path rust/game-client/Cargo.toml -- "$dataDir" "$addonDir" "Sajikis.mdb" 0
~~~

Other shipped base campaigns: Pimmons.mdb, Amazons.mdb, Tutorials.mdb.
The final numeric argument is the mission **order** within that campaign (0-based).
Mission records are ordered by their original numeric index, not file position.

## Controls

- Left click: select a unit, building, animal or tree
- Right click: move selected unit; clicking a nature object starts gathering
- 1 / 2 / 3: house / town hall / warehouse construction tool
- Left click terrain with construction tool and selected unit: place site
- 0 / Esc: cancel construction placement
- WASD / arrows: pan the world camera
- Mouse wheel: zoom
- Middle mouse drag: pan
- P: pause and resume simulation
- F4: minimap
- F5 / F9: quick save / load a local TLWSNAP file
- The save path is under the project root in saves/ and ignored by Git

## How the new Rust modules relate to the original files

- game-core/gd.rs: GD sprite pixels, with lazy per-entry loading in the client
- game-core/od.rs: animation layers and original sprite anchors
- game-core/os.rs: 64x64 original terrain texture sets
- game-core/mission.rs: original MDB campaign records and DVM map files
- game-core/rules.rs: original ODB building footprints, costs and unit properties
- game-core/terrain.rs: MSVC generator and light values for the original base game
- game-core/sim.rs: platform-independent, fixed-step world state, A* routes,
  provisional gathering, delivery, construction, inventory and snapshots
- game-client/resources.rs: GD/OD GPU texture cache and original OS heightfield
- game-client/main.rs: mission loading, game loop, object input, HUD and minimap

## Status / limitations

### Implemented in the integrated client

- Genuine base-campaign seed terrain and 64x44 heightfield drawing
- Reading actual mission and object placements instead of the old 18x18 sample
- Gold campaign's embedded terrain reading (when AO maps are available)
- Original texture categories and shoreline artwork
- Original entity type IDs, sprite names, GD animation and OD anchor offsets
- Original ODB costs, building footprints, resource definitions
- Select, move with A*, gather a nature object, deliver one wood to town storage
- Place a house / town hall / warehouse, build after paying its ODB costs
- HUD, minimap, keyboard controls and local quick snapshots
- Build and Rust unit tests on GitHub Actions

### NOT yet feature-complete or equivalent to the original game

- Gold free-play island generator
- Full texture quad blending, correct corner lighting/fog and renderer cache
- Original 600-slot timing wheel and all unit jobs
- Accurate construction phases/doors/queues and porter capacity
- Correct per-town inventories, economy, population, housing and research
- AI nations, combat, diplomacy, ship navigation and multiplayer
- Campaign goals and progression, menus, mission briefings and sounds
- Original savegame compatibility
- Android-specific touch UI, APK packaging and performance validation
- Exact source-game equivalence on real map files; this requires local testing
- The early integrated version may reveal bugs with particular mission data

The project is deliberately honest about these omissions. Macroquad and the
user's original art are not enough to restore all systems automatically.

## Tests

~~~powershell
cargo test --workspace --all-targets
cargo test --manifest-path rust/game-client/Cargo.toml --all-targets
cargo check --manifest-path rust/game-client/Cargo.toml --all-targets
~~~

Keep the original GD/OD/OS/MDB/DVM/ODB files outside the repository.
