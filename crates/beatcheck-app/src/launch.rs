use crate::brand::{self, StrokeIcon};
use crate::shortcuts::{KeyBindings, ShortcutAction};
use beatcheck_edl::ExportSettings;
use egui::{Align, CornerRadius, Frame, Layout, Margin, RichText, Stroke, Ui, Vec2};
use std::path::PathBuf;

#[derive(Debug, Default)]
pub struct LaunchActions {
    pub open_file_dialog: bool,
    pub open_recent_path: Option<PathBuf>,
    pub open_shortcuts_page: bool,
}

/// Option B: Split Welcome & Resolve Guide
/// Clean obsidian studio layout (no hard-edged fake glow circles).
pub fn draw_split_welcome_screen(
    ui: &mut Ui,
    export: &ExportSettings,
    shortcuts: &KeyBindings,
    recent_files: &[String],
    actions: &mut LaunchActions,
) {
    let avail = ui.available_size();
    let max_w = (avail.x - 24.0).clamp(480.0, 940.0);
    let side_pad = ((avail.x - max_w) * 0.5).max(8.0);
    let top_pad = ((avail.y - 430.0) * 0.44).max(20.0);

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add_space(top_pad);
            ui.horizontal(|ui| {
                ui.add_space(side_pad);
                ui.allocate_ui_with_layout(
                    Vec2::new(max_w, 420.0),
                    Layout::left_to_right(Align::Min),
                    |ui| {
                        let gap = 20.0;
                        let left_w = (max_w - gap) * 0.56;
                        let right_w = (max_w - gap) * 0.44;

                        // LEFT COLUMN: Branded Drop Portal
                        Frame::new()
                            .fill(brand::PANEL)
                            .stroke(Stroke::new(1.0, brand::BORDER))
                            .corner_radius(CornerRadius::same(16))
                            .inner_margin(Margin::same(28))
                            .show(ui, |ui| {
                                ui.set_width(left_w - 56.0);
                                ui.set_min_height(362.0);

                                ui.vertical_centered(|ui| {
                                    ui.add_space(6.0);
                                    brand::draw_pulse_check_logo(ui, 68.0);
                                    ui.add_space(14.0);

                                    ui.horizontal(|ui| {
                                        let wm_w = 138.0;
                                        let pad = ((ui.available_width() - wm_w) * 0.5).max(0.0);
                                        ui.add_space(pad);
                                        brand::draw_wordmark(ui, 28.0);
                                    });

                                    ui.add_space(6.0);
                                    ui.label(
                                        RichText::new(brand::BRAND_TAGLINE)
                                            .font(brand::font_display_medium(15.0))
                                            .color(brand::HIGHLIGHT),
                                    );
                                    ui.add_space(6.0);
                                    ui.label(
                                        RichText::new(brand::BRAND_SUBTITLE)
                                            .size(12.5)
                                            .color(brand::MUTED),
                                    );

                                    ui.add_space(22.0);

                                    Frame::new()
                                        .fill(brand::CANVAS)
                                        .stroke(Stroke::new(1.0, brand::primary_tint(115)))
                                        .corner_radius(CornerRadius::same(12))
                                        .inner_margin(Margin::symmetric(18, 18))
                                        .show(ui, |ui| {
                                            ui.vertical_centered(|ui| {
                                                ui.label(
                                                    RichText::new(
                                                        "Drop an audio track anywhere in this window",
                                                    )
                                                    .font(brand::font_ui_semibold(13.0))
                                                    .color(brand::TEXT),
                                                );
                                                ui.add_space(4.0);
                                                ui.label(
                                                    RichText::new("MP3 · WAV · FLAC · OGG · M4A · AAC")
                                                        .font(brand::font_mono_bold(11.0))
                                                        .color(brand::MUTED),
                                                );
                                                ui.add_space(14.0);
                                                if brand::primary_cta_button(
                                                    ui,
                                                    Some(StrokeIcon::Folder),
                                                    &format!(
                                                        "Import Audio Track  ({})",
                                                        shortcuts.short_label(ShortcutAction::OpenFile)
                                                    ),
                                                    Vec2::new(236.0, 38.0),
                                                )
                                                .clicked()
                                                {
                                                    actions.open_file_dialog = true;
                                                }
                                            });
                                        });
                                });
                            });

                        ui.add_space(gap);

                        // RIGHT COLUMN: DaVinci Resolve Quick-Start + Recent Tracks
                        ui.vertical(|ui| {
                            ui.set_width(right_w);

                            Frame::new()
                                .fill(brand::PANEL)
                                .stroke(Stroke::new(1.0, brand::BORDER))
                                .corner_radius(CornerRadius::same(16))
                                .inner_margin(Margin::same(20))
                                .show(ui, |ui| {
                                    ui.set_width(right_w - 40.0);
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            RichText::new("EDITOR QUICK-START")
                                                .font(brand::font_mono_bold(10.5))
                                                .color(brand::HIGHLIGHT),
                                        );
                                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                            ui.label(
                                                RichText::new(format!(
                                                    "{} · {}",
                                                    export.frame_rate.label(),
                                                    export.start_timecode
                                                ))
                                                .font(brand::font_mono_bold(10.5))
                                                .color(brand::MUTED),
                                            );
                                        });
                                    });
                                    ui.add_space(10.0);

                                    draw_guide_card(
                                        ui,
                                        "01",
                                        "Timeline Marker .edl",
                                        "Drop color-coded beat markers straight onto an existing DaVinci Resolve timeline.",
                                    );
                                    ui.add_space(8.0);
                                    draw_guide_card(
                                        ui,
                                        "02",
                                        "FCPXML Timeline (Audio + Marks)",
                                        "One-click timeline with song & markers synced (Final Cut Pro & Resolve).",
                                    );
                                    ui.add_space(8.0);
                                    draw_guide_card(
                                        ui,
                                        "03",
                                        "Cut-List .edl, CSV & JSON",
                                        "Export beat-to-beat edits or timecodes for Premiere Pro, Avid, After Effects & more.",
                                    );
                                });

                            ui.add_space(14.0);

                            Frame::new()
                                .fill(brand::PANEL)
                                .stroke(Stroke::new(1.0, brand::BORDER))
                                .corner_radius(CornerRadius::same(16))
                                .inner_margin(Margin::same(20))
                                .show(ui, |ui| {
                                    ui.set_width(right_w - 40.0);
                                    let existing_recents: Vec<PathBuf> = recent_files
                                        .iter()
                                        .map(PathBuf::from)
                                        .take(3)
                                        .collect();

                                    if !existing_recents.is_empty() {
                                        ui.label(
                                            RichText::new("RECENT TRACKS")
                                                .font(brand::font_mono_bold(10.5))
                                                .color(brand::HIGHLIGHT),
                                        );
                                        ui.add_space(8.0);
                                        for path in existing_recents {
                                            let raw_name = path
                                                .file_name()
                                                .and_then(|s| s.to_str())
                                                .unwrap_or("Audio Track");
                                            let display_name = truncate_filename(raw_name, 30);
                                            if brand::stroke_button(
                                                ui,
                                                Some(StrokeIcon::Waveform),
                                                &display_name,
                                                false,
                                                30.0,
                                            )
                                            .on_hover_text(path.display().to_string())
                                            .clicked()
                                            {
                                                actions.open_recent_path = Some(path);
                                            }
                                            ui.add_space(4.0);
                                        }
                                    } else {
                                        ui.horizontal(|ui| {
                                            ui.label(
                                                RichText::new("ESSENTIAL SHORTCUTS")
                                                    .font(brand::font_mono_bold(10.5))
                                                    .color(brand::HIGHLIGHT),
                                            );
                                            ui.with_layout(
                                                Layout::right_to_left(Align::Center),
                                                |ui| {
                                                    if ui
                                                        .small_button("Customize")
                                                        .on_hover_text(
                                                            "Open the Keyboard Shortcuts page",
                                                        )
                                                        .clicked()
                                                    {
                                                        actions.open_shortcuts_page = true;
                                                    }
                                                },
                                            );
                                        });
                                        ui.add_space(8.0);
                                        draw_shortcut_row(
                                            ui,
                                            &shortcuts.short_label(ShortcutAction::PlayPause),
                                            "Play / Pause audio",
                                        );
                                        draw_shortcut_row(
                                            ui,
                                            &shortcuts.label(ShortcutAction::AddRegularBeat),
                                            &format!(
                                                "Drop Beat marker ({}/{}/{} for other types)",
                                                shortcuts.short_label(ShortcutAction::AddDownbeat),
                                                shortcuts.short_label(ShortcutAction::AddStrongBeat),
                                                shortcuts.short_label(ShortcutAction::AddEnergyDrop),
                                            ),
                                        );
                                        draw_shortcut_row(
                                            ui,
                                            &format!(
                                                "{} / {}",
                                                shortcuts.short_label(ShortcutAction::StepFrameBack),
                                                shortcuts
                                                    .short_label(ShortcutAction::StepFrameForward)
                                            ),
                                            "Step playhead -1 / +1 frame",
                                        );
                                        draw_shortcut_row(
                                            ui,
                                            &format!(
                                                "{} / {}",
                                                shortcuts.short_label(ShortcutAction::NudgeMarkBack),
                                                shortcuts
                                                    .short_label(ShortcutAction::NudgeMarkForward)
                                            ),
                                            "Move selected marker -1 / +1 frame",
                                        );
                                        draw_shortcut_row(
                                            ui,
                                            &shortcuts.short_label(ShortcutAction::ToggleMetronome),
                                            "Toggle sample-accurate metronome click",
                                        );
                                    }
                                });
                        });
                    },
                );
            });
        });
}

fn truncate_filename(name: &str, max_chars: usize) -> String {
    let count = name.chars().count();
    if count <= max_chars {
        name.to_string()
    } else {
        let keep = max_chars.saturating_sub(1);
        let prefix: String = name.chars().take(keep).collect();
        format!("{prefix}…")
    }
}

fn draw_guide_card(ui: &mut Ui, step: &str, title: &str, desc: &str) {
    Frame::new()
        .fill(brand::SURFACE)
        .stroke(Stroke::new(1.0, brand::BORDER))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::symmetric(12, 9))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(step)
                        .font(brand::font_mono_bold(11.0))
                        .color(brand::PRIMARY),
                );
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.label(
                        RichText::new(title)
                            .font(brand::font_ui_semibold(12.0))
                            .color(brand::TEXT),
                    );
                    ui.label(RichText::new(desc).size(11.0).color(brand::MUTED));
                });
            });
        });
}

fn draw_shortcut_row(ui: &mut Ui, key: &str, action: &str) {
    ui.horizontal(|ui| {
        Frame::new()
            .fill(brand::SURFACE)
            .stroke(Stroke::new(1.0, brand::BORDER))
            .corner_radius(CornerRadius::same(5))
            .inner_margin(Margin::symmetric(7, 2))
            .show(ui, |ui| {
                ui.label(
                    RichText::new(key)
                        .font(brand::font_mono_bold(10.5))
                        .color(brand::LAVENDER),
                );
            });
        ui.label(RichText::new(action).size(11.5).color(brand::MUTED));
    });
    ui.add_space(2.0);
}

/// Sleek, centered Obsidian & Electric Violet analyzing card (no harsh background circles).
pub fn draw_analyzing_screen(ui: &mut Ui, filename: &str, progress: f32, stage: &str) {
    let avail = ui.available_size();
    let card_w = 480.0f32.min(avail.x - 40.0).max(320.0);
    let card_h = 270.0f32;
    let left_pad = ((avail.x - card_w) * 0.5).max(16.0);
    let top_pad = ((avail.y - card_h) * 0.45).max(24.0);
    let clamped = progress.clamp(0.02, 1.0);

    ui.add_space(top_pad);
    ui.horizontal(|ui| {
        ui.add_space(left_pad);

        Frame::new()
            .fill(brand::PANEL)
            .stroke(Stroke::new(1.0, brand::BORDER))
            .corner_radius(CornerRadius::same(16))
            .inner_margin(Margin::same(28))
            .show(ui, |ui| {
                // Explicitly switch back to vertical layout (outer wrapper is horizontal for centering!)
                ui.vertical(|ui| {
                    ui.set_width(card_w - 56.0);

                    // Header Row: Real Pulse Check Icon + Title + Live Percentage
                    ui.horizontal(|ui| {
                        brand::draw_pulse_check_logo(ui, 48.0);
                        ui.add_space(8.0);
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 3.0;
                            ui.label(
                                RichText::new("Analyzing Track")
                                    .font(brand::font_display_bold(18.0))
                                    .color(brand::TEXT),
                            );
                            ui.label(
                                RichText::new(if filename.is_empty() {
                                    "Preparing audio stream..."
                                } else {
                                    filename
                                })
                                .size(12.5)
                                .color(brand::MUTED),
                            );
                        });

                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(
                                RichText::new(format!("{:.0}%", clamped * 100.0))
                                    .font(brand::font_mono_bold(20.0))
                                    .color(brand::LAVENDER),
                            );
                        });
                    });

                    ui.add_space(20.0);

                    // Clean slim 6px electric-violet progress track
                    let bar_w = ui.available_width();
                    let (rect, _) =
                        ui.allocate_exact_size(Vec2::new(bar_w, 6.0), egui::Sense::hover());
                    let p = ui.painter();
                    p.rect_filled(rect, 3.0, brand::SURFACE);
                    let fill_w = (rect.width() * clamped).max(6.0);
                    let fill_rect =
                        egui::Rect::from_min_size(rect.min, Vec2::new(fill_w, rect.height()));
                    p.rect_filled(fill_rect, 3.0, brand::PRIMARY);

                    ui.add_space(18.0);

                    // Clean 3-step pipeline status list
                    Frame::new()
                        .fill(brand::CANVAS)
                        .stroke(Stroke::new(1.0, brand::BORDER))
                        .corner_radius(CornerRadius::same(10))
                        .inner_margin(Margin::symmetric(14, 12))
                        .show(ui, |ui| {
                            ui.vertical(|ui| {
                                let steps = [
                                    (0.15, "01", "Decode audio & build multi-band waveform"),
                                    (0.75, "02", "Beat This! neural beat & downbeat inference"),
                                    (0.98, "03", "Detect energy drops & build beat grid"),
                                ];
                                for (idx, (done_at, num, label)) in steps.iter().enumerate() {
                                    let prev_done = if idx == 0 { 0.0 } else { steps[idx - 1].0 };
                                    let is_done = clamped >= *done_at;
                                    let is_active = !is_done && clamped >= prev_done;

                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing.x = 8.0;
                                        let (dot_r, _) = ui.allocate_exact_size(
                                            Vec2::splat(14.0),
                                            egui::Sense::hover(),
                                        );
                                        if is_done {
                                            brand::paint_stroke_icon(
                                                ui.painter(),
                                                dot_r,
                                                StrokeIcon::Check,
                                                brand::SUCCESS,
                                            );
                                        } else if is_active {
                                            ui.painter().circle_filled(
                                                dot_r.center(),
                                                4.0,
                                                brand::PRIMARY,
                                            );
                                            ui.painter().circle_stroke(
                                                dot_r.center(),
                                                6.0,
                                                Stroke::new(1.0, brand::highlight_tint(140)),
                                            );
                                        } else {
                                            ui.painter().circle_stroke(
                                                dot_r.center(),
                                                3.5,
                                                Stroke::new(1.2, brand::BORDER),
                                            );
                                        }

                                        ui.label(
                                            RichText::new(*num)
                                                .font(brand::font_mono_bold(11.0))
                                                .color(if is_active {
                                                    brand::HIGHLIGHT
                                                } else {
                                                    brand::MUTED
                                                }),
                                        );
                                        ui.label(
                                            RichText::new(*label)
                                                .size(12.0)
                                                .color(if is_done || is_active {
                                                    brand::TEXT
                                                } else {
                                                    brand::MUTED
                                                }),
                                        );
                                    });
                                    if idx < 2 {
                                        ui.add_space(6.0);
                                    }
                                }
                            });
                        });

                    ui.add_space(12.0);

                    // Subtle footer stage & offline badge
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(stage)
                                .font(egui::FontId::monospace(11.0))
                                .color(brand::HIGHLIGHT),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(
                                RichText::new("100% Offline")
                                    .font(brand::font_mono_bold(10.5))
                                    .color(brand::MUTED),
                            );
                        });
                    });
                });
            });
    });
}
