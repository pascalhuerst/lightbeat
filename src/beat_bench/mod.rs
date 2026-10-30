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

use crate::audio::tracker::BeatTracker;

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

/// Push the file through the tracker in small chunks, as the audio worker would.
fn run_tracker(audio: &[f32], sr: u32, range: Option<(f64, f64)>) -> TrackerRun {
    const CHUNK: usize = 256;
    let mut tracker = match range {
        Some((lo, hi)) => BeatTracker::with_range(sr, lo, hi),
        None => BeatTracker::new(sr),
    };
    let mut run = TrackerRun::default();
    for (i, chunk) in audio.chunks(CHUNK).enumerate() {
        tracker.push(chunk);
        let t = ((i + 1) * CHUNK).min(audio.len()) as f64 / sr as f64;
        let state = tracker.state();
        run.tempo.push((t, state.tempo_bpm));
        run.phase.push((t, state.beat_phase));
        run.bar.push((t, state.bar_position));
    }
    if std::env::var("BENCH_BAR_DEBUG").is_ok() {
        let (scores, chosen) = tracker.bar_scores();
        let fmt: Vec<String> = scores.iter().map(|s| format!("{s:.2}")).collect();
        eprintln!("  final scores [{}] chosen {chosen} lock phase {:.2}", fmt.join(" "), tracker.lock_phase());
    }
    run
}

fn score_tracker(dir: &Path, filter: Option<&str>) {
    println!("Beat tracker, {}", dir.display());
    println!(
        "{:<26} {:>7} {:>6} {:>5} {:>6} {:>6} {:>8} {:>7} {:>8} {:>6} {:>6}",
        "scenario", "true", "tempo", "ratio", "lock s", "beatF1", "clock<50", "offset", "jitter95", "lock s", "bar<50"
    );
    println!(
        "{:<26} {:>7} {:>6} {:>5} {:>6} {:>6} {:>8} {:>7} {:>8} {:>6} {:>6}",
        "", "bpm", "acc1", "", "tempo", "", "ms", "ms", "ms", "clock", ""
    );
    for name in scenario_names(filter) {
        let Some((truth, audio)) = load(dir, &name) else {
            println!("{name:<26} (missing; run `bench generate` first)");
            continue;
        };
        let run = run_tracker(&audio, truth.sample_rate, truth.tempo_range);
        let t = metrics::score_tempo(&truth, &run);
        let p = metrics::score_phase(&truth, &run);
        let bar = metrics::score_bar(&truth, &run);
        if std::env::var("BENCH_BAR_DEBUG").is_ok() {
            let w: Vec<String> = metrics::bar_by_window(&truth, &run).iter().map(|(t, f)| format!("{t:.0}s:{:.0}%", f * 100.0)).collect();
            eprintln!("{name}: bar ok per 10 s: {}", w.join(" "));
            eprintln!("{name}: bar error histogram (beats, quarter-beat bins): {:?}", metrics::bar_error_histogram(&truth, &run));
        }
        let secs = |v: Option<f64>| v.map(|s| format!("{s:.1}")).unwrap_or_else(|| "never".into());
        println!(
            "{:<26} {:>7.1} {:>5.0}% {:>5.2} {:>6} {:>6.2} {:>7.0}% {:>+7.0} {:>8.0} {:>6} {:>5.0}%",
            name,
            truth.bpm_at(truth.duration_s * 0.5),
            t.acc1 * 100.0,
            t.median_ratio,
            secs(t.lock_s),
            p.f1,
            p.hit50 * 100.0,
            p.offset_ms,
            p.jitter_p95_ms,
            secs(p.lock_s),
            bar * 100.0,
        );
    }
}

fn scenario_names(filter: Option<&str>) -> Vec<String> {
    let mut names: Vec<String> = scenarios::all().iter().map(|s| s.name.to_string()).collect();
    names.retain(|n| matches(n, filter));
    names
}

fn score(dir: &Path, filter: Option<&str>) {
    score_tracker(dir, filter);
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
            ..Default::default()
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
            ..Default::default()
        };
        let s = metrics::score_tempo(&g.truth, &run);
        assert_eq!(s.acc1, 0.0);
        assert_eq!(s.acc2, 1.0);
    }

    fn scenario(name: &str) -> scenarios::Generated {
        (scenarios::all().into_iter().find(|s| s.name == name).unwrap().build)()
    }

    #[test]
    fn tracker_locks_tempo_and_beat_on_a_house_track() {
        let g = scenario("house_124");
        let run = run_tracker(&g.audio, g.truth.sample_rate, None);
        let tempo = metrics::score_tempo(&g.truth, &run);
        assert!(tempo.acc1 > 0.95, "tempo accuracy {}", tempo.acc1);
        let phase = metrics::score_phase(&g.truth, &run);
        assert!(phase.hit50 > 0.9, "beat clock within 50 ms {}", phase.hit50);
        assert!(phase.jitter_p95_ms < 30.0, "jitter {}", phase.jitter_p95_ms);
    }

    #[test]
    fn tracker_resyncs_after_a_late_mix() {
        let g = scenario("mix_sloppy");
        let run = run_tracker(&g.audio, g.truth.sample_rate, None);
        let phase = metrics::score_phase(&g.truth, &run);
        assert!(phase.hit50 > 0.9, "beat clock within 50 ms {}", phase.hit50);
    }

    #[test]
    fn tempo_range_settles_the_octave_of_drum_and_bass() {
        let g = scenario("dnb_174");
        let wide = run_tracker(&g.audio, g.truth.sample_rate, None);
        let narrow = run_tracker(&g.audio, g.truth.sample_rate, g.truth.tempo_range);
        assert!(metrics::score_tempo(&g.truth, &narrow).acc1 > 0.9);
        assert!(metrics::score_tempo(&g.truth, &wide).acc1 < 0.5, "the default range reads 87 BPM");
    }

    #[test]
    fn beat_clock_analyzer_counts_beats_and_reports_tempo() {
        use crate::audio::analyzers::AnalyzerProc;
        let g = scenario("house_124");
        let (_handle, mut proc) = crate::audio::analyzers::beat_clock::create(g.truth.sample_rate);
        for chunk in g.audio.chunks(512) {
            proc.step(chunk);
        }
        let out = proc.outputs();
        assert_eq!(out.len(), 7);
        assert!((out[5] - 124.0).abs() < 2.0, "bpm {}", out[5]);
        // About 124 beats a minute for 80 s, minus the time to lock.
        let beats = proc.onset_count();
        assert!((120..=170).contains(&beats), "{beats} beats");
        assert!(out[1] >= 0.0 && out[1] < 1.0 && out[2] >= 0.0 && out[2] < 1.0);
    }
}
