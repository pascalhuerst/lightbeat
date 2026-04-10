# LightBeat

A beat-synchronized, node-based signal processing tool built with Rust and egui. LightBeat connects to Ableton Link to receive tempo and transport information, then routes signals through a visual node graph to drive light control and other beat-reactive outputs.

## Building

```sh
cargo build --release
```

Requires a C++ compiler for the Ableton Link dependency (cmake).

## Running

```sh
cargo run
```

Or run the release binary directly — make sure `settings.json` is in the working directory.

## Concepts

### Signal Types

Inspired by Bitwig's Grid, signals are typed by color:

| Type | Color | Description |
|------|-------|-------------|
| **Logic** | Yellow | Bistate signal. >=0.5 = high, <0.5 = low. Reacts to transitions. |
| **Phase** | Purple | Unipolar 0..1, wrapping. Drives sequencer position and timing. |
| **Untyped** | Red | Generic signal, any range. |
| **Any** | Grey | Accepts any signal type (used by monitoring nodes like Scope). |

Only matching types can be connected. Any-type inputs accept all types and adopt the connected signal's color.

### Nodes

- **Clock** — Ableton Link source. Outputs: beat (Logic), play (Logic), phase (Phase). Shows BPM, peer count, play state, and a beat flash LED (brighter on the downbeat).
- **Phase Scaler** — Scales phase timing by powers of 2. Buttons for quick x2/div2. Division stays perfectly in sync with the input (zero crossings align).
- **Step Sequencer** — Phase-driven sequencer with adjustable step count (1-64). Click/drag faders to set values. Outputs: trigger (Logic) on step change, value (Untyped) for current step. Resizable.
- **Scope** — Dual-input oscilloscope. Accepts any signal type. Auto-scales Y range based on connected signal. Input port fills with the waveform color (green/orange) and outlines with the signal type color. Resizable. Larger waveform view available in the inspector.

### Node Graph

- **Right-click** canvas to add nodes
- **Click** a node to select, **Ctrl+Click** to toggle multi-select
- **Drag** on empty canvas to draw a selection rectangle
- **Drag** title bar to move nodes (moves all selected)
- **Drag** from output port to input port to connect (magnetic snap)
- **Click** a connected input port to disconnect (starts re-dragging from the output end)
- **Drag** bottom-right corner of resizable nodes to resize
- **Middle mouse** drag to pan the canvas

### Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| **Delete** / **Backspace** | Delete selected nodes |
| **Ctrl+D** | Duplicate selected nodes with settings |
| **Ctrl+C** | Copy selected nodes |
| **Ctrl+V** | Paste at mouse position |
| **Ctrl+S** | Save project |

### Inspector

The right panel shows details for the selected node: input/output ports with live values, editable parameters, and custom displays (e.g. scope waveform). When multiple nodes are selected, common parameters can be edited in bulk.

## Files

- `settings.json` — Application settings (loaded from working directory)
- `project.json` — Saved node graph (auto-saved on close, auto-loaded on open by default)

### Settings

```json
{
  "autosave_on_close": true,
  "autoload_on_open": true,
  "snap_to_grid": false
}
```

## Project Structure

```
src/
  main.rs              App entry point, eframe setup
  config.rs            Settings file (settings.json)
  project.rs           Project save/load (project.json)
  beat_clock.rs        Beat clock thread, Link polling, BeatListener trait
  link_controller.rs   Ableton Link wrapper with callbacks
  widgets/
    mod.rs             Widget module exports
    clock.rs           Clock node (Ableton Link source)
    phase_scaler.rs    Phase scaler node (power-of-2 timing)
    step_sequencer.rs  Step sequencer node
    scope.rs           Oscilloscope node
    inspector.rs       Inspector panel rendering
    nodes/
      mod.rs           Node system exports
      types.rs         PortType, PortDef, NodeId, Connection
      node.rs          NodeWidget trait, ParamDef, NodeState
      graph.rs         Node graph editor (rendering, interaction, signal propagation)
```
