mod view;

use gpui_kit::component::diff::{DiffFile, DiffLinePosition, DiffMode, DiffSide, DiffState};

use std::sync::Arc;

use gpui_kit::*;
use ritkraken_git::{self as git, FileTarget, PATCH_VIEW_LIMIT, Patch, Repo};

use super::{Client, Load, preferences::Preferences};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum DiffLayout {
    Hunk,
    Inline,
    #[default]
    Split,
}

impl DiffLayout {
    pub(super) const ALL: [Self; 3] = [Self::Hunk, Self::Inline, Self::Split];

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Hunk => "Hunk",
            Self::Inline => "Inline",
            Self::Split => "Split",
        }
    }

    fn apply(self, state: &mut DiffState, cx: &mut Context<DiffState>) {
        let mode = if self == Self::Split {
            DiffMode::Split
        } else {
            DiffMode::Unified
        };
        state.set_mode(mode, cx);
        state.set_context_lines((self == Self::Hunk).then_some(3), cx);
    }
}

pub(super) struct FileReview {
    pub(super) file: FileTarget,
    pub(super) patch: Load<Arc<Patch>>,
    pub(super) viewer: Option<Entity<DiffState>>,
    pub(super) message: Option<String>,
    pub(super) compact_context: bool,
    pub(super) hunks: Vec<String>,
    pub(super) source: Option<Load<Entity<DiffState>>>,
    pub(super) show_source: bool,
    pub(super) collapsed_sidebar: bool,
}

impl FileReview {
    pub(super) fn state(&self) -> Option<&Entity<DiffState>> {
        if self.show_source {
            match &self.source {
                Some(Load::Ready(state)) => Some(state),
                _ => None,
            }
        } else {
            self.viewer.as_ref()
        }
    }
}

struct LoadedDiff {
    patch: Arc<Patch>,
    hunks: Vec<String>,
    document: git::Result<Vec<DiffFile>>,
    compact_context: bool,
}

fn load_diff(repo: &Repo, file: &FileTarget) -> git::Result<LoadedDiff> {
    let patch = Arc::new(repo.patch(file)?);
    let hunks = patch.hunk_labels();
    let document = parse_patch(patch.bytes());
    let needs_context = matches!(file.change.code, b'M' | b'R' | b'C')
        && document
            .as_ref()
            .is_ok_and(|files| files.iter().any(|file| !file.is_binary()));
    let (document, compact_context) = if !needs_context {
        (document, false)
    } else if let Ok(Some(full)) = repo.full_patch(file)
        && patch.same_file_versions(&full)
        && let Ok(files) = parse_patch(&full)
    {
        (Ok(files), false)
    } else {
        (document, true)
    };
    Ok(LoadedDiff {
        patch,
        hunks,
        document,
        compact_context,
    })
}

fn parse_patch(bytes: &[u8]) -> git::Result<Vec<DiffFile>> {
    if bytes.len() > PATCH_VIEW_LIMIT {
        return Err("This diff exceeds the 8 MiB preview limit.".into());
    }
    let files = if bytes.is_empty() {
        Vec::new()
    } else {
        DiffFile::parse(&String::from_utf8_lossy(bytes))
            .map_err(|error| format!("Git produced a diff this viewer cannot display.\n{error}"))?
    };
    if files.is_empty() {
        Err("No textual changes for this file.".into())
    } else {
        Ok(files)
    }
}

impl Client {
    pub(super) fn open_diff(
        &mut self,
        id: u64,
        file: FileTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let collapse_default = self.saved.preferences.collapse_review_sidebar;
        let Some(tab) = self.tab_mut(id) else { return };
        let repo = tab.repo.clone();
        let target = file.clone();
        let previous = tab.review.active.as_ref();
        let show_source = file.change.code == b' ' || previous.is_some_and(|diff| diff.show_source);
        let collapsed_sidebar = previous.map_or(collapse_default, |diff| diff.collapsed_sidebar);
        let request = tab.review.set_active(Some(FileReview {
            file,
            patch: Load::Pending,
            viewer: None,
            message: None,
            compact_context: false,
            hunks: Vec::new(),
            source: None,
            show_source,
            collapsed_sidebar,
        }));
        Self::spawn_background(
            window,
            cx,
            move || load_diff(&repo, &target),
            move |this, result, _, cx| {
                let Some((diff, layout)) = this.review_for_request(id, request) else {
                    return;
                };
                match result {
                    Ok(loaded) => {
                        let line = loaded.patch.first_context_line();
                        diff.patch = Load::Ready(loaded.patch);
                        diff.hunks = loaded.hunks;
                        diff.compact_context = loaded.compact_context;
                        match loaded.document {
                            Ok(files) => {
                                let anchor = line.and_then(|line| {
                                    let file = files.first()?;
                                    file.modified_path()?;
                                    Some(DiffLinePosition::new(
                                        file.path().clone(),
                                        DiffSide::Modified,
                                        line,
                                    ))
                                });
                                diff.viewer = Some(cx.new(|cx| {
                                    let mut state = DiffState::new(files, cx);
                                    layout.apply(&mut state, cx);
                                    if let Some(anchor) = anchor.filter(|anchor| anchor.line() > 1)
                                    {
                                        state.scroll_to_line(anchor, cx);
                                    }
                                    state
                                }));
                            }
                            Err(message) => diff.message = Some(message.to_string()),
                        }
                    }
                    Err(error) => diff.patch = Load::Failed(error.to_string()),
                }
                cx.notify();
            },
        );
        if show_source {
            self.show_source(id, true, window, cx);
        }
        cx.notify();
    }

    pub(super) fn show_source(
        &mut self,
        id: u64,
        show: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = self.tab_mut(id) else { return };
        let Some(diff) = tab.review.active.as_mut() else {
            return;
        };
        diff.show_source = show;
        cx.notify();
        if !show || diff.source.is_some() {
            return;
        }
        diff.source = Some(Load::Pending);
        let request = tab.review.request;
        let target = diff.file.clone();
        let repo = tab.repo.clone();
        Self::spawn_background(
            window,
            cx,
            move || {
                let text = repo.file_contents(&target)?;
                let path = target.change.path.to_string_lossy().into_owned();
                Ok::<_, git::Error>(DiffFile::unchanged(path, &text))
            },
            move |this, result, _, cx| {
                let Some((diff, _)) = this.review_for_request(id, request) else {
                    return;
                };
                let viewer = result
                    .map(|file| cx.new(|cx| DiffState::new([file], cx).with_context_lines(None)));
                diff.source = Some(viewer.into());
                cx.notify();
            },
        );
    }

    fn review_for_request(
        &mut self,
        id: u64,
        request: u64,
    ) -> Option<(&mut FileReview, DiffLayout)> {
        let review = &mut self.tab_mut(id)?.review;
        if review.request != request {
            return None;
        }
        Some((review.active.as_mut()?, review.layout))
    }

    pub(super) fn set_diff_layout(&mut self, id: u64, layout: DiffLayout, cx: &mut Context<Self>) {
        let Some(tab) = self.tab_mut(id) else { return };
        tab.review.layout = layout;
        if let Some(diff) = tab.review.active.as_mut() {
            diff.show_source = false;
            if let Some(viewer) = &diff.viewer {
                viewer.update(cx, |state, cx| layout.apply(state, cx));
            }
        }
        cx.notify();
    }

    pub(super) fn move_diff_change(&mut self, id: u64, forward: bool, cx: &mut Context<Self>) {
        if let Some(state) = self
            .tab_mut(id)
            .and_then(|tab| tab.review.active.as_ref()?.viewer.as_ref())
        {
            state.update(cx, |state, cx| {
                if forward {
                    state.next_change(cx);
                } else {
                    state.previous_change(cx);
                }
            });
        }
    }
}

pub(super) struct ReviewState {
    pub(super) active: Option<FileReview>,
    request: u64,
    pub(super) layout: DiffLayout,
    pub(super) wrap_lines: bool,
}

impl ReviewState {
    pub(super) fn new(options: &Preferences) -> Self {
        Self {
            active: None,
            request: 0,
            layout: options.diff_layout,
            wrap_lines: options.wrap_lines,
        }
    }

    pub(super) fn close(&mut self) {
        self.set_active(None);
    }

    fn set_active(&mut self, active: Option<FileReview>) -> u64 {
        self.active = active;
        self.request += 1;
        self.request
    }
}

impl Client {
    pub(super) fn close_diff(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.session.active {
            if let Some(tab) = self.tab_mut(id) {
                tab.review.close();
                tab.history.focus.focus(window, cx);
                cx.notify();
            }
        }
    }
}
