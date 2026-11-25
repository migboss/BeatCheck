use crate::brand::{self, StrokeIcon};
use crate::config::{format_live_smpte, resolve_color32, ResolvePreset, WaveformVisualMode};
use crate::hud::HudActions;
use beatcheck_audio::AudioPlayer;
use beatcheck_core::model::{GridMode, MarkKind, ResolveColor, TimeSignature, TrackAnalysis};
use beatcheck_edl::{ExportSettings, FrameRate};
use egui::{Align, CornerRadius, Frame, Layout, Margin, RichText, ScrollArea, Stroke, Ui, Vec2};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrawerTab {
    Export,
    TuneGrid,
    Settings,
}

#[derive(Debug, Default)]
pub struct ExportPanelActions {
    pub export_marker_edl: bool,
    pub export_fcpxml: bool,
    pub export_cut_list_edl: bool,
    pub export_csv: bool,
    pub export_json: bool,
    pub open_preview: bool,
    pub save_portable_config: bool,
    pub close_drawer: bool,
    pub select_audio_device: Option<Option<String>>,
    pub reconnect_audio: bool,
    pub test_audio_tone: bool,
}

/// Always-on Right Resolve Inspector with 3 crisp tabs:
/// 1. `Export`   — Timeline FPS & Start TC, Marker Presets, Count Summary, Resolve .edl / FCPXML export
/// 2. `Tune Grid`— Selected Marker 1-Frame Mover, Tempo/Phase, Locked vs Dynamic, Global Nudge, Reset
/// 3. `Settings` — Audio Output Device & Test Beep, Marker Colors & Label Templates, Cut Anticipation, Defaults
#[allow(clippy::too_many_arguments)]
pub fn draw_slide_over_drawer(
    ui: &mut Ui,
    tab: &mut DrawerTab,
    visual_mode: &mut WaveformVisualMode,
    track: Option<&mut TrackAnalysis>,
    selected_mark_id: Option<u64>,
    resolved_count: usize,
    can_reset: bool,
    player: &AudioPlayer,
    audio_devices: &[String],
    settings: &mut ExportSettings,
    frame_sec: f64,
    export_actions: &mut ExportPanelActions,
    hud_actions: &mut HudActions,
) {
    // Drawer Header
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("Timeline Inspector")
                .font(brand::font_display_bold(15.5))
                .color(brand::TEXT),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if brand::stroke_button(ui, Some(StrokeIcon::DrawerClose), "", false, 26.0)
                .on_hover_text("Hide inspector (E)")
                .clicked()
            {
                export_actions.close_drawer = true;
            }
        });
    });

    ui.add_space(6.0);

    // Crisp 3-Tab Switcher: Export | Tune Grid | Settings
    Frame::new()
        .fill(brand::CANVAS)
        .stroke(Stroke::new(1.0, brand::BORDER))
        .corner_radius(CornerRadius::same(9))
        .inner_margin(Margin::same(3))
        .show(ui, |ui| {
            ui.columns(3, |cols| {
                for (ci, (t_val, label)) in [
                    (DrawerTab::Export, "Export"),
                    (DrawerTab::TuneGrid, "Tune Grid"),
                    (DrawerTab::Settings, "Settings"),
                ]
                .into_iter()
                .enumerate()
                {
                    let active = *tab == t_val;
                    let btn = egui::Button::new(
                        RichText::new(label)
                            .font(brand::font_ui_semibold(11.5))
                            .color(if active { brand::TEXT } else { brand::MUTED }),
                    )
                    .fill(if active {
                        brand::PRIMARY
                    } else {
                        egui::Color32::TRANSPARENT
                    })
                    .stroke(Stroke::NONE)
                    .corner_radius(CornerRadius::same(7));
                    if cols[ci]
                        .add_sized([cols[ci].available_width(), 26.0], btn)
                        .clicked()
                    {
                        *tab = t_val;
                    }
                }
            });
        });

    ui.add_space(10.0);

    let panel_w = ui.available_width().max(240.0);
    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_width(panel_w);
            ui.set_max_width(panel_w);
            match *tab {
                DrawerTab::Export => draw_export_tab(
                    ui,
                    panel_w,
                    track.as_deref(),
                    resolved_count,
                    settings,
                    export_actions,
                ),
                DrawerTab::TuneGrid => draw_tune_grid_tab(
                    ui,
                    panel_w,
                    track,
                    selected_mark_id,
                    can_reset,
                    settings,
                    frame_sec,
                    hud_actions,
                ),
                DrawerTab::Settings => draw_settings_tab(
                    ui,
                    panel_w,
                    visual_mode,
                    track.as_deref(),
                    player,
                    audio_devices,
                    settings,
                    export_actions,
                ),
            }
        });
}

fn draw_export_tab(
    ui: &mut Ui,
    panel_w: f32,
    track: Option<&TrackAnalysis>,
    resolved_count: usize,
    settings: &mut ExportSettings,
    actions: &mut ExportPanelActions,
) {
    // 1. Timeline Sync (FPS & Start Timecode right at the top — set before editing 1-frame markers!)
    ui.label(
        RichText::new("TIMELINE FPS & TIMECODE")
            .font(brand::font_mono_bold(10.5))
            .color(brand::HIGHLIGHT),
    );
    ui.add_space(4.0);

    let tc_valid = settings.frame_rate.parse_smpte(&settings.start_timecode);

    Frame::new()
        .fill(brand::SURFACE)
        .stroke(Stroke::new(1.0, brand::BORDER))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(10))
        .show(ui, |ui| {
            ui.set_width((panel_w - 22.0).max(180.0));
            egui::Grid::new("resolve_timeline_grid")
                .num_columns(2)
                .spacing([8.0, 8.0])
                .show(ui, |ui| {
                    ui.label(RichText::new("Frame Rate").size(12.0).color(brand::MUTED));
                    egui::ComboBox::from_id_salt("fps_combo")
                        .width(156.0)
                        .selected_text(
                            RichText::new(settings.frame_rate.label())
                                .font(brand::font_mono_bold(12.0))
                                .color(brand::LAVENDER),
                        )
                        .show_ui(ui, |ui| {
                            for fps in FrameRate::ALL {
                                ui.selectable_value(&mut settings.frame_rate, fps, fps.label());
                            }
                        });
                    ui.end_row();

                    ui.label(RichText::new("Start TC").size(12.0).color(brand::MUTED));
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        ui.add(
                            egui::TextEdit::singleline(&mut settings.start_timecode)
                                .font(egui::TextStyle::Monospace)
                                .desired_width(86.0),
                        );
                        if ui
                            .small_button("01:00")
                            .on_hover_text("Standard broadcast start 01:00:00:00")
                            .clicked()
                        {
                            settings.start_timecode = if settings.frame_rate.is_drop_frame() {
                                "01:00:00;00".to_string()
                            } else {
                                "01:00:00:00".to_string()
                            };
                        }
                        if ui
                            .small_button("00:00")
                            .on_hover_text("Zero start 00:00:00:00")
                            .clicked()
                        {
                            settings.start_timecode = if settings.frame_rate.is_drop_frame() {
                                "00:00:00;00".to_string()
                            } else {
                                "00:00:00:00".to_string()
                            };
                        }
                    });
                    ui.end_row();
                });

            if let Err(err) = &tc_valid {
                ui.add_space(4.0);
                ui.label(RichText::new(err.to_string()).size(11.0).color(brand::ERROR));
            }
        });

    ui.add_space(12.0);

    // 2. One-Click DaVinci Resolve Presets (flush 2x2 full-width grid)
    ui.label(
        RichText::new("MARKER PRESET")
            .font(brand::font_mono_bold(10.5))
            .color(brand::HIGHLIGHT),
    );
    ui.add_space(4.0);

    for row in ResolvePreset::ALL.chunks(2) {
        ui.columns(2, |cols| {
            for (ci, preset) in row.iter().enumerate() {
                let active = preset.matches(&settings.style);
                let btn = egui::Button::new(
                    RichText::new(preset.name())
                        .font(if active {
                            brand::font_ui_semibold(12.0)
                        } else {
                            egui::FontId::proportional(12.0)
                        })
                        .color(if active { brand::LAVENDER } else { brand::MUTED }),
                )
                .fill(if active {
                    brand::primary_tint(42)
                } else {
                    brand::SURFACE
                })
                .stroke(Stroke::new(
                    1.0,
                    if active { brand::PRIMARY } else { brand::BORDER },
                ))
                .corner_radius(CornerRadius::same(8));

                if cols[ci]
                    .add_sized([cols[ci].available_width(), 30.0], btn)
                    .on_hover_text(preset.description())
                    .clicked()
                {
                    preset.apply(&mut settings.style);
                }
            }
        });
        ui.add_space(5.0);
    }

    ui.add_space(8.0);

    // 3. Compact 2x2 Active Marker Summary (click Settings tab to customize colors/templates)
    Frame::new()
        .fill(brand::CANVAS)
        .stroke(Stroke::new(1.0, brand::BORDER))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.set_width((panel_w - 22.0).max(180.0));
            for row in MarkKind::ALL.chunks(2) {
                ui.columns(2, |cols| {
                    for (ci, &kind) in row.iter().enumerate() {
                        let cfg = settings.style.style_for(kind);
                        let count = track
                            .map(|t| {
                                t.marks
                                    .iter()
                                    .filter(|m| {
                                        m.kind == kind && settings.style.is_mark_included(m)
                                    })
                                    .count()
                            })
                            .unwrap_or(0);
                        cols[ci].horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 5.0;
                            let (r, _) =
                                ui.allocate_exact_size(Vec2::splat(9.0), egui::Sense::hover());
                            let col = if cfg.enabled {
                                resolve_color32(cfg.color)
                            } else {
                                brand::BORDER
                            };
                            ui.painter().rect_filled(r, 2.5, col);
                            ui.label(
                                RichText::new(format!("{} ({count})", kind.short_name()))
                                    .font(brand::font_mono_bold(10.5))
                                    .color(if cfg.enabled {
                                        brand::TEXT
                                    } else {
                                        brand::MUTED
                                    }),
                            );
                        });
                    }
                });
                ui.add_space(3.0);
            }
        });

    ui.add_space(14.0);

    // 4. Primary & Secondary Export Actions
    let can_export = track.is_some() && tc_valid.is_ok() && resolved_count > 0;

    ui.add_enabled_ui(can_export, |ui| {
        if brand::primary_cta_button(
            ui,
            Some(StrokeIcon::Export),
            &format!("Export Marker .edl ({resolved_count})"),
            Vec2::new(panel_w, 38.0),
        )
        .on_hover_text("CMX 3600 Marker EDL (DaVinci Resolve: Timeline -> Import -> Timeline Markers from EDL...)")
        .clicked()
        {
            actions.export_marker_edl = true;
        }

        ui.add_space(8.0);

        ui.columns(2, |cols| {
            if cols[0]
                .add_sized(
                    [cols[0].available_width(), 30.0],
                    egui::Button::new(RichText::new("FCPXML Timeline").size(12.0)),
                )
                .on_hover_text("Imports BOTH the audio clip AND markers together (Final Cut Pro & DaVinci Resolve)")
                .clicked()
            {
                actions.export_fcpxml = true;
            }
            if cols[1]
                .add_sized(
                    [cols[1].available_width(), 30.0],
                    egui::Button::new(RichText::new("Cut-List .edl").size(12.0)),
                )
                .on_hover_text("Beat-to-beat sequential CMX 3600 edit segments (Premiere Pro, Resolve, Avid)")
                .clicked()
            {
                actions.export_cut_list_edl = true;
            }
        });

        ui.add_space(6.0);

        ui.columns(3, |cols| {
            if cols[0]
                .add_sized([cols[0].available_width(), 26.0], egui::Button::new("CSV"))
                .clicked()
            {
                actions.export_csv = true;
            }
            if cols[1]
                .add_sized([cols[1].available_width(), 26.0], egui::Button::new("JSON"))
                .clicked()
            {
                actions.export_json = true;
            }
            if cols[2]
                .add_sized(
                    [cols[2].available_width(), 26.0],
                    egui::Button::new("Preview"),
                )
                .clicked()
            {
                actions.open_preview = true;
            }
        });
    });

    if track.is_none() {
        ui.add_space(10.0);
        ui.label(
            RichText::new("Set your timeline FPS & Start Timecode above, then drop an audio track to analyze.")
                .size(11.5)
                .color(brand::MUTED),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_tune_grid_tab(
    ui: &mut Ui,
    panel_w: f32,
    track: Option<&mut TrackAnalysis>,
    selected_mark_id: Option<u64>,
    can_reset: bool,
    export: &ExportSettings,
    frame_sec: f64,
    actions: &mut HudActions,
) {
    let Some(track) = track else {
        ui.label(
            RichText::new("Import an audio track to fine-tune individual markers frame-by-frame, shift beat 1, or adjust BPM.")
                .size(12.0)
                .color(brand::MUTED),
        );
        return;
    };

    // 0. Add Any Beat Marker at Playhead (1-click + keys 1, 2, 3, 4!)
    ui.label(
        RichText::new("ADD MARKER AT PLAYHEAD")
            .font(brand::font_mono_bold(10.5))
            .color(brand::HIGHLIGHT),
    );
    ui.add_space(4.0);

    Frame::new()
        .fill(brand::SURFACE)
        .stroke(Stroke::new(1.0, brand::BORDER))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(10))
        .show(ui, |ui| {
            ui.set_width((panel_w - 22.0).max(180.0));
            let pairs = [
                [(MarkKind::Downbeat, "1"), (MarkKind::StrongBeat, "2")],
                [(MarkKind::RegularBeat, "3"), (MarkKind::EnergyDrop, "4")],
            ];
            for (ri, row) in pairs.iter().enumerate() {
                ui.columns(2, |cols| {
                    for (ci, &(kind, key)) in row.iter().enumerate() {
                        let dot_col = resolve_color32(export.style.style_for(kind).color);
                        let btn = egui::Button::new(
                            RichText::new(format!("+ {} ({key})", kind.short_name()))
                                .font(brand::font_ui_semibold(11.5))
                                .color(brand::TEXT),
                        )
                        .fill(egui::Color32::from_rgba_unmultiplied(
                            dot_col.r(),
                            dot_col.g(),
                            dot_col.b(),
                            28,
                        ))
                        .stroke(Stroke::new(
                            1.0,
                            egui::Color32::from_rgba_unmultiplied(
                                dot_col.r(),
                                dot_col.g(),
                                dot_col.b(),
                                170,
                            ),
                        ))
                        .corner_radius(CornerRadius::same(7));

                        if cols[ci]
                            .add_sized([cols[ci].available_width(), 28.0], btn)
                            .on_hover_text(format!(
                                "Insert {} at current playhead snapped to exact frame (Shortcut: Key {key})",
                                kind.display_name()
                            ))
                            .clicked()
                        {
                            actions.add_mark_at_playhead = Some(kind);
                        }
                    }
                });
                if ri == 0 {
                    ui.add_space(5.0);
                }
            }
            ui.add_space(4.0);
            ui.label(
                RichText::new("Tip: Tap keys 1–4 while playing, or right-click anywhere on the waveform.")
                    .size(10.5)
                    .color(brand::MUTED),
            );
        });

    ui.add_space(12.0);

    // 1. Selected Marker 1-Frame Editor (DaVinci Resolve Style!)
    ui.label(
        RichText::new(format!(
            "MARKER 1-FRAME MOVER ({})",
            export.frame_rate.label()
        ))
        .font(brand::font_mono_bold(10.5))
        .color(brand::HIGHLIGHT),
    );
    ui.add_space(4.0);

    let selected_mark = selected_mark_id.and_then(|id| track.marks.iter().find(|m| m.id == id));

    Frame::new()
        .fill(brand::SURFACE)
        .stroke(Stroke::new(
            1.0,
            if selected_mark.is_some() {
                brand::PRIMARY
            } else {
                brand::BORDER
            },
        ))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(10))
        .show(ui, |ui| {
            ui.set_width((panel_w - 22.0).max(180.0));
            if let Some(m) = selected_mark {
                let tc = format_live_smpte(export.frame_rate, &export.start_timecode, m.time_sec);
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("Bar {}.{} · {}", m.bar, m.beat_in_bar, m.kind.short_name()))
                            .font(brand::font_ui_semibold(12.0))
                            .color(brand::TEXT),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            RichText::new(tc)
                                .font(brand::font_mono_bold(12.0))
                                .color(brand::LAVENDER),
                        );
                    });
                });
                ui.add_space(6.0);
                ui.columns(3, |cols| {
                    if cols[0]
                        .add_sized([cols[0].available_width(), 28.0], egui::Button::new("◀ -1f"))
                        .on_hover_text("Move marker 1 frame earlier (,)")
                        .clicked()
                    {
                        actions.nudge_selected_frames = Some(-1);
                    }
                    if cols[1]
                        .add_sized([cols[1].available_width(), 28.0], egui::Button::new("+1f ▶"))
                        .on_hover_text("Move marker 1 frame later (.)")
                        .clicked()
                    {
                        actions.nudge_selected_frames = Some(1);
                    }
                    if cols[2]
                        .add_sized(
                            [cols[2].available_width(), 28.0],
                            egui::Button::new(RichText::new("Delete").color(brand::ERROR)),
                        )
                        .clicked()
                    {
                        actions.delete_selected_mark = true;
                    }
                });
            } else {
                ui.label(
                    RichText::new("Click any beat line on the waveform to select it, or drag it to snap frame-by-frame.")
                        .size(11.5)
                        .color(brand::MUTED),
                );
            }
        });

    ui.add_space(12.0);

    // 2. Tempo & Downbeat Phase Card
    ui.label(
        RichText::new("TEMPO & DOWNBEAT PHASE")
            .font(brand::font_mono_bold(10.5))
            .color(brand::HIGHLIGHT),
    );
    ui.add_space(4.0);

    Frame::new()
        .fill(brand::SURFACE)
        .stroke(Stroke::new(1.0, brand::BORDER))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(10))
        .show(ui, |ui| {
            ui.set_width((panel_w - 22.0).max(180.0));
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("{:.1} BPM", track.active_bpm))
                        .font(brand::font_mono_bold(14.5))
                        .color(brand::LAVENDER),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(&track.detection_engine)
                            .font(brand::font_mono_bold(10.5))
                            .color(brand::MUTED),
                    );
                });
            });
            ui.add_space(8.0);
            ui.columns(3, |cols| {
                if cols[0]
                    .add_sized([cols[0].available_width(), 28.0], egui::Button::new("÷2 BPM"))
                    .clicked()
                {
                    actions.bpm_multiplier = Some(0.5);
                }
                if cols[1]
                    .add_sized([cols[1].available_width(), 28.0], egui::Button::new("×2 BPM"))
                    .clicked()
                {
                    actions.bpm_multiplier = Some(2.0);
                }
                if cols[2]
                    .add_sized([cols[2].available_width(), 28.0], egui::Button::new("Tap (T)"))
                    .clicked()
                {
                    actions.tap_tempo = true;
                }
            });
            ui.add_space(6.0);
            ui.columns(2, |cols| {
                if cols[0]
                    .add_sized(
                        [cols[0].available_width(), 28.0],
                        egui::Button::new("◀ Shift Beat 1"),
                    )
                    .clicked()
                {
                    actions.shift_downbeat = Some(-1);
                }
                if cols[1]
                    .add_sized(
                        [cols[1].available_width(), 28.0],
                        egui::Button::new("Shift Beat 1 ▶"),
                    )
                    .clicked()
                {
                    actions.shift_downbeat = Some(1);
                }
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Time Signature").size(12.0).color(brand::MUTED));
                let mut selected_ts = track.active_time_sig;
                egui::ComboBox::from_id_salt("drawer_time_sig_combo")
                    .width(72.0)
                    .selected_text(selected_ts.label())
                    .show_ui(ui, |ui| {
                        for ts in TimeSignature::ALL {
                            ui.selectable_value(&mut selected_ts, ts, ts.label());
                        }
                    });
                if selected_ts != track.active_time_sig {
                    actions.set_time_sig = Some(selected_ts);
                }
            });
        });

    ui.add_space(12.0);

    // 3. Grid Mode, Global Nudge & Drop Sensitivity
    ui.label(
        RichText::new("GRID MODE & GLOBAL SHIFT")
            .font(brand::font_mono_bold(10.5))
            .color(brand::HIGHLIGHT),
    );
    ui.add_space(4.0);

    Frame::new()
        .fill(brand::SURFACE)
        .stroke(Stroke::new(1.0, brand::BORDER))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(10))
        .show(ui, |ui| {
            ui.set_width((panel_w - 22.0).max(180.0));
            let locked = track.grid_mode == GridMode::LockedConstantBpm;
            ui.columns(2, |cols| {
                if cols[0]
                    .selectable_label(locked, "Locked BPM")
                    .on_hover_text("Strict quantized BPM grid")
                    .clicked()
                    && !locked
                {
                    actions.set_grid_mode = Some(GridMode::LockedConstantBpm);
                }
                if cols[1]
                    .selectable_label(!locked, "Dynamic")
                    .on_hover_text("Follow live drummer tempo drift")
                    .clicked()
                    && locked
                {
                    actions.set_grid_mode = Some(GridMode::Dynamic);
                }
            });
            ui.add_space(6.0);
            let mut extrap = track.extrapolate_silence;
            if ui
                .checkbox(&mut extrap, "Fill beatless Intro / Outro")
                .changed()
            {
                actions.set_extrapolate = Some(extrap);
            }
            ui.add_space(8.0);
            ui.label(RichText::new("Shift All Marks").size(11.5).color(brand::MUTED));
            ui.add_space(4.0);
            ui.columns(4, |cols| {
                if cols[0]
                    .add_sized([cols[0].available_width(), 26.0], egui::Button::new("-1f"))
                    .clicked()
                {
                    actions.nudge_sec = Some(-frame_sec);
                }
                if cols[1]
                    .add_sized([cols[1].available_width(), 26.0], egui::Button::new("-10ms"))
                    .clicked()
                {
                    actions.nudge_sec = Some(-0.010);
                }
                if cols[2]
                    .add_sized([cols[2].available_width(), 26.0], egui::Button::new("+10ms"))
                    .clicked()
                {
                    actions.nudge_sec = Some(0.010);
                }
                if cols[3]
                    .add_sized([cols[3].available_width(), 26.0], egui::Button::new("+1f"))
                    .clicked()
                {
                    actions.nudge_sec = Some(frame_sec);
                }
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Drop Sensitivity").size(11.5).color(brand::MUTED));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("{:.0}%", track.drop_sensitivity * 100.0))
                            .font(brand::font_mono_bold(11.5))
                            .color(brand::LAVENDER),
                    );
                });
            });
            let mut drop_sens = track.drop_sensitivity;
            let drop_resp = ui.add(
                egui::Slider::new(&mut drop_sens, 0.0..=1.0).show_value(false),
            );
            if drop_resp.changed() {
                actions.set_drop_sensitivity = Some(drop_sens);
            }
            if !drop_resp.dragged() {
                actions.drop_sensitivity_released = true;
            }
        });

    ui.add_space(12.0);

    ui.add_enabled_ui(can_reset, |ui| {
        if brand::stroke_button(
            ui,
            Some(StrokeIcon::Reset),
            "Reset All to Algorithm Marks",
            false,
            30.0,
        )
        .on_hover_text("Restore original detected marks & BPM (undoable with Ctrl+Z)")
        .clicked()
        {
            actions.reset_to_auto = true;
        }
    });
}

/// New Settings Tab: Audio Output Device & Diagnostics, Export Range & Offsets,
/// Full 16-Color Resolve Marker Customization, and Portable Defaults.
fn draw_settings_tab(
    ui: &mut Ui,
    panel_w: f32,
    visual_mode: &mut WaveformVisualMode,
    track: Option<&TrackAnalysis>,
    player: &AudioPlayer,
    audio_devices: &[String],
    settings: &mut ExportSettings,
    actions: &mut ExportPanelActions,
) {
    // 0. Waveform Display Mode (3 modes: Outline | HD Solid | RGB Bands)
    ui.label(
        RichText::new("WAVEFORM DISPLAY MODE")
            .font(brand::font_mono_bold(10.5))
            .color(brand::HIGHLIGHT),
    );
    ui.add_space(4.0);

    Frame::new()
        .fill(brand::SURFACE)
        .stroke(Stroke::new(1.0, brand::BORDER))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(10))
        .show(ui, |ui| {
            ui.set_width((panel_w - 22.0).max(180.0));
            ui.columns(3, |cols| {
                for (ci, mode) in WaveformVisualMode::ALL.into_iter().enumerate() {
                    let active = *visual_mode == mode;
                    let btn = egui::Button::new(
                        RichText::new(mode.label())
                            .font(if active {
                                brand::font_ui_semibold(11.5)
                            } else {
                                egui::FontId::proportional(11.5)
                            })
                            .color(if active { brand::LAVENDER } else { brand::MUTED }),
                    )
                    .fill(if active {
                        brand::primary_tint(44)
                    } else {
                        brand::CANVAS
                    })
                    .stroke(Stroke::new(
                        1.0,
                        if active { brand::PRIMARY } else { brand::BORDER },
                    ))
                    .corner_radius(CornerRadius::same(7));
                    if cols[ci]
                        .add_sized([cols[ci].available_width(), 28.0], btn)
                        .on_hover_text(mode.description())
                        .clicked()
                    {
                        *visual_mode = mode;
                    }
                }
            });
            ui.add_space(5.0);
            ui.label(
                RichText::new(visual_mode.description())
                    .size(11.0)
                    .color(brand::MUTED),
            );
        });

    ui.add_space(12.0);

    // 1. Audio Output Device & Live Diagnostics
    ui.label(
        RichText::new("AUDIO OUTPUT")
            .font(brand::font_mono_bold(10.5))
            .color(brand::HIGHLIGHT),
    );
    ui.add_space(4.0);

    Frame::new()
        .fill(brand::SURFACE)
        .stroke(Stroke::new(1.0, brand::BORDER))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(10))
        .show(ui, |ui| {
            ui.set_width((panel_w - 22.0).max(180.0));

            let current_sel = player.selected_device_name().map(|s| s.to_string());
            let selected_label = current_sel
                .as_deref()
                .unwrap_or("System Default");

            ui.label(RichText::new("Output Device").size(11.5).color(brand::MUTED));
            egui::ComboBox::from_id_salt("audio_device_combo")
                .width((panel_w - 44.0).max(160.0))
                .selected_text(selected_label)
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(current_sel.is_none(), "System Default")
                        .clicked()
                    {
                        actions.select_audio_device = Some(None);
                    }
                    for dev in audio_devices {
                        let is_sel = current_sel.as_deref() == Some(dev.as_str());
                        if ui.selectable_label(is_sel, dev).clicked() {
                            actions.select_audio_device = Some(Some(dev.clone()));
                        }
                    }
                });

            ui.add_space(6.0);
            ui.label(
                RichText::new(format!("Active: {}", player.active_device_name()))
                    .size(11.0)
                    .color(brand::TEXT),
            );
            if !player.active_format_info().is_empty() {
                ui.label(
                    RichText::new(player.active_format_info())
                        .font(brand::font_mono_bold(10.5))
                        .color(brand::MUTED),
                );
            }
            if let Some(err) = player.stream_error() {
                ui.add_space(4.0);
                ui.label(RichText::new(err).size(11.0).color(brand::ERROR));
            }

            // Live Output Level Bar
            ui.add_space(6.0);
            let peak = player.output_peak().clamp(0.0, 1.0);
            let (mrect, _) =
                ui.allocate_exact_size(Vec2::new(ui.available_width(), 6.0), egui::Sense::hover());
            ui.painter().rect_filled(mrect, 3.0, brand::CANVAS);
            if peak > 0.005 {
                let fill = egui::Rect::from_min_size(
                    mrect.min,
                    Vec2::new(mrect.width() * peak, mrect.height()),
                );
                ui.painter().rect_filled(fill, 3.0, brand::PRIMARY);
            }

            ui.add_space(8.0);
            ui.columns(2, |cols| {
                if cols[0]
                    .add_sized([cols[0].available_width(), 26.0], egui::Button::new("Test Beep"))
                    .on_hover_text("Play a short 440Hz ping through the active audio output")
                    .clicked()
                {
                    actions.test_audio_tone = true;
                }
                if cols[1]
                    .add_sized([cols[1].available_width(), 26.0], egui::Button::new("Reconnect"))
                    .on_hover_text("Re-open the Windows/macOS audio device stream")
                    .clicked()
                {
                    actions.reconnect_audio = true;
                }
            });
        });

    ui.add_space(12.0);

    // 2. Export Range, Bar Stride & Cut Anticipation
    ui.label(
        RichText::new("EXPORT RANGE & CUT OFFSET")
            .font(brand::font_mono_bold(10.5))
            .color(brand::HIGHLIGHT),
    );
    ui.add_space(4.0);

    Frame::new()
        .fill(brand::SURFACE)
        .stroke(Stroke::new(1.0, brand::BORDER))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(10))
        .show(ui, |ui| {
            ui.set_width((panel_w - 22.0).max(180.0));
            ui.checkbox(&mut settings.respect_in_out_range, "Export only In–Out range");
            ui.add_enabled(
                settings.respect_in_out_range,
                egui::Checkbox::new(
                    &mut settings.rebase_in_point_to_start,
                    "Rebase In-point to Start TC",
                ),
            );
            ui.add_space(8.0);
            ui.label(RichText::new("Downbeat Bar Stride").size(11.5).color(brand::MUTED));
            ui.horizontal(|ui| {
                for stride in [1u32, 2, 4, 8] {
                    let lbl = if stride == 1 {
                        "1 Bar".to_string()
                    } else {
                        format!("{stride} Bars")
                    };
                    ui.selectable_value(&mut settings.style.downbeat_bar_stride, stride, lbl);
                }
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Cut Anticipation:").size(11.5).color(brand::MUTED));
                ui.add(
                    egui::DragValue::new(&mut settings.anticipation_frames)
                        .range(-12..=12)
                        .suffix(" f"),
                );
                ui.add(
                    egui::DragValue::new(&mut settings.anticipation_ms)
                        .range(-250.0..=250.0)
                        .speed(1.0)
                        .suffix(" ms"),
                );
            })
            .response
            .on_hover_text("Shift exported markers slightly early so visual cuts land punchier");
        });

    ui.add_space(12.0);

    // 3. Per-Beat-Type Resolve Colors & Label Templates
    ui.label(
        RichText::new("MARKER COLORS & LABELS")
            .font(brand::font_mono_bold(10.5))
            .color(brand::HIGHLIGHT),
    );
    ui.add_space(4.0);

    for kind in MarkKind::ALL {
        let count = track
            .map(|t| {
                t.marks
                    .iter()
                    .filter(|m| m.kind == kind && settings.style.is_mark_included(m))
                    .count()
            })
            .unwrap_or(0);
        let cfg = settings.style.style_for_mut(kind);
        Frame::new()
            .fill(brand::SURFACE)
            .stroke(Stroke::new(1.0, brand::BORDER))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(Margin::same(8))
            .show(ui, |ui| {
                ui.set_width((panel_w - 18.0).max(180.0));
                ui.horizontal(|ui| {
                    ui.checkbox(
                        &mut cfg.enabled,
                        RichText::new(kind.display_name()).font(brand::font_ui_semibold(12.0)),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            RichText::new(format!("{count}"))
                                .font(brand::font_mono_bold(11.0))
                                .color(brand::MUTED),
                        );
                    });
                });
                ui.horizontal(|ui| {
                    let swatch = resolve_color32(cfg.color);
                    let (r, _) =
                        ui.allocate_exact_size(Vec2::new(13.0, 13.0), egui::Sense::hover());
                    ui.painter().rect_filled(r, 3.0, swatch);

                    egui::ComboBox::from_id_salt(("color_combo", kind))
                        .width(92.0)
                        .selected_text(cfg.color.display_name())
                        .show_ui(ui, |ui| {
                            for c in ResolveColor::ALL {
                                ui.horizontal(|ui| {
                                    let (sr, _) = ui.allocate_exact_size(
                                        Vec2::new(11.0, 11.0),
                                        egui::Sense::hover(),
                                    );
                                    ui.painter().rect_filled(sr, 2.0, resolve_color32(c));
                                    ui.selectable_value(&mut cfg.color, c, c.display_name());
                                });
                            }
                        });

                    ui.add(
                        egui::TextEdit::singleline(&mut cfg.label_template)
                            .desired_width(ui.available_width())
                            .hint_text("{bar}, {beat}"),
                    )
                    .on_hover_text("Variables: {bar}, {beat}, {type}, {bpm}");
                });
            });
        ui.add_space(4.0);
    }

    ui.add_space(10.0);
    if brand::stroke_button(
        ui,
        Some(StrokeIcon::Save),
        "Save Defaults to beatcheck.toml",
        false,
        30.0,
    )
    .on_hover_text("Saves FPS, Start TC, colors & audio volumes right beside the executable")
    .clicked()
    {
        actions.save_portable_config = true;
    }
}
