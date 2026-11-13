# UI/UX streamlining

Inconsistencies found by reading the code, with what to change. Nothing here
removes a function; most items replace several ways of doing one thing with one.

- Paths are relative to `src/`; `W/` is `widgets/`, `N/` is `widgets/nodes/`,
  `E/` is `engine/nodes/`.
- Everything was read in the source, nothing was tried in the running app.
  Items I could not fully confirm say "unverified".
- **Priority**: P1 = can lose work or shows wrong data, P2 = noticeable
  friction, P3 = polish.
- Renaming or reordering ports of existing nodes repoints saved wires: those
  items are marked **[wires]** and need a `PROJECT_VERSION` bump (or, by your
  rule of no users yet, a re-save of the test projects).

The catalogue has 93 node types in 11 menu categories; the shell is a menu bar,
an "Add Nodes" panel on the left, the canvas, an inspector on the right with a
snapshot panel under it, a status bar, and two floating windows (Setup with 10
tabs, DMX Monitor).

---

## 1. Things that can lose work or show stale data (P1)

| # | What | Evidence | Change |
|---|---|---|---|
| 1.1 | **New, Open and Open Recent discard unsaved work without asking.** Only Quit and closing the window check `is_project_dirty`. | `main.rs:1563`, `1534`; checks only at `2184`, `2837`, `2844` | Send all four through the existing Save / Discard / Cancel dialog. |
| 1.2 | **File > Save and Ctrl+S do different things.** The menu saves the project; Ctrl+S saves project and setup. The setup is otherwise written only on exit and on import, so a crash loses setup edits. | `main.rs:2255-2258` vs `2792-2795`; `2949`, `1405` | Make both the same, and save the setup shortly after each change (debounced). |
| 1.3 | **Save on an untitled project writes `project.json` silently** while the title says "untitled". | `main.rs:1133-1137`, `2062`, `2872` | Fall through to Save As when there is no path. |
| 1.4 | **Two windows are both titled "Setup"** (the Setup window and the import/export notice). egui takes the window id from the title, so they collide when both are open, which is the normal case after an import. | `main.rs:2196`, `2369` | Title the notice "Import Setup" / "Export Setup". |
| 1.5 | **Palette edits in Setup do not reach Palette Select nodes.** Palettes are synced at start and on setup load only; gradients sync every frame, groups/objects/interfaces by dirty flag. | `main.rs:445`, `1647` vs `2122`, `2768-2784`; `N/math/palette_select.rs:61`, `209` | One sync strategy for all setup data. (The node-side refresh was not traced to the end: unverified how stale it gets.) |
| 1.6 | **Renaming a group does not reach the pickers** until some other group edit: the name field is the only edit that does not set `needs_refresh`. | `W/group_list.rs:49` vs `:80`, `:84`, `:96` | Set the flag (falls out of 1.5). |
| 1.7 | **Shortcuts test `modifiers.ctrl` only**, so Cmd+S/O/Q/Z would not work on the Mac build (unverified at run time); and there is no check for a focused text field, so Ctrl+Z while typing also undoes in the project. | `main.rs:2792-2812`; the DMX monitor does it right at `W/dmx_monitor.rs:158` | Use `modifiers.command`; skip while a text field has focus. |
| 1.8 | **File errors go to the terminal.** Setup import/export show a window; project save/open, setup save, macro load/delete and autosave only `eprintln!`. | `main.rs:1399`, `1429` vs `1139`, `1438`, `1554`, `1106`, `1807`, `1998`, `2924` | One notice (toast or status-bar line) for every file result. |
| 1.9 | **"Delete from library" removes a macro file for good, with no confirmation**, and is only in a right-click menu. | `main.rs:1791-1795`, `1805` | Confirm; it is the one delete that no undo covers. |
| 1.10 | **Removing a port from the middle of a list keeps the wires only on three nodes.** MIDI Input, MIDI Output and DMX Input implement `take_removed_port`; Subgraph ports, Portal Send ports and Lookup columns remove with a plain `Vec::remove`. | `N/io/midi_input.rs:183`, `midi_output.rs:95`, `dmx_input.rs:288` vs `N/meta/subgraph.rs:257`, `portal.rs:138` | Use the same hook everywhere (unverified whether the wires after the removed port really shift on those nodes; it should be tried). |

---

## 2. Undo and delete (P2)

- **2.1 Ctrl+Z chooses its undo stack by where the pointer is** (setup only while
  the pointer is over the Setup window), and the Edit menu has four entries (Undo/
  Redo Project, Undo/Redo Setup). The Import Setup tooltip says "Edit, Undo".
  `main.rs:2378`, `2816`, `2279-2295`, `2268`. -> One Undo/Redo pair that acts on
  whatever was changed last, with the label naming it ("Undo: rename group").
- **2.2 Delete never confirms and never tells what depends on the thing.**
  Deleting a fixture template leaves objects showing "???" (`W/object_list.rs:91`),
  a deleted object stays in groups as a silently skipped row (`W/group_list.rs:63`),
  a deleted interface is "None" in Objects but "???" in the monitor
  (`W/object_list.rs:126`, `W/dmx_monitor.rs:74`). -> One placeholder for
  "missing", and a "used by N objects" note next to the delete button.
- **2.3 "Reset to factory preset"** replaces all of a controller's inputs without
  asking (`W/input_controller_list.rs:177`). Setup undo probably brings them back;
  a confirmation is still cheap.

---

## 3. Setup window: ten tabs, three designs (P2)

- **3.1 Three list layouts.** Tables with an inline name (Interfaces, Objects,
  Color Palettes, Gradients); collapsing headers with a "Name:" field inside,
  closed by default in Fixtures/Groups/Palette Groups and open by default in
  Audio Inputs/Input Controllers; flat blocks with a status dot (DMX Inputs).
  `W/interface_list.rs:67`, `W/fixture_list.rs:39`, `W/audio_input_list.rs:45`,
  `W/dmx_input_list.rs:67`. -> One pattern: a list on the left, the selected
  item's details on the right; or at least one shared header (name, status,
  enabled, duplicate, delete).
- **3.2 Delete is an X in the row in five tabs and a text button at the bottom
  of the body in five others** ("Delete template", "Delete group", "Delete
  input", "Delete controller"); some X have a tooltip, some not; "Remove" and
  "Delete" are both used. `W/object_list.rs:148`, `W/fixture_list.rs:145`,
  `W/group_list.rs:195`, `W/dmx_input_list.rs:76`. -> X with tooltip, in the header.
- **3.3 Seven styles of Add button**: "Add Group", "Add Empty Preset", "+ Add
  Audio Input", "+ Art-Net", phosphor plus with "Add universe", a combo box used
  as a button ("+ Add analyzer"), and "Create" for objects.
  `W/group_list.rs:209`, `W/gradient_preset_list.rs:79`, `W/audio_input_list.rs:205`,
  `:181`, `W/interface_list.rs:122`, `W/dmx_input_list.rs:144`, `W/object_list.rs:210`.
  -> Phosphor plus and sentence case everywhere ("Add group").
- **3.4 No Duplicate anywhere in Setup** (and none for snapshots). Objects have a
  batch creator, the only bulk tool. -> Duplicate in the shared header.
- **3.5 Reorder exists for group members only.** Palettes in a palette group
  cannot be reordered although Palette Select picks them by index
  (`W/color_palette_group_list.rs:50-73` vs `W/group_list.rs:66-71`). -> A shared
  reorderable row, palette groups first.
- **3.6 Fixture channels are the least editable list in the app**: only "Remove
  last", no rename, no reorder, and no button for kinds the code knows (CMY, H/S,
  16-bit pan/tilt, Raw). `W/fixture_list.rs:138`, `:58`, `:114-134`, `:169-181`.
  -> X and arrows per channel, one "Add channel" dropdown with all kinds.
- **3.7 Adding a member is one button per candidate** in Groups and Palette
  Groups, a dropdown for analyzers. `W/group_list.rs:91`, `W/audio_input_list.rs:180`.
  -> Dropdown everywhere; a row of buttons does not scale to 40 objects.
- **3.8 "Enabled" is an unlabelled "On" column for interfaces, a checkbox for
  audio and DMX inputs, and missing for input controllers.**
  `W/interface_list.rs:76`, `W/audio_input_list.rs:54`, `W/dmx_input_list.rs:75`.
- **3.9 Connection status looks different in every tab**: coloured words
  (Connected / Waiting / No mapping / Disabled), other words for the X1 ("Waiting
  for device" / "Disconnected"), a painted dot for DMX inputs, "● live / ○ test"
  in the monitor; three different greens. `W/audio_input_list.rs:96`,
  `W/input_controller_list.rs:107`, `:118`, `W/dmx_input_list.rs:38`,
  `W/dmx_monitor.rs:107`. -> One `status_badge` with one vocabulary.
- **3.10 Rescan has four names and three scopes**: "Rescan" in the status bar
  (everything), "Rescan" in Input Controllers (controllers only), "Refresh
  devices" in Audio Inputs, a refresh icon for macros; the warning is "Not
  found:" in one place and "Hardware not found:" in another, and the Audio Inputs
  tab has none. `main.rs:2629`, `2626`, `W/input_controller_list.rs:18-21`,
  `W/audio_input_list.rs:10`. -> One Rescan that does everything, the same banner
  in each hardware tab.
- **3.11 A DMX address is entered three ways**: columns Ch, Univ, Sub, Net in
  Objects; net, sub, uni fields plus "= port address" in DMX Inputs; and the DMX
  Monitor only knows universe 0..15 with net and subnet forced to 0, so it cannot
  show an object patched elsewhere. `W/object_list.rs:69-72`,
  `W/dmx_input_list.rs:108`, `W/dmx_monitor.rs:87-102`. -> One address widget
  (net.sub.universe) in all three.
- **3.12 Gradient presets cannot be edited where they are listed.** The tab only
  renames, deletes and adds an empty one; editing means loading the preset into a
  Gradient Source node and saving it, which always creates a *new* preset.
  Palettes are edited in their tab. `W/gradient_preset_list.rs:53-82`,
  `N/math/gradient_source.rs:381`, `main.rs:877-885`. -> A stop editor in the tab
  and "Update preset" on the node.
- **3.13 Learn**: a "Learn" / "Stop Learning" toggle per controller, and a raw
  "⟳" character (not a phosphor icon) per row, with different hint texts.
  `W/input_controller_list.rs:163`, `:499`.
- **3.14 Nested scrolling**: each tab has its own vertical `ScrollArea` inside
  the window's `ScrollArea::both`. `W/fixture_list.rs:32`, `main.rs:1820`.

---

## 4. Menus, windows, settings (P2/P3)

- **4.1 Setup lives alone in a "Window" menu, DMX Monitor in "View"**; both are
  floating windows. `main.rs:2301`, `2320`. -> Both in View (or both in Window).
- **4.2 No shortcut is shown in any menu** (Ctrl+S/O/Q/Z exist), and the
  canvas keys (`F`, `M`, `H`, Space, Ctrl+D) are only in tooltips or nowhere.
  -> Shortcut texts in the menus and a Help > Keyboard Shortcuts window.
- **4.3 Several settings have no control at all**: `snap_to_grid`,
  `autosave_enabled`, `autoload_on_open`, `dmx_bypass_on_startup`,
  `idle_repaint_hz`, `msaa_samples` are only read (`config.rs:28-63`). -> A small
  Preferences window; "Snap to grid" in View.
- **4.4 Update settings are in the Help menu and again in the Software Update
  window, and behave differently** (changing the channel in the window checks at
  once, in the menu it does not). `main.rs:2336-2349` vs `1336-1351`. -> Keep
  them in the window.
- **4.5 Panel visibility is remembered in three ways**: Add Nodes per project
  (still called `show_library`), the inspector in `settings.json`, the DMX
  Monitor, Setup window and its tab not at all. `main.rs:1200`, `2316`, `406`.
  -> All in `settings.json`.
- **4.6 Two node finders with different rules.** The Add Nodes panel matches
  substrings and is drag-only (a click does nothing); quick add matches paths and
  adds on Enter. `main.rs:1686-1690`, `1745-1754`. -> Same matcher; click or Enter
  adds in the panel too. See also 5.1.
- **4.7 The DMX Monitor's main gestures are written nowhere** (Ctrl+drag =
  override, Ctrl+double-click = clear, Shift = fine). `W/dmx_monitor.rs:161-205`.
  The group "Space" editor does print its hints. -> One hint line.
- **4.8 The multi-selection inspector is a reduced copy of the single one**:
  no dropdown params, no "in" checkbox, raw param names, no reset on
  checkboxes. `W/inspector.rs:423-494` vs `253-383`. -> One row function for both.
- **4.9 "Multi-frame editing isn't supported yet."** is shown to the user
  (`main.rs:2539`). -> Edit colour for all selected frames, or say nothing.

---

## 5. The node catalogue (P2)

- **5.1 Two taxonomies.** The menu has 11 categories, quick add has 14 groups,
  and about 30 nodes are in different places in the two. "Math" holds routing
  (Port Hub, Multiplexer), triggers (Counter, Schmitt, the banks), sources
  (Const, Random), position and clock nodes; "Route" has exactly one entry;
  Color Display is under Color and Peak Level Meter under UI although both are
  `display/...` in quick add. `main.rs:484-847`, `N/paths.rs:26-112`. -> Register
  nodes under the quick-add groups and delete most of the explicit path table.
  One mental map instead of two.
- **5.2 Names that break "X to Y" or say the wrong thing.**
  - Position Merge / Position Split -> "Pan Tilt to Position" / "Position to Pan
    Tilt" (and lowercase ports; they are the only capitalised ones,
    `E/math/position_ops.rs:17`).
  - "Latch" is a one-tick delay and does not latch (`E/transport/latch.rs:3`)
    -> "Tick Delay".
  - "Flip-Flop" in the menu, "SR FF" on the node, "SR latch" in the description
    (`N/math/flipflop.rs:29-42`) -> "SR Flip-Flop".
  - "Clock" is the Link clock (`clock/link`, "Link Clock" in CLAUDE.md) -> "Link
    Clock".
  - "Scaler" (`math/scaler`) next to "Scale Offset" (`math/scale`); the Scaler's
    description says "to an output range" but it always maps to 0..1; its ports
    are "min"/"max" and its params "In Min"/"In Max"
    (`E/math/scaler.rs:35`, `:99`, `N/math/scaler.rs:22`) -> "Range to Unit" or
    similar, same words on ports and params.
  - "Fill" is a Loop without the suffix, and Fill and Bar Loop take no phase
    although a Loop is "a function of a phase" (`E/../effects/fill.rs:22`,
    `bar.rs:55`) -> a third kind, e.g. plain "Fill" and "Bar".
- **5.3 Overlap worth merging** (the functions stay, the number of things to
  know shrinks):
  - Gain, Scale Offset, Bipolar, Scaler and Mul are five nodes around
    `in * a + b`. -> One "Scale" node with presets, or keep them and make the
    names tell them apart.
  - Sin / Cos are the LFO in "Phase input" mode with one waveform
    (`E/math/oscillator.rs:40`, `E/transport/lfo.rs:234`). -> Drop them or offer
    them as LFO presets in quick add.
  - Gradient Stops exists only to feed Palette to Gradient's four positions
    (`E/ui/gradient_stops.rs:34`, `E/math/palette_to_gradient.rs:39`). -> Put the
    four handles into Palette to Gradient's body.
  - Toggle Bank = N flip-flops, Trigger Bank = N threshold pulses, next to
    Flip-Flop, Schmitt Trigger and Change Detect. -> A "Channels" count on the
    single nodes (or buses, see `ui_ux_lookout_and_ideas.md`).
  - Level Meter and Gauge, Add/Sub/Mul/Div, the four compares, AND/OR/XOR and
    the three Consts are separate entries over one widget each, while Color
    Modifier and Envelope use a dropdown for the same situation. -> Keep the
    separate quick-add entries (they are fast) but let the inspector switch the
    operation, so a `>=` can become `<=` without rewiring.
- **5.4 Operator nodes show their name twice** in the inspector (heading and
  type line), because they return the operator from `title()` as well
  (`N/math/compare.rs:32`, `math_op.rs:55`, `logic_gate.rs:44`,
  `oscillator.rs:35`; `W/inspector.rs:202-210`).
- **5.5 Stale descriptions**: Gradient Source still mentions "Group Output"
  (`N/math/gradient_source.rs:160`); Launchpad says LEDs "drive red brightness"
  (`N/io/launchpad.rs:65`, the engine does red and green); Const says "any port
  type, edited via the inspector" (`N/math/constant.rs:49`); Step Sequencer and
  Demultiplexer omit their trigger/phase modes. -> One pass over `description()`.

---

## 6. Type and mode conventions (P2)

- **6.1 Port Hub offers "Auto" but never detects**: it has no `on_ui_connect`
  hooks, unlike the Multiplexer next to it (`N/math/port_hub.rs:60-101` vs
  `N/math/multiplex.rs:95-107`). -> Add the two hooks.
- **6.2 Sample & Hold, Latch and Smoothing are fixed to Untyped**, though holding
  or delaying a Color is a natural use (`E/transport/sample_hold.rs:24`,
  `latch.rs:24`, `smoothing.rs:56`); Transition has no Position or Gradient mode
  (`transition.rs:210`). -> `Any` plus inference, as in the canonical pattern.
- **6.3 The type setting is called "Mode" on some nodes and "Type" on others**,
  and the inspector special-cases the literal name "Mode"
  (`W/inspector.rs:156`; `color_components.rs:154` vs `multiplex.rs:179`). A
  plain number is called "Untyped", "Float" or "Number" depending on the node
  (`multiplex.rs:31`, `transition.rs:210`, `N/math/lookup.rs:41`). -> "Type" with
  Auto / Number / Logic / Phase / Color ... everywhere.
- **6.4 "Index or phase" is expressed in four ways**: "Select Mode" (Counter,
  Mux/Demux, Palette Select), "Input" (Lookup), "Mode" (Step Sequencer), a
  checkbox "Phase input" (LFO); the stack's `scene` is index only. -> One choice
  with one name, or better: infer it from a Phase wire, which is the convention
  already.
- **6.5 Two mechanisms for "a wire overrides this setting"**: hand-built ports
  with `overridden_param_indices` (Scaler, Scale Offset, Bipolar, Counter, Phase
  Scaler, Palette to Gradient) and promoted params (everything else). -> New
  nodes use promotion only; the old ones could become pre-promoted params
  **[wires]**.
- **6.6 Settings with a native input that can also be promoted**, giving two
  ports for one value: Trigger Hold "Duration" (which also picks the input by
  "value > 0" rather than by being wired, `E/transport/hold.rs:42-48`) and
  Internal Clock "BPM". -> List them in `non_promotable_params`.

---

## 7. Ports and params: one word per thing (P3, mostly [wires]-free renames)

Port *names* are not part of saved wires (wires go by index), so these are safe.

| Concept | Names in use | Suggest |
|---|---|---|
| Edge input | "trigger", "sync" (LFO), "clk" (JK), "sample on" (Change Detect), "trig" (Push), "T1.." (Toggle Bank) | "trigger"; "gate" only for levels; "reset" |
| Main input / output | "in", "value", "x", "signal", "In 1" / "out", "value", "y", "envelope" | "in" / "out" on single-signal nodes |
| Gradient input of effects | "color" (all five effects) | "gradient", as everywhere else |
| Numbered ports | "in0" (Mux), "in1" (Port Hub), "in 1" (Scope, Portal), "in 0" (Subgraph), "r,c" from 0 (groups); Palette Select counts from 0 in the inspector and from 1 on the node | from 1, with a space |
| 0..1 scaler | "Intensity" (effects), "amount" (stack, Color Modifier), "opacity" (layers), "Gain", "amp" | "amount" |
| Burst lifetime | "Decay" (Ripple), "Lifetime" (Spark) | "Lifetime" |
| Time | seconds (Envelope, Time to Phase, effects), ms (Random Glide), **engine ticks without a unit** (Trigger Hold 1..100000, Smoothing Window), "x" (Scope) | seconds; ticks are an implementation detail |
| Name field | "Label:" (Button) vs "Name:" (Fader, XY Pad, Value Display, stack); "Enable input" vs "Input enabled" | "Name:" |

Further:

- **7.1 Port order [wires]**: trigger first on Transition, Envelope, Trigger
  Hold; value first on Sample & Hold and Change Detect. -> Signal first, trigger
  after, on all.
- **7.2 Same concept, different port type**: `play` is Logic on the Link clock
  and Untyped on the Internal Clock; Toggle Bank outputs are Untyped while
  Flip-Flop's Q is Logic; LED inputs are Color on the Launchpad, Untyped on X1
  and Push. -> Logic for states, Color for LEDs that have colour.
- **7.3 The Link clock has no `bpm` and no status output**; the audio Beat Clock
  has `bpm`, and every hardware node has `connected`. -> `bpm` and `peers` on
  the Link clock.
- **7.4 Button and Fader have no params at all** (mode, bipolar, orientation live
  in the config), so those settings cannot be promoted, stored in snapshots, reset
  by double-click or edited for several nodes at once. -> Make the simple ones
  params.

---

## 8. Inspectors and lists on nodes (P2/P3)

- **8.1 The same setting twice in one inspector**: the generic section lists
  every param and the node's own section draws some again. Port Hub (Type,
  Slots), Multiplexer and Demultiplexer (Select), Colors to Gradient / Gradient
  to Colors (Stops). `N/math/port_hub.rs:90-118`, `multiplex.rs:123-141`,
  `color_reshape.rs:61-73`. -> Delete the custom copies.
- **8.2 Editing a list of ports or entries works differently on every node.**
  Add is a phosphor plus (MIDI, DMX), "+ Add Column" (Lookup), a small "+ Add
  Input" (Subgraph, Portal), a bare plus in the node body (Clock Divider, Phase
  Scaler), one button per candidate (Palette Select), a checkbox per stop
  (Gradient Source). Remove is an X with or without tooltip, or "last entry
  only". Reorder is arrows (Palette Select, Gradient Source), a drag handle
  (Effect Stack), or missing (MIDI Input, Lookup columns, Subgraph, Portal).
  -> One shared list editor: drag handle, name, type, X, "Add ..." at the bottom.
  Combined with 1.10 this also fixes the wire handling in one place.
- **8.3 The number of variadic ports is set in four ways**: dragging onto a "+"
  port (math, logic), an Int setting ("Slots", "Channels", "Stops"), a "+" button
  in the node (Clock Divider, Phase Scaler), an inspector list. -> The "+" port
  wherever the ports are all alike; it is the most direct.
- **8.4 Three controls for choosing a mode** in custom inspectors: radio buttons
  (Button mode, Fader orientation), selectable labels (stack mode), combo boxes
  (Color Mixer and all generic params); orientation is a checkbox "Vertical" on
  Level Meter. -> Combo boxes, or make them params (7.4).
- **8.5 Fader and Fader Group hide the default port list in the inspector,
  Button and Button Group do not** (`N/ui/fader.rs:155`, `fader_group.rs:187`).

---

## 9. Node bodies (P3)

- **9.1 Controls and displays that are "the point of their node" are not all
  compact.** Compact: Fader, groups, XY Pad, Color Mixer, Gradient Stops, Peak
  Level Meter, Button. Standard frame with title and enable switch: Gain knob,
  Const, Toggle Bank, Level Meter, Gauge, Value Display, LED Display. -> Compact
  for the four displays and Gain.
- **9.2 Gestures differ between in-node controls.** Fader: absolute drag, Shift
  = fine, double-click = reset (`W/fader.rs:177-218`). Step Sequencer: its own
  copy of that. Gain: relative drag, no Shift, double-click to a hard-coded 1.0
  instead of the stored default (`N/math/gain.rs:121-131`). XY Pad: no fine mode,
  no reset. Gradient Stops: drag only. -> The fader's rules for all of them
  (Shift = fine, double-click = the node's recorded default).
- **9.3 Port names inside the node** are on for MIDI In/Out, Audio Input, Input
  Controller, reshape nodes, portals and subgraphs, but not for the nodes with
  the most ports: Launchpad S (82 outputs), X1, Push 1, DMX Input, Lookup Table,
  Mux/Demux, Port Hub. -> Switch it on there.

---

## 10. Wording and looks (P3)

- **10.1 Tab, heading and item name disagree**: tab "Palette Groups", heading
  "Color Palette Groups", item "Palette Group N"; tab "Gradients", items
  "gradient presets"; tab "Fixture Templates", item "Fixture N", column
  "Template"; "interface" means a DMX output in one tab and a network adapter in
  another (`W/dmx_input_list.rs:88`). Every tab repeats its label as a heading.
  -> Drop the headings, one noun per thing.
- **10.2 Capitalisation and punctuation**: window titles in Title Case ("DMX
  Monitor") and sentence case ("Save as macro"); buttons likewise ("Clear
  Overrides", "Listen to it"); "Setup…" with an ellipsis character, "Open..."
  with three dots; field labels with a colon in Setup and without in the
  inspector; "nothing chosen" is "(none)", "None", "No interface", "Default" or
  "All networks". -> Sentence case, "...", colons everywhere or nowhere,
  "(none)".
- **10.3 Empty states**: "No groups.", "No interfaces configured.", "No audio
  inputs. Add one below.", "No gradient presets yet." in different greys.
  -> `empty_state(ui, "No groups yet. Add one below.")`.
- **10.4 Section headers** come in three styles (strong 11 pt; strong default;
  grey strong 11 pt). `W/inspector.rs:223`, `:270`, `main.rs:1716`.
- **10.5 Numbers**: sliders in the inspector, drag fields everywhere else;
  units as " ms", " s", "K", or as a prefix ("LEDs: ", "net "); double-click
  reset only in the inspector. -> One helper for number fields with units.
- **10.6 Inline colours** (against the project rule): about 95 `Color32`
  literals in the shell and list panels and about 110 in node widgets, not
  counting colours that come from data. They fall into a few repeated cases, so
  a handful of constants removes most:

  | Use | Where (examples) | Constant |
  |---|---|---|
  | Dim text, grey 120/140 | 40+ places: `W/fixture_list.rs:27`, `main.rs:1716`, `N/io/x1.rs:110`, `N/math/multiplex.rs:117` | `theme::TEXT_DIM` (exists) |
  | Status green / amber | `W/audio_input_list.rs:96`, `W/input_controller_list.rs:107`, `W/dmx_monitor.rs:107` (three different greens) | `SEM_SUCCESS`, `SEM_WARNING` |
  | Blackout / Bypass / tick warning | `main.rs:2583-2602`, `2695` | `SEM_DANGER`, `SEM_WARNING` |
  | Errors as `LIGHT_RED` | `main.rs:1674`, `2469` | `SEM_DANGER` |
  | DMX monitor's private palette | `W/dmx_monitor.rs:10-14`, `224-244` | theme |
  | Play green | `N/io/clock.rs:66`, `internal_clock.rs:94` | new |
  | Plot background grey 20-28 | `N/display/scope.rs:135`, `N/transport/transition.rs:105`, `N/ui/xy_pad.rs:89`, `N/io/audio_input.rs:454` | new `PLOT_BG` |
  | Curve teal, highlight cyan | `transition.rs:108`, `audio_input.rs:456`; `lookup.rs:397`, `xy_pad.rs:106` | new |
  | "Off" grey 60 | `toggle_bank.rs:80`, `trigger_bank.rs:110`, `button.rs:146`, `button_group.rs:196` | new |
  | Transparency checkerboard | `color_display.rs:197`, `palette_to_gradient.rs:110`, `gradient_source.rs:450`, `lookup.rs:715` | new, plus one shared draw function |
  | Macro lock tint | `N/meta/subgraph.rs:170` | `ACCENT_MACRO` |
  | `CENTER_LINE_COLOR` defined twice | `N/ui/fader.rs:16`, `fader_group.rs:16` | theme |

---

## 11. Suggested order

1. Section 1 (ten items, mostly small): they cost work or trust.
2. One shared set of Setup building blocks (header with name/status/enabled/
   duplicate/delete, add button, empty state, status badge, address widget):
   fixes most of sections 3 and 10 at once and shrinks the list files.
3. One shared list editor for node inspectors (8.2, 1.10).
4. Catalogue: one taxonomy (5.1), renames (5.2, table in 7). Cheap now, costly
   once projects exist.
5. Type/mode conventions (section 6), then merges (5.3).
6. Colours (10.6) alongside whatever file is being touched anyway.
