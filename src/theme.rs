//! ArcCalc design tokens. One place for color, type, space, radius.
//! Visual language: tactical charcoal + warm amber (not ARCTracker's teal-blue).

use eframe::egui::{self, Color32, CornerRadius, Stroke, Vec2};

// ----- 4pt rhythm ------------------------------------------------------------
pub const SPACE_2: f32 = 2.0;
pub const SPACE_4: f32 = 4.0;
pub const SPACE_6: f32 = 6.0;
pub const SPACE_8: f32 = 8.0;
pub const SPACE_10: f32 = 10.0;
pub const SPACE_12: f32 = 12.0;
pub const SPACE_14: f32 = 14.0;
pub const SPACE_16: f32 = 16.0;
pub const SPACE_20: f32 = 20.0;

// ----- type ------------------------------------------------------------------
pub const TEXT_TITLE: f32 = 13.0;
pub const TEXT_BODY: f32 = 14.0;
pub const TEXT_BTN: f32 = 13.5;
pub const TEXT_CAPTION: f32 = 11.0;
pub const TEXT_MICRO: f32 = 10.5;
pub const TEXT_WIN: f32 = 12.0;

// ----- chrome ----------------------------------------------------------------
pub const WIN_W: f32 = 372.0;
pub const TITLE_H: f32 = 40.0;
pub const BODY_PAD_X: f32 = 18.0;
pub const BODY_PAD_Y: f32 = 14.0;
pub const CARD_H: f32 = 56.0;
/// Gap between Steam and Inventory (tighter).
pub const CARD_GAP: f32 = 8.0;
/// Black band between major blocks. Same value everywhere:
/// Inventory→Prepare Steam, Prepare Steam→settings card, last card→window bottom.
pub const BLOCK_GAP: f32 = 16.0;
pub const BTN_H: f32 = 42.0;
/// Quiet goals button under Sync now.
pub const OVERLAY_BTN_H: f32 = 34.0;
/// Settings card: sync, language, overlay layout, size, key, reset, Save.
pub const SETTINGS_PANEL_H: f32 = 514.0;

/// Full body stack height (must match draw_body exactly; item_spacing=0).
/// Always ends with BLOCK_GAP under the last block.
pub fn content_stack_h(settings_open: bool) -> f32 {
    // SETTINGS link + gap
    let mut h = 22.0 + SPACE_12;
    // Steam + Inventory
    h += CARD_H + CARD_GAP + CARD_H;
    // Inventory → Sync now → goals overlay
    h += BLOCK_GAP + BTN_H;
    h += SPACE_8 + OVERLAY_BTN_H;
    if settings_open {
        // Prepare Steam → UPDATE INTERVAL card
        h += BLOCK_GAP + SETTINGS_PANEL_H;
    }
    // Last block → window bottom (same as Inventory → Prepare Steam)
    h += BLOCK_GAP;
    h
}

/// Window height: border + title + top pad + stack (stack already includes bottom BLOCK_GAP).
pub fn window_h(settings_open: bool) -> f32 {
    TITLE_H + BODY_PAD_Y + content_stack_h(settings_open) + 2.0
}
pub const ICON_TILE: f32 = 32.0;
pub const LOGO: f32 = 20.0;
pub const WIN_BTN: f32 = 28.0;
pub const ACCENT_BAR: f32 = 3.0;

// ----- radii -----------------------------------------------------------------
pub const R_WIN: u8 = 12;
pub const R_CARD: u8 = 10;
pub const R_BTN: u8 = 9;
pub const R_TILE: u8 = 8;
pub const R_LOGO: u8 = 5;
pub const R_PILL: u8 = 6;

// ----- palette ---------------------------------------------------------------
pub fn bg() -> Color32 {
    Color32::from_rgb(0x0e, 0x0f, 0x13)
}
pub fn titlebar() -> Color32 {
    Color32::from_rgb(0x14, 0x15, 0x1a)
}
pub fn card() -> Color32 {
    Color32::from_rgb(0x1a, 0x1b, 0x22)
}
pub fn card_hover() -> Color32 {
    Color32::from_rgb(0x1f, 0x20, 0x28)
}
pub fn input() -> Color32 {
    Color32::from_rgb(0x16, 0x17, 0x1d)
}
pub fn border() -> Color32 {
    Color32::from_rgb(0x2c, 0x2e, 0x38)
}
pub fn border_soft() -> Color32 {
    Color32::from_rgb(0x24, 0x26, 0x2e)
}
pub fn hairline() -> Color32 {
    Color32::from_rgb(0x1e, 0x20, 0x27)
}

pub fn fg() -> Color32 {
    Color32::from_rgb(0xed, 0xed, 0xf0)
}
pub fn fg_secondary() -> Color32 {
    Color32::from_rgb(0x9a, 0x9b, 0xa4)
}
pub fn fg_dim() -> Color32 {
    Color32::from_rgb(0x7a, 0x7c, 0x86)
}
pub fn fg_soft() -> Color32 {
    Color32::from_rgb(0xb7, 0xb8, 0xc0)
}

/// Brand amber
pub fn accent() -> Color32 {
    Color32::from_rgb(0xe8, 0xa3, 0x3d)
}
pub fn accent_dim() -> Color32 {
    Color32::from_rgb(0xb8, 0x7e, 0x28)
}
pub fn accent_fg() -> Color32 {
    Color32::from_rgb(0x2b, 0x1c, 0x05)
}
pub fn accent_soft_fill() -> Color32 {
    Color32::from_rgba_unmultiplied(0xe8, 0xa3, 0x3d, 28)
}

pub fn success() -> Color32 {
    Color32::from_rgb(0x5a, 0xc8, 0x8e)
}
pub fn success_soft() -> Color32 {
    Color32::from_rgba_unmultiplied(0x5a, 0xc8, 0x8e, 28)
}
pub fn warning() -> Color32 {
    Color32::from_rgb(0xe8, 0xa3, 0x3d)
}
pub fn danger() -> Color32 {
    Color32::from_rgb(0xe0, 0x5a, 0x54)
}
pub fn idle_bar() -> Color32 {
    Color32::from_rgb(0x3a, 0x3c, 0x46)
}

pub const FONT_SANS: &str = "Inter";
pub const FONT_MONO: &str = "JetBrainsMono";

pub fn apply(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.dark_mode = true;
    visuals.panel_fill = bg();
    visuals.window_fill = bg();
    visuals.extreme_bg_color = input();
    visuals.faint_bg_color = card();
    visuals.override_text_color = Some(fg());
    visuals.selection.bg_fill = accent();
    visuals.selection.stroke = Stroke::new(1.0, accent_fg());
    visuals.widgets.noninteractive.bg_fill = card();
    visuals.widgets.inactive.bg_fill = input();
    visuals.widgets.hovered.bg_fill = card_hover();
    visuals.widgets.active.bg_fill = card_hover();
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, fg());
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, fg());
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, fg());
    visuals.widgets.active.fg_stroke = Stroke::new(1.0, fg());
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, border_soft());
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, border());
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, border());
    for w in [
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
        &mut visuals.widgets.noninteractive,
    ] {
        w.corner_radius = CornerRadius::same(R_BTN);
    }
    visuals.window_shadow = egui::Shadow::NONE;
    visuals.popup_shadow = egui::Shadow::NONE;
    visuals.window_stroke = Stroke::new(1.0, border());
    visuals.window_corner_radius = CornerRadius::same(R_WIN);

    let mut style = (*ctx.style()).clone();
    style.visuals = visuals;
    style.spacing.item_spacing = Vec2::new(SPACE_8, SPACE_6);
    style.spacing.button_padding = Vec2::new(SPACE_12, SPACE_8);
    style.spacing.window_margin = egui::Margin::same(0);
    style.interaction.tooltip_delay = 0.35;
    ctx.set_style(style);
}
