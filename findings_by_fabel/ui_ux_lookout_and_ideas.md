# UI/UX outlook: big graphs and the next level of abstraction

Question: big graphs become hard to navigate. Should LightBeat get another level
of abstraction, and what could it look like?

Short answer: **yes, but not another level of nesting.** Nesting is already three
deep in your own projects and is part of the problem. What is missing are two
abstractions that are *orthogonal* to nesting:

1. **Width**: one wire that carries N values, and nodes that work on all N at once.
2. **A surface to play on**: the controls you touch live, separated from the
   plumbing that makes them work.

Plus one repair to nesting itself: **subgraphs that are instances of one
definition** rather than copies.

Everything below is a proposal; nothing was changed in the code.

---

## 1. What your own project says

Measured from `test_project_audio_midi_lp_1.json` (the largest recent project; it
still uses the old portal node names, so it predates the portal simplification):

| Fact | Number |
|---|---|
| Nodes in total | 374, on 18 levels, nesting depth 3 |
| Root level | 73 nodes, 112 wires |
| "LaunchPad" subgraph | 50 nodes, **485 wires** (about 10 per node) |
| "Sequence Trigger Generator" | 34 nodes, 81 wires: **16 x (Schmitt Trigger + AND)**, one OR, one Trigger Bank |
| Copies of that subgraph | **4**, independent of each other |
| Subgraphs holding nothing but 16 x Components to Color | 4 |
| Node types by count | Schmitt Trigger 71, Components to Color 68, AND 67 = **55 % of all nodes** |
| Portal nodes | 45 |

So more than half of the project is three node types, each placed 16 at a time,
inside subgraphs that exist four times. The graph is not big because the idea
is big. It is big because the tool makes you write the same thing 16 x 4 times
and connect every lane by hand.

Four different problems are mixed up in "big graphs are hard to navigate":

| Problem | Symptom | Cure |
|---|---|---|
| **A. Width** | the same node 16 times, 16 parallel wires | buses (section 2) |
| **B. Copies** | the same subgraph 4 times, a fix has to be made 4 times | linked definitions (section 3) |
| **C. Play vs. patch** | faders and buttons are scattered between logic nodes | front panels and a Desk view (sections 4, 5) |
| **D. Orientation** | "where was that node?", wires disappear into portals | navigation aids (section 6) |

Only D is about navigating. A and B are about there being too much to navigate.
Curing those first is worth more than any minimap.

---

## 2. Buses: width as a property of a wire

### Idea

A port carries **N lanes** of its type. This is *not* a new port type (the
convention "do not add port types lightly" stays intact): a Logic port that is 16
wide is still Logic, a wire between two of them is one wire, drawn thicker with a
small count on it.

```
today                                    with buses

Launchpad  o--------o Schmitt o--o AND   Launchpad  o===16===o Schmitt o===16===o AND
           o--------o Schmitt o--o AND                                            ^
           o--------o Schmitt o--o AND   Trigger Bank o===16===-------------------+
           ... 16 times ...
```

Rules, all of them already familiar from the mode auto-detection pattern:

- **A node takes its width from its first wire.** A Schmitt Trigger that gets a
  16-wide wire becomes 16 Schmitt Triggers with shared settings. No dropdown.
- **Narrow meets wide: broadcast.** A single value on a 16-wide node's other
  input feeds every lane (one threshold fader for all 16).
- **Wide meets wide: lane by lane.** Widths have to match, otherwise the wire is
  refused, the same way a type mismatch is refused today.
- **Three small tools** turn buses back into wires and the reverse: *Merge* (N
  inputs to one bus), *Split* (one bus to N outputs), *Pick* (one lane by index).
  Port Hub and Multiplexer/Demultiplexer are most of this already.
- **Hardware nodes offer both.** The Launchpad has an 8x8 pad bus (or 8 row
  buses) next to its single ports; a Fader Group has one bus output.

With this, "Sequence Trigger Generator" goes from 34 nodes and 81 wires to about
4 nodes and 5 wires, and the 16 x Components to Color subgraphs become one node.

### How it could be built

The engine is closer to this than it looks: values are already flat floats per
port (`total_channels`, `port_base_index`), and a Color is "3 floats that travel
together". A bus is "count x that".

The cheap way to make *every* node bus-capable without touching the nodes:

- The graph, not the node, owns the lanes, exactly as it does for promoted
  params (`engine/promoted.rs`). When a node's input is N wide, the engine keeps
  N instances of its `ProcessNode` and routes lane k to instance k. `set_param`
  goes to all of them.
- Nodes opt out with something like `ProcessNode::max_lanes() -> usize`
  (default: unlimited; 1 for hardware nodes, Effect Stack, Subgraph, UI
  controls). Same shape as `non_promotable_params`.
- `RoutePlan` resolves widths once, as it resolves offsets today. Width changes
  mark the plan dirty.
- A node's body shows lane 0, with a small lane stepper where that matters
  (Scope, Value Display). Displays could later draw all lanes at once (16 LEDs
  in a row), which is nicer than 16 LED Display nodes.

Costs and risks: every place that assumes "a port is `port_type.channels()`
floats" has to learn the width; save files gain a width per port (or derive it
from the wires on load, which is cleaner); the instance trick multiplies state,
so 64 lanes of a heavy node cost 64 times as much. None of that is a redesign.

### A first step that costs almost nothing: cable collapse

Before any engine work: when several wires run between the same two nodes, **draw
them as one thick wire with a count**, fanning out only near the ports (or only
on hover). `connect_bundle` already creates such groups, so the model is there;
this is drawing only. On the LaunchPad level it would turn most of the 485 wires
into a few dozen cables, and it shows whether buses feel right before they
exist.

---

## 3. Linked subgraphs: one definition, many instances

Today a macro is copied into the project when it is placed (a locked Subgraph
with remapped ids), and a duplicated subgraph is an independent copy. Four copies
of "Sequence Trigger Generator" mean four places to fix a bug.

Proposal:

- A project has **definitions** (a list, like palettes or gradient presets). A
  Subgraph node is an **instance** of one.
- Opening an instance opens the definition; a banner says "editing *Sequence
  Trigger Generator*, used 4 times". Every instance follows.
- What differs per instance are the **exposed settings** (promoted params on the
  subgraph's own node) and the wires. That is all an instance stores.
- "Make unique" detaches an instance into its own definition (today's "Embed").
- The macro library becomes "definitions shared between projects". A placed
  macro can stay linked to its library version and offer "update to library
  version", or be embedded.
- Duplicate (`Ctrl+D`) on a subgraph makes another instance; `Shift` makes a
  unique copy. The node shows a small "x4" so you know it is shared.

Engine-side each instance still has its own `InnerGraph` (state must be per
instance); only the UI and the save file share the definition. An edit is sent as
the same `SubgraphInnerCommand` to every instance path. The save file shrinks
too: the big project stores the same 34 nodes four times today.

Buses reduce the need for this (16 lanes replace many copies), but they do not
replace it: "one sequencer voice" used four times with different settings is the
natural case for instances.

---

## 4. Front panels: a subgraph you do not have to open

A subgraph's node already shows the Value and LED Displays inside it
(`inner_value_displays`). Take that one step further:

- Any control inside (Fader, Button, XY Pad, Color Mixer, a promoted param) can
  be marked **"Show on parent"**.
- The subgraph node then draws those controls on its body, live and usable, in a
  simple grid you can arrange.
- A subgraph becomes a device with a front panel: you build it once, close it,
  and from then on you play it from outside.

```
+-- Strobe Voice ---------------- [open] --+
o trig    [ Rate ====|---- ]    ( Armed )  o out
o phase   [ Width ==|------ ]   * active   |
+------------------------------------------+
```

This is the smallest change with the largest effect on daily use, because it
removes most reasons to navigate at all. It also makes macros far more useful:
a macro with a panel is an instrument, not a black box.

---

## 5. The Desk: a second view for playing

Patching and playing are different activities, and today they share one canvas.
During a set you want the faders, buttons, meters and the stack preview; you do
not want 485 wires behind them.

Proposal: a second view on the same project, toggled with `Tab`.

- **Patch** is today's canvas.
- **Desk** shows only the nodes that are controls or displays (Button, Fader,
  groups, XY Pad, Color Mixer, meters, Scope, Effect Stack preview, snapshot
  lists, front panels from section 4), without wires, laid out freely on
  **pages**. Controls from any nesting level can be placed on it, so the depth of
  the patch stops mattering.
- A control is the same node in both views; the Desk only stores a second
  position and size. "Show on Desk" is a flag on the node, like "Hide unused
  ports".
- The Desk can be locked (no dragging, only playing), go fullscreen, and be put
  on a second window or a touch screen.
- Right-click a control on the Desk: "Show in patch" jumps to the node.

This is the "other level of abstraction" in the plainest sense: *above* the
graph sits what the graph is for. It also gives snapshots a natural home (a row
of scene buttons per page).

---

## 6. Navigation aids (cheap, independent, worth doing anyway)

Present today: breadcrumbs, minimap, fit to selection, frames, quick add,
linked-node highlighting, hide unused ports. Missing, in rough order of value
per effort:

1. **Find (`Ctrl+F`)**: type a node name, type or portal name, get hits across
   all levels, Enter jumps there (navigates into the subgraph and centres). The
   quick-add ranking can be reused.
2. **Jump across portals**: double-click a Portal Receive to go to its Send;
   on a Send, a list of its Receives. A portal is a wire you cannot follow, so
   this is the missing "follow the wire". Same for an effect and its stack.
3. **Outline panel**: a tree of levels, frames and named nodes (the "Stack tree"
   in the Effect Stack inspector is the precedent). Click to go there. It makes
   frames into chapters.
4. **Wire focus**: when a node is selected, its wires stay bright and all others
   dim; hovering a wire brightens the whole run upstream and downstream. On a
   level with 485 wires this is the difference between readable and not.
5. **Semantic zoom**: below a certain zoom, draw nodes as plain titled blocks
   (no ports, no body, no port labels) and frame titles large. Faster to read
   and far fewer shapes to tessellate, which is where the UI time goes.
6. **Bookmarks**: `Ctrl+1..9` stores level and view, `1..9` returns. Saved with
   the project.
7. **Back / forward** through visited places (mouse buttons 4/5, `Alt+Left`),
   as in a browser. Cheap once navigation goes through one function.
8. **Collapsible frames**: a frame folds to its title bar; wires that cross it
   end at its edge. A lightweight alternative to making a subgraph when you
   only want something out of sight.
9. **Port tags instead of Receive nodes**: an input can be bound to a portal
   name directly and shows a small coloured tag instead of a node plus a wire.
   Same engine mechanism, 13 fewer nodes on the big project's levels.

---

## 7. What I would do, and in which order

| Step | What | Effort | Why this position |
|---|---|---|---|
| 1 | Wire focus, Find, portal jump, cable collapse | small | pure UI, no save-format change, immediate relief on the levels you have |
| 2 | Front panels (section 4) | medium | builds on `inner_value_displays`; removes most navigation |
| 3 | Linked definitions (section 3) | medium | touches save format and the macro library; do it before there are users |
| 4 | Buses (section 2) | large | the real cure for width; step 1's cable collapse tells you whether it feels right |
| 5 | Desk view (section 5) | medium | most valuable once panels exist; needs a layout model of its own |

What I would **not** do: deeper nesting as the answer, more node colours or
shapes as the answer, or automatic graph layout (it destroys the spatial memory
that makes a patch yours).

One decision to make early because it shapes the rest: **are buses fixed-width
per wire (16 means 16), or can a lane count follow a setup object (a group's
strip count, a controller's pad count)?** Fixed is simpler and enough for the
project analysed here. Following the setup is where it gets powerful ("one lane
per strip of this group"), and it is what would connect the bus idea to the
effect and space concept later.
