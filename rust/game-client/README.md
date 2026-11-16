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
- F10: switch interface language Russian / English (Russian by default if Cyrillic TTF is available)
- Middle mouse drag: pan
- X: explicitly discard undelivered cargo when no reachable storage exists (cargo is lost)
- Building preview: green footprint means valid placement; red means invalid or worker busy
- Construction: finish the current job before placing another; sites have progress and can be resumed by right-click
- P: pause and resume simulation
- F4: minimap
- F5 / F9: quick save / load TLWSNAP6 (units, terrain, economy, porter cargo and unfinished-site stock); legacy TLWSNAP1-5 files remain readable
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
- Place a house / town hall / warehouse, request original ODB materials and build as porters deliver them
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

Existing limitations include exact original animation easing and timing, complete
building activity/door events, animal eating and regrowth, combat effects,
speech/sounds, the original multi-page interface and all command categories.
Regression tests and CI prove compilation and basic invariants, **not**
visual equivalence with external original game data or a specific FPS gain.

## Русская локализация и плавное движение (2026-11-16)

**Русский интерфейс включён по умолчанию**. Игровой клиент отображает
ресурсы, кампанию, меню строительства, названия известных игровых объектов,
статусы персонажей, сообщения, ошибки, подсказки и сохранения на русском языке.
**F10** переключает русский и английский без перезапуска игры.

Для кириллицы Macroquad загружает TTF **из самой операционной системы**;
проприетарные шрифты Microsoft не копируются в репозиторий:

- Windows: `C:\Windows\Fonts\segoeui.ttf`, `arial.ttf`, `tahoma.ttf`
- Linux: установленные DejaVu Sans или Liberation Sans
- Дополнительно можно указать `TLW_FONT` со своим TTF, содержащим кириллицу

Если подходящий шрифт недоступен, включается английский интерфейс и
выводится объяснение в PowerShell. Пример ручного выбора шрифта:

~~~powershell
$env:TLW_FONT = "C:\Windows\Fonts\arial.ttf"
powershell -NoProfile -ExecutionPolicy Bypass -File .\rust\game-client\launch.ps1
~~~

Перевод не переписывает оригинальные ODB/OD/GD: используются русские
эквиваленты по подтверждённым идентификаторам типов, а неизвестные
названия остаются на исходном языке до пополнения словаря. Сейчас
переведены большинство основных ролей юнитов и зданий, типы ресурсов,
одна известная миссия «Засуха» и весь действующий интерфейс прототипа.
Полная локализация сюжетных текстов оригинальной игры ещё не завершена.

**Перемещение:** игровые координаты и A* остались детерминированными
и сохраняются в TLWSNAP3, но спрайты юнитов и животных теперь плавно
интерполируются между соседними клетками в течение 6 игровых тиков.
Интерполяция учитывает замыкание координат карты по краям и применяется
к отрисовке, попаданию мыши и выделению рамкой. Это визуальный слой:
состояние экономики и симуляции не меняется.

Проверка `cargo check`/`cargo test` в CI подтверждает компиляцию
и автоматические регрессии. Визуально подтвердить отображение
кириллицы, разбиение надписей и плавность анимации можно только
запуском в Windows с оригинальными ресурсами.


## November 16, 2026 - classic HUD / terrain / rules playtest update

See [TEST_CHECKLIST.md](TEST_CHECKLIST.md) for precise Windows acceptance tests.

- Real original race UI background and function icons from local interface.gd.
  Original layout reference: docs/ui/side-panel.md, 192x768 logical panel.
  World fills the full viewport height; top mini-map, layer toggles,
  zoom ring, treasury chest, town, build, selection and settings pages.
  Font rendering remains in Russian and English (F10).
- Torus rendering across map boundaries, including camera indicator and
  animated sprites; improved height-adjusted world picking.
- Original terrain material cross-fade approximation instead of hard seams
  between grass / snow / rock. Coastline still needs in-game verification.
- Desktop 60 FPS cap (TLW_MAX_FPS=30..144), F3 performance overlay.
- Rasterized terrain/object blockers accelerate A* routing.
- Original-style paved, leveled foundations when loading base-map buildings
  and after new construction; refreshed minimap after build/load.
- Work roles: builder cannot harvest nature. Porter harvests bushes/crops,
  woodcutter harvests trees, stonemason extracts stone. A new base
  mission lacking surface workers starts with a builder and two porters.
- TLWSNAP4 stores permanent terrain changes. Earlier TLWSNAP1/2/3 saves
  remain loadable, although their original format cannot include edits.

This update has automated compile/unit tests, but real original artwork,
in-game rendering, Windows responsiveness and full scenario behavior
require the player's next screenshot/video. It is not a complete port.


## Second video review (2026-11-16, 78 seconds)

The second recording confirmed that the native race panel, Russian UI,
a wrong-worker warning, moving units and house construction display on
Windows. It showed approximately 51 FPS at 1.85x zoom in a 660-object
scene. That measurement alone is insufficient to attribute a specific
CPU/GPU bottleneck.

The subsequent update ports the minimap coordinate system (original
transposed x/y mapping), 152x152 image and RGB averages from original
OS files, relief shading, 3x3 blur, toroidal camera centring, original
race-coloured markers and viewport diamond. It also returns from
building placement to the selected site's page after a successful
construction order.

Press F3 to expose averaged per-frame CPU timings for game updates,
world rendering and the HUD. These separate game simulation cost from
terrain and interface drawing. The next video should keep the
diagnostic overlay visible throughout the camera and zoom checks.

See [the playtest checklist](TEST_CHECKLIST.md).


## 2026-11-16, third playtest repair package

Source: uploaded 56.53-second desktop review recording.
The original interface, animated construction and F5/F9 prompts work.
See [TEST_CHECKLIST.md](TEST_CHECKLIST.md) for evidence, limitations
and exact timestamp-based validation requirements.

**Updated client:** original-style per-type building/unit/nature
selection panel, working command to resume an incomplete site,
build-order profession checks, disabled invalid build buttons and
per-good town storage capacity display.

**Updated core:** 600-slot game scheduler in scheduler.rs with LIFO
job order, persistent job holds and deterministic tests. Original
economy formula from economy.rs consumes food/luxuries every 299
gameplay ticks using float32 fractional carry. The live client currently
counts outdoor own-race units as population; matching original town
indoor residents/ownership is a remaining migration. Building work
uses ten-stage, 15-tick/material-sensitive timing approximation rather
than the earlier flat 100 ticks and increases HP as it progresses.
At the time of the third playtest, porter-driven deliveries had not yet been implemented; see the fourth playtest update below.

**Saves:** TLWSNAP5 stores material/food/luxury fractions and race
participation in addition to previous terrain and unit state; legacy
versions 1–4 remain readable. The save is not compatible with original
1999 game executables.

**Graphics:** 60 FPS native limiter now uses coarse sleep followed
by a short scheduling-yield phase to avoid Windows timer oversleep.
The captured 54 FPS at 2.05x zoom was not CPU rendering-bound:
the overlay showed ~0.1 ms update, 1.6 ms world and 0.4 ms HUD.
The new frame pacing requires desktop confirmation.

All new functionality is on the same rust-port-foundation branch, not
a second standalone demo.


## Fourth playtest and integrated logistics / terrain-atlas update — 2026-11-16

The 94.57-second Windows recording demonstrates mid-construction
loading (80% back to 40%), resuming a builder, and completion with full
HP. In the same video the FPS is 58 at 2.05x but only 34 at 0.50x,
even though CPU world drawing takes roughly 1.2–2.5 ms.

**Original logistics now implemented as an initial functional slice:**
each unfinished site has an independent requested-goods list and local
stock; placing it no longer instantly removes all costs from town goods.
Idle original porters (type 9) walk to a completed town hall/warehouse,
take one available good, then carry it to the construction site. The
town's inventory decrements only at actual pickup. The builder consumes
one delivered unit at a time in source good-index order, waits when the
first remaining good is not available, and advances work on its next
15-tick construction poll. Materials not in stock do not prohibit
placement; the unfinished site waits for production/delivery. Stock
limits for completed halls and warehouses now clamp arrivals per good.

**Versioned snapshots:** TLWSNAP6 persists per-site needed/received
materials and porter pickup/delivery routes. F9 can still read
TLWSNAP1–5. Older unfinished sites get reconstructed local inventory
so a pre-logistics save does not deadlock.

**Rendering optimization:** the original opaque 64x64 OS terrain
frames now share one tightly packed RGBA GPU atlas instead of
requiring per-variant texture binding. Isometric material blending and
original coastline frames still select the same images and
UV orientation. It is expected to improve distant camera performance,
but the exact gain and visual correctness must be verified on the
player's GPU (GitHub CI does not render the proprietary local assets).

**Still missing versus the original:** exact porter dispatcher
priorities and door queues, source-building visits, per-town rather
than global inventory, site reservations and refund/cancellation
semantics, multiple simultaneous builders with distinct work phases,
delivery animations, producer operations and training, native fog and
territory checks, and fully accurate construction cost scheduling.

See the new [fourth-playtest section](TEST_CHECKLIST.md)
for the complete next-test protocol.
