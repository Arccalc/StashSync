//! ArcCalc Helper UI — polished egui chrome (same approach as ARCTracker Sync).

use std::f32::consts::TAU;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use eframe::egui::{
    self, pos2, vec2, Align, Align2, Color32, CornerRadius, FontData, FontDefinitions, FontFamily,
    FontId, Frame, Layout, Margin, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Ui, Vec2,
};

use crate::config::HelperConfig;
use crate::i18n::{self, Key, Lang};
use crate::state::{ConnState, SharedState};
use crate::theme::{self, *};

pub struct HelperApp {
    state: Arc<SharedState>,
    poll_minutes: f32,
    interval_text: String,
    config: HelperConfig,
    show_settings: bool,
    lang: Lang,
}

impl HelperApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        state: Arc<SharedState>,
        config: HelperConfig,
    ) -> Self {
        install_fonts(&cc.egui_ctx);
        theme::apply(&cc.egui_ctx);
        let poll_minutes = (config.poll_interval_secs as f32 / 60.0).clamp(1.0, 30.0);
        let interval_text = minutes_label(poll_minutes);
        let lang = Lang::parse(&config.language);
        Self {
            state,
            poll_minutes,
            interval_text,
            config,
            show_settings: false,
            lang,
        }
    }

    fn persist(&mut self) {
        self.config.save();
    }

    fn set_auto(&mut self, on: bool) {
        if self.config.auto_sync == on {
            return;
        }
        self.config.auto_sync = on;
        self.state.auto_sync.store(on, Ordering::Relaxed);
        if on {
            self.state.force_poll.store(true, Ordering::Relaxed);
        }
        self.persist();
    }

    fn set_startup(&mut self, on: bool) {
        if self.config.run_at_startup == on {
            return;
        }
        self.config.run_at_startup = on;
        crate::config::apply_run_at_startup(on);
        self.persist();
    }

    fn set_lang(&mut self, lang: Lang) {
        if self.lang == lang {
            return;
        }
        self.lang = lang;
        self.config.language = lang.code().to_string();
        self.persist();
    }

    fn save_interval(&mut self) {
        commit_minutes(&self.interval_text, &mut self.poll_minutes);
        self.poll_minutes = self.poll_minutes.clamp(1.0, 30.0).round();
        self.interval_text = minutes_label(self.poll_minutes);
        let secs = (self.poll_minutes as u64 * 60)
            .clamp(HelperConfig::min_poll(), HelperConfig::max_poll());
        self.config.poll_interval_secs = secs;
        self.state.poll_interval_secs.store(secs, Ordering::Relaxed);
        self.config.save();
        self.state
            .log(format!("Update interval: {} min", self.poll_minutes as i32));
    }

    fn window_height(&self) -> f32 {
        window_h(self.show_settings)
    }
}

impl eframe::App for HelperApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint_after(std::time::Duration::from_millis(350));

        let h = self.window_height();
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(vec2(WIN_W, h)));
        ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(vec2(WIN_W, h)));
        ctx.send_viewport_cmd(egui::ViewportCommand::MaxInnerSize(vec2(WIN_W, h)));
        // Never allow maximize — this is a fixed compact utility.
        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(false));

        let snap = self.state.snapshot();

        // Outer window border (borderless chrome)
        egui::CentralPanel::default()
            .frame(Frame::NONE.fill(bg()).inner_margin(Margin::ZERO))
            .show(ctx, |ui| {
                let outer = ui.max_rect();
                ui.painter().rect_stroke(
                    outer.shrink(0.5),
                    CornerRadius::same(R_WIN),
                    Stroke::new(1.0, border()),
                    StrokeKind::Inside,
                );

                let content = outer.shrink(1.0);
                let title_rect =
                    Rect::from_min_size(content.min, vec2(content.width(), TITLE_H));
                draw_titlebar(ui, ctx, title_rect);

                // Full body under title — bottom gap is real ui.add_space(BLOCK_GAP)
                // at the end of draw_body (same call as Inventory → Prepare Steam).
                let body =
                    Rect::from_min_max(pos2(content.left(), title_rect.bottom()), content.max);

                ui.allocate_ui_at_rect(body, |ui| {
                    ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                    Frame::NONE
                        .fill(bg())
                        .inner_margin(Margin {
                            left: BODY_PAD_X as i8,
                            right: BODY_PAD_X as i8,
                            top: BODY_PAD_Y as i8,
                            bottom: 0,
                        })
                        .show(ui, |ui| {
                            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                            ui.set_width(ui.available_width());
                            draw_body(ui, &snap, self);
                        });
                });
            });
    }
}

// =============================================================================
// Title bar
// =============================================================================

fn draw_titlebar(ui: &mut Ui, ctx: &egui::Context, rect: Rect) {
    ui.painter()
        .rect_filled(rect, CornerRadius::ZERO, titlebar());
    ui.painter().hline(
        rect.x_range(),
        rect.bottom() - 0.5,
        Stroke::new(1.0, hairline()),
    );

    let drag = ui.interact(rect, ui.id().with("drag"), Sense::click_and_drag());
    if drag.dragged() {
        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }

    // Logo tile
    let logo = Rect::from_center_size(
        pos2(rect.left() + BODY_PAD_X + LOGO * 0.5, rect.center().y),
        vec2(LOGO, LOGO),
    );
    ui.painter()
        .rect_filled(logo, CornerRadius::same(R_LOGO), accent());
    ui.painter().rect_stroke(
        logo,
        CornerRadius::same(R_LOGO),
        Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 20)),
        StrokeKind::Inside,
    );
    paint_emblem(ui, logo);

    ui.painter().text(
        pos2(logo.right() + SPACE_8, rect.center().y),
        Align2::LEFT_CENTER,
        "StashSync",
        FontId::new(TEXT_TITLE, FontFamily::Name(FONT_SANS.into())),
        fg(),
    );

    // Only minimize + close (no maximize / expand)
    let mut x = rect.right() - BODY_PAD_X;
    x = paint_win_btn(ui, x, rect.center().y, WinIcon::Close, ctx);
    let _ = paint_win_btn(ui, x, rect.center().y, WinIcon::Min, ctx);
}

enum WinIcon {
    Min,
    Close,
}

fn paint_win_btn(ui: &mut Ui, right: f32, cy: f32, kind: WinIcon, ctx: &egui::Context) -> f32 {
    let size = WIN_BTN;
    let rect = Rect::from_center_size(pos2(right - size * 0.5, cy), vec2(size, size));
    let id = ui.id().with(match kind {
        WinIcon::Min => "min",
        WinIcon::Close => "close",
    });
    let resp = ui.interact(rect, id, Sense::click());
    let hovered = resp.hovered();

    if hovered {
        let fill = match kind {
            WinIcon::Close => Color32::from_rgba_unmultiplied(0xe0, 0x5a, 0x54, 40),
            WinIcon::Min => Color32::from_rgba_unmultiplied(255, 255, 255, 12),
        };
        ui.painter()
            .rect_filled(rect, CornerRadius::same(6), fill);
    }

    let col = if hovered {
        match kind {
            WinIcon::Close => danger(),
            WinIcon::Min => fg_soft(),
        }
    } else {
        fg_dim()
    };
    let c = rect.center();
    let s = Stroke::new(1.2, col);
    match kind {
        WinIcon::Min => {
            ui.painter()
                .line_segment([pos2(c.x - 4.0, c.y), pos2(c.x + 4.0, c.y)], s);
        }
        WinIcon::Close => {
            ui.painter()
                .line_segment([pos2(c.x - 3.5, c.y - 3.5), pos2(c.x + 3.5, c.y + 3.5)], s);
            ui.painter()
                .line_segment([pos2(c.x + 3.5, c.y - 3.5), pos2(c.x - 3.5, c.y + 3.5)], s);
        }
    }

    if resp.clicked() {
        match kind {
            WinIcon::Close => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            WinIcon::Min => ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true)),
        }
    }
    right - size
}

// =============================================================================
// Body
// =============================================================================

fn draw_body(ui: &mut Ui, snap: &crate::state::StatusSnapshot, app: &mut HelperApp) {
    // Explicit gaps only — no automatic item_spacing (must match content_stack_h).
    ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
    let full_w = ui.available_width();

    // SETTINGS link — top right of body
    ui.allocate_ui_with_layout(
        vec2(full_w, 22.0),
        Layout::right_to_left(Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            let label = if app.show_settings {
                format!("{}  ▴", app.lang.text(Key::Settings))
            } else {
                app.lang.text(Key::Settings).to_string()
            };
            let (id, rect) = ui.allocate_space(vec2(168.0, 22.0));
            let resp = ui.interact(rect, id, Sense::click());
            let col = if resp.hovered() {
                fg_soft()
            } else {
                fg_dim()
            };
            if resp.hovered() {
                ui.painter().rect_filled(
                    rect,
                    CornerRadius::same(R_PILL),
                    Color32::from_rgba_unmultiplied(255, 255, 255, 8),
                );
            }
            // gear
            paint_gear(ui, pos2(rect.left() + 10.0, rect.center().y), col);
            ui.painter().text(
                pos2(rect.left() + 22.0, rect.center().y),
                Align2::LEFT_CENTER,
                label,
                FontId::new(TEXT_MICRO, FontFamily::Name(FONT_MONO.into())),
                col,
            );
            if resp.clicked() {
                app.show_settings = !app.show_settings;
            }
        },
    );

    ui.add_space(SPACE_12);

    // Status cards
    let (s_active, s_icon, s_tone, s_text) = steam_visual(snap, app.lang);
    let (i_active, i_icon, i_tone, i_text) = inventory_visual(snap, app.lang);

    status_card(ui, full_w, s_active, s_icon, s_tone, "STEAM", &s_text);
    ui.add_space(CARD_GAP);
    status_card(
        ui,
        full_w,
        i_active,
        i_icon,
        i_tone,
        app.lang.text(Key::Inventory),
        &i_text,
    );

    // Screenshot_11 red arrows: black band Inventory → Prepare Steam
    ui.add_space(BLOCK_GAP);

    // Sign-in needs Steam already open.
    let syncing = snap.steam == ConnState::Working || snap.inventory == ConnState::Working;
    let (cta, enabled) = if syncing {
        (app.lang.text(Key::Syncing), false)
    } else {
        (app.lang.text(Key::SyncNow), true)
    };
    if primary_cta(ui, full_w, cta, enabled) {
        app.state.force_poll.store(true, Ordering::Relaxed);
    }

    // Settings open: same BLOCK_GAP above the UPDATE INTERVAL card.
    if app.show_settings {
        ui.add_space(BLOCK_GAP);
        let mut save = false;
        let mut auto = app.config.auto_sync;
        let mut startup = app.config.run_at_startup;
        let mut lang = app.lang;
        settings_panel(
            ui,
            full_w,
            app.lang,
            &mut app.poll_minutes,
            &mut app.interval_text,
            &mut auto,
            &mut startup,
            &mut lang,
            &mut save,
        );
        if auto != app.config.auto_sync {
            app.set_auto(auto);
        }
        if startup != app.config.run_at_startup {
            app.set_startup(startup);
        }
        if lang != app.lang {
            app.set_lang(lang);
        }
        if save {
            app.save_interval();
        }
    }

    // Bottom band under last block = IDENTICAL to Inventory → Prepare Steam.
    ui.add_space(BLOCK_GAP);
}

// ----- status cards ----------------------------------------------------------

#[derive(Clone, Copy)]
enum StatusIcon {
    Warning,
    Clock,
    Check,
    Spinner,
    Error,
}

fn steam_visual(
    snap: &crate::state::StatusSnapshot,
    lang: Lang,
) -> (bool, StatusIcon, Color32, String) {
    match snap.steam {
        ConnState::Ok => (false, StatusIcon::Check, success(), lang.text(Key::SignedIn).into()),
        ConnState::Working => (
            true,
            StatusIcon::Spinner,
            warning(),
            lang.text(Key::AskingSteam).into(),
        ),
        ConnState::Error => (
            true,
            StatusIcon::Error,
            danger(),
            short(&snap.steam_detail, lang.text(Key::Error)),
        ),
        _ => (true, StatusIcon::Warning, accent(), lang.text(Key::OpenSteam).into()),
    }
}

fn inventory_visual(
    snap: &crate::state::StatusSnapshot,
    lang: Lang,
) -> (bool, StatusIcon, Color32, String) {
    match snap.inventory {
        ConnState::Ok if snap.mapped_keys > 0 => {
            let when = snap
                .last_sync
                .map(|t| t.format("%H:%M").to_string())
                .unwrap_or_else(|| "—".into());
            (
                false,
                StatusIcon::Check,
                success(),
                i18n::synced_line(lang, snap.mapped_keys, &when),
            )
        }
        ConnState::Ok => (false, StatusIcon::Check, success(), lang.text(Key::Synced).into()),
        ConnState::Working => (
            true,
            StatusIcon::Spinner,
            warning(),
            lang.text(Key::Syncing).into(),
        ),
        ConnState::Error if snap.inventory_detail == crate::i18n::BUILD_NOT_READY => (
            true,
            StatusIcon::Error,
            danger(),
            lang.text(Key::BuildNotReady).into(),
        ),
        ConnState::Error => (
            true,
            StatusIcon::Error,
            danger(),
            short(&snap.inventory_detail, lang.text(Key::SyncFailed)),
        ),
        _ if snap.token == ConnState::Ok => {
            (false, StatusIcon::Clock, fg_dim(), lang.text(Key::Waiting).into())
        }
        _ => (false, StatusIcon::Clock, fg_dim(), lang.text(Key::OpenSteam).into()),
    }
}

fn short(s: &str, fallback: &str) -> String {
    if s.is_empty() {
        fallback.into()
    } else if s.len() > 42 {
        format!("{}…", &s[..40])
    } else {
        s.into()
    }
}

fn status_card(
    ui: &mut Ui,
    full_w: f32,
    active: bool,
    icon: StatusIcon,
    tone: Color32,
    section: &str,
    detail: &str,
) {
    let (id, rect) = ui.allocate_space(vec2(full_w, CARD_H));
    let resp = ui.interact(rect, id, Sense::hover());

    let fill = if resp.hovered() {
        card_hover()
    } else {
        card()
    };
    // Active attention: full opacity; waiting: slightly muted
    let alpha = if active { 1.0 } else { 0.72 };

    ui.painter().rect_filled(
        rect,
        CornerRadius {
            nw: 0,
            ne: R_CARD,
            sw: 0,
            se: R_CARD,
        },
        with_a(fill, alpha),
    );

    // Left accent rail
    let bar_col = if active { tone } else { idle_bar() };
    ui.painter().rect_filled(
        Rect::from_min_max(
            pos2(rect.left(), rect.top() + 1.0),
            pos2(rect.left() + ACCENT_BAR, rect.bottom() - 1.0),
        ),
        CornerRadius::ZERO,
        bar_col,
    );

    // Soft icon tile
    let tile = Rect::from_center_size(
        pos2(rect.left() + SPACE_14 + ICON_TILE * 0.5 + 2.0, rect.center().y),
        vec2(ICON_TILE, ICON_TILE),
    );
    ui.painter().rect_filled(
        tile,
        CornerRadius::same(R_TILE),
        with_a(tone, if active { 0.14 } else { 0.08 }),
    );
    paint_status_icon(ui, tile.center(), icon, with_a(tone, alpha.max(0.85)));

    // Labels
    let tx = tile.right() + SPACE_12;
    ui.painter().text(
        pos2(tx, rect.top() + 17.0),
        Align2::LEFT_CENTER,
        section,
        FontId::new(TEXT_MICRO, FontFamily::Name(FONT_MONO.into())),
        with_a(fg_dim(), alpha),
    );
    ui.painter().text(
        pos2(tx, rect.top() + 36.0),
        Align2::LEFT_CENTER,
        detail,
        FontId::new(TEXT_BODY, FontFamily::Name(FONT_SANS.into())),
        with_a(fg(), alpha),
    );
}

fn paint_status_icon(ui: &Ui, c: Pos2, icon: StatusIcon, color: Color32) {
    let s = Stroke::new(1.65, color);
    match icon {
        StatusIcon::Warning | StatusIcon::Error => {
            let top = pos2(c.x, c.y - 7.5);
            let bl = pos2(c.x - 7.5, c.y + 6.5);
            let br = pos2(c.x + 7.5, c.y + 6.5);
            ui.painter().line_segment([top, bl], s);
            ui.painter().line_segment([bl, br], s);
            ui.painter().line_segment([br, top], s);
            ui.painter()
                .line_segment([pos2(c.x, c.y - 2.5), pos2(c.x, c.y + 1.5)], s);
            ui.painter().circle_filled(pos2(c.x, c.y + 4.5), 1.15, color);
        }
        StatusIcon::Clock => {
            ui.painter().circle_stroke(c, 7.5, s);
            ui.painter()
                .line_segment([c, pos2(c.x, c.y - 4.5)], s);
            ui.painter()
                .line_segment([c, pos2(c.x + 3.8, c.y + 1.2)], s);
        }
        StatusIcon::Check => {
            ui.painter().line_segment(
                [pos2(c.x - 5.0, c.y + 0.5), pos2(c.x - 1.2, c.y + 4.2)],
                s,
            );
            ui.painter().line_segment(
                [pos2(c.x - 1.2, c.y + 4.2), pos2(c.x + 6.0, c.y - 4.0)],
                s,
            );
        }
        StatusIcon::Spinner => {
            // arc-ish spinner (static elegant ring with gap)
            ui.painter().circle_stroke(c, 7.0, Stroke::new(1.5, with_a(color, 0.35)));
            // partial arc via short segments
            for i in 0..10 {
                let a0 = -1.2 + (i as f32) * 0.18;
                let a1 = a0 + 0.14;
                let p0 = pos2(c.x + a0.cos() * 7.0, c.y + a0.sin() * 7.0);
                let p1 = pos2(c.x + a1.cos() * 7.0, c.y + a1.sin() * 7.0);
                ui.painter().line_segment([p0, p1], s);
            }
        }
    }
}

// ----- CTA -------------------------------------------------------------------

fn primary_cta(ui: &mut Ui, full_w: f32, label: &str, enabled: bool) -> bool {
    let (id, rect) = ui.allocate_space(vec2(full_w, BTN_H));
    let resp = ui.interact(
        rect,
        id,
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );

    let fill = if !enabled {
        input()
    } else if resp.is_pointer_button_down_on() {
        accent_dim()
    } else if resp.hovered() {
        // slightly brighter hover
        Color32::from_rgb(0xf0, 0xb0, 0x4a)
    } else {
        accent()
    };
    let text_col = if enabled { accent_fg() } else { fg_dim() };

    ui.painter()
        .rect_filled(rect, CornerRadius::same(R_BTN), fill);
    // top highlight edge when enabled
    if enabled {
        ui.painter().hline(
            rect.x_range().shrink(10.0),
            rect.top() + 1.0,
            Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 28)),
        );
    }

    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        label,
        FontId::new(TEXT_BTN, FontFamily::Name(FONT_SANS.into())),
        text_col,
    );

    let clicked = enabled && resp.clicked();
    if enabled && resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    clicked
}

// ----- settings --------------------------------------------------------------

/// Same horizontal geometry as `status_card`: allocate exactly `full_w`.
fn settings_panel(
    ui: &mut Ui,
    full_w: f32,
    lang: Lang,
    poll_minutes: &mut f32,
    interval_text: &mut String,
    auto_sync: &mut bool,
    startup: &mut bool,
    language: &mut Lang,
    save_clicked: &mut bool,
) {
    let panel_h = SETTINGS_PANEL_H;
    let (id, rect) = ui.allocate_space(vec2(full_w, panel_h));
    let _ = id;

    ui.painter().rect_filled(
        rect,
        CornerRadius {
            nw: 0,
            ne: R_CARD,
            sw: 0,
            se: R_CARD,
        },
        card(),
    );
    ui.painter().rect_filled(
        Rect::from_min_max(
            pos2(rect.left(), rect.top() + 1.0),
            pos2(rect.left() + ACCENT_BAR, rect.bottom() - 1.0),
        ),
        CornerRadius::ZERO,
        idle_bar(),
    );

    let pad_x = 16.0;
    let pad_y = 12.0;
    let content = Rect::from_min_max(
        pos2(rect.left() + pad_x, rect.top() + pad_y),
        pos2(rect.right() - pad_x, rect.bottom() - pad_y),
    );

    let auto_row = Rect::from_min_size(content.min, vec2(content.width(), 28.0));
    if toggle_row(ui, auto_row, *auto_sync, lang.text(Key::AutoSync), "auto_sync") {
        *auto_sync = !*auto_sync;
    }

    ui.painter().text(
        pos2(content.left(), content.top() + 28.0 + 8.0 + 8.0),
        Align2::LEFT_CENTER,
        lang.text(Key::Interval),
        FontId::new(TEXT_MICRO, FontFamily::Name(FONT_SANS.into())),
        if *auto_sync { fg_dim() } else { with_a(fg_dim(), 0.45) },
    );

    let step = Rect::from_min_size(
        pos2(content.left(), content.top() + 28.0 + 8.0 + 16.0 + 2.0),
        vec2(content.width(), 36.0),
    );
    if *auto_sync {
        interval_stepper_in_rect(ui, step, poll_minutes, interval_text, lang.text(Key::Min));
    } else {
        paint_idle_stepper(ui, step, interval_text, lang.text(Key::Min));
    }

    let startup_row = Rect::from_min_size(
        pos2(content.left(), content.top() + 100.0),
        vec2(content.width(), 28.0),
    );
    if toggle_row(ui, startup_row, *startup, lang.text(Key::Startup), "startup") {
        *startup = !*startup;
    }

    let lang_row = Rect::from_min_size(
        pos2(content.left(), content.top() + 136.0),
        vec2(content.width(), 36.0),
    );
    language_row(ui, lang_row, lang, language);

    let save_h = 34.0;
    let save = Rect::from_min_max(
        pos2(content.left(), content.bottom() - save_h),
        pos2(content.right(), content.bottom()),
    );
    let sresp = ui.interact(save, ui.id().with("save_interval"), Sense::click());
    let sfill = if sresp.is_pointer_button_down_on() {
        accent_dim()
    } else if sresp.hovered() {
        Color32::from_rgb(0xf0, 0xb0, 0x4a)
    } else {
        accent()
    };
    ui.painter()
        .rect_filled(save, CornerRadius::same(R_BTN), sfill);
    ui.painter().text(
        save.center(),
        Align2::CENTER_CENTER,
        lang.text(Key::Save),
        FontId::new(13.0, FontFamily::Name(FONT_SANS.into())),
        accent_fg(),
    );
    if sresp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if sresp.clicked() {
        commit_minutes(interval_text, poll_minutes);
        *interval_text = minutes_label(*poll_minutes);
        *save_clicked = true;
    }
}

fn toggle_row(ui: &mut Ui, rect: Rect, on: bool, label: &str, salt: &str) -> bool {
    let resp = ui.interact(rect, ui.id().with(salt), Sense::click());
    let box_side = 16.0;
    let mark = Rect::from_center_size(
        pos2(rect.left() + box_side * 0.5, rect.center().y),
        vec2(box_side, box_side),
    );
    let fill = if on { accent() } else { input() };
    ui.painter()
        .rect_filled(mark, CornerRadius::same(4), fill);
    ui.painter().rect_stroke(
        mark,
        CornerRadius::same(4),
        Stroke::new(1.0, if on { accent() } else { border() }),
        StrokeKind::Inside,
    );
    if on {
        let s = Stroke::new(1.6, accent_fg());
        let c = mark.center();
        ui.painter().line_segment(
            [pos2(c.x - 4.0, c.y + 0.5), pos2(c.x - 1.2, c.y + 3.2)],
            s,
        );
        ui.painter().line_segment(
            [pos2(c.x - 1.2, c.y + 3.2), pos2(c.x + 4.2, c.y - 3.0)],
            s,
        );
    }
    ui.painter().text(
        pos2(mark.right() + 8.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::new(TEXT_BODY, FontFamily::Name(FONT_SANS.into())),
        if resp.hovered() { fg() } else { fg_soft() },
    );
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.clicked()
}

fn language_row(ui: &mut Ui, rect: Rect, ui_lang: Lang, language: &mut Lang) {
    ui.painter().text(
        pos2(rect.left(), rect.center().y),
        Align2::LEFT_CENTER,
        ui_lang.text(Key::Language),
        FontId::new(TEXT_BODY, FontFamily::Name(FONT_SANS.into())),
        fg_soft(),
    );
    let combo = Rect::from_min_max(pos2(rect.left() + 96.0, rect.top()), rect.max);
    let resp = ui.allocate_ui_at_rect(combo, |ui| {
        ui.set_min_height(rect.height());
        egui::ComboBox::from_id_salt("stashsync_lang")
            .width(combo.width())
            .selected_text(language.name())
            .show_ui(ui, |ui| {
                for choice in Lang::all() {
                    ui.selectable_value(language, choice, choice.name());
                }
            });
    });
    let _ = resp;
}

fn paint_idle_stepper(ui: &Ui, row: Rect, interval_text: &str, unit: &str) {
    ui.painter()
        .rect_filled(row, CornerRadius::same(R_TILE), with_a(input(), 0.55));
    ui.painter().text(
        row.center(),
        Align2::CENTER_CENTER,
        format!("{interval_text} {unit}"),
        FontId::new(16.0, FontFamily::Name(FONT_SANS.into())),
        with_a(fg_dim(), 0.55),
    );
}

fn minutes_label(minutes: f32) -> String {
    format!("{}", minutes.round().clamp(1.0, 30.0) as i32)
}

fn commit_minutes(text: &str, poll_minutes: &mut f32) {
    let digits: String = text.chars().filter(|c| c.is_ascii_digit()).collect();
    if let Ok(n) = digits.parse::<u32>() {
        if n > 0 {
            *poll_minutes = (n as f32).clamp(1.0, 30.0);
        }
    }
}

fn step_minutes(poll_minutes: &mut f32, interval_text: &mut String, delta: f32) {
    let base = poll_minutes.round().clamp(1.0, 30.0);
    *poll_minutes = (base + delta).clamp(1.0, 30.0);
    *interval_text = minutes_label(*poll_minutes);
}

/// − / minutes / + . The number is typed in place, on the same dark row.
fn interval_stepper_in_rect(
    ui: &mut Ui,
    row: Rect,
    poll_minutes: &mut f32,
    interval_text: &mut String,
    unit: &str,
) {
    let btn = 34.0;

    ui.painter()
        .rect_filled(row, CornerRadius::same(R_TILE), input());
    ui.painter().rect_stroke(
        row,
        CornerRadius::same(R_TILE),
        Stroke::new(1.0, border_soft()),
        StrokeKind::Inside,
    );

    let cy = row.center().y;
    let left_btn =
        Rect::from_center_size(pos2(row.left() + 8.0 + btn * 0.5, cy), vec2(btn, btn));
    let right_btn =
        Rect::from_center_size(pos2(row.right() - 8.0 - btn * 0.5, cy), vec2(btn, btn));
    let value_rect = Rect::from_min_max(
        pos2(left_btn.right() + 6.0, row.top()),
        pos2(right_btn.left() - 6.0, row.bottom()),
    );

    if step_btn(ui, left_btn, "−", ui.id().with("int_minus")) {
        step_minutes(poll_minutes, interval_text, -1.0);
    }
    if step_btn(ui, right_btn, "+", ui.id().with("int_plus")) {
        step_minutes(poll_minutes, interval_text, 1.0);
    }

    let num_w = 36.0;
    let group_w = num_w + 28.0;
    let group = Rect::from_center_size(value_rect.center(), vec2(group_w, value_rect.height()));
    let num_rect = Rect::from_min_size(
        pos2(group.left(), group.top()),
        vec2(num_w, group.height()),
    );

    let resp = ui.allocate_ui_at_rect(num_rect, |ui| {
        ui.style_mut().visuals.widgets.inactive.bg_fill = Color32::TRANSPARENT;
        ui.style_mut().visuals.widgets.hovered.bg_fill = Color32::TRANSPARENT;
        ui.style_mut().visuals.widgets.active.bg_fill = Color32::TRANSPARENT;
        ui.style_mut().visuals.widgets.inactive.bg_stroke = Stroke::NONE;
        ui.style_mut().visuals.widgets.hovered.bg_stroke = Stroke::NONE;
        ui.style_mut().visuals.widgets.active.bg_stroke = Stroke::NONE;
        ui.style_mut().visuals.selection.bg_fill = accent_soft_fill();
        ui.style_mut().visuals.selection.stroke = Stroke::new(1.0, fg());
        ui.add(
            egui::TextEdit::singleline(interval_text)
                .id(ui.id().with("interval_minutes"))
                .font(FontId::new(16.0, FontFamily::Name(FONT_SANS.into())))
                .text_color(fg())
                .horizontal_align(Align::Center)
                .vertical_align(Align::Center)
                .frame(false)
                .margin(Margin::symmetric(0, 8))
                .desired_width(num_rect.width())
                .char_limit(2)
                .background_color(Color32::TRANSPARENT),
        )
    });
    let resp = resp.inner;

    if resp.changed() {
        let digits: String = interval_text
            .chars()
            .filter(|c| c.is_ascii_digit())
            .take(2)
            .collect();
        if digits != *interval_text {
            *interval_text = digits;
        }
    }
    if resp.lost_focus() {
        commit_minutes(interval_text, poll_minutes);
        *interval_text = minutes_label(*poll_minutes);
    } else if !resp.has_focus() {
        let label = minutes_label(*poll_minutes);
        if interval_text != &label {
            *interval_text = label;
        }
    }
    if resp.gained_focus() {
        if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), resp.id) {
            let n = interval_text.chars().count();
            state.cursor.set_char_range(Some(egui::text::CCursorRange::two(
                egui::text::CCursor::new(0),
                egui::text::CCursor::new(n),
            )));
            state.store(ui.ctx(), resp.id);
        }
    }

    ui.painter().text(
        pos2(num_rect.right() + 2.0, cy),
        Align2::LEFT_CENTER,
        unit,
        FontId::new(TEXT_CAPTION, FontFamily::Name(FONT_SANS.into())),
        fg_dim(),
    );
    if resp.has_focus() {
        ui.painter().hline(
            num_rect.x_range(),
            num_rect.bottom() - 7.0,
            Stroke::new(1.0, accent()),
        );
    }
}

fn step_btn(ui: &mut Ui, rect: Rect, glyph: &str, id: egui::Id) -> bool {
    let resp = ui.interact(rect, id, Sense::click());
    let fill = if resp.is_pointer_button_down_on() {
        card_hover()
    } else if resp.hovered() {
        Color32::from_rgba_unmultiplied(255, 255, 255, 14)
    } else {
        Color32::TRANSPARENT
    };
    if fill.a() > 0 {
        ui.painter()
            .rect_filled(rect.shrink(2.0), CornerRadius::same(8), fill);
    }
    let col = if resp.hovered() { fg() } else { fg_soft() };
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        glyph,
        FontId::new(18.0, FontFamily::Name(FONT_SANS.into())),
        col,
    );
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.clicked()
}

// ----- icons -----------------------------------------------------------------

fn paint_emblem(ui: &Ui, tile: Rect) {
    let ink = accent_fg();
    let x = tile.left();
    let y = tile.top();
    let w = tile.width();
    let h = tile.height();
    let lid = Rect::from_min_size(
        pos2(x + w * 0.16, y + h * 0.18),
        vec2(w * 0.68, h * 0.16),
    );
    let body = Rect::from_min_size(
        pos2(x + w * 0.22, y + h * 0.40),
        vec2(w * 0.56, h * 0.44),
    );
    ui.painter().rect_filled(lid, CornerRadius::same(2), ink);
    ui.painter().rect_filled(body, CornerRadius::same(2), ink);
    ui.painter().add(egui::Shape::convex_polygon(
        vec![
            pos2(x + w * 0.32, y + h * 0.50),
            pos2(x + w * 0.68, y + h * 0.50),
            pos2(x + w * 0.50, y + h * 0.74),
        ],
        accent(),
        Stroke::NONE,
    ));
}

fn paint_gear(ui: &Ui, c: Pos2, color: Color32) {
    ui.painter()
        .circle_stroke(c, 3.4, Stroke::new(1.15, color));
    ui.painter().circle_filled(c, 1.15, color);
    for i in 0..6 {
        let a = (i as f32) * TAU / 6.0;
        let p0 = pos2(c.x + a.cos() * 4.2, c.y + a.sin() * 4.2);
        let p1 = pos2(c.x + a.cos() * 6.0, c.y + a.sin() * 6.0);
        ui.painter().line_segment([p0, p1], Stroke::new(1.25, color));
    }
}

fn with_a(c: Color32, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (a.clamp(0.0, 1.0) * 255.0) as u8)
}

// ----- fonts -----------------------------------------------------------------

fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        FONT_SANS.to_owned(),
        FontData::from_static(include_bytes!("../assets/fonts/Inter-Regular.ttf")).into(),
    );
    fonts.font_data.insert(
        "InterMedium".to_owned(),
        FontData::from_static(include_bytes!("../assets/fonts/Inter-Medium.ttf")).into(),
    );
    fonts.font_data.insert(
        "InterSemiBold".to_owned(),
        FontData::from_static(include_bytes!("../assets/fonts/Inter-SemiBold.ttf")).into(),
    );
    fonts.font_data.insert(
        FONT_MONO.to_owned(),
        FontData::from_static(include_bytes!("../assets/fonts/JetBrainsMono-Medium.ttf")).into(),
    );
    fonts.font_data.insert(
        "JetBrainsMonoBold".to_owned(),
        FontData::from_static(include_bytes!("../assets/fonts/JetBrainsMono-Bold.ttf")).into(),
    );

    for fam in [
        FontFamily::Proportional,
        FontFamily::Name(FONT_SANS.into()),
    ] {
        let e = fonts.families.entry(fam).or_default();
        e.insert(0, FONT_SANS.to_owned());
        e.push("InterMedium".to_owned());
        e.push("InterSemiBold".to_owned());
    }
    for fam in [FontFamily::Monospace, FontFamily::Name(FONT_MONO.into())] {
        let e = fonts.families.entry(fam).or_default();
        e.insert(0, FONT_MONO.to_owned());
        e.push("JetBrainsMonoBold".to_owned());
    }
    // Inter in this folder is a Latin subset. Segoe UI covers Russian, German, and French.
    if let Ok(bytes) = std::fs::read(r"C:\Windows\Fonts\segoeui.ttf") {
        fonts
            .font_data
            .insert("SegoeUI".to_owned(), FontData::from_owned(bytes).into());
        for fam in [
            FontFamily::Proportional,
            FontFamily::Name(FONT_SANS.into()),
        ] {
            fonts
                .families
                .entry(fam)
                .or_default()
                .push("SegoeUI".to_owned());
        }
    }
    ctx.set_fonts(fonts);
}
