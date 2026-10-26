// Hide console on Windows release builds — logs go to the window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::fs::{self};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use anyhow::{bail, Result};
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tokio_tungstenite::tungstenite::handshake::server::{Request, Response};
use tokio_tungstenite::tungstenite::http;

mod app;
mod config;
mod embark_auth;
mod hotkey;
mod i18n;
mod loadout;
mod loadout_frames;
mod loadout_names;
mod mappings;
mod overlay_win;
mod state;
mod win;

use config::HelperConfig;
use state::{ConnState, SharedState};

pub const ARC_RAIDERS_STEAM_APP_ID: u32 = 1808500;

// ---------------------------------------------------------------------------
// Inventory
// ---------------------------------------------------------------------------

const KNOWN_BUILD_LABEL: &str = "CL1389382-7EAFFE60-BK4740";
const EMBARK_APP_ID: &str = "2118292977";

fn installed_changelist() -> Option<String> {
    let base = std::env::var_os("LOCALAPPDATA")?;
    let path = PathBuf::from(base)
        .join("PioneerGame")
        .join("Saved")
        .join("Config")
        .join("WindowsClient")
        .join("GameUserSettings.ini");
    let text = fs::read_to_string(path).ok()?;
    for line in text.lines() {
        let Some(value) = line.trim().strip_prefix("LastChangelist=") else {
            continue;
        };
        let value = value.trim();
        if !value.is_empty() && value.chars().all(|ch| ch.is_ascii_digit()) {
            return Some(value.to_string());
        }
    }
    None
}

/// Short form first. That is the form Embark accepts today. The full public
/// build id is the fallback for the day that form stops working.
fn manifest_label_candidates(version: &str) -> Vec<String> {
    let parts: Vec<&str> = version.split('-').collect();
    if parts.len() < 3 || parts.len() > 6 {
        return Vec::new();
    }
    let changelist = parts[0];
    let hash = parts[1];
    let bk = parts[2];
    let shape_ok = changelist.starts_with("CL")
        && changelist.len() > 2
        && changelist[2..].chars().all(|ch| ch.is_ascii_digit())
        && hash.len() == 8
        && hash.chars().all(|ch| ch.is_ascii_hexdigit())
        && bk.starts_with("BK")
        && bk.len() > 2
        && bk[2..].chars().all(|ch| ch.is_ascii_digit());
    if !shape_ok {
        return Vec::new();
    }
    let extras_ok = parts[3..].iter().all(|part| {
        !part.is_empty()
            && part.len() <= 32
            && part.chars().all(|ch| ch.is_ascii_alphanumeric())
    });
    let mut out = vec![format!("{changelist}-{hash}-{bk}")];
    if parts.len() > 3 && extras_ok {
        let full = parts.join("-");
        if full != out[0] {
            out.push(full);
        }
    }
    out
}

fn labels_from_build_list(value: &serde_json::Value, changelist: &str) -> Vec<String> {
    let prefix = format!("CL{changelist}-");
    let Some(elements) = value.get("elements").and_then(|item| item.as_array()) else {
        return Vec::new();
    };
    let mut fallback = Vec::new();
    for item in elements {
        let Some(version) = item.get("buildVersion").and_then(|item| item.as_str()) else {
            continue;
        };
        if !version.starts_with(&prefix) {
            continue;
        }
        let labels = manifest_label_candidates(version);
        if labels.is_empty() {
            continue;
        }
        let live = item
            .get("labelName")
            .and_then(|item| item.as_str())
            .is_some_and(|name| name.contains("Live"));
        if live {
            return labels;
        }
        if fallback.is_empty() {
            fallback = labels;
        }
    }
    fallback
}

fn push_unique(labels: &mut Vec<String>, label: &str) {
    if !labels.iter().any(|item| item == label) {
        labels.push(label.to_string());
    }
}

async fn manifest_labels(client: &reqwest::Client) -> Vec<String> {
    let mut labels = Vec::new();
    if let Some(changelist) = installed_changelist() {
        for page in 1..=3 {
            let url = format!(
                "https://api.egdata.app/sandboxes/b4bb52a95d0b43d9af543c6ec3c54e04/builds?page={page}&limit=50"
            );
            let Ok(response) = client.get(url).send().await else {
                break;
            };
            if !response.status().is_success() {
                break;
            }
            let Ok(value) = response.json::<serde_json::Value>().await else {
                break;
            };
            let found = labels_from_build_list(&value, &changelist);
            if !found.is_empty() {
                labels = found;
                break;
            }
            let count = value.get("count").and_then(|item| item.as_u64()).unwrap_or(0);
            if page * 50 >= count {
                break;
            }
        }
    }
    push_unique(&mut labels, KNOWN_BUILD_LABEL);
    labels
}

enum ManifestAnswer {
    Ready(String),
    NotReady,
    Rejected(u16),
}

async fn request_manifest(
    client: &reqwest::Client,
    token: &str,
    label: &str,
) -> Result<ManifestAnswer> {
    let body = format!(
        r#"{{"build_id":"{label}","playtest_id":0,"artifact_id":"","store_deployment_target":"Production","manifest_id":0,"app_id":"{EMBARK_APP_ID}"}}"#
    );
    let response = client
        .post("https://api-gateway.europe.es-pio.net/v1/shared/manifest")
        .header("Authorization", format!("Bearer {token}"))
        .header(
            "User-Agent",
            "EmbarkGameBoot/1.0 (Windows; 10.0.19045.1.0.64bit)",
        )
        .header("x-embark-telemetry-client-platform", "3")
        .header(
            "x-embark-telemetry-uuid",
            "f47ac10b-58cc-4372-a567-0e02b2c3d479",
        )
        .header("Content-Type", "application/json")
        .body(body)
        .send()
        .await?;
    let status = response.status();
    let text = response.text().await?;
    if status.as_u16() == 401 || status.as_u16() == 403 {
        return Ok(ManifestAnswer::Rejected(status.as_u16()));
    }
    if !status.is_success() {
        return Ok(ManifestAnswer::NotReady);
    }
    let value: serde_json::Value = match serde_json::from_str(&text) {
        Ok(value) => value,
        Err(_) => return Ok(ManifestAnswer::NotReady),
    };
    let id = value.get("id").and_then(|item| {
        item.as_u64()
            .or_else(|| item.as_i64().map(|number| number as u64))
            .or_else(|| item.as_str().and_then(|text| text.parse().ok()))
    });
    match id.filter(|number| *number > 0) {
        Some(number) => Ok(ManifestAnswer::Ready(number.to_string())),
        None => Ok(ManifestAnswer::NotReady),
    }
}

async fn query_inventory(token: &str, manifest_id: Option<&str>) -> Result<serde_json::Value> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()?;

    if let Some(id) = manifest_id {
        return fetch_inventory(&client, token, id).await;
    }

    let labels = manifest_labels(&client).await;
    let mut waiting_for_build = false;
    for label in &labels {
        let manifest = match request_manifest(&client, token, label).await? {
            ManifestAnswer::Ready(id) => id,
            ManifestAnswer::NotReady => {
                waiting_for_build = true;
                continue;
            }
            ManifestAnswer::Rejected(status) => {
                bail!("manifest lookup failed: HTTP {status}");
            }
        };
        match fetch_inventory(&client, token, &manifest).await {
            Ok(inventory) => return Ok(inventory),
            Err(err) if format!("{err:#}").contains("status 409") => {
                waiting_for_build = true;
                continue;
            }
            Err(err) => return Err(err),
        }
    }
    if waiting_for_build {
        bail!("{}", crate::i18n::BUILD_NOT_READY);
    }
    bail!("manifest lookup failed");
}

async fn fetch_inventory(
    client: &reqwest::Client,
    token: &str,
    manifest: &str,
) -> Result<serde_json::Value> {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("Authorization", format!("Bearer {}", token).parse()?);
    headers.insert("x-embark-manifest-id", manifest.parse()?);
    headers.insert("x-embark-telemetry-client-platform", "3".parse()?);
    headers.insert(
        "x-embark-telemetry-uuid",
        "f47ac10b-58cc-4372-a567-0e02b2c3d479".parse()?,
    );
    headers.insert("Content-Type", "application/json".parse()?);

    let url = "https://api-gateway.europe.es-pio.net/v1/pioneer/inventory";
    let res = client.get(url).headers(headers.clone()).send().await?;

    if res.status() == reqwest::StatusCode::METHOD_NOT_ALLOWED {
        let res_post = client.post(url).headers(headers).body("{}").send().await?;
        let status = res_post.status();
        if !status.is_success() {
            let err_text = res_post.text().await?;
            bail!("Embark API returned status {}: {}", status, err_text);
        }
        return Ok(res_post.json().await?);
    }

    let status = res.status();
    if !status.is_success() {
        let err_text = res.text().await?;
        bail!("Embark API returned status {}: {}", status, err_text);
    }
    Ok(res.json().await?)
}

fn json_i64(value: &serde_json::Value) -> i64 {
    value
        .as_i64()
        .or_else(|| value.as_u64().map(|n| n as i64))
        .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
        .unwrap_or(0)
}

fn inventory_items(raw: &serde_json::Value) -> Option<&Vec<serde_json::Value>> {
    raw.get("items")
        .and_then(|value| value.as_array())
        .or_else(|| raw.pointer("/data/items").and_then(|value| value.as_array()))
}

fn translate_inventory(raw: &serde_json::Value) -> (serde_json::Value, u64, u64) {
    let mut mapped = serde_json::Map::new();
    let mut raw_entries = 0u64;
    if let Some(items) = inventory_items(raw) {
        for item in items {
            let Some(asset_id_val) = item.get("gameAssetId") else {
                continue;
            };
            let Some(amount_val) = item.get("amount") else {
                continue;
            };
            let asset_id = json_i64(asset_id_val) as i32;
            let amount = json_i64(amount_val);
            if amount <= 0 {
                continue;
            }
            raw_entries += 1;
            if let Some(name) = mappings::get_item_name(asset_id) {
                let entry = mapped
                    .entry(name.to_string())
                    .or_insert(serde_json::Value::Number(0.into()));
                if let Some(current_val) = entry.as_i64() {
                    *entry = serde_json::Value::Number((current_val + amount).into());
                }
            }
        }
    }
    let keys = mapped.len() as u64;
    (serde_json::Value::Object(mapped), keys, raw_entries)
}

fn payload_shape(raw: &serde_json::Value) -> String {
    let top = raw
        .as_object()
        .map(|object| object.keys().cloned().collect::<Vec<_>>().join(", "))
        .unwrap_or_else(|| raw.get("items").map(|_| "array").unwrap_or("?").into());
    let sample = inventory_items(raw)
        .and_then(|items| items.first())
        .and_then(|item| item.as_object())
        .map(|object| object.keys().cloned().collect::<Vec<_>>().join(", "))
        .unwrap_or_default();
    if sample.is_empty() {
        top
    } else {
        format!("{top}; item: {sample}")
    }
}

fn save_raw_inventory(inventory: &serde_json::Value) {
    let pretty_json = serde_json::to_string_pretty(inventory).unwrap_or_default();
    let _ = fs::write("raw_inventory.json", &pretty_json);
    let _ = fs::write("../../raw_inventory.json", &pretty_json);
    let js_content = format!("window.rawInventory = {};", pretty_json);
    let _ = fs::write("raw_inventory.js", &js_content);
    let _ = fs::write("../../raw_inventory.js", &js_content);
}

async fn handle_inventory_update(
    inventory: serde_json::Value,
    last_inventory: &Arc<Mutex<Option<serde_json::Value>>>,
    tx_broadcast: &broadcast::Sender<String>,
    state: &SharedState,
) {
    if std::env::args().any(|arg| arg == "--dump-raw") {
        save_raw_inventory(&inventory);
    }
    let (mut translated, mapped_keys, raw_stacks) = translate_inventory(&inventory);
    let loadout_slots = loadout::extract_loadout(&inventory).map(|loadout| {
        let count = loadout.as_object().map(|object| object.len()).unwrap_or(0);
        if let Some(object) = translated.as_object_mut() {
            object.insert("_loadout".to_string(), loadout);
        }
        count
    });
    state.set_sync_stats(mapped_keys, raw_stacks);
    if mapped_keys == 0 {
        state.log(format!(
            "Embark returned {raw_stacks} stacks and no calculator items ({})",
            payload_shape(&inventory)
        ));
    }

    let mut update = false;
    {
        let mut last_inv_lock = last_inventory.lock().unwrap();
        state.set_changes(last_inv_lock.as_ref().map(|old| changed_items(old, &translated)));
        if last_inv_lock.as_ref() != Some(&translated) {
            *last_inv_lock = Some(translated.clone());
            update = true;
        }
    }
    state
        .done_until_ms
        .store(state::now_ms() + 2600, Ordering::Relaxed);

    if update {
        let loadout_note = loadout_slots
            .map(|count| format!(" · loadout {count}"))
            .unwrap_or_default();
        state.log(format!(
            "Inventory synced · {mapped_keys} items{loadout_note}"
        ));
        state.set_inventory(ConnState::Ok, format!("Synced · {mapped_keys} items"));
        let _ = tx_broadcast.send(translated.to_string());
    } else {
        state.set_inventory(ConnState::Ok, format!("Synced · {mapped_keys} items"));
    }
}

/// Calculator items whose count differs between two syncs (worn gear is not counted).
fn changed_items(old: &serde_json::Value, new: &serde_json::Value) -> u64 {
    let empty = serde_json::Map::new();
    let old = old.as_object().unwrap_or(&empty);
    let new = new.as_object().unwrap_or(&empty);
    let mut keys: Vec<&String> = old.keys().chain(new.keys()).collect();
    keys.sort();
    keys.dedup();
    keys.into_iter()
        .filter(|key| !key.starts_with('_'))
        .filter(|key| {
            old.get(*key).map(json_i64).unwrap_or(0) != new.get(*key).map(json_i64).unwrap_or(0)
        })
        .count() as u64
}

// ---------------------------------------------------------------------------
// Background services
// ---------------------------------------------------------------------------

fn allowed_origins() -> Vec<String> {
    let mut origins = vec!["https://arccalc.github.io".to_string()];
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--allow-origin" {
            if let Some(value) = args.get(i + 1) {
                if !origins.iter().any(|origin| origin == value) {
                    origins.push(value.clone());
                }
                i += 2;
                continue;
            }
        }
        i += 1;
    }
    origins
}

fn local_page_origin(origin: &str) -> bool {
    if origin == "null" {
        return true;
    }
    let Some(rest) = origin.strip_prefix("http://") else {
        return false;
    };
    let hostport = rest.split(['/', '?', '#']).next().unwrap_or("");
    let hostport = hostport
        .rsplit_once('@')
        .map(|(_, host)| host)
        .unwrap_or(hostport);
    let host = if let Some(host) = hostport.strip_prefix('[') {
        host.split(']').next().unwrap_or("")
    } else {
        hostport.split(':').next().unwrap_or("")
    };
    host.eq_ignore_ascii_case("localhost") || host == "127.0.0.1" || host == "::1"
}

fn origin_allowed(request: &Request, allowed: &[String]) -> bool {
    let Some(value) = request.headers().get("origin") else {
        return false;
    };
    let Ok(text) = value.to_str() else {
        return false;
    };
    local_page_origin(text) || allowed.iter().any(|origin| origin == text)
}

fn forbidden() -> http::Response<Option<String>> {
    http::Response::builder()
        .status(403)
        .header("Connection", "close")
        .body(Some("Forbidden".to_string()))
        .expect("403 response")
}

async fn accept_calculator(
    stream: tokio::net::TcpStream,
    allowed: &[String],
) -> Result<(
    tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    bool,
)> {
    let allowed = allowed.to_vec();
    let local = Arc::new(AtomicBool::new(false));
    let local_flag = local.clone();
    let stream = tokio_tungstenite::accept_hdr_async(
        stream,
        move |request: &Request, response: Response| {
            let origin = request
                .headers()
                .get("origin")
                .and_then(|value| value.to_str().ok())
                .unwrap_or("");
            local_flag.store(local_page_origin(origin), Ordering::Relaxed);
            if origin_allowed(request, &allowed) {
                Ok(response)
            } else {
                Err(forbidden())
            }
        },
    )
    .await?;
    Ok((stream, local.load(Ordering::Relaxed)))
}

fn start_background(state: Arc<SharedState>) {
    thread::spawn(move || {
        let rt = match tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(e) => {
                state.log(format!("Failed to start runtime: {e}"));
                return;
            }
        };
        rt.block_on(async move {
            if let Err(e) = run_services(state).await {
                // surface fatal errors if any
                eprintln!("service error: {e}");
            }
        });
    });
}

async fn run_services(state: Arc<SharedState>) -> Result<()> {
    state.set_steam(ConnState::Idle, "Open Steam and sign in");
    state.set_inventory(ConnState::Idle, "Open Steam");

    // --- shared inventory ---
    let last_inventory: Arc<Mutex<Option<serde_json::Value>>> = Arc::new(Mutex::new(None));
    let (tx_broadcast, _) = broadcast::channel::<String>(16);
    let ws_clients = Arc::new(std::sync::atomic::AtomicU64::new(0));

    // --- websocket ---
    {
        let tx_broadcast = tx_broadcast.clone();
        let last_inventory = last_inventory.clone();
        let state_ws = state.clone();
        let ws_clients = ws_clients.clone();
        tokio::spawn(async move {
            let addr = "127.0.0.1:8085";
            match TcpListener::bind(addr).await {
                Ok(listener) => {
                    state_ws.set_websocket(ConnState::Ok, format!("Listening on ws://{addr}"));
                    state_ws.log(format!("WebSocket server on ws://{addr}"));
                    let allowed_origins = allowed_origins();
                    while let Ok((stream, _)) = listener.accept().await {
                        let tx_broadcast = tx_broadcast.clone();
                        let last_inventory = last_inventory.clone();
                        let state_ws = state_ws.clone();
                        let ws_clients = ws_clients.clone();
                        let allowed_origins = allowed_origins.clone();
                        tokio::spawn(async move {
                            if let Ok((ws_stream, local_page)) =
                                accept_calculator(stream, &allowed_origins).await
                            {
                                let n = ws_clients.fetch_add(1, Ordering::Relaxed) + 1;
                                state_ws.set_ws_clients(n);
                                state_ws.set_websocket(
                                    ConnState::Ok,
                                    format!("Listening · {n} client(s)"),
                                );
                                state_ws.log("Calculator connected");

                                let (mut ws_sender, mut ws_receiver) = ws_stream.split();
                                let mut rx_broadcast = tx_broadcast.subscribe();

                                let initial_inv = {
                                    last_inventory.lock().unwrap().clone()
                                };
                                if let Some(inv_val) = initial_inv {
                                    if ws_sender
                                        .send(tokio_tungstenite::tungstenite::Message::Text(
                                            inv_val.to_string(),
                                        ))
                                        .await
                                        .is_ok()
                                    {
                                        state_ws.mark_pushed();
                                    }
                                }

                                loop {
                                    tokio::select! {
                                        val = rx_broadcast.recv() => {
                                            if let Ok(msg) = val {
                                                if ws_sender
                                                    .send(tokio_tungstenite::tungstenite::Message::Text(msg))
                                                    .await
                                                    .is_err()
                                                {
                                                    break;
                                                }
                                                state_ws.mark_pushed();
                                            }
                                        }
                                        msg = ws_receiver.next() => {
                                            match msg {
                                                Some(Ok(tokio_tungstenite::tungstenite::Message::Text(text))) => {
                                                    if let Some(view) =
                                                        state::parse_overlay(&text, local_page)
                                                    {
                                                        state_ws.set_overlay(view);
                                                    }
                                                }
                                                None
                                                | Some(Ok(tokio_tungstenite::tungstenite::Message::Close(_)))
                                                | Some(Err(_)) => break,
                                                _ => {}
                                            }
                                        }
                                    }
                                }

                                let n = ws_clients
                                    .fetch_sub(1, Ordering::Relaxed)
                                    .saturating_sub(1);
                                state_ws.set_ws_clients(n);
                                state_ws.set_websocket(
                                    ConnState::Ok,
                                    format!("Listening · {n} client(s)"),
                                );
                                state_ws.log("Calculator disconnected");
                            }
                        });
                    }
                }
                Err(e) => {
                    state_ws.set_websocket(ConnState::Error, format!("Bind failed: {e}"));
                    state_ws.log(format!("WebSocket bind error: {e}"));
                }
            }
        });
    }

    // --- poller ---
    {
        let last_inventory = last_inventory.clone();
        let tx_broadcast = tx_broadcast.clone();
        let state_poll = state.clone();
        tokio::spawn(async move {
            if state_poll.auto_sync.load(Ordering::Relaxed) {
                state_poll.force_poll.store(true, Ordering::Relaxed);
            }
            let mut elapsed_ms: u64 = 0;
            loop {
                tokio::time::sleep(Duration::from_millis(200)).await;
                elapsed_ms += 200;

                let force = state_poll.force_poll.swap(false, Ordering::Relaxed);
                let auto = state_poll.auto_sync.load(Ordering::Relaxed);
                let interval_ms =
                    state_poll.poll_interval_secs.load(Ordering::Relaxed).max(30) * 1000;
                // The window counts down to this moment.
                let next = if auto {
                    crate::state::now_ms() + interval_ms.saturating_sub(elapsed_ms)
                } else {
                    0
                };
                state_poll.next_poll_ms.store(next, Ordering::Relaxed);

                if !crate::state::due(force, auto, elapsed_ms, interval_ms) {
                    if !auto {
                        elapsed_ms = 0;
                    }
                    continue;
                }
                elapsed_ms = 0;

                // After «Sign out» only a click on «Sign in» may open a new Steam session.
                if !force
                    && state_poll.signed_out.load(Ordering::Relaxed)
                    && !embark_auth::has_session()
                {
                    continue;
                }
                state_poll.signed_out.store(false, Ordering::Relaxed);

                let token = if let Some(token) = embark_auth::cached_token() {
                    token
                } else {
                    state_poll.set_steam(ConnState::Working, "Asking Steam…");
                    state_poll.set_inventory(ConnState::Working, "Signing in…");
                    match tokio::task::spawn_blocking(embark_auth::current_token).await {
                        Ok(Ok(token)) => token,
                        Ok(Err(err)) => {
                            let message = format!("{err:#}");
                            state_poll.set_steam(ConnState::Error, message.clone());
                            state_poll.set_inventory(ConnState::Idle, "Open Steam");
                            state_poll.log(format!("Steam sign-in: {}", embark_auth::redact(&message)));
                            continue;
                        }
                        Err(err) => {
                            state_poll.set_steam(ConnState::Error, "Sign-in failed");
                            state_poll.set_inventory(ConnState::Error, "Sync failed");
                            state_poll.log(format!("Sign-in task: {err}"));
                            continue;
                        }
                    }
                };

                state_poll.set_steam(ConnState::Ok, "Signed in");
                state_poll.prepared.store(true, Ordering::Relaxed);
                state_poll.set_token(ConnState::Ok, "Steam sign-in");
                state_poll.set_inventory(ConnState::Working, "Querying Embark…");

                let mut attempt = query_inventory(&token, None).await;
                if let Err(ref err) = attempt {
                    if format!("{err:#}").contains("status 401") {
                        embark_auth::invalidate();
                        if let Ok(Ok(fresh)) =
                            tokio::task::spawn_blocking(embark_auth::current_token).await
                        {
                            attempt = query_inventory(&fresh, None).await;
                        }
                    }
                }

                match attempt {
                    Ok(inventory) => {
                        handle_inventory_update(
                            inventory,
                            &last_inventory,
                            &tx_broadcast,
                            &state_poll,
                        )
                        .await;
                    }
                    Err(err) => {
                        let message = embark_auth::redact(&format!("{err:#}"));
                        let detail = if message.contains(crate::i18n::BUILD_NOT_READY) {
                            crate::i18n::BUILD_NOT_READY.to_string()
                        } else {
                            message
                        };
                        state_poll.set_inventory(ConnState::Error, detail.clone());
                        state_poll.log(format!("Inventory error: {detail}"));
                        // Retry in 30 s instead of waiting for the whole interval.
                        elapsed_ms = interval_ms.saturating_sub(30_000);
                    }
                }
            }
        });
    }

    loop {
        if state.shutdown.load(Ordering::Relaxed) {
            break;
        }
        // Old button restarted Steam. Treat a leftover request as "sync now".
        if state.prepare_requested.swap(false, Ordering::Relaxed) {
            state.force_poll.store(true, Ordering::Relaxed);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    Ok(())
}

fn probe_report(token: &str) -> String {
    let http = match reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
    {
        Ok(http) => http,
        Err(err) => return format!("http err {err}"),
    };
    let mut lines = Vec::new();
    let mut manifest_ids = Vec::new();
    if let Some(claims) = jwt_claims(token) {
        collect_manifests(&claims, &mut manifest_ids);
        lines.push(format!(
            "token claims={} manifests={}",
            json_keys(&claims),
            manifest_ids.len()
        ));
        if let Some(ext) = claims.get("ext") {
            lines.push(format!("ext {}", key_tree(ext, 0)));
            collect_manifests(ext, &mut manifest_ids);
        }
    }
    let profile = signed_call(
        &http,
        "GET",
        "https://api-gateway.europe.es-pio.net/v1/shared/profile",
        token,
        None,
        None,
    );
    collect_manifests(&profile.body, &mut manifest_ids);
    lines.push(format!(
        "profile status={} keys={} manifests={}",
        profile.status,
        json_keys(&profile.body),
        manifest_ids.join(",")
    ));

    for (method, path, body) in [
        ("GET", "/v1/shared/manifest", None),
        ("POST", "/v1/shared/manifest", Some("{}")),
        ("POST", "/v1/shared/manifest", Some(r#"{"appId":"1808500"}"#)),
        ("GET", "/v1/pioneer/manifest", None),
        ("GET", "/v1/pioneer/inventory", None),
        ("POST", "/v1/pioneer/inventory", Some("{}")),
        ("GET", "/v1/pioneer/inventory/v1", None),
        ("GET", "/v1/pioneer/stash", None),
        ("GET", "/v1/pioneer/quests", None),
        ("GET", "/v1/shared/manifest/latest", None),
        ("GET", "/v1/shared/builds", None),
        ("GET", "/v1/shared/bootstrap", None),
        ("GET", "/v1/pioneer/bootstrap", None),
        ("POST", "/v1/shared/manifest/latest", Some("{}")),
    ] {
        let url = format!("https://api-gateway.europe.es-pio.net{path}");
        let hit = signed_call(&http, method, &url, token, None, body);
        collect_manifests(&hit.body, &mut manifest_ids);
        lines.push(format!(
            "{method} {path} body={} status={} note={}",
            body.unwrap_or("-"),
            hit.status,
            hit.note
        ));
    }

    for path in [
        "/v1/shared/manifest:refetch",
        "/v1/shared/manifest/refetch",
        "/v1/shared/refetch-manifest",
        "/v1/pioneer/manifest:refetch",
        "/v1/shared/client-manifest",
        "/v1/shared/artifacts",
        "/v1/shared/versions",
        "/v1/pioneer/version",
        "/v1/pioneer/metadata",
        "/v1/shared/build",
        "/v1/pioneer/build",
        "/v1/shared/content",
        "/v1/pioneer/content",
        "/v1/shared/game-data",
        "/v1/pioneer/game-data",
        "/v1/shared/config",
        "/v1/pioneer/config",
    ] {
        for method in ["GET", "POST"] {
            let body = if method == "POST" { Some("{}") } else { None };
            let url = format!("https://api-gateway.europe.es-pio.net{path}");
            let hit = signed_call(&http, method, &url, token, None, body);
            collect_manifests(&hit.body, &mut manifest_ids);
            if hit.status == 404 && hit.candidates.is_empty() && !hit.note.contains("digits=") {
                continue;
            }
            lines.push(format!(
                "{method} {path} status={} note={}",
                hit.status, hit.note
            ));
        }
    }

    let grpc_body = "\u{0}\u{0}\u{0}\u{0}\u{0}";
    for (path, ctype) in [
        ("/v1/shared/manifest", "application/grpc"),
        ("/v1/shared/manifest", "application/grpc-web+proto"),
        ("/v1/pioneer/manifest", "application/grpc"),
        ("/v1/shared/manifest:refetch", "application/grpc"),
    ] {
        let url = format!("https://api-gateway.europe.es-pio.net{path}");
        let hit = signed_call_typed(&http, "POST", &url, token, None, Some(grpc_body), Some(ctype));
        lines.push(format!(
            "POST {path} ctype={ctype} status={} note={}",
            hit.status, hit.note
        ));
    }

    let stale = "6393983602370192963";
    for path in [
        format!("/v1/shared/manifest/{stale}"),
        format!("/v1/pioneer/manifest/{stale}"),
        "/v1/shared/manifest/current".to_string(),
        "/v1/shared/manifest/latest".to_string(),
    ] {
        let url = format!("https://api-gateway.europe.es-pio.net{path}");
        let hit = signed_call(&http, "GET", &url, token, None, None);
        lines.push(format!("GET {path} status={} note={}", hit.status, hit.note));
        for candidate in &hit.candidates {
            lines.push(format!("path-candidate len={}", candidate.len()));
        }
    }
    for body in [
        r#"{"manifest_id":0}"#,
        r#"{"manifest_id":-1}"#,
        r#"{"manifest_id":6393983602370192963}"#,
        r#"{"manifest_id":6833066887505174904}"#,
        r#"{"manifest_id":7673634727303110182}"#,
        r#"{"app_id":"x","build_id":"x","platform":"x","tenancy":"x","hash":"x","version":"x","changelist":"x","latest":"x","id":"x"}"#,
    ] {
        let url = "https://api-gateway.europe.es-pio.net/v1/shared/manifest";
        let hit = signed_call(&http, "POST", url, token, None, Some(body));
        collect_manifests(&hit.body, &mut manifest_ids);
        if let Some(id) = hit.body.get("id").and_then(|value| value.as_u64()) {
            let text = id.to_string();
            let tail = &text[text.len().saturating_sub(4)..];
            lines.push(format!("response-id len={} tail={tail}", text.len()));
            if !manifest_ids.iter().any(|item| item == &text) {
                manifest_ids.push(text);
            }
        }
        if let Some(text) = hit.body.get("buildId").and_then(|value| value.as_str()) {
            if text.len() <= 80 && !text.contains('@') {
                lines.push(format!("buildId {text}"));
            } else {
                lines.push(format!("buildId {}", skeleton(text)));
            }
        }
        for key in ["validFrom", "validUntil", "retrievedAt"] {
            if let Some(stamp) = hit.body.get(key).and_then(|value| value.as_u64()) {
                lines.push(format!("{key} {}", unix_note(stamp)));
            }
        }
        lines.push(format!("manifest-strings {}", string_notes(&hit.body)));
        lines.push(format!(
            "POST manifest numeric status={} note={}",
            hit.status, hit.note
        ));
        for candidate in &hit.candidates {
            if !manifest_ids.iter().any(|item| item == candidate) {
                manifest_ids.push(candidate.clone());
            }
            lines.push(format!("body-candidate len={}", candidate.len()));
        }
    }
    let game = signed_call(
        &http,
        "GET",
        "https://api-gateway.europe.es-pio.net/v1/pioneer/game-data",
        token,
        Some(stale),
        None,
    );
    lines.push(format!(
        "game-data stale status={} note={}",
        game.status, game.note
    ));

    let local_ids = local_digit_candidates();
    lines.push(format!("local id candidates={}", local_ids.len()));
    for candidate in local_ids.into_iter().take(15) {
        if !manifest_ids.iter().any(|item| item == &candidate) {
            manifest_ids.push(candidate);
        }
    }
    for sample in [
        "1",
        "6393983602370192963",
        "1389382",
        "1315578",
        "4879539862843747573",
        "6833066887505174904",
        "7673634727303110182",
    ] {
        if !manifest_ids.iter().any(|item| item == sample) {
            manifest_ids.push(sample.to_string());
        }
    }
    manifest_ids.sort();
    manifest_ids.dedup();
    for manifest in &manifest_ids {
        let url = "https://api-gateway.europe.es-pio.net/v1/pioneer/inventory";
        let hit = signed_call(&http, "GET", url, token, Some(manifest), None);
        let tag = if manifest.len() >= 8 {
            format!("{}..{}", &manifest[..4], &manifest[manifest.len() - 4..])
        } else {
            manifest.clone()
        };
        lines.push(format!(
            "inventory id={tag} len={} status={} note={}",
            manifest.len(),
            hit.status,
            hit.note
        ));
        if hit.status == 200 {
            lines.push(format!("inventory shape={}", json_keys(&hit.body)));
        }
        for candidate in &hit.candidates {
            if manifest_ids.iter().any(|item| item == candidate) {
                continue;
            }
            let again = signed_call(&http, "GET", url, token, Some(candidate), None);
            lines.push(format!(
                "inventory follow len={} status={} note={}",
                candidate.len(),
                again.status,
                again.note
            ));
            if again.status == 200 {
                lines.push(format!("inventory shape={}", json_keys(&again.body)));
            }
        }
    }
    lines.join("\n")
}

struct ProbeHit {
    status: u16,
    note: String,
    body: serde_json::Value,
    candidates: Vec<String>,
}

fn signed_call(
    http: &reqwest::blocking::Client,
    method: &str,
    url: &str,
    token: &str,
    manifest: Option<&str>,
    body: Option<&str>,
) -> ProbeHit {
    signed_call_typed(http, method, url, token, manifest, body, None)
}

fn signed_call_typed(
    http: &reqwest::blocking::Client,
    method: &str,
    url: &str,
    token: &str,
    manifest: Option<&str>,
    body: Option<&str>,
    content_type: Option<&str>,
) -> ProbeHit {
    let mut request = http
        .request(method.parse().unwrap_or(reqwest::Method::GET), url)
        .header("Authorization", format!("Bearer {token}"))
        .header(
            "User-Agent",
            "EmbarkGameBoot/1.0 (Windows; 10.0.19045.1.0.64bit)",
        )
        .header("x-embark-telemetry-client-platform", "3");
    if let Some(manifest) = manifest {
        request = request.header("x-embark-manifest-id", manifest);
    }
    if let Some(body) = body {
        request = request
            .header(
                "Content-Type",
                content_type.unwrap_or("application/json"),
            )
            .body(body.to_string());
    }
    match request.send() {
        Ok(response) => {
            let status = response.status().as_u16();
            let header_note = header_notes(response.headers());
            let text = response.text().unwrap_or_default();
            let parsed = serde_json::from_str::<serde_json::Value>(&text)
                .unwrap_or(serde_json::Value::Null);
            let candidates = candidate_ids(&text);
            let note = {
                let body_note = if parsed.is_null() {
                    format!("skel={}", skeleton(&text))
                } else {
                    json_keys(&parsed)
                };
                format!(
                    "blen={} {header_note} body={body_note} candidates={}",
                    text.len(),
                    candidates.len()
                )
            };
            ProbeHit {
                status,
                note,
                body: parsed,
                candidates,
            }
        }
        Err(err) => ProbeHit {
            status: 0,
            note: embark_auth::redact(&err.to_string()),
            body: serde_json::Value::Null,
            candidates: Vec::new(),
        },
    }
}

fn unix_note(stamp: u64) -> String {
    let seconds = if stamp > 1_000_000_000_000 {
        stamp / 1000
    } else {
        stamp
    };
    if !(1_500_000_000..2_200_000_000).contains(&seconds) {
        return format!("len={}", stamp.to_string().len());
    }
    let days = (seconds / 86_400) as i64;
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

fn string_notes(value: &serde_json::Value) -> String {
    let mut notes = Vec::new();
    walk_strings(value, &mut notes);
    if notes.is_empty() {
        "-".to_string()
    } else {
        notes.join(" ")
    }
}

fn walk_strings(value: &serde_json::Value, notes: &mut Vec<String>) {
    if notes.len() >= 40 {
        return;
    }
    match value {
        serde_json::Value::Object(object) => {
            for (key, child) in object {
                if let Some(text) = child.as_str() {
                    if text.chars().all(|ch| ch.is_ascii_digit()) && text.len() >= 6 {
                        let tail = &text[text.len().saturating_sub(4)..];
                        notes.push(format!("{key}:digits{}:{tail}", text.len()));
                    } else if let Some(host) = text
                        .strip_prefix("https://")
                        .or_else(|| text.strip_prefix("http://"))
                    {
                        let host = host.split('/').next().unwrap_or(host);
                        notes.push(format!("{key}:url:{host}"));
                    }
                }
                walk_strings(child, notes);
            }
        }
        serde_json::Value::Array(items) => {
            for child in items.iter().take(3) {
                walk_strings(child, notes);
            }
        }
        _ => {}
    }
}

fn header_notes(headers: &reqwest::header::HeaderMap) -> String {
    let mut parts = Vec::new();
    for (name, value) in headers.iter() {
        let key = name.as_str().to_ascii_lowercase();
        if key.contains("cookie") || key == "authorization" || key.contains("token") {
            continue;
        }
        let Ok(text) = value.to_str() else {
            continue;
        };
        let digits = digit_runs(text);
        let interesting = key.contains("manifest")
            || key.contains("build")
            || key.contains("version")
            || key == "allow"
            || key == "content-type"
            || key.starts_with("grpc-");
        if digits.is_empty() && !interesting {
            continue;
        }
        let shown = if !digits.is_empty() {
            format!("digits={digits}")
        } else if text.len() <= 80 && !text.contains('@') {
            skeleton(text)
        } else {
            format!("len={}", text.len())
        };
        parts.push(format!("{key}={shown}"));
    }
    if parts.is_empty() {
        "headers=-".to_string()
    } else {
        parts.join(" ")
    }
}

fn json_keys(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Object(object) => object
            .iter()
            .map(|(key, child)| format!("{key}:{}", json_kind(child)))
            .collect::<Vec<_>>()
            .join(","),
        serde_json::Value::Array(items) => format!("array{}", items.len()),
        serde_json::Value::Null => "empty".into(),
        _ => json_kind(value).into(),
    }
}

fn key_tree(value: &serde_json::Value, depth: usize) -> String {
    if depth > 2 {
        return json_kind(value).into();
    }
    match value {
        serde_json::Value::Object(object) => object
            .iter()
            .map(|(key, child)| format!("{key}={}", key_tree(child, depth + 1)))
            .collect::<Vec<_>>()
            .join(","),
        serde_json::Value::Array(items) => format!("array{}", items.len()),
        _ => json_kind(value).into(),
    }
}

fn json_kind(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Object(_) => "object",
        serde_json::Value::Array(items) => {
            if items.is_empty() {
                "array0"
            } else {
                "array"
            }
        }
        serde_json::Value::String(_) => "string",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::Bool(_) => "bool",
        serde_json::Value::Null => "null",
    }
}

fn local_digit_candidates() -> Vec<String> {
    let mut found = Vec::new();
    let root = PathBuf::from(r"C:\Users\Ilyuha\AppData\Local\PioneerGame\Saved\SaveGames");
    let Ok(entries) = fs::read_dir(&root) else {
        return found;
    };
    for entry in entries.flatten() {
        let Ok(data) = fs::read(entry.path()) else {
            continue;
        };
        if data.len() > 2_000_000 {
            continue;
        }
        push_digit_runs(&data, 1, &mut found);
        push_digit_runs(&data, 2, &mut found);
    }
    found.sort();
    found.dedup();
    found
}

fn push_digit_runs(data: &[u8], step: usize, found: &mut Vec<String>) {
    let mut current = String::new();
    let mut index = 0;
    while index + step <= data.len() {
        let byte = data[index];
        let digit = byte.is_ascii_digit() && (step == 1 || data[index + 1] == 0);
        if digit {
            current.push(byte as char);
        } else if (16..=20).contains(&current.len()) {
            found.push(std::mem::take(&mut current));
        } else {
            current.clear();
        }
        index += step;
    }
    if (16..=20).contains(&current.len()) {
        found.push(current);
    }
}

fn candidate_ids(text: &str) -> Vec<String> {
    let mut ids = digit_runs_vec(text);
    let trimmed = text.trim().trim_matches('"');
    if let Ok(bytes) = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, trimmed)
    {
        ids.extend(digit_runs_vec(&String::from_utf8_lossy(&bytes)));
    }
    ids.sort();
    ids.dedup();
    ids
}

fn digit_runs_vec(text: &str) -> Vec<String> {
    digit_runs(text)
        .split(',')
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .collect()
}

fn skeleton(text: &str) -> String {
    let mut out = String::new();
    let mut run = String::new();
    let flush = |run: &mut String, out: &mut String| {
        if run.len() >= 8 {
            out.push('{');
            out.push_str(&run.len().to_string());
            out.push('}');
        } else {
            out.push_str(run);
        }
        run.clear();
    };
    for ch in text.chars().take(240) {
        if ch.is_ascii_alphanumeric() {
            run.push(ch);
        } else {
            flush(&mut run, &mut out);
            out.push(ch);
        }
    }
    flush(&mut run, &mut out);
    out
}

fn digit_runs(text: &str) -> String {
    let mut runs = Vec::new();
    let mut current = String::new();
    for ch in text.chars().chain(std::iter::once(' ')) {
        if ch.is_ascii_digit() {
            current.push(ch);
        } else if (15..=22).contains(&current.len()) {
            runs.push(current.clone());
            current.clear();
        } else {
            current.clear();
        }
    }
    runs.join(",")
}

fn jwt_claims(token: &str) -> Option<serde_json::Value> {
    let payload = token.split('.').nth(1)?;
    let mut padded = payload.replace('-', "+").replace('_', "/");
    while padded.len() % 4 != 0 {
        padded.push('=');
    }
    let bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, padded).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn collect_manifests(value: &serde_json::Value, found: &mut Vec<String>) {
    match value {
        serde_json::Value::Object(object) => {
            for (key, child) in object {
                let name = key.to_ascii_lowercase();
                if name.contains("manifest") || name.contains("build") {
                    if let Some(number) = child.as_u64() {
                        found.push(number.to_string());
                    } else if let Some(text) = child.as_str() {
                        if text.chars().all(|ch| ch.is_ascii_digit()) && text.len() >= 6 {
                            found.push(text.to_string());
                        }
                    }
                }
                collect_manifests(child, found);
            }
        }
        serde_json::Value::Array(items) => {
            for child in items.iter().take(3) {
                collect_manifests(child, found);
            }
        }
        _ => {}
    }
}

fn sync_once_report() -> String {
    let token = match embark_auth::current_token() {
        Ok(token) => token,
        Err(err) => return format!("auth err {}", embark_auth::redact(&format!("{err:#}"))),
    };
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(err) => return format!("runtime err {err}"),
    };
    match runtime.block_on(query_inventory(&token, None)) {
        Ok(inventory) => {
            save_raw_inventory(&inventory);
            let (_translated, keys, raw) = translate_inventory(&inventory);
            format!(
                "ok mapped={keys} raw={raw} shape={} auth_fields={}",
                payload_shape(&inventory),
                embark_auth::last_auth_fields()
            )
        }
        Err(err) => format!(
            "inventory err {}",
            embark_auth::redact(&format!("{err:#}"))
        ),
    }
}

// ---------------------------------------------------------------------------
// Entry
// ---------------------------------------------------------------------------

fn main() {
    let mut args = std::env::args();
    let _exe = args.next();
    if args.next().as_deref() == Some("--embark-auth") {
        let path = args.next().unwrap_or_default();
        if let Err(err) = embark_auth::write_steam_handoff(std::path::Path::new(&path)) {
            let payload = serde_json::json!({
                "error": embark_auth::redact(&format!("{err:#}")),
            });
            let _ = std::fs::write(&path, payload.to_string());
        }
        return;
    }
    if std::env::args().any(|arg| arg == "--sync-once") {
        let report = sync_once_report();
        let _ = fs::write("sync-once-result.txt", &report);
        return;
    }
    if std::env::args().any(|arg| arg == "--probe-stash") {
        let report = match embark_auth::current_token() {
            Ok(token) => probe_report(&token),
            Err(err) => format!("auth err {}", embark_auth::redact(&format!("{err:#}"))),
        };
        let _ = fs::write("stash-probe.txt", &report);
        return;
    }
    // Starting the app again shows the running copy. Debug builds driven over
    // DevTools (STASHSYNC_CDP) may run next to an installed copy.
    let test_build = cfg!(debug_assertions) && std::env::var_os("STASHSYNC_CDP").is_some();
    if !test_build && !win::first_instance() {
        return;
    }

    let config = HelperConfig::load();
    config::apply_run_at_startup(config.run_at_startup);
    // Windows start: stay in the tray unless there is still a sign-in to do.
    let start_in_tray = std::env::args().any(|arg| arg == config::TRAY_ARG)
        && !config.signed_out
        && embark_auth::has_session();
    let state = SharedState::new(config.poll_interval_secs, config.auto_sync);
    state.signed_out.store(config.signed_out, Ordering::Relaxed);
    state.log("StashSync started");
    state.log(format!(
        "Poll interval: {}s (config: {:?})",
        config.poll_interval_secs,
        HelperConfig::config_path()
    ));

    if config.auto_prepare_steam {
        state.force_poll.store(true, Ordering::Relaxed);
    }

    start_background(state.clone());
    app::run(state, config, start_in_tray);
}

#[cfg(test)]
mod build_label_tests {
    use super::manifest_label_candidates;

    #[test]
    fn keeps_a_three_part_build_id() {
        let version = "CL1389382-7EAFFE60-BK4740";
        assert_eq!(manifest_label_candidates(version), vec![version.to_string()]);
    }

    #[test]
    fn tries_the_short_form_before_the_full_build_id() {
        let version = "CL1424961-ABF27201-BK4845-7415387169922308700";
        assert_eq!(
            manifest_label_candidates(version),
            vec![
                "CL1424961-ABF27201-BK4845".to_string(),
                version.to_string(),
            ]
        );
    }

    #[test]
    fn allows_a_local_file_and_a_local_preview() {
        assert!(super::local_page_origin("null"));
        assert!(super::local_page_origin("http://127.0.0.1:5500"));
        assert!(super::local_page_origin("http://localhost:8080"));
        assert!(!super::local_page_origin("https://arccalc.github.io"));
        assert!(!super::local_page_origin("http://127.0.0.1.evil.example"));
    }

    #[test]
    fn keeps_the_short_form_when_a_later_part_is_not_a_build_id() {
        let version = "CL1424961-ABF27201-BK4845-not a build";
        assert_eq!(
            manifest_label_candidates(version),
            vec!["CL1424961-ABF27201-BK4845".to_string()]
        );
    }
}
