use std::{
    fs,
    path::{Path, PathBuf},
};

use super::{
    paths::{config_dir, state_dir},
    preferences::Preferences,
};

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
        let value = read_state(REPOSITORIES_FILE).unwrap_or_default();
        let paths = |key: &str| serde_json::from_value(value[key].clone()).unwrap_or_default();
        Self {
            recent: paths("recent"),
            favorites: paths("favorites"),
            projects: paths("projects"),
            open_tabs: paths("open_tabs"),
            active_tab: serde_json::from_value(value["active_tab"].clone()).unwrap_or_default(),
            preferences: Preferences::read(
                &read_json(config_dir(), PREFERENCES_FILE).unwrap_or_default(),
            ),
        }
    }

    pub(super) fn save(&self) {
        write_json(config_dir(), PREFERENCES_FILE, &self.preferences.json());
        write_state(
            REPOSITORIES_FILE,
            &serde_json::json!({
                "recent": self.recent,
                "favorites": self.favorites,
                "projects": self.projects,
                "open_tabs": self.open_tabs,
                "active_tab": self.active_tab,
            }),
        );
    }

    pub(super) fn remember(&mut self, root: &Path) {
        self.recent.retain(|path| path != root);
        self.recent.insert(0, root.to_owned());
        self.recent.truncate(25);
    }
}

/// Preferences, in the configuration directory.
const PREFERENCES_FILE: &str = "settings.json";
/// Recent, favorite and open repositories, in the state directory.
const REPOSITORIES_FILE: &str = "repositories.json";

/// Reads a JSON file from the application state directory.
pub(super) fn read_state(file: &str) -> Option<serde_json::Value> {
    read_json(state_dir(), file)
}

/// Atomically replaces a JSON file in the application state directory.
pub(super) fn write_state(file: &str, json: &serde_json::Value) {
    write_json(state_dir(), file, json);
}

fn read_json(directory: Option<PathBuf>, file: &str) -> Option<serde_json::Value> {
    let bytes = fs::read(directory?.join(file)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn write_json(directory: Option<PathBuf>, file: &str, json: &serde_json::Value) {
    let Some(directory) = directory else {
        return;
    };
    let path = directory.join(file);
    if fs::create_dir_all(&directory).is_ok() {
        let temporary = path.with_extension(format!("{}.new", std::process::id()));
        if fs::write(&temporary, json.to_string()).is_ok() {
            let _ = fs::rename(temporary, path);
        }
    }
}
