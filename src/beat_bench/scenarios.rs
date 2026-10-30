//! The test set: dance music in several shapes, played the way vinyl behaves.

use super::synth::{self, Curve, Genre, Kick, Section, TempoMap, Track, layer::*, pat, sec, SR};
use super::truth::Truth;

pub struct Generated {
    pub truth: Truth,
    pub audio: Vec<f32>,
}

pub struct Scenario {
    pub name: &'static str,
    pub build: fn() -> Generated,
}

pub fn all() -> Vec<Scenario> {
    macro_rules! s {
        ($name:literal, $f:expr) => {
            Scenario { name: $name, build: $f }
        };
    }
    vec![
        s!("house_124", house_124),
        s!("techno_132", techno_132),
        s!("breaks_130", breaks_130),
        s!("dnb_174", dnb_174),
        s!("halftime_140", halftime_140),
        s!("hiphop_90_swing", hiphop_90),
        s!("disco_116_swing", disco_116),
        s!("trance_138_breakdown", trance_138),
        s!("electro_128_syncopated", electro_128),
        s!("minimal_126_sparse", minimal_126),
        s!("house_122_vinyl", house_vinyl),
        s!("techno_134_vinyl_drift", techno_vinyl),
        s!("mix_tight", mix_tight),
        s!("mix_sloppy", mix_sloppy),
        s!("mix_trainwreck", mix_trainwreck),
        s!("mix_tempo_change", mix_tempo_change),
    ]
}

// ---------------------------------------------------------------------------
// Genres
// ---------------------------------------------------------------------------

const ZERO: [f32; 16] = [0.0; 16];

fn house() -> Genre {
    Genre {
        kick_kind: Kick::House,
        kick: pat("X...X...X...X..."),
        snare: ZERO,
        clap: pat("....X.......X..."),
        chat: pat("oxoxoxoxoxoxoxox"),
        ohat: pat("..x...x...x...x."),
        perc: pat("..o...o...o...o."),
        bass: vec![(2, 2.0), (6, 2.0), (10, 2.0), (14, 2.0)],
        swing: 0.0,
        root: 33.0,
        progression: [0, 0, -2, -5],
        sidechain: 0.6,
    }
}

fn techno() -> Genre {
    Genre {
        kick_kind: Kick::Techno,
        kick: pat("X...X...X...X..."),
        snare: ZERO,
        clap: pat("....x.......x..."),
        chat: pat("xoxoxoxoxoxoxoxo"),
        ohat: pat("..X...X...X...X."),
        perc: pat("...o..o...o..o.."),
        bass: vec![(2, 3.0), (6, 3.0), (10, 3.0), (14, 3.0)],
        swing: 0.0,
        root: 31.0,
        progression: [0, 0, 0, -2],
        sidechain: 0.7,
    }
}

fn breaks() -> Genre {
    Genre {
        kick_kind: Kick::Breaks,
        kick: pat("X.......x.X....."),
        snare: pat("....X..o....X..o"),
        clap: ZERO,
        chat: pat("x.x.x.x.x.x.x.x."),
        ohat: pat("......x.......x."),
        perc: pat("o..o..o.o..o.o.."),
        bass: vec![(0, 5.0), (10, 3.0)],
        swing: 0.08,
        root: 36.0,
        progression: [0, -3, -5, -7],
        sidechain: 0.4,
    }
}

fn dnb() -> Genre {
    Genre {
        kick_kind: Kick::Breaks,
        kick: pat("X.........x....."),
        snare: pat("....X.......X..."),
        clap: ZERO,
        chat: pat("x.x.x.x.x.x.x.x."),
        ohat: ZERO,
        perc: pat("..o...o...o...o."),
        bass: vec![(0, 6.0), (10, 5.0)],
        swing: 0.0,
        root: 34.0,
        progression: [0, -2, -5, -7],
        sidechain: 0.3,
    }
}

fn halftime() -> Genre {
    Genre {
        kick_kind: Kick::Techno,
        kick: pat("X.........x....."),
        snare: pat("........X......."),
        clap: ZERO,
        chat: pat("x.x.x.x.x.x.x.x."),
        ohat: ZERO,
        perc: pat("o...o...o...o..."),
        bass: vec![(0, 8.0), (10, 6.0)],
        swing: 0.0,
        root: 29.0,
        progression: [0, 0, -3, -5],
        sidechain: 0.5,
    }
}

fn hiphop() -> Genre {
    Genre {
        kick_kind: Kick::Breaks,
        kick: pat("X.....x...X.o..."),
        snare: pat("....X.......X..."),
        clap: ZERO,
        chat: pat("x.x.x.x.x.x.x.x."),
        ohat: pat("...............o"),
        perc: ZERO,
        bass: vec![(0, 6.0), (10, 4.0)],
        swing: 0.18,
        root: 36.0,
        progression: [0, 0, -2, -4],
        sidechain: 0.0,
    }
}

fn disco() -> Genre {
    Genre {
        kick_kind: Kick::House,
        kick: pat("X...X...X...X..."),
        snare: pat("....X.......X..."),
        clap: ZERO,
        chat: pat("o.x.o.x.o.x.o.x."),
        ohat: pat("..x...x...x...x."),
        perc: ZERO,
        bass: (0..8).map(|i| (i * 2, 2.0)).collect(),
        swing: 0.22,
        root: 40.0,
        progression: [0, 3, 5, 7],
        sidechain: 0.0,
    }
}

fn trance() -> Genre {
    Genre {
        kick_kind: Kick::House,
        kick: pat("X...X...X...X..."),
        snare: ZERO,
        clap: pat("....x.......x..."),
        chat: pat("oxoxoxoxoxoxoxox"),
        ohat: pat("..x...x...x...x."),
        perc: ZERO,
        bass: vec![(2, 2.0), (6, 2.0), (10, 2.0), (14, 2.0)],
        swing: 0.0,
        root: 34.0,
        progression: [0, -4, -2, -7],
        sidechain: 0.7,
    }
}

fn electro() -> Genre {
    Genre {
        kick_kind: Kick::House,
        kick: pat("X..x..x...x.x..."),
        snare: pat("....X.......X..."),
        clap: ZERO,
        chat: pat("x.x.x.x.x.x.x.x."),
        ohat: pat(".......x.......x"),
        perc: pat("..o..o..o..o..o."),
        bass: vec![(0, 3.0), (3, 3.0), (6, 3.0), (10, 2.0), (12, 3.0)],
        swing: 0.0,
        root: 35.0,
        progression: [0, 0, -2, -3],
        sidechain: 0.4,
    }
}

fn minimal() -> Genre {
    Genre {
        kick_kind: Kick::Techno,
        kick: pat("X.......X......."),
        snare: ZERO,
        clap: ZERO,
        chat: pat("..x...x...x...x."),
        ohat: ZERO,
        perc: pat("..o..x..o..x.o.."),
        bass: vec![(2, 2.0), (11, 3.0)],
        swing: 0.05,
        root: 33.0,
        progression: [0, 0, 0, -5],
        sidechain: 0.3,
    }
}

// ---------------------------------------------------------------------------
// Arrangements
// ---------------------------------------------------------------------------

const FULL: u32 = KICK | SNARE | CLAP | HATS | OHAT | PERC | BASS;

/// Intro, build, full groove, a kick-less breakdown with a snare roll, drop.
fn standard(bars: u32) -> Vec<Section> {
    let all = [
        sec(8, KICK | HATS),
        sec(8, KICK | HATS | OHAT | BASS),
        sec(8, FULL | PAD),
        sec(8, HATS | PAD | ARP),
        sec(4, HATS | PAD | ROLL),
        sec(16, FULL | PAD | ARP),
    ];
    truncate(&all, bars)
}

fn truncate(all: &[Section], bars: u32) -> Vec<Section> {
    let mut left = bars;
    let mut out = Vec::new();
    for s in all {
        if left == 0 {
            break;
        }
        let n = s.bars.min(left);
        out.push(sec(n, s.layers));
        left -= n;
    }
    out
}

fn bars_for(bpm: f64, seconds: f64) -> u32 {
    (seconds * bpm / 240.0).ceil() as u32 + 1
}

fn track(genre: Genre, curve: Curve, seconds: f64, seed: u64, arrangement: fn(u32) -> Vec<Section>) -> Track {
    let bars = bars_for(curve.base * 1.06, seconds);
    Track { genre, sections: arrangement(bars), curve, seed, vinyl: false }
}

// ---------------------------------------------------------------------------
// Single tracks
// ---------------------------------------------------------------------------

/// Render one track from its first beat for `seconds`.
fn single(name: &str, description: &str, tr: Track, seconds: f64) -> Generated {
    let map = TempoMap::build(seconds + 5.0, &tr.curve);
    let mut audio = synth::render(&tr, &map);
    audio.truncate((seconds * SR as f64) as usize);
    synth::master(&mut audio);

    let mut beats = Vec::new();
    let mut downbeats = Vec::new();
    let mut k = 0;
    while let Some(t) = map.time_of_beat(k as f64) {
        if t >= seconds {
            break;
        }
        beats.push(t);
        if k % 4 == 0 {
            downbeats.push(t);
        }
        k += 1;
    }
    let bpm = (0..)
        .map(|i| i as f64 * 0.5)
        .take_while(|t| *t < seconds)
        .map(|t| (t, tr.curve.bpm(t)))
        .collect();
    let no_kick = tr
        .no_kick_beats()
        .into_iter()
        .filter_map(|(a, b)| Some((map.time_of_beat(a)?, map.time_of_beat(b).unwrap_or(seconds))))
        .collect();

    Generated {
        truth: Truth {
            name: name.to_string(),
            description: description.to_string(),
            sample_rate: SR,
            duration_s: seconds,
            bpm,
            beats,
            downbeats,
            skip: Vec::new(),
            no_kick,
            tempo_range: None,
        },
        audio,
    }
}

const SINGLE_SECONDS: f64 = 80.0;

fn house_124() -> Generated {
    let t = track(house(), Curve::constant(124.0), SINGLE_SECONDS, 1, standard);
    single("house_124", "Four on the floor, off-beat hats and bass, clap on 2 and 4. Kick-less breakdown.", t, SINGLE_SECONDS)
}

fn techno_132() -> Generated {
    let t = track(techno(), Curve::constant(132.0), SINGLE_SECONDS, 2, standard);
    single("techno_132", "Long kicks, rolling hats, almost no harmonic movement.", t, SINGLE_SECONDS)
}

fn breaks_130() -> Generated {
    let t = track(breaks(), Curve::constant(130.0), SINGLE_SECONDS, 3, standard);
    single("breaks_130", "Broken beat: kick off the beat, ghost snares, no four on the floor.", t, SINGLE_SECONDS)
}

fn dnb_174() -> Generated {
    let t = track(dnb(), Curve::constant(174.0), SINGLE_SECONDS, 4, standard);
    let mut g = single("dnb_174", "Two-step drum and bass. Tempo octave risk: 87 vs 174; scored with a 150-190 range.", t, SINGLE_SECONDS);
    g.truth.tempo_range = Some((150.0, 190.0));
    g
}

fn halftime_140() -> Generated {
    let t = track(halftime(), Curve::constant(140.0), SINGLE_SECONDS, 5, standard);
    single("halftime_140", "Half-time feel: snare only on beat 3. Truth is 140; the feel is 70.", t, SINGLE_SECONDS)
}

fn hiphop_90() -> Generated {
    let t = track(hiphop(), Curve::constant(90.0), SINGLE_SECONDS, 6, standard);
    single("hiphop_90_swing", "Heavy swing, syncopated kick.", t, SINGLE_SECONDS)
}

fn disco_116() -> Generated {
    let t = track(disco(), Curve::constant(116.0), SINGLE_SECONDS, 7, standard);
    single("disco_116_swing", "Swung sixteenths, walking octave bass, four on the floor.", t, SINGLE_SECONDS)
}

fn trance_138() -> Generated {
    let arrangement = |bars: u32| {
        truncate(
            &[
                sec(8, KICK | HATS),
                sec(8, KICK | HATS | BASS | OHAT),
                sec(16, FULL | PAD | ARP),
                sec(16, PAD | ARP),
                sec(8, PAD | ARP | ROLL),
                sec(16, FULL | PAD | ARP),
            ],
            bars,
        )
    };
    let t = track(trance(), Curve::constant(138.0), SINGLE_SECONDS, 8, arrangement);
    single("trance_138_breakdown", "Sixteen-bar breakdown without any kick or percussion, then a roll and the drop.", t, SINGLE_SECONDS)
}

fn electro_128() -> Generated {
    let t = track(electro(), Curve::constant(128.0), SINGLE_SECONDS, 9, standard);
    single("electro_128_syncopated", "Kick not on every beat, so the beat has to come from elsewhere.", t, SINGLE_SECONDS)
}

fn minimal_126() -> Generated {
    let t = track(minimal(), Curve::constant(126.0), SINGLE_SECONDS, 10, standard);
    single("minimal_126_sparse", "Kick only on beats 1 and 3; the rest is sparse percussion.", t, SINGLE_SECONDS)
}

fn house_vinyl() -> Generated {
    let curve = Curve {
        base: 122.0,
        drift_pct_per_min: 1.5,
        wow: vec![(0.55, 0.15), (6.0, 0.04)],
        bumps: vec![(20.0, 6.0, 2.5), (47.0, 5.0, -2.0)],
        ..Default::default()
    };
    let mut t = track(house(), curve, SINGLE_SECONDS, 11, standard);
    t.vinyl = true;
    single("house_122_vinyl", "House on a record: crackle, rumble, motor wow, slow pitch drift, two nudges.", t, SINGLE_SECONDS)
}

fn techno_vinyl() -> Generated {
    let curve = Curve {
        base: 134.0,
        drift_pct_per_min: -3.0,
        wow: vec![(0.5, 0.2), (8.0, 0.05)],
        bumps: vec![(15.0, 8.0, 3.0), (40.0, 6.0, -3.0), (62.0, 6.0, 2.0)],
        ..Default::default()
    };
    let mut t = track(techno(), curve, SINGLE_SECONDS, 12, standard);
    t.vinyl = true;
    single("techno_134_vinyl_drift", "Techno on a record with a strong drift and three pitch nudges.", t, SINGLE_SECONDS)
}

// ---------------------------------------------------------------------------
// Mixes
// ---------------------------------------------------------------------------

struct MixSpec {
    name: &'static str,
    description: &'static str,
    a: Track,
    b: Track,
    /// Bar of track A on which track B's first beat is meant to land.
    b_at_bar: f64,
    /// How late B actually lands, in seconds.
    b_offset_s: f64,
    /// Pitch nudge that brings B back in time: (B-local start s, peak percent).
    correction: Option<(f64, f64)>,
    fade_from_bar: f64,
    fade_bars: f64,
    seconds: f64,
}

fn mix(spec: MixSpec) -> Generated {
    let seconds = spec.seconds;
    let a_map = TempoMap::build(seconds + 5.0, &spec.a.curve);

    let mut b_curve = spec.b.curve.clone();
    let mut corr_end = None;
    if let Some((start, pct)) = spec.correction {
        let pct = pct * spec.b_offset_s.signum();
        let dur = 2.0 * spec.b_offset_s.abs() / (pct.abs() / 100.0);
        b_curve.bumps.push((start, dur, pct));
        corr_end = Some(start + dur);
    }
    let b_map = TempoMap::build(seconds + 5.0, &b_curve);

    let a_audio = {
        let mut x = synth::render(&spec.a, &a_map);
        synth::master(&mut x);
        x
    };
    let b_audio = {
        let mut x = synth::render(&spec.b, &b_map);
        synth::master(&mut x);
        x
    };

    let b_start = a_map.time_of_beat(spec.b_at_bar * 4.0).unwrap_or(0.0) + spec.b_offset_s;
    let fade_start = a_map.time_of_beat(spec.fade_from_bar * 4.0).unwrap_or(0.0);
    let fade_end = a_map.time_of_beat((spec.fade_from_bar + spec.fade_bars) * 4.0).unwrap_or(seconds);
    let mid = 0.5 * (fade_start + fade_end);

    let n = (seconds * SR as f64) as usize;
    let b_off = (b_start * SR as f64).round() as usize;
    let mut out = vec![0.0f32; n];
    for i in 0..n {
        let t = i as f64 / SR as f64;
        let x = ((t - fade_start) / (fade_end - fade_start)).clamp(0.0, 1.0) as f32;
        let (ga, gb) = ((x * std::f32::consts::FRAC_PI_2).cos(), (x * std::f32::consts::FRAC_PI_2).sin());
        let a = a_audio.get(i).copied().unwrap_or(0.0);
        let b = if i >= b_off { b_audio.get(i - b_off).copied().unwrap_or(0.0) } else { 0.0 };
        out[i] = a * ga + b * gb;
    }
    synth::master(&mut out);

    // The dominant grid switches from A to B in the middle of the fade.
    let mut beats = Vec::new();
    let mut downbeats = Vec::new();
    let mut k = 0;
    while let Some(t) = a_map.time_of_beat(k as f64) {
        if t >= mid.min(seconds) {
            break;
        }
        beats.push(t);
        if k % 4 == 0 {
            downbeats.push(t);
        }
        k += 1;
    }
    let mut k = 0;
    while let Some(local) = b_map.time_of_beat(k as f64) {
        let t = b_start + local;
        if t >= seconds {
            break;
        }
        if t >= mid {
            beats.push(t);
            if k % 4 == 0 {
                downbeats.push(t);
            }
        }
        k += 1;
    }
    let bpm = (0..)
        .map(|i| i as f64 * 0.5)
        .take_while(|t| *t < seconds)
        .map(|t| {
            if t < mid { (t, spec.a.curve.bpm(t)) } else { (t, b_curve.bpm((t - b_start).max(0.0))) }
        })
        .collect();

    let mut skip = vec![(fade_start, fade_end, "both tracks audible".to_string())];
    if let Some(end) = corr_end {
        skip.push((b_start, b_start + end, "incoming track not yet in time".to_string()));
    }

    Generated {
        truth: Truth {
            name: spec.name.to_string(),
            description: spec.description.to_string(),
            sample_rate: SR,
            duration_s: seconds,
            bpm,
            beats,
            downbeats,
            skip,
            no_kick: Vec::new(),
            tempo_range: None,
        },
        audio: out,
    }
}

fn mix_pair(seconds: f64, a_seed: u64) -> (Track, Track) {
    let a = track(house(), Curve::constant(124.0), seconds, a_seed, |bars| {
        truncate(&[sec(8, KICK | HATS), sec(40, FULL | PAD | ARP), sec(8, KICK | HATS)], bars)
    });
    let b = track(techno(), Curve::constant(124.0), seconds, a_seed + 100, |bars| {
        truncate(&[sec(8, HATS | PERC), sec(8, KICK | HATS | BASS), sec(40, FULL | PAD)], bars)
    });
    (a, b)
}

fn mix_tight() -> Generated {
    let (a, b) = mix_pair(100.0, 20);
    mix(MixSpec {
        name: "mix_tight",
        description: "House into techno, both 124 BPM, 8 ms apart, 8-bar fade.",
        a, b,
        b_at_bar: 16.0, b_offset_s: 0.008, correction: None,
        fade_from_bar: 20.0, fade_bars: 8.0, seconds: 100.0,
    })
}

fn mix_sloppy() -> Generated {
    let (a, b) = mix_pair(100.0, 30);
    mix(MixSpec {
        name: "mix_sloppy",
        description: "Incoming track lands 150 ms late and is nudged back into time over about 10 s.",
        a, b,
        b_at_bar: 14.0, b_offset_s: 0.150, correction: Some((6.0, 3.0)),
        fade_from_bar: 22.0, fade_bars: 8.0, seconds: 100.0,
    })
}

fn mix_trainwreck() -> Generated {
    let (a, b) = mix_pair(100.0, 40);
    mix(MixSpec {
        name: "mix_trainwreck",
        description: "Incoming track runs half a beat late for 15 s, then is pulled back with a long nudge.",
        a, b,
        b_at_bar: 12.0, b_offset_s: 0.242, correction: Some((15.0, 4.0)),
        fade_from_bar: 28.0, fade_bars: 8.0, seconds: 100.0,
    })
}

fn mix_tempo_change() -> Generated {
    let (a, mut b) = mix_pair(100.0, 50);
    // B is pitched down to 124 for the mix, then released to 128.
    b.curve = Curve { base: 124.0, steps: vec![(40.0, 25.0, 128.0)], ..Default::default() };
    mix(MixSpec {
        name: "mix_tempo_change",
        description: "Mix at 124, then the incoming record is released to 128 BPM over 25 s.",
        a, b,
        b_at_bar: 16.0, b_offset_s: 0.004, correction: None,
        fade_from_bar: 20.0, fade_bars: 8.0, seconds: 100.0,
    })
}
