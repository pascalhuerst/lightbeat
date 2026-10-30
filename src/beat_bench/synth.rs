//! A small drum-machine style synthesizer for making test material.
//!
//! The point is not to sound good but to be musically *shaped* like dance
//! music: kick, clap/snare, hats, bass that changes with a chord progression,
//! pads, sections with and without kick, fills at phrase ends, swing, and
//! sidechain pumping. Everything is placed on a tempo map that can drift and
//! wobble like a record, so the exact beat grid is known.

use std::f32::consts::TAU;

pub const SR: u32 = 44_100;
const SRF: f32 = SR as f32;

// ---------------------------------------------------------------------------
// Randomness
// ---------------------------------------------------------------------------

pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    /// Uniform in [-1, 1).
    pub fn bipolar(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1u64 << 23) as f32 - 1.0
    }
    /// Uniform in [0, 1).
    pub fn unit(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1u64 << 24) as f32
    }
}

// ---------------------------------------------------------------------------
// Tempo
// ---------------------------------------------------------------------------

/// A tempo that changes the way a turntable's does.
#[derive(Clone, Default)]
pub struct Curve {
    pub base: f64,
    /// Slow pitch-fader drift, percent per minute.
    pub drift_pct_per_min: f64,
    /// (frequency Hz, depth percent): motor wow and flutter.
    pub wow: Vec<(f64, f64)>,
    /// Pitch nudges: (start s, duration s, peak percent). Raised-cosine shaped.
    pub bumps: Vec<(f64, f64, f64)>,
    /// Tempo changes: (start s, ramp s, new bpm).
    pub steps: Vec<(f64, f64, f64)>,
}

impl Curve {
    pub fn constant(bpm: f64) -> Self {
        Self { base: bpm, ..Default::default() }
    }

    pub fn bpm(&self, t: f64) -> f64 {
        let mut base = self.base;
        for &(start, ramp, to) in &self.steps {
            let x = ((t - start) / ramp.max(1e-6)).clamp(0.0, 1.0);
            let s = x * x * (3.0 - 2.0 * x);
            base += (to - base) * s;
        }
        let mut pct = self.drift_pct_per_min * t / 60.0;
        for &(f, depth) in &self.wow {
            pct += depth * (std::f64::consts::TAU * f * t).sin();
        }
        for &(start, dur, peak) in &self.bumps {
            if t >= start && t < start + dur {
                let x = (t - start) / dur;
                pct += peak * 0.5 * (1.0 - (std::f64::consts::TAU * x).cos());
            }
        }
        base * (1.0 + pct / 100.0)
    }
}

/// Beat position as a function of time, and back.
pub struct TempoMap {
    step: f64,
    beats: Vec<f64>,
}

impl TempoMap {
    pub fn build(duration_s: f64, curve: &Curve) -> Self {
        let step = 0.005;
        let n = (duration_s / step) as usize + 2;
        let mut beats = Vec::with_capacity(n);
        let mut acc = 0.0;
        let mut prev = curve.bpm(0.0);
        beats.push(0.0);
        for i in 1..n {
            let cur = curve.bpm(i as f64 * step);
            acc += 0.5 * (prev + cur) / 60.0 * step;
            beats.push(acc);
            prev = cur;
        }
        Self { step, beats }
    }

    pub fn duration(&self) -> f64 {
        (self.beats.len() - 1) as f64 * self.step
    }

    pub fn beat_at(&self, t: f64) -> f64 {
        let x = (t / self.step).clamp(0.0, (self.beats.len() - 1) as f64);
        let i = (x as usize).min(self.beats.len() - 2);
        let f = x - i as f64;
        self.beats[i] + (self.beats[i + 1] - self.beats[i]) * f
    }

    pub fn time_of_beat(&self, beat: f64) -> Option<f64> {
        if beat < 0.0 || beat > *self.beats.last()? {
            return None;
        }
        let i = self.beats.partition_point(|&b| b <= beat).saturating_sub(1);
        let i = i.min(self.beats.len() - 2);
        let span = self.beats[i + 1] - self.beats[i];
        let f = if span > 0.0 { (beat - self.beats[i]) / span } else { 0.0 };
        Some((i as f64 + f) * self.step)
    }
}

// ---------------------------------------------------------------------------
// Instruments (one-shot sample banks)
// ---------------------------------------------------------------------------

fn expd(t: f32, tau: f32) -> f32 {
    (-t / tau).exp()
}

fn highpass(buf: &mut [f32], a: f32) {
    let (mut y, mut x1) = (0.0f32, 0.0f32);
    for s in buf.iter_mut() {
        y = a * (y + *s - x1);
        x1 = *s;
        *s = y;
    }
}

fn lowpass(buf: &mut [f32], a: f32) {
    let mut y = 0.0f32;
    for s in buf.iter_mut() {
        y += a * (*s - y);
        *s = y;
    }
}

#[derive(Clone, Copy)]
pub enum Kick {
    /// Short and punchy.
    House,
    /// Long, with a tail the bass sits under.
    Techno,
    /// Thin and clicky.
    Breaks,
}

pub struct Bank {
    pub kick: Vec<f32>,
    pub snare: Vec<f32>,
    pub clap: Vec<f32>,
    pub chat: Vec<f32>,
    pub ohat: Vec<f32>,
    pub rim: Vec<f32>,
    pub shaker: Vec<f32>,
    pub crash: Vec<f32>,
}

impl Bank {
    pub fn new(kick: Kick, seed: u64) -> Self {
        let mut rng = Rng::new(seed);
        Self {
            kick: make_kick(kick),
            snare: make_snare(&mut rng),
            clap: make_clap(&mut rng),
            chat: make_hat(&mut rng, 0.03, 0.12),
            ohat: make_hat(&mut rng, 0.16, 0.4),
            rim: make_rim(&mut rng),
            shaker: make_hat(&mut rng, 0.05, 0.2),
            crash: make_hat(&mut rng, 0.9, 1.4),
        }
    }
}

fn make_kick(kind: Kick) -> Vec<f32> {
    let (len, f0, f1, sweep, decay, click) = match kind {
        Kick::House => (0.35, 55.0, 150.0, 0.035, 0.16, 0.25),
        Kick::Techno => (0.55, 48.0, 160.0, 0.04, 0.30, 0.15),
        Kick::Breaks => (0.25, 70.0, 200.0, 0.02, 0.09, 0.45),
    };
    let n = (len * SRF) as usize;
    let mut phase = 0.0f32;
    (0..n)
        .map(|i| {
            let t = i as f32 / SRF;
            let f = f0 + (f1 - f0) * expd(t, sweep);
            phase += TAU * f / SRF;
            phase.sin() * expd(t, decay) + click * expd(t, 0.002) * (t * 3_000.0 * TAU).sin()
        })
        .collect()
}

fn make_snare(rng: &mut Rng) -> Vec<f32> {
    let n = (0.3 * SRF) as usize;
    let mut noise: Vec<f32> = (0..n).map(|_| rng.bipolar()).collect();
    highpass(&mut noise, 0.8);
    (0..n)
        .map(|i| {
            let t = i as f32 / SRF;
            noise[i] * 0.8 * expd(t, 0.08) + (t * 190.0 * TAU).sin() * 0.6 * expd(t, 0.05)
        })
        .collect()
}

fn make_clap(rng: &mut Rng) -> Vec<f32> {
    let n = (0.3 * SRF) as usize;
    let mut noise: Vec<f32> = (0..n).map(|_| rng.bipolar()).collect();
    highpass(&mut noise, 0.7);
    lowpass(&mut noise, 0.6);
    (0..n)
        .map(|i| {
            let t = i as f32 / SRF;
            let bursts: f32 = (0..3).map(|b| expd((t - b as f32 * 0.011).max(0.0), 0.006) * (t >= b as f32 * 0.011) as u8 as f32).sum();
            noise[i] * (bursts * 0.5 + expd(t, 0.09) * 0.5)
        })
        .collect()
}

fn make_hat(rng: &mut Rng, decay: f32, len: f32) -> Vec<f32> {
    let n = (len * SRF) as usize;
    let mut noise: Vec<f32> = (0..n).map(|_| rng.bipolar()).collect();
    highpass(&mut noise, 0.93);
    noise.iter().enumerate().map(|(i, s)| s * expd(i as f32 / SRF, decay)).collect()
}

fn make_rim(rng: &mut Rng) -> Vec<f32> {
    let n = (0.1 * SRF) as usize;
    (0..n)
        .map(|i| {
            let t = i as f32 / SRF;
            ((t * 1_700.0 * TAU).sin() * 0.7 + rng.bipolar() * 0.3) * expd(t, 0.012)
        })
        .collect()
}

pub fn midi_hz(note: f32) -> f32 {
    440.0 * 2f32.powf((note - 69.0) / 12.0)
}

/// A bass note: filtered saw plus sub, snappy envelope.
fn bass_note(freq: f32, len: usize) -> Vec<f32> {
    let mut phase = 0.0f32;
    let mut sub = 0.0f32;
    let mut out: Vec<f32> = (0..len)
        .map(|i| {
            let t = i as f32 / SRF;
            phase = (phase + freq / SRF).fract();
            sub += TAU * freq / SRF;
            let env = (1.0 - expd(t, 0.004)) * expd(t, 0.25).max(0.35) * (1.0 - (i as f32 / len as f32).powi(8));
            ((phase * 2.0 - 1.0) * 0.5 + sub.sin() * 0.8) * env
        })
        .collect();
    lowpass(&mut out, 0.12);
    out
}

/// A chord: detuned saws with a slow attack and a low cutoff.
fn pad_chord(freqs: &[f32], len: usize) -> Vec<f32> {
    let mut phases = vec![0.0f32; freqs.len() * 2];
    let mut out: Vec<f32> = (0..len)
        .map(|i| {
            let t = i as f32 / SRF;
            let mut s = 0.0;
            for (k, &f) in freqs.iter().enumerate() {
                for (d, det) in [0.997f32, 1.003].iter().enumerate() {
                    let ph = &mut phases[k * 2 + d];
                    *ph = (*ph + f * det / SRF).fract();
                    s += *ph * 2.0 - 1.0;
                }
            }
            let env = (1.0 - expd(t, 0.15)) * (1.0 - (i as f32 / len as f32).powi(6));
            s * env / freqs.len() as f32
        })
        .collect();
    lowpass(&mut out, 0.07);
    out
}

fn pluck(freq: f32, len: usize) -> Vec<f32> {
    let mut phase = 0.0f32;
    let mut out: Vec<f32> = (0..len)
        .map(|i| {
            phase = (phase + freq / SRF).fract();
            (phase * 2.0 - 1.0) * expd(i as f32 / SRF, 0.08)
        })
        .collect();
    lowpass(&mut out, 0.2);
    out
}

// ---------------------------------------------------------------------------
// Patterns
// ---------------------------------------------------------------------------

/// 16 steps. 'X' accent, 'x' normal, 'o' ghost, '.' rest.
pub fn pat(s: &str) -> [f32; 16] {
    let mut out = [0.0; 16];
    for (i, c) in s.chars().filter(|c| !c.is_whitespace() && *c != '|').take(16).enumerate() {
        out[i] = match c {
            'X' => 1.0,
            'x' => 0.75,
            'o' => 0.4,
            _ => 0.0,
        };
    }
    out
}

pub mod layer {
    pub const KICK: u32 = 1;
    pub const SNARE: u32 = 2;
    pub const CLAP: u32 = 4;
    pub const HATS: u32 = 8;
    pub const OHAT: u32 = 16;
    pub const PERC: u32 = 32;
    pub const BASS: u32 = 64;
    pub const PAD: u32 = 128;
    pub const ARP: u32 = 256;
    /// Snare roll that tightens towards the end of the section.
    pub const ROLL: u32 = 512;
    pub const GROOVE: u32 = KICK | CLAP | HATS | OHAT | PERC | BASS;
}

#[derive(Clone)]
pub struct Genre {
    pub kick_kind: Kick,
    pub kick: [f32; 16],
    pub snare: [f32; 16],
    pub clap: [f32; 16],
    pub chat: [f32; 16],
    pub ohat: [f32; 16],
    pub perc: [f32; 16],
    /// (step, length in steps) of bass notes in a bar.
    pub bass: Vec<(usize, f32)>,
    /// Fraction of a sixteenth by which off-beat sixteenths are delayed.
    pub swing: f32,
    /// Root note and per-bar semitone offsets: a harmonic change every bar.
    pub root: f32,
    pub progression: [i32; 4],
    pub sidechain: f32,
}

#[derive(Clone)]
pub struct Section {
    pub bars: u32,
    pub layers: u32,
}

pub fn sec(bars: u32, layers: u32) -> Section {
    Section { bars, layers }
}

#[derive(Clone)]
pub struct Track {
    pub genre: Genre,
    pub sections: Vec<Section>,
    pub curve: Curve,
    pub seed: u64,
    /// Vinyl surface noise, rumble and hiss.
    pub vinyl: bool,
}

impl Track {
    pub fn bars(&self) -> u32 {
        self.sections.iter().map(|s| s.bars).sum()
    }

    /// Time spans in which no section has a kick, in beats from the start.
    pub fn no_kick_beats(&self) -> Vec<(f64, f64)> {
        let mut out = Vec::new();
        let mut bar = 0;
        for s in &self.sections {
            if s.layers & layer::KICK == 0 {
                out.push((bar as f64 * 4.0, (bar + s.bars) as f64 * 4.0));
            }
            bar += s.bars;
        }
        out
    }
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

fn add(buf: &mut [f32], at: usize, sample: &[f32], gain: f32) {
    if at >= buf.len() {
        return;
    }
    let n = sample.len().min(buf.len() - at);
    for i in 0..n {
        buf[at + i] += sample[i] * gain;
    }
}

/// Render a track on its own timeline (beat 0 at sample 0).
pub fn render(track: &Track, map: &TempoMap) -> Vec<f32> {
    let total_beats = track.bars() as f64 * 4.0;
    let len = (map.time_of_beat(total_beats).unwrap_or(map.duration()) * SRF as f64) as usize
        + SR as usize;
    let g = &track.genre;
    let bank = Bank::new(g.kick_kind, track.seed);
    let mut rng = Rng::new(track.seed ^ 0xABCD);

    let mut drums = vec![0.0f32; len];
    let mut tonal = vec![0.0f32; len];
    let mut kicks: Vec<usize> = Vec::new();

    let at_beat = |beat: f64| -> Option<usize> {
        map.time_of_beat(beat).map(|t| (t * SRF as f64).round() as usize)
    };

    let mut bar_index = 0u32;
    for (si, section) in track.sections.iter().enumerate() {
        for b in 0..section.bars {
            let abs_bar = bar_index + b;
            let l = section.layers;
            let last_of_phrase = abs_bar % 8 == 7;
            let chord = g.progression[(abs_bar % 4) as usize];
            for step in 0..16usize {
                let swing = if step % 2 == 1 { g.swing as f64 * 0.25 } else { 0.0 };
                let beat = abs_bar as f64 * 4.0 + step as f64 * 0.25 + swing;
                let Some(at) = at_beat(beat) else { continue };

                if l & layer::KICK != 0 && g.kick[step] > 0.0 {
                    add(&mut drums, at, &bank.kick, g.kick[step]);
                    kicks.push(at);
                }
                if l & layer::SNARE != 0 && g.snare[step] > 0.0 {
                    add(&mut drums, at, &bank.snare, 0.7 * g.snare[step]);
                }
                if l & layer::CLAP != 0 && g.clap[step] > 0.0 {
                    add(&mut drums, at, &bank.clap, 0.6 * g.clap[step]);
                }
                if l & layer::HATS != 0 && g.chat[step] > 0.0 {
                    add(&mut drums, at, &bank.chat, 0.28 * g.chat[step]);
                }
                if l & layer::OHAT != 0 && g.ohat[step] > 0.0 {
                    add(&mut drums, at, &bank.ohat, 0.3 * g.ohat[step]);
                }
                if l & layer::PERC != 0 && g.perc[step] > 0.0 {
                    let s = if step % 4 == 3 { &bank.rim } else { &bank.shaker };
                    add(&mut drums, at, s, 0.35 * g.perc[step]);
                }
                // The fill at the end of every eighth bar marks the phrase.
                if last_of_phrase && step >= 12 && l & (layer::SNARE | layer::CLAP) != 0 {
                    add(&mut drums, at, &bank.snare, 0.6 + 0.1 * (step - 12) as f32);
                }
                if l & layer::ROLL != 0 {
                    // Eighths, then sixteenths, then 32nds in the last bars.
                    let pos = abs_bar - bar_index;
                    let from_end = section.bars - 1 - pos;
                    let density = if from_end >= 2 { 4 } else if from_end == 1 { 2 } else { 1 };
                    if step % density == 0 {
                        add(&mut drums, at, &bank.snare, 0.35 + 0.1 * (pos as f32 / section.bars as f32));
                    }
                }

                if l & layer::BASS != 0 {
                    for &(bs, blen) in &g.bass {
                        if bs == step {
                            let note = g.root + chord as f32;
                            let end = at_beat(beat + blen as f64 * 0.25).unwrap_or(at + 2000);
                            let n = bass_note(midi_hz(note), end.saturating_sub(at).max(400));
                            add(&mut tonal, at, &n, 0.7);
                        }
                    }
                }
                if l & layer::ARP != 0 && step % 2 == 0 {
                    let notes = [0, 3, 7, 10];
                    let note = g.root + 24.0 + chord as f32 + notes[(step / 2 + abs_bar as usize) % 4] as f32;
                    add(&mut tonal, at, &pluck(midi_hz(note), 8_000), 0.18);
                }
            }
            if l & layer::PAD != 0 {
                let beat = abs_bar as f64 * 4.0;
                if let (Some(a), Some(z)) = (at_beat(beat), at_beat(beat + 4.0)) {
                    let r = g.root + 12.0 + chord as f32;
                    let freqs = [midi_hz(r), midi_hz(r + 3.0), midi_hz(r + 7.0), midi_hz(r + 10.0)];
                    add(&mut tonal, a, &pad_chord(&freqs, z - a), 0.25);
                }
            }
            if b == 0 && si > 0 {
                if let Some(at) = at_beat(abs_bar as f64 * 4.0) {
                    add(&mut drums, at, &bank.crash, 0.35);
                }
            }
        }
        bar_index += section.bars;
    }

    // Sidechain: tonal layers duck on every kick.
    if g.sidechain > 0.0 {
        let mut duck = vec![1.0f32; len];
        for &k in &kicks {
            for i in 0..(0.35 * SRF) as usize {
                if k + i >= len {
                    break;
                }
                let d = 1.0 - g.sidechain * expd(i as f32 / SRF, 0.09);
                duck[k + i] = duck[k + i].min(d);
            }
        }
        for (t, d) in tonal.iter_mut().zip(&duck) {
            *t *= d;
        }
    }

    let mut out: Vec<f32> = drums.iter().zip(&tonal).map(|(a, b)| a + b).collect();

    if track.vinyl {
        add_vinyl(&mut out, &mut rng);
    }
    out
}

fn add_vinyl(buf: &mut [f32], rng: &mut Rng) {
    let n = buf.len();
    // Hiss.
    for s in buf.iter_mut() {
        *s += rng.bipolar() * 0.004;
    }
    // Rumble.
    let mut rumble: Vec<f32> = (0..n).map(|_| rng.bipolar()).collect();
    lowpass(&mut rumble, 0.004);
    for (s, r) in buf.iter_mut().zip(&rumble) {
        *s += r * 0.9;
    }
    // Crackle: sparse clicks, about thirty a second.
    let clicks = (n as f32 / SRF * 30.0) as usize;
    for _ in 0..clicks {
        let at = (rng.unit() * n as f32) as usize;
        let amp = (rng.unit().powi(3) * 0.12 + 0.01) * rng.bipolar().signum();
        for i in 0..40 {
            if at + i < n {
                buf[at + i] += amp * expd(i as f32, 6.0);
            }
        }
    }
}

/// Scale to a sensible peak and round off the loudest transients.
pub fn master(buf: &mut [f32]) {
    let peak = buf.iter().fold(0.0f32, |m, s| m.max(s.abs())).max(1e-6);
    let g = 0.9 / peak;
    for s in buf.iter_mut() {
        *s = (*s * g * 1.4).tanh() * 0.7;
    }
}
