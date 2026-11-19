# Things to try by hand: outlook steps 1 to 5

Nothing here was run in the GUI when it was built (`v0.3.0-beta.1` and the
fixes after it). The lists
come from the agents that wrote each step. Tick what works; anything that does
not is a bug to report.

A project with a wide level helps for step 1 (the "LaunchPad" level of
`test_project_audio_midi_lp_1.json`, 485 wires). Opening an old project also
exercises the migration: a `.vN.json` backup appears next to it and a "Files"
notice says so.

## Before anything else

- [ ] Open each test project. It converts with a backup and a notice, and
      looks as it did before.
- [ ] Old wires on Fader Group, Button Group, Trigger Bank, Toggle Bank and
      Launchpad S outputs still sit on the same controls (their outputs moved
      by one for the new bus port).

## Step 1: wires, find, jumps

Wire focus
- [ ] Select a node: its wires stay bright, the rest dims. Deselect: all back.
- [ ] Hover a wire: the whole run up- and downstream lights, through chains.
- [ ] A selected wire is never dimmed.

Cable collapse
- [ ] Several wires between two nodes show as one thick wire with a count.
- [ ] Hovering the cable opens it without flicker; so does selecting either node.
- [ ] Inside an opened cable: click, delete and re-drag a single wire.
- [ ] A cable of mixed port types is grey; one type keeps its colour.
- [ ] The straight fan-out near the ports and the label position look right at
      several zooms.
- [ ] View, "Collapse parallel wires" switches it off and on; survives a restart.
- [ ] The big level stays smooth while hovering wires.

Find
- [ ] `Ctrl+F` opens the box; typing finds nodes and frames on other levels.
- [ ] Enter jumps into a deeply nested level; the breadcrumb is right.
- [ ] A frame hit centres the frame.
- [ ] `Ctrl+F` while a text field has focus does nothing odd.
- [ ] `Ctrl+F` does not also fit the view; plain `F` still does.

Jumps
- [ ] Double-click a Portal Receive: goes to its Send, also in another subgraph.
- [ ] The Send's inspector lists its Receives; each row jumps.
- [ ] An effect's inspector has "Go to stack"; the stack's layer rows jump.
- [ ] The node's right-click menu has "Go to ..." entries.

## Step 2: front panels

- [ ] Inside a subgraph, right-click a Fader: "Show on parent". It appears on
      the Subgraph node and works from there.
- [ ] "Show on parent" is offered only for controls and displays (Fader,
      Button, Const, Gain, Step Sequencer, Envelope, Value Display, Scope, ...),
      not for an Add, a Bus Split or a Portal Receive; the pin next to `in` is
      offered for every node. A Subgraph is offered only once it has a panel.
- [ ] Dragging a Fader, Button, XY Pad or Color Mixer on the panel does not
      drag the Subgraph node or start a lasso.
- [ ] Double-click on a panel control does not open the subgraph.
- [ ] Resize the Subgraph node: the cells wrap; the header does not overlap
      them, also at other zooms.
- [ ] Value Display, LED Display and Scope as panel cells look acceptable.
- [ ] Pin a single setting (the pin next to `in` in the inspector): a slider
      or a dropdown fits its cell; a wired setting shows as driven.
- [ ] The Subgraph inspector's "Front panel" list: move up/down, remove, jump.
- [ ] A subgraph in a subgraph, both with panels: the inner panel shows as one
      cell on the outer one.
- [ ] Place a macro that has flagged controls: the panel is there at once and
      works while the macro is locked.
- [ ] Save, reopen, undo, copy/paste: the panel is still there.

## Step 3: linked subgraphs

- [ ] `Ctrl+D` on a subgraph: both show "x2". `Ctrl+Shift+D`: a unique copy.
- [ ] Open one instance: the banner says how often it is used.
- [ ] Add a node and a wire inside one: the other follows.
- [ ] Remove a node inside one: the other follows.
- [ ] A fader on the panel has its own value per instance; a counter inside
      counts on its own per instance.
- [ ] Turn a knob that is NOT on the panel inside one instance: the other
      follows without its running state being reset.
- [ ] Undo right after an edit: one step undoes the edit in all instances.
      Redo. No sync fires back.
- [ ] Navigate out right after an edit: the others are up to date.
- [ ] "Make unique" on an instance: it stops following.
- [ ] Copy an instance and paste it: another instance. Pasting a definition
      into itself is refused with a notice.
- [ ] Edit, "Link identical subgraphs" on an old project with copies (the four
      "Sequence Trigger Generator"): they become instances; a notice counts them.
- [ ] Place the same macro twice: two instances. Change the library file:
      the inspector offers "Update from library"; it updates both.
- [ ] "Embed" on a macro instance unlocks all its instances.
- [ ] Hand-edit a saved project so two instances differ, reopen: the first
      wins, one line on stderr.

Known behaviour, not a bug: a Button's latch or a Fader's value inside a
definition follows the definition unless the control is on the front panel.
Removing a port from the middle of a definition drops (does not move) the
wires on the other instances' outer sides.

## Step 4: buses

- [ ] Launchpad `pads` -> Schmitt Trigger -> AND -> Bus Split: the Schmitt and
      the AND show "x64", the wires are thick with the count.
- [ ] A single wire on the AND's second input feeds every lane.
- [ ] Bus Merge of 8 wires -> a node -> Bus Split gives 8 outputs with the
      right values; Bus Pick with an index picks one lane.
- [ ] The inspector's Lane stepper changes what the node body, the port values
      and the hover scope show.
- [ ] A wide wire onto a promoted setting's port: each lane gets its own value.
- [ ] Two wide wires of different widths into one node: the second is refused.
- [ ] A wide wire into a node that cannot be laned (a Fader, an effect, a
      Subgraph) is refused.
- [ ] Change Bus Merge's input count with a chain behind it: the chain follows.
- [ ] Remove the source of a laned chain: it falls back to single wires.
- [ ] Fader Group, Button Group, Trigger Bank, Toggle Bank: the bus output
      carries all values.
- [ ] Save and reopen a project with a bus and a Bus Split: all wires survive.
- [ ] Undo/redo and copy/paste of a laned chain.
- [ ] Hovering a wide port says how many lanes.

Through subgraphs and portals
- [ ] A wide wire into a Subgraph input: inside, the Graph Input port is wide
      and a node behind it is laned. A wide wire into Graph Output leaves the
      Subgraph node wide.
- [ ] Change the width outside: the inside follows. Nested two levels deep.
- [ ] A wide wire into a Portal Send: every Receive of that name is equally
      wide, also on other levels. Remove the source: they fall back.
- [ ] "Move selection into subgraph" across a wide wire keeps it.
- [ ] Two instances of a linked subgraph, one fed a bus and one a single wire:
      each runs at its own width.
- [ ] A single wire dropped on a Bus Split that has several outputs wired is
      refused.
- [ ] A bus wire next to another wire between the same two nodes stays thick
      (it is not collapsed into a cable).

## Step 5: the Desk

- [ ] `Tab` switches Patch and Desk; not while typing in a text field. (It did
      not work at all in `v0.3.0-beta.1`.)
- [ ] "Show on Desk" from a node's right-click menu and from its inspector,
      also for a node deep inside subgraphs. It lands on the current page.
- [ ] "Show on Desk" is offered only for controls and displays, in the menu
      and in the inspector; a project from before this (an Add on the Desk)
      shows nothing for it, with no gap. The snapshot list is offered for every
      node.
- [ ] Controls on the Desk work (fader, button, XY pad, colour mixer).
- [ ] Move by the title strip, resize by the corner; both snap. Cursor and
      hover outline feel right.
- [ ] Pages: add, rename by double-click, reorder, delete (asks when not
      empty; its items move to the first page).
- [ ] Right-click an item: "Show in patch" jumps to the node; "Remove from Desk".
- [ ] Lock: nothing moves, the side panels go, playing still works. Saved
      with the project.
- [ ] Full screen from the button and with `F11`, on Linux and on the Mac.
- [ ] A subgraph with a front panel on the Desk shows its panel.
- [ ] Two instances of a linked subgraph: their controls on different pages.
- [ ] The same node on the Desk and on a parent's front panel at once: both
      work, no egui id warnings (red text).
- [ ] A snapshot list on the Desk (from the snapshot panel): a row of buttons,
      the active one lit, a click recalls, also with a fade.
- [ ] Scroll a page larger than the window; drag items near its edges.
- [ ] Undo after moving an item, adding a page, locking.
- [ ] Duplicate a node that is on the Desk: the copy is not on the Desk.
- [ ] In Desk view, canvas shortcuts (Delete, `Ctrl+D`, `Ctrl+V`, `F`, `H`,
      `M`, `Ctrl+F`) do nothing to the hidden canvas.

Known limits: the Desk does not zoom, the selected page is not remembered
between sessions, selecting a Desk item does not drive the inspector.
