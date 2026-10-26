use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;

/// 30 seconds minimum (manual testing / impatient users).
const MIN_POLL_SECS: u64 = 30;
/// 30 minutes max.
const MAX_POLL_SECS: u64 = 30 * 60;
/// Default: 3 minutes.
const DEFAULT_POLL_SECS: u64 = 3 * 60;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HelperConfig {
    /// How often to re-query Embark inventory (seconds).
    pub poll_interval_secs: u64,
    /// Auto-start Steam prep when the app opens.
    pub auto_prepare_steam: bool,
    /// When false, only Sync now talks to the game. The interval does nothing.
    pub auto_sync: bool,
    /// Current-user Run key. Off removes the value.
    pub run_at_startup: bool,
    /// en, ru, de, or fr. Empty or unknown means English.
    pub language: String,
    /// Goal tiles in the corner, 20–100.
    pub overlay_opacity: u8,
    /// When true, the corner list is text. When false, it is icon squares.
    pub overlay_list: bool,
    /// Square icons run down, then across. Otherwise they run across, then down.
    pub overlay_vertical: bool,
    /// Icon size, 50–160 percent.
    pub overlay_scale: u8,
    /// Top-left of the overlay, in screen pixels.
    pub overlay_x: i32,
    pub overlay_y: i32,
    /// Virtual-key code that toggles the overlay. F8 is 0x77.
    pub overlay_vk: u32,
    /// tl, tr, bl or br keeps the overlay in that screen corner. Empty after a drag.
    /// A config from 0.2.x has no corner: its saved x / y stay in charge.
    #[serde(default)]
    pub overlay_corner: String,
    /// Collected goals leave the overlay.
    pub overlay_hide_done: bool,
    /// Overlay on or off (button, tray switch or the hotkey).
    pub overlay_shown: bool,
    /// Set by «Sign out». The app waits for «Sign in» before it asks Steam again.
    pub signed_out: bool,
}

impl Default for HelperConfig {
    fn default() -> Self {
        Self {
            poll_interval_secs: DEFAULT_POLL_SECS,
            auto_prepare_steam: false,
            auto_sync: true,
            run_at_startup: false,
            language: String::new(),
            overlay_opacity: 90,
            overlay_list: false,
            overlay_vertical: false,
            overlay_scale: 100,
            overlay_x: 16,
            overlay_y: 16,
            overlay_vk: crate::hotkey::VK_F8,
            overlay_corner: "tl".into(),
            overlay_hide_done: true,
            overlay_shown: false,
            signed_out: false,
        }
    }
}

impl HelperConfig {
    pub fn clamp(mut self) -> Self {
        self.poll_interval_secs = self.poll_interval_secs.clamp(MIN_POLL_SECS, MAX_POLL_SECS);
        self.overlay_opacity = self.overlay_opacity.clamp(20, 100);
        self.overlay_scale = self.overlay_scale.clamp(50, 160);
        if !(0x08..=0xFE).contains(&self.overlay_vk) {
            self.overlay_vk = crate::hotkey::VK_F8;
        }
        self.overlay_x = self.overlay_x.clamp(-4000, 8000);
        self.overlay_y = self.overlay_y.clamp(-4000, 8000);
        if !matches!(self.overlay_corner.as_str(), "" | "tl" | "tr" | "bl" | "br") {
            self.overlay_corner = "tl".into();
        }
        self
    }

    pub fn min_poll() -> u64 {
        MIN_POLL_SECS
    }

    pub fn max_poll() -> u64 {
        MAX_POLL_SECS
    }

    pub fn default_poll() -> u64 {
        DEFAULT_POLL_SECS
    }

    pub fn config_path() -> PathBuf {
        if let Some(base) = directories::ProjectDirs::from("io", "ArcCalc", "Helper") {
            base.config_dir().join("config.json")
        } else {
            PathBuf::from("arccalc-helper-config.json")
        }
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        let mut cfg = match fs::read_to_string(&path) {
            Ok(raw) => serde_json::from_str::<HelperConfig>(&raw).unwrap_or_default(),
            Err(_) => HelperConfig::default(),
        };
        if cfg.language.is_empty() {
            cfg.language = crate::win::system_language().to_string();
        }
        // Old builds defaulted to a few seconds — migrate to 3 minutes.
        if cfg.poll_interval_secs < 60 {
            cfg.poll_interval_secs = DEFAULT_POLL_SECS;
        }
        cfg.language = normalize_language(&cfg.language);
        cfg.clamp()
    }

    pub fn save(&self) {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(raw) = serde_json::to_string_pretty(self) {
            let _ = fs::write(path, raw);
        }
    }
}

pub fn normalize_language(code: &str) -> String {
    match code {
        "en" | "ru" | "de" | "fr" => code.to_string(),
        _ => "en".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::normalize_language;

    #[test]
    fn unknown_language_is_english() {
        assert_eq!(normalize_language(""), "en");
        assert_eq!(normalize_language("ru"), "ru");
        assert_eq!(normalize_language("es"), "en");
    }
}

/// Writes or removes HKCU\...\Run\StashSync. No administrator rights.
pub fn apply_run_at_startup(enabled: bool) {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let Ok((key, _)) = hkcu.create_subkey(r"Software\Microsoft\Windows\CurrentVersion\Run") else {
        return;
    };
    if !enabled {
        let _ = key.delete_value("StashSync");
        return;
    }
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let path = exe.display().to_string();
    let value = if path.contains(' ') {
        format!("\"{path}\"")
    } else {
        path
    };
    let _ = key.set_value("StashSync", &value);
}
