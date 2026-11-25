use serde::{Deserialize, Serialize};
use std::fmt;

/// All 16 official DaVinci Resolve marker colors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ResolveColor {
    Red,
    Blue,
    Cyan,
    Green,
    Yellow,
    Pink,
    Purple,
    Fuchsia,
    Rose,
    Lavender,
    Sky,
    Mint,
    Lemon,
    Sand,
    Cocoa,
    Cream,
}

impl ResolveColor {
    pub const ALL: [ResolveColor; 16] = [
        ResolveColor::Red,
        ResolveColor::Yellow,
        ResolveColor::Blue,
        ResolveColor::Green,
        ResolveColor::Cyan,
        ResolveColor::Purple,
        ResolveColor::Pink,
        ResolveColor::Fuchsia,
        ResolveColor::Rose,
        ResolveColor::Lavender,
        ResolveColor::Sky,
        ResolveColor::Mint,
        ResolveColor::Lemon,
        ResolveColor::Sand,
        ResolveColor::Cocoa,
        ResolveColor::Cream,
    ];

    /// Exact CMX 3600 Resolve marker color tag value.
    pub fn edl_name(&self) -> &'static str {
        match self {
            ResolveColor::Red => "ResolveColorRed",
            ResolveColor::Blue => "ResolveColorBlue",
            ResolveColor::Cyan => "ResolveColorCyan",
            ResolveColor::Green => "ResolveColorGreen",
            ResolveColor::Yellow => "ResolveColorYellow",
            ResolveColor::Pink => "ResolveColorPink",
            ResolveColor::Purple => "ResolveColorPurple",
            ResolveColor::Fuchsia => "ResolveColorFuchsia",
            ResolveColor::Rose => "ResolveColorRose",
            ResolveColor::Lavender => "ResolveColorLavender",
            ResolveColor::Sky => "ResolveColorSky",
            ResolveColor::Mint => "ResolveColorMint",
            ResolveColor::Lemon => "ResolveColorLemon",
            ResolveColor::Sand => "ResolveColorSand",
            ResolveColor::Cocoa => "ResolveColorCocoa",
            ResolveColor::Cream => "ResolveColorCream",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            ResolveColor::Red => "Red",
            ResolveColor::Blue => "Blue",
            ResolveColor::Cyan => "Cyan",
            ResolveColor::Green => "Green",
            ResolveColor::Yellow => "Yellow",
            ResolveColor::Pink => "Pink",
            ResolveColor::Purple => "Purple",
            ResolveColor::Fuchsia => "Fuchsia",
            ResolveColor::Rose => "Rose",
            ResolveColor::Lavender => "Lavender",
            ResolveColor::Sky => "Sky",
            ResolveColor::Mint => "Mint",
            ResolveColor::Lemon => "Lemon",
            ResolveColor::Sand => "Sand",
            ResolveColor::Cocoa => "Cocoa",
            ResolveColor::Cream => "Cream",
        }
    }

    /// sRGB color matching DaVinci Resolve's timeline marker swatches.
    pub fn rgb(&self) -> [u8; 3] {
        match self {
            ResolveColor::Red => [231, 60, 60],
            ResolveColor::Blue => [58, 134, 255],
            ResolveColor::Cyan => [0, 198, 215],
            ResolveColor::Green => [46, 204, 113],
            ResolveColor::Yellow => [244, 196, 48],
            ResolveColor::Pink => [247, 120, 161],
            ResolveColor::Purple => [155, 89, 182],
            ResolveColor::Fuchsia => [217, 70, 239],
            ResolveColor::Rose => [244, 63, 94],
            ResolveColor::Lavender => [167, 139, 250],
            ResolveColor::Sky => [56, 189, 248],
            ResolveColor::Mint => [52, 211, 153],
            ResolveColor::Lemon => [250, 224, 83],
            ResolveColor::Sand => [212, 163, 115],
            ResolveColor::Cocoa => [138, 90, 68],
            ResolveColor::Cream => [245, 235, 210],
        }
    }
}

impl fmt::Display for ResolveColor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.display_name())
    }
}

/// Core mark categories detected by BeatCheck.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MarkKind {
    Downbeat,
    StrongBeat,
    RegularBeat,
    EnergyDrop,
}

impl MarkKind {
    pub const ALL: [MarkKind; 4] = [
        MarkKind::Downbeat,
        MarkKind::StrongBeat,
        MarkKind::RegularBeat,
        MarkKind::EnergyDrop,
    ];

    pub fn display_name(&self) -> &'static str {
        match self {
            MarkKind::Downbeat => "Downbeat (Beat 1)",
            MarkKind::StrongBeat => "Strong Beat",
            MarkKind::RegularBeat => "Regular Beat",
            MarkKind::EnergyDrop => "Energy Drop / Hit",
        }
    }

    pub fn short_name(&self) -> &'static str {
        match self {
            MarkKind::Downbeat => "Downbeat",
            MarkKind::StrongBeat => "Strong",
            MarkKind::RegularBeat => "Beat",
            MarkKind::EnergyDrop => "Drop",
        }
    }

    /// Higher priority wins if two marks quantize onto the exact same video frame.
    pub fn priority(&self) -> u8 {
        match self {
            MarkKind::EnergyDrop => 4,
            MarkKind::Downbeat => 3,
            MarkKind::StrongBeat => 2,
            MarkKind::RegularBeat => 1,
        }
    }

    pub fn default_color(&self) -> ResolveColor {
        match self {
            MarkKind::Downbeat => ResolveColor::Red,
            MarkKind::StrongBeat => ResolveColor::Yellow,
            MarkKind::RegularBeat => ResolveColor::Blue,
            MarkKind::EnergyDrop => ResolveColor::Green,
        }
    }

    pub fn default_template(&self) -> &'static str {
        match self {
            MarkKind::Downbeat => "Bar {bar}",
            MarkKind::StrongBeat => "Bar {bar}.{beat} (Strong)",
            MarkKind::RegularBeat => "Bar {bar}.{beat}",
            MarkKind::EnergyDrop => "Drop (Bar {bar}.{beat})",
        }
    }
}

/// Supported musical time signatures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TimeSignature {
    FourFour,
    ThreeFour,
    SixEight,
    TwoFour,
    FiveFour,
    SevenEight,
}

impl TimeSignature {
    pub const ALL: [TimeSignature; 6] = [
        TimeSignature::FourFour,
        TimeSignature::ThreeFour,
        TimeSignature::SixEight,
        TimeSignature::TwoFour,
        TimeSignature::FiveFour,
        TimeSignature::SevenEight,
    ];

    pub fn beats_per_bar(&self) -> u8 {
        match self {
            TimeSignature::FourFour => 4,
            TimeSignature::ThreeFour => 3,
            TimeSignature::SixEight => 6,
            TimeSignature::TwoFour => 2,
            TimeSignature::FiveFour => 5,
            TimeSignature::SevenEight => 7,
        }
    }

    /// Secondary accented beats (1-based within bar, excluding Beat 1 which is always Downbeat).
    pub fn is_strong_beat(&self, beat_in_bar: u8) -> bool {
        match self {
            TimeSignature::FourFour => beat_in_bar == 3,
            TimeSignature::ThreeFour => false,
            TimeSignature::SixEight => beat_in_bar == 4,
            TimeSignature::TwoFour => false,
            TimeSignature::FiveFour => beat_in_bar == 4,
            TimeSignature::SevenEight => beat_in_bar == 4 || beat_in_bar == 6,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            TimeSignature::FourFour => "4/4",
            TimeSignature::ThreeFour => "3/4",
            TimeSignature::SixEight => "6/8",
            TimeSignature::TwoFour => "2/4",
            TimeSignature::FiveFour => "5/4",
            TimeSignature::SevenEight => "7/8",
        }
    }
}

impl fmt::Display for TimeSignature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GridMode {
    Dynamic,
    LockedConstantBpm,
}

/// A single beat or structural marker on the track timeline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BeatMark {
    pub id: u64,
    pub time_sec: f64,
    pub kind: MarkKind,
    pub bar: u32,
    pub beat_in_bar: u8,
    pub energy: f32,
    pub confidence: f32,
    pub custom_label: Option<String>,
}

/// Precomputed multi-band waveform bin for fast 60 FPS DJ-RGB & Resolve-mono rendering.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct WaveformBin {
    pub peak: f32,
    pub rms: f32,
    pub low: f32,
    pub mid: f32,
    pub high: f32,
}

/// Per-category visibility, Resolve color, and label settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkStyleConfig {
    pub enabled: bool,
    pub color: ResolveColor,
    pub label_template: String,
}

/// Full styling and filtering config shared by the UI visualizer and exporters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkExportStyle {
    pub downbeat: MarkStyleConfig,
    pub strong_beat: MarkStyleConfig,
    pub regular_beat: MarkStyleConfig,
    pub energy_drop: MarkStyleConfig,
    /// Export every Nth bar for downbeats (1 = every bar, 2 = every 2 bars, 4, 8).
    pub downbeat_bar_stride: u32,
}

impl Default for MarkExportStyle {
    fn default() -> Self {
        Self {
            downbeat: MarkStyleConfig {
                enabled: true,
                color: MarkKind::Downbeat.default_color(),
                label_template: MarkKind::Downbeat.default_template().to_string(),
            },
            strong_beat: MarkStyleConfig {
                enabled: true,
                color: MarkKind::StrongBeat.default_color(),
                label_template: MarkKind::StrongBeat.default_template().to_string(),
            },
            regular_beat: MarkStyleConfig {
                enabled: true,
                color: MarkKind::RegularBeat.default_color(),
                label_template: MarkKind::RegularBeat.default_template().to_string(),
            },
            energy_drop: MarkStyleConfig {
                enabled: true,
                color: MarkKind::EnergyDrop.default_color(),
                label_template: MarkKind::EnergyDrop.default_template().to_string(),
            },
            downbeat_bar_stride: 1,
        }
    }
}

impl MarkExportStyle {
    pub fn style_for(&self, kind: MarkKind) -> &MarkStyleConfig {
        match kind {
            MarkKind::Downbeat => &self.downbeat,
            MarkKind::StrongBeat => &self.strong_beat,
            MarkKind::RegularBeat => &self.regular_beat,
            MarkKind::EnergyDrop => &self.energy_drop,
        }
    }

    pub fn style_for_mut(&mut self, kind: MarkKind) -> &mut MarkStyleConfig {
        match kind {
            MarkKind::Downbeat => &mut self.downbeat,
            MarkKind::StrongBeat => &mut self.strong_beat,
            MarkKind::RegularBeat => &mut self.regular_beat,
            MarkKind::EnergyDrop => &mut self.energy_drop,
        }
    }

    pub fn is_mark_included(&self, mark: &BeatMark) -> bool {
        let cfg = self.style_for(mark.kind);
        if !cfg.enabled {
            return false;
        }
        if mark.kind == MarkKind::Downbeat && self.downbeat_bar_stride > 1 {
            let zero_based = mark.bar.saturating_sub(1);
            return zero_based % self.downbeat_bar_stride == 0;
        }
        true
    }

    pub fn render_label(&self, mark: &BeatMark, bpm: f64) -> String {
        if let Some(custom) = &mark.custom_label {
            if !custom.trim().is_empty() {
                return sanitize_marker_text(custom);
            }
        }
        let tmpl = &self.style_for(mark.kind).label_template;
        let rendered = tmpl
            .replace("{bar}", &mark.bar.to_string())
            .replace("{beat}", &mark.beat_in_bar.to_string())
            .replace("{type}", mark.kind.short_name())
            .replace("{bpm}", &format!("{:.1}", bpm));
        sanitize_marker_text(&rendered)
    }
}

/// Remove characters that break CMX 3600 pipe-delimited Resolve marker comments.
pub fn sanitize_marker_text(input: &str) -> String {
    input
        .chars()
        .map(|c| match c {
            '|' | '\r' | '\n' | '\t' => ' ',
            c if c.is_ascii() && !c.is_control() => c,
            _ => ' ',
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Complete analyzed track state (serializable snapshot for editing & undo).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackAnalysis {
    pub title: String,
    pub file_path: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub duration_sec: f64,
    pub detected_bpm: f64,
    pub active_bpm: f64,
    pub bpm_multiplier: f64,
    pub tempo_stability: f32,
    pub detected_time_sig: TimeSignature,
    pub active_time_sig: TimeSignature,
    pub grid_mode: GridMode,
    pub detection_engine: String,
    pub downbeat_phase_shift: i32,
    pub extrapolate_silence: bool,
    pub in_point_sec: f64,
    pub out_point_sec: f64,
    pub marks: Vec<BeatMark>,
    /// Raw dynamic beat timestamps (seconds) preserved so toggling Locked/Dynamic is non-destructive.
    pub raw_beat_times: Vec<f64>,
    /// Raw detected downbeat timestamps (seconds) from neural/DSP tracker.
    pub raw_downbeat_times: Vec<f64>,
    /// Raw detected energy drop timestamps (seconds) + strength score (0..1).
    pub raw_drops: Vec<(f64, f32)>,
    pub drop_sensitivity: f32,
    pub waveform_bins_per_sec: f32,
    pub waveform: Vec<WaveformBin>,
    pub next_mark_id: u64,
}
