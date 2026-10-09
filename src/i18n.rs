//! Visible copy for the four languages the calculator offers.

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    En,
    Ru,
    De,
    Fr,
}

pub enum Key {
    Settings,
    Inventory,
    SyncNow,
    Syncing,
    Interval,
    Save,
    Min,
    SignedIn,
    AskingSteam,
    OpenSteam,
    Error,
    Synced,
    SyncFailed,
    BuildNotReady,
    Waiting,
    AutoSync,
    Startup,
    Language,
}

impl Lang {
    pub fn parse(code: &str) -> Self {
        match code {
            "ru" => Lang::Ru,
            "de" => Lang::De,
            "fr" => Lang::Fr,
            _ => Lang::En,
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Ru => "ru",
            Lang::De => "de",
            Lang::Fr => "fr",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::Ru => "Русский",
            Lang::De => "Deutsch",
            Lang::Fr => "Français",
        }
    }

    pub fn all() -> [Lang; 4] {
        [Lang::En, Lang::Ru, Lang::De, Lang::Fr]
    }

    pub fn text(self, key: Key) -> &'static str {
        match (self, key) {
            (Lang::En, Key::Settings) => "SETTINGS",
            (Lang::Ru, Key::Settings) => "НАСТРОЙКИ",
            (Lang::De, Key::Settings) => "EINSTELLUNGEN",
            (Lang::Fr, Key::Settings) => "RÉGLAGES",

            (Lang::En, Key::Inventory) => "INVENTORY",
            (Lang::Ru, Key::Inventory) => "ИНВЕНТАРЬ",
            (Lang::De, Key::Inventory) => "INVENTAR",
            (Lang::Fr, Key::Inventory) => "INVENTAIRE",

            (Lang::En, Key::SyncNow) => "Sync now",
            (Lang::Ru, Key::SyncNow) => "Синхронизировать",
            (Lang::De, Key::SyncNow) => "Synchronisieren",
            (Lang::Fr, Key::SyncNow) => "Synchroniser",

            (Lang::En, Key::Syncing) => "Syncing…",
            (Lang::Ru, Key::Syncing) => "Синхронизация…",
            (Lang::De, Key::Syncing) => "Synchronisiere…",
            (Lang::Fr, Key::Syncing) => "Synchronisation…",

            (Lang::En, Key::Interval) => "UPDATE INTERVAL",
            (Lang::Ru, Key::Interval) => "ИНТЕРВАЛ",
            (Lang::De, Key::Interval) => "INTERVALL",
            (Lang::Fr, Key::Interval) => "INTERVALLE",

            (Lang::En, Key::Save) => "Save",
            (Lang::Ru, Key::Save) => "Сохранить",
            (Lang::De, Key::Save) => "Speichern",
            (Lang::Fr, Key::Save) => "Enregistrer",

            (Lang::En, Key::Min) => "min",
            (Lang::Ru, Key::Min) => "мин",
            (Lang::De, Key::Min) => "Min.",
            (Lang::Fr, Key::Min) => "min",

            (Lang::En, Key::SignedIn) => "Signed in",
            (Lang::Ru, Key::SignedIn) => "Вход выполнен",
            (Lang::De, Key::SignedIn) => "Angemeldet",
            (Lang::Fr, Key::SignedIn) => "Connecté",

            (Lang::En, Key::AskingSteam) => "Asking Steam…",
            (Lang::Ru, Key::AskingSteam) => "Запрос к Steam…",
            (Lang::De, Key::AskingSteam) => "Frage Steam…",
            (Lang::Fr, Key::AskingSteam) => "Demande à Steam…",

            (Lang::En, Key::OpenSteam) => "Open Steam",
            (Lang::Ru, Key::OpenSteam) => "Откройте Steam",
            (Lang::De, Key::OpenSteam) => "Steam öffnen",
            (Lang::Fr, Key::OpenSteam) => "Ouvrez Steam",

            (Lang::En, Key::Error) => "Error",
            (Lang::Ru, Key::Error) => "Ошибка",
            (Lang::De, Key::Error) => "Fehler",
            (Lang::Fr, Key::Error) => "Erreur",

            (Lang::En, Key::Synced) => "Synced",
            (Lang::Ru, Key::Synced) => "Синхронизировано",
            (Lang::De, Key::Synced) => "Synchronisiert",
            (Lang::Fr, Key::Synced) => "Synchronisé",

            (Lang::En, Key::SyncFailed) => "Sync failed",
            (Lang::Ru, Key::SyncFailed) => "Сбой синхронизации",
            (Lang::De, Key::SyncFailed) => "Sync fehlgeschlagen",
            (Lang::Fr, Key::SyncFailed) => "Échec de la synchro",

            (Lang::En, Key::BuildNotReady) => "Game update, retrying",
            (Lang::Ru, Key::BuildNotReady) => "Обновление игры, повтор",
            (Lang::De, Key::BuildNotReady) => "Spielupdate, neuer Versuch",
            (Lang::Fr, Key::BuildNotReady) => "Màj du jeu, nouvel essai",

            (Lang::En, Key::Waiting) => "Waiting for first sync…",
            (Lang::Ru, Key::Waiting) => "Ждём первую синхронизацию…",
            (Lang::De, Key::Waiting) => "Warte auf den ersten Sync…",
            (Lang::Fr, Key::Waiting) => "En attente du premier sync…",

            (Lang::En, Key::AutoSync) => "Auto sync",
            (Lang::Ru, Key::AutoSync) => "Автосинхронизация",
            (Lang::De, Key::AutoSync) => "Automatisch synchronisieren",
            (Lang::Fr, Key::AutoSync) => "Synchronisation automatique",

            (Lang::En, Key::Startup) => "Start with Windows",
            (Lang::Ru, Key::Startup) => "Запускать при загрузке",
            (Lang::De, Key::Startup) => "Mit Windows starten",
            (Lang::Fr, Key::Startup) => "Lancer au démarrage",

            (Lang::En, Key::Language) => "Language",
            (Lang::Ru, Key::Language) => "Язык",
            (Lang::De, Key::Language) => "Sprache",
            (Lang::Fr, Key::Language) => "Langue",
        }
    }
}

/// Marker stored on the inventory row while Embark has not published the new build yet.
pub const BUILD_NOT_READY: &str = "build-not-ready";

pub fn synced_line(lang: Lang, count: u64, when: &str) -> String {
    match lang {
        Lang::En => format!("Synced · {count} items · {when}"),
        Lang::Ru => format!("Синхронизировано · {count} шт. · {when}"),
        Lang::De => format!("Synchronisiert · {count} · {when}"),
        Lang::Fr => format!("Synchronisé · {count} · {when}"),
    }
}
