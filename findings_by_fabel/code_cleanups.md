# Code cleanups

Dead code, temporary code that can go, and things that could be simpler, faster
or safer. Collected by reading the code, `cargo build`, `cargo clippy
--all-targets`, and one check of a throwaway copy with every
`#[allow(dead_code)]` removed (to see what they hide). No source file was
changed.

- Paths are relative to `src/`. Effort: S = hours, M = days, L = a week or more.
- "Unverified" marks what was read but not reproduced or measured.
- The numbers: 61k lines in 262 files, 18 build warnings, 98 clippy warnings (25
  auto-fixable), 294 tests. No `TODO`/`FIXME`/`todo!` anywhere. All 28
  dependencies are used, no theme constant is unused, and every registered node
  has an engine arm and the reverse.

## Status

Updated as items are implemented. Items not listed here are open.

| Item | State | Commit / note |
|---|---|---|
| R1 | done | 8f0d27c: a setup that failed to load is never saved over and is kept as `setup.json.bad` |
| R2 | done | 8f0d27c: file errors are shown in a "File problems" window (settings save still silent) |
| S5 | done | ebb5af5: `widgets/nodes/graph.rs` is now `widgets/nodes/graph/` (ten files, move-only) |
| D2 | done | 819f375 |
| D5 | done | 94246d9 |
| D6 | done | ad35b00: the callbacks are used by the session and stay; the run loop, `LedHandle` and the allows are gone |
| D7 | done | 26e0159 |
| D8 | done | f93a96f (except `_is_factory_kind` in the input controller list) |
| D4 | partly | f2c0195: engine and colour items. Open: `StepValueEdits`, `unshape` (widgets) |
| D9 | done | 14a6687: the branches were identical since the first commit; collapsed |
| D12 | partly | 06398e8: `color/gradient.rs`. Open: `&Box<T>` in the inspector, manual copies in the lookup widget |
| E5 | done | b4e5679 |
| E6 | done | c68c9aa |
| E7 | done | a8ea40f |
| E9 | done | 2c1ebc9 |
| R4 | done | 323f744 |
| R3 | partly | 31d200d: setup and settings are written atomically. Open: project and autosave writes |
| R7 | done | 31d200d |
| S6 | done | merge 4799001: `main.rs` is 469 lines, the rest is `src/app/` (registry, files, update_ui, macros, panels, frame), move-only |
| E1, D10 | done | da0c2e4 |
| E3 | done | 9aaf900 |
| E4 | done | 8a34c1d |
| E2 | done | 52ed3c3 |
| E10 | done | bddb4e1 (the stacks still refresh their group targets every frame, which the audit had missed) |
| D11 | partly | 8f0c476: graph module. Open: Gradient Source description, `project.rs` comment, `is_newer` doc comment |
| R6 | done | 128cbaf: absolute deadlines, nodes read the measured step from `engine::time::dt()` |
| S3 | done | 5254b9c, 4ec8ddd: the root graph owns an `InnerGraph` core |
| S4 | done | CLAUDE.md corrected |
| E8 | partly | fe5aacb: one lock and one `params()` per node per sync. Open: `ParamDef` names still allocate |
| R5 | partly | f57f705: a tick panic is caught and stops the engine, `EngineHandle::is_running()`. Open: nothing in the UI shows it yet |
| R8 | partly | bc0eb19: dropped commands are counted. Open: the UI does not react yet |
| T2 | done | the three personal JSON files are no longer tracked |
| T4 | done | CLAUDE.md updated (again after each restructuring) |
| U1 | skipped | saves a few lines per manager at the risk of the constructors `main.rs` calls |
| S1 | done | 821a1a7, f033ab0: `NodeWidget::restore_from_save`, called by load, paste and move; Button Group is now restored too |
| S2 | done | 3f299ed: one `node_table()` with both constructors per node type |
| T6 | partly | 9dae9d4: the Formula node has its own small parser, `meval` and `nom 1.x` are gone. Open: `bindgen`/`nom 4` through `ableton-link` (needs a newer upstream) |
| M3 | done | e3ad157 |
| M4 | done | dfa78a4 |
| M5 | done | f21a67f |
| M6 | done | 856f49b |
| M7 | done | fd018f1 |
| M1, M2, M8 | kept | three test projects have no version, one has an "ADSR"; delete after they are re-saved or retired |
| D3 | done | 679d718 |
| D4 | done | 679d718 (rest) |
| D12 | done | 679d718 (rest) |
| D11 | done | b5a13bb (descriptions) |
| D1, U2 - U7, S7, E11, T1, T3, T5 | open | not started |
| R2 (settings), R3 (project file), R5/R8 (UI part) | in progress | branch `feature/shell-leftovers`, not merged |

Checked against your own files on 2026-11-13: `setup.json` uses the new pixel
format only (M3 can go), no saved Fader or Fader Group uses the old keys (M4,
M5 can go), `settings.json` does not use the old alias (M6 can go). Three test
projects have no `version` and one still has an "ADSR", so M1 and M2 stay until
those are re-saved or retired.

---

## The five to do first

| # | What | Why | Effort |
|---|---|---|---|
| R1 | A setup that fails to load is overwritten with an empty one on exit | data loss, and likely with the no-compat rule | S |
| E1 | Wire drawing rebuilds each node's port lists about 9 times per wire per frame | the canvas is where the UI time goes | S |
| S1 | Widget restore after load is written three times, and paste has only part of it | probable bug source, and every new node adds to it | M |
| E5-E7 | Audio Input, portals and hardware nodes allocate on every engine tick | 1 kHz thread | S |
| D1 | 37 blanket `#[allow(dead_code)]` | they hide everything in section 1 | S |

---

## 1. Dead code

**D1. Blanket `#[allow(dead_code)]` hides real dead code (S).** 37 of them:
`engine/types.rs:147,197,343,411`, `widgets/nodes/node.rs:23,186`,
`link_controller.rs:6`, `objects/channel.rs:190`,
`input_controller/x1/controller.rs` (11), `widgets/nodes/graph.rs:213,253,861`,
`project.rs:109,147,347`, `config.rs:122`. Several sit on things that *are* used
(`load_graph`, `AppConfig::save`, `ParamDef`), so they are only noise. Remove
all, then deal with what the compiler shows: D2 to D8.

**D2. `EngineCommand::SetParam` is never constructed (S).** `engine/types.rs:352`
and its handler `engine/graph.rs:149-157`. Params travel through
`pending_params`.

**D3. Unused helpers on `dyn NodeWidget` (S).** `widgets/nodes/node.rs:216-252`:
`read_input`, `read_output`, `params`, `set_param` are never called; `set_param`
is an empty "for now" stub.

**D4. Never-used items (S).**
- `StepValueEdits` (`widgets/nodes/transport/step_sequencer.rs:13`)
- `CommandRingBuffer` (`engine/mod.rs:26`)
- `_TYPES_REFERENCE` (`engine/nodes/math/port_hub.rs:137`) and
  `_keep_led_handle_referenced` (`input_controller/x1/session.rs:253`): code that
  exists to silence a warning
- `unshape` (`widgets/nodes/transport/envelope.rs:321`)
- `Gradient::sample_wrapped` (`color/gradient.rs:84`); `Gradient::sample` (`:47`)
  is only reached from tests

**D5. Display fields the engine fills on every UI sync and nobody reads (S).**
`GainDisplay.input` (`engine/nodes/math/gain.rs:9`);
`LookupDisplay.{columns,data,row_count}` (`lookup.rs:46-49`, a Vec clone per
sync); `MathDisplay.{connected_types,output_type}` (`math_op.rs:35`);
`PortalSendDisplay.port_defs` (`meta/portal.rs:69`); `LfoDisplay.waveform`
(`transport/lfo.rs:41`). Dropping them also saves engine-thread clones.

**D6. The X1's callback-style API is unused (S).**
`input_controller/x1/controller.rs`: `run` (226), `clear_callbacks` (291),
`set_led_pressed` (305), `last_state` (311), `LedHandle::{set_raw,set_pressed}`
(149, 157), event fields `modifiers` and `previous`. The session path is the one
in use.

**D7. Fixture accessors kept for moving heads (S).** `objects/channel.rs:219-311`:
`raw`, `pixel`, `clear_pixels`, `dimmer_value`, `set_dimmer`, `pan`, `tilt`,
`set_pan_tilt`. Remove until needed, or keep with one allow that says why.

**D8. Smaller ones (S).** `DmxInputHub::edit` (`dmx_input/mod.rs:205`);
`MidiHub::{input_connected,output_connected}` (`midi_io.rs:297,327`);
`DmxSharedState.monitor_key` (`dmx_io.rs:85`); `AnalyzerWorker.sample_rate`
(`audio/analyzers/worker.rs:40`); `AudioChunk.start_sample`,
`InputStream.{channels,samples_produced}` (`audio/backend.rs:77,91`);
`ActivityLogEntry.instant` (`input_controller/mod.rs:211`);
`MacroEntry.{creator,date}` (`macros/library.rs:20`); `LinkEvent` payloads
(`link_controller.rs:8,10`); assigned and never read: `found`
(`engine/nodes/io/push1.rs:336`), `disabled` (`input_controller/midi.rs:396`);
unused import `beat_bench/mod.rs:263`; unused parameter `_is_factory_kind`
(`widgets/input_controller_list.rs:317`).

**D9. Conditions whose branches are the same (S).**
`input_controller/mod.rs:341` (`if binary { Value } else { Value }`, then
`let _ = binary;`) and `:911-915`. Either collapse, or this is a lost feature
(binary inputs were meant to get a trigger mode?): worth a look before deleting.

**D10. A port list built and thrown away (S).** `widgets/nodes/graph.rs:4870-4874`
(`let ports = ...; let _ = ports;`), twice per wire per frame. See E1.

**D11. Leftovers of the removed Group Output (S).** User-visible text at
`widgets/nodes/math/gradient_source.rs:160` ("Output feeds Group Output");
comments at `widgets/nodes/graph.rs:1755`, `project.rs:1022`. The doc comment of
`cleanup_stale_connections` sits above `set_stack_names`
(`graph.rs:1403-1405`); the doc comment "Inline inspector for a selected
decorative frame" sits on `is_newer` (`main.rs:2959`).

**D12. Clippy findings that are more than style (S).** Three "operation has no
effect" at `color/gradient.rs:217`; `&Box<T>` at `widgets/inspector.rs:392`; two
manual copies that are `copy_from_slice` (`widgets/nodes/math/lookup.rs:160,183`).
The rest is style in bulk (15 type_complexity, 16 needless_range_loop, 9 manual
fill, 6 manual clamp).

---

## 2. Temporary migrations to delete

Your rule: migrations go once the test projects are re-saved. These remain.

| # | What | Where | Handles |
|---|---|---|---|
| M1 | `connections_for_current_layout`, `STATUS_PORT_NODE_TYPES`, tests | `project.rs:39-54`, `321-345`, `922-945` | version < 1: outputs of the five hardware nodes shift by one (the `connected` port); version < 2: Change Detect inputs swapped. The header comment only describes version 1. |
| M2 | `"ADSR" => "Envelope"` type-name alias | `project.rs:353-359` | old node name; an alias of the kind CLAUDE.md rules out |
| M3 | `PixelFormat` legacy deserialiser | `objects/channel.rs:40-80` | `"Rgb"`/`"Grb"` strings and the old tagged enum. After a re-save of `setup.json` this is a plain derive. |
| M4 | Fader legacy fields | `engine/nodes/ui/fader.rs:147-170` | old `"value"`, old `override_enabled` + `reset_mode` |
| M5 | Fader Group legacy key | `engine/nodes/ui/fader_group.rs:231` | `values` as fallback for `mouse_values` |
| M6 | `AppConfig` alias | `config.rs:27` | `autosave_on_close` |
| M7 | `GradientStop.alpha` default "for old JSON" | `color/gradient.rs:12-16` | could stay as a deliberate format default |
| M8 | `SavedParamValue` untagged with `as_f64` coercion | `project.rs:137-160`, `379-396` | unverified: may be migration tolerance, may be needed because untagged JSON cannot tell Int from Choice. Check first. |

Note: the largest test project (`test_project_audio_midi_lp_1.json`) still
contains "Input Portal TX/RX" and "Output Portal TX/RX" nodes, which no longer
exist; and one older project has an "ADSR". So the test projects need more than a
re-save before M1/M2 can go, or they are retired.

---

## 3. Structure

**S1. Per-type widget restore exists three times, and the copies differ (M).**
`load_graph` has about 30 blocks of `if saved.type_name == "X" { downcast;
restore }` (`project.rs:413-770`). Paste repeats 9 of them
(`widgets/nodes/graph.rs:2534-2619`), "Move to Subgraph" another 8
(`:3920-3987`). 22 widgets already have the same inherent method
`restore_from_save_data(&mut self, &serde_json::Value)`. Paste calls it only for
portals and subgraphs, so a pasted Fader, Button, Lookup Table, DMX Input or
MIDI node probably starts with default ports and can lose wires on its dynamic
ports (unverified at run time; worth trying: copy a Lookup Table with extra
columns).
-> Make it a `NodeWidget` method with an empty default, called from one helper
used by load, paste, move and unpack. The step "restore it in project.rs" then
disappears from the new-node checklist.

**S2. A node is registered in four places, matched by its display string (M-L).**
Widget factory (`main.rs:479-849`, 85 calls, each with hand-typed
`new_shared_state(in, out)` numbers that must match the engine), engine arm
(`main.rs:918-1050`, about 95 string arms), the restore of S1, and
`widgets/nodes/paths.rs`. Effects are listed twice (`:836-847`, `:1041-1045`).
-> One table of `NodeDescriptor { group, name, make_widget, make_engine }`, the
factories taking a context struct (hubs, portal registry, object store). Take
the buffer sizes from the engine node (`total_channels`, or a `max_channels()`
for dynamic ones) instead of typing them. About 80 of the 95 types are plain
`X::new(id)` pairs. Together with S1 and S7, adding a node becomes two files and
one line.

**S3. `EngineGraph` and `InnerGraph` are near-copies (M).** Same code in both:
the pending-params step of `tick` (`engine/graph.rs:229-261` vs
`engine/nodes/meta/subgraph.rs:194-224`), `rebuild_plan` (`113-132` vs
`162-181`), `sync_shared_state` (`342-376` vs `287-321`), command handling
(`134-196` vs `501-556`; `SubgraphInnerCmd` repeats 7 `EngineCommand` variants).
`RoutePlan`, `PromotedInputs` and `SaveDataGate` are shared already.
-> Let the root graph *own* an `InnerGraph` (call it `GraphCore`) with empty
bridges and keep only what is root-only (probe, profiler, effect flatten, DMX).
About 200 lines less, and the rule "change both together", which already cost
98.6 % of a profile once, cannot be broken any more.

**S4. Subgraph command routing is already recursive (S, docs only).**
`apply_inner_cmd_at_path` (`engine/nodes/meta/subgraph.rs:485-498`) recurses to
any depth (guard: 32). CLAUDE.md's "up to 2 levels of nesting" is out of date.

**S5. Split `widgets/nodes/graph.rs` (5362 lines) (M, mechanical).**

| New file | Content | Lines today |
|---|---|---|
| `graph/quick_add.rs` | matching, popup, tests | 59-142, 2233-2311, 5024-5190 |
| `graph/frames.rs` | frames, minimap, fit | 220-236, 338-622 |
| `graph/links.rs` | effect stacks, portals | 726-790, 1406-1533, tests 5191-5281 |
| `graph/connections.rs` | add/remove, remaps, hidden ports, cleanup | 1254-1402, 1535-1710, 2701-2740 |
| `graph/clipboard.rs` | copy, paste, duplicate, delete | 254-284, 2312-2685 |
| `graph/menus.rs` | port and context menus | 2741-3032 |
| `graph/interact.rs` | `handle_interactions`, hover, cursor | 956-1253, 3033-3633 |
| `graph/subgraph_ops.rs` | navigate, move into, unpack | 3634-4269 |
| `graph/draw.rs` | the free drawing functions | 4270-4890 |

`show` itself (1712-2232, 520 lines) wants to become draw_frames / draw_wires /
draw_nodes / draw_content; `handle_interactions` is 600 lines in one function.

**S6. Split `main.rs` (3069 lines) (M).** `update_frame` is one function of 860
lines (2073-2934); its comment markers are the cuts: bookkeeping, menu bar, setup
window, macro dialog, inspector, status bar, canvas drops, shortcuts, close
handling. Files: `app/registry.rs` (479-849, 908-1080), `app/files.rs`
(1082-1238, 1360-1650), `app/update_ui.rs` (1240-1358), `app/macros.rs`,
`app/panels.rs`.

**S7. Ports are declared twice per node (M).** 195 `PortDef::new` in the
widgets against 246 in the engine; e.g. `engine/nodes/math/gain.rs:30` and
`widgets/nodes/math/gain.rs:41`. -> The engine publishes its port list to the
shared state when it changes, and `ui_inputs`/`ui_outputs` get a default that
reads it. Nodes with fixed ports drop both methods, the two sides cannot
disagree, and the per-frame port building of E1/E2 goes away. (The engine
changing ports must still mark the route plan dirty, as CLAUDE.md warns.)

---

## 4. Duplication

**U1. Seven list managers repeat `{items, next_id}`, `from_x`, id allocation
(S).** `widgets/color_palette_list.rs:7-19`, `color_palette_group_list.rs:5-17`,
`fixture_list.rs:7-19`, `gradient_preset_list.rs:7-27`, `group_list.rs:9-22`,
`object_list.rs:8-37`, `interface_list.rs:7-41`. -> A generic `IdList<T>`.
Goes well with the shared Setup building blocks proposed in
`ui_ux_streamlining.md`.

**U2. Launchpad, Push 1 and X1 widgets share their binding UI (S-M).**
`widgets/nodes/io/launchpad.rs:116-178`; against `x1.rs` only 79 lines differ
after substituting the name. Identical restore blocks at `project.rs:551-570`.
-> One `ControllerBinding` with `sync_from`, `show_status`, `show_picker(kind)`.

**U3. Hardware engine nodes share the "lock, find by id, check kind, copy name
and status" opening (M).** `engine/nodes/io/launchpad.rs:101-112`,
`x1.rs:104-115`, `push1.rs:332-353`, `input_controller.rs:98-202`. -> One
`bind_controller` helper; fixes E7 in passing.

**U4. `NodeWidget` boilerplate (M).** All 75 impls write `node_id`, `type_name`,
`shared_state`, `as_any_mut`, `min_width`, `min_content_height` by hand, and
most open `show_content` with "lock shared, downcast the display, copy to
self". -> A `NodeBase { id, shared }`, a small macro for the identity methods,
and `fn display<T>(&self) -> Option<T>`.

**U5. `ProcessNode` boilerplate (M).** About 65 impls with
`write_input`/`read_input`/`read_output` of the shape `if i == 0 {..} else
{0.0}`; 48 `params`/`set_param` pairs that state names and ranges twice (e.g.
`engine/nodes/math/gain.rs:56-70`). -> Simple nodes hold `inputs: [f32; N]`,
`outputs: [f32; N]` behind default methods; params are described once in a
table that drives `params`, `set_param` and the clamping.

**U6. Hand-written JSON in save/load (M).** 121 `json!` and 92
`data.get("..").and_then(..)` chains in `engine/`, against 11
`serde_json::from_value`. Some nodes also save what is already a param
(`engine/nodes/math/gain.rs:72-80` saves `gain`, which `SavedNode.params` has
too). -> A `#[derive(Serialize, Deserialize)] #[serde(default)]` config struct
per node; drop save data that repeats params.

**U7. Two mutex families (M).** 375 `lock().unwrap()` on `std::sync::Mutex`;
`parking_lot` is already used in 13 files. -> `parking_lot::Mutex` for the
shared state: no unwraps, no poisoning (see R5), and it is faster uncontended.

---

## 5. Efficiency

Engine thread (1 kHz) first, then UI.

**E5. Audio Input builds its display data on every tick (engine, S).**
`engine/nodes/io/audio_input.rs`: the global audio-state lock twice (`:87`,
`:178`), `analyzer_kinds.clone()` (`:107`), a frame clone (`:145`), counts
cloned twice (`:156-157`), and at `:181-198` a name clone, `analyzer_param_defs()`,
a name clone per port and a Vec per analyzer. -> Move 178-198 into
`update_display` (every 16th tick); compare kinds without cloning.

**E6. Portals clone port definitions and values on every tick (engine, S).**
Send: `engine/nodes/meta/portal.rs:127-138` (name, ports with a String each,
values, map insert). Receive: `:215-219` clones both back. -> Insert only when
name or ports change, otherwise `copy_from_slice`. With 45 portal nodes in the
large project this is not small.

**E7. Hardware nodes clone the controller name and lock the global controller
list on every tick (engine, S).** `engine/nodes/io/launchpad.rs:112`,
`x1.rs:115`, `push1.rs:353`; `input_controller.rs:201-203` already compares
first.

**E8. The UI sync allocates per node (engine, ~62 Hz x nodes, M).**
`engine/graph.rs:350` (`non_promotable_params()` copied into the shared state),
`:362` (`node.params()`: a fresh `Vec<ParamDef>` with owned `String` names), a
second lock per node at `:371`, and 50 nodes re-box their display in
`update_display`. Same in `engine/nodes/meta/subgraph.rs:295-320`. -> `ParamDef`
names as `&'static str` or `Cow`, write values into the existing Vec, one lock,
reuse the display box with `downcast_mut`.

**E9. Median smoothing allocates and sorts per tick (engine, S).**
`engine/nodes/transport/smoothing.rs:109-110`. -> A scratch Vec in the node and
`select_nth_unstable_by`.

**E1. Wire drawing rebuilds port lists about 9 times per wire per frame (UI,
S).** `widgets/nodes/graph.rs:1905-1910`: per wire, `port_screen_pos` twice
(`:4859-4875`) and `port_type_for` (`:4877-4890`). Each `port_screen_pos` does a
linear search for the node, builds a port list it throws away (D10), calls
`node_size` (both lists plus two lock-and-clone of the hidden ports,
`widgets/nodes/node.rs:434-438`) and `port_pos` (another list). Every list is a
`Vec` of cloned `String`s, and `ui_inputs_full` locks the mutex. All of it runs
*before* the off-screen cull, which is inside `draw_bezier` (`:4823`). On the
level with 485 wires that is some 4000 port lists per frame.
-> Compute the node rects (today computed after the wires, `:1959-1969`) and
each node's port types once per frame, then use the existing
`port_screen_pos_from_rects` (`:4270`).

**E2. Three per-frame passes derive every node's ports again (UI, M).**
`deselect_on_port_change` hashes all port names of every node (`:1666-1680`);
`cleanup_stale_connections` does two linear node searches and two port lists
per wire, O(wires x nodes) (`:1688-1710`); `refresh_hidden_ports` locks once or
twice per node (`:1535-1568`). Hardware nodes are the worst case: the Launchpad
builds 80 + 82 ports with `format!` on every call
(`widgets/nodes/io/launchpad.rs:69-110`). -> A per-frame port cache, or a port
generation counter per node so that these only look at nodes that changed. S7
solves it at the root.

**E3. Node chrome is drawn for nodes that are off screen (UI, S).** The loop at
`graph.rs:1981-2020` has no visibility test (content is clipped at `:2031`,
wires at `:4823`). -> `if !canvas_rect.intersects(node_rects[i]) { continue; }`.

**E4. A full `egui::Style` clone per visible node per frame when zoom != 1 (UI,
S).** `graph.rs:2044-2060`. -> Build the scaled style once per frame, share it
as `Arc<Style>`.

**E10. `reconcile_effect_stacks` does about seven passes over all levels every
frame (UI, S-M).** `graph.rs:1413-1533`, with String clones per stack and
effect, and it pushes members and conflicts to every stack even when nothing
changed. -> Leave early when the collected (names, links) equal last frame's.

**E11. The dirty check serialises the whole project (UI, S, low).**
`main.rs:1166-1168`; used by autosave (every 30 s) and on close. Fine as it is;
compare `ProjectFile`s or a hash if it ever shows up.

---

## 6. Robustness

**R1. A setup that fails to load is overwritten on exit (S, data loss).**
`load_setup` only prints the error (`main.rs:1100`) and the app goes on with an
empty setup; `on_exit` always saves the setup (`main.rs:2949`), as does Ctrl+S
(`:2794`). With "no backward compat", a `setup.json` that no longer parses is a
realistic case, and it is then replaced by an empty one without a word.
-> On a load error: do not save, move the file to `setup.json.bad`, and tell
the user in a dialog.

**R2. Save errors only go to stderr (S).** Project (`main.rs:1137`), setup
(`:1106`), autosave (`:1185`), settings (`config.rs:126`, `let _ =`). A packaged
build has no terminal. -> One visible error channel (also in
`ui_ux_streamlining.md`, 1.8).

**R3. Files are not written atomically (S).** `std::fs::write` at
`setup.rs:71`, `config.rs:126`, `main.rs:1185`. A crash or full disk in the
middle leaves half a file. -> `paths::write_atomic` (temp file, then rename).

**R4. `ProcessNode::as_any_mut` defaults to `unimplemented!` (S).**
`engine/types.rs:448`; only 7 nodes override it, and it is called on whatever
node a subgraph path names (`engine/graph.rs:213-216`,
`engine/nodes/meta/subgraph.rs:492-495`). A stale path that names another kind
of node panics the engine thread. -> Return `Option`.

**R5. When the engine thread dies, nothing says so (M).** It is spawned without
a panic handler (`engine/mod.rs:57-60`). If it panics while holding a node's
mutex, the mutex is poisoned and the UI panics at one of the 375
`lock().unwrap()`; otherwise the UI keeps running against a dead engine and the
lights freeze. -> `catch_unwind` around `tick`, a visible "engine stopped" state
with a restart, and U7.

**R6. Fixed `DT = 0.001` while a tick takes 1 ms *plus* the work (M).** The loop
is `tick(); sleep(1 ms)` (`engine/graph.rs:383-392`), so the real period is tick
time plus sleep plus overshoot. Nodes that count fixed steps run slow by that
much: Envelope (`engine/nodes/transport/envelope.rs:157,176,200`), Time to Phase
(`time_to_phase.rs:8,68`), snapshot fades (`engine/snapshots.rs:20,302`), Trigger
Hold, Smoothing. With the ~215 us per tick CLAUDE.md quotes that would be at
least 20 %, and it varies with load (not measured: easy to check with a Time to
Phase of 10 s and a stopwatch). LFO, Internal Clock and Random use real elapsed
time. -> Pass the measured `dt` into `process`, or run the loop against absolute
deadlines (which also steadies the DMX cadence).

**R7. Settings are reset silently on a parse error (S).** `config.rs:113`
(`unwrap_or_default()`), and the struct has `#[serde(default)]` per field only
(`:27-58`), so one new field without it wipes the recent projects and the
latency. -> `#[serde(default)]` on the struct, and report the error.

**R8. A dropped engine command leaves UI and engine out of step (S).**
`EngineHandle::send` blocks the UI for up to about 18 ms and then drops the
command with an `eprintln` (`engine/mod.rs:73-92`). -> At least flag "needs
full re-sync" and reload the graph into the engine.

---

## 7. Tooling and repository

**T1. CI only runs `cargo test` (S).** `.github/workflows/ci.yml:44`; no clippy,
no `[lints]` table. After section 1, `cargo clippy -- -D warnings` is within
reach (25 of 98 warnings fix themselves). `cargo fmt --check` reports 2377
hunks: decide once whether to format (one commit) or not.

**T2. `settings.json`, `setup.json` and `project.json` are tracked and always
modified (S).** `setup.json` is 155 KB of your personal fixtures and
controllers. -> Untrack and ignore them; put an example setup into `examples/`
if one is wanted.

**T3. Root files (S).** `overview.md` (untracked session notes), `things.txt`
(ignored, but it is the live backlog; two of its items look done: hardware
status outputs, discard on close). -> One place for the backlog (`docs/` or
issues). `target/` is 18 GB; `cargo clean` now and then.

**T4. CLAUDE.md is out of date in places (S).** The directory map lists
`engine/nodes/output/` with group, group_preview, effect_stack (the directory
does not exist; effects are in `engine/effects/`); "up to 2 levels of nesting"
(S4); "~46 tests" (294); "Up/Dn buttons" in the group list (they are arrows);
the new-node checklist does not mention the restore block in `project.rs` (S1)
or `paths.rs`.

**T5. Inline colours (M).** 209 `Color32` literals outside `theme.rs`; top
files `widgets/nodes/graph.rs` (20), `widgets/input_controller_list.rs` (20),
`main.rs` (17), `widgets/nodes/math/lookup.rs` (11), `widgets/dmx_monitor.rs`
(10). The breakdown by use and the constants that would cover them are in
`ui_ux_streamlining.md`, 10.6. A CI grep for `Color32::from_` outside
`theme.rs` (with an allow-list for data colours) would keep it that way.

**T6. Dependencies that will stop compiling one day (S-M).** `nom 1.2.4` through
`meval 0.2` (the Formula node); `cexpr 0.3.6` and `nom 4.2.3` through `bindgen
0.49` in `ableton-link 0.1`. -> Replace `meval` (`fasteval`, `evalexpr`); the
Link binding needs a newer upstream or a fork.

---

## 8. A possible order

1. R1, R3, R7 (an afternoon; they protect your files).
2. D1 to D12 and M-items whose test files are gone anyway, then clippy in CI.
3. E1, E3, E4 and E5 to E7 (small, measurable: note the numbers before and
   after with `LIGHTBEAT_PERF_LOG=1`).
4. S1 (restore as a trait method), then S2 (one registry) and S7 (ports from
   the engine): together they cut the cost of every future node.
5. S3 (one graph core), R5, R6.
6. S5/S6 file splits, whenever a quiet moment allows; purely mechanical.
