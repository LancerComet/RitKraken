mod conflicts;
mod loading;
mod refresh;
mod staging;
mod view;

use std::sync::Arc;

use gpui_kit::*;
use ritkraken_diff::{DiffFile, DiffMode, DiffOverview, DiffParseOptions, DiffState};
use ritkraken_git::{self as git, FileTarget, PATCH_VIEW_LIMIT, Patch, Repo};

use super::{Client, Load, Message, preferences::Preferences};
use loading::load_source;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum DiffLayout {
    Hunk,
    Inline,
    #[default]
    Split,
}

impl DiffLayout {
    pub(super) const ALL: [Self; 3] = [Self::Hunk, Self::Inline, Self::Split];

    pub(super) fn id(self) -> &'static str {
        match self {
            Self::Hunk => "Hunk",
            Self::Inline => "Inline",
            Self::Split => "Split",
        }
    }

    pub(super) fn label_key(self) -> &'static str {
        match self {
            Self::Hunk => "app.review.hunk",
            Self::Inline => "app.review.inline",
            Self::Split => "app.review.split",
        }
    }

    pub(super) fn apply(self, state: &mut DiffState, cx: &mut Context<DiffState>) {
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
    conflict: Option<Entity<conflicts::ConflictReview>>,
    pub(super) file: FileTarget,
    pub(super) patch: Load<Arc<Patch>>,
    pub(super) viewer: Option<Entity<DiffState>>,
    pub(super) message: Option<Message>,
    pub(super) compact_context: bool,
    pub(super) source: Option<Load<Entity<DiffState>>>,
    source_text: Option<Arc<str>>,
    pub(super) show_source: bool,
    pub(super) collapsed_sidebar: bool,
}

impl FileReview {
    fn sync_source_overview(&self, cx: &mut App) {
        if let (Some(viewer), Some(Load::Ready(source))) = (&self.viewer, &self.source) {
            let changes = DiffOverview::from_files(viewer.read(cx).files());
            source.update(cx, |state, cx| state.set_overview_changes(changes, cx));
        }
    }

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
    conflict: Option<Arc<git::ConflictFile>>,
    patch: Arc<Patch>,
    document: Result<Vec<DiffFile>, Message>,
    compact_context: bool,
}

fn load_diff(
    repo: &Repo,
    file: &FileTarget,
    ignore_edge_whitespace: bool,
    previous: Option<&Patch>,
    previous_conflict: Option<&git::ConflictFile>,
) -> git::Result<Option<LoadedDiff>> {
    let conflict = repo.conflict_file(file)?.map(Arc::new);
    let patch = Arc::new(
        repo.patch(file)?
            .with_edge_whitespace_ignored(ignore_edge_whitespace),
    );
    if previous == Some(patch.as_ref()) && conflict.as_deref() == previous_conflict {
        return Ok(None);
    }
    if conflict.is_some() {
        return Ok(Some(LoadedDiff {
            conflict,
            patch,
            document: Ok(Vec::new()),
            compact_context: false,
        }));
    }
    let document = parse_patch(patch.bytes(), patch.ignores_edge_whitespace());
    let needs_context = matches!(file.change.code, b'M' | b'R' | b'C')
        && document
            .as_ref()
            .is_ok_and(|files| files.iter().any(|file| !file.is_binary()));
    let (document, compact_context) = if !needs_context {
        (document, false)
    } else if let Ok(compact) = &document
        && let Some(files) = load_full_diff(repo, file, &patch, compact)
    {
        (Ok(files), false)
    } else {
        (document, true)
    };
    let document = document.and_then(|files| {
        if patch.ignores_edge_whitespace()
            && patch.first_context_line().is_some()
            && files
                .iter()
                .all(DiffFile::same_text_ignoring_edge_whitespace)
        {
            Err(Message::Key("app.review.no_changes_ignoring_whitespace"))
        } else {
            Ok(files)
        }
    });
    let compact_context = compact_context && document.is_ok();
    Ok(Some(LoadedDiff {
        conflict: None,
        patch,
        document,
        compact_context,
    }))
}

fn load_full_diff(
    repo: &Repo,
    file: &FileTarget,
    patch: &Patch,
    compact: &[DiffFile],
) -> Option<Vec<DiffFile>> {
    let bytes = repo.full_patch(file).ok()??;
    if !patch.same_file_versions(&bytes) {
        return None;
    }
    if patch.ignores_edge_whitespace() && std::str::from_utf8(&bytes).is_err() {
        return None;
    }
    let files = parse_patch(&bytes, patch.ignores_edge_whitespace()).ok()?;
    if patch.hunk_count() == 0 {
        return Some(files);
    }
    let [compact] = compact else { return None };
    let [full]: [DiffFile; 1] = files.try_into().ok()?;
    Some(vec![full.with_hunk_headings_from(compact)?])
}

pub(in crate::app) fn parse_patch(
    bytes: &[u8],
    ignore_edge_whitespace: bool,
) -> Result<Vec<DiffFile>, Message> {
    if bytes.len() > PATCH_VIEW_LIMIT {
        return Err(Message::Key("app.error.diff_preview_limit"));
    }
    let files = if bytes.is_empty() {
        Vec::new()
    } else {
        DiffFile::parse_with(
            &String::from_utf8_lossy(bytes),
            DiffParseOptions::new().ignore_edge_whitespace(ignore_edge_whitespace),
        )
        .map_err(|error| Message::detail("app.error.diff_parse", error))?
    };
    if files.is_empty() {
        Err(Message::Key("app.review.no_text_changes"))
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
        tab.changes.selection.preview(&file);
        let repo = tab.repo.clone();
        let ignore_edge_whitespace = tab.review.ignore_edge_whitespace;
        let target = file.clone();
        let previous = tab.review.active.as_ref();
        let viewer = previous
            .filter(|diff| {
                diff.file.scope == file.scope && diff.file.change.path == file.change.path
            })
            .and_then(|diff| diff.viewer.clone());
        let show_source = file.change.code == b' ' || previous.is_some_and(|diff| diff.show_source);
        let collapsed_sidebar = previous.map_or(collapse_default, |diff| diff.collapsed_sidebar);
        let request = tab.review.set_active(Some(FileReview {
            conflict: None,
            file,
            patch: Load::Pending,
            viewer: None,
            message: None,
            compact_context: false,
            source: None,
            source_text: None,
            show_source,
            collapsed_sidebar,
        }));
        Self::spawn_background(
            window,
            cx,
            move || load_diff(&repo, &target, ignore_edge_whitespace, None, None),
            move |this, result, window, cx| {
                let Some((diff, layout)) = this.review_for_request(id, request) else {
                    return;
                };
                match result {
                    Ok(Some(loaded)) => diff.apply_loaded(loaded, layout, viewer, id, window, cx),
                    Ok(None) => {}
                    Err(error) => diff.patch = Load::Failed(error.into()),
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
        self.load_review_source(id, window, cx);
    }

    fn load_review_source(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tab) = self.tab_mut(id) else { return };
        let Some(diff) = tab.review.active.as_mut() else {
            return;
        };
        if diff.source.is_none() {
            diff.source = Some(Load::Pending);
        }
        let request = tab.review.request;
        let target = diff.file.clone();
        let repo = tab.repo.clone();
        let previous = diff.source_text.clone();
        Self::spawn_background(
            window,
            cx,
            move || load_source(&repo, &target, previous.as_deref()),
            move |this, result, _, cx| {
                let Some((diff, _)) = this.review_for_request(id, request) else {
                    return;
                };
                if diff.apply_source(result, cx) {
                    cx.notify();
                }
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

    fn toggle_edge_whitespace(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tab) = self.tab_mut(id).filter(|tab| !tab.busy) else {
            return;
        };
        let Some(review) = tab
            .review
            .active
            .as_ref()
            .filter(|review| !review.show_source)
        else {
            return;
        };
        let file = review.file.clone();
        tab.review.ignore_edge_whitespace = !tab.review.ignore_edge_whitespace;
        self.open_diff(id, file, window, cx);
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
    refresh_request: u64,
    pub(super) layout: DiffLayout,
    pub(super) wrap_lines: bool,
    ignore_edge_whitespace: bool,
}

impl ReviewState {
    pub(super) fn new(options: &Preferences) -> Self {
        Self {
            active: None,
            request: 0,
            refresh_request: 0,
            layout: options.diff_layout,
            wrap_lines: options.wrap_lines,
            ignore_edge_whitespace: false,
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
