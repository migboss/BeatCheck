use crate::brand::{self, StrokeIcon};
use crate::config::{PortableConfig, ScrollMode, WaveformVisualMode};
use crate::export_panel::{draw_slide_over_drawer, DrawerTab, ExportPanelActions};
use crate::hud::{draw_top_hud, draw_transport_bar, draw_waveform_toolbar, HudActions};
use crate::launch::{draw_analyzing_screen, draw_split_welcome_screen, LaunchActions};
use crate::shortcuts::{
    draw_shortcuts_page, KeyBindings, ShortcutAction, ShortcutsPageActions, ShortcutsPageState,
};
use crate::waveform::{draw_main_waveform, draw_mini_overview, WaveformActions, WaveformState};
use beatcheck_audio::AudioPlayer;
use beatcheck_core::editing::{self, EditHistory};
use beatcheck_core::model::{BeatMark, MarkKind, TrackAnalysis};
use beatcheck_core::{analyze_audio, decode_audio_file};
use beatcheck_edl::{
    export_csv, export_cut_list_edl, export_fcpxml, export_json, export_resolve_marker_edl,
    resolve_markers, ExportSettings,
};
use egui::{Margin, RichText, Stroke};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::time::{Duration, Instant};

enum WorkerMessage {
    Progress(f32, String),
    Done {
        path: PathBuf,
        analysis: TrackAnalysis,
        stereo_pcm: Arc<Vec<f32>>,
    },
    Error(String),
}

pub struct BeatCheckApp {
    player: AudioPlayer,
    audio_devices: Vec<String>,
    track: Option<TrackAnalysis>,
    /// Pristine algorithm output saved on load so the user can Reset all edits cleanly (and Ctrl+Z it!).
    original_track: Option<TrackAnalysis>,
    history: EditHistory,
    drop_drag_undo_pushed: bool,
    export: ExportSettings,
    wf_state: WaveformState,
    visual_mode: WaveformVisualMode,
    scroll_mode: ScrollMode,
    main_view_width_px: f32,
    recent_files: Vec<String>,

    // Right Resolve Inspector (starts ALWAYS ON by default!)
    drawer_open: bool,
    drawer_tab: DrawerTab,

    // Background decoding + neural/DSP analysis
    worker_rx: Option<Receiver<WorkerMessage>>,
    loading_filename: String,
    loading_progress: f32,
    loading_stage: String,

    // Tap tempo state
    tap_times: Vec<Instant>,

    // Live EDL Preview window & Editorial Precision status message
    preview_open: bool,
    preview_text: String,
    status_message: Option<(String, bool)>,

    // Customizable Keyboard Shortcuts & Shortcuts Page
    shortcuts: KeyBindings,
    shortcuts_page_open: bool,
    shortcuts_ui: ShortcutsPageState,

    // Automatic debounced preference persistence (beatcheck.toml)
    last_saved_config: PortableConfig,
    pending_save_since: Option<Instant>,
}

impl BeatCheckApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        mut player: AudioPlayer,
        initial_file: Option<PathBuf>,
    ) -> Self {
        brand::configure_brand_theme(&cc.egui_ctx);

        let cfg = PortableConfig::load_portable();
        if let Some(dev_name) = cfg.audio_output_device.clone() {
            let _ = player.select_output_device(Some(dev_name));
        }
        player.set_metronome_enabled(cfg.metronome_enabled);
        player.set_music_volume(cfg.music_volume);
        player.set_click_volume(cfg.click_volume);
        player.set_muted(cfg.muted);
        player.set_loop_enabled(cfg.loop_enabled);
        player.set_speed(cfg.playback_speed);

        let audio_devices = AudioPlayer::list_output_devices();
        let init_status = player
            .stream_error()
            .map(|e| (format!("Audio warning: {e}"), true));

        let mut recent_files = cfg.recent_files;
        recent_files.retain(|p| std::path::Path::new(p).exists());

        let mut app = Self {
            player,
            audio_devices,
            track: None,
            original_track: None,
            history: EditHistory::new(64),
            drop_drag_undo_pushed: false,
            export: cfg.export,
            wf_state: WaveformState::default(),
            visual_mode: cfg.waveform_mode,
            scroll_mode: cfg.scroll_mode,
            main_view_width_px: 980.0,
            recent_files,
            drawer_open: cfg.drawer_open,
            drawer_tab: DrawerTab::Export,
            worker_rx: None,
            loading_filename: String::new(),
            loading_progress: 0.0,
            loading_stage: String::new(),
            tap_times: Vec::new(),
            preview_open: false,
            preview_text: String::new(),
            status_message: init_status,
            shortcuts: cfg.shortcuts,
            shortcuts_page_open: false,
            shortcuts_ui: ShortcutsPageState::default(),
            last_saved_config: PortableConfig::default(),
            pending_save_since: None,
        };
        app.last_saved_config = app.current_config();

        if let Some(path) = initial_file {
            app.start_loading_file(path);
        }
        app
    }

    fn current_config(&self) -> PortableConfig {
        PortableConfig {
            export: self.export.clone(),
            waveform_mode: self.visual_mode,
            scroll_mode: self.scroll_mode,
            metronome_enabled: self.player.metronome_enabled(),
            music_volume: self.player.music_volume(),
            click_volume: self.player.click_volume(),
            muted: self.player.is_muted(),
            loop_enabled: self.player.loop_enabled(),
            playback_speed: self.player.speed(),
            drawer_open: self.drawer_open,
            audio_output_device: self.player.selected_device_name().map(|s| s.to_string()),
            recent_files: self.recent_files.clone(),
            shortcuts: self.shortcuts.clone(),
        }
    }

    /// Debounced auto-save: writes `beatcheck.toml` ~400ms after any preference changes
    /// so dragging sliders doesn't thrash disk, while clicks/toggles persist automatically.
    fn poll_auto_save_preferences(&mut self, ctx: &egui::Context) {
        let current = self.current_config();
        if current == self.last_saved_config {
            self.pending_save_since = None;
            return;
        }
        let now = Instant::now();
        match self.pending_save_since {
            None => {
                self.pending_save_since = Some(now);
                ctx.request_repaint_after(Duration::from_millis(450));
            }
            Some(since) if now.duration_since(since) >= Duration::from_millis(400) => {
                if current.save_portable().is_ok() {
                    self.shortcuts_ui.dirty = false;
                }
                // Update snapshot even on read-only media so we don't retry every frame
                self.last_saved_config = current;
                self.pending_save_since = None;
            }
            Some(_) => {
                ctx.request_repaint_after(Duration::from_millis(200));
            }
        }
    }

    fn visible_marks_for_audio(&self, track: &TrackAnalysis) -> Vec<BeatMark> {
        track
            .marks
            .iter()
            .filter(|m| self.export.style.is_mark_included(m))
            .map(|m| {
                let mut q = m.clone();
                q.time_sec = self
                    .export
                    .quantized_mark_time_sec(m.time_sec, track.in_point_sec)
                    .clamp(0.0, track.duration_sec);
                q
            })
            .collect()
    }

    fn sync_audio_clicks(&self) {
        if let Some(track) = &self.track {
            let active = self.visible_marks_for_audio(track);
            self.player.update_marks(&active);
        }
    }

    fn record_recent_file(&mut self, path: &PathBuf) {
        let s = path.display().to_string();
        self.recent_files.retain(|x| x != &s && std::path::Path::new(x).exists());
        self.recent_files.insert(0, s);
        self.recent_files.truncate(5);
    }

    fn validate_selected_mark(&mut self) {
        if self.wf_state.selected_mark_ids.is_empty() {
            return;
        }
        if let Some(track) = &self.track {
            let style = &self.export.style;
            self.wf_state.selected_mark_ids.retain(|&id| {
                track
                    .marks
                    .iter()
                    .find(|m| m.id == id)
                    .is_some_and(|m| style.is_mark_included(m))
            });
        } else {
            self.wf_state.clear_selection();
        }
    }

    fn select_all_visible_marks(&mut self) {
        let Some(track) = &self.track else { return };
        let style = &self.export.style;
        self.wf_state.selected_mark_ids = track
            .marks
            .iter()
            .filter(|m| style.is_mark_included(m))
            .map(|m| m.id)
            .collect();
        let count = self.wf_state.selected_count();
        self.status_message = Some((
            format!("Selected {count} visible marker{}", if count == 1 { "" } else { "s" }),
            false,
        ));
    }

    fn ensure_time_visible(&mut self, t: f64) {
        let Some(track) = &self.track else { return };
        let visible_sec = (self.main_view_width_px / self.wf_state.pixels_per_sec.max(1.0)) as f64;
        let max_start = (track.duration_sec - visible_sec).max(0.0);
        let margin = visible_sec * 0.12;
        if t < self.wf_state.view_start_sec + margin
            || t > self.wf_state.view_start_sec + visible_sec - margin
        {
            self.wf_state.view_start_sec = (t - visible_sec * 0.35).clamp(0.0, max_start);
        }
    }

    fn start_loading_file(&mut self, path: PathBuf) {
        self.shortcuts_page_open = false;
        self.shortcuts_ui.capturing = None;
        self.player.set_playing(false);
        self.loading_filename = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("Audio Track")
            .to_string();
        self.loading_progress = 0.03;
        self.loading_stage = "Decoding audio stream...".to_string();
        if self.player.stream_error().is_none() {
            self.status_message = None;
        }

        let (tx, rx) = mpsc::channel();
        self.worker_rx = Some(rx);

        std::thread::spawn(move || {
            let _ = tx.send(WorkerMessage::Progress(0.04, "Decoding audio file...".into()));
            match decode_audio_file(&path) {
                Ok(mut decoded) => {
                    let stereo_pcm = Arc::new(std::mem::take(&mut decoded.stereo_pcm));
                    let analysis = analyze_audio(&decoded, |p, msg| {
                        let _ = tx.send(WorkerMessage::Progress(p, msg.to_string()));
                    });
                    let _ = tx.send(WorkerMessage::Done {
                        path,
                        analysis,
                        stereo_pcm,
                    });
                }
                Err(err) => {
                    let _ = tx.send(WorkerMessage::Error(err.to_string()));
                }
            }
        });
    }

    fn poll_worker(&mut self) {
        let mut finished = false;
        let mut completed_path: Option<PathBuf> = None;
        if let Some(rx) = &self.worker_rx {
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    WorkerMessage::Progress(p, stage) => {
                        self.loading_progress = p;
                        self.loading_stage = stage;
                    }
                    WorkerMessage::Done {
                        path,
                        analysis,
                        stereo_pcm,
                    } => {
                        completed_path = Some(path);
                        let active_clicks = self.visible_marks_for_audio(&analysis);
                        self.player.load_track(
                            stereo_pcm,
                            analysis.sample_rate,
                            analysis.duration_sec,
                            &active_clicks,
                        );
                        self.wf_state = WaveformState::default();
                        self.history.clear();
                        self.drop_drag_undo_pushed = false;
                        if let Some(err) = self.player.stream_error() {
                            self.status_message = Some((format!("Audio error: {err}"), true));
                        } else {
                            self.status_message = Some((
                                format!(
                                    "{:.1} BPM ({}) · {} marks ready · {}",
                                    analysis.active_bpm,
                                    analysis.active_time_sig,
                                    analysis.marks.len(),
                                    self.export.frame_rate.label()
                                ),
                                false,
                            ));
                        }
                        self.original_track = Some(analysis.clone());
                        self.track = Some(analysis);
                        finished = true;
                    }
                    WorkerMessage::Error(err) => {
                        self.status_message = Some((format!("Import failed: {err}"), true));
                        finished = true;
                    }
                }
            }
        }
        if let Some(path) = completed_path {
            self.record_recent_file(&path);
        }
        if finished {
            self.worker_rx = None;
        }
    }

    fn open_file_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Import Audio Track")
            .add_filter("Audio Files", &["mp3", "wav", "flac", "ogg", "m4a", "aac"])
            .pick_file()
        {
            self.start_loading_file(path);
        }
    }

    fn step_playhead_by_frames(&mut self, frames: i32) {
        let Some(track) = &self.track else { return };
        let target_sec = crate::config::step_time_by_frames(
            self.player.position_sec(),
            frames,
            self.export.frame_rate,
            track.duration_sec,
        );
        self.player.seek(target_sec);
        self.ensure_time_visible(target_sec);
    }

    fn nudge_selected_mark_by_frames(&mut self, frames: i32) {
        if self.wf_state.selected_mark_ids.is_empty() {
            return;
        }
        let undo_lbl = self.shortcuts.short_label(ShortcutAction::Undo);
        if let Some(track) = self.track.as_mut() {
            let selected: Vec<(u64, f64)> = track
                .marks
                .iter()
                .filter(|m| self.wf_state.selected_mark_ids.contains(&m.id))
                .map(|m| (m.id, m.time_sec))
                .collect();
            if selected.is_empty() {
                self.wf_state.clear_selection();
                return;
            }
            if selected.len() == 1 {
                let (sel_id, cur_t) = selected[0];
                let snapped_t = crate::config::step_time_by_frames(
                    cur_t,
                    frames,
                    self.export.frame_rate,
                    track.duration_sec,
                );
                if (snapped_t - cur_t).abs() < 1e-9 {
                    return;
                }
                self.history.push_snapshot(track);
                editing::move_mark(track, sel_id, snapped_t);
                let smpte_t = self
                    .export
                    .mark_smpte_offset_sec(snapped_t, track.in_point_sec);
                let tc = crate::config::format_live_smpte(
                    self.export.frame_rate,
                    &self.export.start_timecode,
                    smpte_t,
                );
                self.status_message = Some((
                    format!("Moved selected mark {:+}f -> {tc}  ({undo_lbl} to undo)", frames),
                    false,
                ));
            } else {
                let frame_sec = 1.0 / self.export.frame_rate.fps_f64();
                let raw_delta = frames as f64 * frame_sec;
                let min_t = selected
                    .iter()
                    .map(|&(_, t)| t)
                    .fold(f64::INFINITY, f64::min);
                let max_t = selected
                    .iter()
                    .map(|&(_, t)| t)
                    .fold(f64::NEG_INFINITY, f64::max);
                let clamped_delta =
                    raw_delta.clamp(-min_t, (track.duration_sec - max_t).max(0.0));
                if clamped_delta.abs() < 1e-9 {
                    return;
                }
                self.history.push_snapshot(track);
                let updates: Vec<(u64, f64)> = selected
                    .iter()
                    .map(|&(id, t)| (id, t + clamped_delta))
                    .collect();
                editing::move_marks(track, &updates);
                self.status_message = Some((
                    format!(
                        "Moved {} selected marks {:+}f  ({undo_lbl} to undo)",
                        updates.len(),
                        frames
                    ),
                    false,
                ));
            }
        }
        self.sync_audio_clicks();
    }

    fn add_mark_at_playhead(&mut self, kind: MarkKind) {
        let pos = self.player.position_sec();
        let fps = self.export.frame_rate;
        let mut added_tc = None;
        if let Some(track) = self.track.as_mut() {
            self.history.push_snapshot(track);
            let snapped_t = crate::config::step_time_by_frames(pos, 0, fps, track.duration_sec);
            let id = editing::add_mark(track, snapped_t, kind);
            self.wf_state.select_single(id);
            // Make sure this beat type is visible so the user immediately sees the mark they added!
            self.export.style.style_for_mut(kind).enabled = true;
            let smpte_t = self
                .export
                .mark_smpte_offset_sec(snapped_t, track.in_point_sec);
            added_tc = Some(crate::config::format_live_smpte(
                fps,
                &self.export.start_timecode,
                smpte_t,
            ));
        }
        if let Some(tc) = added_tc {
            self.sync_audio_clicks();
            self.status_message = Some((
                format!(
                    "Added {} at {tc}  ({} to undo)",
                    kind.short_name(),
                    self.shortcuts.short_label(ShortcutAction::Undo)
                ),
                false,
            ));
        }
    }

    fn delete_selected_mark(&mut self) {
        if self.wf_state.selected_mark_ids.is_empty() {
            return;
        }
        let mut removed = 0usize;
        if let Some(track) = self.track.as_mut() {
            let has_any = track
                .marks
                .iter()
                .any(|m| self.wf_state.selected_mark_ids.contains(&m.id));
            if has_any {
                self.history.push_snapshot(track);
                removed = editing::delete_marks(track, &self.wf_state.selected_mark_ids);
            }
            self.wf_state.clear_selection();
        }
        if removed > 0 {
            self.sync_audio_clicks();
            let undo_lbl = self.shortcuts.short_label(ShortcutAction::Undo);
            let msg = if removed == 1 {
                format!("Deleted selected mark  ({undo_lbl} to undo)")
            } else {
                format!("Deleted {removed} selected marks  ({undo_lbl} to undo)")
            };
            self.status_message = Some((msg, false));
        }
    }

    fn reclassify_selected_marks(&mut self, kind: MarkKind) {
        if self.wf_state.selected_mark_ids.is_empty() {
            return;
        }
        let mut changed = 0usize;
        if let Some(track) = self.track.as_mut() {
            let has_any = track
                .marks
                .iter()
                .any(|m| self.wf_state.selected_mark_ids.contains(&m.id));
            if has_any {
                self.history.push_snapshot(track);
                changed = editing::reclassify_marks(track, &self.wf_state.selected_mark_ids, kind);
                self.export.style.style_for_mut(kind).enabled = true;
            }
        }
        if changed > 0 {
            self.sync_audio_clicks();
            let undo_lbl = self.shortcuts.short_label(ShortcutAction::Undo);
            self.status_message = Some((
                format!(
                    "Set {changed} selected mark{} to {}  ({undo_lbl} to undo)",
                    if changed == 1 { "" } else { "s" },
                    kind.short_name()
                ),
                false,
            ));
        }
    }

    fn reset_to_original_algorithm(&mut self) {
        if let (Some(orig), Some(track)) = (self.original_track.clone(), self.track.as_mut()) {
            self.history.push_snapshot(track);
            *track = orig;
            self.wf_state.clear_selection();
            self.player
                .set_loop_range(track.in_point_sec, track.out_point_sec);
            self.sync_audio_clicks();
            self.status_message = Some((
                format!(
                    "Reset all edits to original algorithm marks  ({} to undo)",
                    self.shortcuts.short_label(ShortcutAction::Undo)
                ),
                false,
            ));
        }
    }

    fn handle_keyboard_shortcuts(&mut self, ctx: &egui::Context) {
        if self.shortcuts_page_open
            || self.preview_open
            || ctx.wants_keyboard_input()
            || ctx.memory(|m| m.any_popup_open())
        {
            return;
        }

        let kb = &self.shortcuts;
        let (
            play_pause,
            toggle_metronome,
            toggle_loop,
            tap_tempo,
            toggle_inspector,
            deselect_mark,
            rewind_home,
            step_left,
            step_right,
            prev_downbeat,
            next_downbeat,
            nudge_back,
            nudge_fwd,
            delete_mark,
            select_all_marks,
            open_file,
            undo,
            redo,
            add_key,
        ) = ctx.input(|i| {
            let mark_key = if kb.is_pressed(ShortcutAction::AddRegularBeat, i) {
                Some(MarkKind::RegularBeat)
            } else if kb.is_pressed(ShortcutAction::AddDownbeat, i) {
                Some(MarkKind::Downbeat)
            } else if kb.is_pressed(ShortcutAction::AddStrongBeat, i) {
                Some(MarkKind::StrongBeat)
            } else if kb.is_pressed(ShortcutAction::AddEnergyDrop, i) {
                Some(MarkKind::EnergyDrop)
            } else {
                None
            };
            (
                kb.is_pressed(ShortcutAction::PlayPause, i),
                kb.is_pressed(ShortcutAction::ToggleMetronome, i),
                kb.is_pressed(ShortcutAction::ToggleLoop, i),
                kb.is_pressed(ShortcutAction::TapTempo, i),
                kb.is_pressed(ShortcutAction::ToggleInspector, i),
                kb.is_pressed(ShortcutAction::DeselectMark, i),
                kb.is_pressed(ShortcutAction::RewindToStart, i),
                kb.is_pressed(ShortcutAction::StepFrameBack, i),
                kb.is_pressed(ShortcutAction::StepFrameForward, i),
                kb.is_pressed(ShortcutAction::PrevDownbeat, i),
                kb.is_pressed(ShortcutAction::NextDownbeat, i),
                kb.is_pressed(ShortcutAction::NudgeMarkBack, i),
                kb.is_pressed(ShortcutAction::NudgeMarkForward, i),
                kb.is_pressed(ShortcutAction::DeleteSelectedMark, i),
                kb.is_pressed(ShortcutAction::SelectAllMarks, i),
                kb.is_pressed(ShortcutAction::OpenFile, i),
                kb.is_pressed(ShortcutAction::Undo, i),
                kb.is_pressed(ShortcutAction::Redo, i),
                mark_key,
            )
        });

        if open_file {
            self.open_file_dialog();
        }
        if toggle_inspector {
            self.drawer_open = !self.drawer_open;
        }
        if deselect_mark {
            self.wf_state.clear_selection();
        }
        if self.track.is_none() || self.worker_rx.is_some() {
            return;
        }
        if select_all_marks {
            self.select_all_visible_marks();
        }
        if let Some(kind) = add_key {
            self.add_mark_at_playhead(kind);
        }
        if play_pause {
            self.player.toggle_play();
        }
        if toggle_metronome {
            self.player
                .set_metronome_enabled(!self.player.metronome_enabled());
        }
        if toggle_loop {
            self.player.set_loop_enabled(!self.player.loop_enabled());
        }
        if rewind_home {
            self.player.seek(0.0);
            self.ensure_time_visible(0.0);
        }
        if nudge_back {
            self.nudge_selected_mark_by_frames(-1);
        }
        if nudge_fwd {
            self.nudge_selected_mark_by_frames(1);
        }
        if step_left {
            self.step_playhead_by_frames(-1);
        }
        if step_right {
            self.step_playhead_by_frames(1);
        }
        if prev_downbeat {
            self.jump_downbeat(-1);
        }
        if next_downbeat {
            self.jump_downbeat(1);
        }
        if tap_tempo {
            self.handle_tap_tempo();
        }
        if undo {
            self.perform_undo();
        }
        if redo {
            self.perform_redo();
        }
        if delete_mark {
            self.delete_selected_mark();
        }
    }

    fn save_portable_config(&mut self, report_to_shortcuts_notice: bool) {
        let cfg = self.current_config();
        match cfg.save_portable() {
            Ok(path) => {
                self.last_saved_config = cfg;
                self.pending_save_since = None;
                self.shortcuts_ui.dirty = false;
                let msg = format!("Preferences saved to {}", path.display());
                if report_to_shortcuts_notice {
                    self.shortcuts_ui.notice = Some((msg.clone(), false));
                }
                self.status_message = Some((msg, false));
            }
            Err(err) => {
                if report_to_shortcuts_notice {
                    self.shortcuts_ui.notice = Some((err.clone(), true));
                }
                self.status_message = Some((err, true));
            }
        }
    }

    fn jump_downbeat(&mut self, dir: i32) {
        let Some(track) = &self.track else { return };
        let pos = self.player.position_sec();
        let in_pt = track.in_point_sec;
        let dur = track.duration_sec;
        let target = if dir < 0 {
            track
                .marks
                .iter()
                .filter(|m| m.kind == MarkKind::Downbeat)
                .map(|m| self.export.quantized_mark_time_sec(m.time_sec, in_pt).clamp(0.0, dur))
                .filter(|&t| t < pos - 0.15)
                .next_back()
                .unwrap_or(0.0)
        } else {
            track
                .marks
                .iter()
                .filter(|m| m.kind == MarkKind::Downbeat)
                .map(|m| self.export.quantized_mark_time_sec(m.time_sec, in_pt).clamp(0.0, dur))
                .find(|&t| t > pos + 0.05)
                .unwrap_or(dur)
        };
        self.player.seek(target);
        self.ensure_time_visible(target);
    }

    fn handle_tap_tempo(&mut self) {
        let now = Instant::now();
        if let Some(&last) = self.tap_times.last() {
            if now.duration_since(last).as_secs_f64() > 2.0 {
                self.tap_times.clear();
            }
        }
        self.tap_times.push(now);
        if self.tap_times.len() > 8 {
            self.tap_times.remove(0);
        }
        if self.tap_times.len() >= 3 {
            let intervals: Vec<f64> = self
                .tap_times
                .windows(2)
                .map(|w| w[1].duration_since(w[0]).as_secs_f64())
                .collect();
            let avg_dt = intervals.iter().sum::<f64>() / intervals.len() as f64;
            if avg_dt > 0.25 && avg_dt < 1.5 {
                let bpm = 60.0 / avg_dt;
                let anchor = self.player.position_sec();
                let mut applied_bpm = None;
                if let Some(track) = self.track.as_mut() {
                    self.history.push_snapshot(track);
                    editing::apply_tap_tempo(track, bpm, anchor);
                    applied_bpm = Some(track.active_bpm);
                }
                if let Some(abpm) = applied_bpm {
                    self.sync_audio_clicks();
                    self.status_message = Some((format!("Tap tempo locked: {:.1} BPM", abpm), false));
                }
            }
        }
    }

    fn perform_undo(&mut self) {
        let mut undone = false;
        if let Some(track) = self.track.as_mut() {
            if self.history.undo(track) {
                self.player
                    .set_loop_range(track.in_point_sec, track.out_point_sec);
                undone = true;
            }
        }
        if undone {
            self.wf_state.cancel_active_gestures();
            self.validate_selected_mark();
            self.sync_audio_clicks();
            self.status_message = Some(("Undid last change".into(), false));
        }
    }

    fn perform_redo(&mut self) {
        let mut redone = false;
        if let Some(track) = self.track.as_mut() {
            if self.history.redo(track) {
                self.player
                    .set_loop_range(track.in_point_sec, track.out_point_sec);
                redone = true;
            }
        }
        if redone {
            self.wf_state.cancel_active_gestures();
            self.validate_selected_mark();
            self.sync_audio_clicks();
            self.status_message = Some(("Redid change".into(), false));
        }
    }

    fn run_export(&mut self, ext: &str, default_suffix: &str, content: Result<String, String>) {
        let Some(track) = &self.track else { return };
        match content {
            Ok(data) => {
                let default_name = format!("{}_{}.{}", track.title, default_suffix, ext);
                if let Some(path) = rfd::FileDialog::new()
                    .set_file_name(&default_name)
                    .add_filter(ext.to_uppercase(), &[ext])
                    .save_file()
                {
                    match std::fs::write(&path, data) {
                        Ok(()) => {
                            let file_label = path
                                .file_name()
                                .and_then(|s| s.to_str())
                                .unwrap_or("file");
                            self.status_message = Some((
                                format!(
                                    "Exported {} · {} · {}",
                                    file_label,
                                    self.export.frame_rate.label(),
                                    self.export.start_timecode
                                ),
                                false,
                            ));
                        }
                        Err(err) => {
                            self.status_message = Some((format!("Save error: {err}"), true));
                        }
                    }
                }
            }
            Err(err) => {
                self.status_message = Some((format!("Export error: {err}"), true));
            }
        }
    }
}

impl eframe::App for BeatCheckApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_worker();
        self.handle_keyboard_shortcuts(ctx);

        let dropped: Option<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .find_map(|f| f.path.clone())
        });
        if let Some(path) = dropped {
            self.start_loading_file(path);
        }

        let is_playing = self.player.is_playing();
        if is_playing || self.worker_rx.is_some() || self.player.output_peak() > 0.01 {
            ctx.request_repaint();
        }
        let playhead_sec = self.player.position_sec();
        self.validate_selected_mark();

        let resolved_count = self
            .track
            .as_ref()
            .and_then(|t| resolve_markers(t, &self.export).ok())
            .map(|v| v.len())
            .unwrap_or(0);

        let export_before = self.export.clone();

        let mut hud_actions = HudActions::default();
        let mut wf_actions = WaveformActions::default();
        let mut export_actions = ExportPanelActions::default();
        let mut launch_actions = LaunchActions::default();
        let mut shortcuts_actions = ShortcutsPageActions::default();

        // On Windows, dragging a window to maximize (Aero Snap) can overhang the left border by ~6-8px;
        // add a small safe left inset while maximized so no edge button or waveform border clips.
        #[cfg(target_os = "windows")]
        let win_max_left_pad: i8 = if ctx.input(|i| i.viewport().maximized == Some(true)) {
            8
        } else {
            0
        };
        #[cfg(not(target_os = "windows"))]
        let win_max_left_pad: i8 = 0;

        // 1. Clean Top Brand HUD (overlap-free!)
        egui::TopBottomPanel::top("top_hud_panel")
            .frame(
                egui::Frame::new()
                    .fill(brand::PANEL)
                    .stroke(Stroke::new(1.0, brand::BORDER))
                    .inner_margin(Margin {
                        left: 14 + win_max_left_pad,
                        right: 14,
                        top: 9,
                        bottom: 9,
                    }),
            )
            .show(ctx, |ui| {
                draw_top_hud(
                    ui,
                    self.track.as_mut(),
                    &self.export,
                    &self.shortcuts,
                    playhead_sec,
                    is_playing,
                    self.drawer_open,
                    self.shortcuts_page_open,
                    resolved_count,
                    &mut hud_actions,
                );
            });

        // 2. Minimalist Bottom Transport Deck (shown when track is loaded)
        if self.track.is_some() {
            egui::TopBottomPanel::bottom("bottom_transport_panel")
                .frame(
                    egui::Frame::new()
                        .fill(brand::PANEL)
                        .stroke(Stroke::new(1.0, brand::BORDER))
                        .inner_margin(Margin {
                            left: 14 + win_max_left_pad,
                            right: 14,
                            top: 9,
                            bottom: 9,
                        }),
                )
                .show(ctx, |ui| {
                    draw_transport_bar(
                        ui,
                        &self.player,
                        &self.shortcuts,
                        self.track.is_some(),
                        self.status_message.as_ref(),
                        &mut hud_actions,
                    );
                });
        }

        // 3. Always-On Right Resolve Inspector (Export | Tune Grid | Settings)
        let drawer_anim =
            ctx.animate_bool_responsive(egui::Id::new("resolve_slide_over_anim"), self.drawer_open);
        if drawer_anim > 0.01 {
            let target_w = 344.0 * drawer_anim;
            let frame_sec = 1.0 / self.export.frame_rate.fps_f64();
            let can_reset = self.original_track.is_some();
            egui::SidePanel::right("resolve_slide_over_drawer")
                .resizable(false)
                .exact_width(target_w.max(24.0))
                .frame(
                    egui::Frame::new()
                        .fill(brand::PANEL)
                        .stroke(Stroke::new(1.0, brand::BORDER))
                        .inner_margin(Margin::same(14)),
                )
                .show(ctx, |ui| {
                    if drawer_anim > 0.45 {
                        draw_slide_over_drawer(
                            ui,
                            &mut self.drawer_tab,
                            &mut self.visual_mode,
                            self.track.as_mut(),
                            &self.wf_state.selected_mark_ids,
                            resolved_count,
                            can_reset,
                            &self.player,
                            &self.audio_devices,
                            &mut self.export,
                            &self.shortcuts,
                            frame_sec,
                            &mut export_actions,
                            &mut hud_actions,
                        );
                    }
                });
        }

        // 4. Center Workspace (Keyboard Shortcuts Page | Launch Screen | Analyzing Card | Waveform)
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(brand::CANVAS)
                    .inner_margin(Margin {
                        left: 12 + win_max_left_pad,
                        right: 12,
                        top: 12,
                        bottom: 12,
                    }),
            )
            .show(ctx, |ui| {
                if self.shortcuts_page_open {
                    draw_shortcuts_page(
                        ui,
                        &mut self.shortcuts,
                        &mut self.shortcuts_ui,
                        self.track.is_some(),
                        &mut shortcuts_actions,
                    );
                    return;
                }

                if self.worker_rx.is_some() {
                    draw_analyzing_screen(
                        ui,
                        &self.loading_filename,
                        self.loading_progress,
                        &self.loading_stage,
                    );
                    return;
                }

                let can_reset = self.original_track.is_some();
                let Some(track) = self.track.as_mut() else {
                    draw_split_welcome_screen(
                        ui,
                        &self.export,
                        &self.shortcuts,
                        &self.recent_files,
                        &mut launch_actions,
                    );
                    return;
                };

                draw_waveform_toolbar(
                    ui,
                    track,
                    &mut self.export,
                    &self.shortcuts,
                    &self.wf_state.selected_mark_ids,
                    &mut self.visual_mode,
                    &mut self.scroll_mode,
                    self.history.can_undo(),
                    self.history.can_redo(),
                    can_reset,
                    &mut hud_actions,
                );
                ui.add_space(8.0);

                draw_mini_overview(
                    ui,
                    track,
                    &mut self.wf_state,
                    &self.export.style,
                    self.visual_mode,
                    playhead_sec,
                    self.main_view_width_px,
                    &mut wf_actions,
                );
                ui.add_space(8.0);

                self.main_view_width_px = draw_main_waveform(
                    ui,
                    track,
                    &mut self.wf_state,
                    &self.export,
                    &self.shortcuts,
                    self.visual_mode,
                    self.scroll_mode,
                    playhead_sec,
                    is_playing,
                    &mut wf_actions,
                );
            });

        // Sync audio clicks when beat-type filters, frame rate, or cut offset change
        if hud_actions.mark_filter_changed || self.export != export_before {
            self.validate_selected_mark();
            self.sync_audio_clicks();
        }

        // Audio output controls from Settings tab
        if let Some(dev_opt) = export_actions.select_audio_device {
            self.audio_devices = AudioPlayer::list_output_devices();
            match self.player.select_output_device(dev_opt) {
                Ok(()) => {
                    self.status_message = Some((
                        format!("Audio output switched to {}", self.player.active_device_name()),
                        false,
                    ));
                }
                Err(e) => {
                    self.status_message = Some((format!("Audio switch failed: {e}"), true));
                }
            }
        }
        if export_actions.reconnect_audio {
            self.audio_devices = AudioPlayer::list_output_devices();
            match self.player.reconnect() {
                Ok(()) => {
                    self.status_message = Some((
                        format!("Reconnected audio: {}", self.player.active_device_name()),
                        false,
                    ));
                }
                Err(e) => {
                    self.status_message = Some((format!("Audio reconnect failed: {e}"), true));
                }
            }
        }
        if export_actions.test_audio_tone {
            self.player.trigger_test_tone();
        }

        // Apply deferred actions cleanly
        if hud_actions.open_file_dialog || launch_actions.open_file_dialog {
            self.open_file_dialog();
        }
        if let Some(recent) = launch_actions.open_recent_path {
            self.start_loading_file(recent);
        }
        if hud_actions.toggle_drawer {
            self.drawer_open = !self.drawer_open;
        }
        if export_actions.close_drawer {
            self.drawer_open = false;
        }
        if hud_actions.toggle_shortcuts_page {
            self.shortcuts_page_open = !self.shortcuts_page_open;
            self.shortcuts_ui.capturing = None;
        }
        if export_actions.open_shortcuts_page || launch_actions.open_shortcuts_page {
            self.shortcuts_page_open = true;
            self.shortcuts_ui.capturing = None;
        }
        if shortcuts_actions.close_page {
            self.shortcuts_page_open = false;
            self.shortcuts_ui.capturing = None;
        }
        if shortcuts_actions.bindings_changed {
            // Auto-save runs right below via poll_auto_save_preferences (flushed on exit too)
            self.pending_save_since = Some(Instant::now());
        }
        if let Some(seek_t) = wf_actions.seek_to {
            self.player.seek(seek_t);
        }
        if hud_actions.rewind_to_start {
            self.player.seek(0.0);
            self.ensure_time_visible(0.0);
        }
        if let Some(dir) = hud_actions.jump_downbeat {
            self.jump_downbeat(dir);
        }
        if hud_actions.tap_tempo {
            self.handle_tap_tempo();
        }
        if hud_actions.undo {
            self.perform_undo();
        }
        if hud_actions.redo {
            self.perform_redo();
        }
        if hud_actions.reset_to_auto {
            self.reset_to_original_algorithm();
        }
        if let Some(df) = hud_actions.nudge_selected_frames {
            self.nudge_selected_mark_by_frames(df);
        }
        if let Some(kind) = hud_actions.add_mark_at_playhead {
            self.add_mark_at_playhead(kind);
        }
        if let Some(kind) = hud_actions.reclassify_selected_marks {
            self.reclassify_selected_marks(kind);
        }
        if hud_actions.delete_selected_mark {
            self.delete_selected_mark();
        }
        if hud_actions.clear_selection {
            self.wf_state.clear_selection();
        }

        let mut need_audio_sync = false;
        if let Some(snap) = wf_actions.undo_snapshot {
            self.history.push_snapshot(&snap);
        }
        if let Some(kind) = wf_actions.enable_mark_kind {
            self.export.style.style_for_mut(kind).enabled = true;
            need_audio_sync = true;
        }
        if let Some(track) = self.track.as_mut() {
            if let Some(mult) = hud_actions.bpm_multiplier {
                let candidate = track.detected_bpm * (track.bpm_multiplier * mult);
                if (35.0..=260.0).contains(&candidate) {
                    self.history.push_snapshot(track);
                    if editing::toggle_bpm_multiplier(track, mult) {
                        self.wf_state.clear_selection();
                        wf_actions.marks_mutated = true;
                    }
                }
            }
            if let Some(delta) = hud_actions.shift_downbeat {
                self.history.push_snapshot(track);
                editing::shift_downbeat_phase(track, delta);
                self.wf_state.clear_selection();
                wf_actions.marks_mutated = true;
            }
            if let Some(ts) = hud_actions.set_time_sig {
                self.history.push_snapshot(track);
                editing::set_time_signature(track, ts);
                self.wf_state.clear_selection();
                wf_actions.marks_mutated = true;
            }
            if let Some(mode) = hud_actions.set_grid_mode {
                self.history.push_snapshot(track);
                editing::set_grid_mode(track, mode);
                self.wf_state.clear_selection();
                wf_actions.marks_mutated = true;
            }
            if let Some(extrap) = hud_actions.set_extrapolate {
                self.history.push_snapshot(track);
                track.extrapolate_silence = extrap;
                editing::rebuild_marks_from_raw(track);
                self.wf_state.clear_selection();
                wf_actions.marks_mutated = true;
            }
            if let Some(new_sens) = hud_actions.set_drop_sensitivity {
                if !self.drop_drag_undo_pushed {
                    self.history.push_snapshot(track);
                    self.drop_drag_undo_pushed = true;
                }
                track.drop_sensitivity = new_sens;
                editing::rebuild_energy_drops_only(track);
                wf_actions.marks_mutated = true;
            }
            if hud_actions.drop_sensitivity_released {
                self.drop_drag_undo_pushed = false;
            }
            if let Some(delta_sec) = hud_actions.nudge_sec {
                self.history.push_snapshot(track);
                editing::nudge_all_marks(track, delta_sec);
                wf_actions.marks_mutated = true;
            }
            if let Some(z) = hud_actions.zoom_factor {
                let min_pps = (self.main_view_width_px / track.duration_sec.max(1.0) as f32)
                    .min(15.0)
                    .max(0.5);
                self.wf_state.pixels_per_sec =
                    (self.wf_state.pixels_per_sec * z).clamp(min_pps, 900.0);
            }
            if hud_actions.zoom_fit {
                self.wf_state.pixels_per_sec =
                    (self.main_view_width_px / track.duration_sec.max(1.0) as f32)
                        .clamp(0.5, 900.0);
                self.wf_state.view_start_sec = 0.0;
            }
            if wf_actions.marks_mutated {
                need_audio_sync = true;
            }
            if wf_actions.in_out_changed {
                self.player
                    .set_loop_range(track.in_point_sec, track.out_point_sec);
            }
        }
        if need_audio_sync {
            self.validate_selected_mark();
            self.sync_audio_clicks();
        }
        if let Some(hint) = wf_actions.status_hint {
            self.status_message = Some((hint, false));
        }

        // Export actions
        if export_actions.export_marker_edl {
            let res = self
                .track
                .as_ref()
                .ok_or_else(|| "No track".to_string())
                .and_then(|t| export_resolve_marker_edl(t, &self.export).map_err(|e| e.to_string()));
            self.run_export("edl", "Markers", res);
        }
        if export_actions.export_fcpxml {
            let res = self
                .track
                .as_ref()
                .ok_or_else(|| "No track".to_string())
                .and_then(|t| export_fcpxml(t, &self.export).map_err(|e| e.to_string()));
            self.run_export("fcpxml", "Timeline", res);
        }
        if export_actions.export_cut_list_edl {
            let res = self
                .track
                .as_ref()
                .ok_or_else(|| "No track".to_string())
                .and_then(|t| export_cut_list_edl(t, &self.export).map_err(|e| e.to_string()));
            self.run_export("edl", "CutList", res);
        }
        if export_actions.export_csv {
            let res = self
                .track
                .as_ref()
                .ok_or_else(|| "No track".to_string())
                .and_then(|t| export_csv(t, &self.export).map_err(|e| e.to_string()));
            self.run_export("csv", "Beats", res);
        }
        if export_actions.export_json {
            let res = self
                .track
                .as_ref()
                .ok_or_else(|| "No track".to_string())
                .and_then(|t| export_json(t, &self.export).map_err(|e| e.to_string()));
            self.run_export("json", "Beats", res);
        }
        if export_actions.open_preview {
            if let Some(track) = &self.track {
                self.preview_text = export_resolve_marker_edl(track, &self.export)
                    .unwrap_or_else(|e| format!("Preview error: {e}"));
                self.preview_open = true;
            }
        }
        if export_actions.save_portable_config {
            self.save_portable_config(false);
        }
        if shortcuts_actions.save_portable_config {
            self.save_portable_config(true);
        }

        // Live EDL Preview Modal
        if self.preview_open {
            let mut open = self.preview_open;
            egui::Window::new("Marker EDL Preview")
                .open(&mut open)
                .default_size([660.0, 440.0])
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("CMX 3600 Marker EDL · Resolve: Right-click timeline in Media Pool -> Timelines -> Import -> Timeline Markers from EDL...")
                                .size(12.0)
                                .color(brand::MUTED),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if brand::stroke_button(ui, Some(StrokeIcon::Copy), "Copy EDL", false, 26.0)
                                .clicked()
                            {
                                ctx.copy_text(self.preview_text.clone());
                                self.status_message =
                                    Some(("Marker EDL copied to clipboard".into(), false));
                            }
                        });
                    });
                    ui.add_space(6.0);
                    ui.separator();
                    ui.add_space(6.0);
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut self.preview_text)
                                .font(egui::TextStyle::Monospace)
                                .desired_width(f32::INFINITY)
                                .interactive(false),
                        );
                    });
                });
            self.preview_open = open;
        }

        // Automatically persist any preference changes (Click, Outline/Solid/RGB, Tape/Page,
        // Beat Filters, FPS, Volumes, Loop, Speed, Inspector state, Shortcuts, Recent files)
        self.poll_auto_save_preferences(ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        let current = self.current_config();
        if current != self.last_saved_config {
            let _ = current.save_portable();
        }
    }
}
