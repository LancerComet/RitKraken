mod picker;
mod view;

use std::sync::Arc;

use gpui_kit::{component::input::InputState, prelude::*, *};
use ritkraken_diff::{DiffState, DiffTypography};
use ritkraken_git::{RefComparison, Reference, Repo, Snapshot, short_id};

use crate::{
    app::{
        Client, Load, Message,
        modals::{ContentModal, ModalHandle},
        preferences::Preferences,
        review::{DiffLayout, parse_patch},
    },
    i18n::{LocalizedInput, tr},
};

const COMPARE_LIMIT: usize = 1000;

/// A picked revision: the label shown in the picker and the string passed to
/// Git (`HEAD` or a full ref name).
#[derive(Clone)]
pub(super) struct CompareRef {
    label: String,
    revision: String,
}

impl CompareRef {
    pub(super) fn head(snapshot: &Snapshot) -> Self {
        let label = snapshot.branch.clone().unwrap_or_else(|| {
            snapshot.head.as_deref().map_or_else(
                || "HEAD".to_owned(),
                |head| tr!("app.compare.detached_head", sha = short_id(head)),
            )
        });
        Self {
            label,
            revision: "HEAD".into(),
        }
    }

    pub(super) fn reference(reference: &Reference) -> Self {
        Self {
            label: reference.name.clone(),
            revision: reference.full_name(),
        }
    }
}

#[derive(Clone, Eq, PartialEq)]
enum Selection {
    All,
    Commit(String),
}

#[derive(Clone, Copy)]
enum PickerSide {
    Base,
    Compare,
}

/// The ref-comparison modal: commits each side owns exclusively plus the diff
/// the compare side would bring into the base.
pub(super) struct RefCompare {
    repo: Repo,
    snapshot: Snapshot,
    base: CompareRef,
    compare: CompareRef,
    result: Load<Arc<RefComparison>>,
    selection: Selection,
    all_view: Load<Entity<DiffState>>,
    commit_view: Load<Entity<DiffState>>,
    request: u64,
    filter: LocalizedInput,
    typography: DiffTypography,
    layout: DiffLayout,
    wrap_lines: bool,
    _filter: Subscription,
}

impl RefCompare {
    pub(super) fn open(
        repo: Repo,
        snapshot: &Snapshot,
        preferences: &Preferences,
        base: CompareRef,
        compare: CompareRef,
        window: &mut Window,
        cx: &mut Context<Client>,
    ) {
        let title = tr!("app.compare.title");
        let snapshot = snapshot.clone();
        let viewer = cx.new(|cx| {
            let filter =
                LocalizedInput::new("app.compare.filter", cx, |cx| InputState::new(window, cx));
            let _filter = cx.observe(&filter, |_, _, cx| cx.notify());
            let mut this = Self {
                repo,
                snapshot,
                base,
                compare,
                result: Load::Pending,
                selection: Selection::All,
                all_view: Load::Pending,
                commit_view: Load::Pending,
                request: 0,
                filter,
                typography: preferences.editor.diff_typography(),
                layout: preferences.diff_layout,
                wrap_lines: preferences.wrap_lines,
                _filter,
            };
            this.reload(window, cx);
            this
        });
        Client::present_content_modal(
            ContentModal::new(
                ModalHandle::new(cx),
                title,
                viewer,
                size(px(1400.), px(877.)),
                false,
            ),
            window,
            cx,
        );
    }

    fn reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.result = Load::Pending;
        self.selection = Selection::All;
        self.all_view = Load::Pending;
        self.commit_view = Load::Pending;
        self.request += 1;
        let request = self.request;
        let repo = self.repo.clone();
        let (base, compare) = (self.base.revision.clone(), self.compare.revision.clone());
        Client::spawn_background(
            window,
            cx,
            move || repo.compare_refs(&base, &compare, COMPARE_LIMIT),
            move |this: &mut Self, result, _, cx| {
                if this.request != request {
                    return;
                }
                this.result = result.map(Arc::new).map_err(Message::from).into();
                this.show_all(cx);
                cx.notify();
            },
        );
        cx.notify();
    }

    fn show_all(&mut self, cx: &mut Context<Self>) {
        self.selection = Selection::All;
        self.request += 1;
        if let Load::Ready(comparison) = &self.result {
            self.all_view = comparison.patch.as_ref().map_or_else(
                || Load::Failed(Message::Key("app.error.diff_preview_limit")),
                |bytes| self.parse(bytes, cx),
            );
        }
        cx.notify();
    }

    fn select_commit(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        self.selection = Selection::Commit(id.clone());
        self.commit_view = Load::Pending;
        self.request += 1;
        let request = self.request;
        let repo = self.repo.clone();
        Client::spawn_background(
            window,
            cx,
            move || repo.commit_diff(&id),
            move |this: &mut Self, result, _, cx| {
                if this.request != request {
                    return;
                }
                this.commit_view = match result {
                    Err(error) => Load::Failed(error.into()),
                    Ok(None) => Load::Failed(Message::Key("app.error.diff_preview_limit")),
                    Ok(Some(bytes)) => this.parse(&bytes, cx),
                };
                cx.notify();
            },
        );
        cx.notify();
    }

    fn swap(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        std::mem::swap(&mut self.base, &mut self.compare);
        self.reload(window, cx);
    }

    fn set_ref(
        &mut self,
        side: PickerSide,
        selected: CompareRef,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match side {
            PickerSide::Base => self.base = selected,
            PickerSide::Compare => self.compare = selected,
        }
        self.reload(window, cx);
    }

    fn parse(&self, bytes: &[u8], cx: &mut Context<Self>) -> Load<Entity<DiffState>> {
        match parse_patch(bytes, false) {
            Ok(files) => Load::Ready(cx.new(|cx| {
                let mut state = DiffState::new(files, cx);
                self.layout.apply(&mut state, cx);
                state
            })),
            Err(Message::Key("app.review.no_text_changes")) => {
                Load::Failed(Message::Key("app.compare.no_changes"))
            }
            Err(message) => Load::Failed(message),
        }
    }
}

impl Client {
    /// Opens the ref-comparison modal for the tab.
    pub(super) fn open_compare(
        &mut self,
        id: u64,
        base: CompareRef,
        compare: CompareRef,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = self.tab(id) else { return };
        RefCompare::open(
            tab.repo.clone(),
            &tab.active_history().snapshot,
            &self.saved.preferences,
            base,
            compare,
            window,
            cx,
        );
    }
}
