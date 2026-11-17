//! Per-user application directories, following each platform's convention:
//! Windows `%LOCALAPPDATA%\RitKraken`, macOS
//! `~/Library/Application Support/RitKraken`, and on other systems the XDG
//! config (preferences) and state (repositories, layout, logs) directories.

use std::path::PathBuf;
#[cfg(any(test, not(any(windows, target_os = "macos"))))]
use std::{ffi::OsString, path::Path};

#[derive(Clone, Copy, Debug)]
enum Kind {
    /// Settings the user chose.
    Config,
    /// Data the application keeps between runs: history, layout, logs.
    State,
}

pub(super) fn config_dir() -> Option<PathBuf> {
    directory(Kind::Config)
}

pub(super) fn state_dir() -> Option<PathBuf> {
    directory(Kind::State)
}

pub(super) fn logs_dir() -> Option<PathBuf> {
    state_dir().map(|directory| directory.join("logs"))
}

fn directory(kind: Kind) -> Option<PathBuf> {
    // Tests and isolated runs keep every file under one directory.
    if let Some(path) = std::env::var_os("RITKRAKEN_STATE_DIR") {
        return Some(PathBuf::from(path));
    }
    platform(kind)
}

#[cfg(windows)]
fn platform(_: Kind) -> Option<PathBuf> {
    Some(local_app_data()?.join("RitKraken"))
}

#[cfg(target_os = "macos")]
fn platform(_: Kind) -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var_os("HOME")?).join("Library/Application Support/RitKraken"))
}

#[cfg(not(any(windows, target_os = "macos")))]
fn platform(kind: Kind) -> Option<PathBuf> {
    xdg(kind, |name| std::env::var_os(name))
}

#[cfg(any(test, not(any(windows, target_os = "macos"))))]
fn xdg(kind: Kind, var: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    let (variable, fallback) = match kind {
        Kind::Config => ("XDG_CONFIG_HOME", ".config"),
        Kind::State => ("XDG_STATE_HOME", ".local/state"),
    };
    // The XDG specification ignores relative paths in these variables.
    let base = var(variable)
        .map(PathBuf::from)
        .filter(|path| path.has_root())
        .or_else(|| var("HOME").map(|home| Path::new(&home).join(fallback)))?;
    Some(base.join("ritkraken"))
}

/// Prefers the Known Folder API over `%LOCALAPPDATA%`, which a process
/// environment can lack or override.
#[cfg(windows)]
fn local_app_data() -> Option<PathBuf> {
    known_local_app_data().or_else(|| std::env::var_os("LOCALAPPDATA").map(PathBuf::from))
}

#[cfg(windows)]
fn known_local_app_data() -> Option<PathBuf> {
    use windows::Win32::{
        System::Com::CoTaskMemFree,
        UI::Shell::{FOLDERID_LocalAppData, KF_FLAG_DEFAULT, SHGetKnownFolderPath},
    };

    let path =
        unsafe { SHGetKnownFolderPath(&FOLDERID_LocalAppData, KF_FLAG_DEFAULT, None) }.ok()?;
    // The shell allocates the returned string; release it after copying.
    let copied = unsafe { path.to_string() }.ok().map(PathBuf::from);
    unsafe { CoTaskMemFree(Some(path.0.cast_const().cast())) };
    copied
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, ffi::OsString, path::PathBuf};

    use super::{Kind, xdg};

    fn resolve(kind: Kind, values: &[(&str, &str)]) -> Option<PathBuf> {
        let values: HashMap<_, _> = values.iter().copied().collect();
        xdg(kind, |name| values.get(name).map(OsString::from))
    }

    #[test]
    fn xdg_keeps_preferences_in_config_and_runtime_data_in_state() {
        let home = [("HOME", "/home/user")];
        assert_eq!(
            resolve(Kind::Config, &home),
            Some(PathBuf::from("/home/user/.config/ritkraken"))
        );
        assert_eq!(
            resolve(Kind::State, &home),
            Some(PathBuf::from("/home/user/.local/state/ritkraken"))
        );
        let custom = [
            ("HOME", "/home/user"),
            ("XDG_CONFIG_HOME", "/cfg"),
            ("XDG_STATE_HOME", "/st"),
        ];
        assert_eq!(
            resolve(Kind::Config, &custom),
            Some(PathBuf::from("/cfg/ritkraken"))
        );
        assert_eq!(
            resolve(Kind::State, &custom),
            Some(PathBuf::from("/st/ritkraken"))
        );
    }

    #[test]
    fn relative_xdg_paths_are_ignored_as_the_specification_requires() {
        let values = [("HOME", "/home/user"), ("XDG_STATE_HOME", "relative/state")];
        assert_eq!(
            resolve(Kind::State, &values),
            Some(PathBuf::from("/home/user/.local/state/ritkraken"))
        );
        assert_eq!(resolve(Kind::State, &[]), None);
    }
}
