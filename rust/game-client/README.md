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

- Left click: select a unit, building, animal or tree by its visible sprite
- Left drag in the world: select a rectangle of up to 12 friendly units
- Shift + click/drag: add friendly units; Ctrl + click: toggle one unit
- Double click on a friendly unit: select nearby friendly units of the same type
- Right click: issue an order for all selected units, harvest, or resume unfinished construction
- Right drag: no order (matches the original drag-vs-click threshold)
- Left click on the minimap: centre camera on that position
- Right click on the minimap: move all selected friendly units to that position
- Left click the side-panel build rows: select a construction tool
- 1 / 2 / 3: house / town hall / warehouse construction tool
- Left click terrain with construction tool and selected unit: place site
- 0 / Esc: cancel construction placement
- WASD / arrows or moving the mouse to the world edge: pan the camera
- Mouse wheel: zoom; Page Up / Page Down: far-out / normal zoom
- Middle mouse drag: pan
- X: explicitly discard undelivered cargo when no reachable storage exists (cargo is lost)
- Building preview: green footprint means valid placement; red means invalid or worker busy
- Construction: finish the current job before placing another; sites have progress and can be resumed by right-click
- P: pause and resume simulation
- F4: minimap
- F5 / F9: quick save / load a local TLWSNAP3 file (preserves jobs, cargo and facing; TLWSNAP1/2 loads are supported)
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

## Resource-animation and gameplay fixes (2026-11-16)

- Static nature objects use their idle sprite until a worker harvests them.
  A short depletion animation plays once, at the end of harvesting.
- Animation frames are now derived from simulation job states, not wall time.
  Frozen/paused worlds no longer play resource destruction loops.
- Construction placement checks the complete ODB footprint, storage costs,
  current worker job, and path to the foundation perimeter.
- Incomplete buildings are tinted with a progress indicator; they can be
  resumed with a right-click, without paying the materials twice.
- Only completed storage buildings accept deliveries. A worker keeps cargo
  when storage is missing/unreachable, and automatically retries.
- To avoid getting stuck with the only builder carrying undeliverable goods,
  select that builder and press X. This **intentionally loses carried goods**.
- Until the original settlement-creation system is restored, each race is
  limited to one Town Hall to prevent unlimited free Town Hall placement.
- The renderer and simulation have automated regression tests. The exact
  appearance with the original external GD/OD assets must be verified locally.

## Directional units and nature sprite state (2026-11-16)

The original OD kind-0 record has 40 slots (five actions × eight facings).
The client now resolves **slot = action × 8 + facing** for each visible
unit, using the exact native direction mapping. Current facing survives
movement completion, pauses, and TLWSNAP3 save/load. Walk (MG), work (WO)
and carry (CA) select distinct animations, falling back to the matching
walk direction if the requested action has no dedicated sprite.

The original nature records deliberately have multiple actions:
- Tree (ODB class 8): **CA0** (slot 32) is the intact standing tree.
  Slot 0 is not the correct initial state for this object.
- Berry bush (class 9) and stone (class 13): **MS0**, frame 0 initially.
- Growing resources (classes 10–12): **ST0**.
- Buildings: unfinished **BU0** and completed **MS0**.

Only a gathering unit advances the resource depletion frames. A building
construction stage advances with actual construction progress. The
sprite cache is indexed by ID, action and facing rather than ID alone.

CI verifies the Rust implementation, but the external original GD/OD
archives are not included in the repository. Visual appearance of each
race and nature type needs a real client check with those assets.

## Native-control, wildlife animation and rendering performance update (2026-11-16)

The rebuilt Rust client now follows the original control concepts from
`docs/rendering/objects.md` and `docs/ui/side-panel.md`: alpha-visible object
clicks, own-unit priority, group selection, drag-box selection, Shift/Ctrl
selection semantics and group orders. The hit mask is currently generated
from the *first* sprite frame, so exact hit pixels of an animated frame can
still differ slightly from the historical compositor.

Animal sprites of ODB class 7 use real 8-way MG motion and native ID3 idle
cycles. A deterministic, limited roam scheduler is in the Rust simulation.
This is not the full original animal-home/path-planning/eating/respawn job 0x24
(`units-and-jobs.md` 7.7), which remains future restoration work.
Idle workers can turn and play their actual OD ID3/ID5 animation when it
exists; absent OD idle rows do not fall back to fake walking-in-place.

Original GD animation layers are now distinguished by OD role flags.
Independent random building details (RA*) animate individually instead of
cycling synchronously. Doors and production/work layers remain stopped
until the matching simulation event exists; they are not faked. Water
and shoreline animation are retained.

**Performance:** Minimap terrain is one uploaded GPU texture rather than
thousands of rectangles each frame; the decoded sprite cache uses bounded
LRU eviction instead of throwing all variants away; alpha hit tests use
one-bit masks; terrain water/coast names no longer allocate a new String
per rendered cell. The HUD displays live FPS and cache occupancy.

Existing limitations include original sprite motion interpolation, complete
building activity/door events, animal eating and regrowth, combat effects,
speech/sounds, the original multi-page interface and all command categories.
Regression tests and CI prove compilation and basic invariants, **not**
visual equivalence with external original game data or a specific FPS gain.
