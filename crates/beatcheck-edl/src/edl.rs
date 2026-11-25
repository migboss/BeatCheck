use crate::timecode::{FrameRate, TimecodeError};
use beatcheck_core::model::{
    sanitize_marker_text, MarkExportStyle, MarkKind, ResolveColor, TrackAnalysis,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Settings controlling how marks are projected onto DaVinci Resolve's timeline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExportSettings {
    pub frame_rate: FrameRate,
    /// Timeline start timecode (default `"01:00:00:00"` for DaVinci Resolve).
    pub start_timecode: String,
    /// Shift all markers earlier (negative) or later (positive) in milliseconds.
    pub anticipation_ms: f64,
    /// Additional shift in whole video frames (e.g. `-1` for snappy pre-beat cuts).
    pub anticipation_frames: i64,
    /// Only export marks within `[in_point_sec, out_point_sec]`.
    pub respect_in_out_range: bool,
    /// When using In/Out range, rebase the In point to `start_timecode` if true,
    /// otherwise keep original song offset relative to `start_timecode`.
    pub rebase_in_point_to_start: bool,
    pub style: MarkExportStyle,
}

impl Default for ExportSettings {
    fn default() -> Self {
        Self {
            frame_rate: FrameRate::Fps25,
            start_timecode: "01:00:00:00".to_string(),
            anticipation_ms: 0.0,
            anticipation_frames: 0,
            respect_in_out_range: true,
            rebase_in_point_to_start: false,
            style: MarkExportStyle::default(),
        }
    }
}

/// A resolved, frame-quantized, deduplicated marker ready for any exporter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolvedMarker {
    pub event_index: usize,
    pub frame_index: u64,
    pub timecode: String,
    pub time_sec: f64,
    pub kind: MarkKind,
    pub color: ResolveColor,
    pub label: String,
    pub bar: u32,
    pub beat_in_bar: u8,
    pub energy: f32,
}

/// Filter, apply anticipation offset, quantize to exact Resolve frames, and resolve collisions.
pub fn resolve_markers(
    track: &TrackAnalysis,
    settings: &ExportSettings,
) -> Result<Vec<ResolvedMarker>, TimecodeError> {
    let start_frame = settings.frame_rate.parse_smpte(&settings.start_timecode)?;
    let origin_sec = if settings.respect_in_out_range && settings.rebase_in_point_to_start {
        track.in_point_sec
    } else {
        0.0
    };

    // Map absolute frame -> winning (priority, ResolvedMarker)
    let mut by_frame: BTreeMap<u64, (u8, ResolvedMarker)> = BTreeMap::new();

    for mark in &track.marks {
        if !settings.style.is_mark_included(mark) {
            continue;
        }
        if settings.respect_in_out_range
            && (mark.time_sec + 1e-4 < track.in_point_sec
                || mark.time_sec - 1e-4 > track.out_point_sec)
        {
            continue;
        }

        let shifted_sec = (mark.time_sec - origin_sec) + (settings.anticipation_ms / 1000.0);
        let rel_frames =
            settings.frame_rate.seconds_to_frames(shifted_sec) + settings.anticipation_frames;
        let abs_frame = (start_frame as i64 + rel_frames).max(0) as u64;

        let color = settings.style.style_for(mark.kind).color;
        let label = settings.style.render_label(mark, track.active_bpm);
        let prio = mark.kind.priority();

        let candidate = ResolvedMarker {
            event_index: 0,
            frame_index: abs_frame,
            timecode: settings.frame_rate.format_frames(abs_frame),
            time_sec: mark.time_sec,
            kind: mark.kind,
            color,
            label,
            bar: mark.bar,
            beat_in_bar: mark.beat_in_bar,
            energy: mark.energy,
        };

        match by_frame.get(&abs_frame) {
            Some((existing_prio, _)) if *existing_prio >= prio => {}
            _ => {
                by_frame.insert(abs_frame, (prio, candidate));
            }
        }
    }

    let mut out = Vec::with_capacity(by_frame.len());
    for (idx, (_, (_, mut marker))) in by_frame.into_iter().enumerate() {
        marker.event_index = idx + 1;
        out.push(marker);
    }
    Ok(out)
}

/// Export DaVinci Resolve CMX 3600 Timeline Markers EDL.
/// Import in Resolve via: Timeline -> Import -> Timeline Markers from EDL...
pub fn export_resolve_marker_edl(
    track: &TrackAnalysis,
    settings: &ExportSettings,
) -> Result<String, TimecodeError> {
    let markers = resolve_markers(track, settings)?;
    let title = sanitize_edl_title(&format!(
        "{}_{:.0}BPM_{}",
        track.title,
        track.active_bpm,
        track.active_time_sig.label().replace('/', "-")
    ));

    let mut out = String::with_capacity(markers.len() * 140 + 128);
    out.push_str(&format!("TITLE: {title}\r\n"));
    out.push_str(&format!("FCM: {}\r\n\r\n", settings.frame_rate.fcm_header()));

    for m in &markers {
        let in_tc = &m.timecode;
        let out_tc = settings.frame_rate.format_frames(m.frame_index + 1);
        let event_num = format_event_number(m.event_index);
        // Resolve CMX 3600 Marker EDL format
        out.push_str(&format!(
            "{event_num}  001      V     C        {in_tc} {out_tc} {in_tc} {out_tc}\r\n"
        ));
        out.push_str(&format!(
            " |C:{} |M:{} |D:1\r\n\r\n",
            m.color.edl_name(),
            sanitize_marker_text(&m.label)
        ));
    }

    Ok(out)
}

/// Export Beat-to-Beat Cut-List EDL (sequential CMX 3600 edits spanning from mark to mark).
/// Useful for placeholder slug tracks or auto-conform workflows in DaVinci Resolve.
pub fn export_cut_list_edl(
    track: &TrackAnalysis,
    settings: &ExportSettings,
) -> Result<String, TimecodeError> {
    let markers = resolve_markers(track, settings)?;
    let start_frame = settings.frame_rate.parse_smpte(&settings.start_timecode)?;
    let end_sec = if settings.respect_in_out_range {
        track.out_point_sec
    } else {
        track.duration_sec
    };
    let origin_sec = if settings.respect_in_out_range && settings.rebase_in_point_to_start {
        track.in_point_sec
    } else {
        0.0
    };
    let end_frame = start_frame
        + settings
            .frame_rate
            .seconds_to_frames((end_sec - origin_sec).max(0.0))
            .max(1) as u64;

    let title = sanitize_edl_title(&format!("{}_CutList", track.title));
    let mut out = String::with_capacity(markers.len() * 150 + 128);
    out.push_str(&format!("TITLE: {title}\r\n"));
    out.push_str(&format!("FCM: {}\r\n\r\n", settings.frame_rate.fcm_header()));

    for (i, m) in markers.iter().enumerate() {
        let next_frame = markers
            .get(i + 1)
            .map(|n| n.frame_index)
            .unwrap_or(end_frame)
            .max(m.frame_index + 1);
        let rec_in = settings.frame_rate.format_frames(m.frame_index);
        let rec_out = settings.frame_rate.format_frames(next_frame);
        let dur_frames = next_frame - m.frame_index;
        let src_in = settings.frame_rate.format_frames(0);
        let src_out = settings.frame_rate.format_frames(dur_frames);
        let event_num = format_event_number(i + 1);

        out.push_str(&format!(
            "{event_num}  BL       V     C        {src_in} {src_out} {rec_in} {rec_out}\r\n"
        ));
        out.push_str(&format!(
            "* FROM CLIP NAME: {}\r\n",
            sanitize_marker_text(&m.label)
        ));
        out.push_str(&format!(
            " |C:{} |M:{} |D:1\r\n\r\n",
            m.color.edl_name(),
            sanitize_marker_text(&m.label)
        ));
    }

    Ok(out)
}

fn format_event_number(index: usize) -> String {
    if index <= 999 {
        format!("{index:03}")
    } else {
        format!("{index:04}")
    }
}

fn sanitize_edl_title(input: &str) -> String {
    let cleaned: String = input
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "BeatCheck".to_string()
    } else {
        cleaned.chars().take(64).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use beatcheck_core::model::{BeatMark, GridMode, TimeSignature};

    fn sample_track() -> TrackAnalysis {
        TrackAnalysis {
            title: "Test Track | Demo".to_string(),
            file_path: "C:/Music/test.mp3".to_string(),
            sample_rate: 44100,
            channels: 2,
            duration_sec: 10.0,
            detected_bpm: 120.0,
            active_bpm: 120.0,
            bpm_multiplier: 1.0,
            tempo_stability: 0.98,
            detected_time_sig: TimeSignature::FourFour,
            active_time_sig: TimeSignature::FourFour,
            grid_mode: GridMode::LockedConstantBpm,
            detection_engine: "Test".to_string(),
            downbeat_phase_shift: 0,
            extrapolate_silence: true,
            in_point_sec: 0.0,
            out_point_sec: 10.0,
            marks: vec![
                BeatMark {
                    id: 1,
                    time_sec: 0.0,
                    kind: MarkKind::Downbeat,
                    bar: 1,
                    beat_in_bar: 1,
                    energy: 0.9,
                    confidence: 0.95,
                    custom_label: None,
                },
                BeatMark {
                    id: 2,
                    time_sec: 0.5,
                    kind: MarkKind::RegularBeat,
                    bar: 1,
                    beat_in_bar: 2,
                    energy: 0.5,
                    confidence: 0.9,
                    custom_label: None,
                },
                BeatMark {
                    id: 3,
                    time_sec: 1.0,
                    kind: MarkKind::StrongBeat,
                    bar: 1,
                    beat_in_bar: 3,
                    energy: 0.8,
                    confidence: 0.92,
                    custom_label: None,
                },
                BeatMark {
                    id: 4,
                    time_sec: 1.0, // Same frame collision with StrongBeat -> EnergyDrop wins
                    kind: MarkKind::EnergyDrop,
                    bar: 1,
                    beat_in_bar: 3,
                    energy: 1.0,
                    confidence: 0.99,
                    custom_label: Some("Big | Drop".to_string()),
                },
            ],
            raw_beat_times: vec![],
            raw_downbeat_times: vec![],
            raw_drops: vec![],
            drop_sensitivity: 0.5,
            waveform_bins_per_sec: 100.0,
            waveform: vec![],
            next_mark_id: 5,
        }
    }

    #[test]
    fn marker_edl_exports_colors_and_resolves_collisions() {
        let track = sample_track();
        let settings = ExportSettings::default(); // 25 fps, 01:00:00:00
        let edl = export_resolve_marker_edl(&track, &settings).unwrap();

        assert!(edl.contains("FCM: NON-DROP FRAME"));
        assert!(edl.contains("01:00:00:00 01:00:00:01"));
        assert!(edl.contains("|C:ResolveColorRed |M:Bar 1 |D:1"));
        // 0.5s at 25 fps = frame 12.5 -> rounds to 13 -> 01:00:00:13
        assert!(edl.contains("01:00:00:13 01:00:00:14"));
        assert!(edl.contains("|C:ResolveColorBlue |M:Bar 1.2 |D:1"));
        // At 1.0s (01:00:01:00), EnergyDrop outranks StrongBeat and pipe is sanitized
        assert!(edl.contains("01:00:01:00 01:00:01:01"));
        assert!(edl.contains("|C:ResolveColorGreen |M:Big Drop |D:1"));
        assert!(!edl.contains("Big | Drop"));
    }

    #[test]
    fn cut_list_edl_produces_contiguous_segments() {
        let track = sample_track();
        let settings = ExportSettings::default();
        let edl = export_cut_list_edl(&track, &settings).unwrap();
        assert!(edl.contains("001  BL       V     C"));
        assert!(edl.contains("01:00:00:00 01:00:00:13"));
        assert!(edl.contains("01:00:00:13 01:00:01:00"));
    }
}
