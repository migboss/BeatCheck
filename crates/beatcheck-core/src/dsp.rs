use crate::model::WaveformBin;
use rustfft::{num_complex::Complex, FftPlanner};
use std::f32::consts::PI;

pub const ANALYSIS_SR: u32 = 22_050;
pub const ANALYSIS_FPS: f64 = 50.0;
pub const N_FFT: usize = 1024;
pub const HOP_LENGTH: usize = 441; // 22050 / 441 = exact 50.0 fps
pub const N_MELS: usize = 128;

/// Computed 50 fps spectral features used by both neural ONNX inference and DSP trackers.
#[derive(Debug, Clone)]
pub struct SpectralFeatures {
    pub num_frames: usize,
    /// Flattened `[num_frames, 128]` Beat This! Slaney log-mel spectrogram.
    pub log_mel: Vec<f32>,
    /// Multi-band onset strength curve (50 fps, normalized 0..1).
    pub dsp_beat_act: Vec<f32>,
    /// Low-frequency bass/kick accent curve (50 fps, normalized 0..1).
    pub dsp_downbeat_act: Vec<f32>,
    /// Full-band RMS loudness per 50 fps frame (normalized 0..1).
    pub rms_curve: Vec<f32>,
    /// Sub-bass (30-150 Hz) energy per 50 fps frame (normalized 0..1).
    pub bass_curve: Vec<f32>,
}

/// Compute Beat This! compatible 128-band Slaney log-mel spectrogram + DSP onset/energy curves.
pub fn extract_spectral_features(mono_22k: &[f32]) -> SpectralFeatures {
    let n_bins = N_FFT / 2 + 1; // 513
    let pad = N_FFT / 2; // 512 (center=True matching torchaudio)
    let num_frames = if mono_22k.is_empty() {
        1
    } else {
        (mono_22k.len() / HOP_LENGTH) + 1
    };

    // Periodic Hann window matching PyTorch `torch.hann_window(1024)`
    let window: Vec<f32> = (0..N_FFT)
        .map(|i| 0.5 * (1.0 - (2.0 * PI * i as f32 / N_FFT as f32).cos()))
        .collect();

    let mel_fb = build_slaney_mel_filterbank(ANALYSIS_SR as f32, N_FFT, N_MELS, 30.0, 11_000.0);

    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(N_FFT);
    let mut fft_buf = vec![Complex::new(0.0f32, 0.0f32); N_FFT];
    let mut mag_prev = vec![0.0f32; n_bins];
    let mut mag_curr = vec![0.0f32; n_bins];

    let mut log_mel = vec![0.0f32; num_frames * N_MELS];
    let mut flux_full = vec![0.0f32; num_frames];
    let mut flux_bass = vec![0.0f32; num_frames];
    let mut rms_curve = vec![0.0f32; num_frames];
    let mut bass_curve = vec![0.0f32; num_frames];

    // Frequency bin boundaries for 22050 Hz / 1024 (~21.53 Hz per bin)
    let bass_hi_bin = ((150.0 / (ANALYSIS_SR as f32 / N_FFT as f32)).round() as usize).min(n_bins);
    let mid_hi_bin = ((2500.0 / (ANALYSIS_SR as f32 / N_FFT as f32)).round() as usize).min(n_bins);

    for frame in 0..num_frames {
        let center_start = (frame * HOP_LENGTH) as isize - pad as isize;
        let mut energy_sum = 0.0f32;
        for i in 0..N_FFT {
            let idx = center_start + i as isize;
            // Reflect padding matches torchaudio center=True
            let sample = if mono_22k.is_empty() {
                0.0
            } else if idx < 0 {
                let refl = (-idx) as usize;
                mono_22k[refl.min(mono_22k.len() - 1)]
            } else if (idx as usize) >= mono_22k.len() {
                let over = (idx as usize) - (mono_22k.len() - 1);
                let refl = mono_22k.len().saturating_sub(1 + over);
                mono_22k[refl]
            } else {
                mono_22k[idx as usize]
            };
            energy_sum += sample * sample;
            fft_buf[i] = Complex::new(sample * window[i], 0.0);
        }
        rms_curve[frame] = (energy_sum / N_FFT as f32).sqrt();

        fft.process(&mut fft_buf);

        // Magnitude normalized by frame_length (N_FFT) matching Beat This! `normalized='frame_length', power=1`
        let inv_n = 1.0 / (N_FFT as f32);
        let mut bass_e = 0.0f32;
        let mut f_bass = 0.0f32;
        let mut f_mid = 0.0f32;
        let mut f_high = 0.0f32;

        for b in 0..n_bins {
            let m = fft_buf[b].norm() * inv_n;
            mag_curr[b] = m;
            let diff = ((1.0 + 100.0 * m).ln() - (1.0 + 100.0 * mag_prev[b]).ln()).max(0.0);
            if b >= 1 && b < bass_hi_bin {
                bass_e += m * m;
                f_bass += diff;
            } else if b >= bass_hi_bin && b < mid_hi_bin {
                f_mid += diff;
            } else if b >= mid_hi_bin {
                f_high += diff;
            }
        }
        bass_curve[frame] = bass_e.sqrt();
        // Weighted onset strength (modern electronic/hip-hop kick + snare + hi-hat balance)
        flux_bass[frame] = f_bass / (bass_hi_bin.max(1) as f32);
        flux_full[frame] = 0.45 * (f_bass / bass_hi_bin.max(1) as f32)
            + 0.40 * (f_mid / (mid_hi_bin - bass_hi_bin).max(1) as f32)
            + 0.15 * (f_high / (n_bins - mid_hi_bin).max(1) as f32);

        // Project magnitude spectrum through 128 Slaney mel filters and apply ln(1 + 1000 * x)
        let row_offset = frame * N_MELS;
        for (m_idx, sparse_row) in mel_fb.iter().enumerate() {
            let mut acc = 0.0f32;
            for &(bin, w) in sparse_row {
                acc += mag_curr[bin] * w;
            }
            log_mel[row_offset + m_idx] = (1000.0 * acc).ln_1p();
        }

        mag_prev.copy_from_slice(&mag_curr);
    }

    let dsp_beat_act = adaptive_normalize_onset(&flux_full, 15);
    let dsp_downbeat_act = adaptive_normalize_onset(&flux_bass, 25);
    normalize_in_place(&mut rms_curve);
    normalize_in_place(&mut bass_curve);

    SpectralFeatures {
        num_frames,
        log_mel,
        dsp_beat_act,
        dsp_downbeat_act,
        rms_curve,
        bass_curve,
    }
}

/// Build sparse Slaney Mel filterbank matching `torchaudio.transforms.MelSpectrogram(..., mel_scale="slaney")`.
fn build_slaney_mel_filterbank(
    sr: f32,
    n_fft: usize,
    n_mels: usize,
    f_min: f32,
    f_max: f32,
) -> Vec<Vec<(usize, f32)>> {
    let n_bins = n_fft / 2 + 1;
    let hz_to_mel = |f: f32| -> f32 {
        let f_sp = 200.0 / 3.0;
        let min_log_hz = 1000.0;
        let min_log_mel = min_log_hz / f_sp;
        let logstep = (6.4f32).ln() / 27.0;
        if f >= min_log_hz {
            min_log_mel + (f / min_log_hz).ln() / logstep
        } else {
            f / f_sp
        }
    };
    let mel_to_hz = |m: f32| -> f32 {
        let f_sp = 200.0 / 3.0;
        let min_log_hz = 1000.0;
        let min_log_mel = min_log_hz / f_sp;
        let logstep = (6.4f32).ln() / 27.0;
        if m >= min_log_mel {
            min_log_hz * (logstep * (m - min_log_mel)).exp()
        } else {
            f_sp * m
        }
    };

    let m_min = hz_to_mel(f_min);
    let m_max = hz_to_mel(f_max);
    let m_pts: Vec<f32> = (0..=n_mels + 1)
        .map(|i| m_min + (m_max - m_min) * (i as f32) / ((n_mels + 1) as f32))
        .collect();
    let f_pts: Vec<f32> = m_pts.into_iter().map(mel_to_hz).collect();
    let bin_freqs: Vec<f32> = (0..n_bins)
        .map(|b| (b as f32) * sr / (n_fft as f32))
        .collect();

    let mut fb = Vec::with_capacity(n_mels);
    for m in 0..n_mels {
        let f_left = f_pts[m];
        let f_center = f_pts[m + 1];
        let f_right = f_pts[m + 2];
        let mut row = Vec::new();
        for (b, &freq) in bin_freqs.iter().enumerate() {
            let w = if freq > f_left && freq <= f_center && (f_center - f_left) > 1e-6 {
                (freq - f_left) / (f_center - f_left)
            } else if freq > f_center && freq < f_right && (f_right - f_center) > 1e-6 {
                (f_right - freq) / (f_right - f_center)
            } else {
                0.0
            };
            if w > 0.0 {
                row.push((b, w));
            }
        }
        fb.push(row);
    }
    fb
}

fn adaptive_normalize_onset(curve: &[f32], half_win: usize) -> Vec<f32> {
    let n = curve.len();
    if n == 0 {
        return vec![];
    }
    let mut out = vec![0.0f32; n];
    for i in 0..n {
        let lo = i.saturating_sub(half_win);
        let hi = (i + half_win + 1).min(n);
        let mean: f32 = curve[lo..hi].iter().sum::<f32>() / (hi - lo) as f32;
        out[i] = (curve[i] - mean * 0.95).max(0.0);
    }
    normalize_in_place(&mut out);
    out
}

fn normalize_in_place(v: &mut [f32]) {
    let max_val = v.iter().copied().fold(0.0f32, f32::max);
    if max_val > 1e-9 {
        let inv = 1.0 / max_val;
        for x in v.iter_mut() {
            *x = (*x * inv).clamp(0.0, 1.0);
        }
    }
}

/// Detect major energy drops, chorus entries, and breakdown transitions from 50 fps RMS & sub-bass curves.
/// Returns sorted `(time_sec, strength 0..1)` candidates.
pub fn detect_energy_drops(rms: &[f32], bass: &[f32], fps: f64) -> Vec<(f64, f32)> {
    let n = rms.len().min(bass.len());
    if n < (fps * 4.0) as usize {
        return vec![];
    }

    // Compare 2.0s post-window vs 2.0s pre-window for sharp energy surges (drops/choruses)
    // and sharp energy plunges (breakdowns).
    let win = (fps * 2.0) as usize;
    let mut prefix_rms = vec![0.0f64; n + 1];
    let mut prefix_bass = vec![0.0f64; n + 1];
    for i in 0..n {
        prefix_rms[i + 1] = prefix_rms[i] + rms[i] as f64;
        prefix_bass[i + 1] = prefix_bass[i] + bass[i] as f64;
    }

    let mut novelty = vec![0.0f32; n];
    for i in win..(n - win) {
        let pre_rms = (prefix_rms[i] - prefix_rms[i - win]) / win as f64;
        let post_rms = (prefix_rms[i + win] - prefix_rms[i]) / win as f64;
        let pre_bass = (prefix_bass[i] - prefix_bass[i - win]) / win as f64;
        let post_bass = (prefix_bass[i + win] - prefix_bass[i]) / win as f64;

        // Positive jump (Bass drop / chorus impact) weighted highest for modern electronic/hip-hop/trap
        let rise = ((post_rms - pre_rms).max(0.0) * 0.55 + (post_bass - pre_bass).max(0.0) * 0.75)
            * (0.5 + post_rms);
        // Dramatic breakdown (loud -> sudden quiet)
        let fall = ((pre_rms - post_rms).max(0.0) * 0.45 + (pre_bass - post_bass).max(0.0) * 0.35)
            * pre_rms;
        novelty[i] = rise.max(fall * 0.75) as f32;
    }

    normalize_in_place(&mut novelty);

    // Peak-pick with minimum 6.0 second spacing so only genuine structural drops/transitions fire
    let min_gap = (fps * 6.0) as usize;
    let mut peaks: Vec<(usize, f32)> = Vec::new();
    for i in win..(n - win) {
        let v = novelty[i];
        if v < 0.22 {
            continue;
        }
        let lo = i.saturating_sub(min_gap / 3);
        let hi = (i + min_gap / 3).min(n);
        if novelty[lo..hi].iter().all(|&x| v >= x) {
            peaks.push((i, v));
        }
    }

    // Greedy non-maximum suppression by strength with 6.0s exclusion window
    peaks.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let mut selected: Vec<(usize, f32)> = Vec::new();
    for (idx, score) in peaks {
        if selected
            .iter()
            .all(|&(s_idx, _)| idx.abs_diff(s_idx) >= min_gap)
        {
            selected.push((idx, score));
        }
    }
    selected.sort_by_key(|&(idx, _)| idx);

    selected
        .into_iter()
        .map(|(idx, score)| (idx as f64 / fps, score))
        .collect()
}

/// Precompute multi-band RGB waveform bins (`200 bins/sec`) in a single O(N) pass for 60 FPS GPU rendering.
pub fn compute_waveform_bins(mono: &[f32], sample_rate: u32, bins_per_sec: f32) -> Vec<WaveformBin> {
    if mono.is_empty() || sample_rate == 0 || bins_per_sec <= 0.0 {
        return vec![];
    }
    let samples_per_bin = (sample_rate as f64 / bins_per_sec as f64).max(1.0);
    let num_bins = ((mono.len() as f64) / samples_per_bin).ceil() as usize;

    // Fast 1-pole crossover coefficients (200 Hz low-pass, 2500 Hz mid/high split)
    let dt = 1.0 / sample_rate as f32;
    let rc_low = 1.0 / (2.0 * PI * 200.0);
    let a_low = dt / (rc_low + dt);
    let rc_mid = 1.0 / (2.0 * PI * 2500.0);
    let a_mid = dt / (rc_mid + dt);

    let mut lp_low = 0.0f32;
    let mut lp_mid = 0.0f32;

    let mut bins = Vec::with_capacity(num_bins);
    let mut max_band = 1e-6f32;

    for b in 0..num_bins {
        let start = (b as f64 * samples_per_bin) as usize;
        let end = (((b + 1) as f64 * samples_per_bin) as usize).min(mono.len());
        if start >= end {
            bins.push(WaveformBin::default());
            continue;
        }
        let count = (end - start) as f32;
        let mut peak = 0.0f32;
        let mut rms_sq = 0.0f32;
        let mut low_sq = 0.0f32;
        let mut mid_sq = 0.0f32;
        let mut high_sq = 0.0f32;

        for &s in &mono[start..end] {
            let a = s.abs();
            if a > peak {
                peak = a;
            }
            rms_sq += s * s;

            lp_low += a_low * (s - lp_low);
            lp_mid += a_mid * (s - lp_mid);
            let low = lp_low;
            let mid = lp_mid - lp_low;
            let high = s - lp_mid;

            low_sq += low * low;
            mid_sq += mid * mid;
            high_sq += high * high;
        }

        let low_r = (low_sq / count).sqrt();
        let mid_r = (mid_sq / count).sqrt();
        let high_r = (high_sq / count).sqrt();
        max_band = max_band.max(low_r).max(mid_r).max(high_r);

        bins.push(WaveformBin {
            peak: peak.min(1.0),
            rms: (rms_sq / count).sqrt().min(1.0),
            low: low_r,
            mid: mid_r,
            high: high_r,
        });
    }

    // Normalize band energies relative to peak band energy so RGB waveform colors pop vibrantly
    let inv = 1.0 / max_band;
    for bin in &mut bins {
        bin.low = (bin.low * inv).clamp(0.0, 1.0);
        bin.mid = (bin.mid * inv).clamp(0.0, 1.0);
        bin.high = (bin.high * inv).clamp(0.0, 1.0);
    }

    bins
}
