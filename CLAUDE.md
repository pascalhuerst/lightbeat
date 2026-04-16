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

Multi-channel ports (`Color`=3, `Position`=2, `Palette`=12) are handled
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
│   │   ├── io/                   # category (matches the menu categories).
│   │   ├── transport/            # clock, lfo, phase scaler, transition, etc.
│   │   ├── math/                 # constants, oscillators, color/palette ops.
│   │   ├── display/              # scope, value/color display.
│   │   ├── meta/                 # subgraph (with InnerGraph + bridge nodes).
│   │   ├── output/               # group output, effect_stack.
│   │   └── ui/                   # button.
│   └── patterns/                 # NEW: pattern impls used by the Effect Stack.
│       ├── mod.rs                # Pattern trait, factory, Pixel struct.
│       ├── bar.rs / solid.rs     # First two patterns. Add new ones here.
├── widgets/
│   ├── inspector.rs              # show_inspector(node) — generic params/ports view.
│   ├── dmx_monitor.rs            # Per-universe channel grid w/ test universes.
│   ├── fixture_list.rs           # Fixture template editor (channels incl. LedStrip).
│   ├── object_list.rs            # Fixture instances (address, interface).
│   ├── group_list.rs             # Groups + per-strip layout + member reorder.
│   ├── color_palette_list.rs     # Palettes (a palette = 4 colors).
│   ├── color_palette_group_list.rs
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
├── beat_clock.rs                 # Ableton Link wrapper, BeatPattern subscriptions.
├── dmx_io.rs                     # DmxOutputManager (engine side), shared state,
│                                 # per-channel Override mechanism, test_universes.
├── project.rs                    # Save/load NodeGraph (root level + recursive
│                                 # inner_graph for subgraphs). Restores widget
│                                 # state for SubgraphWidget / GroupWidget /
│                                 # EffectStackWidget after engine load_data.
├── setup.rs                      # SetupFile (fixtures, objects, interfaces,
│                                 # groups, palettes) — separate from project.
└── config.rs                     # AppConfig (autoload, snap_to_grid, etc.).
```

---

## 3. Concepts that are hard to infer from code

### Port types (`engine::types::PortType`)
- `Logic` (yellow), `Phase` (purple), `Untyped` (red), `Any` (gray, accepts
  anything), `Color` (cyan, 3 channels), `Position` (greenish, 2 channels),
  `Palette` (warm white, 12 channels = 4×RGB).
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

### Effect Stack (`engine/nodes/output/effect_stack.rs`)
- One node per output target group(s). Inspector holds an ordered list of
  layers (`Vec<EffectLayerConfig { pattern_type, blend, opacity }>`). Each
  layer renders into a per-strip `[Pixel { color: Rgb, alpha: f32 }]`
  buffer; the stack composites bottom-to-top using
  `BlendMode::blend(base, effect, opacity * alpha)` from `color::blend`.
- Layer inputs are exposed as **prefixed ports** on the Effect Stack node:
  `L1.position`, `L1.width`, `L1.color`, `L2.color`, … New layers grow new
  ports. The widget rebuilds its `ui_inputs` from the layer list each call.
- Patterns implement the `Pattern` trait in `engine::patterns`. To add a new
  pattern: drop a file in `engine/patterns/`, register it in
  `create_pattern()` and `all_pattern_types()`. Each pattern declares its
  `input_ports()` and a `render(inputs, frames)` function.

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

### Subgraph save/load
- `SavedNode` has an optional `inner_graph: Option<ProjectFile>` for recursive
  serialization. `load_graph` navigates into a subgraph, recursively loads its
  inner content, and navigates back. `send_load_data_recursive` walks the same
  path to dispatch `LoadData` commands as `SubgraphInnerCommand` for inner
  nodes.

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
- There are some unit tests in `objects/channel.rs`, `objects/fixture.rs`,
  `objects/object.rs`, `color/blend.rs`, `color/gradient.rs`. `cargo test` runs them.
- We use **egui-phosphor** for icon glyphs (registered in `main.rs::new`).
  Use `egui_phosphor::regular::ARROW_UP`, `ARROW_DOWN`, `X`, `CARET_RIGHT`,
  `ARROW_SQUARE_OUT`, etc. for any icon-style buttons. The default egui font
  doesn't include arrows/X glyphs.

---

## 6. Conventions / gotchas

- **No emojis in code or files** unless the user asks for them.
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
composition primitive.

When in doubt about a design choice, prefer **flexibility presented in an
easy-to-understand way** — the user's stated guiding principle.
