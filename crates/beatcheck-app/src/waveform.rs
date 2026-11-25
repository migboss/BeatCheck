use crate::brand;
use crate::config::{
    format_live_smpte, mark_color32, step_time_by_frames, ScrollMode, WaveformVisualMode,
};
use crate::shortcuts::{KeyBindings, ShortcutAction};
use beatcheck_core::editing;
use beatcheck_core::model::{MarkExportStyle, MarkKind, TrackAnalysis};
use beatcheck_edl::ExportSettings;
use egui::{
    epaint::{Mesh, PathShape, Vertex, WHITE_UV},
    Align2, Color32, CursorIcon, Pos2, Rect, Response, Sense, Shape, Stroke, StrokeKind, Ui, Vec2,
};
use std::collections::HashSet;

#[derive(Debug, Clone)]
pub struct WaveformState {
    /// Horizontal scale in pixels per second.
    pub pixels_per_sec: f32,
    /// Leftmost visible time in seconds (used in PageScroll mode and when paused).
    pub view_start_sec: f64,
    /// Currently selected mark IDs (supports single click, Shift+Click toggle, and marquee box drag).
    pub selected_mark_ids: HashSet<u64>,
    /// Anchor mark currently being dragged with mouse (moves all `selected_mark_ids` together).
    pub dragging_mark_id: Option<u64>,
    /// Original timestamp of the anchor mark when drag started (for live +/- frame delta HUD).
    pub drag_origin_sec: f64,
    /// Original displayed (quantized) timestamp of the anchor mark when drag started.
    pub drag_origin_display_sec: f64,
    /// Original `(mark_id, time_sec)` of every mark participating in the active drag gesture.
    pub drag_origin_times: Vec<(u64, f64)>,
    /// Last applied time delta during an active drag (avoids no-op Undo snapshots & 60Hz click rebuilds).
    pub drag_last_applied_delta: f64,
    /// Whether an Undo snapshot was already pushed for the current drag gesture.
    pub drag_snapshot_pushed: bool,
    /// Active drag-to-select marquee box on the waveform: `(time_sec, screen_y)`.
    pub marquee_start: Option<(f64, f32)>,
    pub marquee_current: Option<(f64, f32)>,
    /// Pre-drag selection snapshot when holding Shift during a marquee drag.
    pub marquee_base_selection: HashSet<u64>,
    /// True while dragging on the top ruler to scrub the playhead.
    pub scrubbing_ruler: bool,
    /// Context menu target mark ID and persisted timeline position (so empty-timeline popup never blanks!).
    pub context_mark_id: Option<u64>,
    pub context_time_sec: Option<f64>,
    pub label_edit_buf: String,
    pub dragging_in_handle: bool,
    pub dragging_out_handle: bool,
}

impl Default for WaveformState {
    fn default() -> Self {
        Self {
            pixels_per_sec: 115.0,
            view_start_sec: 0.0,
            selected_mark_ids: HashSet::new(),
            dragging_mark_id: None,
            drag_origin_sec: 0.0,
            drag_origin_display_sec: 0.0,
            drag_origin_times: Vec::new(),
            drag_last_applied_delta: 0.0,
            drag_snapshot_pushed: false,
            marquee_start: None,
            marquee_current: None,
            marquee_base_selection: HashSet::new(),
            scrubbing_ruler: false,
            context_mark_id: None,
            context_time_sec: None,
            label_edit_buf: String::new(),
            dragging_in_handle: false,
            dragging_out_handle: false,
        }
    }
}

impl WaveformState {
    pub fn select_single(&mut self, id: u64) {
        self.selected_mark_ids.clear();
        self.selected_mark_ids.insert(id);
    }

    pub fn toggle_selected(&mut self, id: u64) {
        if !self.selected_mark_ids.remove(&id) {
            self.selected_mark_ids.insert(id);
        }
    }

    pub fn clear_selection(&mut self) {
        self.selected_mark_ids.clear();
        self.cancel_active_gestures();
    }

    pub fn cancel_active_gestures(&mut self) {
        self.dragging_mark_id = None;
        self.drag_origin_times.clear();
        self.drag_last_applied_delta = 0.0;
        self.drag_snapshot_pushed = false;
        self.marquee_start = None;
        self.marquee_current = None;
        self.marquee_base_selection.clear();
    }

    pub fn is_selected(&self, id: u64) -> bool {
        self.selected_mark_ids.contains(&id)
    }

    pub fn selected_count(&self) -> usize {
        self.selected_mark_ids.len()
    }
}

/// Actions emitted by interactive waveform clicks/drags for the parent app to apply.
#[derive(Debug, Default)]
pub struct WaveformActions {
    pub seek_to: Option<f64>,
    /// Pre-mutation TrackAnalysis snapshot captured BEFORE editing so Undo always restores the true prior state!
    pub undo_snapshot: Option<TrackAnalysis>,
    pub enable_mark_kind: Option<MarkKind>,
    pub marks_mutated: bool,
    pub in_out_changed: bool,
    pub status_hint: Option<String>,
}

/// Paints a precise, world-time-locked symmetrical waveform outline
/// using a GPU triangle-strip fill mesh + crisp hairline contour strokes (zero shake, razor transients!).
#[allow(clippy::too_many_arguments)]
fn paint_smooth_outline_mesh(
    painter: &egui::Painter,
    _rect: Rect,
    xs: &[f32],
    heights: &[f32],
    mid_y: f32,
    playhead_x: f32,
    is_mini: bool,
) {
    if xs.len() < 2 {
        return;
    }

    // Keep full transient sharpness on the main waveform (no blurry attack/release hold!);
    // apply only a tiny 1-pass micro-smoothing on the mini overview strip.
    let mut pts_x = Vec::with_capacity(xs.len());
    let mut pts_h = Vec::with_capacity(heights.len());
    if is_mini {
        let mut sm = heights.to_vec();
        for i in 1..sm.len() - 1 {
            sm[i] = 0.20 * heights[i - 1] + 0.60 * heights[i] + 0.20 * heights[i + 1];
        }
        pts_x.extend_from_slice(xs);
        pts_h.extend_from_slice(&sm);
    } else {
        // Precise peak-preserving contour: keeps every kick & snare spike exact to the millisecond
        pts_x.extend_from_slice(xs);
        pts_h.extend_from_slice(heights);
    }

    // Continuous GPU triangle-strip interior fill (zero vertical banding!)
    let mut mesh = Mesh::default();
    let fill_played = if is_mini {
        brand::primary_tint(54)
    } else {
        brand::primary_tint(42)
    };
    let fill_upcoming = if is_mini {
        brand::deep_violet_tint(38)
    } else {
        brand::deep_violet_tint(28)
    };

    let mut top_played = Vec::new();
    let mut bot_played = Vec::new();
    let mut top_upcoming = Vec::new();
    let mut bot_upcoming = Vec::new();

    for idx in 0..pts_x.len() {
        let x = pts_x[idx];
        let h = pts_h[idx].max(0.8);
        let pt_top = Pos2::new(x, mid_y - h);
        let pt_bot = Pos2::new(x, mid_y + h);
        let col = if x <= playhead_x {
            fill_played
        } else {
            fill_upcoming
        };

        let base = mesh.vertices.len() as u32;
        mesh.vertices.push(Vertex {
            pos: pt_top,
            uv: WHITE_UV,
            color: col,
        });
        mesh.vertices.push(Vertex {
            pos: pt_bot,
            uv: WHITE_UV,
            color: col,
        });
        if idx > 0 {
            let prev = base - 2;
            mesh.indices
                .extend_from_slice(&[prev, prev + 1, base, prev + 1, base + 1, base]);
        }

        if x <= playhead_x {
            top_played.push(pt_top);
            bot_played.push(pt_bot);
        } else {
            if top_upcoming.is_empty() {
                if let Some(&lt) = top_played.last() {
                    top_upcoming.push(lt);
                }
                if let Some(&lb) = bot_played.last() {
                    bot_upcoming.push(lb);
                }
            }
            top_upcoming.push(pt_top);
            bot_upcoming.push(pt_bot);
        }
    }

    painter.add(Shape::mesh(mesh));

    // Delicate, razor-precise contour outline
    let stroke_up = if is_mini { 1.05 } else { 1.15 };
    let stroke_pl = if is_mini { 1.15 } else { 1.25 };

    if top_upcoming.len() >= 2 {
        painter.add(Shape::Path(PathShape::line(
            top_upcoming,
            Stroke::new(stroke_up, brand::primary_tint(220)),
        )));
        painter.add(Shape::Path(PathShape::line(
            bot_upcoming,
            Stroke::new(stroke_up, brand::primary_tint(220)),
        )));
    }
    if top_played.len() >= 2 {
        painter.add(Shape::Path(PathShape::line(
            top_played,
            Stroke::new(stroke_pl, brand::highlight_tint(245)),
        )));
        painter.add(Shape::Path(PathShape::line(
            bot_played,
            Stroke::new(stroke_pl, brand::highlight_tint(245)),
        )));
    }
}

/// Draw the full-track mini-overview strip (top navigation bar with In/Out range brackets).
#[allow(clippy::too_many_arguments)]
pub fn draw_mini_overview(
    ui: &mut Ui,
    track: &mut TrackAnalysis,
    wf: &mut WaveformState,
    style: &MarkExportStyle,
    visual_mode: WaveformVisualMode,
    playhead_sec: f64,
    main_view_width_px: f32,
    actions: &mut WaveformActions,
) {
    let desired = Vec2::new(ui.available_width(), 42.0);
    let (rect, _) = ui.allocate_exact_size(desired, Sense::hover());
    let resp = ui.interact(
        rect,
        egui::Id::new("beatcheck_mini_overview"),
        Sense::click_and_drag(),
    );
    let painter = ui.painter_at(rect);

    painter.rect_filled(rect, 8.0, brand::CANVAS);

    let dur = track.duration_sec.max(0.01);
    let rw = rect.width().max(1.0);
    let time_to_x = |t: f64| rect.left() + ((t / dur) as f32).clamp(0.0, 1.0) * rw;
    let x_to_time = |x: f32| (((x - rect.left()) / rw).clamp(0.0, 1.0) as f64) * dur;

    // Draw overview waveform (Smooth Luminous Outline or DJ RGB)
    if !track.waveform.is_empty() {
        let mid_y = rect.center().y;
        let half_h = rect.height() * 0.38;

        if visual_mode != WaveformVisualMode::RgbFrequency {
            let step_px = 5.0f32;
            let samples = ((rect.width() / step_px).ceil() as usize).max(2);
            let mut xs = Vec::with_capacity(samples + 1);
            let mut hs = Vec::with_capacity(samples + 1);

            for s in 0..=samples {
                let x = (rect.left() + s as f32 * step_px).min(rect.right());
                let frac0 = (s as f32 / samples as f32).clamp(0.0, 1.0);
                let frac1 = ((s + 1) as f32 / samples as f32).clamp(0.0, 1.0);
                let b0 = ((frac0 * track.waveform.len() as f32) as usize)
                    .min(track.waveform.len() - 1);
                let b1 = ((frac1 * track.waveform.len() as f32).ceil() as usize)
                    .max(b0 + 1)
                    .min(track.waveform.len());
                let mut peak = 0.0f32;
                for bin in &track.waveform[b0..b1] {
                    peak = peak.max(bin.peak);
                }
                xs.push(x);
                hs.push((peak * half_h).max(1.0));
            }
            paint_smooth_outline_mesh(
                &painter,
                rect,
                &xs,
                &hs,
                mid_y,
                time_to_x(playhead_sec),
                true,
            );
        } else {
            let cols = (rect.width().floor() as usize).max(1);
            for c in 0..cols {
                let b0 = c * track.waveform.len() / cols;
                let b1 = ((c + 1) * track.waveform.len() / cols)
                    .max(b0 + 1)
                    .min(track.waveform.len());
                let mut peak = 0.0f32;
                let mut low = 0.0f32;
                let mut mid = 0.0f32;
                let mut high = 0.0f32;
                for bin in &track.waveform[b0..b1] {
                    peak = peak.max(bin.peak);
                    low = low.max(bin.low);
                    mid = mid.max(bin.mid);
                    high = high.max(bin.high);
                }
                let x = rect.left() + c as f32;
                let h = (peak * half_h).max(1.0);
                let col = dj_rgb_color(low, mid, high, 145);
                painter.line_segment(
                    [Pos2::new(x, mid_y - h), Pos2::new(x, mid_y + h)],
                    Stroke::new(1.0, col),
                );
            }
        }
    }

    // Dim excluded In/Out regions
    let in_x = time_to_x(track.in_point_sec);
    let out_x = time_to_x(track.out_point_sec);
    if in_x > rect.left() {
        painter.rect_filled(
            Rect::from_min_max(rect.min, Pos2::new(in_x, rect.bottom())),
            0.0,
            Color32::from_rgba_unmultiplied(0, 0, 0, 145),
        );
    }
    if out_x < rect.right() {
        painter.rect_filled(
            Rect::from_min_max(Pos2::new(out_x, rect.top()), rect.max),
            0.0,
            Color32::from_rgba_unmultiplied(0, 0, 0, 145),
        );
    }

    // Energy Drop ticks on mini-overview (only if EnergyDrop is enabled!)
    if style.energy_drop.enabled {
        let drop_col = mark_color32(style, MarkKind::EnergyDrop);
        for m in track.marks.iter().filter(|m| m.kind == MarkKind::EnergyDrop) {
            let x = time_to_x(m.time_sec);
            painter.line_segment(
                [Pos2::new(x, rect.top() + 3.0), Pos2::new(x, rect.bottom() - 3.0)],
                Stroke::new(1.5, drop_col),
            );
        }
    }

    // Current viewport highlight window in electric violet
    let view_span_sec = (main_view_width_px / wf.pixels_per_sec.max(0.5)) as f64;
    let vx0 = time_to_x(wf.view_start_sec);
    let vx1 = time_to_x((wf.view_start_sec + view_span_sec).min(dur));
    let view_rect = Rect::from_min_max(
        Pos2::new(vx0, rect.top() + 1.5),
        Pos2::new(vx1.max(vx0 + 6.0), rect.bottom() - 1.5),
    );
    painter.rect_filled(view_rect, 4.0, brand::primary_tint(28));
    painter.rect_stroke(
        view_rect,
        4.0,
        Stroke::new(1.0, brand::highlight_tint(160)),
        StrokeKind::Inside,
    );

    // In / Out bracket handles
    let bracket_col = brand::LAVENDER;
    painter.line_segment(
        [Pos2::new(in_x, rect.top()), Pos2::new(in_x, rect.bottom())],
        Stroke::new(2.0, bracket_col),
    );
    painter.line_segment(
        [Pos2::new(out_x, rect.top()), Pos2::new(out_x, rect.bottom())],
        Stroke::new(2.0, bracket_col),
    );

    // Playhead line
    let px = time_to_x(playhead_sec);
    painter.line_segment(
        [Pos2::new(px, rect.top()), Pos2::new(px, rect.bottom())],
        Stroke::new(1.5, brand::TEXT),
    );

    painter.rect_stroke(rect, 8.0, Stroke::new(1.0, brand::BORDER), StrokeKind::Inside);

    // Drag In/Out handles or click/drag to seek
    if let Some(pos) = resp.interact_pointer_pos() {
        if resp.drag_started() {
            if (pos.x - in_x).abs() < 8.0 {
                wf.dragging_in_handle = true;
                actions.undo_snapshot = Some(track.clone());
            } else if (pos.x - out_x).abs() < 8.0 {
                wf.dragging_out_handle = true;
                actions.undo_snapshot = Some(track.clone());
            }
        }
        if wf.dragging_in_handle && resp.dragged() {
            track.in_point_sec = x_to_time(pos.x).min(track.out_point_sec - 0.5).max(0.0);
            actions.in_out_changed = true;
        } else if wf.dragging_out_handle && resp.dragged() {
            track.out_point_sec = x_to_time(pos.x).max(track.in_point_sec + 0.5).min(dur);
            actions.in_out_changed = true;
        } else if resp.clicked() || resp.dragged() {
            let t = x_to_time(pos.x);
            actions.seek_to = Some(t);
            wf.view_start_sec =
                (t - view_span_sec * 0.35).clamp(0.0, (dur - view_span_sec).max(0.0));
        }
    }
    if resp.drag_stopped() || (!resp.dragged() && !resp.drag_started()) {
        wf.dragging_in_handle = false;
        wf.dragging_out_handle = false;
    }
}

/// Draw the main zoomable waveform canvas with smooth Luminous Vector Outline,
/// 1:1 filtered DaVinci Resolve markers (hidden when disabled!), DaVinci frame-snap dragging,
/// and live SMPTE + frame-delta HUD tooltip.
#[allow(clippy::too_many_arguments)]
pub fn draw_main_waveform(
    ui: &mut Ui,
    track: &mut TrackAnalysis,
    wf: &mut WaveformState,
    export: &ExportSettings,
    shortcuts: &KeyBindings,
    visual_mode: WaveformVisualMode,
    scroll_mode: ScrollMode,
    playhead_sec: f64,
    is_playing: bool,
    actions: &mut WaveformActions,
) -> f32 {
    let style = &export.style;
    let frame_sec = 1.0 / export.frame_rate.fps_f64();

    let avail = ui.available_size();
    let desired = Vec2::new(avail.x.max(200.0), avail.y.max(200.0));
    let (rect, _) = ui.allocate_exact_size(desired, Sense::hover());
    let resp = ui.interact(
        rect,
        egui::Id::new("beatcheck_main_waveform"),
        Sense::click_and_drag(),
    );

    let dur = track.duration_sec.max(0.01);
    let min_pps = (rect.width() / dur as f32).min(15.0).max(0.5);
    let view_span_sec = (rect.width() / wf.pixels_per_sec.max(0.5)) as f64;

    // Follow playhead smoothly during playback
    if is_playing {
        match scroll_mode {
            ScrollMode::CenteredTape => {
                wf.view_start_sec = playhead_sec - view_span_sec * 0.35;
            }
            ScrollMode::PageScroll => {
                if playhead_sec < wf.view_start_sec
                    || playhead_sec > wf.view_start_sec + view_span_sec * 0.92
                {
                    wf.view_start_sec = (playhead_sec - view_span_sec * 0.08)
                        .clamp(0.0, (dur - view_span_sec).max(0.0));
                }
            }
        }
    }

    // Mouse wheel zoom & horizontal pan
    if resp.hovered() {
        let (scroll_delta, modifiers, hover_pos) =
            ui.input(|i| (i.smooth_scroll_delta, i.modifiers, i.pointer.hover_pos()));
        if scroll_delta.y.abs() > 0.1 && !modifiers.shift {
            let anchor_x = hover_pos.map(|p| p.x).unwrap_or(rect.center().x);
            let anchor_frac = ((anchor_x - rect.left()) / rect.width()).clamp(0.0, 1.0) as f64;
            let anchor_time = wf.view_start_sec + anchor_frac * view_span_sec;

            let factor = (1.0 + scroll_delta.y * 0.003).clamp(0.75, 1.35);
            wf.pixels_per_sec = (wf.pixels_per_sec * factor).clamp(min_pps, 900.0);
            let new_span = (rect.width() / wf.pixels_per_sec) as f64;
            wf.view_start_sec = anchor_time - anchor_frac * new_span;
        } else if scroll_delta.x.abs() > 0.1 || (modifiers.shift && scroll_delta.y.abs() > 0.1) {
            let dx = if scroll_delta.x.abs() > 0.1 {
                scroll_delta.x
            } else {
                scroll_delta.y
            };
            wf.view_start_sec -= (dx / wf.pixels_per_sec) as f64;
        }
    }

    if !is_playing || scroll_mode == ScrollMode::PageScroll {
        let min_start = if scroll_mode == ScrollMode::CenteredTape {
            -view_span_sec * 0.35
        } else {
            0.0
        };
        wf.view_start_sec = wf.view_start_sec.clamp(min_start, dur);
    }

    let ruler_h = 28.0;
    let wave_rect = Rect::from_min_max(Pos2::new(rect.left(), rect.top() + ruler_h), rect.max);
    let ruler_rect = Rect::from_min_max(rect.min, Pos2::new(rect.right(), rect.top() + ruler_h));

    let view_start = wf.view_start_sec;
    let pps = wf.pixels_per_sec;
    let time_to_x = move |t: f64| rect.left() + ((t - view_start) as f32) * pps;
    let x_to_time = move |x: f32| view_start + ((x - rect.left()) / pps) as f64;

    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 10.0, brand::CANVAS);
    painter.rect_filled(ruler_rect, 0.0, brand::PANEL);
    painter.line_segment(
        [
            Pos2::new(rect.left(), wave_rect.top()),
            Pos2::new(rect.right(), wave_rect.top()),
        ],
        Stroke::new(1.0, brand::BORDER),
    );

    // Subtle alternating musical bar shading
    let view_end_sec = wf.view_start_sec + view_span_sec;
    let downbeats: Vec<(u32, f64)> = track
        .marks
        .iter()
        .filter(|m| m.kind == MarkKind::Downbeat)
        .map(|m| (m.bar, m.time_sec))
        .collect();
    for pair in downbeats.windows(2) {
        let (bar, t0) = pair[0];
        let t1 = pair[1].1;
        if bar % 2 == 0 && t1 >= wf.view_start_sec && t0 <= view_end_sec {
            let x0 = time_to_x(t0).max(wave_rect.left());
            let x1 = time_to_x(t1).min(wave_rect.right());
            if x1 > x0 {
                painter.rect_filled(
                    Rect::from_min_max(
                        Pos2::new(x0, wave_rect.top()),
                        Pos2::new(x1, wave_rect.bottom()),
                    ),
                    0.0,
                    Color32::from_rgba_unmultiplied(139, 92, 246, 6),
                );
            }
        }
    }

    // Subtle dB reference hairlines
    let mid_y = wave_rect.center().y;
    let half_h = wave_rect.height() * 0.36;
    for frac in [-0.5f32, 0.0, 0.5] {
        let y = mid_y + frac * half_h;
        painter.line_segment(
            [Pos2::new(wave_rect.left(), y), Pos2::new(wave_rect.right(), y)],
            Stroke::new(
                1.0,
                Color32::from_rgba_unmultiplied(255, 255, 255, if frac == 0.0 { 16 } else { 7 }),
            ),
        );
    }

    // Subtle video-frame grid lines when zoomed in (anchored to export origin so rebased grids match Resolve!)
    let grid_origin_sec = export.export_origin_sec(track.in_point_sec);
    let frame_px = (frame_sec as f32) * pps;
    if frame_px >= 12.0 {
        let first_f = export
            .frame_rate
            .seconds_to_frames(wf.view_start_sec - grid_origin_sec)
            - 1;
        let last_f = export
            .frame_rate
            .seconds_to_frames(view_end_sec - grid_origin_sec)
            + 1;
        let grid_alpha = ((frame_px - 12.0) * 1.4).clamp(10.0, 26.0) as u8;
        let grid_col = Color32::from_rgba_unmultiplied(255, 255, 255, grid_alpha);
        for f in first_f..=last_f {
            let t = grid_origin_sec + export.frame_rate.frames_to_seconds(f);
            if t < 0.0 || t > dur {
                continue;
            }
            let fx = time_to_x(t);
            if fx >= wave_rect.left() && fx <= wave_rect.right() {
                painter.line_segment(
                    [Pos2::new(fx, wave_rect.top()), Pos2::new(fx, wave_rect.bottom())],
                    Stroke::new(1.0, grid_col),
                );
            }
        }
    }

    // 1. Render Waveform (Precise Outline | HD Solid Actual Waveform | DJ RGB Bands)
    let px = time_to_x(playhead_sec);
    if !track.waveform.is_empty() {
        let bins_per_sec = track.waveform_bins_per_sec as f64;
        let hd_half_h = wave_rect.height() * 0.44;
        match visual_mode {
            WaveformVisualMode::LuminousOutline => {
                // High-precision world-time-locked bin sampling (~2.0px spacing -> razor-sharp transients, ZERO shake!)
                let target_bins = ((2.2f64 / pps.max(0.5) as f64) * bins_per_sec).max(1.0);
                let mut stride: usize = 1;
                while (stride as f64) < target_bins && stride < 512 {
                    stride *= 2;
                }

                let total_nodes = (track.waveform.len() + stride - 1) / stride;
                let margin_nodes: isize = 4;
                let vis_start_bin = (wf.view_start_sec * bins_per_sec).floor() as isize;
                let vis_end_bin = (view_end_sec * bins_per_sec).ceil() as isize;
                let first_node = ((vis_start_bin / stride as isize) - margin_nodes).max(0) as usize;
                let last_node = ((vis_end_bin / stride as isize) + margin_nodes)
                    .clamp(0, total_nodes.saturating_sub(1) as isize)
                    as usize;

                let mut xs = Vec::with_capacity(last_node.saturating_sub(first_node) + 2);
                let mut hs = Vec::with_capacity(last_node.saturating_sub(first_node) + 2);

                for node in first_node..=last_node {
                    let b0 = (node * stride).min(track.waveform.len() - 1);
                    let b1 = ((node + 1) * stride).min(track.waveform.len()).max(b0 + 1);
                    let mut peak = -1.0f32;
                    let mut peak_bin = b0;
                    for (offset, bin) in track.waveform[b0..b1].iter().enumerate() {
                        if bin.peak > peak {
                            peak = bin.peak;
                            peak_bin = b0 + offset;
                        }
                    }
                    // Anchor vertex at exact peak bin time (±1.25ms) so kick spikes align 1:1 with Resolve!
                    let t_peak = (peak_bin as f64 + 0.5) / bins_per_sec;
                    xs.push(time_to_x(t_peak));
                    hs.push((peak.max(0.0) * hd_half_h).max(1.0));
                }

                paint_smooth_outline_mesh(&painter, wave_rect, &xs, &hs, mid_y, px, false);
            }
            WaveformVisualMode::ResolveMono => {
                // HD Solid Mode: maximum-precision actual waveform (DAW/Fairlight style, 1px Peak + RMS)
                let step_px = 1.0f32;
                let mut x = wave_rect.left();
                while x < wave_rect.right() {
                    let t0 = x_to_time(x);
                    let t1 = x_to_time(x + step_px);
                    if t1 >= 0.0 && t0 <= dur {
                        let b0 = ((t0.max(0.0) * bins_per_sec).floor() as usize)
                            .min(track.waveform.len() - 1);
                        let b1 = ((t1.min(dur) * bins_per_sec).ceil() as usize)
                            .max(b0 + 1)
                            .min(track.waveform.len());

                        let mut peak = 0.0f32;
                        let mut rms = 0.0f32;
                        for bin in &track.waveform[b0..b1] {
                            peak = peak.max(bin.peak);
                            rms = rms.max(bin.rms);
                        }

                        let h_peak = (peak * hd_half_h).max(1.0);
                        let h_rms = (rms * hd_half_h).max(0.5);
                        let played = x <= px;

                        let peak_col = if played {
                            brand::highlight_tint(215)
                        } else {
                            brand::primary_tint(185)
                        };
                        let rms_col = if played {
                            Color32::from_rgba_unmultiplied(245, 240, 255, 235)
                        } else {
                            Color32::from_rgba_unmultiplied(210, 198, 252, 205)
                        };

                        painter.line_segment(
                            [Pos2::new(x, mid_y - h_peak), Pos2::new(x, mid_y + h_peak)],
                            Stroke::new(1.0, peak_col),
                        );
                        painter.line_segment(
                            [Pos2::new(x, mid_y - h_rms), Pos2::new(x, mid_y + h_rms)],
                            Stroke::new(1.0, rms_col),
                        );
                    }
                    x += step_px;
                }
            }
            WaveformVisualMode::RgbFrequency => {
                // DJ RGB Frequency Bands (Red=Bass/Kick, Green=Mids/Snare, Blue=Highs/Hats)
                let step_px = 1.0f32;
                let mut x = wave_rect.left();
                while x < wave_rect.right() {
                    let t0 = x_to_time(x);
                    let t1 = x_to_time(x + step_px);
                    if t1 >= 0.0 && t0 <= dur {
                        let b0 = ((t0.max(0.0) * bins_per_sec).floor() as usize)
                            .min(track.waveform.len() - 1);
                        let b1 = ((t1.min(dur) * bins_per_sec).ceil() as usize)
                            .max(b0 + 1)
                            .min(track.waveform.len());

                        let mut peak = 0.0f32;
                        let mut rms = 0.0f32;
                        let mut low = 0.0f32;
                        let mut mid = 0.0f32;
                        let mut high = 0.0f32;
                        for bin in &track.waveform[b0..b1] {
                            peak = peak.max(bin.peak);
                            rms = rms.max(bin.rms);
                            low = low.max(bin.low);
                            mid = mid.max(bin.mid);
                            high = high.max(bin.high);
                        }

                        let h_peak = (peak * hd_half_h).max(1.0);
                        let h_rms = (rms * hd_half_h).max(0.5);
                        let outer_col = dj_rgb_color(low, mid, high, 185);
                        let inner_col = dj_rgb_color(
                            (low * 1.25).min(1.0),
                            (mid * 1.25).min(1.0),
                            (high * 1.25).min(1.0),
                            235,
                        );

                        painter.line_segment(
                            [Pos2::new(x, mid_y - h_peak), Pos2::new(x, mid_y + h_peak)],
                            Stroke::new(1.0, outer_col),
                        );
                        painter.line_segment(
                            [Pos2::new(x, mid_y - h_rms), Pos2::new(x, mid_y + h_rms)],
                            Stroke::new(1.0, inner_col),
                        );
                    }
                    x += step_px;
                }
            }
        }
    }

    // Shade excluded In/Out regions subtly
    let in_x = time_to_x(track.in_point_sec);
    let out_x = time_to_x(track.out_point_sec);
    if in_x > wave_rect.left() {
        let r = Rect::from_min_max(
            wave_rect.min,
            Pos2::new(in_x.min(wave_rect.right()), wave_rect.bottom()),
        );
        painter.rect_filled(r, 0.0, Color32::from_rgba_unmultiplied(0, 0, 0, 128));
    }
    if out_x < wave_rect.right() {
        let r = Rect::from_min_max(
            Pos2::new(out_x.max(wave_rect.left()), wave_rect.top()),
            wave_rect.max,
        );
        painter.rect_filled(r, 0.0, Color32::from_rgba_unmultiplied(0, 0, 0, 128));
    }

    // 2. Render ONLY enabled/included markers at their EXACT exported video-frame time (100% WYSIWYG with Resolve!).
    // When actively holding Alt to sub-frame-drag, preview dragged marks at their live sub-frame position.
    let (hover_pos, shift_held, alt_held) =
        ui.input(|i| (i.pointer.hover_pos(), i.modifiers.shift, i.modifiers.alt));
    let sub_frame_drag_active = wf.dragging_mark_id.is_some() && alt_held;
    let mark_display_time = |id: u64, raw_t: f64| -> f64 {
        if sub_frame_drag_active && wf.is_selected(id) {
            raw_t.clamp(0.0, dur)
        } else {
            export
                .quantized_mark_time_sec(raw_t, track.in_point_sec)
                .clamp(0.0, dur)
        }
    };

    // Collect visible marks with their screen X positions for exact hit-testing and density checks.
    let visible_marks: Vec<(u64, f32)> = track
        .marks
        .iter()
        .filter(|m| {
            let dt = mark_display_time(m.id, m.time_sec);
            style.is_mark_included(m)
                && dt >= wf.view_start_sec - 0.5
                && dt <= view_end_sec + 0.5
        })
        .map(|m| (m.id, time_to_x(mark_display_time(m.id, m.time_sec))))
        .collect();

    let hit_mark_at = |pt: Pos2, max_dist: f32| -> Option<(u64, f32, f32)> {
        if !wave_rect.contains(pt) {
            return None;
        }
        let mut best_idx: Option<usize> = None;
        let mut best_dist = max_dist + 0.001;
        for (idx, &(_id, mx)) in visible_marks.iter().enumerate() {
            let d = (pt.x - mx).abs();
            if d <= best_dist {
                best_dist = d;
                best_idx = Some(idx);
            }
        }
        let idx = best_idx?;
        let (id, mx) = visible_marks[idx];
        // Compute pixel distance to nearest distinct-x neighbor so densely packed zoomed-out
        // markers default to marquee box selection rather than accidentally grabbing a single line.
        let mut min_neighbor_dx = f32::INFINITY;
        for (j, &(_, ox)) in visible_marks.iter().enumerate() {
            if j != idx {
                let dx = (ox - mx).abs();
                if dx > 0.5 && dx < min_neighbor_dx {
                    min_neighbor_dx = dx;
                }
            }
        }
        Some((id, best_dist, min_neighbor_dx))
    };

    let hovered_hit = hover_pos.and_then(|hp| hit_mark_at(hp, 7.0));
    let hovered_mark_id = hovered_hit.map(|(id, _, _)| id);
    let can_drag_hovered_mark = hovered_hit.is_some_and(|(id, dist, neighbor_dx)| {
        !shift_held && (wf.is_selected(id) || (neighbor_dx >= 20.0 && dist <= 5.0))
    });

    for m in &track.marks {
        if !style.is_mark_included(m) {
            continue;
        }
        let disp_t = mark_display_time(m.id, m.time_sec);
        if disp_t < wf.view_start_sec - 0.5 || disp_t > view_end_sec + 0.5 {
            continue;
        }
        let x = time_to_x(disp_t);
        if x < rect.left() - 2.0 || x > rect.right() + 2.0 {
            continue;
        }

        let base_col = mark_color32(style, m.kind);
        let is_selected = wf.is_selected(m.id);
        let is_hovered = hovered_mark_id == Some(m.id) || wf.dragging_mark_id == Some(m.id);

        let dt_after_hit = playhead_sec - disp_t;
        let hit_glow = if is_playing && (0.0..=0.14).contains(&dt_after_hit) {
            (1.0 - (dt_after_hit / 0.14) as f32).powi(2)
        } else {
            0.0
        };

        let alpha: u8 = if is_selected || is_hovered || hit_glow > 0.05 {
            255
        } else {
            match m.kind {
                MarkKind::Downbeat | MarkKind::EnergyDrop => 240,
                MarkKind::StrongBeat => 215,
                MarkKind::RegularBeat => 170,
            }
        };

        let col = Color32::from_rgba_unmultiplied(base_col.r(), base_col.g(), base_col.b(), alpha);
        let base_width = match m.kind {
            MarkKind::Downbeat | MarkKind::EnergyDrop => 2.0,
            MarkKind::StrongBeat => 1.6,
            MarkKind::RegularBeat => 1.2,
        };
        let width =
            base_width + hit_glow * 2.2 + if is_selected || is_hovered { 1.3 } else { 0.0 };

        if hit_glow > 0.05 || is_selected {
            let halo_alpha = ((hit_glow * 105.0) + if is_selected { 60.0 } else { 0.0 }) as u8;
            let halo_col = Color32::from_rgba_unmultiplied(
                base_col.r(),
                base_col.g(),
                base_col.b(),
                halo_alpha,
            );
            painter.line_segment(
                [Pos2::new(x, wave_rect.top()), Pos2::new(x, wave_rect.bottom())],
                Stroke::new(width + 6.0, halo_col),
            );
        }

        let (y_top, y_bot) = match m.kind {
            MarkKind::Downbeat | MarkKind::EnergyDrop => (wave_rect.top(), wave_rect.bottom()),
            MarkKind::StrongBeat => (wave_rect.top() + 8.0, wave_rect.bottom() - 8.0),
            MarkKind::RegularBeat => (wave_rect.top() + 18.0, wave_rect.bottom() - 18.0),
        };

        painter.line_segment([Pos2::new(x, y_top), Pos2::new(x, y_bot)], Stroke::new(width, col));

        // Selected marker crisp white handle diamond at top of wave_rect
        if is_selected {
            let cy = wave_rect.top() + 7.0;
            let r = 5.5;
            let diamond = vec![
                Pos2::new(x, cy - r),
                Pos2::new(x + r, cy),
                Pos2::new(x, cy + r),
                Pos2::new(x - r, cy),
            ];
            painter.add(Shape::convex_polygon(
                diamond,
                brand::TEXT,
                Stroke::new(1.5, base_col),
            ));
        }

        // Ruler tags for Downbeats and Energy Drops
        if m.kind == MarkKind::Downbeat {
            let tag_rect =
                Rect::from_min_size(Pos2::new(x, ruler_rect.top() + 4.0), Vec2::new(34.0, 19.0));
            painter.rect_filled(
                tag_rect,
                4.0,
                Color32::from_rgba_unmultiplied(base_col.r(), base_col.g(), base_col.b(), 220),
            );
            painter.text(
                tag_rect.center(),
                Align2::CENTER_CENTER,
                format!("B{}", m.bar),
                brand::font_mono_bold(10.5),
                Color32::WHITE,
            );
        } else if m.kind == MarkKind::EnergyDrop {
            let tag_rect =
                Rect::from_min_size(Pos2::new(x, ruler_rect.top() + 4.0), Vec2::new(40.0, 19.0));
            painter.rect_filled(tag_rect, 4.0, base_col);
            painter.text(
                tag_rect.center(),
                Align2::CENTER_CENTER,
                "DROP",
                brand::font_mono_bold(9.5),
                Color32::from_rgb(8, 24, 14),
            );
        }
    }

    // 3. Live Floating Frame-Snap HUD Pill while dragging one or more markers (DaVinci Resolve feel!)
    if let Some(drag_id) = wf.dragging_mark_id {
        if let Some(m) = track.marks.iter().find(|m| m.id == drag_id) {
            let disp_t = mark_display_time(m.id, m.time_sec);
            let x = time_to_x(disp_t);
            let smpte_t = export.mark_smpte_offset_sec(m.time_sec, track.in_point_sec);
            let tc = format_live_smpte(export.frame_rate, &export.start_timecode, smpte_t);
            let delta_str = if sub_frame_drag_active {
                let delta_ms = ((m.time_sec - wf.drag_origin_sec) * 1000.0).round() as i64;
                format!("{delta_ms:+}ms")
            } else {
                let delta_frames =
                    ((disp_t - wf.drag_origin_display_sec) / frame_sec).round() as i64;
                if delta_frames == 0 {
                    "0f".to_string()
                } else if delta_frames > 0 {
                    format!("+{delta_frames}f")
                } else {
                    format!("{delta_frames}f")
                }
            };
            let drag_count = wf.drag_origin_times.len().max(1);
            let hud_text = if drag_count > 1 {
                format!("{drag_count} marks · {tc}  ({delta_str})")
            } else {
                format!("{tc}  ({delta_str})")
            };
            let galley = painter.layout_no_wrap(hud_text, brand::font_mono_bold(11.5), brand::TEXT);
            let pill_w = galley.size().x + 16.0;
            let pill_h = 24.0;
            let pill_x = (x + 10.0)
                .min(wave_rect.right() - pill_w - 6.0)
                .max(wave_rect.left() + 6.0);
            let pill_rect = Rect::from_min_size(
                Pos2::new(pill_x, wave_rect.top() + 14.0),
                Vec2::new(pill_w, pill_h),
            );
            painter.rect_filled(pill_rect, 6.0, brand::PANEL);
            painter.rect_stroke(
                pill_rect,
                6.0,
                Stroke::new(1.0, brand::PRIMARY),
                StrokeKind::Inside,
            );
            painter.galley(
                Pos2::new(
                    pill_rect.left() + 8.0,
                    pill_rect.center().y - galley.size().y * 0.5,
                ),
                galley,
                brand::LAVENDER,
            );
        }
    }

    // 4. Draw Playhead Cursor
    if px >= rect.left() && px <= rect.right() {
        painter.line_segment(
            [Pos2::new(px, rect.top()), Pos2::new(px, rect.bottom())],
            Stroke::new(4.5, brand::primary_tint(58)),
        );
        painter.line_segment(
            [Pos2::new(px, rect.top()), Pos2::new(px, rect.bottom())],
            Stroke::new(1.75, brand::TEXT),
        );
        let tri = vec![
            Pos2::new(px - 6.0, rect.top()),
            Pos2::new(px + 6.0, rect.top()),
            Pos2::new(px, rect.top() + 9.0),
        ];
        painter.add(Shape::convex_polygon(tri, brand::HIGHLIGHT, Stroke::NONE));
    }

    painter.rect_stroke(rect, 10.0, Stroke::new(1.0, brand::BORDER), StrokeKind::Inside);

    if wf.marquee_start.is_some() {
        ui.ctx().set_cursor_icon(CursorIcon::Crosshair);
    } else if wf.dragging_mark_id.is_some() || can_drag_hovered_mark {
        ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
    }

    handle_waveform_interactions(
        ui,
        &resp,
        ruler_rect,
        wave_rect,
        track,
        export,
        shortcuts,
        wf,
        hovered_mark_id,
        &hit_mark_at,
        frame_sec,
        &x_to_time,
        actions,
    );

    // 5. Paint active drag-to-select marquee box (zero frame lag!)
    if let (Some((t0, y0)), Some((t1, y1))) = (wf.marquee_start, wf.marquee_current) {
        let x0 = time_to_x(t0.min(t1)).clamp(wave_rect.left(), wave_rect.right());
        let x1 = time_to_x(t0.max(t1)).clamp(wave_rect.left(), wave_rect.right());
        let top_y = y0.min(y1).clamp(wave_rect.top(), wave_rect.bottom());
        let bot_y = y0.max(y1).clamp(wave_rect.top(), wave_rect.bottom());

        if (x1 - x0) >= 2.0 {
            // Full-height timeline range highlight band so user clearly sees which vertical mark lines are inside
            let band_rect = Rect::from_min_max(
                Pos2::new(x0, wave_rect.top()),
                Pos2::new(x1, wave_rect.bottom()),
            );
            painter.rect_filled(band_rect, 0.0, brand::primary_tint(20));

            // Crisp dragged selection box
            let box_rect = Rect::from_min_max(
                Pos2::new(x0, top_y.min(bot_y - 4.0).max(wave_rect.top())),
                Pos2::new(x1, bot_y.max(top_y + 4.0).min(wave_rect.bottom())),
            );
            painter.rect_filled(box_rect, 3.0, brand::primary_tint(42));
            painter.rect_stroke(
                box_rect,
                3.0,
                Stroke::new(1.3, brand::HIGHLIGHT),
                StrokeKind::Inside,
            );

            // Live selected count badge
            let count = wf.selected_count();
            let badge_text = if count == 1 {
                "1 mark".to_string()
            } else {
                format!("{count} marks")
            };
            let galley =
                painter.layout_no_wrap(badge_text, brand::font_mono_bold(10.5), brand::TEXT);
            let pill_w = galley.size().x + 12.0;
            let pill_h = 20.0;
            let pill_x = (x1 + 6.0)
                .min(wave_rect.right() - pill_w - 6.0)
                .max(wave_rect.left() + 6.0);
            let pill_y = (top_y - pill_h - 4.0)
                .max(wave_rect.top() + 6.0)
                .min(wave_rect.bottom() - pill_h - 6.0);
            let pill_rect =
                Rect::from_min_size(Pos2::new(pill_x, pill_y), Vec2::new(pill_w, pill_h));
            painter.rect_filled(pill_rect, 5.0, brand::PANEL);
            painter.rect_stroke(
                pill_rect,
                5.0,
                Stroke::new(1.0, brand::PRIMARY),
                StrokeKind::Inside,
            );
            painter.galley(
                Pos2::new(
                    pill_rect.left() + 6.0,
                    pill_rect.center().y - galley.size().y * 0.5,
                ),
                galley,
                brand::LAVENDER,
            );
        }
    }

    rect.width()
}

#[allow(clippy::too_many_arguments)]
fn handle_waveform_interactions(
    ui: &mut Ui,
    resp: &Response,
    ruler_rect: Rect,
    wave_rect: Rect,
    track: &mut TrackAnalysis,
    export: &ExportSettings,
    shortcuts: &KeyBindings,
    wf: &mut WaveformState,
    hovered_mark_id: Option<u64>,
    hit_mark_at: &impl Fn(Pos2, f32) -> Option<(u64, f32, f32)>,
    frame_sec: f64,
    x_to_time: &impl Fn(f32) -> f64,
    actions: &mut WaveformActions,
) {
    let style = &export.style;
    let undo_lbl = shortcuts.short_label(ShortcutAction::Undo);
    let (modifiers, press_origin) = ui.input(|i| (i.modifiers, i.pointer.press_origin()));

    // Middle-mouse drag pans viewport
    if resp.dragged_by(egui::PointerButton::Middle) {
        let dx = resp.drag_delta().x;
        wf.view_start_sec -= (dx / wf.pixels_per_sec) as f64;
        return;
    }

    // Left-button drag start:
    //  1) On the top ruler -> scrub playhead
    //  2) On an already-selected marker (without Shift), OR on a well-spaced unselected marker (zoomed in, without Shift) -> drag marker(s)
    //  3) Anywhere else on the waveform body (including dense zoomed-out markers, or whenever Shift is held) -> drag-to-select marquee box!
    if resp.drag_started_by(egui::PointerButton::Primary) {
        if let Some(pos) = resp.interact_pointer_pos() {
            let origin = press_origin.unwrap_or(pos);
            if ruler_rect.contains(origin) {
                wf.scrubbing_ruler = true;
                let t = x_to_time(pos.x).clamp(0.0, track.duration_sec);
                actions.seek_to = Some(t);
            } else {
                let drag_mark_hit = if modifiers.shift {
                    None
                } else {
                    hit_mark_at(origin, 7.0).and_then(|(id, dist, neighbor_dx)| {
                        if wf.is_selected(id) || (neighbor_dx >= 20.0 && dist <= 5.0) {
                            Some(id)
                        } else {
                            None
                        }
                    })
                };

                if let Some(mid) = drag_mark_hit {
                    if !wf.is_selected(mid) {
                        wf.select_single(mid);
                    }
                    wf.dragging_mark_id = Some(mid);
                    wf.drag_origin_sec = track
                        .marks
                        .iter()
                        .find(|m| m.id == mid)
                        .map(|m| m.time_sec)
                        .unwrap_or(0.0);
                    wf.drag_origin_display_sec = export
                        .quantized_mark_time_sec(wf.drag_origin_sec, track.in_point_sec)
                        .clamp(0.0, track.duration_sec);
                    wf.drag_origin_times = track
                        .marks
                        .iter()
                        .filter(|m| wf.is_selected(m.id))
                        .map(|m| (m.id, m.time_sec))
                        .collect();
                    wf.drag_last_applied_delta = 0.0;
                    wf.drag_snapshot_pushed = false;
                } else {
                    let start_t = x_to_time(origin.x).clamp(0.0, track.duration_sec);
                    let start_y = origin.y.clamp(wave_rect.top(), wave_rect.bottom());
                    let cur_t = x_to_time(pos.x).clamp(0.0, track.duration_sec);
                    let cur_y = pos.y.clamp(wave_rect.top(), wave_rect.bottom());
                    wf.marquee_base_selection = if modifiers.shift {
                        wf.selected_mark_ids.clone()
                    } else {
                        HashSet::new()
                    };
                    wf.marquee_start = Some((start_t, start_y));
                    wf.marquee_current = Some((cur_t, cur_y));
                }
            }
        }
    }

    if let Some(drag_id) = wf.dragging_mark_id {
        if resp.dragged_by(egui::PointerButton::Primary) {
            if let Some(pos) = resp.interact_pointer_pos() {
                let raw_t = x_to_time(pos.x).clamp(0.0, track.duration_sec);
                let raw_delta = raw_t - wf.drag_origin_display_sec;
                let snapped_delta = if modifiers.alt {
                    raw_delta
                } else {
                    (raw_delta / frame_sec).round() * frame_sec
                };
                let min_origin = wf
                    .drag_origin_times
                    .iter()
                    .map(|&(_, t)| t)
                    .fold(f64::INFINITY, f64::min);
                let max_origin = wf
                    .drag_origin_times
                    .iter()
                    .map(|&(_, t)| t)
                    .fold(f64::NEG_INFINITY, f64::max);
                let clamped_delta = if min_origin.is_finite() && max_origin.is_finite() {
                    snapped_delta.clamp(-min_origin, (track.duration_sec - max_origin).max(0.0))
                } else {
                    0.0
                };

                // Only push an Undo snapshot and rebuild clicks when the snapped position actually moves!
                if (clamped_delta - wf.drag_last_applied_delta).abs() > 1e-9 {
                    if !wf.drag_snapshot_pushed {
                        actions.undo_snapshot = Some(track.clone());
                        wf.drag_snapshot_pushed = true;
                    }
                    if wf.drag_origin_times.len() <= 1 {
                        let new_t = (wf.drag_origin_sec + clamped_delta).clamp(0.0, track.duration_sec);
                        editing::move_mark(track, drag_id, new_t);
                    } else {
                        let updates: Vec<(u64, f64)> = wf
                            .drag_origin_times
                            .iter()
                            .map(|&(id, orig_t)| (id, orig_t + clamped_delta))
                            .collect();
                        editing::move_marks(track, &updates);
                    }
                    wf.drag_last_applied_delta = clamped_delta;
                    actions.marks_mutated = true;
                }
            }
        }
        if resp.drag_stopped()
            || (!resp.dragged_by(egui::PointerButton::Primary)
                && !resp.drag_started_by(egui::PointerButton::Primary))
        {
            wf.dragging_mark_id = None;
            wf.drag_origin_times.clear();
            wf.drag_last_applied_delta = 0.0;
            wf.drag_snapshot_pushed = false;
        }
    } else if let Some((start_t, _)) = wf.marquee_start {
        if resp.dragged_by(egui::PointerButton::Primary) {
            if let Some(pos) = resp.interact_pointer_pos() {
                let cur_t = x_to_time(pos.x).clamp(0.0, track.duration_sec);
                let cur_y = pos.y.clamp(wave_rect.top(), wave_rect.bottom());
                wf.marquee_current = Some((cur_t, cur_y));
                let t_min = start_t.min(cur_t);
                let t_max = start_t.max(cur_t);
                let mut next_sel = wf.marquee_base_selection.clone();
                for m in &track.marks {
                    let disp_t = export
                        .quantized_mark_time_sec(m.time_sec, track.in_point_sec)
                        .clamp(0.0, track.duration_sec);
                    if style.is_mark_included(m) && disp_t >= t_min && disp_t <= t_max {
                        next_sel.insert(m.id);
                    }
                }
                wf.selected_mark_ids = next_sel;
            }
        }
        if resp.drag_stopped()
            || (!resp.dragged_by(egui::PointerButton::Primary)
                && !resp.drag_started_by(egui::PointerButton::Primary))
        {
            wf.marquee_start = None;
            wf.marquee_current = None;
            wf.marquee_base_selection.clear();
        }
    } else if wf.scrubbing_ruler {
        if resp.dragged_by(egui::PointerButton::Primary) {
            if let Some(pos) = resp.interact_pointer_pos() {
                let t = x_to_time(pos.x).clamp(0.0, track.duration_sec);
                actions.seek_to = Some(t);
            }
        }
        if resp.drag_stopped()
            || (!resp.dragged_by(egui::PointerButton::Primary)
                && !resp.drag_started_by(egui::PointerButton::Primary))
        {
            wf.scrubbing_ruler = false;
        }
    } else if resp.clicked_by(egui::PointerButton::Primary) {
        if let Some(pos) = resp.interact_pointer_pos() {
            let t = x_to_time(pos.x).clamp(0.0, track.duration_sec);
            if modifiers.ctrl || modifiers.command {
                let kind = if modifiers.shift {
                    MarkKind::Downbeat
                } else {
                    MarkKind::RegularBeat
                };
                let snapped_t =
                    step_time_by_frames(t, 0, export.frame_rate, track.duration_sec);
                actions.undo_snapshot = Some(track.clone());
                let new_id = editing::add_mark(track, snapped_t, kind);
                wf.select_single(new_id);
                actions.enable_mark_kind = Some(kind);
                actions.marks_mutated = true;
            } else if let Some(mid) = hovered_mark_id {
                if modifiers.shift {
                    wf.toggle_selected(mid);
                } else {
                    wf.select_single(mid);
                    actions.seek_to = Some(t);
                }
            } else {
                if !modifiers.shift {
                    wf.clear_selection();
                }
                actions.seek_to = Some(t);
            }
        }
    }

    // Right-click context menu on mark(s) or timeline position (persists context_time_sec across frames!)
    if resp.secondary_clicked() {
        wf.context_mark_id = hovered_mark_id;
        wf.context_time_sec = resp
            .interact_pointer_pos()
            .map(|p| x_to_time(p.x).clamp(0.0, track.duration_sec));
        if let Some(mid) = hovered_mark_id {
            if !wf.is_selected(mid) {
                wf.select_single(mid);
            }
            if wf.selected_count() == 1 {
                wf.label_edit_buf = track
                    .marks
                    .iter()
                    .find(|m| m.id == mid)
                    .and_then(|m| m.custom_label.clone())
                    .unwrap_or_default();
            }
        }
    }

    resp.context_menu(|ui| {
        if let Some(mid) = wf.context_mark_id {
            let sel_count = wf.selected_count();
            if sel_count > 1 && wf.is_selected(mid) {
                // Multi-selection context menu
                ui.label(
                    egui::RichText::new(format!("Edit {sel_count} Selected Marks"))
                        .font(brand::font_ui_semibold(12.5))
                        .color(brand::TEXT),
                );
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("◀ -1 Frame").clicked() {
                        nudge_selection_in_menu(track, &wf.selected_mark_ids, -frame_sec, actions);
                    }
                    if ui.button("+1 Frame ▶").clicked() {
                        nudge_selection_in_menu(track, &wf.selected_mark_ids, frame_sec, actions);
                    }
                });
                ui.separator();
                for kind in MarkKind::ALL {
                    if ui
                        .button(format!("Set All as {}", kind.display_name()))
                        .clicked()
                    {
                        actions.undo_snapshot = Some(track.clone());
                        let changed = editing::reclassify_marks(track, &wf.selected_mark_ids, kind);
                        actions.enable_mark_kind = Some(kind);
                        actions.marks_mutated = true;
                        actions.status_hint = Some(format!(
                            "Set {changed} selected marks to {}",
                            kind.short_name()
                        ));
                        ui.close_menu();
                    }
                }
                ui.separator();
                if ui.button("Clear Selection").clicked() {
                    wf.clear_selection();
                    ui.close_menu();
                }
                if ui
                    .button(
                        egui::RichText::new(format!("Delete {sel_count} Marks"))
                            .color(brand::ERROR),
                    )
                    .clicked()
                {
                    actions.undo_snapshot = Some(track.clone());
                    let removed = editing::delete_marks(track, &wf.selected_mark_ids);
                    wf.clear_selection();
                    wf.context_mark_id = None;
                    actions.marks_mutated = true;
                    actions.status_hint = Some(format!("Deleted {removed} selected marks"));
                    ui.close_menu();
                }
            } else {
                // Single-mark context menu
                ui.label(
                    egui::RichText::new("Edit Beat Mark")
                        .font(brand::font_ui_semibold(12.5))
                        .color(brand::TEXT),
                );
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("◀ -1 Frame").clicked() {
                        if let Some(cur_t) =
                            track.marks.iter().find(|m| m.id == mid).map(|m| m.time_sec)
                        {
                            let snapped = step_time_by_frames(
                                cur_t,
                                -1,
                                export.frame_rate,
                                track.duration_sec,
                            );
                            if (snapped - cur_t).abs() > 1e-9 {
                                actions.undo_snapshot = Some(track.clone());
                                editing::move_mark(track, mid, snapped);
                                actions.marks_mutated = true;
                            }
                        }
                    }
                    if ui.button("+1 Frame ▶").clicked() {
                        if let Some(cur_t) =
                            track.marks.iter().find(|m| m.id == mid).map(|m| m.time_sec)
                        {
                            let snapped = step_time_by_frames(
                                cur_t,
                                1,
                                export.frame_rate,
                                track.duration_sec,
                            );
                            if (snapped - cur_t).abs() > 1e-9 {
                                actions.undo_snapshot = Some(track.clone());
                                editing::move_mark(track, mid, snapped);
                                actions.marks_mutated = true;
                            }
                        }
                    }
                });
                ui.separator();
                for kind in MarkKind::ALL {
                    if ui.button(format!("Set as {}", kind.display_name())).clicked() {
                        actions.undo_snapshot = Some(track.clone());
                        editing::reclassify_mark(track, mid, kind);
                        actions.enable_mark_kind = Some(kind);
                        actions.marks_mutated = true;
                        ui.close_menu();
                    }
                }
                ui.separator();
                ui.label(
                    egui::RichText::new("Custom Marker Label")
                        .size(11.5)
                        .color(brand::MUTED),
                );
                ui.horizontal(|ui| {
                    ui.text_edit_singleline(&mut wf.label_edit_buf);
                    if ui.button("Apply").clicked() {
                        actions.undo_snapshot = Some(track.clone());
                        let lbl = if wf.label_edit_buf.trim().is_empty() {
                            None
                        } else {
                            Some(wf.label_edit_buf.trim().to_string())
                        };
                        editing::set_mark_label(track, mid, lbl);
                        actions.marks_mutated = true;
                        ui.close_menu();
                    }
                });
                ui.separator();
                ui.label(
                    egui::RichText::new("Re-Lock Grid From Here")
                        .size(11.0)
                        .color(brand::HIGHLIGHT),
                );
                if ui
                    .button("Anchor as Beat 1 (Downbeat) -> End")
                    .on_hover_text("Keeps earlier marks untouched and re-locks all beats from here to the end starting on Beat 1")
                    .clicked()
                {
                    if let Some(cur_t) =
                        track.marks.iter().find(|m| m.id == mid).map(|m| m.time_sec)
                    {
                        actions.undo_snapshot = Some(track.clone());
                        editing::relock_grid_from_time(track, cur_t, 1);
                        wf.clear_selection();
                        actions.marks_mutated = true;
                        actions.status_hint = Some(format!(
                            "Re-locked grid from marker as Beat 1 ({undo_lbl} to undo)"
                        ));
                        ui.close_menu();
                    }
                }
                if ui
                    .button("Anchor as Beat 2 (Clap / Backbeat) -> End")
                    .on_hover_text("Keeps earlier marks untouched and re-locks all beats from here to the end starting on Beat 2")
                    .clicked()
                {
                    if let Some(cur_t) =
                        track.marks.iter().find(|m| m.id == mid).map(|m| m.time_sec)
                    {
                        actions.undo_snapshot = Some(track.clone());
                        editing::relock_grid_from_time(track, cur_t, 2);
                        wf.clear_selection();
                        actions.marks_mutated = true;
                        actions.status_hint = Some(format!(
                            "Re-locked grid from marker as Beat 2 / Clap ({undo_lbl} to undo)"
                        ));
                        ui.close_menu();
                    }
                }
                ui.separator();
                if ui
                    .button(egui::RichText::new("Delete Mark").color(brand::ERROR))
                    .clicked()
                {
                    actions.undo_snapshot = Some(track.clone());
                    editing::delete_mark(track, mid);
                    wf.clear_selection();
                    wf.context_mark_id = None;
                    actions.marks_mutated = true;
                    ui.close_menu();
                }
            }
        } else if let Some(t) = wf.context_time_sec {
            ui.label(
                egui::RichText::new(format!("Timeline @ {:.2}s", t))
                    .font(brand::font_mono_bold(12.0))
                    .color(brand::HIGHLIGHT),
            );
            ui.separator();
            for kind in MarkKind::ALL {
                if ui.button(format!("Add {} Here", kind.short_name())).clicked() {
                    let snapped_t =
                        step_time_by_frames(t, 0, export.frame_rate, track.duration_sec);
                    actions.undo_snapshot = Some(track.clone());
                    let id = editing::add_mark(track, snapped_t, kind);
                    wf.select_single(id);
                    actions.enable_mark_kind = Some(kind);
                    actions.marks_mutated = true;
                    ui.close_menu();
                }
            }
            ui.separator();
            if ui.button("Select All Visible Marks").clicked() {
                wf.selected_mark_ids = track
                    .marks
                    .iter()
                    .filter(|m| style.is_mark_included(m))
                    .map(|m| m.id)
                    .collect();
                ui.close_menu();
            }
            ui.separator();
            ui.label(
                egui::RichText::new("Re-Lock Grid From Here")
                    .size(11.0)
                    .color(brand::HIGHLIGHT),
            );
            if ui
                .button("Anchor as Beat 1 (Downbeat) -> End")
                .on_hover_text("Keeps earlier marks untouched and re-locks all beats from here to the end starting on Beat 1")
                .clicked()
            {
                let snapped_t = step_time_by_frames(t, 0, export.frame_rate, track.duration_sec);
                actions.undo_snapshot = Some(track.clone());
                editing::relock_grid_from_time(track, snapped_t, 1);
                wf.clear_selection();
                actions.marks_mutated = true;
                actions.status_hint = Some(format!(
                    "Re-locked grid from here as Beat 1 ({undo_lbl} to undo)"
                ));
                ui.close_menu();
            }
            if ui
                .button("Anchor as Beat 2 (Clap / Backbeat) -> End")
                .on_hover_text("Keeps earlier marks untouched and re-locks all beats from here to the end starting on Beat 2")
                .clicked()
            {
                let snapped_t = step_time_by_frames(t, 0, export.frame_rate, track.duration_sec);
                actions.undo_snapshot = Some(track.clone());
                editing::relock_grid_from_time(track, snapped_t, 2);
                wf.clear_selection();
                actions.marks_mutated = true;
                actions.status_hint = Some(format!(
                    "Re-locked grid from here as Beat 2 / Clap ({undo_lbl} to undo)"
                ));
                ui.close_menu();
            }
            ui.separator();
            if ui.button("Set Export In Point Here").clicked() {
                actions.undo_snapshot = Some(track.clone());
                track.in_point_sec = t.min(track.out_point_sec - 0.5).max(0.0);
                actions.in_out_changed = true;
                ui.close_menu();
            }
            if ui.button("Set Export Out Point Here").clicked() {
                actions.undo_snapshot = Some(track.clone());
                track.out_point_sec = t.max(track.in_point_sec + 0.5).min(track.duration_sec);
                actions.in_out_changed = true;
                ui.close_menu();
            }
        }
    });
}

fn nudge_selection_in_menu(
    track: &mut TrackAnalysis,
    selected_ids: &HashSet<u64>,
    delta_sec: f64,
    actions: &mut WaveformActions,
) {
    let selected_times: Vec<(u64, f64)> = track
        .marks
        .iter()
        .filter(|m| selected_ids.contains(&m.id))
        .map(|m| (m.id, m.time_sec))
        .collect();
    if selected_times.is_empty() {
        return;
    }
    let min_t = selected_times
        .iter()
        .map(|&(_, t)| t)
        .fold(f64::INFINITY, f64::min);
    let max_t = selected_times
        .iter()
        .map(|&(_, t)| t)
        .fold(f64::NEG_INFINITY, f64::max);
    let clamped_delta = delta_sec.clamp(-min_t, (track.duration_sec - max_t).max(0.0));
    if clamped_delta.abs() < 1e-9 {
        return;
    }
    actions.undo_snapshot = Some(track.clone());
    let updates: Vec<(u64, f64)> = selected_times
        .into_iter()
        .map(|(id, t)| (id, t + clamped_delta))
        .collect();
    editing::move_marks(track, &updates);
    actions.marks_mutated = true;
}

fn dj_rgb_color(low: f32, mid: f32, high: f32, alpha: u8) -> Color32 {
    let r = ((low * 245.0 + mid * 45.0 + 25.0).min(255.0)) as u8;
    let g = ((mid * 225.0 + high * 120.0 + low * 40.0 + 20.0).min(255.0)) as u8;
    let b = ((high * 255.0 + mid * 95.0 + 35.0).min(255.0)) as u8;
    Color32::from_rgba_unmultiplied(r, g, b, alpha)
}
