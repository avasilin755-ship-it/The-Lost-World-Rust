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
