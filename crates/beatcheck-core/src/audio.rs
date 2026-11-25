use anyhow::{anyhow, Context, Result};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_MP3, CODEC_TYPE_NULL};
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

/// Upper bound for MP3 priming delay restoration (Xing/Info header frame + LAME 12-bit delay + decoder delay is <= ~5,776 samples).
const MAX_MP3_PRIMING_SAMPLES: usize = 16_384;

fn mp3_priming_samples_for_codec(
    codec: symphonia::core::codecs::CodecType,
    delay: Option<u32>,
    info_header_frame_samples: usize,
) -> usize {
    if codec == CODEC_TYPE_MP3 {
        let total = (delay.unwrap_or(0) as usize).saturating_add(info_header_frame_samples);
        total.min(MAX_MP3_PRIMING_SAMPLES)
    } else {
        0
    }
}

/// Detect whether an MP3 file begins with a `Xing`, `Info`, or `VBRI` metadata header frame.
/// Symphonia's `MpaReader` always skips that first metadata frame (1152 samples for MPEG-1 Layer III,
/// 576 samples for MPEG-2/2.5 Layer III), whereas DaVinci Resolve decodes it as silent timeline samples.
/// Returning the exact sample count of that skipped header frame (or 0 when absent) keeps MP3s
/// sample-accurate with DaVinci Resolve's timeline waveform.
fn detect_mp3_info_header_frame_samples(path: &Path) -> usize {
    let Ok(mut file) = File::open(path) else {
        return 0;
    };

    // Skip one or more leading ID3v2 tags (including large embedded cover art).
    let mut offset: u64 = 0;
    let mut hdr = [0u8; 10];
    for _ in 0..8 {
        if file.seek(SeekFrom::Start(offset)).is_err() || file.read_exact(&mut hdr).is_err() {
            return 0;
        }
        if &hdr[0..3] != b"ID3"
            || (hdr[6] & 0x80) != 0
            || (hdr[7] & 0x80) != 0
            || (hdr[8] & 0x80) != 0
            || (hdr[9] & 0x80) != 0
        {
            break;
        }
        let tag_size = ((hdr[6] as u64) << 21)
            | ((hdr[7] as u64) << 14)
            | ((hdr[8] as u64) << 7)
            | (hdr[9] as u64);
        let footer = if (hdr[5] & 0x10) != 0 { 10u64 } else { 0u64 };
        offset = offset.saturating_add(10).saturating_add(tag_size).saturating_add(footer);
    }

    if file.seek(SeekFrom::Start(offset)).is_err() {
        return 0;
    }
    let mut buf = [0u8; 16_384];
    let mut total_read = 0usize;
    while total_read < buf.len() {
        match file.read(&mut buf[total_read..]) {
            Ok(0) => break,
            Ok(n) => total_read += n,
            Err(_) => return 0,
        }
    }
    if total_read < 4 {
        return 0;
    }
    detect_mp3_info_header_frame_samples_from_bytes(&buf[..total_read])
}

fn detect_mp3_info_header_frame_samples_from_bytes(bytes: &[u8]) -> usize {
    const BITRATES_V1_L1: [u32; 16] = [
        0, 32, 64, 96, 128, 160, 192, 224, 256, 288, 320, 352, 384, 416, 448, 0,
    ];
    const BITRATES_V1_L2: [u32; 16] = [
        0, 32, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 384, 0,
    ];
    const BITRATES_V1_L3: [u32; 16] = [
        0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 0,
    ];
    const BITRATES_V2_L1: [u32; 16] = [
        0, 32, 48, 56, 64, 80, 96, 112, 128, 144, 160, 176, 192, 224, 256, 0,
    ];
    const BITRATES_V2_L23: [u32; 16] = [
        0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160, 0,
    ];
    const SR_V1: [u32; 3] = [44_100, 48_000, 32_000];
    const SR_V2: [u32; 3] = [22_050, 24_000, 16_000];
    const SR_V25: [u32; 3] = [11_025, 12_000, 8_000];

    let mut cur = 0usize;
    while cur + 4 <= bytes.len() {
        if bytes[cur] == 0xFF && (bytes[cur + 1] & 0xE0) == 0xE0 {
            let b1 = bytes[cur + 1];
            let b2 = bytes[cur + 2];
            let b3 = bytes[cur + 3];
            let version_bits = (b1 >> 3) & 0x03; // 0=2.5, 1=reserved, 2=2, 3=1
            let layer_bits = (b1 >> 1) & 0x03; // 0=reserved, 1=L3, 2=L2, 3=L1
            let has_crc = (b1 & 0x01) == 0;
            let br_idx = ((b2 >> 4) & 0x0F) as usize;
            let sr_idx = ((b2 >> 2) & 0x03) as usize;
            let padding = ((b2 >> 1) & 0x01) as usize;
            let ch_mode = (b3 >> 6) & 0x03; // 3 = mono

            if version_bits != 1 && layer_bits != 0 && br_idx >= 1 && br_idx <= 14 && sr_idx <= 2 {
                let is_mpeg1 = version_bits == 3;
                let bitrate_kbps = match (is_mpeg1, layer_bits) {
                    (true, 3) => BITRATES_V1_L1[br_idx],
                    (true, 2) => BITRATES_V1_L2[br_idx],
                    (true, 1) => BITRATES_V1_L3[br_idx],
                    (false, 3) => BITRATES_V2_L1[br_idx],
                    (false, _) => BITRATES_V2_L23[br_idx],
                    _ => 0,
                };
                let sr = match version_bits {
                    3 => SR_V1[sr_idx],
                    2 => SR_V2[sr_idx],
                    0 => SR_V25[sr_idx],
                    _ => 0,
                };
                if bitrate_kbps > 0 && sr > 0 {
                    let samples_per_frame: usize = match (is_mpeg1, layer_bits) {
                        (_, 3) => 384,
                        (_, 2) => 1152,
                        (true, 1) => 1152,
                        (false, 1) => 576,
                        _ => 0,
                    };
                    let bitrate = bitrate_kbps * 1000;
                    let frame_len = if layer_bits == 3 {
                        ((12 * bitrate / sr) as usize + padding) * 4
                    } else {
                        ((samples_per_frame / 8) * bitrate as usize) / sr as usize + padding
                    };
                    if frame_len >= 24 {
                        // When enough bytes follow this candidate frame, verify the next MPEG sync word
                        // so random 0xFF bytes in pre-frame garbage cannot masquerade as Frame 0.
                        let next_sync_ok = if cur + frame_len + 2 <= bytes.len() {
                            bytes[cur + frame_len] == 0xFF
                                && (bytes[cur + frame_len + 1] & 0xE0) == 0xE0
                        } else {
                            true
                        };
                        if !next_sync_ok {
                            cur += 1;
                            continue;
                        }
                        let frame_end = (cur + frame_len).min(bytes.len());
                        let frame = &bytes[cur..frame_end];
                        let crc_len = if has_crc { 2 } else { 0 };
                        let side_info_len = match (is_mpeg1, ch_mode == 3) {
                            (true, true) => 17,
                            (true, false) => 32,
                            (false, true) => 9,
                            (false, false) => 17,
                        };
                        let xing_offsets = [4 + crc_len + side_info_len, 4 + side_info_len];
                        for off in xing_offsets {
                            if let Some(tag) = frame.get(off..off + 4) {
                                if tag == b"Xing" || tag == b"Info" {
                                    return samples_per_frame;
                                }
                            }
                        }
                        let vbri_offsets = [4 + crc_len + 32, 36];
                        for off in vbri_offsets {
                            if let Some(tag) = frame.get(off..off + 4) {
                                if tag == b"VBRI" {
                                    return samples_per_frame;
                                }
                            }
                        }
                        // First valid MPEG audio frame has no Xing/Info/VBRI header -> real audio starts at Frame 0.
                        return 0;
                    }
                }
            }
        }
        cur += 1;
    }
    0
}

/// Decode MP3, WAV, FLAC, OGG, AAC, or M4A into in-memory PCM aligned with DaVinci Resolve's timeline.
pub fn decode_audio_file<P: AsRef<Path>>(path: P) -> Result<DecodedAudio> {
    let path_ref = path.as_ref();
    let file = File::open(path_ref)
        .with_context(|| format!("Failed to open audio file: {}", path_ref.display()))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path_ref.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    // Keep enable_gapless = true so Symphonia cleanly parses container edit lists (M4A/MP4/OGG).
    // For MP3, Symphonia skips any leading Xing/Info/VBRI header frame (1152 or 576 samples) and
    // trims LAME/Lavc `codec_params.delay` (~1105 samples), whereas DaVinci Resolve preserves both
    // on the timeline — so we restore those exact priming samples below to stay 1:1 with Resolve!
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
    let info_header_frame_samples = if codec_params.codec == CODEC_TYPE_MP3 {
        detect_mp3_info_header_frame_samples(path_ref)
    } else {
        0
    };
    let mp3_priming_samples = mp3_priming_samples_for_codec(
        codec_params.codec,
        codec_params.delay,
        info_header_frame_samples,
    );

    let prealloc_frames = estimated_frames.saturating_add(mp3_priming_samples);
    let mut stereo_pcm = Vec::with_capacity(prealloc_frames.saturating_mul(2));
    let mut mono_pcm = Vec::with_capacity(prealloc_frames);
    if mp3_priming_samples > 0 {
        stereo_pcm.resize(mp3_priming_samples.saturating_mul(2), 0.0);
        mono_pcm.resize(mp3_priming_samples, 0.0);
    }
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

    if mono_pcm.len() <= mp3_priming_samples {
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

#[cfg(test)]
mod tests {
    use super::*;
    use symphonia::core::codecs::{CODEC_TYPE_AAC, CODEC_TYPE_FLAC, CODEC_TYPE_PCM_S16LE};

    #[test]
    fn mp3_priming_samples_is_capped_and_only_applied_to_mp3() {
        assert_eq!(
            mp3_priming_samples_for_codec(CODEC_TYPE_MP3, Some(1105), 1152),
            2257
        );
        assert_eq!(
            mp3_priming_samples_for_codec(CODEC_TYPE_MP3, None, 1152),
            1152
        );
        assert_eq!(mp3_priming_samples_for_codec(CODEC_TYPE_MP3, None, 0), 0);
        assert_eq!(
            mp3_priming_samples_for_codec(CODEC_TYPE_MP3, Some(u32::MAX), 1152),
            MAX_MP3_PRIMING_SAMPLES
        );
        assert_eq!(
            mp3_priming_samples_for_codec(CODEC_TYPE_PCM_S16LE, Some(1105), 1152),
            0
        );
        assert_eq!(
            mp3_priming_samples_for_codec(CODEC_TYPE_FLAC, Some(1105), 1152),
            0
        );
        assert_eq!(
            mp3_priming_samples_for_codec(CODEC_TYPE_AAC, Some(2112), 1152),
            0
        );
    }

    #[test]
    fn detects_mp3_xing_info_and_vbri_header_frames_accurately() {
        // MPEG-1 Layer III Stereo 256kbps 44.1kHz (frame len = 835 bytes), Info@36 (matches mixkit/Lavc)
        let mut mpeg1_info = vec![0u8; 835];
        mpeg1_info[0..4].copy_from_slice(&[0xFF, 0xFB, 0xD0, 0x00]);
        mpeg1_info[36..40].copy_from_slice(b"Info");
        assert_eq!(
            detect_mp3_info_header_frame_samples_from_bytes(&mpeg1_info),
            1152
        );

        // MPEG-1 Layer III Mono 128kbps 44.1kHz, Xing@21
        let mut mpeg1_mono_xing = vec![0u8; 417];
        mpeg1_mono_xing[0..4].copy_from_slice(&[0xFF, 0xFB, 0x90, 0xC0]);
        mpeg1_mono_xing[21..25].copy_from_slice(b"Xing");
        assert_eq!(
            detect_mp3_info_header_frame_samples_from_bytes(&mpeg1_mono_xing),
            1152
        );

        // MPEG-2 Layer III Stereo 64kbps 22.05kHz (samples_per_frame = 576), Xing@21
        let mut mpeg2_xing = vec![0u8; 208];
        mpeg2_xing[0..4].copy_from_slice(&[0xFF, 0xF3, 0x80, 0x00]);
        mpeg2_xing[21..25].copy_from_slice(b"Xing");
        assert_eq!(
            detect_mp3_info_header_frame_samples_from_bytes(&mpeg2_xing),
            576
        );

        // MPEG-1 Layer III with VBRI@36
        let mut mpeg1_vbri = vec![0u8; 417];
        mpeg1_vbri[0..4].copy_from_slice(&[0xFF, 0xFB, 0x90, 0x00]);
        mpeg1_vbri[36..40].copy_from_slice(b"VBRI");
        assert_eq!(
            detect_mp3_info_header_frame_samples_from_bytes(&mpeg1_vbri),
            1152
        );

        // Plain CBR MPEG-1 Layer III frame with no Xing/Info/VBRI tag -> 0
        let mut plain_cbr = vec![0u8; 835];
        plain_cbr[0..4].copy_from_slice(&[0xFF, 0xFB, 0xD0, 0x00]);
        assert_eq!(
            detect_mp3_info_header_frame_samples_from_bytes(&plain_cbr),
            0
        );
    }
}
