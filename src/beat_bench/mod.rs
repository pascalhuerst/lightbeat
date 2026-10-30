//! Offline test bench for beat tracking.
//!
//! `lightbeat bench generate [dir] [name]` writes synthetic dance-music test
//! files (WAV plus the exact beat grid as JSON). `lightbeat bench score [dir]
//! [name]` pushes them through the trackers and prints how well they did.
//! No window is opened. Nothing here is used by the app itself.

mod metrics;
mod scenarios;
mod synth;
mod truth;

use std::path::{Path, PathBuf};

use crate::audio::analyzers::{AnalyzerProc, beat};

use metrics::TrackerRun;
use truth::Truth;

const DEFAULT_DIR: &str = "beat_bench_data";

/// Runs the bench if the command line asks for it. Returns true when it did.
pub fn run_if_requested() -> bool {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) != Some("bench") {
        return false;
    }
    let dir = PathBuf::from(args.get(3).map(String::as_str).unwrap_or(DEFAULT_DIR));
    let filter = args.get(4).map(String::as_str);
    match args.get(2).map(String::as_str) {
        Some("generate") => generate(&dir, filter),
        Some("score") => score(&dir, filter),
        _ => eprintln!("usage: lightbeat bench <generate|score> [dir] [name filter]"),
    }
    true
}

fn generate(dir: &Path, filter: Option<&str>) {
    std::fs::create_dir_all(dir).expect("create output directory");
    for sc in scenarios::all().into_iter().filter(|s| matches(s.name, filter)) {
        let g = (sc.build)();
        let wav = dir.join(format!("{}.wav", sc.name));
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: synth::SR,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::create(&wav, spec).expect("create wav");
        for s in &g.audio {
            w.write_sample((s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16).expect("write");
        }
        w.finalize().expect("finalize wav");
        let json = serde_json::to_string_pretty(&g.truth).expect("serialize truth");
        std::fs::write(dir.join(format!("{}.json", sc.name)), json).expect("write truth");
        println!("{:<26} {:>5.0} s  {}", sc.name, g.truth.duration_s, g.truth.description);
    }
}

fn matches(name: &str, filter: Option<&str>) -> bool {
    filter.is_none_or(|f| name.contains(f))
}

fn load(dir: &Path, name: &str) -> Option<(Truth, Vec<f32>)> {
    let truth: Truth = serde_json::from_str(&std::fs::read_to_string(dir.join(format!("{name}.json"))).ok()?).ok()?;
    let mut reader = hound::WavReader::open(dir.join(format!("{name}.wav"))).ok()?;
    let spec = reader.spec();
    let channels = spec.channels as usize;
    let interleaved: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().filter_map(Result::ok).collect(),
        hound::SampleFormat::Int => {
            let scale = 1.0 / (1i64 << (spec.bits_per_sample - 1)) as f32;
            reader.samples::<i32>().filter_map(Result::ok).map(|s| s as f32 * scale).collect()
        }
    };
    let mono = interleaved.chunks(channels).map(|c| c.iter().sum::<f32>() / channels as f32).collect();
    Some((truth, mono))
}

/// The current `Beat` analyzer, driven the way the audio worker drives it.
fn run_current_analyzer(audio: &[f32], sr: u32) -> TrackerRun {
    const CHUNK: usize = 256;
    let (_handle, mut proc) = beat::create(sr);
    let latency = proc.output_latency_samples() as f64;
    let mut run = TrackerRun::default();
    let mut seen = 0u64;
    for (i, chunk) in audio.chunks(CHUNK).enumerate() {
        proc.step(chunk);
        let t = ((i + 1) * CHUNK) as f64 - latency;
        let t = t / sr as f64;
        run.tempo.push((t, proc.outputs().get(1).copied().unwrap_or(0.0) as f64));
        let count = proc.onset_count();
        for _ in seen..count {
            run.onsets.push(t);
        }
        seen = count;
    }
    run
}

fn score(dir: &Path, filter: Option<&str>) {
    println!("Current Beat analyzer (SuperFlux onsets + autocorrelation BPM), {}", dir.display());
    println!(
        "{:<26} {:>7} {:>7} {:>6} {:>6} {:>7} {:>7} {:>8} {:>8}",
        "scenario", "true", "ratio", "acc1", "acc2", "lock s", "on/beat", "beat-hit", "on-grid"
    );
    let mut names: Vec<String> = scenarios::all().iter().map(|s| s.name.to_string()).collect();
    names.retain(|n| matches(n, filter));
    for name in names {
        let Some((truth, audio)) = load(dir, &name) else {
            println!("{name:<26} (missing; run `bench generate` first)");
            continue;
        };
        let run = run_current_analyzer(&audio, truth.sample_rate);
        let t = metrics::score_tempo(&truth, &run);
        let o = metrics::score_onsets(&truth, &run);
        let mid_bpm = truth.bpm_at(truth.duration_s * 0.5);
        println!(
            "{:<26} {:>7.1} {:>7.2} {:>5.0}% {:>5.0}% {:>7} {:>7.1} {:>7.0}% {:>7.0}%",
            name,
            mid_bpm,
            t.median_ratio,
            t.acc1 * 100.0,
            t.acc2 * 100.0,
            t.lock_s.map(|s| format!("{s:.1}")).unwrap_or_else(|| "never".into()),
            o.onsets_per_beat,
            o.beat_hit * 100.0,
            o.on_grid * 100.0,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use synth::{Curve, TempoMap};

    #[test]
    fn tempo_map_places_beats_where_the_tempo_says() {
        let map = TempoMap::build(20.0, &Curve::constant(120.0));
        assert!((map.time_of_beat(4.0).unwrap() - 2.0).abs() < 1e-6);
        assert!((map.beat_at(2.0) - 4.0).abs() < 1e-6);
    }

    #[test]
    fn tempo_map_round_trips_on_a_drifting_tempo() {
        let curve = Curve { base: 124.0, drift_pct_per_min: 3.0, wow: vec![(0.5, 0.2)], ..Default::default() };
        let map = TempoMap::build(60.0, &curve);
        for beat in [1.0, 17.5, 60.0, 100.25] {
            let t = map.time_of_beat(beat).unwrap();
            assert!((map.beat_at(t) - beat).abs() < 1e-4, "beat {beat}");
        }
    }

    #[test]
    fn generated_truth_matches_the_tempo() {
        let g = (scenarios::all().into_iter().find(|s| s.name == "house_124").unwrap().build)();
        assert_eq!(g.audio.len(), (g.truth.duration_s * synth::SR as f64) as usize);
        let gap = g.truth.beats[11] - g.truth.beats[10];
        assert!((gap - 60.0 / 124.0).abs() < 1e-6);
        assert_eq!(g.truth.downbeats[1], g.truth.beats[4]);
        assert!(g.audio.iter().any(|s| s.abs() > 0.3), "the signal is not silent");
        assert!(g.audio.iter().all(|s| s.abs() <= 1.0));
    }

    #[test]
    fn a_mix_switches_grids_at_the_middle_of_the_fade() {
        let g = (scenarios::all().into_iter().find(|s| s.name == "mix_sloppy").unwrap().build)();
        // Beats stay monotonic across the hand-over, and bars stay on beats.
        assert!(g.truth.beats.windows(2).all(|w| w[1] > w[0]));
        assert!(g.truth.downbeats.iter().all(|d| g.truth.beats.contains(d)));
    }

    #[test]
    fn perfect_tempo_scores_full_marks_and_locks_immediately() {
        let g = (scenarios::all().into_iter().find(|s| s.name == "house_124").unwrap().build)();
        let run = TrackerRun {
            tempo: (0..800).map(|i| (i as f64 * 0.1, 124.0)).collect(),
            onsets: Vec::new(),
        };
        let s = metrics::score_tempo(&g.truth, &run);
        assert_eq!(s.acc1, 1.0);
        assert!(s.lock_s.unwrap() < 0.01);
        assert!((s.median_ratio - 1.0).abs() < 1e-9);
    }

    #[test]
    fn a_half_tempo_estimate_counts_for_acc2_but_not_acc1() {
        let g = (scenarios::all().into_iter().find(|s| s.name == "house_124").unwrap().build)();
        let run = TrackerRun {
            tempo: (0..800).map(|i| (i as f64 * 0.1, 62.0)).collect(),
            onsets: Vec::new(),
        };
        let s = metrics::score_tempo(&g.truth, &run);
        assert_eq!(s.acc1, 0.0);
        assert_eq!(s.acc2, 1.0);
    }
}
