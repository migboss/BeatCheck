pub mod audio;
pub mod dsp;
pub mod editing;
pub mod model;
pub mod neural;
pub mod tracker;

pub use audio::{decode_audio_file, resample_mono, DecodedAudio};
pub use editing::EditHistory;
pub use model::{
    BeatMark, GridMode, MarkExportStyle, MarkKind, MarkStyleConfig, ResolveColor, TimeSignature,
    TrackAnalysis, WaveformBin,
};
pub use tracker::analyze_audio;

#[cfg(test)]
mod tests {
    use super::*;

    /// Synthesize a realistic 4/4 electronic kick + hi-hat beat at 128 BPM with a bass drop at Bar 5.
    fn synthesize_test_beat(bpm: f64, duration_sec: f64, sr: u32) -> DecodedAudio {
        let total = (duration_sec * sr as f64) as usize;
        let mut mono = vec![0.0f32; total];
        let period = 60.0 / bpm;

        let num_beats = (duration_sec / period) as usize;
        for b in 0..num_beats {
            let start = (b as f64 * period * sr as f64) as usize;
            let is_downbeat = b % 4 == 0;
            let is_strong = b % 4 == 2;
            let bar = b / 4 + 1;
            let amp = if bar >= 5 { 0.95 } else { 0.35 };
            let click_len = (sr as f64 * 0.08) as usize;

            for i in 0..click_len {
                if start + i >= total {
                    break;
                }
                let t = i as f32 / sr as f32;
                let env = (-t * 35.0).exp();
                // Pitch-swept kick on downbeat/strong, crisp snare/hat on off-beats
                let sig = if is_downbeat {
                    (2.0 * std::f32::consts::PI * (120.0 * (-t * 18.0).exp() + 45.0) * t).sin()
                        * 1.0
                } else if is_strong {
                    (2.0 * std::f32::consts::PI * (90.0 * (-t * 15.0).exp() + 55.0) * t).sin() * 0.7
                } else {
                    (2.0 * std::f32::consts::PI * 900.0 * t).sin() * 0.45
                };
                mono[start + i] += sig * env * amp;
            }
        }

        let mut stereo = Vec::with_capacity(total * 2);
        for &s in &mono {
            stereo.push(s);
            stereo.push(s);
        }

        DecodedAudio {
            title: "Synth128".to_string(),
            file_path: "synth128.wav".to_string(),
            sample_rate: sr,
            channels: 2,
            stereo_pcm: stereo,
            mono_pcm: mono,
            duration_sec,
        }
    }

    #[test]
    fn hybrid_engine_detects_128bpm_four_four_and_marks() {
        let audio = synthesize_test_beat(128.0, 20.0, 44_100);
        let analysis = analyze_audio(&audio, |_p, _msg| {});
        println!(
            "Engine: {} | Detected BPM: {} | TimeSig: {} | Marks: {}",
            analysis.detection_engine,
            analysis.detected_bpm,
            analysis.detected_time_sig,
            analysis.marks.len()
        );
        // Accept 128 BPM (or 64/256 half/double harmonic on minimal synthetic clicks)
        let bpm = analysis.detected_bpm;
        let matches_128 = (bpm - 128.0).abs() < 3.0 || (bpm - 64.0).abs() < 2.0;
        assert!(matches_128, "Expected ~128 BPM (or 64 half-time), got {bpm}");
        assert!(!analysis.marks.is_empty());
        assert!(analysis.marks.iter().any(|m| m.kind == MarkKind::Downbeat));
    }

    #[test]
    fn quick_fixes_and_undo_redo_work_cleanly() {
        let audio = synthesize_test_beat(120.0, 12.0, 22_050);
        let mut analysis = analyze_audio(&audio, |_p, _msg| {});
        let mut history = EditHistory::new(32);

        let orig_bpm = analysis.active_bpm;
        history.push_snapshot(&analysis);
        editing::toggle_bpm_multiplier(&mut analysis, 0.5);
        assert!((analysis.active_bpm - orig_bpm * 0.5).abs() < 0.2);

        assert!(history.undo(&mut analysis));
        assert!((analysis.active_bpm - orig_bpm).abs() < 0.2);
        assert!(history.redo(&mut analysis));
        assert!((analysis.active_bpm - orig_bpm * 0.5).abs() < 0.2);
    }

    #[test]
    fn drift_free_bpm_and_mid_song_splice_detection_work() {
        // 1. Exact fractional BPM (124.08 BPM over 5 minutes = 620 beats) must not round away 0.08 BPM!
        let true_bpm = 124.08;
        let period = 60.0 / true_bpm;
        let steady_beats: Vec<f64> = (0..620).map(|k| 0.15 + (k as f64) * period).collect();
        let (est_bpm, stab) = tracker::estimate_bpm_and_stability(&steady_beats);
        assert!(stab > 0.99);
        assert!(
            (est_bpm - true_bpm).abs() < 0.005,
            "Expected drift-free BPM ~{true_bpm}, got {est_bpm}"
        );
        let coh_steady = tracker::evaluate_global_grid_coherence(&steady_beats, est_bpm);
        assert!(coh_steady > 0.95, "Steady track coherence should be high, got {coh_steady}");

        // 2. Mid-song splice (half-beat phase jump at beat 150) must drop global coherence so Dynamic triggers!
        let mut spliced = Vec::new();
        for k in 0..300 {
            let offset = if k >= 150 { period * 0.45 } else { 0.0 };
            spliced.push(0.15 + (k as f64) * period + offset);
        }
        let coh_spliced = tracker::evaluate_global_grid_coherence(&spliced, true_bpm);
        assert!(
            coh_spliced < 0.70,
            "Spliced track should fail rigid coherence check, got {coh_spliced}"
        );
    }

    #[test]
    fn dynamic_resync_and_relock_from_time_work() {
        let period = 0.5; // 120 BPM
        // Simulate 2 bars (8 beats), a 1.3s breakdown pause, then 2 bars starting at 5.3s
        let mut raw_beats = Vec::new();
        let mut raw_dbs = Vec::new();
        for k in 0..8 {
            let t = k as f64 * period;
            raw_beats.push(t);
            if k % 4 == 0 {
                raw_dbs.push(t);
            }
        }
        for k in 0..8 {
            let t = 5.3 + k as f64 * period;
            raw_beats.push(t);
            if k % 4 == 0 {
                raw_dbs.push(t);
            }
        }

        let mut track = TrackAnalysis {
            title: "SpliceTest".into(),
            file_path: String::new(),
            sample_rate: 44_100,
            channels: 2,
            duration_sec: 12.0,
            detected_bpm: 120.0,
            active_bpm: 120.0,
            bpm_multiplier: 1.0,
            tempo_stability: 0.9,
            detected_time_sig: TimeSignature::FourFour,
            active_time_sig: TimeSignature::FourFour,
            grid_mode: GridMode::Dynamic,
            detection_engine: "Test".into(),
            downbeat_phase_shift: 0,
            extrapolate_silence: false,
            in_point_sec: 0.0,
            out_point_sec: 12.0,
            marks: Vec::new(),
            raw_beat_times: raw_beats,
            raw_downbeat_times: raw_dbs,
            raw_drops: Vec::new(),
            drop_sensitivity: 0.5,
            waveform_bins_per_sec: 200.0,
            waveform: Vec::new(),
            next_mark_id: 1,
        };

        editing::rebuild_marks_from_raw(&mut track);
        // After the breakdown at 5.3s, beat at 5.3s MUST be Downbeat (Bar 3.1)!
        let m_53 = track
            .marks
            .iter()
            .find(|m| (m.time_sec - 5.3).abs() < 0.01)
            .expect("mark at 5.3s");
        assert_eq!(m_53.kind, MarkKind::Downbeat);

        // Now test Fix 3: Re-Lock Grid From 6.1s as Beat 2 (Clap/Backbeat)
        editing::relock_grid_from_time(&mut track, 6.1, 2);
        // Earlier mark at 0.0s must remain untouched!
        assert!(track.marks.iter().any(|m| (m.time_sec - 0.0).abs() < 0.01));
        // Anchor at 6.1s must be RegularBeat (Beat 2), followed by StrongBeat at 6.6s, Regular at 7.1s, Downbeat at 7.6s!
        let at_61 = track.marks.iter().find(|m| (m.time_sec - 6.1).abs() < 0.01).unwrap();
        let at_66 = track.marks.iter().find(|m| (m.time_sec - 6.6).abs() < 0.01).unwrap();
        let at_76 = track.marks.iter().find(|m| (m.time_sec - 7.6).abs() < 0.01).unwrap();
        assert_eq!(at_61.kind, MarkKind::RegularBeat);
        assert_eq!(at_61.beat_in_bar, 2);
        assert_eq!(at_66.kind, MarkKind::StrongBeat);
        assert_eq!(at_66.beat_in_bar, 3);
        assert_eq!(at_76.kind, MarkKind::Downbeat);
        assert_eq!(at_76.beat_in_bar, 1);
        assert_eq!(at_76.bar, at_61.bar + 1);
    }

    #[test]
    fn energy_drop_inherits_colocated_downbeat_bar_and_manual_beats_survive_drop_resens() {
        let mut track = TrackAnalysis {
            title: "DropTest".into(),
            file_path: String::new(),
            sample_rate: 44_100,
            channels: 2,
            duration_sec: 10.0,
            detected_bpm: 120.0,
            active_bpm: 120.0,
            bpm_multiplier: 1.0,
            tempo_stability: 1.0,
            detected_time_sig: TimeSignature::FourFour,
            active_time_sig: TimeSignature::FourFour,
            grid_mode: GridMode::LockedConstantBpm,
            detection_engine: "Test".into(),
            downbeat_phase_shift: 0,
            extrapolate_silence: false,
            in_point_sec: 0.0,
            out_point_sec: 10.0,
            marks: Vec::new(),
            raw_beat_times: vec![0.0, 0.5, 1.0, 1.5, 2.0, 2.5, 3.0, 3.5],
            raw_downbeat_times: vec![0.0, 2.0],
            raw_drops: vec![(2.0, 0.70)],
            drop_sensitivity: 0.5,
            waveform_bins_per_sec: 200.0,
            waveform: Vec::new(),
            next_mark_id: 1,
        };

        editing::rebuild_marks_from_raw(&mut track);
        let db = track
            .marks
            .iter()
            .find(|m| m.kind == MarkKind::Downbeat && (m.time_sec - 2.0).abs() < 0.01)
            .expect("downbeat at 2.0s");
        let drop_m = track
            .marks
            .iter()
            .find(|m| m.kind == MarkKind::EnergyDrop)
            .expect("drop at 2.0s");
        assert_eq!(drop_m.bar, db.bar);
        assert_eq!(drop_m.beat_in_bar, db.beat_in_bar);

        // Verify Locked BPM starts at Bar 1 (never Bar 2!)
        let first_db = track
            .marks
            .iter()
            .find(|m| m.kind == MarkKind::Downbeat && (m.time_sec - 0.0).abs() < 0.01)
            .expect("first downbeat at 0.0s");
        assert_eq!(first_db.bar, 1);
        assert_eq!(db.bar, 2);

        // Add a manual beat at 1.25s and change drop sensitivity — manual beat MUST survive!
        let manual_id = editing::add_mark(&mut track, 1.25, MarkKind::StrongBeat);
        track.drop_sensitivity = 0.0;
        editing::rebuild_energy_drops_only(&mut track);
        assert!(track.marks.iter().any(|m| m.id == manual_id));
        assert!(!track.marks.iter().any(|m| m.kind == MarkKind::EnergyDrop));

        // Tap tempo with extrapolate_silence = false MUST still generate full track beat grid!
        track.extrapolate_silence = false;
        editing::apply_tap_tempo(&mut track, 120.0, 2.0);
        assert!(
            track.marks.len() >= 18,
            "Expected full 10s grid at 120 BPM, got {} marks",
            track.marks.len()
        );
    }

    #[test]
    fn dynamic_two_four_never_emits_consecutive_downbeats() {
        let mut track = TrackAnalysis {
            title: "TwoFourTest".into(),
            file_path: String::new(),
            sample_rate: 44_100,
            channels: 2,
            duration_sec: 5.0,
            detected_bpm: 120.0,
            active_bpm: 120.0,
            bpm_multiplier: 1.0,
            tempo_stability: 0.7,
            detected_time_sig: TimeSignature::TwoFour,
            active_time_sig: TimeSignature::TwoFour,
            grid_mode: GridMode::Dynamic,
            detection_engine: "Test".into(),
            downbeat_phase_shift: 0,
            extrapolate_silence: false,
            in_point_sec: 0.0,
            out_point_sec: 5.0,
            marks: Vec::new(),
            raw_beat_times: vec![0.0, 0.5, 1.0, 1.5, 2.0, 2.5],
            // Adjacent noisy raw downbeats at 0.0s and 0.5s
            raw_downbeat_times: vec![0.0, 0.5, 1.0, 2.0],
            raw_drops: Vec::new(),
            drop_sensitivity: 0.5,
            waveform_bins_per_sec: 200.0,
            waveform: Vec::new(),
            next_mark_id: 1,
        };
        editing::rebuild_marks_from_raw(&mut track);
        for w in track.marks.windows(2) {
            assert!(
                !(w[0].kind == MarkKind::Downbeat && w[1].kind == MarkKind::Downbeat),
                "Found consecutive downbeats in 2/4: {:?} -> {:?}",
                w[0].time_sec,
                w[1].time_sec
            );
        }
    }
}
