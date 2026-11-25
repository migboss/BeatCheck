use crate::edl::{resolve_markers, ExportSettings};
use crate::timecode::TimecodeError;
use beatcheck_core::model::TrackAnalysis;

/// Export FCPXML v1.9 timeline containing the audio asset on Track A1 AND all beat markers.
/// Import into DaVinci Resolve via: File -> Import -> Timeline...
pub fn export_fcpxml(
    track: &TrackAnalysis,
    settings: &ExportSettings,
) -> Result<String, TimecodeError> {
    let markers = resolve_markers(track, settings)?;
    let fps = settings.frame_rate;
    let start_frame = fps.parse_smpte(&settings.start_timecode)?;
    let total_frames = fps.seconds_to_frames(track.duration_sec).max(1) as u64;

    // Trim clip start & duration when rebasing In-Out range so audio stays 1:1 synced with markers!
    let rebased = settings.respect_in_out_range && settings.rebase_in_point_to_start;
    let clip_media_start_frames = if rebased {
        fps.seconds_to_frames(track.in_point_sec.max(0.0)).max(0) as u64
    } else {
        0
    };
    let clip_dur_frames = if rebased {
        let out_f = fps.seconds_to_frames(track.out_point_sec.max(track.in_point_sec)).max(1) as u64;
        out_f.saturating_sub(clip_media_start_frames).max(1)
    } else {
        total_frames
    };

    let frame_dur = fps.fcpxml_frame_duration();
    let tc_start_str = fps.frames_to_fcpxml_time(start_frame);
    let asset_dur_str = fps.frames_to_fcpxml_time(total_frames);
    let clip_start_str = if clip_media_start_frames == 0 {
        "0s".to_string()
    } else {
        fps.frames_to_fcpxml_time(clip_media_start_frames)
    };
    let clip_dur_str = fps.frames_to_fcpxml_time(clip_dur_frames);
    let tc_format = if fps.is_drop_frame() { "DF" } else { "NDF" };

    let title = xml_escape(&track.title);
    let file_uri = xml_escape(&path_to_file_uri(&track.file_path));
    let clip_name = xml_escape(
        std::path::Path::new(&track.file_path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(&track.title),
    );

    let mut xml = String::with_capacity(markers.len() * 140 + 1024);
    xml.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    xml.push_str("<!DOCTYPE fcpxml>\n");
    xml.push_str("<fcpxml version=\"1.9\">\n");
    xml.push_str("  <resources>\n");
    xml.push_str(&format!(
        "    <format id=\"r1\" name=\"FFVideoFormat1080p{}\" frameDuration=\"{frame_dur}\" width=\"1920\" height=\"1080\"/>\n",
        fps.nominal_fps()
    ));
    xml.push_str(&format!(
        "    <asset id=\"r2\" name=\"{clip_name}\" src=\"{file_uri}\" start=\"0s\" duration=\"{asset_dur_str}\" hasAudio=\"1\" audioSources=\"1\" audioChannels=\"{}\" audioRate=\"{}\"/>\n",
        track.channels.max(1),
        track.sample_rate.max(44100)
    ));
    xml.push_str("  </resources>\n");
    xml.push_str("  <library>\n");
    xml.push_str(&format!("    <event name=\"{title}\">\n"));
    xml.push_str(&format!("      <project name=\"{title} ({:.1} BPM)\">\n", track.active_bpm));
    xml.push_str(&format!(
        "        <sequence format=\"r1\" duration=\"{clip_dur_str}\" tcStart=\"{tc_start_str}\" tcFormat=\"{tc_format}\" audioLayout=\"stereo\" audioRate=\"48k\">\n"
    ));
    xml.push_str("          <spine>\n");
    xml.push_str(&format!(
        "            <asset-clip name=\"{clip_name}\" ref=\"r2\" offset=\"{tc_start_str}\" start=\"{clip_start_str}\" duration=\"{clip_dur_str}\" tcFormat=\"{tc_format}\">\n"
    ));

    for m in &markers {
        // Marker start in asset-clip local media coordinates (`clip_media_start_frames + rel_frame`)
        let rel_frame = m.frame_index.saturating_sub(start_frame);
        let m_start = fps.frames_to_fcpxml_time(clip_media_start_frames + rel_frame);
        let label = xml_escape(&m.label);
        let note = xml_escape(&format!(
            "{} | {} | {} | Bar {}.{}",
            m.kind.short_name(),
            m.color.edl_name(),
            m.timecode,
            m.bar,
            m.beat_in_bar
        ));
        xml.push_str(&format!(
            "              <marker start=\"{m_start}\" duration=\"{frame_dur}\" value=\"{label}\" note=\"{note}\"/>\n"
        ));
    }

    xml.push_str("            </asset-clip>\n");
    xml.push_str("          </spine>\n");
    xml.push_str("        </sequence>\n");
    xml.push_str("      </project>\n");
    xml.push_str("    </event>\n");
    xml.push_str("  </library>\n");
    xml.push_str("</fcpxml>\n");

    Ok(xml)
}

fn xml_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn path_to_file_uri(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    let mut encoded = String::with_capacity(normalized.len() + 16);
    for (idx, b) in normalized.bytes().enumerate() {
        let is_drive_colon = b == b':' && idx == 1 && normalized.as_bytes()[0].is_ascii_alphabetic();
        let unreserved = b.is_ascii_alphanumeric()
            || matches!(b, b'-' | b'.' | b'_' | b'~' | b'/')
            || is_drive_colon;
        if unreserved {
            encoded.push(b as char);
        } else {
            encoded.push_str(&format!("%{b:02X}"));
        }
    }
    if encoded.starts_with('/') {
        format!("file://localhost{encoded}")
    } else {
        format!("file://localhost/{encoded}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use beatcheck_core::model::{BeatMark, GridMode, MarkKind, TimeSignature};

    #[test]
    fn fcpxml_contains_asset_and_markers() {
        let track = TrackAnalysis {
            title: "Neon Beat".to_string(),
            file_path: "F:\\Music\\Neon Beat.mp3".to_string(),
            sample_rate: 44100,
            channels: 2,
            duration_sec: 4.0,
            detected_bpm: 120.0,
            active_bpm: 120.0,
            bpm_multiplier: 1.0,
            tempo_stability: 1.0,
            detected_time_sig: TimeSignature::FourFour,
            active_time_sig: TimeSignature::FourFour,
            grid_mode: GridMode::LockedConstantBpm,
            detection_engine: "Test".to_string(),
            downbeat_phase_shift: 0,
            extrapolate_silence: true,
            in_point_sec: 0.0,
            out_point_sec: 4.0,
            marks: vec![BeatMark {
                id: 1,
                time_sec: 1.0,
                kind: MarkKind::Downbeat,
                bar: 1,
                beat_in_bar: 1,
                energy: 0.9,
                confidence: 1.0,
                custom_label: None,
            }],
            raw_beat_times: vec![],
            raw_downbeat_times: vec![],
            raw_drops: vec![],
            drop_sensitivity: 0.5,
            waveform_bins_per_sec: 100.0,
            waveform: vec![],
            next_mark_id: 2,
        };
        let xml = export_fcpxml(&track, &ExportSettings::default()).unwrap();
        assert!(xml.contains("<fcpxml version=\"1.9\">"));
        assert!(xml.contains("file://localhost/F:/Music/Neon%20Beat.mp3"));
        assert!(xml.contains("frameDuration=\"1/25s\""));
        assert!(xml.contains("start=\"25/25s\""));
        assert!(xml.contains("ResolveColorRed"));
    }

    #[test]
    fn fcpxml_escapes_special_path_chars_and_trims_rebased_in_out() {
        let track = TrackAnalysis {
            title: "Rock & Roll".to_string(),
            file_path: "F:\\Music\\O'Connor & \"Best\" #1.mp3".to_string(),
            sample_rate: 44100,
            channels: 2,
            duration_sec: 10.0,
            detected_bpm: 120.0,
            active_bpm: 120.0,
            bpm_multiplier: 1.0,
            tempo_stability: 1.0,
            detected_time_sig: TimeSignature::FourFour,
            active_time_sig: TimeSignature::FourFour,
            grid_mode: GridMode::LockedConstantBpm,
            detection_engine: "Test".to_string(),
            downbeat_phase_shift: 0,
            extrapolate_silence: false,
            in_point_sec: 2.0,
            out_point_sec: 6.0,
            marks: vec![BeatMark {
                id: 1,
                time_sec: 3.0,
                kind: MarkKind::Downbeat,
                bar: 2,
                beat_in_bar: 1,
                energy: 0.9,
                confidence: 1.0,
                custom_label: None,
            }],
            raw_beat_times: vec![],
            raw_downbeat_times: vec![],
            raw_drops: vec![],
            drop_sensitivity: 0.5,
            waveform_bins_per_sec: 100.0,
            waveform: vec![],
            next_mark_id: 2,
        };
        let mut settings = ExportSettings::default();
        settings.respect_in_out_range = true;
        settings.rebase_in_point_to_start = true;
        let xml = export_fcpxml(&track, &settings).unwrap();
        // Percent-encoded quotes/apostrophe/ampersand/hash — no raw unescaped quote breaking src="..."
        assert!(xml.contains("O%27Connor%20%26%20%22Best%22%20%231.mp3"));
        // Clip starts at 2s (50 frames) and lasts 4s (100 frames); marker at 3s is at media start 75/25s
        assert!(xml.contains("start=\"50/25s\""));
        assert!(xml.contains("duration=\"100/25s\""));
        assert!(xml.contains("start=\"75/25s\""));
    }
}
