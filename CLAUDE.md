# LightBeat — Onboarding for a fresh Claude session

A beat-synchronized, node-graph–based lighting control app written in Rust + egui.
Drives DMX over ArtNet/sACN, syncs tempo with Ableton Link.

This document is **dense on purpose** — read it in full before exploring code.

---

## 1. Architecture in one paragraph

There are **two threads**: a UI thread (egui, ~60fps) and an **engine** thread
(~1 kHz). They communicate two ways:
1. A **lock-free ringbuf** (`ringbuf` crate) carries structural commands
   (`EngineCommand`: AddNode, AddConnection, RemoveNode, LoadData, …).
2. Per node, an `Arc<Mutex<NodeSharedState>>` carries values both ways: engine
   writes outputs/inputs/display every tick, UI reads them; UI pushes
   `pending_params` and `pending_config`, engine consumes them.

A node has **two halves**:
- **`ProcessNode`** trait (engine side, in `src/engine/nodes/...`) — pure
  processing. Owns the values. Implements `process()`, `read_output`,
  `write_input`, `save_data`, `load_data`, `update_display`.
- **`NodeWidget`** trait (UI side, in `src/widgets/nodes/...`) — rendering and
  inspector. Holds `Arc<Mutex<NodeSharedState>>`. Implements `show_content`,
  `show_inspector`, `ui_inputs`, `ui_outputs`, `on_ui_connect`,
  `on_ui_output_connect`.

Multi-channel ports (`Color`=3, `Position`=2, `Palette`=12, `Gradient`=40) are handled
by `port_base_index()` and `total_channels()` from `engine::types`.

---

## 2. Directory map

```
src/
├── main.rs                       # App, window setup, node factory registration,
│                                 # wire_new_nodes (engine side spawn for each
│                                 # widget), file-dialog plumbing, save/load.
├── engine/
│   ├── mod.rs                    # EngineHandle, ringbuf wiring, thread spawn.
│   ├── graph.rs                  # EngineGraph: tick(), apply_command(),
│   │                             # SubgraphInnerCommand routing.
│   ├── types.rs                  # NodeId, PortDef, PortType, ParamDef,
│   │                             # ParamValue, ProcessNode trait, EngineCommand,
│   │                             # NodeSharedState, port_base_index,
│   │                             # total_channels.
│   ├── nodes/                    # ProcessNode implementations grouped by
│   │   ├── io/                   # category (matches the menu categories):
│   │   │                         # clock, internal_clock, audio_input,
│   │   │                         # input_controller, launchpad, push1, x1.
│   │   ├── transport/            # lfo, envelope, phase scaler, transition,
│   │   │                         # smoothing, latch, sample_hold, sequencer...
│   │   ├── math/                 # constants, oscillators, logic, counters,
│   │   │                         # color/palette/gradient ops, lookup, schmitt.
│   │   ├── display/              # scope, value/color/LED display.
│   │   ├── meta/                 # subgraph (InnerGraph + bridge nodes),
│   │   │                         # portal (wireless connections).
│   │   ├── output/               # group, group_preview, effect_stack.
│   │   └── ui/                   # button(+group), fader(+group), xy_pad,
│   │                             # peak_meter, gradient_stops.
│   └── effects/                  # Effect Stack + effects, EffectHub, geometry.
├── widgets/
│   ├── inspector.rs              # show_inspector(node) — generic params/ports view.
│   ├── dmx_monitor.rs            # Per-universe channel grid w/ test universes.
│   ├── fixture_list.rs           # Fixture template editor (channels incl. LedStrip).
│   ├── object_list.rs            # Fixture instances (address, interface).
│   ├── group_list.rs             # Groups + per-strip layout + member reorder.
│   ├── color_palette_list.rs     # Palettes (a palette = 4 colors).
│   ├── color_palette_group_list.rs
│   ├── gradient_preset_list.rs   # Reusable gradient presets.
│   ├── input_controller_list.rs  # Virtual MIDI/X1 controllers + learned inputs.
│   ├── audio_input_list.rs       # Audio inputs + analyzer lists.
│   ├── interface_list.rs         # ArtNet/sACN interface config.
│   └── nodes/
│       ├── graph.rs              # NodeGraph: levels, navigation, interactions,
│       │                         # connections, clipboard, context menu,
│       │                         # cleanup_stale_connections (drops wires whose
│       │                         # types are no longer compatible).
│       ├── node.rs               # NodeWidget trait + layout helpers.
│       ├── types.rs              # UI port colors, UiPortDef.
│       └── <category>/<name>.rs  # widget per node, mirrors engine/nodes structure.
├── color/                        # Reusable color types/utilities — USE THESE for
│   │                             # any new color/blend/palette code.
│   ├── types.rs                  # Rgb, Rgba, Hsv with lerp/clamp/from_egui/etc.
│   ├── blend.rs                  # BlendMode (Override/Add/Max/Min/Multiply) —
│   │                             # used by Effect Stack compositing.
│   ├── gradient.rs               # Gradient with stops + sample().
│   ├── convert.rs                # color-space conversions, float_to_dmx.
│   └── temperature.rs            # rgb_to_rgbw with Kelvin.
├── objects/
│   ├── channel.rs                # ChannelKind (Dimmer, Color, PanTilt, Raw,
│   │                             # LedStrip{count, format}). PixelFormat is now
│   │                             # a Vec<PixelChannel> with manual deserializer
│   │                             # accepting legacy enum forms.
│   ├── fixture.rs                # Fixture template (id, name, channels).
│   ├── object.rs                 # Object: instance with DMX address + iface.
│   ├── group.rs                  # Group + StripLayout (logical_start/end).
│   ├── color_palette.rs          # ColorPalette (4 colors), ColorPaletteGroup.
│   ├── universe.rs               # DmxUniverse buffer.
│   └── output.rs                 # OutputConfig (ArtNet/sACN serialization).
├── interfaces/                   # ArtNet/sACN sender impls.
├── audio/                        # Audio input subsystem: AudioBackend trait
│                                 # (cpal impl), per-input analyzer threads
│                                 # (beat clock, peak level, envelope); the beat
│                                 # tracking itself is in audio/tracker/.
├── input_controller/             # Persistent virtual MIDI/X1 controller slots
│                                 # with learned inputs; midir + reconnect worker.
├── macros/                       # .lbm library of reusable subgraphs.
├── theme.rs                      # ALL UI colors (neutrals, SEM_*, TYPE_*).
├── perf.rs                       # Lock-free Gauge counters, NodeProfiler.
├── link_controller.rs            # Link session snapshots/events for the UI.
├── beat_clock.rs                 # Ableton Link wrapper, BeatPattern subscriptions.
├── dmx_io.rs                     # DmxOutputManager (engine side), shared state,
│                                 # per-channel Override mechanism, test_universes.
├── project.rs                    # Save/load NodeGraph (root level + recursive
│                                 # inner_graph for subgraphs). Restores widget
│                                 # state for SubgraphWidget / GroupWidget /
│                                 # EffectStackWidget after engine load_data.
├── setup.rs                      # SetupFile (fixtures, objects, interfaces,
│                                 # groups, palettes, gradient presets, input
│                                 # controllers, audio inputs) — separate from
│                                 # project.
└── config.rs                     # AppConfig (autoload, snap_to_grid, etc.).
```

---

## 3. Concepts that are hard to infer from code

### Port types (`engine::types::PortType`)
- `Logic` (yellow), `Phase` (purple), `Untyped` (red), `Any` (gray, accepts
  anything), `Color` (cyan, 3 channels), `Position` (greenish, 2 channels),
  `Palette` (warm white, 12 channels = 4×RGB), `Gradient` (turquoise, 40
  channels = 8 stops × (r,g,b,a,position); stops with alpha < 0 are unused).
- Colors for ports come from `theme::TYPE_*`; never put inline `Color32`s in
  widgets, add a constant to `theme.rs` instead.
- `Any` ports render as a **white ring** with no fill. `Any.compatible_with(*)`
  is always true. Use `Any` for "neutral / not yet decided" states.

### Palette
- A **palette is a set of 4 colors** (Primary/Secondary/Third/Fourth).
- The data type is `ColorPalette` (struct, in `objects/color_palette.rs`).
- The signal type is `PortType::Palette` carrying 12 floats (4 × RGB).
- A `ColorPaletteGroup` is a collection of palettes. The PaletteSelect node
  picks one palette from a group by indices.
- Throughout the codebase, "palette" — never "stack". The rename is complete
  including JSON files. There are NO backward-compat aliases anymore.

### Groups & strip layout
- `Group { object_ids: Vec<u32>, strip_layout: Vec<StripLayout> }`.
- `StripLayout { object_id, logical_start: f32, logical_end: f32 }` defines
  where a strip's first/last LED maps onto the group's normalized 0..1 axis.
  `end < start` means the strip is reversed.
- For non-strip Color fixtures in a group, the **Effect Stack** computes
  implicit positions from `i / (n-1)` based on order in `object_ids`. The
  group_list UI has Up/Dn buttons to reorder.

### Gradient
- `PortType::Gradient` carries `GRADIENT_STOP_COUNT` (8) stops. Convert with
  `Gradient::from_channels` / `sample_with_alpha` in `color/gradient.rs`.
- Sources: the Gradient Source node (can load a `GradientPreset` from
  `SetupFile::gradient_presets`; presets are copied, not linked) and Palette
  to Gradient.
- **Group Output** takes `select / width / gradient` and samples the gradient
  per LED. **Group Output Preview** (`output/group_preview.rs`) has the same
  inputs but draws LED dots instead of sending DMX; `Group::preview_columns`
  controls how strips are laid out in it.

### Effects and stacks (`engine/effects/`, `widgets/nodes/output/effect*.rs`)
- An **effect** is a small node (Fill, Wave Loop, Ripple Burst, ...) with control
  inputs and no outputs. It picks an **Effect Stack** by name in its inspector,
  renders its own pixel layer about 60 times a second and hands it to the
  `EffectHub`. The stack blends its linked effects' layers in the order of its
  list (top first) and publishes one image; once per frame the hub flattens all
  stacks, by priority, into the objects' pixels. Effects never write to the
  objects, so they cannot fight over them. A group may have several stacks;
  a stack has `priority`, `blend` and an `amount` input, so stacks work as scenes.
- Names: a **Burst** is spawned by a trigger and lives for a while (ripple,
  comet); a **Loop** is a function of a phase (wave, chase, fill). Not "voices".
- To add an effect: implement `EffectKind` (ports, params, `update` for state,
  `render`) in `engine/effects/`, give it an `EffectSpec` in
  `widgets/nodes/output/effect.rs`, register widget + engine arm in `main.rs`.
  Parameters are `ParamDef`s, so any of them can be promoted to an input.
  Triggers are edge-counted in `write_input` at the full tick rate (a 1 ms pulse
  is not lost); rendering is gated to `RENDER_HZ`.
- Link by **name**, like portals, so copy/paste and macros keep working. The
  graph (`reconcile_effect_stacks`) hands each stack the effects that point at
  it every frame, follows renames and publishes the names for the pickers. The
  stack stores order, blend, opacity, mute and solo per effect; linked nodes
  share an accent colour (`theme::link_color`) and are highlighted together via
  `NodeWidget::link_key`.
- Positions: `Geometry::position` gives (x, row). There is only one row until
  groups get a real layout (see things.txt), but effects already measure
  distances in two dimensions.

### Subgraph (`engine/nodes/meta/subgraph.rs`)
- Acts like a "module" with its own inner graph and configurable input/output
  ports. Double-click (or "Open" button) to navigate in. Double-click empty
  canvas inside to navigate out.
- Inside, two **bridge pseudo-nodes** appear: `GraphInputWidget` / `GraphOutputWidget`
  (NodeIds `u64::MAX-1` and `u64::MAX`). Connections to/from them are saved
  along with all inner connections.
- `EngineCommand::SubgraphInnerCommand { subgraph_path, command }` routes
  inner mutations to the right subgraph (supports up to 2 levels of nesting
  in `apply_subgraph_inner_cmd`).
- "Move selection into subgraph" right-click action analyzes cut wires,
  deduplicates by source port, creates a new Subgraph + bridge connections,
  preserves params via `pending_params`/`pending_config`.

### Mode auto-detection pattern (NEW)
- Nodes that worked across multiple types (Color Split/Merge, Color Display,
  Transition, Scope) used to have a "mode" dropdown forcing the user to pick.
- New convention: default `Mode = Neutral`, ports are `Any` (white ring),
  process is no-op. On the **first wire** attached to either side, the widget
  infers the mode and pushes it to the engine via `pending_params`.
- Wired up by the trait callbacks `on_ui_connect(input_port, source_type)`
  and `on_ui_output_connect(output_port, dest_type)`. Both are called by
  `graph.rs::add_connection`.
- Inspector still has the dropdown so the user can manually force/reset to
  Auto. When they switch modes, the new `cleanup_stale_connections` (which
  also checks **type compatibility**, not just port-index range) drops any
  wires whose types no longer match — so a manual Auto reset disconnects
  everything cleanly.
- See Color Split/Merge as the canonical example. Scope was the original
  inspiration but only auto-detects on the input side.

### LED strips
- `ChannelKind::LedStrip { count, format: PixelFormat }`. `PixelFormat` is
  `{ channels: Vec<PixelChannel> }` where each `PixelChannel` is `Red`,
  `Green`, `Blue`, or `White { temperature_k }`. Presets: `PixelFormat::rgb()`,
  `grb()`, `rgbw(temp)`, `grbw(temp)`, `grbww(t1, t2)`. White channels are
  computed via `rgb_to_rgbw(rgb, temperature_k)` per channel.
- Custom `Deserialize` impl on `PixelFormat` accepts the legacy enum form
  (e.g. `{"Rgbw":{"white_temperature":6500}}`) so old setup.json files load.
- The fixture editor lets you set pixel count (auto-resizes the value
  buffer) and pick format from a preset dropdown + per-W-temperature pickers.

### DMX monitor
- Has a separate **interface** dropdown and **universe** number picker.
  Selecting any (interface, universe) registers it as a `test_universe` in
  shared state, so the engine ensures it exists on that interface — this lets
  you Ctrl+drag overrides on a wire without first setting up fixtures/objects.

### Portals (`engine/nodes/meta/portal.rs`)
- Wireless connections, matched by name. Two independent families:
  **Output Portal** (one TX with inputs, many RX with outputs: broadcast) and
  **Input Portal** (one RX with outputs near consumers, one TX with inputs
  far away). Registries are separate, so a name can be reused across families.

### Macros (`macros/`)
- A `.lbm` file is a subgraph's inner `ProjectFile` plus metadata, stored in
  `~/.local/share/lightbeat/macros/<group>/.../<name>.lbm`. Instantiating one
  creates a locked Subgraph with remapped `NodeId`s.

### Hardware / IO
- **Audio Input** nodes read analyzer outputs produced on separate threads by
  `audio::manager`; configuration lives in `SetupFile::audio_inputs`.
- **Input Controller** nodes read a persistent virtual controller
  (`SetupFile::input_controllers`). Missing hardware just emits 0s and is
  reconnected by a ~1 Hz worker. Binding modes (Value / TriggerOnPress /
  TriggerOnRelease) are applied in the engine node.
- Every hardware node (Input Controller, Audio Input, Launchpad S, Push 1,
  X1) has a leading **`connected`** Logic output (port 0): high while the
  device has a live session. Build layouts with `io::status::with_status_port`.
- `ProjectFile.version` records port-layout changes that would repoint saved
  wires. When you insert or reorder ports on an existing node type, bump
  `PROJECT_VERSION` and extend `connections_for_current_layout` in
  `project.rs` (it runs per level, including inner subgraphs and macros).
- Missing hardware is looked for on setup load, on edits and on the
  status-bar **Rescan** button, never on a timer (USB/ALSA enumeration is
  slow and prints to stderr every time). Missing controllers/inputs show as
  `Waiting` and are listed in the status bar.
- Launchpad, Push 1 and X1 have dedicated nodes (`io/`); X1 code is in
  `input_controller/x1/`.

### Subgraph save/load
- `SavedNode` has an optional `inner_graph: Option<ProjectFile>` for recursive
  serialization. `load_graph` navigates into a subgraph, recursively loads its
  inner content, and navigates back. `send_load_data_recursive` walks the same
  path to dispatch `LoadData` commands as `SubgraphInnerCommand` for inner
  nodes.

---

## 3b. Engine tick and performance

- `EngineGraph::tick` order: take pending params/config (one lock per node) ->
  rebuild the routing plan if dirty -> `process()` all nodes -> run the plan
  (propagate connections) -> mirror state to UI **every 16th tick**
  (`UI_SYNC_EVERY_N_TICKS`) -> DMX manager tick (runs on the send cadence).
- **Routing is cached** (`engine/routing.rs`, `RoutePlan`): connection
  endpoints, port offsets and the connected-input bitmap are resolved once and
  reused. The plan is rebuilt when marked dirty (any engine command, any
  applied pending param/config/promoted list) or when a node's input/output
  *count* or the subgraph bridge's channel total no longer matches. A node
  that changes its ports on its own inside `process()` is only caught if the
  port count changes; if you add one that changes types at constant count and
  channel width, mark the plan dirty some other way. Resolving every wire
  every tick was ~165 of ~215 us/tick on the reference project.
- `save_data` is rebuilt every 16th tick only while the user is interacting
  (`engine::sync_hint::set_interacting`, driven by the undo window in
  `main.rs`) and every ~128 ticks otherwise. It is only read by save, copy,
  move-between-levels, macros and undo snapshots, so don't read it for live
  display; use `update_display`.
- `InnerGraph::tick` (subgraphs) must mirror `EngineGraph::tick`. It once
  lagged behind and was 98.6% of the profile (1740 -> 181 us/tick on the
  371-node reference project after fixing). Change both together.
- Because UI state is only fresh every 16 ticks, never rely on `shared.*`
  being current within a tick; engine logic must use the node's own fields.
- Undo snapshots run only within 2 s of real user input, at most 10 Hz.
- Inputs are written every tick via `write_input`, so a node never misses a
  value; pulses/edges must be latched inside the node if `process()` could
  be skipped.
- Status bar shows engine tick, UI frame, graph canvas share, fps and undo
  snapshot cost (click to reset peaks). `LIGHTBEAT_PROFILE_NODES=1` prints a
  per-node-type `process()` table every 2000 ticks, with a second table for
  nodes inside subgraphs (a subgraph's own time already includes them).
- `LIGHTBEAT_PERF_LOG=1` prints the status-bar gauges once a second to
  stderr, plus eframe's total CPU per frame, the real fps and the CPU share of
  the UI thread and the process (read from /proc). Use it instead of reading
  the UI. `perf::PhaseClock` adds per-phase rows to the node profile.
- **UI cost** is dominated by egui tessellating and painting the node canvas
  (~3.5 ms of ~4.5 ms per frame on the reference project), not by our
  `update()` (~1.2 ms). Levers: `AppConfig::idle_repaint_hz` (default 30; full
  rate for 2 s after any input), `AppConfig::msaa_samples` (>0 turns egui's
  feathering off and lets the GPU smooth edges; ~a third less UI CPU), fewer
  shapes (wires are one `Shape::line` each and culled when off screen).
  `request_repaint_after` subtracts egui's predicted frame time, so add it
  back or the request collapses to the next vsync.
- When measuring the UI thread, make sure the window is actually being drawn:
  an occluded Wayland window gets no frame callbacks and reads ~0% CPU.
  Compare old and new builds alternately; CPU % also moves with frequency
  scaling.
- Measure with `--release` (debug is ~4.5x slower). Autoload uses
  `config.recent_projects.first()` from `settings.json` in the **working
  directory**, not `project.json`; check it before trusting a measurement.

---

## 3c. Promoted params (params as inputs)

- Any param can be exposed as an input port from the inspector (checkbox
  `in` next to the control). The node knows nothing about it: the graph adds
  the ports after the node's own inputs and turns each incoming value into a
  `set_param` (`engine/promoted.rs`, used by both `EngineGraph::tick` and
  `InnerGraph::tick`). Float/Int take the raw value clamped to the param's
  range, Bool switches at 0.5, Choice spreads a 0..1 signal over its options.
- The list lives in `NodeSharedState::promoted` (UI-owned, saved as
  `SavedNode.promoted`); changes reach the engine via `pending_promoted`.
  `NodeWidget::ui_inputs_full()` is `ui_inputs()` plus those ports; the graph
  must call it, never `ui_inputs()`, when dealing with ports.
- Toggling remaps wires on the later promoted ports
  (`remap_promoted_connections`). Connect/disconnect hooks are not called for
  promoted ports, since nodes only know their own inputs.
- A node opts params out with `ProcessNode::non_promotable_params()`: ones
  that reshape its ports or buffers (modes, types, slot counts) and ones
  already served by a native input port. New nodes with such params must list
  them.

---

## 3d. Beat tracking (`audio/tracker/`, `beat_bench/`)

- `BeatTracker` turns audio into tempo, a free-running beat clock and a bar
  position. Stages: `frontend` (4-band + clap/snare onset strength, 2048-pt FFT,
  hop 256), `tempo` (autocorrelation scored per candidate tempo, followed by a
  forward filter, so it moves smoothly like a record's pitch), `phase` (a
  clock nudged by a histogram of onsets over the last ~6 beats), `bar` (which
  of 16 sixteenth steps is the bar start, from kick/bass/clap/hat templates, the
  strongest kick, and `novelty`: where the spectrum changes). The clock runs
  through breakdowns; the bar choice is sticky once clearly ahead.
- The `BeatClock` analyzer exposes it: ports `beat` (trigger), `beat phase`,
  `bar phase`, `16th phase`, `step` (0..15), `bpm`, `confidence`. Params
  `min_bpm`/`max_bpm` (a range under one octave is what decides 87 vs 174
  BPM), `bar_shift` (sixteenths, to fix the bar start by ear) and `lead_ms`.
- Outputs update once per audio chunk (~10-20 ms), like every analyzer.
- **Test bench**: `lightbeat bench generate [dir] [name]` writes synthetic
  dance music with an exact beat/bar grid (`beat_bench_data/`, gitignored);
  `lightbeat bench score [dir] [name]` scores the tracker with no window. `BENCH_BAR_DEBUG=1` prints diagnostics. Run it
  after any change to `audio/tracker/` and read the per-scenario table; the
  `cargo test` cases only guard the basics. Synthetic audio is easier than
  records, so real clips should be added as `name.wav` + `name.json`.
- Known limits (see the scores): the bar start of a pure kick+hat loop is
  undecidable until something else happens; half-time, heavy swing and
  syncopated patterns lock a sub-beat off; drum and bass needs a 150-190 BPM
  range.

---

## 4. Adding a new node — checklist

1. **Engine file**: `src/engine/nodes/<category>/<snake_name>.rs` —
   define struct + `impl ProcessNode`. If it has runtime state to persist,
   implement `save_data()` and `load_data()`.
2. **Widget file**: `src/widgets/nodes/<category>/<snake_name>.rs` —
   define struct + `impl NodeWidget`. Set `description()`. If the node has
   widget-only state (selection, layer list), restore it in `project.rs` after
   `load_data` runs (see how Group/Subgraph/EffectStack do it).
3. Register both in their respective `mod.rs` files.
4. In `main.rs`:
   - Add the `use` import.
   - Register the widget factory: `self.graph.register_node("Category", "Display Name", |id| Box::new(MyWidget::new(id, new_shared_state(num_inputs, num_outputs))));`
   - Add the engine arm in `wire_new_nodes`: `"Display Name" => Some(Box::new(MyProcessNode::new(id))),`
5. The **first input/output channel counts** in `new_shared_state(in, out)`
   must match the engine's `total_channels(inputs)` and `total_channels(outputs)`.
   For nodes with dynamic ports (e.g. mode switching), pass a generous max.

---

## 5. Build / test

- `cargo build 2>&1 | grep "^error"` — fast iteration check.
- `cargo run` — launches the app. Default project is `project.json` in the
  working directory; setup is `setup.json`. Both auto-load if `autoload_on_open`
  is set in `~/.config/lightbeat/...` (managed by `config.rs`).
- There are some unit tests in `objects/`, `color/` and elsewhere (~46);
  `cargo test` runs them. The crate uses edition 2024, so it needs a recent
  stable toolchain (`rustup update stable`).
- We use **egui-phosphor** for icon glyphs (registered in `main.rs::new`).
  Use `egui_phosphor::regular::ARROW_UP`, `ARROW_DOWN`, `X`, `CARET_RIGHT`,
  `ARROW_SQUARE_OUT`, etc. for any icon-style buttons. The default egui font
  doesn't include arrows/X glyphs.

---

## 6. Conventions / gotchas

- **No emojis in code or files** unless the user asks for them.
- **No inline colors in widgets** — use `theme.rs` constants.
- **Comments** describe *why*, not what. Don't narrate steps. Don't add
  comments that just restate the next line.
- **No backward-compat aliases** unless asked — the user is in active
  development and prefers clean code over compat shims.
- **Don't introduce new port types lightly** — the user prefers reusing
  `Any` and inferring modes from connections over adding type variants.
- **`color::*` is the canonical color toolkit** — always reuse `Rgb`, `Hsv`,
  `BlendMode`, `Gradient`, `rgb_to_rgbw` instead of writing new color math.
- **Engine never reads `save_data` from shared state** — `pending_config` is
  the channel for UI→engine config. They were once fused and caused subtle
  bugs.
- **Multi-channel inputs**: when adding a node with `Color`/`Palette` ports,
  remember `write_input(channel, v)` is per-float, not per-port. Use
  `port_base_index(ports, port_idx)` to find where a logical port starts.
- **Cross-level engine commands** in subgraphs go through `push_engine_cmd`
  which auto-wraps in `SubgraphInnerCommand` based on the active level.
- **Project save** always uses the *root* level
  (`project::save_graph` calls `graph.root_level()`), so save can happen
  while the user is navigated inside a subgraph.
- **`cleanup_stale_connections`** runs every frame and now drops
  type-incompatible wires too. This is what makes mode switching "feel right".

---

## 7. Where the user is in their journey

LightBeat is a personal project for live lighting control. The user has a
record-shelf installation with **6 LED strips** (top/middle/bottom × left/right),
each 158 RGB or GRBWW LEDs, where LED 0 is at the centre — hence the
`StripLayout` with reversible logical positions. They drive these via 6 ArtNet
universes. They iteratively grow the app: add nodes when needed, refine UX
when something is awkward. Recent themes: stricter terminology (palette
everywhere), auto-detection over explicit config, Effect Stack as the
composition primitive, engine/UI performance (measured, with numbers in
commit messages).

When in doubt about a design choice, prefer **flexibility presented in an
easy-to-understand way** — the user's stated guiding principle.
