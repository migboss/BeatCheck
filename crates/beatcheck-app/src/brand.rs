use egui::{
    Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, IconData, Pos2, Rect,
    Response, RichText, Sense, Shape, Stroke, StrokeKind, Ui, Vec2,
};
use std::sync::Arc;

// =============================================================================
// 01. VOICE & TAGLINE — Option A: Editorial Precision
// =============================================================================
pub const BRAND_NAME: &str = "BeatCheck";
pub const BRAND_TAGLINE: &str = "Cut on the beat. Every frame.";
pub const BRAND_SUBTITLE: &str =
    "Offline neural beat tracking engineered for frame-accurate video editing timelines.";

#[cfg(target_os = "macos")]
pub const MOD_KEY: &str = "Cmd";
#[cfg(not(target_os = "macos"))]
pub const MOD_KEY: &str = "Ctrl";

#[cfg(target_os = "macos")]
pub const REDO_SHORTCUT: &str = "Cmd+Shift+Z";
#[cfg(not(target_os = "macos"))]
pub const REDO_SHORTCUT: &str = "Ctrl+Y";

// =============================================================================
// 02. COLOR PALETTE — Option A: Obsidian & Electric Violet
// =============================================================================
pub const CANVAS: Color32 = Color32::from_rgb(8, 8, 12); // #08080C
pub const PANEL: Color32 = Color32::from_rgb(16, 16, 23); // #101017
pub const SURFACE: Color32 = Color32::from_rgb(25, 24, 37); // #191825
pub const SURFACE_HOVER: Color32 = Color32::from_rgb(35, 33, 52); // #232134
pub const BORDER: Color32 = Color32::from_rgb(45, 42, 66); // #2D2A42
pub const DEEP_VIOLET: Color32 = Color32::from_rgb(59, 29, 122); // #3B1D7A
pub const PRIMARY: Color32 = Color32::from_rgb(139, 92, 246); // #8B5CF6
pub const PRIMARY_HOVER: Color32 = Color32::from_rgb(157, 116, 249); // #9D74F9
pub const HIGHLIGHT: Color32 = Color32::from_rgb(167, 139, 250); // #A78BFA
pub const LAVENDER: Color32 = Color32::from_rgb(221, 214, 254); // #DDD6FE
pub const TEXT: Color32 = Color32::from_rgb(248, 247, 255); // #F8F7FF
pub const MUTED: Color32 = Color32::from_rgb(154, 150, 181); // #9A96B5
pub const SUCCESS: Color32 = Color32::from_rgb(74, 222, 128);
pub const ERROR: Color32 = Color32::from_rgb(248, 113, 113);

pub fn primary_tint(alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(139, 92, 246, alpha)
}

pub fn highlight_tint(alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(167, 139, 250, alpha)
}

pub fn deep_violet_tint(alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(59, 29, 122, alpha)
}

// =============================================================================
// 03. APP ICON — Option A: Pulse Check (64x64 native icon + 128x128 GPU texture)
// =============================================================================
const ICON_64_RGBA: &[u8] = include_bytes!("../../../assets/brand/icon_64.rgba");
const ICON_128_RGBA: &[u8] = include_bytes!("../../../assets/brand/icon_128.rgba");

pub fn load_window_icon() -> IconData {
    IconData {
        rgba: ICON_64_RGBA.to_vec(),
        width: 64,
        height: 64,
    }
}

fn pulse_check_texture(ctx: &egui::Context) -> egui::TextureHandle {
    let id = egui::Id::new("brand_pulse_check_icon_tex");
    if let Some(tex) = ctx.data(|d| d.get_temp::<egui::TextureHandle>(id)) {
        return tex;
    }
    let image = egui::ColorImage::from_rgba_unmultiplied([128, 128], ICON_128_RGBA);
    let tex = ctx.load_texture(
        "beatcheck_pulse_check_icon",
        image,
        egui::TextureOptions::LINEAR,
    );
    ctx.data_mut(|d| d.insert_temp(id, tex.clone()));
    tex
}

// =============================================================================
// 04. TYPOGRAPHY — Option A: Geometric Precision
//     Space Grotesk (Display) · Inter (UI) · JetBrains Mono (Timecode / BPM)
// =============================================================================
const SPACE_GROTESK_BOLD: &[u8] = include_bytes!("../../../assets/fonts/SpaceGrotesk-Bold.ttf");
const SPACE_GROTESK_MEDIUM: &[u8] = include_bytes!("../../../assets/fonts/SpaceGrotesk-Medium.ttf");
const INTER_REGULAR: &[u8] = include_bytes!("../../../assets/fonts/Inter-Regular.ttf");
const INTER_SEMIBOLD: &[u8] = include_bytes!("../../../assets/fonts/Inter-SemiBold.ttf");
const JETBRAINS_REGULAR: &[u8] = include_bytes!("../../../assets/fonts/JetBrainsMono-Regular.ttf");
const JETBRAINS_BOLD: &[u8] = include_bytes!("../../../assets/fonts/JetBrainsMono-Bold.ttf");

pub fn font_display_bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("display_bold".into()))
}

pub fn font_display_medium(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("display_medium".into()))
}

pub fn font_ui_semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("ui_semibold".into()))
}

pub fn font_mono_bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("mono_bold".into()))
}

pub fn configure_brand_theme(ctx: &egui::Context) {
    // 1. Install embedded Geometric Precision fonts
    let mut fonts = FontDefinitions::default();

    fonts.font_data.insert(
        "space_grotesk_bold".into(),
        Arc::new(FontData::from_static(SPACE_GROTESK_BOLD)),
    );
    fonts.font_data.insert(
        "space_grotesk_medium".into(),
        Arc::new(FontData::from_static(SPACE_GROTESK_MEDIUM)),
    );
    fonts.font_data.insert(
        "inter_regular".into(),
        Arc::new(FontData::from_static(INTER_REGULAR)),
    );
    fonts.font_data.insert(
        "inter_semibold".into(),
        Arc::new(FontData::from_static(INTER_SEMIBOLD)),
    );
    fonts.font_data.insert(
        "jetbrains_regular".into(),
        Arc::new(FontData::from_static(JETBRAINS_REGULAR)),
    );
    fonts.font_data.insert(
        "jetbrains_bold".into(),
        Arc::new(FontData::from_static(JETBRAINS_BOLD)),
    );

    if let Some(prop) = fonts.families.get_mut(&FontFamily::Proportional) {
        prop.insert(0, "inter_regular".into());
    }
    if let Some(mono) = fonts.families.get_mut(&FontFamily::Monospace) {
        mono.insert(0, "jetbrains_regular".into());
    }

    fonts.families.insert(
        FontFamily::Name("display_bold".into()),
        vec!["space_grotesk_bold".into(), "inter_semibold".into()],
    );
    fonts.families.insert(
        FontFamily::Name("display_medium".into()),
        vec!["space_grotesk_medium".into(), "inter_regular".into()],
    );
    fonts.families.insert(
        FontFamily::Name("ui_semibold".into()),
        vec!["inter_semibold".into(), "inter_regular".into()],
    );
    fonts.families.insert(
        FontFamily::Name("mono_bold".into()),
        vec!["jetbrains_bold".into(), "jetbrains_regular".into()],
    );

    ctx.set_fonts(fonts);

    // 2. Apply Obsidian & Electric Violet visuals
    let mut visuals = egui::Visuals::dark();
    visuals.window_fill = PANEL;
    visuals.panel_fill = PANEL;
    visuals.extreme_bg_color = CANVAS;
    visuals.faint_bg_color = SURFACE;
    visuals.override_text_color = Some(TEXT);
    visuals.window_corner_radius = CornerRadius::same(12);
    visuals.window_stroke = Stroke::new(1.0, BORDER);
    visuals.menu_corner_radius = CornerRadius::same(10);

    visuals.widgets.noninteractive.bg_fill = PANEL;
    visuals.widgets.noninteractive.weak_bg_fill = SURFACE;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, MUTED);
    visuals.widgets.noninteractive.corner_radius = CornerRadius::same(8);

    visuals.widgets.inactive.bg_fill = SURFACE;
    visuals.widgets.inactive.weak_bg_fill = SURFACE;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    visuals.widgets.inactive.corner_radius = CornerRadius::same(8);

    visuals.widgets.hovered.bg_fill = SURFACE_HOVER;
    visuals.widgets.hovered.weak_bg_fill = SURFACE_HOVER;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, PRIMARY);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, TEXT);
    visuals.widgets.hovered.corner_radius = CornerRadius::same(8);

    visuals.widgets.active.bg_fill = PRIMARY;
    visuals.widgets.active.weak_bg_fill = PRIMARY;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, HIGHLIGHT);
    visuals.widgets.active.fg_stroke = Stroke::new(1.5, TEXT);
    visuals.widgets.active.corner_radius = CornerRadius::same(8);

    visuals.widgets.open.bg_fill = SURFACE_HOVER;
    visuals.widgets.open.weak_bg_fill = SURFACE_HOVER;
    visuals.widgets.open.bg_stroke = Stroke::new(1.0, PRIMARY);
    visuals.widgets.open.fg_stroke = Stroke::new(1.0, TEXT);
    visuals.widgets.open.corner_radius = CornerRadius::same(8);

    visuals.selection.bg_fill = primary_tint(68);
    visuals.selection.stroke = Stroke::new(1.0, PRIMARY);
    visuals.hyperlink_color = HIGHLIGHT;
    visuals.warn_fg_color = Color32::from_rgb(250, 204, 21);
    visuals.error_fg_color = ERROR;

    ctx.set_visuals(visuals);

    // 3. Refined spacing & typography ergonomics
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = Vec2::new(8.0, 6.0);
    style.spacing.button_padding = Vec2::new(10.0, 5.0);
    style.spacing.slider_width = 96.0;
    style.spacing.combo_width = 110.0;
    style.interaction.tooltip_delay = 0.25;
    ctx.set_style(style);
}

// =============================================================================
// PULSE CHECK APP ICON & GEOMETRIC WORDMARK
// =============================================================================

/// Draws the crisp, Retina-filtered Option A "Pulse Check" app icon texture (zero stroke-overlap artifacts).
pub fn draw_pulse_check_logo(ui: &mut Ui, size: f32) -> Response {
    let tex = pulse_check_texture(ui.ctx());
    ui.add(egui::Image::new((tex.id(), Vec2::splat(size))).fit_to_exact_size(Vec2::splat(size)))
}

/// Draws the Option A Geometric Precision wordmark: bold white "Beat" + medium electric-violet "Check".
pub fn draw_wordmark(ui: &mut Ui, size: f32) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.label(RichText::new("Beat").font(font_display_bold(size)).color(TEXT));
        ui.label(
            RichText::new("Check")
                .font(font_display_medium(size))
                .color(HIGHLIGHT),
        );
    });
}

// =============================================================================
// 05. MINIMALIST STROKE VECTOR ICONS (1.5px Geometric Lines — Zero Emojis!)
// =============================================================================
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum StrokeIcon {
    Play,
    Pause,
    Rewind,
    PrevBar,
    NextBar,
    Folder,
    Metronome,
    Loop,
    Volume,
    VolumeMute,
    Waveform,
    Scroll,
    Undo,
    Redo,
    ZoomIn,
    ZoomOut,
    Fit,
    Export,
    Sliders,
    DrawerOpen,
    DrawerClose,
    Copy,
    Save,
    Eye,
    Check,
    Reset,
    Trash,
}

pub fn paint_stroke_icon(painter: &egui::Painter, rect: Rect, icon: StrokeIcon, color: Color32) {
    let c = rect.center();
    let s = rect.width().min(rect.height());
    let p = |nx: f32, ny: f32| Pos2::new(rect.left() + nx * s, rect.top() + ny * s);
    let st = Stroke::new((s * 0.105).clamp(1.3, 1.85), color);

    match icon {
        StrokeIcon::Play => {
            let tri = vec![p(0.28, 0.18), p(0.82, 0.50), p(0.28, 0.82)];
            painter.add(Shape::convex_polygon(tri, color, Stroke::NONE));
        }
        StrokeIcon::Pause => {
            let w = s * 0.21;
            let h = s * 0.62;
            painter.rect_filled(
                Rect::from_center_size(Pos2::new(c.x - s * 0.19, c.y), Vec2::new(w, h)),
                1.5,
                color,
            );
            painter.rect_filled(
                Rect::from_center_size(Pos2::new(c.x + s * 0.19, c.y), Vec2::new(w, h)),
                1.5,
                color,
            );
        }
        StrokeIcon::Rewind => {
            painter.line_segment([p(0.22, 0.22), p(0.22, 0.78)], st);
            let tri = vec![p(0.76, 0.22), p(0.32, 0.50), p(0.76, 0.78)];
            painter.add(Shape::closed_line(tri, st));
        }
        StrokeIcon::PrevBar => {
            painter.add(Shape::line(vec![p(0.62, 0.22), p(0.34, 0.50), p(0.62, 0.78)], st));
        }
        StrokeIcon::NextBar => {
            painter.add(Shape::line(vec![p(0.38, 0.22), p(0.66, 0.50), p(0.38, 0.78)], st));
        }
        StrokeIcon::Folder => {
            let pts = vec![
                p(0.12, 0.26),
                p(0.40, 0.26),
                p(0.50, 0.36),
                p(0.88, 0.36),
                p(0.88, 0.78),
                p(0.12, 0.78),
            ];
            painter.add(Shape::closed_line(pts, st));
        }
        StrokeIcon::Metronome => {
            let body = vec![p(0.50, 0.14), p(0.80, 0.84), p(0.20, 0.84)];
            painter.add(Shape::closed_line(body, st));
            painter.line_segment([p(0.50, 0.70), p(0.74, 0.34)], st);
            painter.circle_filled(p(0.74, 0.34), (s * 0.08).max(1.4), color);
        }
        StrokeIcon::Loop => {
            painter.add(Shape::line(vec![p(0.20, 0.44), p(0.20, 0.30), p(0.76, 0.30)], st));
            painter.add(Shape::line(vec![p(0.64, 0.18), p(0.78, 0.30), p(0.64, 0.42)], st));
            painter.add(Shape::line(vec![p(0.80, 0.56), p(0.80, 0.70), p(0.24, 0.70)], st));
            painter.add(Shape::line(vec![p(0.36, 0.58), p(0.22, 0.70), p(0.36, 0.82)], st));
        }
        StrokeIcon::Volume => {
            let cone = vec![
                p(0.14, 0.40),
                p(0.32, 0.40),
                p(0.54, 0.20),
                p(0.54, 0.80),
                p(0.32, 0.60),
                p(0.14, 0.60),
            ];
            painter.add(Shape::closed_line(cone, st));
            painter.add(Shape::line(vec![p(0.68, 0.36), p(0.75, 0.50), p(0.68, 0.64)], st));
            painter.add(Shape::line(vec![p(0.80, 0.26), p(0.89, 0.50), p(0.80, 0.74)], st));
        }
        StrokeIcon::VolumeMute => {
            let cone = vec![
                p(0.14, 0.40),
                p(0.32, 0.40),
                p(0.52, 0.22),
                p(0.52, 0.78),
                p(0.32, 0.60),
                p(0.14, 0.60),
            ];
            painter.add(Shape::closed_line(cone, st));
            painter.line_segment([p(0.66, 0.36), p(0.88, 0.64)], st);
            painter.line_segment([p(0.88, 0.36), p(0.66, 0.64)], st);
        }
        StrokeIcon::Waveform => {
            let pts = vec![
                p(0.10, 0.50),
                p(0.24, 0.34),
                p(0.38, 0.66),
                p(0.52, 0.18),
                p(0.66, 0.82),
                p(0.78, 0.40),
                p(0.90, 0.50),
            ];
            painter.add(Shape::line(pts, st));
        }
        StrokeIcon::Scroll => {
            painter.rect_stroke(
                Rect::from_min_max(p(0.14, 0.26), p(0.86, 0.74)),
                2.0,
                st,
                StrokeKind::Inside,
            );
            painter.line_segment([p(0.50, 0.16), p(0.50, 0.84)], st);
        }
        StrokeIcon::Undo => {
            painter.add(Shape::line(vec![p(0.24, 0.36), p(0.66, 0.36), p(0.78, 0.52), p(0.66, 0.70), p(0.44, 0.70)], st));
            painter.add(Shape::line(vec![p(0.38, 0.22), p(0.22, 0.36), p(0.38, 0.50)], st));
        }
        StrokeIcon::Redo => {
            painter.add(Shape::line(vec![p(0.76, 0.36), p(0.34, 0.36), p(0.22, 0.52), p(0.34, 0.70), p(0.56, 0.70)], st));
            painter.add(Shape::line(vec![p(0.62, 0.22), p(0.78, 0.36), p(0.62, 0.50)], st));
        }
        StrokeIcon::ZoomIn => {
            painter.line_segment([p(0.22, 0.50), p(0.78, 0.50)], st);
            painter.line_segment([p(0.50, 0.22), p(0.50, 0.78)], st);
        }
        StrokeIcon::ZoomOut => {
            painter.line_segment([p(0.22, 0.50), p(0.78, 0.50)], st);
        }
        StrokeIcon::Fit => {
            painter.add(Shape::line(vec![p(0.18, 0.38), p(0.18, 0.18), p(0.38, 0.18)], st));
            painter.add(Shape::line(vec![p(0.62, 0.18), p(0.82, 0.18), p(0.82, 0.38)], st));
            painter.add(Shape::line(vec![p(0.82, 0.62), p(0.82, 0.82), p(0.62, 0.82)], st));
            painter.add(Shape::line(vec![p(0.38, 0.82), p(0.18, 0.82), p(0.18, 0.62)], st));
        }
        StrokeIcon::Export => {
            painter.line_segment([p(0.50, 0.66), p(0.50, 0.18)], st);
            painter.add(Shape::line(vec![p(0.30, 0.38), p(0.50, 0.18), p(0.70, 0.38)], st));
            painter.add(Shape::line(vec![p(0.18, 0.62), p(0.18, 0.82), p(0.82, 0.82), p(0.82, 0.62)], st));
        }
        StrokeIcon::Sliders => {
            painter.line_segment([p(0.14, 0.34), p(0.86, 0.34)], st);
            painter.circle_filled(p(0.36, 0.34), (s * 0.11).max(2.0), color);
            painter.line_segment([p(0.14, 0.68), p(0.86, 0.68)], st);
            painter.circle_filled(p(0.66, 0.68), (s * 0.11).max(2.0), color);
        }
        StrokeIcon::DrawerOpen => {
            painter.rect_stroke(
                Rect::from_min_max(p(0.14, 0.18), p(0.86, 0.82)),
                2.0,
                st,
                StrokeKind::Inside,
            );
            painter.line_segment([p(0.60, 0.18), p(0.60, 0.82)], st);
        }
        StrokeIcon::DrawerClose => {
            painter.line_segment([p(0.24, 0.24), p(0.76, 0.76)], st);
            painter.line_segment([p(0.76, 0.24), p(0.24, 0.76)], st);
        }
        StrokeIcon::Copy => {
            painter.rect_stroke(
                Rect::from_min_max(p(0.32, 0.16), p(0.84, 0.68)),
                2.0,
                st,
                StrokeKind::Inside,
            );
            painter.add(Shape::line(
                vec![p(0.22, 0.34), p(0.16, 0.34), p(0.16, 0.84), p(0.66, 0.84), p(0.66, 0.76)],
                st,
            ));
        }
        StrokeIcon::Save => {
            painter.rect_stroke(
                Rect::from_min_max(p(0.16, 0.16), p(0.84, 0.84)),
                2.0,
                st,
                StrokeKind::Inside,
            );
            painter.rect_stroke(
                Rect::from_min_max(p(0.30, 0.16), p(0.70, 0.42)),
                1.0,
                st,
                StrokeKind::Inside,
            );
            painter.rect_stroke(
                Rect::from_min_max(p(0.28, 0.56), p(0.72, 0.84)),
                1.0,
                st,
                StrokeKind::Inside,
            );
        }
        StrokeIcon::Eye => {
            painter.add(Shape::line(vec![p(0.12, 0.50), p(0.34, 0.26), p(0.66, 0.26), p(0.88, 0.50)], st));
            painter.add(Shape::line(vec![p(0.12, 0.50), p(0.34, 0.74), p(0.66, 0.74), p(0.88, 0.50)], st));
            painter.circle_stroke(c, s * 0.13, st);
        }
        StrokeIcon::Check => {
            painter.add(Shape::line(vec![p(0.18, 0.52), p(0.42, 0.76), p(0.84, 0.26)], st));
        }
        StrokeIcon::Reset => {
            // Circular counter-clockwise reset arrow
            let pts = vec![
                p(0.24, 0.36),
                p(0.36, 0.22),
                p(0.60, 0.20),
                p(0.78, 0.34),
                p(0.80, 0.58),
                p(0.66, 0.76),
                p(0.42, 0.78),
                p(0.26, 0.66),
            ];
            painter.add(Shape::line(pts, st));
            painter.add(Shape::line(vec![p(0.20, 0.20), p(0.22, 0.39), p(0.41, 0.37)], st));
        }
        StrokeIcon::Trash => {
            painter.line_segment([p(0.20, 0.28), p(0.80, 0.28)], st);
            painter.line_segment([p(0.38, 0.18), p(0.62, 0.18)], st);
            painter.rect_stroke(
                Rect::from_min_max(p(0.26, 0.28), p(0.74, 0.82)),
                1.5,
                st,
                StrokeKind::Inside,
            );
            painter.line_segment([p(0.42, 0.42), p(0.42, 0.68)], st);
            painter.line_segment([p(0.58, 0.42), p(0.58, 0.68)], st);
        }
    }
}

/// Minimalist Stroke Vector Button (Option A): 1px hairline border, transparent or violet tint when active.
pub fn stroke_button(
    ui: &mut Ui,
    icon: Option<StrokeIcon>,
    label: &str,
    active: bool,
    height: f32,
) -> Response {
    let font = if active { font_ui_semibold(12.0) } else { FontId::proportional(12.0) };
    let galley = ui.painter().layout_no_wrap(label.to_string(), font, TEXT);
    let icon_w = if icon.is_some() { 14.0 } else { 0.0 };
    let gap = if icon.is_some() && !label.is_empty() { 6.0 } else { 0.0 };
    let pad_x = if label.is_empty() { 7.0 } else { 10.0 };
    let width = pad_x * 2.0 + icon_w + gap + galley.size().x;

    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
    let hovered = resp.hovered();
    let enabled = ui.is_enabled();

    let (bg, border, fg) = if !enabled {
        (Color32::TRANSPARENT, Color32::from_rgb(32, 30, 46), Color32::from_rgb(80, 76, 102))
    } else if active {
        (
            if hovered { primary_tint(58) } else { primary_tint(40) },
            PRIMARY,
            HIGHLIGHT,
        )
    } else if hovered {
        (SURFACE_HOVER, PRIMARY, TEXT)
    } else {
        (SURFACE, BORDER, MUTED)
    };

    let painter = ui.painter();
    painter.rect_filled(rect, 7.0, bg);
    painter.rect_stroke(rect, 7.0, Stroke::new(1.0, border), StrokeKind::Inside);

    let mut x = rect.left() + pad_x;
    if let Some(ic) = icon {
        let icon_rect = Rect::from_center_size(Pos2::new(x + 7.0, rect.center().y), Vec2::splat(13.5));
        paint_stroke_icon(painter, icon_rect, ic, fg);
        x += icon_w + gap;
    }
    if !label.is_empty() {
        let text_pos = Pos2::new(x, rect.center().y - galley.size().y * 0.5);
        painter.galley(text_pos, galley, fg);
    }

    resp
}

/// Electric Violet Primary CTA Button with crisp white vector icon and bold label.
pub fn primary_cta_button(
    ui: &mut Ui,
    icon: Option<StrokeIcon>,
    label: &str,
    size: Vec2,
) -> Response {
    let font = font_ui_semibold(13.0);
    let galley = ui.painter().layout_no_wrap(label.to_string(), font, TEXT);
    let min_w = galley.size().x + if icon.is_some() { 44.0 } else { 28.0 };
    let desired = Vec2::new(size.x.max(min_w), size.y);

    let (rect, resp) = ui.allocate_exact_size(desired, Sense::click());
    let enabled = ui.is_enabled();
    let hovered = resp.hovered();

    let (bg, border, fg) = if !enabled {
        (SURFACE, BORDER, Color32::from_rgb(95, 90, 120))
    } else if resp.is_pointer_button_down_on() {
        (DEEP_VIOLET, HIGHLIGHT, TEXT)
    } else if hovered {
        (PRIMARY_HOVER, LAVENDER, TEXT)
    } else {
        (PRIMARY, HIGHLIGHT, TEXT)
    };

    let painter = ui.painter();
    painter.rect_filled(rect, 8.0, bg);
    painter.rect_stroke(rect, 8.0, Stroke::new(1.0, border), StrokeKind::Inside);

    let icon_w = if icon.is_some() { 14.0 } else { 0.0 };
    let gap = if icon.is_some() { 7.0 } else { 0.0 };
    let content_w = icon_w + gap + galley.size().x;
    let mut x = rect.center().x - content_w * 0.5;

    if let Some(ic) = icon {
        let icon_rect = Rect::from_center_size(Pos2::new(x + 7.0, rect.center().y), Vec2::splat(14.0));
        paint_stroke_icon(painter, icon_rect, ic, fg);
        x += icon_w + gap;
    }
    let text_pos = Pos2::new(x, rect.center().y - galley.size().y * 0.5);
    painter.galley(text_pos, galley, fg);

    resp
}
