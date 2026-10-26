//! Sign the local Steam session into Embark and return an access token.
//! Steam is contacted only from a short helper process with no window, and
//! that process shuts Steam down before it exits. The calculator window
//! itself never becomes the game, so the Steam overlay is not drawn into it.
//! Later stash reads use the saved Embark session and do not need the game
//! to be closed. Steam must be open and signed in for the first sign-in,
//! and that first sign-in needs the game itself to be closed.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

use anyhow::{anyhow, bail, Result};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use sha2::{Digest, Sha256};
use steamworks::TicketForWebApiResponse;

const TOKEN_URL: &str = "https://auth.embark.net/oauth2/token?skip_link=false";
const CLIENT_ID: &str = "embark-pioneer";
/// Public Pioneer client credential. The token endpoint requires it together
/// with a signature over the exact form body. It is not a user password and
/// must not be written to the log.
const CLIENT_SECRET: &str = "+GoAQg2vzgcohjnW0PKtfiMjLfvSTfcjsyJ8YqH3DuE=";
const AUDIENCE: &str = "https://pioneer.embark.net";
const TENANCY: &str = "pioneer-live";
const APP_ID: &str = "1808500";
const USER_AGENT: &str = "EmbarkGameBoot/1.0 (Windows; 10.0.19045.1.0.64bit)";
/// `embark-auth` sits next to the Steam sign-in errors in the current client.
/// The other two are fallbacks if Embark rejects that identity.
const IDENTITIES: &[&str] = &["embark-auth", "", "pioneer"];

#[derive(Clone)]
struct CachedToken {
    value: String,
    refresh_at: Instant,
    refresh_token: Option<String>,
}

struct TokenGrant {
    access_token: String,
    expires_in: u64,
    refresh_token: Option<String>,
    fields: Vec<String>,
}

struct Cache {
    token: Option<CachedToken>,
}

fn field_names() -> &'static Mutex<String> {
    static FIELDS: OnceLock<Mutex<String>> = OnceLock::new();
    FIELDS.get_or_init(|| Mutex::new(String::new()))
}

pub fn last_auth_fields() -> String {
    field_names()
        .lock()
        .map(|guard| guard.clone())
        .unwrap_or_default()
}

fn cache() -> &'static Mutex<Cache> {
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    CACHE.get_or_init(|| {
        Mutex::new(Cache { token: None })
    })
}

pub fn invalidate() {
    if let Ok(mut guard) = cache().lock() {
        if let Some(token) = guard.token.as_mut() {
            token.value.clear();
            token.refresh_at = Instant::now() - Duration::from_secs(1);
        }
    }
    if let Some(mut session) = load_session() {
        session.access_token.clear();
        session.refresh_unix = 0;
        let _ = save_session(&session);
    }
}

/// True while this computer holds an Embark sign-in that can still be used or refreshed.
pub fn has_session() -> bool {
    if cached_token().is_some() {
        return true;
    }
    load_session().is_some_and(|session| {
        session.refresh_token.is_some()
            || (!session.access_token.is_empty() && session.refresh_unix > unix_now())
    })
}

/// «Sign out»: forget the token in memory and delete the saved session.
pub fn sign_out() {
    if let Ok(mut guard) = cache().lock() {
        guard.token = None;
    }
    if let Ok(path) = session_path() {
        let _ = fs::remove_file(path);
    }
}

pub fn cached_token() -> Option<String> {
    let guard = cache().lock().ok()?;
    let token = guard.token.as_ref()?;
    if Instant::now() < token.refresh_at && !token.value.is_empty() {
        Some(token.value.clone())
    } else {
        None
    }
}

pub fn current_token() -> Result<String> {
    if let Some(token) = cached_token() {
        return Ok(token);
    }
    if let Some(token) = restore_saved_token() {
        return Ok(token);
    }

    let refresh = saved_refresh_token();
    if let Some(refresh) = refresh.as_deref() {
        if let Ok(grant) = refresh_grant(refresh) {
            return Ok(store_grant(grant));
        }
    }

    if game_is_running() {
        bail!("Close the game once to sign in");
    }

    let grant = steam_grant_from_child()?;
    Ok(store_grant(grant))
}

pub fn write_steam_handoff(path: &Path) -> Result<()> {
    let result = sign_in_with_steam();
    let payload = match result {
        Ok(grant) => serde_json::json!({
            "access_token": grant.access_token,
            "expires_in": grant.expires_in,
            "refresh_token": grant.refresh_token,
            "fields": grant.fields,
        }),
        Err(reject) => serde_json::json!({
            "error": redact(&reject.message),
        }),
    };
    write_json(path, &payload)
}

fn sign_in_with_steam() -> Result<TokenGrant, Reject> {
    let client = match steamworks::Client::init_app(crate::ARC_RAIDERS_STEAM_APP_ID) {
        Ok(client) => client,
        Err(err) => {
            return Err(Reject {
                retry: false,
                message: friendly_init(&err.to_string()),
            });
        }
    };
    let outcome = sign_in_with_client(&client);
    // SteamAPI_Shutdown runs when the last client is dropped. The process
    // then exits, which clears the "game is running" state.
    drop(client);
    std::thread::sleep(Duration::from_millis(300));
    outcome
}

fn sign_in_with_client(client: &steamworks::Client) -> Result<TokenGrant, Reject> {
    if !client.user().logged_on() {
        return Err(Reject {
            retry: false,
            message: "Steam is open, but nobody is signed in".into(),
        });
    }
    for _ in 0..5 {
        client.run_callbacks();
        std::thread::sleep(Duration::from_millis(20));
    }
    let persona = client.friends().name();
    let persona = if persona.is_empty() {
        "player".to_string()
    } else {
        persona
    };

    let mut last_reject = None;
    for identity in IDENTITIES {
        let ticket = match web_ticket(client, identity) {
            Ok(ticket) => ticket,
            Err(err) => {
                return Err(Reject {
                    retry: false,
                    message: redact(&format!("{err:#}")),
                });
            }
        };
        match exchange(&persona, &ticket) {
            Ok(grant) => return Ok(grant),
            Err(reject) if reject.retry => last_reject = Some(reject.message),
            Err(reject) => return Err(reject),
        }
    }
    Err(Reject {
        retry: false,
        message: last_reject.unwrap_or_else(|| "Embark rejected the Steam ticket".into()),
    })
}

fn steam_grant_from_child() -> Result<TokenGrant> {
    let dir = session_dir()?;
    fs::create_dir_all(&dir)?;
    let handoff = dir.join("embark-auth-handoff.json");
    let _ = fs::remove_file(&handoff);
    let exe = std::env::current_exe()?;
    let mut command = Command::new(&exe);
    command
        .arg("--embark-auth")
        .arg(&handoff)
        .current_dir(exe.parent().unwrap_or(Path::new(".")))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    let status = command
        .status()
        .map_err(|_| anyhow!("Open Steam and sign in"))?;
    let text = fs::read_to_string(&handoff).map_err(|_| anyhow!("Open Steam and sign in"))?;
    let _ = fs::remove_file(&handoff);
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|_| anyhow!("Open Steam and sign in"))?;
    if let Some(error) = value.get("error").and_then(|item| item.as_str()) {
        bail!("{}", redact(error));
    }
    if !status.success() && value.get("access_token").and_then(|item| item.as_str()).is_none() {
        bail!("Open Steam and sign in");
    }
    grant_from_json(&value).map_err(|reject| anyhow!(reject.message))
}

fn store_grant(grant: TokenGrant) -> String {
    if let Ok(mut fields) = field_names().lock() {
        *fields = grant.fields.join(",");
    }
    let refresh_token = grant.refresh_token.clone();
    let cached = cache_from_grant(&grant);
    let access = cached.value.clone();
    if let Ok(mut guard) = cache().lock() {
        guard.token = Some(cached);
    }
    let _ = save_session(&DiskSession {
        access_token: access.clone(),
        refresh_unix: unix_after(grant.expires_in),
        refresh_token,
    });
    access
}

fn cache_from_grant(grant: &TokenGrant) -> CachedToken {
    let ttl = grant.expires_in.saturating_sub(120).max(30);
    CachedToken {
        value: grant.access_token.clone(),
        refresh_at: Instant::now() + Duration::from_secs(ttl),
        refresh_token: grant.refresh_token.clone(),
    }
}

fn restore_saved_token() -> Option<String> {
    let session = load_session()?;
    let now = unix_now();
    if session.access_token.is_empty() || session.refresh_unix <= now.saturating_add(15) {
        return None;
    }
    let remain = session.refresh_unix.saturating_sub(now);
    let cached = CachedToken {
        value: session.access_token.clone(),
        refresh_at: Instant::now() + Duration::from_secs(remain),
        refresh_token: session.refresh_token,
    };
    let access = cached.value.clone();
    if let Ok(mut guard) = cache().lock() {
        guard.token = Some(cached);
    }
    Some(access)
}

fn saved_refresh_token() -> Option<String> {
    if let Ok(guard) = cache().lock() {
        if let Some(token) = guard
            .token
            .as_ref()
            .and_then(|token| token.refresh_token.clone())
            .filter(|token| !token.is_empty())
        {
            return Some(token);
        }
    }
    load_session().and_then(|session| {
        session
            .refresh_token
            .filter(|token| !token.is_empty())
    })
}

fn game_is_running() -> bool {
    let mut command = Command::new("tasklist");
    command
        .args(["/FI", "IMAGENAME eq PioneerGame.exe", "/NH"])
        .stdin(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    let Ok(output) = command.output() else {
        return false;
    };
    String::from_utf8_lossy(&output.stdout)
        .to_ascii_lowercase()
        .contains("pioneergame.exe")
}

#[derive(Clone)]
struct DiskSession {
    access_token: String,
    refresh_unix: u64,
    refresh_token: Option<String>,
}

fn session_dir() -> Result<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA").ok_or_else(|| anyhow!("Open Steam and sign in"))?;
    Ok(PathBuf::from(base).join("ArcCalc"))
}

fn session_path() -> Result<PathBuf> {
    Ok(session_dir()?.join("embark-session.json"))
}

fn load_session() -> Option<DiskSession> {
    let path = session_path().ok()?;
    let text = fs::read_to_string(path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    Some(DiskSession {
        access_token: value
            .get("access_token")
            .and_then(|item| item.as_str())
            .unwrap_or("")
            .to_string(),
        refresh_unix: value.get("refresh_unix").and_then(|item| item.as_u64()).unwrap_or(0),
        refresh_token: value
            .get("refresh_token")
            .and_then(|item| item.as_str())
            .filter(|token| !token.is_empty())
            .map(str::to_string),
    })
}

fn save_session(session: &DiskSession) -> Result<()> {
    let path = session_path()?;
    write_json(
        &path,
        &serde_json::json!({
            "access_token": session.access_token,
            "refresh_unix": session.refresh_unix,
            "refresh_token": session.refresh_token,
        }),
    )
}

fn write_json(path: &Path, value: &serde_json::Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_string(value)?)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn unix_after(expires_in: u64) -> u64 {
    unix_now().saturating_add(expires_in.saturating_sub(120).max(30))
}

fn grant_from_json(value: &serde_json::Value) -> Result<TokenGrant, Reject> {
    let access_token = value
        .get("access_token")
        .and_then(|item| item.as_str())
        .filter(|token| !token.is_empty())
        .ok_or_else(|| Reject {
            retry: false,
            message: "Open Steam and sign in".into(),
        })?
        .to_string();
    let expires_in = value
        .get("expires_in")
        .and_then(|item| item.as_u64())
        .unwrap_or(20 * 60);
    let refresh_token = value
        .get("refresh_token")
        .and_then(|item| item.as_str())
        .filter(|token| !token.is_empty())
        .map(str::to_string);
    let fields = value
        .get("fields")
        .and_then(|item| item.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    Ok(TokenGrant {
        access_token,
        expires_in,
        refresh_token,
        fields,
    })
}

fn friendly_init(raw: &str) -> String {
    let low = raw.to_lowercase();
    if low.contains("administrator") || low.contains("same user") || low.contains("privilege") {
        "Start the helper without administrator rights".into()
    } else if low.contains("not running")
        || low.contains("connect")
        || low.contains("steamclient")
        || low.contains("no steam")
    {
        "Open Steam and sign in".into()
    } else {
        "Open Steam and sign in".into()
    }
}

fn web_ticket(client: &steamworks::Client, identity: &str) -> Result<String> {
    let (tx, rx) = std::sync::mpsc::channel();
    let handle = client
        .user()
        .authentication_session_ticket_for_webapi(identity);
    let callback = client.register_callback(move |resp: TicketForWebApiResponse| {
        if resp.ticket_handle == handle {
            let _ = tx.send(resp);
        }
    });

    let deadline = Instant::now() + Duration::from_secs(20);
    let response = loop {
        client.run_callbacks();
        match rx.try_recv() {
            Ok(response) => break response,
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                if Instant::now() >= deadline {
                    drop(callback);
                    client.user().cancel_authentication_ticket(handle);
                    bail!("Steam did not answer in time");
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                bail!("Steam did not give a sign-in ticket");
            }
        }
    };
    drop(callback);
    client.user().cancel_authentication_ticket(handle);

    response
        .result
        .map_err(|_| anyhow!("Steam did not give a sign-in ticket"))?;

    let mut bytes = response.ticket;
    let len = if response.ticket_len > 0 {
        response.ticket_len as usize
    } else {
        bytes.len()
    };
    if len < bytes.len() {
        bytes.truncate(len);
    }
    if bytes.is_empty() {
        bail!("Steam returned an empty ticket");
    }
    Ok(hex::encode(bytes))
}

struct Reject {
    retry: bool,
    message: String,
}

fn refresh_grant(refresh_token: &str) -> Result<TokenGrant, Reject> {
    let body = signed_fields(&[
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("client_id", CLIENT_ID),
        ("client_secret", CLIENT_SECRET),
    ]);
    post_token(body)
}

fn exchange(persona: &str, ticket_hex: &str) -> Result<TokenGrant, Reject> {
    post_token(signed_form(persona, ticket_hex))
}

fn post_token(body: String) -> Result<TokenGrant, Reject> {
    let http = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|err| Reject {
            retry: false,
            message: format!("Embark sign-in failed: {err}"),
        })?;

    let mac = STANDARD.encode(hmac_sha256(&signing_key()?, body.as_bytes()));
    let response = http
        .post(TOKEN_URL)
        .header("User-Agent", USER_AGENT)
        .header("x-embark-telemetry-client-platform", "3")
        .header(
            "x-embark-telemetry-uuid",
            "f47ac10b-58cc-4372-a567-0e02b2c3d479",
        )
        .header("x-embark-hmac", mac)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .map_err(|err| Reject {
            retry: false,
            message: format!("Embark sign-in failed: {err}"),
        })?;

    let status = response.status();
    let body = response.text().map_err(|err| Reject {
        retry: false,
        message: format!("Embark sign-in failed: {err}"),
    })?;

    if !status.is_success() {
        // 400/401 can mean this Steam identity is not the one Embark expects.
        // invalid_client is the client credential, not the identity, so the
        // caller still stops after the fallbacks are exhausted.
        let client_rejected = body.contains("invalid_client");
        let kind = serde_json::from_str::<serde_json::Value>(&body)
            .ok()
            .and_then(|value| {
                value
                    .get("error")
                    .and_then(|item| item.as_str())
                    .map(str::to_string)
                    .or_else(|| {
                        value
                            .get("error_code")
                            .and_then(|item| item.as_u64())
                            .map(|code| format!("error_code {code}"))
                    })
            })
            .unwrap_or_else(|| "rejected".to_string());
        return Err(Reject {
            retry: !client_rejected && (status.as_u16() == 400 || status.as_u16() == 401),
            message: format!("Embark rejected the sign-in (HTTP {} {kind})", status.as_u16()),
        });
    }

    let value: serde_json::Value = serde_json::from_str(&body).map_err(|_| Reject {
        retry: false,
        message: "Embark answer was not JSON".into(),
    })?;
    let fields = value
        .as_object()
        .map(|object| object.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    let token = value
        .get("access_token")
        .and_then(|item| item.as_str())
        .or_else(|| value.get("token").and_then(|item| item.as_str()))
        .filter(|item| !item.is_empty())
        .ok_or_else(|| Reject {
            retry: false,
            message: format!(
                "Embark answer had no access token ({})",
                fields.join(",")
            ),
        })?;

    let expires_in = value
        .get("expires_in")
        .and_then(|item| {
            item.as_u64()
                .or_else(|| item.as_str().and_then(|text| text.parse().ok()))
        })
        .unwrap_or(20 * 60);
    let refresh_token = value
        .get("refresh_token")
        .and_then(|item| item.as_str())
        .filter(|token| !token.is_empty())
        .map(str::to_string);

    Ok(TokenGrant {
        access_token: token.to_string(),
        expires_in,
        refresh_token,
        fields,
    })
}

fn signed_form(persona: &str, ticket_hex: &str) -> String {
    signed_fields(&[
        ("grant_type", "client_credentials"),
        ("client_id", CLIENT_ID),
        ("client_secret", CLIENT_SECRET),
        ("audience", AUDIENCE),
        ("external_provider_name", "steam"),
        ("external_provider_token", ticket_hex),
        ("nick_name", persona),
        ("tenancy", TENANCY),
        ("app_id", APP_ID),
    ])
}

fn signed_fields(fields: &[(&str, &str)]) -> String {
    fields
        .iter()
        .map(|(key, value)| format!("{key}={}", form_escape(value)))
        .collect::<Vec<_>>()
        .join("&")
}

fn form_escape(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    const BLOCK: usize = 64;
    let mut key_block = [0u8; BLOCK];
    if key.len() > BLOCK {
        key_block[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        key_block[..key.len()].copy_from_slice(key);
    }
    let mut inner_pad = [0u8; BLOCK];
    let mut outer_pad = [0u8; BLOCK];
    for index in 0..BLOCK {
        inner_pad[index] = key_block[index] ^ 0x36;
        outer_pad[index] = key_block[index] ^ 0x5c;
    }
    let mut inner = Sha256::new();
    inner.update(inner_pad);
    inner.update(message);
    let inner_hash = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(outer_pad);
    outer.update(inner_hash);
    let digest = outer.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    out
}

/// Client request signature. Not a user password.
const SIGNING_KEY_B64: &str = "NKmGq9MAuwIfhwN2C+NYQsqnsCoVKtO1dUV4NGnyM20jx9n18MBWnbLWiRJ0v7lmKu5bTRnPSDr0rPeeBq0bbA==";

fn signing_key() -> Result<Vec<u8>, Reject> {
    STANDARD.decode(SIGNING_KEY_B64).map_err(|_| Reject {
        retry: false,
        message: "Signing key could not be decoded".into(),
    })
}

pub fn redact(body: &str) -> String {
    let mut cleaned = String::new();
    let mut run = String::new();
    let flush = |run: &mut String, cleaned: &mut String| {
        if run.chars().count() >= 24 {
            cleaned.push_str("[redacted]");
        } else {
            cleaned.push_str(run);
        }
        run.clear();
    };
    for ch in body.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '+' | '/' | '=' | '-' | '_') {
            run.push(ch);
        } else {
            flush(&mut run, &mut cleaned);
            cleaned.push(ch);
        }
    }
    flush(&mut run, &mut cleaned);
    if cleaned.chars().count() > 180 {
        cleaned = cleaned.chars().take(180).collect();
    }
    cleaned
}
