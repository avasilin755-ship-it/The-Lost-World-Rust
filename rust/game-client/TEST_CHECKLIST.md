# The Lost World - Windows playtest checklist

Review source: user's 43-second video, 16 November 2026.
This document lists what to test. It does not claim that local
game assets, live gameplay or an Android device have been tested.

## Second recorded playtest — 2026-11-16 (78 seconds)

Source: 20261116-1849-17.8162989.mp4 (1918x1014, 30 fps capture).

**Observed as working:** original race-specific side-panel art; Russian/English
switching (00:03/00:09/00:12); native-unit selection and movement (00:15-00:36);
profession-restricted right-click messages (00:18); building placement and
construction stage visuals (00:42-00:58); camera panning and terrain/trees/animals
(01:03-01:17); F5 *save* notification in English (00:09).

**Observed defects or deviations:** original minimap's transposed,
camera-centred orientation was not implemented; colours were a simple
green/blue classification instead of sampled OS textures; viewport marker
was a circle rather than a diamond; builder remained in active placement
mode after successful construction; diagnostic overlay showed roughly
51 FPS at 1.85x zoom on a scene with 660 objects (00:03).

**Corrections since this recording:** original 152x152 minimap palette,
relief shade and Gaussian blur, wrapped image scroll around the camera,
transposed click mapping, native race-coloured object markers and viewport
diamond; exit placement mode after a successful site creation; F3 CPU
times for simulation/update, world renderer and HUD.

**Not proved by this recording:** F9 restoration after a built foundation,
four-way wrapped map seam, all minimap toggles and drag behaviours, combat,
full resource logistics, correctness of the real Gold campaign, Android,
long-duration memory/performance. The F5 notification proves that the
save action was accepted, not that its contents can be restored perfectly.

### Targeted checks for the next recording

- [ ] Open the minimap, click near its centre: world camera should
      remain in place. Click near its top edge: centre moves to the
      corresponding original map X, not Y. Repeatedly pan; map scrolls
      under the **centred white diamond**, wrapping seamlessly.
- [ ] Observe snow/grass/forest/lake colours on the minimap. Snow should
      be pale and water blue, with no dominant featureless green image.
- [ ] Select a builder, place a house, then check the sidebar switches
      to selected-site information and the red building outline vanishes.
- [ ] Keep F3 on while zooming in/out and navigating a forest:
      report FPS and **CPU update/world/HUD ms** at each zoom level.
- [ ] Build, save F5, move camera and load F9. Confirm the building
      stage, resources, position, and levelled ground are restored.
- [ ] At 1x, inspect trees, rock, berries and snow/water transitions
      for overlays, flicker, clipping and mismatched animations.

## Third Windows playtest — 2026-11-16 (56.53 s)

Source: user-uploaded 20261116-1905-48.3852257.mp4 (1920x1032).
The game is recorded for approximately seconds 0–41, then the video
switches to unrelated desktop applications.

### Confirmed

- 00–16: original HUD, terrain-coloured minimap and camera movement.
- 17–20: selected builder and green multi-cell building footprint.
- 21–31: house cycles through construction sprites and completes.
- 33–38: Russian F5 and F9 success notices appear; completed house
  remains visible after load. This does NOT validate mid-build restore.
- 38: diagnostic overlay shows ~54 FPS at zoom 2.05x, 660 objects,
  65 GD cache entries, CPU update 0.1 ms, world 1.6 ms, HUD 0.4 ms.
  Combined ~2.1 ms CPU drawing/update is not sufficient to explain a
  18.5 ms frame; OS pacing, vsync and GPU timing remain candidates.

### Confirmed defects and fixes since the recording

- A selected building formerly displayed the worker-only Idle / Свободен
  status: replaced with proper translated state, costs, dimensions,
  HP and progress in the selection page.
- Construction finished in about ten seconds and was at maximum HP
  immediately: introduced original-inspired 15-tick work/material
  pacing and ten stages of progressive HP. The exact per-material
  porter delivery loop remains unimplemented.
- Any owned unit could build: only type-11 builders can now start or
  resume work. Resume button dispatches an idle builder to a site.
- Duplicate town hall and missing-resources build buttons now refuse
  selection; town overview shows storage capacity per good.
- Added an independent, tested original 600-slot scheduler module
  (LIFO execution, holds, periodic jobs). Full job integration is pending.
- Live food/luxury consumption now runs each 299 ticks with original
  float32 accumulation. For now, population is approximated by living
  outdoor player units; indoor residents and independent towns remain
  to be ported.
- TLWSNAP5 persists food/luxury fractional needs and participating races.
  Old snapshot versions 1–4 remain readable.
- Improved the native 60 FPS sleep margin to reduce Windows oversleep.
  Still requires confirmation on the player's desktop.

### Targeted checklist for the next video

- [ ] Build a house and note time from 0 to 100%. There should be ten
      construction steps, progressive HP, and a slower build than before.
- [ ] Save F5 while the house is 20–70% complete, let it finish, load
      F9, and verify the previous percentage, worker task and inventory.
- [ ] Select a porter, try placing a building; it should be rejected.
      A builder should still be able to place and resume sites.
- [ ] Stop work on an unfinished house by moving the builder, select
      the house and click Продолжить стройку. Work should restart
      without paying building costs again.
- [ ] Click a completed house: it should display Построено rather
      than Свободен, with HP and footprint details.
- [ ] Open the Town page and verify capacity of 25 per completed
      town hall/warehouse; construction sites add no capacity.
- [ ] Observe food stock for at least two 299-tick rounds (~60 sec);
      it should not decrement once per rendered frame. Indoor
      population is not simulated yet.
- [ ] Keep F3 visible at zoom 1x and 2x. Report FPS plus CPU update,
      world and HUD milliseconds, and monitor refresh rate.
- [ ] Check minimap/world alignment while panning across a torus seam.
- [ ] Test original settings and HUD pages at both 1280x720 and
      1920x1080. Check Russian text truncation and tiny controls.

## Fourth Windows playtest — 2026-11-16 (94.57 seconds)

Source: attached video 20261116-1932-39.4098216.mp4, recorded in
1920x1038 at 30 video fps.

**What the video confirms:** Original Saxiki race HUD, Russian labels and
animated objects remain visible; the house reaches 50% at about 27 s,
80% at 36 s, and returns to an earlier ~40% construction stage after a
load near 45 s. A "builder assigned to construction" toast appears
near 54 s and the finished house has 350 HP with status Built at 63 s.
Saving mid-build and continuing after load is therefore visually
verified in this capture.

**Performance regression:** F3 at 81 s: about 58 FPS, zoom 2.05x,
661 objects, 74 cached sprite entries, update 0.1 ms, world 1.2 ms,
HUD 0.3 ms. F3 at 90 s: about 34 FPS at zoom 0.50x, update 0.0 ms,
world 2.5 ms, HUD 0.2 ms. Neither total CPU time alone explains the
elapsed frame duration; excessive GPU texture changes or sync are
possible causes, not proven by the capture.

**Main confirmed original parity gap:** Up to this recording,
construction resources were charged at placement and did not need to
be physically brought by porters to the site. The new code uses a
separate site ledger, original-style one-unit porter trips and
15-tick builder consumption/work phases. New TLWSNAP6 stores active
transport orders and site inventory. The terrain renderer now packs
original OS frames into one GPU atlas to cut texture switches at zoom
0.50x. Both changes need **new visual validation**.

### New targeted regression checklist

- [ ] Start a house (original cost 3 wood, 4 stone in this map). Immediately
      verify the town inventory is unchanged and the selected site's
      delivered/remaining counts start at 0/3 and 0/4, rather than
      consuming all stock at placement.
- [ ] Watch the two original type-9 porters: one must travel to the
      completed hall/warehouse, collect a single item and walk back to
      the site. The city's stock must decrease only on pickup. There
      should be no instant teleporting materials.
- [ ] When the builder is waiting for wood/stone, the construction
      percentage must **stop**. After a matching item arrives, the
      builder consumes it and the next work step advances progress.
- [ ] Create a site without enough wood/stone. It must be allowed and
      remain waiting instead of becoming a free completed building.
      The original build menu allows such placements.
- [ ] While a porter carries an item, press F5, let them deliver it,
      then F9. The porter and building must return to their saved
      positions, the site material count must not duplicate, and
      construction should continue after the delivery.
- [ ] Save and load a site at 40–80% and verify exact HP, progress and
      needed/local material quantities. Also test loading an older
      TLWSNAP5 save into the new client without corruption.
- [ ] Test interruption: manually send a porter elsewhere after pickup;
      cargo must not disappear or cause the site to receive extra
      resources. The worker can bring it to a completed store later.
- [ ] Complete a second warehouse. Check the original storage capacity
      increases by 25 for every good; delivered goods must not exceed
      the per-good maximum. Construction sites add zero capacity.
- [ ] Repeat F3 measurements in the **same** landscape, without
      changing the window: 0.50x, 1.00x, 2.05x. Note FPS and CPU
      update/world/HUD time for at least 10 seconds at each zoom.
      The prior baseline is 34 FPS at 0.50x and 58 FPS at 2.05x.
- [ ] Examine snow, ground, shore, water animation and mixed-class
      tile borders **after the atlas change**. Verify there are no
      new black tiles, texel bleeding, swapped OS variants, diagonal
      UV errors, flicker or seams at non-integer zoom.
- [ ] Move the camera beyond all four world edges and compare the
      mini-map's scrolling with the game view. No gaps or missing
      objects should appear.
- [ ] Test Russian selection/build/settings pages at 1280x720 and
      1920x1080. Material counters and the "Resume construction"
      button must remain readable and clickable.

**Explicit remaining original-system gaps:** This is not the complete
1999 construction and porter system. The original prioritized request
dispatcher, door queues, per-town stock, transport cancellations and
actual producer jobs still need porting. The 600-slot scheduler is
implemented as a separate Rust module but most live tasks are still
updated directly from Simulation::step.

## Pull and launch

~~~powershell
cd "$HOME\Projects\The-Lost-World-Rust"
git pull origin rust-port-foundation
cargo test --workspace --all-targets
cargo test --manifest-path rust/game-client/Cargo.toml --all-targets
powershell -NoProfile -ExecutionPolicy Bypass -File .\rust\game-client\launch.ps1
~~~

If there is a problem, attach a screenshot and the full PowerShell error.
Keep original copyrighted game data out of public repositories.

## Visual / renderer

- [ ] Original INTERFACE1024 appears as the race-specific right side panel.
      PowerShell reports the count of race UI assets loaded. If it says
      fallback, note which interface.gd could not be found.
- [ ] The world occupies the entire viewport height without the previous
      always-on black top status bar and bottom help bar.
- [ ] Pan across all map edges with WASD: no huge black empty regions.
      The toroidal world and entities wrap smoothly.
- [ ] Zoom 0.5x to 2.75x using mouse wheel and the original zoom slider.
      No object teleporting, texture gaps or misaligned selection circles.
- [ ] At snow, grass, rock and forest borders, observe blended textures.
      Check for seams, checkerboards, triangles and flicker.
- [ ] Examine coasts: animated water, shoreline and beach textures.
      Original coastline masks still require additional parity testing.
- [ ] Buildings stand on flat, paved foundations; doors and roofs
      align with the ground and do not render below the terrain.
- [ ] Trees, rocks, berries stay in intact animation unless harvested.
      Units face their direction of travel; animals wander and idle.
- [ ] Check click picking of small animals/berries and units on hills.

## Simulation

- [ ] In the base campaign without surface units, a completed town
      hall starts with 1 builder (type 11) and 2 porters (type 9).
- [ ] Left click, drag selection rectangle, Shift-add, Ctrl-remove
      and double-click near identical units.
- [ ] Order units to walk across terrain, obstacles and wrapped edges.
      No clipping through buildings or deep water.
- [ ] A builder cannot gather from trees or bushes. Porters can gather
      crops and bushes, woodcutters (type 19) trees, stonemasons
      (type 27) stone. Wrong-worker error must be localized.
- [ ] Gathering resources routes to a usable storage building.
      No reachable storage should lead to holding the cargo.
- [ ] Choose a supported building from the original HUD page;
      green/red placement preview matches real occupied cells.
- [ ] Building costs deducted only once, a reachable worker builds
      it, and a paved foundation appears below it.
- [ ] F5/F9 restores construction, worker task, facing, carrying,
      resource counters and terrain foundation changes.
- [ ] Pause P freezes the simulation tick and motion.

## Classic HUD / performance

- [ ] Original right panel: top minimap, layer switches, options
      control, zoom ring, town, gold, icon function bar and pages.
- [ ] Click each minimap layer icon; correct layer toggles.
- [ ] Click minimap to center camera; right click it to send a move
      command; white camera marker stays on map across wrap.
- [ ] Town, Build, Selection and Settings page buttons work at
      1280x720 and 1920x1080.
- [ ] Pause, F5 save, F9 load, F10 Russian/English work from UI
      and keyboard. Russian labels are complete and readable.
- [ ] F3 shows the diagnostics overlay. Desktop FPS is targeted
      to 60 by default, changeable with TLW_MAX_FPS=30..144.
      CPU/GPU should no longer run the prior 600-800 FPS busy loop.
- [ ] Play 10-15 minutes; look for freezes, crashes, wrong sprites,
      cache thrashing or high CPU use.

## Remaining unimplemented game systems

This release does not yet recreate the original full economy,
600-slot scheduler, needs, housing, training, science, diplomacy,
advanced logistics, AI opponents, battle, sea warfare, mission goals,
multiplayer, original-game save format, sound or Android touch UI.
Some ground blending and HUD pages are still approximations.

## Suggested next 60-second video order

1. Press F3. Move the camera across all four map edges.
2. Select and move workers around obstacles.
3. Try gathering with a builder, then a porter.
4. Construct a house, save with F5, reload with F9.
5. Click each original interface function, minimap layer and
   Settings button. Test F10 language switching.

Report the timestamp, exact action, expected result and observed result.
