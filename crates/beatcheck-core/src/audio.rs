use anyhow::{anyhow, Context, Result};
use std::fs::File;
use std::path::Path;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

/// Full in-memory decoded PCM audio for sample-accurate playback and analysis.
#[derive(Debug, Clone)]
pub struct DecodedAudio {
    pub title: String,
    pub file_path: String,
    pub sample_rate: u32,
    pub channels: u16,
    /// Interleaved stereo `f32` samples `[L0, R0, L1, R1, ...]` at `sample_rate`.
    pub stereo_pcm: Vec<f32>,
    /// Mono downmix `f32` samples at `sample_rate`.
    pub mono_pcm: Vec<f32>,
    pub duration_sec: f64,
}

/// Decode MP3, WAV, FLAC, OGG, AAC, or M4A into in-memory PCM with gapless LAME/Xing priming trim.
pub fn decode_audio_file<P: AsRef<Path>>(path: P) -> Result<DecodedAudio> {
    let path_ref = path.as_ref();
    let file = File::open(path_ref)
        .with_context(|| format!("Failed to open audio file: {}", path_ref.display()))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path_ref.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    // enable_gapless = true honors LAME/Xing/Info encoder delay & padding so MP3 timing aligns with DaVinci Resolve.
    let format_opts = FormatOptions {
        enable_gapless: true,
        ..Default::default()
    };
    let metadata_opts = MetadataOptions::default();

    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &format_opts, &metadata_opts)
        .map_err(|e| anyhow!("Unsupported or unreadable audio format: {e}"))?;

    let mut format = probed.format;

    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| anyhow!("No decodable audio track found in file"))?;

    let track_id = track.id;
    let codec_params = track.codec_params.clone();
    let sample_rate = codec_params
        .sample_rate
        .filter(|&sr| sr > 0)
        .ok_or_else(|| anyhow!("Audio stream missing or invalid sample rate"))?;
    let channels = codec_params
        .channels
        .map(|c| c.count() as u16)
        .unwrap_or(2);

    let mut decoder = symphonia::default::get_codecs()
        .make(&codec_params, &DecoderOptions::default())
        .map_err(|e| anyhow!("Failed to initialize audio decoder: {e}"))?;

    // Cap preallocation (max 10 min initial capacity) so malformed headers cannot trigger OOM abort
    let max_prealloc = (sample_rate as usize).saturating_mul(600);
    let estimated_frames = codec_params
        .n_frames
        .map(|n| (n as usize).min(max_prealloc))
        .unwrap_or_else(|| (sample_rate as usize).saturating_mul(180).min(max_prealloc));
    let mut stereo_pcm = Vec::with_capacity(estimated_frames.saturating_mul(2));
    let mut mono_pcm = Vec::with_capacity(estimated_frames);
    let mut sample_buf: Option<SampleBuffer<f32>> = None;

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(SymphoniaError::IoError(err))
                if err.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(SymphoniaError::ResetRequired) => {
                decoder.reset();
                continue;
            }
            Err(e) => return Err(anyhow!("Error reading audio packet: {e}")),
        };

        if packet.track_id() != track_id {
            continue;
        }

        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(SymphoniaError::IoError(_)) => break,
            Err(e) => return Err(anyhow!("Audio decode failure: {e}")),
        };

        let spec = *decoded.spec();
        let ch_count = spec.channels.count().max(1);
        let frames = decoded.frames();

        let buf = sample_buf.get_or_insert_with(|| SampleBuffer::<f32>::new(decoded.capacity() as u64, spec));
        if buf.capacity() < frames * ch_count {
            *buf = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
        }
        buf.copy_interleaved_ref(decoded);
        let samples = buf.samples();

        for frame_idx in 0..frames {
            let base = frame_idx * ch_count;
            let (l, r) = if ch_count == 1 {
                let s = samples[base];
                (s, s)
            } else {
                (samples[base], samples[base + 1])
            };
            stereo_pcm.push(l);
            stereo_pcm.push(r);
            mono_pcm.push(0.5 * (l + r));
        }
    }

    if mono_pcm.is_empty() {
        return Err(anyhow!("Decoded audio file contains zero samples"));
    }

    let duration_sec = mono_pcm.len() as f64 / sample_rate as f64;
    let title = path_ref
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Untitled")
        .to_string();

    Ok(DecodedAudio {
        title,
        file_path: path_ref.to_string_lossy().to_string(),
        sample_rate,
        channels,
        stereo_pcm,
        mono_pcm,
        duration_sec,
    })
}

/// Resample mono audio to `target_sr` (e.g. 22050 Hz) using anti-aliased windowed-sinc / linear interpolation.
pub fn resample_mono(input: &[f32], src_sr: u32, target_sr: u32) -> Vec<f32> {
    if src_sr == 0 || target_sr == 0 || src_sr == target_sr || input.is_empty() {
        return input.to_vec();
    }

    // Apply a light low-pass prefilter when downsampling to prevent aliasing above target Nyquist.
    let filtered: Vec<f32> = if target_sr < src_sr {
        let cutoff = (0.45 * target_sr as f64 / src_sr as f64).clamp(0.05, 0.49);
        lowpass_fir_3tap_or_windowed(input, cutoff)
    } else {
        input.to_vec()
    };

    let ratio = src_sr as f64 / target_sr as f64;
    let out_len = ((filtered.len() as f64) / ratio).round().max(1.0) as usize;
    let mut out = Vec::with_capacity(out_len);

    for i in 0..out_len {
        let pos = i as f64 * ratio;
        let idx = pos.floor() as usize;
        let frac = (pos - idx as f64) as f32;
        let s0 = filtered.get(idx).copied().unwrap_or(0.0);
        let s1 = filtered.get(idx + 1).copied().unwrap_or(s0);
        out.push(s0 + (s1 - s0) * frac);
    }
    out
}

fn lowpass_fir_3tap_or_windowed(input: &[f32], norm_cutoff: f64) -> Vec<f32> {
    // 15-tap Hann-windowed sinc low-pass filter (fast & clean anti-aliasing for 44.1/48k -> 22.05k)
    const HALF: isize = 7;
    const TAPS: usize = (HALF as usize) * 2 + 1;
    let mut kernel = [0.0f32; TAPS];
    let mut sum = 0.0f64;
    for (k, slot) in kernel.iter_mut().enumerate() {
        let n = k as isize - HALF;
        let sinc = if n == 0 {
            2.0 * norm_cutoff
        } else {
            let x = std::f64::consts::PI * n as f64;
            (2.0 * norm_cutoff * x).sin() / x
        };
        let win = 0.5 * (1.0 - (2.0 * std::f64::consts::PI * k as f64 / (TAPS - 1) as f64).cos());
        let val = sinc * win;
        *slot = val as f32;
        sum += val;
    }
    if sum.abs() > 1e-9 {
        for v in &mut kernel {
            *v /= sum as f32;
        }
    }

    let len = input.len();
    let mut out = vec![0.0f32; len];
    for i in 0..len {
        let mut acc = 0.0f32;
        for (k, &w) in kernel.iter().enumerate() {
            let idx = (i as isize + k as isize - HALF).clamp(0, (len - 1) as isize) as usize;
            acc += input[idx] * w;
        }
        out[i] = acc;
    }
    out
}
