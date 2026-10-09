use std::collections::VecDeque;
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

/// Manual sync always runs. The interval runs only while auto sync is on.
pub fn due(force: bool, auto: bool, elapsed_ms: u64, interval_ms: u64) -> bool {
    force || (auto && elapsed_ms >= interval_ms)
}

impl ConnState {
    pub fn label(self) -> &'static str {
        match self {
            ConnState::Idle => "Idle",
            ConnState::Working => "Working",
            ConnState::Ok => "OK",
            ConnState::Warn => "Warn",
            ConnState::Error => "Error",
        }
    }
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
}

pub struct SharedState {
    pub poll_interval_secs: AtomicU64,
    pub auto_sync: AtomicBool,
    pub force_poll: AtomicBool,
    pub prepare_requested: AtomicBool,
    pub prepared: AtomicBool,
    pub shutdown: AtomicBool,

    inner: Mutex<Inner>,
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
    logs: VecDeque<String>,
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
                logs: VecDeque::new(),
            }),
        })
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
        }
    }

    pub fn logs(&self) -> Vec<String> {
        self.inner
            .lock()
            .map(|g| g.logs.iter().cloned().collect())
            .unwrap_or_default()
    }
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
}
