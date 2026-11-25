use crate::dsp::{detect_energy_drops, extract_spectral_features, compute_waveform_bins, ANALYSIS_FPS, ANALYSIS_SR};
use crate::audio::{resample_mono, DecodedAudio};
use crate::editing::rebuild_marks_from_raw;
use crate::model::{GridMode, TimeSignature, TrackAnalysis};
use crate::neural::infer_beat_this_onnx;

/// Run full Hybrid (Beat This! ONNX + Pure-Rust DSP) analysis on decoded audio.
pub fn analyze_audio(
    audio: &DecodedAudio,
    mut on_progress: impl FnMut(f32, &str),
) -> TrackAnalysis {
    on_progress(0.05, "Computing high-resolution multi-band waveform...");
    let waveform_bins_per_sec = 400.0f32;
    let waveform = compute_waveform_bins(&audio.mono_pcm, audio.sample_rate, waveform_bins_per_sec);

    on_progress(0.12, "Resampling & extracting 128-band Mel spectrogram...");
    let mono_22k = resample_mono(&audio.mono_pcm, audio.sample_rate, ANALYSIS_SR);
    let features = extract_spectral_features(&mono_22k);

    on_progress(0.18, "Running embedded Beat This! ONNX neural tracker...");
    let (beat_act, downbeat_act, engine_name, is_neural) =
        match infer_beat_this_onnx(&features.log_mel, features.num_frames, |p| {
            on_progress(p, "Running embedded Beat This! ONNX neural tracker...");
        }) {
            Ok(act) => (act.beat_probs, act.downbeat_probs, act.engine_name, true),
            Err(_) => (
                features.dsp_beat_act.clone(),
                features.dsp_downbeat_act.clone(),
                "DSP Fallback Engine".to_string(),
                false,
            ),
        };

    on_progress(0.88, "Tracking beats, downbeats, time signature & drops...");

    let (raw_beat_times, raw_downbeat_times, detected_bpm, tempo_stability, detected_time_sig) =
        if is_neural {
            decode_neural_activations(
                &beat_act,
                &downbeat_act,
                &features.dsp_beat_act,
                &features.dsp_downbeat_act,
                &features.rms_curve,
                ANALYSIS_FPS,
                audio.duration_sec,
            )
        } else {
            decode_dsp_activations(
                &beat_act,
                &downbeat_act,
                &features.rms_curve,
                ANALYSIS_FPS,
                audio.duration_sec,
            )
        };

    // Energy drops / chorus transitions from DSP RMS + sub-bass contrast
    let raw_drops = detect_energy_drops(&features.rms_curve, &features.bass_curve, ANALYSIS_FPS);

    // Fix 1: Only auto-enable LockedConstantBpm if BOTH local tempo is steady AND
    // the single global grid stays phase-locked across the start, middle, and end of the track!
    let global_coherence = evaluate_global_grid_coherence(&raw_beat_times, detected_bpm);
    let grid_mode = if tempo_stability >= 0.78 && global_coherence >= 0.80 {
        GridMode::LockedConstantBpm
    } else {
        GridMode::Dynamic
    };

    on_progress(0.96, "Building color-coded beat & drop grid...");

    let mut analysis = TrackAnalysis {
        title: audio.title.clone(),
        file_path: audio.file_path.clone(),
        sample_rate: audio.sample_rate,
        channels: audio.channels,
        duration_sec: audio.duration_sec,
        detected_bpm,
        active_bpm: detected_bpm,
        bpm_multiplier: 1.0,
        tempo_stability,
        detected_time_sig,
        active_time_sig: detected_time_sig,
        grid_mode,
        detection_engine: engine_name,
        downbeat_phase_shift: 0,
        extrapolate_silence: true,
        in_point_sec: 0.0,
        out_point_sec: audio.duration_sec,
        marks: Vec::new(),
        raw_beat_times,
        raw_downbeat_times,
        raw_drops,
        drop_sensitivity: 0.50,
        waveform_bins_per_sec,
        waveform,
        next_mark_id: 1,
    };

    rebuild_marks_from_raw(&mut analysis);
    on_progress(1.0, "Ready");
    analysis
}

/// Decode Beat This! 50 fps neural beat & downbeat probability curves using peak-picking + bar-meter scoring.
#[allow(clippy::too_many_arguments)]
fn decode_neural_activations(
    beat_probs: &[f32],
    downbeat_probs: &[f32],
    dsp_onset: &[f32],
    dsp_bass_onset: &[f32],
    rms_curve: &[f32],
    fps: f64,
    duration_sec: f64,
) -> (Vec<f64>, Vec<f64>, f64, f32, TimeSignature) {
    // Beat This! official minimal post-processing: local maximum in ±3 frames (±60ms) above threshold 0.5
    // with parabolic sub-frame refinement.
    let mut beats = pick_peaks_subframe(beat_probs, 0.45, 3, fps);
    if beats.len() < 4 {
        // Retry with softer threshold for quiet/ambient tracks
        beats = pick_peaks_subframe(beat_probs, 0.25, 3, fps);
    }

    let mut downbeats = pick_peaks_subframe(downbeat_probs, 0.40, 4, fps);
    if downbeats.is_empty() {
        downbeats = pick_peaks_subframe(downbeat_probs, 0.20, 4, fps);
    }

    // If neural peaks are too sparse (e.g. synthetic pure tone / silence), fallback to real DSP spectral curves!
    if beats.len() < 4 {
        return decode_dsp_activations(dsp_onset, dsp_bass_onset, rms_curve, fps, duration_sec);
    }

    let (bpm, stability) = estimate_bpm_and_stability(&beats);
    let time_sig = infer_time_signature_from_downbeats(&beats, &downbeats, downbeat_probs, fps);
    (beats, downbeats, bpm, stability, time_sig)
}

/// Pure-Rust DSP beat/downbeat tracker (tempogram autocorrelation + Ellis dynamic programming beat tracking).
pub fn decode_dsp_activations(
    onset: &[f32],
    bass_onset: &[f32],
    rms: &[f32],
    fps: f64,
    duration_sec: f64,
) -> (Vec<f64>, Vec<f64>, f64, f32, TimeSignature) {
    let n = onset.len();
    if n < (fps * 2.0) as usize {
        return (vec![], vec![], 120.0, 0.0, TimeSignature::FourFour);
    }

    // 1. Autocorrelation tempogram over 65..185 BPM (modern electronic/hip-hop/pop/trap prior)
    let min_lag = (fps * 60.0 / 185.0).round() as usize;
    let max_lag = ((fps * 60.0 / 65.0).round() as usize).min(n / 2);

    let mut best_lag = (fps * 60.0 / 120.0).round() as usize;
    let mut best_score = -1.0f64;

    for lag in min_lag..=max_lag {
        let mut corr = 0.0f64;
        let count = n - lag;
        for i in 0..count {
            corr += (onset[i] as f64) * (onset[i + lag] as f64);
        }
        corr /= count as f64;
        // Include harmonic reinforcement at 2x lag (bar/half-bar periodicity)
        let lag2 = lag * 2;
        if lag2 < n {
            let mut c2 = 0.0f64;
            let count2 = n - lag2;
            for i in 0..count2 {
                c2 += (onset[i] as f64) * (onset[i + lag2] as f64);
            }
            corr += 0.35 * (c2 / count2 as f64);
        }

        let bpm = 60.0 * fps / lag as f64;
        // Gentle log-normal prior centered around 120 BPM (matches EDM/hip-hop/pop distribution)
        let prior = (-0.5 * ((bpm / 120.0).ln() / 0.42).powi(2)).exp();
        let score = corr * (0.65 + 0.35 * prior);
        if score > best_score {
            best_score = score;
            best_lag = lag;
        }
    }

    // 2. Ellis Dynamic Programming beat tracking using target period `best_lag`
    let period = best_lag as f64;
    let tightness = 100.0f64;
    let mut dp = vec![0.0f64; n];
    let mut back = vec![0usize; n];

    let search_lo = (period * 0.5).floor().max(1.0) as usize;
    let search_hi = (period * 2.0).ceil() as usize;

    for t in 0..n {
        let local = onset[t] as f64;
        let mut max_prev = 0.0f64;
        let mut arg_prev = 0usize;
        if t >= search_lo {
            let start = t.saturating_sub(search_hi);
            let end = t - search_lo;
            for p in start..=end {
                let dt = (t - p) as f64;
                let penalty = -tightness * (dt / period).ln().powi(2);
                let val = dp[p] + penalty;
                if val > max_prev {
                    max_prev = val;
                    arg_prev = p;
                }
            }
        }
        dp[t] = local + max_prev;
        back[t] = arg_prev;
    }

    // Backtrack from best ending beat in last 2 periods
    let tail_start = n.saturating_sub((period * 2.5) as usize);
    let mut cur = tail_start;
    for t in tail_start..n {
        if dp[t] > dp[cur] {
            cur = t;
        }
    }

    let mut beat_frames = Vec::new();
    while cur > 0 {
        beat_frames.push(cur);
        let prev = back[cur];
        if prev >= cur || prev == 0 {
            break;
        }
        cur = prev;
    }
    beat_frames.reverse();

    // Filter out silent edges if RMS is near zero
    let beat_times: Vec<f64> = beat_frames
        .iter()
        .filter(|&&f| f < rms.len() && rms[f] > 0.015)
        .map(|&f| {
            // Sub-frame parabolic refinement on onset curve
            let refined = parabolic_refine(onset, f);
            (refined / fps).clamp(0.0, duration_sec)
        })
        .collect();

    let beats = if beat_times.len() >= 4 {
        beat_times
    } else {
        beat_frames.iter().map(|&f| f as f64 / fps).collect()
    };

    let (bpm, stability) = estimate_bpm_and_stability(&beats);

    // 3. Score time signatures & downbeat phase from bass accent + onset emphasis
    let (time_sig, phase) = infer_dsp_meter_and_phase(&beats, onset, bass_onset, fps);
    let bpb = time_sig.beats_per_bar() as usize;
    let downbeats: Vec<f64> = beats
        .iter()
        .enumerate()
        .filter(|(i, _)| (i + bpb - (phase % bpb)) % bpb == 0)
        .map(|(_, &t)| t)
        .collect();

    (beats, downbeats, bpm, stability, time_sig)
}

fn pick_peaks_subframe(probs: &[f32], threshold: f32, radius: usize, fps: f64) -> Vec<f64> {
    let n = probs.len();
    let mut peaks = Vec::new();
    let mut last_peak: Option<usize> = None;

    for i in 1..n.saturating_sub(1) {
        let v = probs[i];
        if v < threshold {
            continue;
        }
        let lo = i.saturating_sub(radius);
        let hi = (i + radius + 1).min(n);
        let is_max = probs[lo..hi].iter().all(|&x| v >= x) && v > probs[i - 1];
        if is_max {
            if let Some(prev) = last_peak {
                if i - prev <= radius {
                    continue;
                }
            }
            last_peak = Some(i);
            let refined = parabolic_refine(probs, i);
            peaks.push(refined / fps);
        }
    }
    peaks
}

fn parabolic_refine(signal: &[f32], idx: usize) -> f64 {
    if idx == 0 || idx + 1 >= signal.len() {
        return idx as f64;
    }
    let y0 = signal[idx - 1] as f64;
    let y1 = signal[idx] as f64;
    let y2 = signal[idx + 1] as f64;
    let denom = y0 - 2.0 * y1 + y2;
    if denom.abs() < 1e-9 {
        return idx as f64;
    }
    let delta = (0.5 * (y0 - y2) / denom).clamp(-0.5, 0.5);
    idx as f64 + delta
}

pub fn estimate_bpm_and_stability(beats: &[f64]) -> (f64, f32) {
    if beats.len() < 2 {
        return (120.0, 0.0);
    }
    let mut ibis: Vec<f64> = beats
        .windows(2)
        .map(|w| w[1] - w[0])
        .filter(|&dt| dt >= 0.20 && dt <= 1.50)
        .collect();
    if ibis.is_empty() {
        return (120.0, 0.0);
    }
    ibis.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median_ibi = ibis[ibis.len() / 2];

    // Local tempo stability: fraction of adjacent intervals within ±5% of median IBI
    let inliers: Vec<f64> = ibis
        .iter()
        .copied()
        .filter(|&dt| (dt - median_ibi).abs() / median_ibi <= 0.05)
        .collect();
    let stability = (inliers.len() as f32) / (ibis.len() as f32);
    let refined_ibi = if inliers.is_empty() {
        median_ibi
    } else {
        inliers.iter().sum::<f64>() / inliers.len() as f64
    };

    // Fix 1: Exact least-squares linear regression over the longest coherent beat run
    // to eliminate cumulative rounding drift over 4–5 minute tracks!
    let exact_ibi = least_squares_period(beats, refined_ibi).unwrap_or(refined_ibi);
    let exact_bpm = (60.0 / exact_ibi).clamp(40.0, 240.0);

    // Only snap to an exact integer BPM if the true measured period is already within 0.008 BPM
    // (i.e. < ~10 ms over a 5-minute song — true DAW-quantized export!). Otherwise preserve exact precision.
    let final_bpm = if stability > 0.90 && (exact_bpm - exact_bpm.round()).abs() < 0.008 {
        exact_bpm.round()
    } else {
        exact_bpm
    };
    (final_bpm, stability)
}

/// Ordinary least-squares fit of `t(k) = anchor + k * period` on the longest contiguous
/// steady-tempo run of detected beats. Eliminates cumulative rounding drift on long tracks.
fn least_squares_period(beats: &[f64], approx_period: f64) -> Option<f64> {
    if beats.len() < 4 || approx_period <= 0.0 {
        return None;
    }

    // Split beats into contiguous coherent runs where each jump is a clean integer multiple (1..=4) of approx_period
    let mut best_run: Vec<(f64, f64)> = Vec::new();
    let mut cur_run: Vec<(f64, f64)> = vec![(0.0, beats[0])];
    let mut cur_k = 0.0f64;

    for w in beats.windows(2) {
        let dt = w[1] - w[0];
        let step_f = (dt / approx_period).round();
        let err = (dt - step_f * approx_period).abs();
        if (1.0..=4.0).contains(&step_f) && err <= approx_period * 0.07 {
            cur_k += step_f;
            cur_run.push((cur_k, w[1]));
        } else {
            if cur_run.len() > best_run.len() {
                best_run = cur_run;
            }
            cur_k = 0.0;
            cur_run = vec![(0.0, w[1])];
        }
    }
    if cur_run.len() > best_run.len() {
        best_run = cur_run;
    }

    if best_run.len() < 4 {
        return None;
    }

    let n = best_run.len() as f64;
    let mean_k = best_run.iter().map(|(k, _)| *k).sum::<f64>() / n;
    let mean_t = best_run.iter().map(|(_, t)| *t).sum::<f64>() / n;
    let mut num = 0.0f64;
    let mut den = 0.0f64;
    for &(k, t) in &best_run {
        let dk = k - mean_k;
        num += dk * (t - mean_t);
        den += dk * dk;
    }
    if den < 1e-6 {
        return None;
    }
    let period = num / den;
    if (period - approx_period).abs() <= approx_period * 0.05 {
        Some(period)
    } else {
        None
    }
}

/// Measure how well a single rigid constant-BPM grid stays phase-locked across the ENTIRE track
/// (checking start, middle, and end thirds separately so a mid-song breakdown splice or tempo drift
/// automatically drops coherence and switches to Dynamic mode).
pub fn evaluate_global_grid_coherence(beats: &[f64], bpm: f64) -> f32 {
    if beats.len() < 8 || bpm <= 0.0 {
        return 0.0;
    }
    let period = 60.0 / bpm;
    let two_pi = std::f64::consts::TAU;
    let mut sin_sum = 0.0f64;
    let mut cos_sum = 0.0f64;
    for &t in beats {
        let angle = two_pi * (t.rem_euclid(period)) / period;
        sin_sum += angle.sin();
        cos_sum += angle.cos();
    }
    let anchor = sin_sum.atan2(cos_sum).rem_euclid(two_pi) * period / two_pi;

    let tolerance_sec = (period * 0.085).clamp(0.025, 0.045);
    let is_locked = |t: f64| -> bool {
        let k = ((t - anchor) / period).round();
        (t - (anchor + k * period)).abs() <= tolerance_sec
    };

    let overall = beats.iter().filter(|&&t| is_locked(t)).count() as f32 / beats.len() as f32;

    // Also evaluate start, middle, and end thirds independently so a shifted second half is caught!
    let third = (beats.len() / 3).max(1);
    let s1 = beats[..third].iter().filter(|&&t| is_locked(t)).count() as f32 / third as f32;
    let s2 = beats[third..third * 2].iter().filter(|&&t| is_locked(t)).count() as f32 / third as f32;
    let rem = beats.len() - third * 2;
    let s3 = beats[third * 2..].iter().filter(|&&t| is_locked(t)).count() as f32 / rem.max(1) as f32;

    overall.min(s1.min(s2).min(s3))
}

fn infer_time_signature_from_downbeats(
    beats: &[f64],
    downbeats: &[f64],
    downbeat_probs: &[f32],
    fps: f64,
) -> TimeSignature {
    if beats.len() < 6 {
        return TimeSignature::FourFour;
    }

    // Score each candidate bar length by measuring mean downbeat activation on the best phase
    // minus non-downbeat mean, plus downbeat-to-downbeat beat count histogram!
    let mut beat_indices_of_downbeats: Vec<usize> = Vec::new();
    for &db in downbeats {
        if let Some((idx, _)) = beats.iter().enumerate().min_by(|(_, a), (_, b)| {
            (*a - db)
                .abs()
                .partial_cmp(&(*b - db).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        }) {
            if (beats[idx] - db).abs() < 0.15 {
                if beat_indices_of_downbeats.last().copied() != Some(idx) {
                    beat_indices_of_downbeats.push(idx);
                }
            }
        }
    }

    // Direct bar-length counts between consecutive detected downbeats
    let mut counts = [0usize; 8];
    for w in beat_indices_of_downbeats.windows(2) {
        let diff = w[1].saturating_sub(w[0]);
        if diff >= 2 && diff <= 7 {
            counts[diff] += 1;
        }
    }

    let candidates = [
        (TimeSignature::FourFour, 4usize, 1.15f64), // Gentle modern music prior for 4/4
        (TimeSignature::ThreeFour, 3, 1.00),
        (TimeSignature::SixEight, 6, 0.96),
        (TimeSignature::TwoFour, 2, 0.80),
        (TimeSignature::FiveFour, 5, 0.90),
        (TimeSignature::SevenEight, 7, 0.88),
    ];

    let total_intervals: usize = counts.iter().sum();
    let mut best_ts = TimeSignature::FourFour;
    let mut best_score = -1.0f64;

    for (ts, bpb, prior) in candidates {
        // Activation-contrast score across phases
        let mut best_phase_contrast = 0.0f64;
        for phase in 0..bpb {
            let mut db_sum = 0.0f64;
            let mut db_n = 0usize;
            let mut other_sum = 0.0f64;
            let mut other_n = 0usize;
            for (i, &t) in beats.iter().enumerate() {
                let f = ((t * fps).round() as usize).min(downbeat_probs.len().saturating_sub(1));
                let p = downbeat_probs[f] as f64;
                if i % bpb == phase {
                    db_sum += p;
                    db_n += 1;
                } else {
                    other_sum += p;
                    other_n += 1;
                }
            }
            if db_n > 0 && other_n > 0 {
                let contrast = (db_sum / db_n as f64) - (other_sum / other_n as f64);
                if contrast > best_phase_contrast {
                    best_phase_contrast = contrast;
                }
            }
        }

        let hist_frac = if total_intervals > 0 {
            counts[bpb] as f64 / total_intervals as f64
        } else {
            0.0
        };
        let score = (0.60 * best_phase_contrast + 0.40 * hist_frac) * prior;
        if score > best_score {
            best_score = score;
            best_ts = ts;
        }
    }

    best_ts
}

fn infer_dsp_meter_and_phase(
    beats: &[f64],
    onset: &[f32],
    bass_onset: &[f32],
    fps: f64,
) -> (TimeSignature, usize) {
    if beats.len() < 4 {
        return (TimeSignature::FourFour, 0);
    }

    // Score beat accent strength (bass weighted 70%, full onset 30%)
    let accents: Vec<f64> = beats
        .iter()
        .map(|&t| {
            let f = (t * fps).round() as usize;
            let lo = f.saturating_sub(2);
            let hi = (f + 3).min(onset.len());
            let o = onset[lo..hi].iter().copied().fold(0.0f32, f32::max) as f64;
            let b = bass_onset[lo..hi.min(bass_onset.len())]
                .iter()
                .copied()
                .fold(0.0f32, f32::max) as f64;
            0.70 * b + 0.30 * o
        })
        .collect();

    let candidates = [
        (TimeSignature::FourFour, 4usize, 1.25f64),
        (TimeSignature::ThreeFour, 3, 1.00),
        (TimeSignature::SixEight, 6, 0.95),
    ];

    let mut best_ts = TimeSignature::FourFour;
    let mut best_phase = 0usize;
    let mut best_score = -1.0f64;

    for (ts, bpb, prior) in candidates {
        for phase in 0..bpb {
            let mut sum_db = 0.0;
            let mut n_db = 0;
            let mut sum_other = 0.0;
            let mut n_other = 0;
            for (i, &a) in accents.iter().enumerate() {
                if i % bpb == phase {
                    sum_db += a;
                    n_db += 1;
                } else {
                    sum_other += a;
                    n_other += 1;
                }
            }
            if n_db > 0 && n_other > 0 {
                let contrast = ((sum_db / n_db as f64) - (sum_other / n_other as f64)) * prior;
                if contrast > best_score {
                    best_score = contrast;
                    best_ts = ts;
                    best_phase = phase;
                }
            }
        }
    }

    (best_ts, best_phase)
}
