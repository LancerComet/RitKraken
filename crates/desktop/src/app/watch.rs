use std::time::{Duration, Instant};

use gpui_kit::*;
use ritkraken_git::RepoWatch;

use super::Client;

const CHECK_INTERVAL: Duration = Duration::from_millis(200);
const FALLBACK_INTERVAL: Duration = Duration::from_secs(2);
const RETRY_INTERVAL: Duration = Duration::from_secs(10);

pub(super) struct ActiveWatch {
    id: u64,
    _task: Task<()>,
}

impl Client {
    pub(super) fn watch_current_repo(&mut self, window: &Window, cx: &mut Context<Self>) {
        let current = self.current().map(|tab| (tab.id, tab.repo.clone()));
        if self.repo_watch.as_ref().map(|watch| watch.id) == current.as_ref().map(|(id, _)| *id) {
            return;
        }
        // Dropping the old task also drops its native watcher. Saved, unopened
        // tabs never start watchers or Git reads.
        self.repo_watch = None;
        let Some((id, repo)) = current else { return };
        let executor = cx.background_executor().clone();
        let task = cx.spawn_in(window, async move |view, cx| {
            let initial = repo.clone();
            let mut watcher = executor
                .spawn(async move { RepoWatch::new(&initial) })
                .await
                .ok();
            let mut last_attempt = Instant::now();
            let mut last_refresh = last_attempt;
            // Covers edits between loading the snapshot and installing watches,
            // and changes made while this tab was inactive.
            let mut dirty_since = Some(last_attempt);
            loop {
                executor.timer(CHECK_INTERVAL).await;
                let Ok((live, covered_at)) = view.update_in(cx, |this, _, _| {
                    (
                        this.session.active == Some(id) && this.tab(id).is_some(),
                        this.tab(id).and_then(|tab| tab.watch_covered_at),
                    )
                }) else {
                    return;
                };
                if !live {
                    return;
                }
                let now = Instant::now();
                if watcher.is_none() && now.duration_since(last_attempt) >= RETRY_INTERVAL {
                    let repo = repo.clone();
                    watcher = executor
                        .spawn(async move { RepoWatch::new(&repo) })
                        .await
                        .ok();
                    last_attempt = Instant::now();
                    dirty_since = Some(now);
                }
                let changes = watcher.as_ref().and_then(RepoWatch::take_changes);
                if let Some(changes) = changes {
                    let restart = changes.needs_restart();
                    if restart {
                        watcher = None;
                        last_attempt = now;
                    }
                    // A completed update already read the state these events
                    // describe; a lost event stream still forces a refresh.
                    let covered = !restart
                        && covered_at.is_some_and(|at| {
                            changes.last_event_at().is_some_and(|last| last <= at)
                        });
                    if !covered {
                        let changed_at = changes.last_event_at().unwrap_or(now);
                        let repo = repo.clone();
                        let needed = executor
                            .spawn(async move { changes.needs_refresh(&repo) })
                            .await
                            .unwrap_or(true);
                        if needed {
                            dirty_since =
                                Some(dirty_since.map_or(changed_at, |since| since.max(changed_at)));
                        }
                    }
                }
                // Filesystems that cannot install native watches still update;
                // normal watchers cause no periodic Git queries while idle.
                dirty_since = dirty_since.or_else(|| {
                    (watcher.is_none() && now.duration_since(last_refresh) >= FALLBACK_INTERVAL)
                        .then_some(now)
                });
                if let Some(since) = dirty_since {
                    if covered_at.is_some_and(|at| since <= at) {
                        dirty_since = None;
                    } else {
                        let Ok(refreshed) = view.update_in(cx, |this, window, cx| {
                            if this.session.active != Some(id)
                                || !this.tab(id).is_some_and(|tab| !tab.busy)
                            {
                                return false;
                            }
                            this.refresh_watched_repo(id, window, cx)
                        }) else {
                            return;
                        };
                        if refreshed {
                            dirty_since = None;
                            last_refresh = Instant::now();
                        }
                    }
                }
            }
        });
        self.repo_watch = Some(ActiveWatch { id, _task: task });
    }
}
