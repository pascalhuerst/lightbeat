<p align="center">
  <img src="assets/wordmark-1024.png" alt="LightBeat" width="640"/>
</p>

<p align="center">
  Beat-driven lighting control for clubs and live music.<br/>
  Node-graph based, written in Rust.
</p>

---

LightBeat is a tool to control DMX lights based on the **structure of the music**
(beats, phrases, energy) rather than the timeline of a cue list. The traditional
lighting world has its roots in theatre, where every cue is hand-fired against
a script. That doesn't fit a club where the dancefloor is the script — so this
is a clean-slate take aimed at venues, not stages.

Under the hood it's a node graph: tempo and audio sources flow into pattern,
gradient, palette, and trigger nodes, then out to ArtNet/sACN universes. Every
node runs on a real-time engine; the UI is decoupled and never blocks the
output thread.

## Download and build

Beta builds for Linux (x86_64) and macOS (Apple Silicon) are on the
[releases page](../../releases): the rolling **beta** release follows `main`,
and tags such as `v0.2.0-beta.1` make releases of their own. They are built by
`.github/workflows/beta.yml`; `ci.yml` runs the tests on both systems.

- **Linux:** unpack the tarball and run `./lightbeat`. It needs the ALSA and
  udev libraries; see `README.txt` inside, which also covers the X1 udev rule.
- **macOS:** unzip and open `LightBeat.app`. The app is signed ad hoc, not
  notarized, so the first time right-click it and choose Open (or run
  `xattr -dr com.apple.quarantine LightBeat.app`). It asks for microphone
  access (audio inputs) and for local network access (ArtNet, sACN, Link).
- **Your files** (settings, setup, the default project) live in
  `~/.config/lightbeat` on Linux and `~/Library/Application Support/LightBeat`
  on macOS. Started from a directory that already has a `settings.json`,
  `setup.json` or `project.json` (a checkout), it uses that directory instead.
  `LIGHTBEAT_HOME` overrides both.

To build from source you need a recent stable Rust, `cmake` and `libclang`
(for Ableton Link), and on Linux the ALSA and JACK development packages
(`libasound2-dev libjack-jackd2-dev libudev-dev`, plus the X11/Wayland
packages listed in the workflow). `cargo build --release --features
vendored-usb` links libusb in, as the releases do.

## Assets

`assets/` contains:

- `logo.svg` — the square icon source (continuous sine morphing into discrete
  beat pulses, on a dark gray rounded square).
- `wordmark.svg` — horizontal variant with the "LightBeat" wordmark next to
  the symbol. Use this for README headers, docs banners, and social cards.
- `icon-{16,32,48,64,128,256,512,1024}.png` — pre-rendered square icons.
  Smaller sizes for favicons / window icons, larger ones for app packaging.
- `wordmark-{256,512,1024,2048}.png` — pre-rendered banners (4:1).
