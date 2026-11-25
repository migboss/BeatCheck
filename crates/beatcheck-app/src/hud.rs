use crate::brand::{self, StrokeIcon};
use crate::config::{
    format_live_smpte, format_mm_ss, mark_color32, resolve_color32, ScrollMode, WaveformVisualMode,
};
use beatcheck_audio::AudioPlayer;
use beatcheck_core::model::{GridMode, MarkExportStyle, MarkKind, TimeSignature, TrackAnalysis};
use beatcheck_edl::ExportSettings;
use egui::{
    Align, Color32, CornerRadius, Frame, Layout, Margin, RichText, Sense, Stroke, StrokeKind, Ui,
    Vec2,
};

#[derive(Debug, Default)]
pub struct HudActions {
    pub open_file_dialog: bool,
    pub bpm_multiplier: Option<f64>,
    pub shift_downbeat: Option<i32>,
    pub set_time_sig: Option<TimeSignature>,
    pub set_grid_mode: Option<GridMode>,
    pub set_extrapolate: Option<bool>,
    pub nudge_sec: Option<f64>,
    pub nudge_selected_frames: Option<i32>,
    pub add_mark_at_playhead: Option<MarkKind>,
    pub delete_selected_mark: bool,
    pub reset_to_auto: bool,
    pub mark_filter_changed: bool,
    pub tap_tempo: bool,
    pub set_drop_sensitivity: Option<f32>,
    pub drop_sensitivity_released: bool,
    pub undo: bool,
    pub redo: bool,
    pub zoom_factor: Option<f32>,
    pub zoom_fit: bool,
    pub rewind_to_start: bool,
    pub jump_downbeat: Option<i32>,
    pub toggle_drawer: bool,
}

/// Clean, overlap-free Top Brand HUD.
/// FPS & Start Timecode live cleanly in the always-open Right Resolve Inspector.
#[allow(clippy::too_many_arguments)]
pub fn draw_top_hud(
    ui: &mut Ui,
    track: Option<&mut TrackAnalysis>,
    export: &ExportSettings,
    playhead_sec: f64,
    is_playing: bool,
    drawer_open: bool,
    resolved_count: usize,
    actions: &mut HudActions,
) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(9.0, 5.0);

        // Brand Identity Lockup (Retina Pulse Check + Geometric Wordmark)
        brand::draw_pulse_check_logo(ui, 28.0);
        brand::draw_wordmark(ui, 18.0);

        ui.add_space(2.0);
        if brand::stroke_button(ui, Some(StrokeIcon::Folder), "Open", false, 28.0)
            .on_hover_text("Import MP3, WAV, FLAC, OGG, or M4A (Ctrl+O) — or drag & drop anywhere")
            .clicked()
        {
            actions.open_file_dialog = true;
        }

        let Some(track) = track else {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                let inspector_btn = brand::stroke_button(
                    ui,
                    Some(if drawer_open {
                        StrokeIcon::DrawerClose
                    } else {
                        StrokeIcon::DrawerOpen
                    }),
                    if drawer_open { "Hide Inspector" } else { "Inspector" },
                    drawer_open,
                    28.0,
                );
                if inspector_btn
                    .on_hover_text("Show or hide the Timeline Inspector panel (E)")
                    .clicked()
                {
                    actions.toggle_drawer = true;
                }
                ui.label(
                    RichText::new(brand::BRAND_TAGLINE)
                        .font(brand::font_display_medium(12.5))
                        .color(brand::MUTED),
                );
            });
            return;
        };

        draw_hairline_sep(ui);

        // Segmented BPM Pill (÷2 | 128.0 BPM | ×2) + Tap + Time Sig
        Frame::new()
            .fill(brand::CANVAS)
            .stroke(Stroke::new(1.0, brand::BORDER))
            .corner_radius(CornerRadius::same(7))
            .inner_margin(Margin::symmetric(4, 2))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                if ui
                    .add(
                        egui::Button::new(
                            RichText::new("÷2")
                                .font(brand::font_mono_bold(11.0))
                                .color(brand::MUTED),
                        )
                        .frame(false),
                    )
                    .on_hover_text("Halve tempo (fix double-time detection)")
                    .clicked()
                {
                    actions.bpm_multiplier = Some(0.5);
                }

                Frame::new()
                    .fill(brand::primary_tint(42))
                    .corner_radius(CornerRadius::same(5))
                    .inner_margin(Margin::symmetric(8, 2))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(format!("{:.1} BPM", track.active_bpm))
                                .font(brand::font_mono_bold(12.0))
                                .color(brand::LAVENDER),
                        );
                    });

                if ui
                    .add(
                        egui::Button::new(
                            RichText::new("×2")
                                .font(brand::font_mono_bold(11.0))
                                .color(brand::MUTED),
                        )
                        .frame(false),
                    )
                    .on_hover_text("Double tempo (fix half-time detection)")
                    .clicked()
                {
                    actions.bpm_multiplier = Some(2.0);
                }
            });

        if brand::stroke_button(ui, None, "Tap", false, 26.0)
            .on_hover_text("Tap tempo repeatedly with the music (shortcut: T)")
            .clicked()
        {
            actions.tap_tempo = true;
        }

        let mut selected_ts = track.active_time_sig;
        egui::ComboBox::from_id_salt("time_sig_combo")
            .width(50.0)
            .selected_text(
                RichText::new(selected_ts.label())
                    .font(brand::font_mono_bold(11.5))
                    .color(brand::TEXT),
            )
            .show_ui(ui, |ui| {
                for ts in TimeSignature::ALL {
                    ui.selectable_value(&mut selected_ts, ts, ts.label());
                }
            });
        if selected_ts != track.active_time_sig {
            actions.set_time_sig = Some(selected_ts);
        }

        draw_hairline_sep(ui);

        // Live Flashing Beat Pips [1][2][3][4] + Bar:Beat Readout
        draw_live_beat_pips(ui, track, &export.style, playhead_sec, is_playing);

        // Right-aligned ONLY when cursor genuinely has >= 330px remaining on this line!
        let remaining_x = (ui.max_rect().right() - ui.cursor().left()).max(0.0);
        if remaining_x >= 330.0 {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                draw_top_hud_right(ui, track, export, playhead_sec, drawer_open, resolved_count, actions, true);
            });
        } else {
            draw_hairline_sep(ui);
            draw_top_hud_right(ui, track, export, playhead_sec, drawer_open, resolved_count, actions, false);
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn draw_top_hud_right(
    ui: &mut Ui,
    track: &TrackAnalysis,
    export: &ExportSettings,
    playhead_sec: f64,
    drawer_open: bool,
    resolved_count: usize,
    actions: &mut HudActions,
    rtl: bool,
) {
    ui.spacing_mut().item_spacing.x = 8.0;
    let smpte = format_live_smpte(export.frame_rate, &export.start_timecode, playhead_sec);
    let dur_str = format!(
        "{} / {}",
        format_mm_ss(playhead_sec),
        format_mm_ss(track.duration_sec)
    );
    let drawer_label = if drawer_open {
        "Hide".to_string()
    } else {
        format!("Inspector ({resolved_count})")
    };

    let draw_toggle = |ui: &mut Ui, actions: &mut HudActions| {
        let drawer_btn = if drawer_open {
            brand::stroke_button(ui, Some(StrokeIcon::DrawerClose), &drawer_label, false, 28.0)
        } else {
            brand::primary_cta_button(
                ui,
                Some(StrokeIcon::DrawerOpen),
                &drawer_label,
                Vec2::new(134.0, 28.0),
            )
        };
        if drawer_btn
            .on_hover_text("Show or hide the Timeline Inspector panel (E)")
            .clicked()
        {
            actions.toggle_drawer = true;
        }
    };

    if rtl {
        draw_toggle(ui, actions);
        draw_hairline_sep(ui);
        ui.label(
            RichText::new(dur_str)
                .font(egui::FontId::monospace(11.5))
                .color(brand::MUTED),
        );
        ui.label(
            RichText::new(smpte)
                .font(brand::font_mono_bold(15.0))
                .color(brand::LAVENDER),
        );
        ui.label(
            RichText::new(export.frame_rate.label())
                .font(brand::font_mono_bold(11.0))
                .color(brand::HIGHLIGHT),
        );
    } else {
        ui.label(
            RichText::new(export.frame_rate.label())
                .font(brand::font_mono_bold(11.0))
                .color(brand::HIGHLIGHT),
        );
        ui.label(
            RichText::new(smpte)
                .font(brand::font_mono_bold(14.5))
                .color(brand::LAVENDER),
        );
        draw_toggle(ui, actions);
    }
}

fn draw_hairline_sep(ui: &mut Ui) {
    let (r, _) = ui.allocate_exact_size(Vec2::new(1.0, 20.0), egui::Sense::hover());
    ui.painter().line_segment(
        [
            egui::Pos2::new(r.center().x, r.top()),
            egui::Pos2::new(r.center().x, r.bottom()),
        ],
        Stroke::new(1.0, brand::BORDER),
    );
}

fn draw_live_beat_pips(
    ui: &mut Ui,
    track: &TrackAnalysis,
    style: &MarkExportStyle,
    playhead_sec: f64,
    is_playing: bool,
) {
    let current = track
        .marks
        .iter()
        .filter(|m| m.kind != MarkKind::EnergyDrop && m.time_sec <= playhead_sec + 0.01)
        .next_back();

    let (cur_bar, cur_beat, dt_since_hit) = match current {
        Some(m) => (m.bar, m.beat_in_bar, (playhead_sec - m.time_sec).max(0.0)),
        None => (1, 0, 1.0),
    };

    let bpb = track.active_time_sig.beats_per_bar();
    ui.label(
        RichText::new(format!("B{:03}:{}", cur_bar, cur_beat.max(1)))
            .font(brand::font_mono_bold(11.5))
            .color(brand::MUTED),
    );

    ui.spacing_mut().item_spacing.x = 3.0;
    for b in 1..=bpb {
        let kind = if b == 1 {
            MarkKind::Downbeat
        } else if track.active_time_sig.is_strong_beat(b) {
            MarkKind::StrongBeat
        } else {
            MarkKind::RegularBeat
        };
        let enabled = style.style_for(kind).enabled;
        let base_col = mark_color32(style, kind);
        let is_active = b == cur_beat && enabled;
        let pulse = if is_active && is_playing && dt_since_hit < 0.16 {
            1.0 - (dt_since_hit / 0.16) as f32
        } else if is_active {
            0.45
        } else {
            0.0
        };

        let fill = if is_active {
            let a = (165.0 + pulse * 90.0) as u8;
            Color32::from_rgba_unmultiplied(base_col.r(), base_col.g(), base_col.b(), a)
        } else {
            brand::SURFACE
        };

        let (pip_rect, _) = ui.allocate_exact_size(Vec2::new(20.0, 20.0), Sense::hover());
        let p = ui.painter();
        p.rect_filled(pip_rect, 4.5, fill);
        p.rect_stroke(
            pip_rect,
            4.5,
            Stroke::new(
                1.0,
                if is_active {
                    brand::TEXT
                } else if enabled {
                    Color32::from_rgba_unmultiplied(base_col.r(), base_col.g(), base_col.b(), 105)
                } else {
                    brand::BORDER
                },
            ),
            StrokeKind::Inside,
        );
        p.text(
            pip_rect.center(),
            egui::Align2::CENTER_CENTER,
            b.to_string(),
            brand::font_mono_bold(10.5),
            if is_active {
                brand::TEXT
            } else if enabled {
                brand::MUTED
            } else {
                Color32::from_rgb(72, 68, 92)
            },
        );
    }
}

/// Main Waveform Utility Bar (100% overlap-free at every window size!):
/// - Row 1: Beat-Type Filter / Solo Pills + Undo / Redo / Reset + View & Zoom controls (auto-wraps cleanly!)
/// - Row 2 (contextual): Dedicated Selected Marker 1-Frame Nudge Bar when a marker is selected
#[allow(clippy::too_many_arguments)]
pub fn draw_waveform_toolbar(
    ui: &mut Ui,
    track: &mut TrackAnalysis,
    export: &mut ExportSettings,
    selected_mark_id: Option<u64>,
    visual_mode: &mut WaveformVisualMode,
    scroll_mode: &mut ScrollMode,
    can_undo: bool,
    can_redo: bool,
    can_reset: bool,
    actions: &mut HudActions,
) {
    // Row 1: Always-clean wrapping bar (never uses overlapping right_to_left when narrow!)
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(6.0, 6.0);

        // 1. Instant 1-Click Beat-Type Visibility & Export Filters
        for kind in MarkKind::ALL {
            let count = track
                .marks
                .iter()
                .filter(|m| m.kind == kind && export.style.is_mark_included(m))
                .count();
            let total_kind = track.marks.iter().filter(|m| m.kind == kind).count();
            let cfg = export.style.style_for(kind);
            let enabled = cfg.enabled;
            let dot_col = resolve_color32(cfg.color);

            let label = format!(
                "{} ({})",
                kind.short_name(),
                if enabled { count } else { total_kind }
            );
            let resp = draw_beat_filter_pill(ui, &label, dot_col, enabled).on_hover_text(format!(
                "Click to show/hide {} on screen & in exported EDL\nShift+Click or Right-click to SOLO only {}",
                kind.display_name(),
                kind.display_name()
            ));

            let shift = ui.input(|i| i.modifiers.shift);
            if resp.clicked() && !shift {
                export.style.style_for_mut(kind).enabled = !enabled;
                actions.mark_filter_changed = true;
            } else if (resp.clicked() && shift) || resp.secondary_clicked() {
                let already_solo = MarkKind::ALL
                    .iter()
                    .all(|&k| export.style.style_for(k).enabled == (k == kind));
                for k in MarkKind::ALL {
                    export.style.style_for_mut(k).enabled =
                        if already_solo { true } else { k == kind };
                }
                actions.mark_filter_changed = true;
            }
        }

        draw_hairline_sep(ui);

        // 2. Undo / Redo / Reset to Algorithm
        ui.add_enabled_ui(can_undo, |ui| {
            if brand::stroke_button(ui, Some(StrokeIcon::Undo), "", false, 26.0)
                .on_hover_text("Undo last edit (Ctrl+Z)")
                .clicked()
            {
                actions.undo = true;
            }
        });
        ui.add_enabled_ui(can_redo, |ui| {
            if brand::stroke_button(ui, Some(StrokeIcon::Redo), "", false, 26.0)
                .on_hover_text("Redo (Ctrl+Y)")
                .clicked()
            {
                actions.redo = true;
            }
        });
        ui.add_enabled_ui(can_reset, |ui| {
            if brand::stroke_button(ui, Some(StrokeIcon::Reset), "Reset", false, 26.0)
                .on_hover_text(
                    "Reset all manual changes back to what the algorithm originally marked (Ctrl+Z undoable)",
                )
                .clicked()
            {
                actions.reset_to_auto = true;
            }
        });

        // Quick "+ Add" Marker Menu at Playhead (1-click for any of the 4 beat types!)
        ui.menu_button(
            RichText::new("+ Add Mark")
                .font(brand::font_ui_semibold(11.5))
                .color(brand::LAVENDER),
            |ui| {
                ui.label(
                    RichText::new("Insert at Playhead (Frame-Snapped)")
                        .font(brand::font_mono_bold(10.5))
                        .color(brand::HIGHLIGHT),
                );
                ui.separator();
                for (idx, kind) in MarkKind::ALL.into_iter().enumerate() {
                    let dot_col = resolve_color32(export.style.style_for(kind).color);
                    let key_num = idx + 1;
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(Vec2::splat(9.0), Sense::hover());
                        ui.painter().circle_filled(r.center(), 4.5, dot_col);
                        if ui
                            .button(format!("{}  (Key {})", kind.display_name(), key_num))
                            .clicked()
                        {
                            actions.add_mark_at_playhead = Some(kind);
                            ui.close_menu();
                        }
                    });
                }
            },
        )
        .response
        .on_hover_text("Drop any beat marker type at the playhead (Shortcuts: keys 1, 2, 3, 4)");

        // 3. View & Zoom group: pure left-to-right flow inside horizontal_wrapped -> 100% IMPOSSIBLE to overlap!
        draw_hairline_sep(ui);

        let locked = track.grid_mode == GridMode::LockedConstantBpm;
        let is_tape = *scroll_mode == ScrollMode::CenteredTape;

        if brand::stroke_button(
            ui,
            Some(StrokeIcon::Sliders),
            if locked { "Locked" } else { "Dynamic" },
            locked,
            26.0,
        )
        .on_hover_text("Toggle Locked Constant BPM vs Dynamic beat tracking")
        .clicked()
        {
            actions.set_grid_mode = Some(if locked {
                GridMode::Dynamic
            } else {
                GridMode::LockedConstantBpm
            });
        }
        if brand::stroke_button(
            ui,
            Some(StrokeIcon::Waveform),
            visual_mode.label(),
            true,
            26.0,
        )
        .on_hover_text(format!(
            "{}\nClick to cycle: Outline -> HD Solid -> RGB Bands",
            visual_mode.description()
        ))
        .clicked()
        {
            *visual_mode = visual_mode.next();
        }
        if brand::stroke_button(
            ui,
            Some(StrokeIcon::Scroll),
            if is_tape { "Tape" } else { "Page" },
            is_tape,
            26.0,
        )
        .on_hover_text("Toggle Centered Tape Scroll vs Page Scroll")
        .clicked()
        {
            *scroll_mode = if is_tape {
                ScrollMode::PageScroll
            } else {
                ScrollMode::CenteredTape
            };
        }
        draw_hairline_sep(ui);
        if brand::stroke_button(ui, Some(StrokeIcon::ZoomOut), "", false, 26.0)
            .on_hover_text("Zoom Out (Mouse Wheel)")
            .clicked()
        {
            actions.zoom_factor = Some(0.72);
        }
        if brand::stroke_button(ui, Some(StrokeIcon::ZoomIn), "", false, 26.0)
            .on_hover_text("Zoom In (Mouse Wheel)")
            .clicked()
        {
            actions.zoom_factor = Some(1.38);
        }
        if brand::stroke_button(ui, Some(StrokeIcon::Fit), "Fit", false, 26.0)
            .on_hover_text("Fit entire track in view")
            .clicked()
        {
            actions.zoom_fit = true;
        }
    });

    // Row 2 (Contextual): Dedicated Selected Marker 1-Frame Nudge Bar — zero overlap ever!
    if let Some(sel_id) = selected_mark_id {
        if let Some(m) = track.marks.iter().find(|m| m.id == sel_id) {
            ui.add_space(5.0);
            let tc = format_live_smpte(export.frame_rate, &export.start_timecode, m.time_sec);
            let mcol = mark_color32(&export.style, m.kind);
            Frame::new()
                .fill(brand::primary_tint(24))
                .stroke(Stroke::new(1.0, brand::primary_tint(160)))
                .corner_radius(CornerRadius::same(8))
                .inner_margin(Margin::symmetric(10, 5))
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(8.0, 4.0);
                        let (dr, _) = ui.allocate_exact_size(Vec2::splat(9.0), Sense::hover());
                        ui.painter().circle_filled(dr.center(), 4.5, mcol);

                        ui.label(
                            RichText::new(format!(
                                "Selected: Bar {}.{} · {}",
                                m.bar,
                                m.beat_in_bar,
                                m.kind.display_name()
                            ))
                            .font(brand::font_ui_semibold(12.0))
                            .color(brand::TEXT),
                        );
                        ui.label(
                            RichText::new(tc)
                                .font(brand::font_mono_bold(12.0))
                                .color(brand::LAVENDER),
                        );

                        draw_hairline_sep(ui);

                        if brand::stroke_button(ui, Some(StrokeIcon::PrevBar), "-1 Frame (,)", false, 24.0)
                            .on_hover_text("Move selected marker earlier by 1 frame (shortcut: , or [)")
                            .clicked()
                        {
                            actions.nudge_selected_frames = Some(-1);
                        }
                        if brand::stroke_button(ui, Some(StrokeIcon::NextBar), "+1 Frame (.)", false, 24.0)
                            .on_hover_text("Move selected marker later by 1 frame (shortcut: . or ])")
                            .clicked()
                        {
                            actions.nudge_selected_frames = Some(1);
                        }
                        if brand::stroke_button(ui, Some(StrokeIcon::Trash), "Delete", false, 24.0)
                            .on_hover_text("Delete selected marker (Delete)")
                            .clicked()
                        {
                            actions.delete_selected_mark = true;
                        }
                    });
                });
        }
    }
}

fn draw_beat_filter_pill(ui: &mut Ui, label: &str, dot_col: Color32, enabled: bool) -> egui::Response {
    let font = if enabled {
        brand::font_ui_semibold(11.5)
    } else {
        egui::FontId::proportional(11.5)
    };
    let galley = ui.painter().layout_no_wrap(label.to_string(), font, brand::TEXT);
    let width = 10.0 + 8.0 + 6.0 + galley.size().x + 10.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, 26.0), Sense::click());
    let hovered = resp.hovered();

    let (bg, border, fg) = if enabled {
        (
            if hovered {
                Color32::from_rgba_unmultiplied(dot_col.r(), dot_col.g(), dot_col.b(), 44)
            } else {
                Color32::from_rgba_unmultiplied(dot_col.r(), dot_col.g(), dot_col.b(), 26)
            },
            Color32::from_rgba_unmultiplied(dot_col.r(), dot_col.g(), dot_col.b(), 165),
            brand::TEXT,
        )
    } else {
        (
            if hovered { brand::SURFACE_HOVER } else { brand::CANVAS },
            brand::BORDER,
            Color32::from_rgb(104, 100, 128),
        )
    };

    let p = ui.painter();
    p.rect_filled(rect, 7.0, bg);
    p.rect_stroke(rect, 7.0, Stroke::new(1.0, border), StrokeKind::Inside);

    let dot_center = egui::Pos2::new(rect.left() + 13.0, rect.center().y);
    if enabled {
        p.circle_filled(dot_center, 4.0, dot_col);
    } else {
        p.circle_stroke(dot_center, 3.8, Stroke::new(1.2, Color32::from_rgb(95, 91, 118)));
    }

    let text_pos = egui::Pos2::new(rect.left() + 23.0, rect.center().y - galley.size().y * 0.5);
    p.galley(text_pos, galley, fg);

    resp
}

/// Option C: Minimalist Bottom Transport Deck (wraps cleanly on narrow non-fullscreen windows)
pub fn draw_transport_bar(
    ui: &mut Ui,
    player: &AudioPlayer,
    has_track: bool,
    status_message: Option<&(String, bool)>,
    actions: &mut HudActions,
) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(7.0, 5.0);

        ui.add_enabled_ui(has_track, |ui| {
            if brand::stroke_button(ui, Some(StrokeIcon::Rewind), "", false, 30.0)
                .on_hover_text("Rewind to Start (Home)")
                .clicked()
            {
                actions.rewind_to_start = true;
            }
            if brand::stroke_button(ui, Some(StrokeIcon::PrevBar), "Bar", false, 30.0)
                .on_hover_text("Jump to previous Downbeat (Left Arrow)")
                .clicked()
            {
                actions.jump_downbeat = Some(-1);
            }

            let playing = player.is_playing();
            if brand::primary_cta_button(
                ui,
                Some(if playing { StrokeIcon::Pause } else { StrokeIcon::Play }),
                if playing { "Pause" } else { "Play" },
                Vec2::new(88.0, 32.0),
            )
            .on_hover_text("Play / Pause (Space)")
            .clicked()
            {
                player.toggle_play();
            }

            if brand::stroke_button(ui, Some(StrokeIcon::NextBar), "Bar", false, 30.0)
                .on_hover_text("Jump to next Downbeat (Right Arrow)")
                .clicked()
            {
                actions.jump_downbeat = Some(1);
            }

            draw_hairline_sep(ui);

            let met_on = player.metronome_enabled();
            if brand::stroke_button(
                ui,
                Some(StrokeIcon::Metronome),
                if met_on { "Click ON" } else { "Click OFF" },
                met_on,
                30.0,
            )
            .on_hover_text("Audible sample-accurate metronome click locked to visible beats (M)")
            .clicked()
            {
                player.set_metronome_enabled(!met_on);
            }
            let mut cvol = player.click_volume();
            if ui
                .add_enabled(
                    met_on,
                    egui::Slider::new(&mut cvol, 0.0..=1.25).show_value(false),
                )
                .on_hover_text("Metronome Click Volume")
                .changed()
            {
                player.set_click_volume(cvol);
            }

            let loop_on = player.loop_enabled();
            if brand::stroke_button(ui, Some(StrokeIcon::Loop), "Loop", loop_on, 30.0)
                .on_hover_text("Loop between In and Out brackets (L)")
                .clicked()
            {
                player.set_loop_enabled(!loop_on);
            }

            draw_hairline_sep(ui);

            let cur_speed = player.speed();
            for spd in [0.5f32, 0.75, 1.0, 1.25] {
                let selected = (cur_speed - spd).abs() < 0.03;
                if brand::stroke_button(ui, None, &format!("{spd}×"), selected, 26.0).clicked() {
                    player.set_speed(spd);
                }
            }

            draw_hairline_sep(ui);

            let muted = player.is_muted();
            if brand::stroke_button(
                ui,
                Some(if muted {
                    StrokeIcon::VolumeMute
                } else {
                    StrokeIcon::Volume
                }),
                "",
                muted,
                28.0,
            )
            .on_hover_text("Mute / Unmute track audio")
            .clicked()
            {
                player.set_muted(!muted);
            }
            let mut mvol = player.music_volume();
            if ui
                .add(egui::Slider::new(&mut mvol, 0.0..=1.25).show_value(false))
                .on_hover_text("Music Volume")
                .changed()
            {
                player.set_music_volume(mvol);
            }
        });

        // Pure left-to-right flow inside horizontal_wrapped -> never overlaps volume slider!
        draw_hairline_sep(ui);
        if let Some((msg, is_err)) = status_message {
            ui.label(
                RichText::new(msg)
                    .font(brand::font_mono_bold(11.0))
                    .color(if *is_err { brand::ERROR } else { brand::SUCCESS }),
            );
        } else if has_track {
            ui.label(
                RichText::new("Drag mark: ±1f snap  ·  1–4: Add mark  ·  ,/.: Nudge  ·  Ctrl+Z: Undo")
                    .size(11.0)
                    .color(brand::MUTED),
            );
        }
    });
}
