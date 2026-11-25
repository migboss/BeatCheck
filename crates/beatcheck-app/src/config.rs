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

/// Portable configuration saved optionally into `beatcheck.toml` next to the executable.
/// Zero Windows Registry / AppData writes; degrades gracefully on read-only media.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortableConfig {
    pub export: ExportSettings,
    #[serde(default = "default_waveform_mode")]
    pub waveform_mode: WaveformVisualMode,
    pub scroll_mode: ScrollMode,
    pub metronome_enabled: bool,
    pub music_volume: f32,
    pub click_volume: f32,
    #[serde(default)]
    pub recent_files: Vec<String>,
}

fn default_waveform_mode() -> WaveformVisualMode {
    WaveformVisualMode::LuminousOutline
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
            recent_files: Vec::new(),
        }
    }
}

impl PortableConfig {
    pub fn config_path() -> Option<PathBuf> {
        let exe = std::env::current_exe().ok()?;
        let dir = exe.parent()?;
        // On macOS, store portable beatcheck.toml beside BeatCheck.app rather than inside Contents/MacOS/
        #[cfg(target_os = "macos")]
        {
            if dir.file_name().and_then(|s| s.to_str()) == Some("MacOS") {
                if let Some(bundle_parent) = dir
                    .parent() // Contents
                    .and_then(|p| p.parent()) // BeatCheck.app
                    .and_then(|p| p.parent()) // folder containing BeatCheck.app
                {
                    return Some(bundle_parent.join("beatcheck.toml"));
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
            .ok_or_else(|| "Could not determine executable directory".to_string())?;
        let text = toml::to_string_pretty(self).map_err(|e| e.to_string())?;
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

pub fn format_mm_ss(sec: f64) -> String {
    let total_cs = (sec.max(0.0) * 100.0).round() as u64;
    let mins = total_cs / 6000;
    let rem_cs = total_cs % 6000;
    let whole_s = rem_cs / 100;
    let frac = rem_cs % 100;
    format!("{mins:02}:{whole_s:02}.{frac:02}")
}
