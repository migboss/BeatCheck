use crate::edl::{resolve_markers, ExportSettings, ResolvedMarker};
use crate::timecode::TimecodeError;
use beatcheck_core::model::TrackAnalysis;
use serde::Serialize;

pub fn export_csv(
    track: &TrackAnalysis,
    settings: &ExportSettings,
) -> Result<String, TimecodeError> {
    let markers = resolve_markers(track, settings)?;
    let mut out = String::with_capacity(markers.len() * 80 + 128);
    out.push_str("event,timecode,frame,time_sec,bar,beat,type,resolve_color,energy,label\n");
    for m in &markers {
        let escaped_label = m.label.replace('"', "\"\"");
        out.push_str(&format!(
            "{},{},{},{:.6},{},{},{},{},{:.3},\"{}\"\n",
            m.event_index,
            m.timecode,
            m.frame_index,
            m.time_sec,
            m.bar,
            m.beat_in_bar,
            m.kind.short_name(),
            m.color.edl_name(),
            m.energy,
            escaped_label
        ));
    }
    Ok(out)
}

#[derive(Serialize)]
struct JsonExportDocument<'a> {
    title: &'a str,
    file_path: &'a str,
    bpm: f64,
    time_signature: &'static str,
    frame_rate: &'static str,
    start_timecode: &'a str,
    duration_sec: f64,
    detection_engine: &'a str,
    markers: Vec<ResolvedMarker>,
}

pub fn export_json(
    track: &TrackAnalysis,
    settings: &ExportSettings,
) -> Result<String, TimecodeError> {
    let markers = resolve_markers(track, settings)?;
    let doc = JsonExportDocument {
        title: &track.title,
        file_path: &track.file_path,
        bpm: track.active_bpm,
        time_signature: track.active_time_sig.label(),
        frame_rate: settings.frame_rate.label(),
        start_timecode: &settings.start_timecode,
        duration_sec: track.duration_sec,
        detection_engine: &track.detection_engine,
        markers,
    };
    Ok(serde_json::to_string_pretty(&doc).unwrap_or_else(|_| "{}".to_string()))
}
