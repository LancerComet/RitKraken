use std::sync::Arc;

use gpui_kit::*;
use ritkraken_diff::{DiffFile, DiffLinePosition, DiffSide, DiffState};
use ritkraken_git::{self as git, FileTarget, Repo};

use super::{DiffLayout, FileReview, LoadedDiff};
use crate::app::{Client, Load};

pub(super) struct LoadedSource {
    text: Arc<str>,
    document: DiffFile,
}

pub(super) fn load_source(
    repo: &Repo,
    target: &FileTarget,
    previous: Option<&str>,
) -> git::Result<Option<LoadedSource>> {
    let text = repo.file_contents(target)?;
    if previous == Some(text.as_str()) {
        return Ok(None);
    }
    Ok(Some(LoadedSource {
        document: DiffFile::unchanged(target.change.path.to_string_lossy().into_owned(), &text),
        text: text.into(),
    }))
}

impl FileReview {
    pub(super) fn apply_loaded(
        &mut self,
        loaded: LoadedDiff,
        layout: DiffLayout,
        retained: Option<Entity<DiffState>>,
        cx: &mut Context<Client>,
    ) {
        let line = loaded.patch.first_context_line();
        let stageable = loaded.patch.hunk_count() > 0;
        self.patch = Load::Ready(loaded.patch);
        self.compact_context = loaded.compact_context;
        self.message = None;
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
                self.viewer = Some(match self.viewer.take().or(retained) {
                    Some(viewer) => {
                        viewer.update(cx, |state, cx| {
                            layout.apply(state, cx);
                            state.refresh_files(files, cx);
                        });
                        viewer
                    }
                    None => cx.new(|cx| {
                        let mut state = DiffState::new(files, cx);
                        layout.apply(&mut state, cx);
                        if stageable {
                            state.scroll_to_first_hunk(cx);
                        } else if let Some(anchor) = anchor.filter(|anchor| anchor.line() > 1) {
                            state.scroll_to_line(anchor, cx);
                        }
                        state
                    }),
                });
            }
            Err(message) => {
                self.viewer = None;
                self.message = Some(message);
            }
        }
        self.sync_source_overview(cx);
    }

    pub(super) fn apply_source(
        &mut self,
        result: git::Result<Option<LoadedSource>>,
        cx: &mut Context<Client>,
    ) -> bool {
        match result {
            Ok(None) => return false,
            Ok(Some(source)) => {
                let viewer = match self.source.take() {
                    Some(Load::Ready(viewer)) => {
                        viewer.update(cx, |state, cx| state.refresh_files([source.document], cx));
                        viewer
                    }
                    _ => {
                        cx.new(|cx| DiffState::new([source.document], cx).with_context_lines(None))
                    }
                };
                self.source = Some(Load::Ready(viewer));
                self.source_text = Some(source.text);
            }
            Err(error) => {
                self.source = Some(Load::Failed(error.into()));
                self.source_text = None;
            }
        }
        self.sync_source_overview(cx);
        true
    }
}
