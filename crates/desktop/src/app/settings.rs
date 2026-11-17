use std::{
    fs,
    path::{Path, PathBuf},
};

use super::preferences::Preferences;

#[derive(Default)]
pub(super) struct SavedRepos {
    pub(super) recent: Vec<PathBuf>,
    pub(super) favorites: Vec<PathBuf>,
    pub(super) projects: Vec<PathBuf>,
    pub(super) open_tabs: Vec<PathBuf>,
    pub(super) active_tab: Option<PathBuf>,
    pub(super) preferences: Preferences,
}

impl SavedRepos {
    pub(super) fn load() -> Self {
        let Some(value) = read_state(SETTINGS_FILE) else {
            return Self::default();
        };
        let paths = |key: &str| serde_json::from_value(value[key].clone()).unwrap_or_default();
        Self {
            recent: paths("recent"),
            favorites: paths("favorites"),
            projects: paths("projects"),
            open_tabs: paths("open_tabs"),
            active_tab: serde_json::from_value(value["active_tab"].clone()).unwrap_or_default(),
            preferences: Preferences::read(&value["preferences"]),
        }
    }

    pub(super) fn save(&self) {
        write_state(
            SETTINGS_FILE,
            &serde_json::json!({
                "recent": self.recent,
                "favorites": self.favorites,
                "projects": self.projects,
                "open_tabs": self.open_tabs,
                "active_tab": self.active_tab,
                "preferences": self.preferences.json(),
            }),
        );
    }

    pub(super) fn remember(&mut self, root: &Path) {
        self.recent.retain(|path| path != root);
        self.recent.insert(0, root.to_owned());
        self.recent.truncate(25);
    }
}

const SETTINGS_FILE: &str = "repositories.json";

/// Reads a JSON file from the application state directory.
pub(super) fn read_state(file: &str) -> Option<serde_json::Value> {
    let bytes = fs::read(state_path(file)?).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Atomically replaces a JSON file in the application state directory.
pub(super) fn write_state(file: &str, json: &serde_json::Value) {
    let Some(path) = state_path(file) else {
        return;
    };
    if let Some(parent) = path.parent()
        && fs::create_dir_all(parent).is_ok()
    {
        let temporary = path.with_extension(format!("{}.new", std::process::id()));
        if fs::write(&temporary, json.to_string()).is_ok() {
            let _ = fs::rename(temporary, path);
        }
    }
}

fn state_path(file: &str) -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("RITKRAKEN_STATE_DIR") {
        return Some(PathBuf::from(path).join(file));
    }
    #[cfg(windows)]
    let directory = PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join("RitKraken");
    #[cfg(target_os = "macos")]
    let directory =
        PathBuf::from(std::env::var_os("HOME")?).join("Library/Application Support/RitKraken");
    #[cfg(not(any(windows, target_os = "macos")))]
    let directory = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state")))?
        .join("ritkraken");
    Some(directory.join(file))
}
