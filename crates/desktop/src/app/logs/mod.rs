//! Application log: errors, warnings and every Git call. Recent entries stay
//! in memory for the log viewer; all entries go to rolling files in the logs
//! directory.

mod file;
mod view;

use std::{
    collections::VecDeque,
    ops::RangeInclusive,
    path::PathBuf,
    sync::{
        Mutex, MutexGuard,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use chrono::{DateTime, Local};
use ritkraken_git::GitCall;

use super::paths::logs_dir;
use file::RollingFile;

pub(super) const DEFAULT_FILE_SIZE_MB: u64 = 2;
pub(super) const FILE_SIZE_RANGE: RangeInclusive<u64> = 1..=1024;
/// Entries shown by the viewer. Git diagnostics are capped at 4000 characters,
/// so this keeps the in-memory log to a few megabytes.
const MEMORY_LIMIT: usize = 2000;
const MEGABYTE: u64 = 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Level {
    Error,
    Warning,
    Git,
}

impl Level {
    fn tag(self) -> &'static str {
        match self {
            Self::Error => "ERROR",
            Self::Warning => "WARN ",
            Self::Git => "GIT  ",
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct GitInfo {
    pub duration: Duration,
    pub exit_code: Option<i32>,
    pub failed: bool,
}

#[derive(Clone, Debug)]
pub(super) struct Entry {
    pub time: DateTime<Local>,
    pub level: Level,
    /// Repository name, when the entry belongs to one.
    pub source: Option<String>,
    pub summary: String,
    pub detail: Option<String>,
    pub git: Option<GitInfo>,
}

impl Entry {
    /// Shows that something went wrong, including Git commands that failed.
    pub fn is_problem(&self) -> bool {
        self.level != Level::Git || self.git.as_ref().is_some_and(|git| git.failed)
    }

    pub fn text(&self) -> String {
        let mut line = format!(
            "{} [{}]",
            self.time.format("%Y-%m-%d %H:%M:%S%.3f"),
            self.level.tag()
        );
        if let Some(source) = &self.source {
            line.push_str(&format!(" {source} ·"));
        }
        if let Some(git) = &self.git {
            line.push_str(&format!(" {} ms ·", git.duration.as_millis()));
            match git.exit_code {
                Some(code) => line.push_str(&format!(" exit {code} ·")),
                None => line.push_str(" not finished ·"),
            }
        }
        line.push(' ');
        line.push_str(&self.summary);
        line.push('\n');
        for detail in self.detail.iter().flat_map(|detail| detail.lines()) {
            if !detail.trim().is_empty() {
                line.push_str("    ");
                line.push_str(detail.trim_end());
            }
            line.push('\n');
        }
        line
    }
}

struct Store {
    entries: VecDeque<Entry>,
    file: Option<RollingFile>,
}

static STORE: Mutex<Store> = Mutex::new(Store {
    entries: VecDeque::new(),
    file: None,
});
static GENERATION: AtomicU64 = AtomicU64::new(0);

fn store() -> MutexGuard<'static, Store> {
    STORE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Starts writing log files and recording Git calls.
pub(super) fn init(file_size_mb: u64) {
    if let Some(directory) = logs_dir() {
        store().file = Some(RollingFile::new(directory, file_size_mb * MEGABYTE));
    }
    ritkraken_git::set_git_call_listener(record_git_call);
}

pub(super) fn set_file_size(file_size_mb: u64) {
    if let Some(file) = &mut store().file {
        file.set_limit(file_size_mb * MEGABYTE);
    }
}

pub(super) fn log_file() -> Option<PathBuf> {
    store().file.as_ref().map(RollingFile::path)
}

/// Changes whenever an entry is added, so the viewer knows to refresh.
pub(super) fn generation() -> u64 {
    GENERATION.load(Ordering::Acquire)
}

/// Newest first.
pub(super) fn entries() -> Vec<Entry> {
    store().entries.iter().rev().cloned().collect()
}

pub(super) fn record(level: Level, source: Option<String>, message: &str) {
    let mut lines = message.trim().lines();
    let summary = lines.next().unwrap_or_default().to_owned();
    let rest = lines.collect::<Vec<_>>().join("\n");
    push(Entry {
        time: Local::now(),
        level,
        source,
        summary,
        detail: (!rest.trim().is_empty()).then_some(rest),
        git: None,
    });
}

fn record_git_call(call: &GitCall) {
    let source = call
        .directory
        .as_ref()
        .and_then(|directory| directory.file_name())
        .map(|name| name.to_string_lossy().into_owned());
    // Keep each call on one line; commit messages may contain line breaks.
    let command = call.args.join(" ").replace(['\r', '\n'], " ");
    push(Entry {
        time: Local::now(),
        level: Level::Git,
        source,
        summary: format!("git {command}"),
        detail: call.detail.clone(),
        git: Some(GitInfo {
            duration: call.duration,
            exit_code: call.exit_code,
            failed: call.failed(),
        }),
    });
}

fn push(entry: Entry) {
    let mut store = store();
    if let Some(file) = &mut store.file {
        file.append(&entry.text());
    }
    if store.entries.len() == MEMORY_LIMIT {
        store.entries.pop_front();
    }
    store.entries.push_back(entry);
    GENERATION.fetch_add(1, Ordering::Release);
}
