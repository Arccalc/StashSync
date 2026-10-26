use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Local};

const MAX_LOG_LINES: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnState {
    Idle,
    Working,
    Ok,
    Warn,
    Error,
}

/// Wall clock in milliseconds, for times the window counts down to.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

/// Manual sync always runs. The interval runs only while auto sync is on.
pub fn due(force: bool, auto: bool, elapsed_ms: u64, interval_ms: u64) -> bool {
    force || (auto && elapsed_ms >= interval_ms)
}

#[derive(Debug, Clone)]
pub struct StatusSnapshot {
    pub steam: ConnState,
    pub steam_detail: String,
    pub token: ConnState,
    pub token_detail: String,
    pub websocket: ConnState,
    pub websocket_detail: String,
    pub inventory: ConnState,
    pub inventory_detail: String,
    pub last_sync: Option<DateTime<Local>>,
    pub mapped_keys: u64,
    pub raw_stacks: u64,
    pub poll_interval_secs: u64,
    pub ws_clients: u64,
    pub prepared: bool,
    /// Calculator items that changed in the last sync. `None` until two syncs can be compared.
    pub changes: Option<u64>,
    /// Last time the stash reached an open calculator tab.
    pub last_push: Option<DateTime<Local>>,
}

pub struct SharedState {
    pub poll_interval_secs: AtomicU64,
    pub auto_sync: AtomicBool,
    pub force_poll: AtomicBool,
    pub prepare_requested: AtomicBool,
    pub prepared: AtomicBool,
    pub shutdown: AtomicBool,
    /// Wall clock (ms) of the next automatic poll. 0 while auto sync is off.
    pub next_poll_ms: AtomicU64,
    /// Wall clock (ms) until which the window shows «done» after a sync.
    pub done_until_ms: AtomicU64,
    /// Set by «Sign out». No automatic Steam sign-in until the user asks again.
    pub signed_out: AtomicBool,
    /// Bumped every time the calculator sends a different goal list.
    pub overlay_version: AtomicU64,

    inner: Mutex<Inner>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverlayTile {
    pub name: String,
    pub label: String,
    pub left: i64,
    /// Goal total and what the stash covers. 0 / 0 when an older calculator sent only `left`.
    pub need: i64,
    pub have: i64,
    pub png: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OverlayView {
    pub tiles: Vec<OverlayTile>,
    /// False until the calculator has sent a goal list at least once.
    pub received: bool,
    /// The calculator sent only what is left (no `need` / `have`).
    pub legacy: bool,
}

struct Inner {
    steam: ConnState,
    steam_detail: String,
    token: ConnState,
    token_detail: String,
    websocket: ConnState,
    websocket_detail: String,
    inventory: ConnState,
    inventory_detail: String,
    last_sync: Option<DateTime<Local>>,
    mapped_keys: u64,
    raw_stacks: u64,
    ws_clients: u64,
    changes: Option<u64>,
    last_push: Option<DateTime<Local>>,
    logs: VecDeque<String>,
    overlay: OverlayView,
}

impl SharedState {
    pub fn new(poll_interval_secs: u64, auto_sync: bool) -> Arc<Self> {
        Arc::new(Self {
            poll_interval_secs: AtomicU64::new(poll_interval_secs),
            auto_sync: AtomicBool::new(auto_sync),
            force_poll: AtomicBool::new(false),
            prepare_requested: AtomicBool::new(false),
            prepared: AtomicBool::new(false),
            shutdown: AtomicBool::new(false),
            next_poll_ms: AtomicU64::new(0),
            done_until_ms: AtomicU64::new(0),
            signed_out: AtomicBool::new(false),
            overlay_version: AtomicU64::new(0),
            inner: Mutex::new(Inner {
                steam: ConnState::Idle,
                steam_detail: "Not started".into(),
                token: ConnState::Idle,
                token_detail: "No token yet".into(),
                websocket: ConnState::Idle,
                websocket_detail: "Starting…".into(),
                inventory: ConnState::Idle,
                inventory_detail: "Waiting for token".into(),
                last_sync: None,
                mapped_keys: 0,
                raw_stacks: 0,
                ws_clients: 0,
                changes: None,
                last_push: None,
                logs: VecDeque::new(),
                overlay: OverlayView::default(),
            }),
        })
    }

    pub fn set_overlay(&self, view: OverlayView) {
        if let Ok(mut guard) = self.inner.lock() {
            if guard.overlay != view {
                guard.overlay = view;
                self.overlay_version.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    pub fn set_changes(&self, changes: Option<u64>) {
        if let Ok(mut g) = self.inner.lock() {
            g.changes = changes;
        }
    }

    pub fn mark_pushed(&self) {
        if let Ok(mut g) = self.inner.lock() {
            g.last_push = Some(Local::now());
        }
    }

    pub fn overlay(&self) -> OverlayView {
        self.inner
            .lock()
            .map(|guard| guard.overlay.clone())
            .unwrap_or_default()
    }

    pub fn log(&self, msg: impl Into<String>) {
        let line = format!("[{}] {}", Local::now().format("%H:%M:%S"), msg.into());
        if let Ok(mut g) = self.inner.lock() {
            g.logs.push_back(line);
            while g.logs.len() > MAX_LOG_LINES {
                g.logs.pop_front();
            }
        }
    }

    pub fn set_steam(&self, state: ConnState, detail: impl Into<String>) {
        if let Ok(mut g) = self.inner.lock() {
            g.steam = state;
            g.steam_detail = detail.into();
        }
    }

    pub fn set_token(&self, state: ConnState, detail: impl Into<String>) {
        if let Ok(mut g) = self.inner.lock() {
            g.token = state;
            g.token_detail = detail.into();
        }
    }

    pub fn set_websocket(&self, state: ConnState, detail: impl Into<String>) {
        if let Ok(mut g) = self.inner.lock() {
            g.websocket = state;
            g.websocket_detail = detail.into();
        }
    }

    pub fn set_inventory(&self, state: ConnState, detail: impl Into<String>) {
        if let Ok(mut g) = self.inner.lock() {
            g.inventory = state;
            g.inventory_detail = detail.into();
        }
    }

    pub fn set_sync_stats(&self, mapped_keys: u64, raw_stacks: u64) {
        if let Ok(mut g) = self.inner.lock() {
            g.mapped_keys = mapped_keys;
            g.raw_stacks = raw_stacks;
            g.last_sync = Some(Local::now());
        }
    }

    pub fn set_ws_clients(&self, n: u64) {
        if let Ok(mut g) = self.inner.lock() {
            g.ws_clients = n;
        }
    }

    pub fn snapshot(&self) -> StatusSnapshot {
        let poll = self.poll_interval_secs.load(Ordering::Relaxed);
        let prepared = self.prepared.load(Ordering::Relaxed);
        let g = self.inner.lock().unwrap();
        StatusSnapshot {
            steam: g.steam,
            steam_detail: g.steam_detail.clone(),
            token: g.token,
            token_detail: g.token_detail.clone(),
            websocket: g.websocket,
            websocket_detail: g.websocket_detail.clone(),
            inventory: g.inventory,
            inventory_detail: g.inventory_detail.clone(),
            last_sync: g.last_sync,
            mapped_keys: g.mapped_keys,
            raw_stacks: g.raw_stacks,
            poll_interval_secs: poll,
            ws_clients: g.ws_clients,
            prepared,
            changes: g.changes,
            last_push: g.last_push,
        }
    }
}

/// Calculator → helper. Anything else (including an inventory payload) is ignored.
/// `allow_local_icons` is true only for a page opened from this computer.
/// A site on the internet cannot ask the helper to read a file.
pub fn parse_overlay(text: &str, allow_local_icons: bool) -> Option<OverlayView> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    if value.get("type").and_then(|item| item.as_str()) != Some("overlay") {
        return None;
    }
    let page = value.get("page").and_then(|item| item.as_str()).unwrap_or("");
    let mut tiles = Vec::new();
    let mut legacy = false;
    if let Some(items) = value.get("items").and_then(|item| item.as_array()) {
        for item in items.iter().take(80) {
            let name = clean_label(item.get("name").and_then(|part| part.as_str()).unwrap_or(""));
            if name.is_empty() {
                continue;
            }
            let left = item
                .get("left")
                .and_then(|part| part.as_i64())
                .unwrap_or(0)
                .clamp(0, 1_000_000);
            let need = item.get("need").and_then(|part| part.as_i64());
            // Collected goals come only with `need`; an old calculator sends left > 0 only.
            if left <= 0 && need.unwrap_or(0) <= 0 {
                continue;
            }
            if need.is_none() {
                legacy = true;
            }
            let need = need.unwrap_or(0).clamp(0, 1_000_000);
            let have = item
                .get("have")
                .and_then(|part| part.as_i64())
                .unwrap_or(0)
                .clamp(0, need);
            let mut label = clean_label(item.get("label").and_then(|part| part.as_str()).unwrap_or(""));
            if label.is_empty() {
                label = name.clone();
            }
            let mut png = item
                .get("icon")
                .and_then(|part| part.as_str())
                .map(decode_icon)
                .unwrap_or_default();
            if png.is_empty() && allow_local_icons {
                let file = item.get("file").and_then(|part| part.as_str()).unwrap_or("");
                png = local_icon(page, file);
            }
            tiles.push(OverlayTile {
                name,
                label,
                left,
                need,
                have,
                png,
            });
        }
    }
    Some(OverlayView {
        tiles,
        received: true,
        legacy,
    })
}

/// Read `img/<file>` next to the local calculator page. The name is one png file, nothing else.
fn local_icon(page: &str, file: &str) -> Vec<u8> {
    let Some(page_path) = file_url_path(page) else {
        return Vec::new();
    };
    let Some(ext) = page_path.extension() else {
        return Vec::new();
    };
    if !ext.eq_ignore_ascii_case("html") && !ext.eq_ignore_ascii_case("htm") {
        return Vec::new();
    }
    if !safe_png_name(file) {
        return Vec::new();
    }
    let Some(dir) = page_path.parent() else {
        return Vec::new();
    };
    let img_dir = dir.join("img");
    let wanted = img_dir.join(file);
    let Ok(root) = img_dir.canonicalize() else {
        return Vec::new();
    };
    let Ok(full) = wanted.canonicalize() else {
        return Vec::new();
    };
    if !full.starts_with(&root) {
        return Vec::new();
    }
    let Ok(bytes) = std::fs::read(&full) else {
        return Vec::new();
    };
    if bytes.len() > 2_000_000 || !bytes.starts_with(b"\x89PNG") {
        return Vec::new();
    }
    fit_icon(&bytes)
}

fn safe_png_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    if bytes.len() < 5 || bytes.len() > 80 || !name.to_ascii_lowercase().ends_with(".png") {
        return false;
    }
    bytes
        .iter()
        .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'.' || *byte == b'_' || *byte == b'-')
}

fn file_url_path(url: &str) -> Option<PathBuf> {
    let rest = url.trim().strip_prefix("file://")?;
    let rest = rest.split(['?', '#']).next().unwrap_or(rest);
    let path = rest.strip_prefix('/').unwrap_or(rest);
    if path.is_empty() || path.contains('\0') {
        return None;
    }
    let decoded = percent_decode(path)?;
    if decoded.is_empty() || decoded.contains('\0') {
        return None;
    }
    Some(PathBuf::from(decoded))
}

fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let high = hex_nibble(bytes[index + 1])?;
            let low = hex_nibble(bytes[index + 2])?;
            out.push((high << 4) | low);
            index += 3;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn fit_icon(bytes: &[u8]) -> Vec<u8> {
    let Ok(image) = image::load_from_memory(bytes) else {
        return Vec::new();
    };
    let image = if image.width() <= 96 && image.height() <= 96 {
        image
    } else {
        image.resize(96, 96, image::imageops::FilterType::Triangle)
    };
    let mut out = Vec::new();
    if image
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .is_err()
    {
        return Vec::new();
    }
    if out.len() > 100_000 || !out.starts_with(b"\x89PNG") {
        return Vec::new();
    }
    out
}

fn decode_icon(text: &str) -> Vec<u8> {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.len() > 140_000 {
        return Vec::new();
    }
    let Ok(bytes) = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, trimmed)
    else {
        return Vec::new();
    };
    if bytes.len() > 100_000 || !bytes.starts_with(b"\x89PNG") {
        return Vec::new();
    }
    bytes
}

fn clean_label(text: &str) -> String {
    let mut out = String::new();
    for ch in text.trim().chars() {
        if ch.is_control() {
            continue;
        }
        out.push(ch);
        if out.chars().count() >= 80 {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::due;

    #[test]
    fn interval_runs_only_while_auto_sync_is_on() {
        assert!(!due(false, false, 999_999, 180_000));
        assert!(due(true, false, 0, 180_000));
        assert!(!due(false, true, 1_000, 180_000));
        assert!(due(false, true, 180_000, 180_000));
    }

    #[test]
    fn overlay_message_keeps_remaining_counts() {
        let view = super::parse_overlay(
            r#"{"type":"overlay","items":[{"name":"Fabric","left":12,"icon":""},{"name":"Wires","left":0}]}"#,
            false,
        )
        .expect("overlay");
        assert_eq!(view.tiles.len(), 1);
        assert_eq!(view.tiles[0].name, "Fabric");
        assert_eq!(view.tiles[0].label, "Fabric");
        assert_eq!(view.tiles[0].left, 12);
        assert!(view.tiles[0].png.is_empty());
    }

    #[test]
    fn overlay_keeps_a_png_icon_and_drops_junk() {
        let png = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";
        let raw = format!(
            r#"{{"type":"overlay","items":[{{"name":"Fabric","left":3,"icon":"{png}"}},{{"name":"Metal Parts","left":1,"icon":"not-an-image"}}]}}"#
        );
        let view = super::parse_overlay(&raw, false).expect("overlay");
        assert_eq!(view.tiles.len(), 2);
        assert!(view.tiles[0].png.starts_with(b"\x89PNG"));
        assert!(view.tiles[1].png.is_empty());
    }

    #[test]
    fn local_page_icon_comes_from_the_img_folder() {
        let png = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==",
        )
        .expect("png");
        let root = std::env::temp_dir().join(format!("stashsync-icon-{}", std::process::id()));
        let img = root.join("img");
        std::fs::create_dir_all(&img).unwrap();
        std::fs::write(img.join("fabric.png"), &png).unwrap();
        std::fs::write(root.join("secret.png"), &png).unwrap();
        std::fs::write(root.join("index.html"), b"").unwrap();
        let page = format!(
            "file:///{}",
            root.join("index.html").display().to_string().replace('\\', "/")
        );
        let raw = format!(
            r#"{{"type":"overlay","page":"{page}","items":[{{"name":"Fabric","left":3,"file":"fabric.png"}},{{"name":"Secret","left":1,"file":"../secret.png"}}]}}"#
        );
        let open = super::parse_overlay(&raw, true).expect("overlay");
        assert!(open.tiles[0].png.starts_with(b"\x89PNG"));
        assert!(open.tiles[1].png.is_empty());
        let closed = super::parse_overlay(&raw, false).expect("overlay");
        assert!(closed.tiles[0].png.is_empty());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn goal_list_keeps_collected_rows_with_totals() {
        let view = super::parse_overlay(
            r#"{"type":"overlay","items":[{"name":"Fabric","left":0,"need":5,"have":7},{"name":"Wires","left":2,"need":3,"have":1}]}"#,
            false,
        )
        .expect("overlay");
        assert!(!view.legacy);
        assert_eq!(view.tiles.len(), 2);
        assert_eq!((view.tiles[0].need, view.tiles[0].have), (5, 5));
        assert_eq!(view.tiles[1].left, 2);
    }

    #[test]
    fn inventory_payload_is_not_an_overlay() {
        assert!(super::parse_overlay(r#"{"Fabric":10,"_loadout":{}}"#, false).is_none());
    }
}
