mod view;

use std::sync::Arc;

use gpui_kit::{
    component::input::{EditorState, InputEvent, Position},
    *,
};
use ritkraken_diff::{DiffConflictResolution, DiffFile, DiffState};
use ritkraken_git::{ConflictFile, ConflictResolution};

use crate::app::{Client, RepoOperation};

pub(in crate::app::review) struct ConflictReview {
    id: u64,
    owner: WeakEntity<Client>,
    pub(super) expected: Arc<ConflictFile>,
    output: Entity<EditorState>,
    sources: [Option<Entity<DiffState>>; 2],
    document: Option<DiffFile>,
    selected: usize,
    pub(super) dirty: bool,
    pub(super) stale: bool,
    horizontal: Entity<gpui_kit::base::ResizableState>,
    vertical: Entity<gpui_kit::base::ResizableState>,
    _edits: Subscription,
}

impl ConflictReview {
    pub(super) fn new(
        id: u64,
        expected: Arc<ConflictFile>,
        window: &mut Window,
        cx: &mut Context<Client>,
    ) -> Entity<Self> {
        let owner = cx.weak_entity();
        cx.new(|cx| {
            let text = expected.working_text.clone().unwrap_or_default();
            let document = DiffFile::parse_conflicts(
                expected.file.change.path.to_string_lossy().into_owned(),
                &text,
            )
            .ok();
            let output = cx.new(|cx| {
                EditorState::new(window, cx)
                    .default_value(text)
                    .line_number(true)
            });
            let sources = [&expected.current, &expected.incoming].map(|version| {
                version
                    .as_ref()
                    .and_then(|version| version.text.as_ref())
                    .map(|text| {
                        cx.new(|cx| {
                            DiffState::new(
                                [DiffFile::unchanged(
                                    expected.file.change.path.to_string_lossy().into_owned(),
                                    text,
                                )],
                                cx,
                            )
                            .with_context_lines(None)
                        })
                    })
            });
            let edits = cx.subscribe(&output, |this: &mut Self, _, event, cx| {
                if matches!(event, InputEvent::Change) {
                    this.reparse(cx);
                }
            });
            Self {
                id,
                owner,
                expected,
                output,
                sources,
                document,
                selected: 0,
                dirty: false,
                stale: false,
                horizontal: cx.new(|_| gpui_kit::base::ResizableState::default()),
                vertical: cx.new(|_| gpui_kit::base::ResizableState::default()),
                _edits: edits,
            }
        })
    }

    fn reparse(&mut self, cx: &mut Context<Self>) {
        let text = self.output.read(cx).value();
        self.dirty = Some(text.as_str()) != self.expected.working_text.as_deref();
        self.document = DiffFile::parse_conflicts(
            self.expected
                .file
                .change
                .path
                .to_string_lossy()
                .into_owned(),
            &text,
        )
        .ok();
        self.selected = self.selected.min(self.count().saturating_sub(1));
        cx.notify();
    }

    fn count(&self) -> usize {
        self.document
            .as_ref()
            .map_or(0, |file| file.conflicts().len())
    }

    fn choose(
        &mut self,
        resolution: DiffConflictResolution,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(document) = &self.document else {
            return;
        };
        if self.count() == 0 {
            return;
        }
        let mut choices = vec![None; self.count()];
        choices[self.selected] = Some(resolution);
        if let Some(text) = document.conflict_output(&choices) {
            self.output
                .update(cx, |state, cx| state.set_value(text, window, cx));
            self.reparse(cx);
            self.reveal(window, cx);
        }
    }

    fn reveal(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(conflict) = self
            .document
            .as_ref()
            .and_then(|file| file.conflicts().get(self.selected))
        {
            let line = conflict.lines().start.saturating_sub(1) as u32;
            self.output.update(cx, |state, cx| {
                state.set_cursor_position(Position { line, character: 0 }, window, cx)
            });
        }
    }

    fn navigate(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.count() == 0 {
            return;
        }
        self.selected = if forward {
            (self.selected + 1) % self.count()
        } else {
            (self.selected + self.count() - 1) % self.count()
        };
        self.reveal(window, cx);
        cx.notify();
    }

    fn apply(&self, resolution: ConflictResolution, window: &mut Window, cx: &mut Context<Self>) {
        let expected = self.expected.clone();
        let _ = self.owner.update(cx, |client, cx| {
            client.update_repo(
                self.id,
                Some(RepoOperation::ResolveConflict(expected, resolution)),
                window,
                cx,
            );
        });
    }

    fn reload(&self, window: &mut Window, cx: &mut Context<Self>) {
        let _ = self.owner.update(cx, |client, cx| {
            if let Some(file) = client
                .tab(self.id)
                .and_then(|tab| tab.review.active.as_ref())
                .map(|review| review.file.clone())
            {
                client.open_diff(self.id, file, window, cx);
            }
        });
    }
}
