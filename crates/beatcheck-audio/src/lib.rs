use anyhow::{anyhow, Result};
use beatcheck_core::model::{BeatMark, MarkKind};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, SampleFormat, Stream, StreamConfig};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

const MAX_VOICES: usize = 16;

/// Active synthesized click / test-tone voice mixed directly inside the audio callback (zero heap allocs).
#[derive(Clone, Copy)]
struct ClickVoice {
    phase: f32,
    freq: f32,
    amp: f32,
    decay: f32,
    remaining_samples: usize,
    is_test_tone: bool,
}

#[derive(Default)]
struct SharedBuffer {
    stereo_pcm: Arc<Vec<f32>>,
    source_sr: u32,
    duration_sec: f64,
    clicks: Vec<(f64, MarkKind)>,
}

struct SharedControl {
    playing: AtomicBool,
    muted: AtomicBool,
    metronome_enabled: AtomicBool,
    loop_enabled: AtomicBool,
    test_tone_requested: AtomicBool,
    /// Authoritative playhead published by the audio callback (read by UI).
    playhead_bits: AtomicU64,
    /// Dedicated seek target written by UI before incrementing `seek_generation` (eliminates TOCTOU race).
    seek_target_bits: AtomicU64,
    seek_generation: AtomicU64,
    /// Separate generation counter for click-schedule edits so live editing never stutters audio playback!
    clicks_generation: AtomicU64,
    loop_in_bits: AtomicU64,
    loop_out_bits: AtomicU64,
    music_volume_bits: AtomicU32,
    click_volume_bits: AtomicU32,
    speed_bits: AtomicU32,
    output_peak_bits: AtomicU32,
    callback_ticks: AtomicU64,
}

impl Default for SharedControl {
    fn default() -> Self {
        Self {
            playing: AtomicBool::new(false),
            muted: AtomicBool::new(false),
            metronome_enabled: AtomicBool::new(true),
            loop_enabled: AtomicBool::new(false),
            test_tone_requested: AtomicBool::new(false),
            playhead_bits: AtomicU64::new(0.0f64.to_bits()),
            seek_target_bits: AtomicU64::new(0.0f64.to_bits()),
            seek_generation: AtomicU64::new(0),
            clicks_generation: AtomicU64::new(0),
            loop_in_bits: AtomicU64::new(0.0f64.to_bits()),
            loop_out_bits: AtomicU64::new(0.0f64.to_bits()),
            music_volume_bits: AtomicU32::new(0.85f32.to_bits()),
            click_volume_bits: AtomicU32::new(0.65f32.to_bits()),
            speed_bits: AtomicU32::new(1.0f32.to_bits()),
            output_peak_bits: AtomicU32::new(0.0f32.to_bits()),
            callback_ticks: AtomicU64::new(0),
        }
    }
}

struct CallbackEngine {
    out_sr: f64,
    voices: [Option<ClickVoice>; MAX_VOICES],
    next_click_idx: usize,
    last_seek_gen: u64,
    last_clicks_gen: u64,
    needs_click_resync: bool,
    local_pos: f64,
    gain_ramp: f32,
}

impl CallbackEngine {
    fn new(out_sr: f64) -> Self {
        Self {
            out_sr: out_sr.max(8000.0),
            voices: [None; MAX_VOICES],
            next_click_idx: 0,
            last_seek_gen: u64::MAX,
            last_clicks_gen: u64::MAX,
            needs_click_resync: true,
            local_pos: 0.0,
            gain_ramp: 0.0,
        }
    }

    fn spawn_voice(&mut self, voice: ClickVoice) {
        if let Some(slot) = self.voices.iter_mut().find(|v| v.is_none()) {
            *slot = Some(voice);
        } else {
            // Overwrite the voice closest to finishing
            if let Some(slot) = self
                .voices
                .iter_mut()
                .min_by_key(|v| v.map(|x| x.remaining_samples).unwrap_or(0))
            {
                *slot = Some(voice);
            }
        }
    }

    /// Render an entire audio buffer while holding `buffer.try_lock()` AT MOST ONCE per callback!
    fn render_buffer<F>(&mut self, buffer: &Mutex<SharedBuffer>, ctrl: &SharedControl, frames: usize, mut write_frame: F)
    where
        F: FnMut(usize, f32, f32),
    {
        if ctrl.test_tone_requested.swap(false, Ordering::Relaxed) {
            let len = (self.out_sr * 0.24) as usize;
            self.spawn_voice(ClickVoice {
                phase: 0.0,
                freq: 440.0,
                amp: 0.45,
                decay: 1.0 / (len.max(1) as f32),
                remaining_samples: len,
                is_test_tone: true,
            });
        }

        let is_playing = ctrl.playing.load(Ordering::Relaxed);
        let muted = ctrl.muted.load(Ordering::Relaxed);
        let music_vol = if muted {
            0.0
        } else {
            f32::from_bits(ctrl.music_volume_bits.load(Ordering::Relaxed))
        };
        let click_vol = if muted || !ctrl.metronome_enabled.load(Ordering::Relaxed) {
            0.0
        } else {
            f32::from_bits(ctrl.click_volume_bits.load(Ordering::Relaxed))
        };
        let speed = f32::from_bits(ctrl.speed_bits.load(Ordering::Relaxed)) as f64;
        let loop_on = ctrl.loop_enabled.load(Ordering::Relaxed);
        let loop_in = f64::from_bits(ctrl.loop_in_bits.load(Ordering::Relaxed));
        let loop_out = f64::from_bits(ctrl.loop_out_bits.load(Ordering::Relaxed));

        // Lock shared PCM/click buffer ONCE per entire audio callback (zero per-sample mutex contention!)
        let buf_guard = buffer.try_lock().ok();

        let seek_gen = ctrl.seek_generation.load(Ordering::Acquire);
        if seek_gen != self.last_seek_gen {
            self.last_seek_gen = seek_gen;
            let target = f64::from_bits(ctrl.seek_target_bits.load(Ordering::Relaxed));
            if target.is_finite() {
                self.local_pos = target.max(0.0);
            }
            self.needs_click_resync = true;
            for v in &mut self.voices {
                if let Some(voice) = v {
                    if !voice.is_test_tone {
                        *v = None;
                    }
                }
            }
        }

        let clicks_gen = ctrl.clicks_generation.load(Ordering::Acquire);
        if clicks_gen != self.last_clicks_gen {
            self.last_clicks_gen = clicks_gen;
            self.needs_click_resync = true;
        }

        // Only clear `needs_click_resync` once we actually hold `buf_guard` so a contended try_lock()
        // never permanently silences metronome clicks!
        if self.needs_click_resync {
            if let Some(buf) = buf_guard.as_ref() {
                self.next_click_idx = buf
                    .clicks
                    .partition_point(|&(t, _)| t < self.local_pos - 0.002);
                self.needs_click_resync = false;
            }
        }

        let ramp_step = 1.0 / ((self.out_sr * 0.005) as f32).max(1.0); // ~5ms anti-click micro-fade
        let two_pi = std::f32::consts::TAU;
        let dt = 1.0 / self.out_sr as f32;
        let step_sec = speed / self.out_sr;

        let mut peak = 0.0f32;
        let mut active_playing = is_playing;

        for frame_i in 0..frames {
            let mut l = 0.0f32;
            let mut r = 0.0f32;

            let target_gain = if active_playing && !muted { 1.0 } else { 0.0 };
            if (self.gain_ramp - target_gain).abs() <= ramp_step {
                self.gain_ramp = target_gain;
            } else if self.gain_ramp < target_gain {
                self.gain_ramp += ramp_step;
            } else {
                self.gain_ramp -= ramp_step;
            }

            if active_playing {
                if let Some(buf) = buf_guard.as_ref() {
                    if !buf.stereo_pcm.is_empty() && buf.source_sr > 0 {
                        let pos = self.local_pos;
                        let next_pos = pos + step_sec;

                        while self.next_click_idx < buf.clicks.len()
                            && buf.clicks[self.next_click_idx].0 <= next_pos
                        {
                            let (t, kind) = buf.clicks[self.next_click_idx];
                            if t >= pos - 0.02 && click_vol > 0.001 {
                                let (freq, amp, dur_ms) = match kind {
                                    MarkKind::Downbeat => (1620.0f32, 0.95f32, 34.0f64),
                                    MarkKind::StrongBeat => (1140.0f32, 0.65f32, 26.0f64),
                                    _ => (780.0f32, 0.42f32, 20.0f64),
                                };
                                let len = ((self.out_sr * dur_ms) / 1000.0) as usize;
                                self.spawn_voice(ClickVoice {
                                    phase: 0.0,
                                    freq,
                                    amp,
                                    decay: 1.0 / (len.max(1) as f32),
                                    remaining_samples: len,
                                    is_test_tone: false,
                                });
                            }
                            self.next_click_idx += 1;
                        }

                        let src_pos = pos * buf.source_sr as f64;
                        let idx = src_pos.floor() as usize;
                        let frac = (src_pos - idx as f64) as f32;
                        let total_frames = buf.stereo_pcm.len() / 2;

                        if idx < total_frames {
                            let l0 = buf.stereo_pcm[idx * 2];
                            let r0 = buf.stereo_pcm[idx * 2 + 1];
                            let next_i = (idx + 1).min(total_frames - 1);
                            let l1 = buf.stereo_pcm[next_i * 2];
                            let r1 = buf.stereo_pcm[next_i * 2 + 1];
                            l = (l0 + (l1 - l0) * frac) * music_vol * self.gain_ramp;
                            r = (r0 + (r1 - r0) * frac) * music_vol * self.gain_ramp;
                        }

                        self.local_pos = next_pos;
                        if loop_on && loop_out > loop_in + 0.05 && self.local_pos >= loop_out {
                            self.local_pos = loop_in;
                            self.next_click_idx = buf.clicks.partition_point(|&(t, _)| t < loop_in);
                        } else if self.local_pos >= buf.duration_sec {
                            self.local_pos = buf.duration_sec;
                            active_playing = false;
                            ctrl.playing.store(false, Ordering::Relaxed);
                        }
                    }
                }
            }

            // Mix active click / test-tone voices from stack array
            let mut sum = 0.0f32;
            for slot in &mut self.voices {
                if let Some(v) = slot {
                    if v.remaining_samples == 0 {
                        *slot = None;
                        continue;
                    }
                    let env = (v.remaining_samples as f32 * v.decay).powi(2);
                    let sample = if v.is_test_tone {
                        (two_pi * v.phase).sin() * env * v.amp
                    } else {
                        ((two_pi * v.phase).sin() + 0.25 * (two_pi * 2.1 * v.phase).sin())
                            * env
                            * v.amp
                            * click_vol
                    };
                    sum += sample;
                    v.phase += v.freq * dt;
                    v.remaining_samples -= 1;
                    if v.remaining_samples == 0 {
                        *slot = None;
                    }
                }
            }
            l = (l + sum).tanh();
            r = (r + sum).tanh();

            peak = peak.max(l.abs().max(r.abs()));
            write_frame(frame_i, l, r);
        }

        ctrl.playhead_bits
            .store(self.local_pos.to_bits(), Ordering::Relaxed);
        ctrl.output_peak_bits
            .store(peak.to_bits(), Ordering::Relaxed);
        ctrl.callback_ticks.fetch_add(1, Ordering::Relaxed);
    }
}

/// Real-time cross-platform audio player (WASAPI on Windows, CoreAudio on macOS).
pub struct AudioPlayer {
    stream: Option<Stream>,
    buffer: Arc<Mutex<SharedBuffer>>,
    ctrl: Arc<SharedControl>,
    stream_error: Arc<Mutex<Option<String>>>,
    selected_device_name: Option<String>,
    active_device_name: String,
    active_format_info: String,
    pub init_error: Option<String>,
}

impl AudioPlayer {
    pub fn new() -> Self {
        let mut player = Self {
            stream: None,
            buffer: Arc::new(Mutex::new(SharedBuffer::default())),
            ctrl: Arc::new(SharedControl::default()),
            stream_error: Arc::new(Mutex::new(None)),
            selected_device_name: None,
            active_device_name: "System Default".to_string(),
            active_format_info: String::new(),
            init_error: None,
        };
        let _ = player.open_stream(None);
        player
    }

    pub fn list_output_devices() -> Vec<String> {
        let host = cpal::default_host();
        let Ok(devs) = host.output_devices() else {
            return Vec::new();
        };
        let mut names = Vec::new();
        for d in devs {
            if let Ok(name) = d.name() {
                if !names.contains(&name) {
                    names.push(name);
                }
            }
        }
        names
    }

    pub fn selected_device_name(&self) -> Option<&str> {
        self.selected_device_name.as_deref()
    }

    pub fn active_device_name(&self) -> &str {
        &self.active_device_name
    }

    pub fn active_format_info(&self) -> &str {
        &self.active_format_info
    }

    pub fn stream_error(&self) -> Option<String> {
        if let Some(err) = &self.init_error {
            return Some(err.clone());
        }
        self.stream_error.lock().ok().and_then(|g| g.clone())
    }

    pub fn output_peak(&self) -> f32 {
        f32::from_bits(self.ctrl.output_peak_bits.load(Ordering::Relaxed))
    }

    pub fn trigger_test_tone(&self) {
        self.ctrl.test_tone_requested.store(true, Ordering::Relaxed);
    }

    pub fn select_output_device(&mut self, device_name: Option<String>) -> Result<()> {
        self.open_stream(device_name)
    }

    pub fn reconnect(&mut self) -> Result<()> {
        let target = self.selected_device_name.clone();
        self.open_stream(target)
    }

    fn open_stream(&mut self, device_name: Option<String>) -> Result<()> {
        self.stream = None;
        if let Ok(mut e) = self.stream_error.lock() {
            *e = None;
        }

        match create_output_stream(
            Arc::clone(&self.buffer),
            Arc::clone(&self.ctrl),
            Arc::clone(&self.stream_error),
            device_name.as_deref(),
        ) {
            Ok((stream, dev_name, fmt_info, matched_requested)) => {
                if let Err(err) = stream.play() {
                    let msg = format!("Failed to start audio stream on '{dev_name}': {err}");
                    self.init_error = Some(msg.clone());
                    return Err(anyhow!(msg));
                }
                self.stream = Some(stream);
                // If requested device was unplugged and we fell back to default, reflect that cleanly
                self.selected_device_name = if matched_requested { device_name } else { None };
                self.active_device_name = dev_name;
                self.active_format_info = fmt_info;
                self.init_error = None;
                // Re-sync playhead in the newly created callback engine
                let cur = self.position_sec();
                self.seek(cur);
                Ok(())
            }
            Err(err) => {
                let msg = err.to_string();
                self.init_error = Some(msg.clone());
                self.selected_device_name = None;
                self.active_device_name = "Unavailable".to_string();
                self.active_format_info.clear();
                Err(anyhow!(msg))
            }
        }
    }

    pub fn load_track(
        &self,
        stereo_pcm: Arc<Vec<f32>>,
        source_sr: u32,
        duration_sec: f64,
        marks: &[BeatMark],
    ) {
        self.set_playing(false);
        if let Ok(mut buf) = self.buffer.lock() {
            buf.stereo_pcm = stereo_pcm;
            buf.source_sr = source_sr;
            buf.duration_sec = duration_sec;
            buf.clicks = extract_click_schedule(marks);
        }
        self.set_loop_range(0.0, duration_sec);
        self.seek(0.0);
    }

    pub fn update_marks(&self, marks: &[BeatMark]) {
        if let Ok(mut buf) = self.buffer.lock() {
            buf.clicks = extract_click_schedule(marks);
        }
        // Only bump clicks_generation so audio playback never stutters or seeks backward during live edits!
        self.ctrl.clicks_generation.fetch_add(1, Ordering::Release);
    }

    pub fn is_playing(&self) -> bool {
        self.ctrl.playing.load(Ordering::Relaxed)
    }

    pub fn set_playing(&self, playing: bool) {
        if playing {
            let dur = self
                .buffer
                .lock()
                .map(|b| b.duration_sec)
                .unwrap_or(0.0);
            if dur > 0.0 && self.position_sec() >= dur - 0.02 {
                let target = if self.loop_enabled() {
                    self.loop_in_sec()
                } else {
                    0.0
                };
                self.seek(target);
            }
        }
        self.ctrl.playing.store(playing, Ordering::Relaxed);
    }

    pub fn toggle_play(&self) {
        let now = self.is_playing();
        self.set_playing(!now);
    }

    pub fn position_sec(&self) -> f64 {
        f64::from_bits(self.ctrl.playhead_bits.load(Ordering::Relaxed))
    }

    pub fn seek(&self, sec: f64) {
        if !sec.is_finite() {
            return;
        }
        let dur = self
            .buffer
            .lock()
            .map(|b| b.duration_sec)
            .unwrap_or(0.0);
        let clamped = sec.clamp(0.0, dur.max(0.0));
        self.ctrl
            .seek_target_bits
            .store(clamped.to_bits(), Ordering::Relaxed);
        self.ctrl
            .playhead_bits
            .store(clamped.to_bits(), Ordering::Relaxed);
        self.ctrl.seek_generation.fetch_add(1, Ordering::Release);
    }

    pub fn set_music_volume(&self, vol: f32) {
        self.ctrl
            .music_volume_bits
            .store(vol.clamp(0.0, 1.5).to_bits(), Ordering::Relaxed);
    }

    pub fn music_volume(&self) -> f32 {
        f32::from_bits(self.ctrl.music_volume_bits.load(Ordering::Relaxed))
    }

    pub fn set_click_volume(&self, vol: f32) {
        self.ctrl
            .click_volume_bits
            .store(vol.clamp(0.0, 1.5).to_bits(), Ordering::Relaxed);
    }

    pub fn click_volume(&self) -> f32 {
        f32::from_bits(self.ctrl.click_volume_bits.load(Ordering::Relaxed))
    }

    pub fn set_muted(&self, muted: bool) {
        self.ctrl.muted.store(muted, Ordering::Relaxed);
    }

    pub fn is_muted(&self) -> bool {
        self.ctrl.muted.load(Ordering::Relaxed)
    }

    pub fn set_metronome_enabled(&self, enabled: bool) {
        self.ctrl.metronome_enabled.store(enabled, Ordering::Relaxed);
    }

    pub fn metronome_enabled(&self) -> bool {
        self.ctrl.metronome_enabled.load(Ordering::Relaxed)
    }

    pub fn set_speed(&self, speed: f32) {
        self.ctrl
            .speed_bits
            .store(speed.clamp(0.5, 1.5).to_bits(), Ordering::Relaxed);
    }

    pub fn speed(&self) -> f32 {
        f32::from_bits(self.ctrl.speed_bits.load(Ordering::Relaxed))
    }

    pub fn set_loop_enabled(&self, enabled: bool) {
        self.ctrl.loop_enabled.store(enabled, Ordering::Relaxed);
    }

    pub fn loop_enabled(&self) -> bool {
        self.ctrl.loop_enabled.load(Ordering::Relaxed)
    }

    pub fn set_loop_range(&self, in_sec: f64, out_sec: f64) {
        self.ctrl
            .loop_in_bits
            .store(in_sec.max(0.0).to_bits(), Ordering::Relaxed);
        self.ctrl
            .loop_out_bits
            .store(out_sec.max(in_sec).to_bits(), Ordering::Relaxed);
    }

    pub fn loop_in_sec(&self) -> f64 {
        f64::from_bits(self.ctrl.loop_in_bits.load(Ordering::Relaxed))
    }
}

impl Default for AudioPlayer {
    fn default() -> Self {
        Self::new()
    }
}

fn extract_click_schedule(marks: &[BeatMark]) -> Vec<(f64, MarkKind)> {
    let mut out: Vec<(f64, MarkKind)> = marks
        .iter()
        .filter(|m| {
            matches!(
                m.kind,
                MarkKind::Downbeat | MarkKind::StrongBeat | MarkKind::RegularBeat
            )
        })
        .map(|m| (m.time_sec, m.kind))
        .collect();
    out.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    out
}

fn pick_output_device(preferred_name: Option<&str>) -> Result<(Device, bool)> {
    let host = cpal::default_host();
    if let Some(wanted) = preferred_name {
        if let Ok(devs) = host.output_devices() {
            for d in devs {
                if d.name().ok().as_deref() == Some(wanted) {
                    return Ok((d, true));
                }
            }
        }
    }
    let dev = host
        .default_output_device()
        .ok_or_else(|| anyhow!("No audio output device found"))?;
    Ok((dev, preferred_name.is_none()))
}

fn pick_best_output_config(device: &Device) -> Result<(SampleFormat, StreamConfig)> {
    if let Ok(def) = device.default_output_config() {
        let fmt = def.sample_format();
        if matches!(
            fmt,
            SampleFormat::F32
                | SampleFormat::F64
                | SampleFormat::I16
                | SampleFormat::I32
                | SampleFormat::U16
        ) {
            return Ok((fmt, def.into()));
        }
    }

    if let Ok(ranges) = device.supported_output_configs() {
        for range in ranges {
            let fmt = range.sample_format();
            if matches!(
                fmt,
                SampleFormat::F32 | SampleFormat::I16 | SampleFormat::I32
            ) {
                let sr = if range.min_sample_rate().0 <= 48_000 && range.max_sample_rate().0 >= 48_000
                {
                    cpal::SampleRate(48_000)
                } else if range.min_sample_rate().0 <= 44_100 && range.max_sample_rate().0 >= 44_100
                {
                    cpal::SampleRate(44_100)
                } else {
                    range.max_sample_rate()
                };
                let chosen = range.with_sample_rate(sr);
                return Ok((fmt, chosen.into()));
            }
        }
    }

    let def = device
        .default_output_config()
        .map_err(|e| anyhow!("Failed to query audio output config: {e}"))?;
    Ok((def.sample_format(), def.into()))
}

fn create_output_stream(
    buffer: Arc<Mutex<SharedBuffer>>,
    ctrl: Arc<SharedControl>,
    stream_error: Arc<Mutex<Option<String>>>,
    preferred_device: Option<&str>,
) -> Result<(Stream, String, String, bool)> {
    let (device, matched_requested) = pick_output_device(preferred_device)?;
    let dev_name = device
        .name()
        .unwrap_or_else(|_| "Default Output Device".to_string());
    let (sample_format, config) = pick_best_output_config(&device)?;
    let out_sr = config.sample_rate.0 as f64;
    let out_channels = (config.channels as usize).max(1);
    let fmt_info = format!(
        "{} Hz · {} ch · {:?}",
        config.sample_rate.0, out_channels, sample_format
    );

    let err_slot = Arc::clone(&stream_error);
    let err_fn = move |err| {
        if let Ok(mut g) = err_slot.lock() {
            *g = Some(format!("Audio stream error: {err}"));
        }
    };

    let mut engine = CallbackEngine::new(out_sr);
    let stream = match sample_format {
        SampleFormat::F32 => device.build_output_stream(
            &config,
            move |data: &mut [f32], _| {
                let n_frames = data.len() / out_channels;
                engine.render_buffer(&buffer, &ctrl, n_frames, |i, l, r| {
                    let base = i * out_channels;
                    if out_channels == 1 {
                        data[base] = 0.5 * (l + r);
                    } else {
                        data[base] = l;
                        data[base + 1] = r;
                        for ch in 2..out_channels {
                            data[base + ch] = 0.0;
                        }
                    }
                });
            },
            err_fn,
            None,
        )?,
        SampleFormat::F64 => device.build_output_stream(
            &config,
            move |data: &mut [f64], _| {
                let n_frames = data.len() / out_channels;
                engine.render_buffer(&buffer, &ctrl, n_frames, |i, l, r| {
                    let base = i * out_channels;
                    if out_channels == 1 {
                        data[base] = 0.5 * (l + r) as f64;
                    } else {
                        data[base] = l as f64;
                        data[base + 1] = r as f64;
                        for ch in 2..out_channels {
                            data[base + ch] = 0.0;
                        }
                    }
                });
            },
            err_fn,
            None,
        )?,
        SampleFormat::I16 => device.build_output_stream(
            &config,
            move |data: &mut [i16], _| {
                let n_frames = data.len() / out_channels;
                engine.render_buffer(&buffer, &ctrl, n_frames, |i, l, r| {
                    let base = i * out_channels;
                    let li = (l.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
                    let ri = (r.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
                    if out_channels == 1 {
                        data[base] = ((li as i32 + ri as i32) / 2) as i16;
                    } else {
                        data[base] = li;
                        data[base + 1] = ri;
                        for ch in 2..out_channels {
                            data[base + ch] = 0;
                        }
                    }
                });
            },
            err_fn,
            None,
        )?,
        SampleFormat::I32 => device.build_output_stream(
            &config,
            move |data: &mut [i32], _| {
                let n_frames = data.len() / out_channels;
                engine.render_buffer(&buffer, &ctrl, n_frames, |i, l, r| {
                    let base = i * out_channels;
                    let li = (l.clamp(-1.0, 1.0) as f64 * i32::MAX as f64) as i32;
                    let ri = (r.clamp(-1.0, 1.0) as f64 * i32::MAX as f64) as i32;
                    if out_channels == 1 {
                        data[base] = ((li as i64 + ri as i64) / 2) as i32;
                    } else {
                        data[base] = li;
                        data[base + 1] = ri;
                        for ch in 2..out_channels {
                            data[base + ch] = 0;
                        }
                    }
                });
            },
            err_fn,
            None,
        )?,
        SampleFormat::U16 => device.build_output_stream(
            &config,
            move |data: &mut [u16], _| {
                let n_frames = data.len() / out_channels;
                engine.render_buffer(&buffer, &ctrl, n_frames, |i, l, r| {
                    let base = i * out_channels;
                    let lu = ((l.clamp(-1.0, 1.0) * 0.5 + 0.5) * u16::MAX as f32) as u16;
                    let ru = ((r.clamp(-1.0, 1.0) * 0.5 + 0.5) * u16::MAX as f32) as u16;
                    if out_channels == 1 {
                        data[base] = ((lu as u32 + ru as u32) / 2) as u16;
                    } else {
                        data[base] = lu;
                        data[base + 1] = ru;
                        for ch in 2..out_channels {
                            data[base + ch] = u16::MAX / 2;
                        }
                    }
                });
            },
            err_fn,
            None,
        )?,
        other => return Err(anyhow!("Unsupported output sample format: {other:?}")),
    };

    Ok((stream, dev_name, fmt_info, matched_requested))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_and_init_audio_output() {
        let devs = AudioPlayer::list_output_devices();
        println!("Output devices ({}): {:?}", devs.len(), devs);
        let player = AudioPlayer::new();
        println!(
            "Active device: '{}' | format: '{}' | err: {:?}",
            player.active_device_name(),
            player.active_format_info(),
            player.stream_error()
        );
    }
}
