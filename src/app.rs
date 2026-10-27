//! The window: a borderless WebView2 page (`ui/`) on a tao event loop, a tray icon
//! with a small flyout page, the global hotkey and the in-game overlay.
//! Sync itself runs on the background thread from `main.rs`; this side reads
//! `SharedState` and pushes a JSON snapshot into the page.

use std::borrow::Cow;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use base64::Engine;
use serde_json::{json, Value};
use tao::dpi::{LogicalSize, PhysicalPosition, PhysicalSize};
use tao::event::{Event, StartCause, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy, EventLoopWindowTarget};
use tao::platform::windows::{WindowBuilderExtWindows, WindowExtWindows};
use tao::window::{Icon, ResizeDirection, Window, WindowBuilder};
use wry::http::{Request, Response};
use wry::{
    MemoryUsageLevel, WebContext, WebView, WebViewBuilder, WebViewBuilderExtWindows,
    WebViewExtWindows,
};

use crate::config::{normalize_language, HelperConfig, WIN_DEFAULT, WIN_MAX, WIN_MIN};
use crate::embark_auth;
use crate::hotkey::Hotkey;
use crate::state::{now_ms, ConnState, SharedState};
use crate::win::{self, TrayClick};

const FLY_W: f64 = 340.0;
const ORIGIN: &str = "http://stashsync.localhost";
const BG: (u8, u8, u8, u8) = (0x1E, 0x1A, 0x3A, 0xFF);
const GAME_EXE: &str = "PioneerGame.exe";

/// Files of the page. Fonts are OFL (assets/fonts/OFL-*.txt).
const FILES: &[(&str, &[u8], &str)] = &[
    ("/index.html", include_bytes!("../ui/index.html"), "text/html; charset=utf-8"),
    ("/app.css", include_bytes!("../ui/app.css"), "text/css; charset=utf-8"),
    ("/app.js", include_bytes!("../ui/app.js"), "text/javascript; charset=utf-8"),
    ("/i18n.js", include_bytes!("../ui/i18n.js"), "text/javascript; charset=utf-8"),
    ("/game.jpg", include_bytes!("../ui/game.jpg"), "image/jpeg"),
    ("/fonts/Unbounded.ttf", include_bytes!("../assets/fonts/Unbounded.ttf"), "font/ttf"),
    ("/fonts/GolosText.ttf", include_bytes!("../assets/fonts/GolosText.ttf"), "font/ttf"),
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Target {
    Main,
    Fly,
}

enum UserEvent {
    Ipc(Target, String),
    Tray(TrayClick),
}

struct Page {
    window: Window,
    view: WebView,
    last_state: String,
    goals_sent: u64,
}

struct App {
    state: Arc<SharedState>,
    config: HelperConfig,
    hotkey: Hotkey,
    main: Page,
    /// The tray flyout page is made on the first tray click; most sessions never open it.
    fly: Option<Page>,
    fly_closed_at: Instant,
    fly_shown_at: Instant,
    fly_h: f64,
    context: WebContext,
    proxy: EventLoopProxy<UserEvent>,
    // Checked once a second, not on every tick.
    signed_in: bool,
    steam_on: bool,
    game_on: bool,
    last_probe: Instant,
    last_overlay: String,
    last_overlay_at: Instant,
    tray_dot: &'static str,
    config_dirty: bool,
    /// The «still in the tray» notification is shown once per run.
    told_tray: bool,
    /// Last resize of the main window; the size is saved once the drag settles.
    resized_at: Option<Instant>,
}

pub fn run(state: Arc<SharedState>, config: HelperConfig, start_in_tray: bool) -> ! {
    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    let data_dir = std::env::var_os("LOCALAPPDATA")
        .map(|base| std::path::PathBuf::from(base).join("StashSync").join("WebView2"));
    let mut context = WebContext::new(data_dir);

    let icon = Icon::from_rgba(
        include_bytes!(concat!(env!("OUT_DIR"), "/icon_256.rgba")).to_vec(),
        256,
        256,
    )
    .ok();
    let main_window = WindowBuilder::new()
        .with_title("StashSync")
        .with_inner_size(LogicalSize::new(config.window_w as f64, config.window_h as f64))
        // Borderless: the page has edge zones that start a resize drag (`resize` command).
        .with_resizable(true)
        .with_maximizable(false)
        .with_decorations(false)
        .with_undecorated_shadow(true)
        .with_window_icon(icon)
        .with_visible(!start_in_tray)
        .build(&event_loop)
        .expect("window");
    win::round_corners(main_window.hwnd());
    apply_size_limits(&main_window);

    let main_view = match build_page(&mut context, &main_window, "/index.html", Target::Main, &proxy) {
        Ok(view) => view,
        Err(err) => {
            // Microsoft's own check (GetAvailableCoreWebView2BrowserVersionString) also finds
            // per-user and fixed-version runtimes. Only when it finds none is the runtime missing;
            // any other failure shows the error itself.
            if wry::webview_version().is_err() {
                if win::ask_yes_no("StashSync", webview_missing_text(&config.language)) {
                    win::open_url(win::WEBVIEW2_URL);
                }
            } else {
                win::message_box(
                    "StashSync",
                    &format!("{}\n\n{err}", window_failed_text(&config.language)),
                );
            }
            std::process::exit(1);
        }
    };
    let (hotkey, hotkey_ok) = crate::hotkey::spawn(config.overlay_vk);
    if !hotkey_ok {
        state.log(format!(
            "Overlay hotkey {} is busy",
            crate::hotkey::key_label(config.overlay_vk)
        ));
    }

    let tray_proxy = std::sync::Mutex::new(proxy.clone());
    win::tray_add(tray_icon("wait"), "StashSync", move |click| {
        if let Ok(proxy) = tray_proxy.lock() {
            let _ = proxy.send_event(UserEvent::Tray(click));
        }
    });

    let mut app = App {
        state,
        config,
        hotkey,
        main: Page {
            window: main_window,
            view: main_view,
            last_state: String::new(),
            goals_sent: u64::MAX,
        },
        fly: None,
        fly_closed_at: Instant::now() - Duration::from_secs(5),
        fly_shown_at: Instant::now(),
        fly_h: 260.0,
        context,
        proxy,
        signed_in: embark_auth::has_session(),
        steam_on: win::steam_signed_in(),
        game_on: false,
        last_probe: Instant::now() - Duration::from_secs(5),
        last_overlay: String::new(),
        last_overlay_at: Instant::now(),
        tray_dot: "wait",
        config_dirty: false,
        told_tray: false,
        resized_at: None,
    };
    if start_in_tray {
        let _ = app.main.view.set_visible(false);
        let _ = app.main.view.set_memory_usage_level(MemoryUsageLevel::Low);
    }

    let tick = Duration::from_millis(250);
    event_loop.run(move |event, target: &EventLoopWindowTarget<UserEvent>, control_flow| {
        match event {
            Event::NewEvents(StartCause::Init) => {
                *control_flow = ControlFlow::WaitUntil(Instant::now() + tick);
            }
            Event::NewEvents(StartCause::ResumeTimeReached { .. }) => {
                app.tick();
                *control_flow = ControlFlow::WaitUntil(Instant::now() + tick);
            }
            Event::UserEvent(UserEvent::Ipc(target, body)) => {
                if app.on_ipc(target, &body) {
                    app.quit();
                    *control_flow = ControlFlow::Exit;
                }
            }
            Event::UserEvent(UserEvent::Tray(click)) => {
                if app.on_tray(click, target) {
                    app.quit();
                    *control_flow = ControlFlow::Exit;
                }
            }
            Event::WindowEvent { window_id, event, .. } => {
                if app.fly.as_ref().is_some_and(|fly| fly.window.id() == window_id) {
                    // Focus also "leaves" when WebView2 takes it into its own child window;
                    // only another window coming to the front closes the flyout.
                    if let WindowEvent::Focused(false) = event {
                        app.hide_fly_if_away();
                    }
                } else if window_id == app.main.window.id() {
                    // Alt+F4 works like the window's ×.
                    match event {
                        WindowEvent::CloseRequested => {
                            if app.close_main() {
                                app.quit();
                                *control_flow = ControlFlow::Exit;
                            }
                        }
                        WindowEvent::Resized(size) => app.on_resized(size),
                        WindowEvent::ScaleFactorChanged { .. } => apply_size_limits(&app.main.window),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    })
}

/// Size limits of the main window. tao applies them to the outer rectangle as if the borderless
/// window had no frame, but it keeps an invisible resize/shadow frame, so the frame is added here
/// (else the page ends up 14 × 8 px short at the limits). Again after a DPI change.
fn apply_size_limits(window: &Window) {
    let sf = window.scale_factor();
    let (outer, inner) = (window.outer_size(), window.inner_size());
    let (fw, fh) = (outer.width.saturating_sub(inner.width), outer.height.saturating_sub(inner.height));
    let px = |v: u32, frame: u32| (v as f64 * sf).round() as u32 + frame;
    window.set_min_inner_size(Some(PhysicalSize::new(px(WIN_MIN.0, fw), px(WIN_MIN.1, fh))));
    window.set_max_inner_size(Some(PhysicalSize::new(px(WIN_MAX.0, fw), px(WIN_MAX.1, fh))));
}

fn build_page(
    context: &mut WebContext,
    window: &Window,
    path: &str,
    target: Target,
    proxy: &EventLoopProxy<UserEvent>,
) -> wry::Result<WebView> {
    let ipc_proxy = std::sync::Mutex::new(proxy.clone());
    WebViewBuilder::new_with_web_context(context)
        .with_url(format!("{ORIGIN}{path}"))
        .with_background_color(BG)
        .with_custom_protocol("stashsync".into(), |_id, request: Request<Vec<u8>>| {
            serve(request.uri().path())
        })
        .with_ipc_handler(move |request: Request<String>| {
            if let Ok(proxy) = ipc_proxy.lock() {
                let _ = proxy.send_event(UserEvent::Ipc(target, request.into_body()));
            }
        })
        // Links open in the browser through `open`; the page itself never leaves the app.
        .with_navigation_handler(|url| url.starts_with(ORIGIN))
        .with_new_window_req_handler(|_url, _features| wry::NewWindowResponse::Deny)
        .with_hotkeys_zoom(false)
        .with_browser_accelerator_keys(false)
        .with_default_context_menus(false)
        .with_https_scheme(false)
        // Software drawing: the window never competes with the game for the GPU.
        // The other flags are wry's defaults, which this call replaces.
        .with_additional_browser_args(browser_args())
        .build(window)
}

fn browser_args() -> String {
    let mut args =
        "--disable-gpu --disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection".to_string();
    // Debug builds only: STASHSYNC_CDP=9222 lets a test drive the real page over DevTools.
    if cfg!(debug_assertions) {
        if let Ok(port) = std::env::var("STASHSYNC_CDP") {
            if port.parse::<u16>().is_ok() {
                args.push_str(&format!(" --remote-debugging-port={port}"));
            }
        }
    }
    args
}

fn serve(path: &str) -> Response<Cow<'static, [u8]>> {
    let path = if path == "/" { "/index.html" } else { path };
    match FILES.iter().find(|(name, _, _)| *name == path) {
        Some((_, bytes, mime)) => Response::builder()
            .header("Content-Type", *mime)
            .body(Cow::Borrowed(*bytes))
            .unwrap(),
        None => Response::builder()
            .status(404)
            .body(Cow::Borrowed(&b""[..]))
            .unwrap(),
    }
}

/// ARC stripes with a status dot, like the tray mark on the board.
fn tray_icon(dot: &str) -> *mut std::ffi::c_void {
    let side = win::small_icon_size();
    let base = image::RgbaImage::from_raw(
        256,
        256,
        include_bytes!(concat!(env!("OUT_DIR"), "/tray_256.rgba")).to_vec(),
    )
    .expect("icon");
    let mut img = image::imageops::resize(&base, side, side, image::imageops::FilterType::Lanczos3);
    let color: [u8; 3] = match dot {
        "ok" => [0x46, 0xC7, 0x6A],
        "warn" => [0xF5, 0xBE, 0x2E],
        "err" => [0xFF, 0x6A, 0x55],
        _ => [0x6C, 0x64, 0xA8],
    };
    let s = side as f32;
    let r = s * 0.17;
    let (cx, cy) = (s - r - 0.5, s - r - 0.5);
    for (x, y, px) in img.enumerate_pixels_mut() {
        let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
        let ring = (r + 1.5 - d).clamp(0.0, 1.0);
        if ring <= 0.0 {
            continue;
        }
        let dot_a = (r - d).clamp(0.0, 1.0);
        let rgb = if dot_a > 0.0 { color } else { [0x12, 0x10, 0x1F] };
        let a = ring;
        for i in 0..3 {
            px[i] = (rgb[i] as f32 * a + px[i] as f32 * (1.0 - a)).round() as u8;
        }
        px[3] = (255.0 * a + px[3] as f32 * (1.0 - a)).round() as u8;
    }
    win::make_icon(img.as_raw(), side)
}

/// Right-click tray menu: open the window, quit.
fn tray_menu_text(lang: &str) -> [&'static str; 2] {
    match lang {
        "ru" => ["Открыть StashSync", "Выйти"],
        "de" => ["StashSync öffnen", "Beenden"],
        "fr" => ["Ouvrir StashSync", "Quitter"],
        _ => ["Open StashSync", "Quit"],
    }
}

fn webview_missing_text(lang: &str) -> &'static str {
    match lang {
        "ru" => "Для окна StashSync нужен Microsoft Edge WebView2 Runtime, а на этом компьютере его нет. Открыть страницу загрузки Microsoft?\n\nПосле установки запустите StashSync снова.",
        "de" => "Das Fenster von StashSync braucht die Microsoft Edge WebView2 Runtime, die auf diesem PC fehlt. Die Downloadseite von Microsoft öffnen?\n\nStarten Sie StashSync nach der Installation erneut.",
        "fr" => "La fenêtre de StashSync a besoin de Microsoft Edge WebView2 Runtime, absent sur ce PC. Ouvrir la page de téléchargement de Microsoft ?\n\nRelancez StashSync après l’installation.",
        _ => "The StashSync window needs Microsoft Edge WebView2 Runtime, which is missing on this PC. Open Microsoft's download page?\n\nStart StashSync again after installing it.",
    }
}

fn window_failed_text(lang: &str) -> &'static str {
    match lang {
        "ru" => "StashSync не смог открыть окно. Перезапустите программу; если ошибка повторится, сообщите о ней на GitHub.",
        "de" => "StashSync konnte sein Fenster nicht öffnen. Starten Sie das Programm neu; tritt der Fehler wieder auf, melden Sie ihn auf GitHub.",
        "fr" => "StashSync n’a pas pu ouvrir sa fenêtre. Relancez l’appli ; si l’erreur revient, signalez-la sur GitHub.",
        _ => "StashSync could not open its window. Restart it; if the error comes back, please report it on GitHub.",
    }
}

fn in_tray_text(lang: &str) -> &'static str {
    match lang {
        "ru" => "StashSync в трее и продолжает обновлять склад. Выйти: правый клик по значку.",
        "de" => "StashSync läuft im Infobereich weiter und aktualisiert das Lager. Beenden: Rechtsklick auf das Symbol.",
        "fr" => "StashSync reste dans la zone de notification et met la réserve à jour. Quitter : clic droit sur l’icône.",
        _ => "StashSync is in the tray and keeps updating the stash. To quit, right-click its icon.",
    }
}

fn sign_in_text(lang: &str) -> &'static str {
    match lang {
        "ru" => "Нужно снова войти, иначе склад не обновится. Нажмите, чтобы открыть StashSync.",
        "de" => "Bitte erneut anmelden, sonst wird das Lager nicht aktualisiert. Klicken, um StashSync zu öffnen.",
        "fr" => "Reconnectez-vous, sinon la réserve ne sera pas mise à jour. Cliquez pour ouvrir StashSync.",
        _ => "Sign in again, or the stash will not update. Click to open StashSync.",
    }
}

impl App {
    fn tick(&mut self) {
        if self.last_probe.elapsed() >= Duration::from_secs(1) {
            self.last_probe = Instant::now();
            let was_signed_in = self.signed_in;
            self.signed_in = embark_auth::has_session();
            // Sign-in lost while the window is away: the stash stops updating, say so.
            let window_away = !self.main.window.is_visible() || self.main.window.is_minimized();
            if was_signed_in && !self.signed_in && !self.config.signed_out && window_away {
                win::tray_balloon("StashSync", sign_in_text(&self.config.language));
            }
            self.steam_on = win::steam_signed_in();
            // The process list matters only before the first sign-in.
            self.game_on = !self.signed_in && win::process_running(GAME_EXE);
        }

        // Focused(false) can arrive while Windows still reports the flyout in front; recheck here.
        if self.fly_shown_at.elapsed() > Duration::from_millis(400) {
            self.hide_fly_if_away();
        }
        if self.hotkey.take_press() {
            self.set_shown(!self.config.overlay_shown);
        }
        let applied = self.hotkey.applied();
        if applied != 0 && applied != self.config.overlay_vk {
            self.config.overlay_vk = applied;
            self.config_dirty = true;
        }
        if self.resized_at.is_some_and(|at| at.elapsed() > Duration::from_millis(700)) {
            self.resized_at = None;
            self.config_dirty = true;
        }
        if self.config_dirty {
            self.config_dirty = false;
            self.config.save();
        }

        self.sync_overlay();
        self.push(Target::Main);
        self.push(Target::Fly);
        self.update_tray();
    }

    fn page(&mut self, target: Target) -> Option<&mut Page> {
        match target {
            Target::Main => Some(&mut self.main),
            Target::Fly => self.fly.as_mut(),
        }
    }

    fn snapshot(&self) -> Value {
        let snap = self.state.snapshot();
        let now = now_ms();
        let working = snap.steam == ConnState::Working || snap.inventory == ConnState::Working;
        let done = !working && self.state.done_until_ms.load(Ordering::Relaxed) > now;
        let failed = self.signed_in
            && !working
            && (snap.inventory == ConnState::Error || snap.steam == ConnState::Error);
        let login_error = if !self.signed_in && snap.steam == ConnState::Error {
            snap.steam_detail.clone()
        } else {
            String::new()
        };
        let (screen_w, screen_h) = win::screen_size();
        let ms = |t: Option<chrono::DateTime<chrono::Local>>| t.map(|t| t.timestamp_millis());
        json!({
            "version": env!("CARGO_PKG_VERSION"),
            "lang": self.config.language,
            "signedIn": self.signed_in,
            "steamOn": self.steam_on,
            "gameOn": self.game_on,
            "signing": !self.signed_in && working,
            "loginError": login_error,
            "phase": if working { "syncing" } else if done { "done" } else { "idle" },
            "failed": failed,
            "buildWait": snap.inventory_detail == crate::i18n::BUILD_NOT_READY,
            "site": snap.ws_clients > 0,
            "lastSync": ms(snap.last_sync),
            "lastPush": ms(snap.last_push),
            "changes": snap.changes,
            "nextPoll": self.state.next_poll_ms.load(Ordering::Relaxed),
            "auto": self.config.auto_sync,
            "interval": (self.config.poll_interval_secs / 60).clamp(1, 30),
            "startup": self.config.run_at_startup,
            "closeQuits": self.config.close_quits,
            "ov": {
                "shown": self.config.overlay_shown,
                "list": self.config.overlay_list,
                "vertical": self.config.overlay_vertical,
                "size": self.config.overlay_scale,
                "opacity": self.config.overlay_opacity,
                "corner": self.config.overlay_corner,
                "x": self.config.overlay_x,
                "y": self.config.overlay_y,
                "key": crate::hotkey::key_label(self.config.overlay_vk),
                "hideDone": self.config.overlay_hide_done,
                "screenW": screen_w,
                "screenH": screen_h,
                "dpi": crate::overlay_win::dpi_scale(),
            },
            "goalsV": self.state.overlay_version.load(Ordering::Relaxed),
            "windowHidden": !self.main.window.is_visible(),
        })
    }

    fn goals_json(&self) -> Value {
        let view = self.state.overlay();
        let items: Vec<Value> = view
            .tiles
            .iter()
            .map(|tile| {
                let icon = if tile.png.is_empty() {
                    String::new()
                } else {
                    format!(
                        "data:image/png;base64,{}",
                        base64::engine::general_purpose::STANDARD.encode(&tile.png)
                    )
                };
                json!({
                    "name": tile.name,
                    "label": tile.label,
                    "left": tile.left,
                    "need": tile.need,
                    "have": tile.have,
                    "icon": icon,
                })
            })
            .collect();
        json!({ "received": view.received, "legacy": view.legacy, "items": items })
    }

    /// Sends what changed. A hidden page waits until it is shown again.
    fn push(&mut self, target: Target) {
        let version = self.state.overlay_version.load(Ordering::Relaxed);
        let goals_sent = match self.page(target) {
            Some(page) if page.window.is_visible() => page.goals_sent,
            _ => return,
        };
        if goals_sent != version {
            let goals = self.goals_json().to_string();
            if let Some(page) = self.page(target) {
                if page.view.evaluate_script(&format!("window.__ss&&__ss.goals({goals})")).is_ok() {
                    page.goals_sent = version;
                }
            }
        }
        let state = self.snapshot().to_string();
        if let Some(page) = self.page(target) {
            if page.last_state != state
                && page
                    .view
                    .evaluate_script(&format!("window.__ss&&__ss.state({state})"))
                    .is_ok()
            {
                page.last_state = state;
            }
        }
    }

    fn sync_overlay(&mut self) {
        let view = self.state.overlay();
        let tiles: Vec<_> = view
            .tiles
            .into_iter()
            .filter(|tile| !self.config.overlay_hide_done || tile.left > 0)
            .collect();
        let frame = crate::overlay_win::OverlayFrame {
            list: self.config.overlay_list,
            vertical: self.config.overlay_vertical,
            opacity: self.config.overlay_opacity,
            scale_pct: self.config.overlay_scale,
            x: self.config.overlay_x,
            y: self.config.overlay_y,
            corner: self.config.overlay_corner.clone(),
            movable: false,
        };
        let key = format!(
            "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
            self.config.overlay_shown,
            frame.list,
            frame.vertical,
            frame.opacity,
            frame.scale_pct,
            frame.x,
            frame.y,
            frame.corner,
            self.config.overlay_hide_done,
            self.state.overlay_version.load(Ordering::Relaxed),
        );
        // Repaint on change, and once a second so it stays above the game.
        if key == self.last_overlay && self.last_overlay_at.elapsed() < Duration::from_secs(1) {
            return;
        }
        self.last_overlay = key;
        self.last_overlay_at = Instant::now();
        crate::overlay_win::sync(self.config.overlay_shown, &frame, &tiles);
    }

    fn update_tray(&mut self) {
        let snap = self.state.snapshot();
        let dot = if !self.signed_in {
            "wait"
        } else if snap.inventory == ConnState::Error || snap.steam == ConnState::Error {
            "err"
        } else if snap.ws_clients == 0 {
            "warn"
        } else {
            "ok"
        };
        if dot != self.tray_dot {
            self.tray_dot = dot;
            win::tray_update(tray_icon(dot), "StashSync");
        }
    }

    fn set_shown(&mut self, on: bool) {
        self.config.overlay_shown = on;
        self.config_dirty = true;
    }

    fn show_main(&mut self) {
        self.hide_fly();
        let _ = self.main.view.evaluate_script("window.__ss&&__ss.visible(true)");
        let _ = self.main.view.set_memory_usage_level(MemoryUsageLevel::Normal);
        let _ = self.main.view.set_visible(true);
        self.main.window.set_visible(true);
        self.main.window.set_minimized(false);
        self.main.window.set_focus();
        self.main.last_state.clear();
    }

    fn hide_main(&mut self) {
        let _ = self.main.view.evaluate_script("window.__ss&&__ss.visible(false)");
        self.main.window.set_visible(false);
        let _ = self.main.view.set_visible(false);
        let _ = self.main.view.set_memory_usage_level(MemoryUsageLevel::Low);
        if !self.told_tray {
            self.told_tray = true;
            win::tray_balloon("StashSync", in_tray_text(&self.config.language));
        }
    }

    /// Remembers the size the user dragged the window to (minimizing reports 0×0, skip it).
    fn on_resized(&mut self, size: tao::dpi::PhysicalSize<u32>) {
        if size.width == 0 || size.height == 0 || self.main.window.is_minimized() {
            return;
        }
        let logical = size.to_logical::<f64>(self.main.window.scale_factor());
        let (w, h) = (logical.width.round() as u32, logical.height.round() as u32);
        if (w, h) != (self.config.window_w, self.config.window_h) {
            self.config.window_w = w.clamp(WIN_MIN.0, WIN_MAX.0);
            self.config.window_h = h.clamp(WIN_MIN.1, WIN_MAX.1);
            self.resized_at = Some(Instant::now());
        }
    }

    /// The window's × and Alt+F4. Returns true when the app should quit.
    fn close_main(&mut self) -> bool {
        if self.config.close_quits {
            return true;
        }
        self.hide_main();
        false
    }

    fn show_fly(&mut self, target: &EventLoopWindowTarget<UserEvent>) {
        if self.fly.is_none() {
            let Ok(window) = WindowBuilder::new()
                .with_title("StashSync")
                .with_inner_size(LogicalSize::new(FLY_W, self.fly_h))
                .with_resizable(false)
                .with_decorations(false)
                .with_undecorated_shadow(true)
                .with_always_on_top(true)
                .with_skip_taskbar(true)
                .with_visible(false)
                .build(target)
            else {
                return;
            };
            win::round_corners(window.hwnd());
            let path = "/index.html?view=tray";
            let Ok(view) = build_page(&mut self.context, &window, path, Target::Fly, &self.proxy) else {
                return;
            };
            self.fly = Some(Page {
                window,
                view,
                last_state: String::new(),
                goals_sent: u64::MAX,
            });
        }
        self.place_fly();
        if let Some(fly) = self.fly.as_mut() {
            let _ = fly.view.evaluate_script("window.__ss&&__ss.visible(true)");
            let _ = fly.view.set_memory_usage_level(MemoryUsageLevel::Normal);
            let _ = fly.view.set_visible(true);
            fly.window.set_visible(true);
            fly.window.set_focus();
            fly.last_state.clear();
        }
        self.fly_shown_at = Instant::now();
        self.push(Target::Fly);
    }

    fn hide_fly_if_away(&mut self) {
        if self.fly.as_ref().is_some_and(|fly| fly.window.is_visible() && !win::is_foreground(fly.window.hwnd())) {
            self.hide_fly();
        }
    }

    fn hide_fly(&mut self) {
        let Some(fly) = self.fly.as_mut() else {
            return;
        };
        if !fly.window.is_visible() {
            return;
        }
        let _ = fly.view.evaluate_script("window.__ss&&__ss.visible(false)");
        fly.window.set_visible(false);
        let _ = fly.view.set_visible(false);
        let _ = fly.view.set_memory_usage_level(MemoryUsageLevel::Low);
        self.fly_closed_at = Instant::now();
    }

    /// Bottom-right above the taskbar, where Windows puts its own flyouts.
    fn place_fly(&mut self) {
        let Some(fly) = self.fly.as_ref() else {
            return;
        };
        let scale = fly.window.scale_factor();
        fly.window.set_inner_size(LogicalSize::new(FLY_W, self.fly_h));
        let (_, _, right, bottom) = win::work_area();
        let x = right as f64 - (FLY_W + 12.0) * scale;
        let y = bottom as f64 - (self.fly_h + 12.0) * scale;
        fly.window.set_outer_position(PhysicalPosition::new(x, y));
    }

    /// Returns true when the app should quit.
    fn on_tray(&mut self, click: TrayClick, target: &EventLoopWindowTarget<UserEvent>) -> bool {
        match click {
            TrayClick::Double | TrayClick::Activate => self.show_main(),
            TrayClick::Right => {
                self.hide_fly();
                let [open, quit] = tray_menu_text(&self.config.language);
                match win::tray_menu(&[(1, open), (2, quit)]) {
                    1 => self.show_main(),
                    2 => return true,
                    _ => {}
                }
            }
            TrayClick::Left => {
                if self.fly.as_ref().is_some_and(|fly| fly.window.is_visible()) {
                    self.hide_fly();
                } else if self.fly_closed_at.elapsed() > Duration::from_millis(350) {
                    // A click on the icon first takes focus from the open flyout and closes it.
                    self.show_fly(target);
                }
            }
        }
        false
    }

    fn quit(&mut self) {
        self.state.shutdown.store(true, Ordering::Relaxed);
        if self.config_dirty {
            self.config.save();
        }
        crate::overlay_win::sync(
            false,
            &crate::overlay_win::OverlayFrame {
                list: false,
                vertical: false,
                opacity: 0,
                scale_pct: 100,
                x: 0,
                y: 0,
                corner: String::new(),
                movable: false,
            },
            &[],
        );
        win::tray_remove();
    }

    /// Commands from the page. Returns true when the app should quit.
    fn on_ipc(&mut self, target: Target, body: &str) -> bool {
        let Ok(msg) = serde_json::from_str::<Value>(body) else {
            return false;
        };
        let cmd = msg.get("cmd").and_then(Value::as_str).unwrap_or("");
        let on = msg.get("on").and_then(Value::as_bool).unwrap_or(false);
        let num = msg.get("v").and_then(Value::as_i64).unwrap_or(0);
        match cmd {
            "ready" => {
                if let Some(page) = self.page(target) {
                    page.last_state.clear();
                    page.goals_sent = u64::MAX;
                }
                self.push(target);
            }
            "sync" => self.state.force_poll.store(true, Ordering::Relaxed),
            "login" => {
                self.state.signed_out.store(false, Ordering::Relaxed);
                self.config.signed_out = false;
                self.config_dirty = true;
                self.state.force_poll.store(true, Ordering::Relaxed);
            }
            "recheck" => self.last_probe = Instant::now() - Duration::from_secs(5),
            "signOut" => {
                embark_auth::sign_out();
                self.state.signed_out.store(true, Ordering::Relaxed);
                self.state.set_steam(ConnState::Idle, "Signed out");
                self.state.set_inventory(ConnState::Idle, "Signed out");
                self.config.signed_out = true;
                self.config_dirty = true;
                self.signed_in = false;
            }
            "setAuto" => {
                if self.config.auto_sync != on {
                    self.config.auto_sync = on;
                    self.state.auto_sync.store(on, Ordering::Relaxed);
                    if on {
                        self.state.force_poll.store(true, Ordering::Relaxed);
                    }
                    self.config_dirty = true;
                }
            }
            "setInterval" => {
                let secs = (num.clamp(1, 30) as u64 * 60)
                    .clamp(HelperConfig::min_poll(), HelperConfig::max_poll());
                self.config.poll_interval_secs = secs;
                self.state.poll_interval_secs.store(secs, Ordering::Relaxed);
                self.config_dirty = true;
            }
            "setStartup" => {
                if self.config.run_at_startup != on {
                    self.config.run_at_startup = on;
                    crate::config::apply_run_at_startup(on);
                    self.config_dirty = true;
                }
            }
            "setCloseQuits" => {
                self.config.close_quits = on;
                self.config_dirty = true;
            }
            "setLang" => {
                let code = msg.get("lang").and_then(Value::as_str).unwrap_or("en");
                self.config.language = normalize_language(code);
                self.config_dirty = true;
            }
            "ovShown" => self.set_shown(on),
            "ovList" => {
                self.config.overlay_list = on;
                self.config_dirty = true;
            }
            "ovVertical" => {
                self.config.overlay_vertical = on;
                self.config_dirty = true;
            }
            "ovSize" => {
                self.config.overlay_scale = num.clamp(50, 160) as u8;
                self.config_dirty = true;
            }
            "ovOpacity" => {
                self.config.overlay_opacity = num.clamp(20, 100) as u8;
                self.config_dirty = true;
            }
            "ovCorner" => {
                let corner = msg.get("corner").and_then(Value::as_str).unwrap_or("tl");
                if matches!(corner, "tl" | "tr" | "bl" | "br") {
                    self.config.overlay_corner = corner.into();
                    self.config_dirty = true;
                }
            }
            "ovPos" => {
                let x = msg.get("x").and_then(Value::as_i64).unwrap_or(16);
                let y = msg.get("y").and_then(Value::as_i64).unwrap_or(16);
                self.config.overlay_x = x.clamp(-4000, 8000) as i32;
                self.config.overlay_y = y.clamp(-4000, 8000) as i32;
                self.config.overlay_corner.clear();
                self.config_dirty = true;
            }
            "ovKey" => {
                let vk = num as u32;
                // Esc cancels; mouse buttons and modifiers alone are not keys here.
                if (0x08..=0xFE).contains(&vk) && vk != 0x1B && !(0x10..=0x12).contains(&vk) {
                    self.config.overlay_vk = vk;
                    self.hotkey.rebind(vk);
                    self.config_dirty = true;
                }
            }
            "ovHideDone" => {
                self.config.overlay_hide_done = on;
                self.config_dirty = true;
            }
            "ovReset" => {
                let fresh = HelperConfig::default();
                self.config.overlay_opacity = fresh.overlay_opacity;
                self.config.overlay_list = fresh.overlay_list;
                self.config.overlay_vertical = fresh.overlay_vertical;
                self.config.overlay_scale = fresh.overlay_scale;
                self.config.overlay_x = fresh.overlay_x;
                self.config.overlay_y = fresh.overlay_y;
                self.config.overlay_corner = fresh.overlay_corner;
                self.config.overlay_hide_done = fresh.overlay_hide_done;
                self.config.overlay_vk = fresh.overlay_vk;
                self.hotkey.rebind(fresh.overlay_vk);
                self.config_dirty = true;
            }
            "minimize" => self.main.window.set_minimized(true),
            "close" => return self.close_main(),
            "openWindow" => self.show_main(),
            "quit" => return true,
            "drag" => {
                let _ = self.main.window.drag_window();
            }
            "resize" => {
                let dir = match msg.get("dir").and_then(Value::as_str).unwrap_or("") {
                    "n" => Some(ResizeDirection::North),
                    "s" => Some(ResizeDirection::South),
                    "e" => Some(ResizeDirection::East),
                    "w" => Some(ResizeDirection::West),
                    "ne" => Some(ResizeDirection::NorthEast),
                    "nw" => Some(ResizeDirection::NorthWest),
                    "se" => Some(ResizeDirection::SouthEast),
                    "sw" => Some(ResizeDirection::SouthWest),
                    _ => None,
                };
                if let Some(dir) = dir {
                    let _ = self.main.window.drag_resize_window(dir);
                }
            }
            "resetSize" => {
                let (w, h) = WIN_DEFAULT;
                self.main.window.set_inner_size(LogicalSize::new(w as f64, h as f64));
            }
            "open" => {
                if let Some(url) = msg.get("url").and_then(Value::as_str) {
                    win::open_url(url);
                }
            }
            "flySize" => {
                self.fly_h = (num as f64).clamp(120.0, 600.0);
                if self.fly.as_ref().is_some_and(|fly| fly.window.is_visible()) {
                    self.place_fly();
                }
            }
            _ => {}
        }
        // Answer at once so a switch never waits for the next tick.
        self.push(Target::Main);
        self.push(Target::Fly);
        // Same for the in-game overlay: a slider drag repaints it on every step, not 4 times a second.
        if cmd.starts_with("ov") {
            self.sync_overlay();
        }
        false
    }
}
