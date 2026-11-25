use crate::shortcuts::KeyBindings;
use beatcheck_core::model::{MarkExportStyle, MarkKind, ResolveColor};
use beatcheck_edl::{ExportSettings, FrameRate};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WaveformVisualMode {
    /// Option C: Luminous Vector Outline — glowing electric-violet contour over obsidian canvas.
    LuminousOutline,
    /// DJ RGB Frequency Bands (Bass=Red, Mids=Green, Highs=Blue).
    RgbFrequency,
    /// Classic Resolve Mono filled waveform.
    ResolveMono,
}

impl WaveformVisualMode {
    pub const ALL: [WaveformVisualMode; 3] = [
        WaveformVisualMode::LuminousOutline,
        WaveformVisualMode::ResolveMono,
        WaveformVisualMode::RgbFrequency,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            WaveformVisualMode::LuminousOutline => "Outline",
            WaveformVisualMode::ResolveMono => "HD Solid",
            WaveformVisualMode::RgbFrequency => "RGB Bands",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            WaveformVisualMode::LuminousOutline => {
                "Precise glowing violet contour outline over a stealth dark interior"
            }
            WaveformVisualMode::ResolveMono => {
                "Maximum-detail Peak + RMS solid waveform for surgical frame alignment"
            }
            WaveformVisualMode::RgbFrequency => {
                "Frequency-colored DJ bands (Red = Kick/Bass, Green = Mids, Blue = Highs)"
            }
        }
    }

    pub fn next(&self) -> Self {
        match self {
            WaveformVisualMode::LuminousOutline => WaveformVisualMode::ResolveMono,
            WaveformVisualMode::ResolveMono => WaveformVisualMode::RgbFrequency,
            WaveformVisualMode::RgbFrequency => WaveformVisualMode::LuminousOutline,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScrollMode {
    CenteredTape,
    PageScroll,
}

/// Portable configuration saved automatically into `beatcheck.toml` next to the executable.
/// Zero Windows Registry / AppData writes; degrades gracefully on read-only media.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PortableConfig {
    pub export: ExportSettings,
    #[serde(default = "default_waveform_mode")]
    pub waveform_mode: WaveformVisualMode,
    pub scroll_mode: ScrollMode,
    pub metronome_enabled: bool,
    pub music_volume: f32,
    pub click_volume: f32,
    #[serde(default)]
    pub muted: bool,
    #[serde(default)]
    pub loop_enabled: bool,
    #[serde(default = "default_playback_speed")]
    pub playback_speed: f32,
    #[serde(default = "default_drawer_open")]
    pub drawer_open: bool,
    #[serde(default)]
    pub audio_output_device: Option<String>,
    #[serde(default)]
    pub recent_files: Vec<String>,
    #[serde(default)]
    pub shortcuts: KeyBindings,
}

fn default_waveform_mode() -> WaveformVisualMode {
    WaveformVisualMode::LuminousOutline
}

fn default_playback_speed() -> f32 {
    1.0
}

fn default_drawer_open() -> bool {
    true
}

impl Default for PortableConfig {
    fn default() -> Self {
        Self {
            export: ExportSettings::default(), // 25 fps, 01:00:00:00, Red/Yellow/Blue/Green
            waveform_mode: WaveformVisualMode::LuminousOutline,
            scroll_mode: ScrollMode::CenteredTape,
            metronome_enabled: true,
            music_volume: 0.85,
            click_volume: 0.65,
            muted: false,
            loop_enabled: false,
            playback_speed: 1.0,
            drawer_open: true,
            audio_output_device: None,
            recent_files: Vec::new(),
            shortcuts: KeyBindings::default(),
        }
    }
}

impl PortableConfig {
    pub fn config_path() -> Option<PathBuf> {
        let exe = std::env::current_exe().ok()?;
        let dir = exe.parent()?;
        // On macOS, store portable beatcheck.toml beside BeatCheck.app if portable;
        // if installed in /Applications or AppTranslocation (and no portable file exists there),
        // fall back to ~/Library/Application Support/BeatCheck/beatcheck.toml.
        #[cfg(target_os = "macos")]
        {
            if dir.file_name().and_then(|s| s.to_str()) == Some("MacOS") {
                if let Some(bundle_parent) = dir
                    .parent() // Contents
                    .and_then(|p| p.parent()) // BeatCheck.app
                    .and_then(|p| p.parent()) // folder containing BeatCheck.app
                {
                    let portable = bundle_parent.join("beatcheck.toml");
                    let parent_str = bundle_parent.to_string_lossy();
                    let is_system_or_translocated =
                        parent_str == "/Applications" || parent_str.contains("AppTranslocation");
                    if portable.exists() || !is_system_or_translocated {
                        return Some(portable);
                    }
                    if let Ok(home) = std::env::var("HOME") {
                        return Some(
                            PathBuf::from(home)
                                .join("Library")
                                .join("Application Support")
                                .join("BeatCheck")
                                .join("beatcheck.toml"),
                        );
                    }
                    return Some(portable);
                }
            }
        }
        Some(dir.join("beatcheck.toml"))
    }

    pub fn load_portable() -> Self {
        if let Some(path) = Self::config_path() {
            if let Ok(text) = std::fs::read_to_string(&path) {
                if let Ok(cfg) = toml::from_str::<Self>(&text) {
                    return cfg;
                }
            }
        }
        Self::default()
    }

    pub fn save_portable(&self) -> Result<PathBuf, String> {
        let path = Self::config_path()
            .ok_or_else(|| "Could not determine configuration path".to_string())?;
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let text = toml::to_string_pretty(self).map_err(|e| e.to_string())?;
        let tmp_path = path.with_extension("toml.tmp");
        // Atomic write via sibling temp file + rename so crashes or AV locks never truncate beatcheck.toml
        if std::fs::write(&tmp_path, &text).is_ok() {
            if std::fs::rename(&tmp_path, &path).is_ok() {
                return Ok(path);
            }
            let _ = std::fs::remove_file(&tmp_path);
        }
        std::fs::write(&path, text)
            .map_err(|e| format!("Read-only location or I/O error ({}): {e}", path.display()))?;
        Ok(path)
    }
}

/// Built-in one-click DaVinci Resolve editing presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvePreset {
    MusicVideoBarsAndDrops,
    AllBeatsAndDrops,
    HalfBarRhythm,
    SlowCinematic4Bar,
}

impl ResolvePreset {
    pub const ALL: [ResolvePreset; 4] = [
        ResolvePreset::MusicVideoBarsAndDrops,
        ResolvePreset::AllBeatsAndDrops,
        ResolvePreset::HalfBarRhythm,
        ResolvePreset::SlowCinematic4Bar,
    ];

    pub fn name(&self) -> &'static str {
        match self {
            ResolvePreset::MusicVideoBarsAndDrops => "Music Video",
            ResolvePreset::AllBeatsAndDrops => "Fast Montage",
            ResolvePreset::HalfBarRhythm => "Half-Bar",
            ResolvePreset::SlowCinematic4Bar => "4-Bar Cinematic",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            ResolvePreset::MusicVideoBarsAndDrops => "Downbeats + Energy Drops",
            ResolvePreset::AllBeatsAndDrops => "Every beat + Energy Drops",
            ResolvePreset::HalfBarRhythm => "Downbeats + Strong Beats + Drops",
            ResolvePreset::SlowCinematic4Bar => "Every 4th Bar + Energy Drops",
        }
    }

    pub fn matches(&self, style: &MarkExportStyle) -> bool {
        match self {
            ResolvePreset::AllBeatsAndDrops => {
                style.downbeat.enabled
                    && style.strong_beat.enabled
                    && style.regular_beat.enabled
                    && style.energy_drop.enabled
                    && style.downbeat_bar_stride == 1
            }
            ResolvePreset::MusicVideoBarsAndDrops => {
                style.downbeat.enabled
                    && !style.strong_beat.enabled
                    && !style.regular_beat.enabled
                    && style.energy_drop.enabled
                    && style.downbeat_bar_stride == 1
            }
            ResolvePreset::HalfBarRhythm => {
                style.downbeat.enabled
                    && style.strong_beat.enabled
                    && !style.regular_beat.enabled
                    && style.energy_drop.enabled
                    && style.downbeat_bar_stride == 1
            }
            ResolvePreset::SlowCinematic4Bar => {
                style.downbeat.enabled
                    && !style.strong_beat.enabled
                    && !style.regular_beat.enabled
                    && style.energy_drop.enabled
                    && style.downbeat_bar_stride == 4
            }
        }
    }

    pub fn apply(&self, style: &mut MarkExportStyle) {
        match self {
            ResolvePreset::AllBeatsAndDrops => {
                style.downbeat.enabled = true;
                style.strong_beat.enabled = true;
                style.regular_beat.enabled = true;
                style.energy_drop.enabled = true;
                style.downbeat_bar_stride = 1;
            }
            ResolvePreset::MusicVideoBarsAndDrops => {
                style.downbeat.enabled = true;
                style.strong_beat.enabled = false;
                style.regular_beat.enabled = false;
                style.energy_drop.enabled = true;
                style.downbeat_bar_stride = 1;
            }
            ResolvePreset::HalfBarRhythm => {
                style.downbeat.enabled = true;
                style.strong_beat.enabled = true;
                style.regular_beat.enabled = false;
                style.energy_drop.enabled = true;
                style.downbeat_bar_stride = 1;
            }
            ResolvePreset::SlowCinematic4Bar => {
                style.downbeat.enabled = true;
                style.strong_beat.enabled = false;
                style.regular_beat.enabled = false;
                style.energy_drop.enabled = true;
                style.downbeat_bar_stride = 4;
            }
        }
    }
}

pub fn resolve_color32(color: ResolveColor) -> egui::Color32 {
    let [r, g, b] = color.rgb();
    egui::Color32::from_rgb(r, g, b)
}

pub fn mark_color32(style: &MarkExportStyle, kind: MarkKind) -> egui::Color32 {
    resolve_color32(style.style_for(kind).color)
}

pub fn format_live_smpte(fps: FrameRate, start_tc: &str, playhead_sec: f64) -> String {
    let start_frame = fps.parse_smpte(start_tc).unwrap_or(0);
    let rel = fps.seconds_to_frames(playhead_sec).max(0) as u64;
    fps.format_frames(start_frame + rel)
}

/// Step a timestamp backward or forward by an exact integer number of timeline frames.
pub fn step_time_by_frames(
    cur_sec: f64,
    delta_frames: i32,
    fps: FrameRate,
    duration_sec: f64,
) -> f64 {
    let max_sec = duration_sec.max(0.0);
    let cur_frame = fps.seconds_to_frames(cur_sec.clamp(0.0, max_sec));
    let target_frame = (cur_frame + delta_frames as i64).max(0);
    fps.frames_to_seconds(target_frame).clamp(0.0, max_sec)
}

pub fn format_mm_ss(sec: f64) -> String {
    let total_cs = (sec.max(0.0) * 100.0).round() as u64;
    let mins = total_cs / 6000;
    let rem_cs = total_cs % 6000;
    let whole_s = rem_cs / 100;
    let frac = rem_cs % 100;
    format!("{mins:02}:{whole_s:02}.{frac:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_time_by_frames_advances_and_retreats_one_smpte_frame_each_click() {
        for fps in FrameRate::ALL {
            let start_tc = if fps.is_drop_frame() {
                "01:00:00;00"
            } else {
                "01:00:00:00"
            };
            let dur = 120.0;
            let mut t = 0.0;
            for expected_frame in 1..=75u64 {
                t = step_time_by_frames(t, 1, fps, dur);
                let expected_tc = fps.format_frames(fps.parse_smpte(start_tc).unwrap() + expected_frame);
                assert_eq!(
                    format_live_smpte(fps, start_tc, t),
                    expected_tc,
                    "forward step mismatch at frame {expected_frame} for {fps}"
                );
            }
            for expected_frame in (0..75u64).rev() {
                t = step_time_by_frames(t, -1, fps, dur);
                let expected_tc = fps.format_frames(fps.parse_smpte(start_tc).unwrap() + expected_frame);
                assert_eq!(
                    format_live_smpte(fps, start_tc, t),
                    expected_tc,
                    "backward step mismatch at frame {expected_frame} for {fps}"
                );
            }
            // Clamps cleanly at 0.0 and duration_sec
            assert_eq!(step_time_by_frames(0.0, -1, fps, dur), 0.0);
            assert_eq!(step_time_by_frames(dur, 1, fps, dur), dur);
        }
    }

    #[test]
    fn portable_config_without_shortcuts_loads_defaults() {
        let mut cfg = PortableConfig::default();
        cfg.metronome_enabled = false;
        let full_toml = toml::to_string_pretty(&cfg).expect("serialize PortableConfig");
        // Strip the [shortcuts] section to simulate a pre-existing beatcheck.toml
        let legacy_toml = full_toml
            .split("[shortcuts")
            .next()
            .expect("split legacy toml");
        let loaded: PortableConfig =
            toml::from_str(legacy_toml).expect("deserialize legacy PortableConfig");
        assert!(!loaded.metronome_enabled);
        assert_eq!(loaded.shortcuts, KeyBindings::default());
        assert_eq!(loaded.playback_speed, 1.0);
        assert!(loaded.drawer_open);
    }

    #[test]
    fn portable_config_persists_all_ui_and_audio_preferences() {
        let mut cfg = PortableConfig::default();
        cfg.metronome_enabled = false;
        cfg.waveform_mode = WaveformVisualMode::RgbFrequency;
        cfg.scroll_mode = ScrollMode::PageScroll;
        cfg.muted = true;
        cfg.loop_enabled = true;
        cfg.playback_speed = 0.75;
        cfg.drawer_open = false;
        cfg.audio_output_device = Some("Studio Monitors".to_string());
        cfg.music_volume = 0.55;
        cfg.click_volume = 0.35;

        let encoded = toml::to_string_pretty(&cfg).expect("serialize full PortableConfig");
        let decoded: PortableConfig =
            toml::from_str(&encoded).expect("deserialize full PortableConfig");
        assert_eq!(cfg, decoded);
    }
}
