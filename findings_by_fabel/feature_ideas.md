# Feature ideas

What LightBeat is, as I read it: an instrument for playing light to music. Tempo
and audio analysis come in, a patch turns them into phases, triggers, colours and
gradients, effects paint a group's space, stacks blend the result, DMX goes out.
It is aimed at clubs, where nobody writes a cue list.

The ideas are ordered inside each section by how much fun or use I expect per
unit of work. Things already on your own lists (`overview.md`, `things.txt`) are
marked "(yours)" and kept short. Structural ideas (buses, front panels, a Desk
view, linked subgraphs, find and bookmarks) are in
`ui_ux_lookout_and_ideas.md` and not repeated here.

Effort: S = a day or less, M = a few days, L = a week or more.

---

## 1. Playing live

**Tap tempo and nudge (S).** The Internal Clock gets `tap` and `nudge +/-`
trigger inputs and a "resync bar" input ("the one is *now*"). When the beat
tracker picks the wrong downbeat or the half tempo, one pad press fixes it. The
audio Beat Clock's `bar_shift` could take the same "the one is now" trigger,
which is much easier than dialling sixteenths by ear.

**Master section (S).** Blackout and Bypass are buttons in the status bar. Add a
grand master fader next to them that sits after everything
(`DmxOutputManager`), and a *Master* node with `level`, `blackout` and `freeze`
(hold the last frame) inputs, so a hardware fader or pad can drive them. Today a
master dimmer has to be built into every root stack, and blackout needs the
mouse.

**Soft start (S).** When Blackout or Bypass is released, and when a project
opens, fade the output in over half a second or so, so that nothing ever
flashes the room. `dmx_bypass_on_startup` has no control in the UI yet; a
"start bypassed" toggle next to Bypass would make it discoverable.

**Scene launcher (M).** Snapshots are per node. Add a *Scene* node (or a list in
the status bar) that recalls named snapshots on many nodes at once, with one
fade phase for all of them. With the Launchpad this becomes a clip grid: one pad
per look, quantised to the next bar.

**Quantised triggers (S).** A small node: a trigger comes in at any time and
goes out on the next beat, bar or 16th (inputs: trigger, phase; setting: grid).
It makes every button press musical. It can be patched today from a Flip-Flop
and an AND (your "Synced Trigger" subgraph does that), which is the sign that it
deserves to be a node.

**Looper for controls (M).** Record the movement of a fader or the presses of a
button over 1, 2 or 4 bars and loop it in sync. Input: value, record gate, bar
phase. Output: the looped value. Overdub by holding record. It is the fastest
way to make a pattern without patching one.

**Morph between two looks (M).** Snapshot fades already blend the settings of
one node. An A/B crossfader that blends *two scenes* (see launcher) under a
fader gives a DJ-style transition between looks.

---

## 2. Effects

**More Bursts and Loops (S each).** Comet (yours), Flash, Chase (yours), plus:
- *Strobe Loop*: on-fraction of a phase, optional random pixel subset per flash.
- *Twinkle*: random pixels fading in and out, density and lifetime as inputs.
- *Noise Loop*: smooth 2D noise drifting through the gradient (clouds, fire,
  water); the single most "looks expensive" effect for LED strips.
- *Meter*: a bar whose length is a value, from the centre or an end; made for
  your shelf where LED 0 is in the middle (kick = bar grows from the centre).
- *Spectrum*: the eight bands of the Spectrum analyzer across x.
- *Scroll*: a value pushed in at one end and shifted along (a waterfall of the
  audio envelope). Very readable on long strips.

**Modifier effects (M).** A kind of entry in a stack that does not draw but
changes what is below it: Mirror, Kaleidoscope around the centre, Shift (move
the picture by a phase), Blur/Trail (feedback: the previous frame faded and
added), Pixelate, Hue shift, Mask. Trails in particular make every Burst look
better at no cost per effect. The stack already blends layers in order, so a
layer that reads the composite so far fits the model.

**Stack preview as a window (S-M).** The stack shows its picture on the node. A
dockable "Stage" window that draws the group's space (strips and fixtures where
the group editor puts them) with the live output makes it possible to build a
show without the installation switched on, and to check a look on the laptop
before it hits the room.

**Per-effect audio hook (S).** A common pattern will be "envelope into amount".
A quick action on an effect's input, "Drive from audio...", that drops the Audio
Input output and wire in one step (the port menu already offers a Button for a
trigger input; this is the same idea).

**Palette as the show's key (S).** A project-wide "current palette" that every
Gradient Source or Palette to Gradient can follow, with a fade when it changes.
Changing one thing recolours the whole show. Portals can do it today; a
first-class setting makes it the default way to work.

---

## 3. Music understanding

**Phrase counter (M).** The beat tracker knows beats and bars. Dance music moves
in 8-, 16- and 32-bar phrases, and Build-up and drop already detects the big
moments. A phrase output (phase over 8/16/32 bars, re-anchored on a detected
drop) lets looks change exactly where the music does, which is the README's
promise ("structure of the music").

**Energy level (S-M).** One slow 0..1 output for "how intense is the music right
now" (loudness, brightness and onset density, smoothed over seconds). Patch it
into a Lookup Table of looks and the show follows the set by itself.

**Section memory (L).** Recognise "this is the breakdown again" within a track
(novelty is already computed for the bar start) and output a section index, so
the same part of a track gets the same look.

**Real recordings in the bench (S, ongoing).** CLAUDE.md says synthetic audio is
easier than records. A handful of annotated real clips (`name.wav` + `name.json`)
for the genres you play would make every tracker change measurable where it
counts.

**Formula with analyzer settings (yours).** `@master_in.beat_clock.min_bpm`.

---

## 4. Patching comfort

**Insert on a wire (S).** Drop a node on a wire, or quick-add while hovering a
wire, and it is spliced in (first compatible input and output). Removing a node
with one input and one output heals the wire (`Ctrl+Delete`).

**Probe everything (S).** Hover scope exists for root-level ports. A pinned
probe (click the hover scope to keep it open as a small floating scope) lets you
watch two or three signals while turning a knob elsewhere, without adding Scope
nodes and wires.

**Value on the wire (S).** Optional: wires carry their live state. A Logic wire
lights while high, a Phase wire shows a moving dot, a Color wire takes its
colour. The patch then explains itself while it runs. Has to be optional and
culled, since wires are where the UI time goes.

**Learn anywhere (M).** Right-click a setting or a Fader node, "Learn MIDI",
move a control: done. It would create or extend a MIDI Input entry and promote
the param behind the scenes. Today that is five steps over two nodes.

**Node presets (S).** Snapshots store settings per node instance. "Save as
preset" would store them per node *type* in the library, so a good Envelope or
Wave Loop setting can be reused in another project.

**Starter templates (S).** New project from a template: "one group, beat clock,
three effects, a stack". Together with the demo project this is the
difference between an empty canvas and something that already blinks.

**Notes on the canvas (S).** Frames have notes. A free text note (sticky) and a
description field per subgraph shown as hover text make shared projects and
your own six-month-old patches readable.

**XY Pad as Position (yours), Phase Scaler inputs (yours).**

---

## 5. Hardware and I/O

**OSC in and out (M).** Opens TouchOSC/phones as control surfaces and lets
LightBeat talk to VJ software (Resolume, TouchDesigner). A phone on the same
network as a remote is a lot of fun for little work.

**MIDI clock in and out (S-M).** Not every DJ setup runs Link. MIDI clock in as
a third clock source next to Link and audio; clock out to sync a drum machine or
a VJ.

**Controller feedback without patching (M).** For the Launchpad, Push and X1: a
default feedback mode where a pad shows the state of what it is wired to (the
LED follows the Button/Toggle it drives). Today feedback needs its own wires
(the 16 x Components to Color subgraphs in your project are exactly that).

**sACN input (yours), visual DMX channel picker (yours).**

**Moving heads (L).** `PanTilt` channels and the `Position` type exist. Missing
is the step from the group's space to pan/tilt: a fixture with a position and
orientation in the room, and a "look at this point" input. Not needed for the
shelf; it is the feature that would make LightBeat usable in an actual club rig.

**Pixel-map output for matrices (M).** Space is already 2D. A fixture kind
"matrix" (width x height, serpentine or not) would let effects paint panels as
well as strips.

---

## 6. Trust and safety for a live tool

**Show mode (S).** A lock that disables everything destructive: delete, wire
edits, setup edits, quit without confirmation, the update prompt. One toggle
before the doors open.

**Undo history list (S).** Undo exists; a visible list of the last steps with
names ("moved 3 nodes", "changed Rate") makes it trustworthy.

**Project versions (S).** Autosave covers crashes. Numbered backups on every
manual save (`name.json.1` to `.9`), or "Save version" with a note, covers the
"it was better yesterday" case.

**Health panel (S).** One place that lists what is wrong right now: devices
waiting, a portal Receive with no Send, an effect pointing at a stack that does
not exist, two roots on one group, universes with no interface. Most of these
are detected already and shown in different places.

**Engine load per level (S).** The node profiler exists as an env var. A small
"load" readout per subgraph in the UI (and a red title when a node dominates)
tells you what to simplify without a terminal.

---

## 7. Just for fun

- **Auto-pilot**: pick the next look at phrase boundaries from a pool, weighted
  by energy. Leave the room and it still looks intentional.
- **Record and replay**: record the DMX output (or better, all control inputs)
  with the audio position and replay it; useful for testing, for showing the
  project to someone, and for making a video without the installation.
- **Screensaver for the shelf**: an idle mode that takes over when no audio has
  been present for some minutes (Signal present exists) and fades back when
  music starts.
- **Randomise with taste**: a dice button on a node that randomises its
  settings within sensible ranges, with undo. Great for finding looks you would
  not have dialled in.
