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
use tao::dpi::{LogicalSize, PhysicalPosition};
use tao::event::{Event, StartCause, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy, EventLoopWindowTarget};
use tao::platform::windows::{WindowBuilderExtWindows, WindowExtWindows};
use tao::window::{Icon, Window, WindowBuilder};
use wry::http::{Request, Response};
use wry::{
    MemoryUsageLevel, WebContext, WebView, WebViewBuilder, WebViewBuilderExtWindows,
    WebViewExtWindows,
};

use crate::config::{normalize_language, HelperConfig};
use crate::embark_auth;
use crate::hotkey::Hotkey;
use crate::state::{now_ms, ConnState, SharedState};
use crate::win::{self, TrayClick};

const WIN_W: f64 = 400.0;
const WIN_H: f64 = 680.0;
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
    told_minimized: bool,
}

pub fn run(state: Arc<SharedState>, config: HelperConfig) -> ! {
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
        .with_inner_size(LogicalSize::new(WIN_W, WIN_H))
        .with_resizable(false)
        .with_maximizable(false)
        .with_decorations(false)
        .with_undecorated_shadow(true)
        .with_window_icon(icon)
        .build(&event_loop)
        .expect("window");
    win::round_corners(main_window.hwnd());

    let main_view = match build_page(&mut context, &main_window, "/index.html", Target::Main, &proxy) {
        Ok(view) => view,
        Err(err) => {
            win::message_box(
                "StashSync",
                &format!(
                    "StashSync needs Microsoft Edge WebView2 Runtime.\n\
                     Download it from https://go.microsoft.com/fwlink/p/?LinkId=2124703\n\n{err}"
                ),
            );
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
        told_minimized: false,
    };

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
            Event::UserEvent(UserEvent::Tray(click)) => app.on_tray(click, target),
            Event::WindowEvent { window_id, event, .. } => {
                if app.fly.as_ref().is_some_and(|fly| fly.window.id() == window_id) {
                    if let WindowEvent::Focused(false) = event {
                        app.hide_fly();
                    }
                } else if window_id == app.main.window.id() {
                    if let WindowEvent::CloseRequested = event {
                        app.quit();
                        *control_flow = ControlFlow::Exit;
                    }
                }
            }
            _ => {}
        }
    })
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

fn minimized_text(lang: &str) -> &'static str {
    match lang {
        "ru" => "StashSync свёрнут. Склад продолжает обновляться.",
        "de" => "StashSync ist minimiert. Das Lager wird weiter aktualisiert.",
        "fr" => "StashSync est réduit. La réserve continue de se mettre à jour.",
        _ => "StashSync is minimized. The stash keeps updating.",
    }
}

impl App {
    fn tick(&mut self) {
        if self.last_probe.elapsed() >= Duration::from_secs(1) {
            self.last_probe = Instant::now();
            self.signed_in = embark_auth::has_session();
            self.steam_on = win::steam_signed_in();
            // The process list matters only before the first sign-in.
            self.game_on = !self.signed_in && win::process_running(GAME_EXE);
        }

        if self.hotkey.take_press() {
            self.set_shown(!self.config.overlay_shown);
        }
        let applied = self.hotkey.applied();
        if applied != 0 && applied != self.config.overlay_vk {
            self.config.overlay_vk = applied;
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
        if !self.told_minimized {
            self.told_minimized = true;
            win::tray_balloon("StashSync", minimized_text(&self.config.language));
        }
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
        self.push(Target::Fly);
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

    fn on_tray(&mut self, click: TrayClick, target: &EventLoopWindowTarget<UserEvent>) {
        match click {
            TrayClick::Double => self.show_main(),
            TrayClick::Left | TrayClick::Right => {
                if self.fly.as_ref().is_some_and(|fly| fly.window.is_visible()) {
                    self.hide_fly();
                } else if self.fly_closed_at.elapsed() > Duration::from_millis(350) {
                    // A click on the icon first takes focus from the open flyout and closes it.
                    self.show_fly(target);
                }
            }
        }
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
            "minimize" => self.hide_main(),
            "openWindow" => self.show_main(),
            "quit" => return true,
            "drag" => {
                let _ = self.main.window.drag_window();
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
        false
    }
}
