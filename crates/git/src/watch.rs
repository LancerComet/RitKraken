use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};

use crate::{
    Repo, Result,
    command::{checked, output_with_input, without_optional_locks},
    protocol::path_from_bytes,
};

const QUIET_PERIOD: Duration = Duration::from_millis(250);
const MAX_DELAY: Duration = Duration::from_secs(1);
const MAX_PATHS: usize = 4096;

/// Watches one checkout and its Git metadata independently of window activation.
pub struct RepoWatch {
    _watcher: RecommendedWatcher,
    pending: Arc<Mutex<Pending>>,
}

#[derive(Default)]
struct Pending {
    changes: RepoChanges,
    first: Option<Instant>,
    last: Option<Instant>,
}

#[derive(Default)]
pub struct RepoChanges {
    metadata: bool,
    restart: bool,
    paths: HashSet<PathBuf>,
    last: Option<Instant>,
}

struct WatchPaths {
    root: PathBuf,
    git: PathBuf,
    common: PathBuf,
}

impl RepoWatch {
    pub fn new(repo: &Repo) -> Result<Self> {
        let paths = WatchPaths::new(repo)?;
        let pending = Arc::new(Mutex::new(Pending::default()));
        let captured = pending.clone();
        let watched = Arc::new(paths);
        let paths = watched.clone();
        let mut watcher = notify::recommended_watcher(move |event: notify::Result<Event>| {
            let changes = match event {
                Ok(event) => paths.changes(event),
                Err(_) => RepoChanges {
                    metadata: true,
                    restart: true,
                    ..Default::default()
                },
            };
            if changes.is_empty() {
                return;
            }
            if let Ok(mut pending) = captured.lock() {
                pending.push(changes, Instant::now());
            }
        })
        .map_err(|error| error.to_string())?;
        watcher
            .watch(&watched.root, RecursiveMode::Recursive)
            .map_err(|error| error.to_string())?;
        if !watched.git.starts_with(&watched.root) {
            watcher
                .watch(&watched.git, RecursiveMode::Recursive)
                .map_err(|error| error.to_string())?;
        }
        if !watched.common.starts_with(&watched.root) && !watched.common.starts_with(&watched.git) {
            watcher
                .watch(&watched.common, RecursiveMode::Recursive)
                .map_err(|error| error.to_string())?;
        }
        Ok(Self {
            _watcher: watcher,
            pending,
        })
    }

    /// Returns a coalesced batch after a quiet period, or at most one second
    /// into a continuous stream. Taking a batch never runs Git or blocks on I/O.
    pub fn take_changes(&self) -> Option<RepoChanges> {
        self.pending.lock().ok()?.take(Instant::now())
    }
}

impl RepoChanges {
    pub fn needs_restart(&self) -> bool {
        self.restart
    }

    /// When the newest change in this batch arrived. A batch fully covered by
    /// a refresh that started later can be skipped entirely.
    pub fn last_event_at(&self) -> Option<Instant> {
        self.last
    }

    fn is_empty(&self) -> bool {
        !self.metadata && self.paths.is_empty()
    }

    /// Uses Git's own ignore rules, which still report tracked files inside
    /// ignored directories. Run this on a background worker.
    pub fn needs_refresh(self, repo: &Repo) -> Result<bool> {
        if self.metadata {
            return Ok(true);
        }
        if self.paths.is_empty() {
            return Ok(false);
        }
        let mut input = Vec::new();
        for path in &self.paths {
            input.extend_from_slice(path.as_os_str().as_encoded_bytes());
            input.push(0);
        }
        let mut command = repo.git();
        command.args(["check-ignore", "-z", "--stdin"]);
        let output = output_with_input(without_optional_locks(&mut command), &[&input])?;
        if !matches!(output.status.code(), Some(0 | 1)) {
            checked(output)?;
            return Ok(true);
        }
        let ignored: HashSet<_> = output
            .stdout
            .split(|byte| *byte == 0)
            .filter(|path| !path.is_empty())
            .map(path_from_bytes)
            .collect();
        Ok(self.paths.iter().any(|path| !ignored.contains(path)))
    }
}

impl WatchPaths {
    fn new(repo: &Repo) -> Result<Self> {
        Ok(Self {
            root: repo.root().canonicalize()?,
            git: repo.git_dir().canonicalize()?,
            common: repo.common_dir().canonicalize()?,
        })
    }

    fn changes(&self, event: Event) -> RepoChanges {
        if event.need_rescan() {
            return RepoChanges {
                metadata: true,
                restart: true,
                ..Default::default()
            };
        }
        if event.kind.is_access() {
            return RepoChanges::default();
        }
        if event.paths.is_empty() {
            return RepoChanges {
                metadata: true,
                ..Default::default()
            };
        }
        let mut changes = RepoChanges::default();
        for path in event.paths {
            if event.kind.is_remove()
                && (path == self.root || path == self.git || path == self.common)
            {
                changes.metadata = true;
                changes.restart = true;
            }
            let metadata = path
                .strip_prefix(&self.git)
                .ok()
                .or_else(|| path.strip_prefix(&self.common).ok());
            if let Some(path) = metadata {
                changes.metadata |= relevant_metadata(path);
            } else if let Ok(path) = path.strip_prefix(&self.root) {
                if path == Path::new(".git")
                    || path.ends_with(".gitignore")
                    || path.as_os_str().is_empty()
                {
                    changes.metadata = true;
                } else {
                    changes.paths.insert(path.to_owned());
                }
            }
        }
        changes
    }
}

fn relevant_metadata(path: &Path) -> bool {
    if path
        .extension()
        .is_some_and(|extension| extension == "lock")
    {
        return false;
    }
    let Some(first) = path.components().next() else {
        return true;
    };
    let first = first.as_os_str();
    matches!(
        first.to_str(),
        Some(
            "HEAD"
                | "index"
                | "refs"
                | "packed-refs"
                | "config"
                | "config.worktree"
                | "shallow"
                | "commondir"
                | "gitdir"
                | "MERGE_HEAD"
                | "REBASE_HEAD"
                | "CHERRY_PICK_HEAD"
                | "REVERT_HEAD"
                | "rebase-merge"
                | "rebase-apply"
                | "sequencer"
        )
    ) || path == Path::new("info").join("exclude")
}

impl Pending {
    fn push(&mut self, changes: RepoChanges, now: Instant) {
        self.first.get_or_insert(now);
        self.last = Some(now);
        self.changes.metadata |= changes.metadata;
        self.changes.restart |= changes.restart;
        if self.changes.metadata {
            self.changes.paths.clear();
        } else {
            self.changes.paths.extend(changes.paths);
            if self.changes.paths.len() > MAX_PATHS {
                self.changes.metadata = true;
                self.changes.paths.clear();
            }
        }
    }

    fn take(&mut self, now: Instant) -> Option<RepoChanges> {
        let (first, last) = (self.first?, self.last?);
        if now.duration_since(last) < QUIET_PERIOD && now.duration_since(first) < MAX_DELAY {
            return None;
        }
        self.first = None;
        self.last = None;
        let mut changes = std::mem::take(&mut self.changes);
        changes.last = Some(last);
        Some(changes)
    }
}

#[cfg(test)]
mod tests;
