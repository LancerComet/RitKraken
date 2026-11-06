use std::{collections::HashSet, path::PathBuf};

use gpui_kit::{prelude::*, *};
use ritkraken_git::Repo;

use crate::{
    app::{
        repository_manager::OpenRepo,
        widgets::{PAPER, Tone, button, error_text, note},
    },
    i18n::tr,
};

use super::{Client, Message, Tab};

pub(in crate::app) struct SavedTab {
    pub(super) id: u64,
    pub(super) path: PathBuf,
    state: SavedTabState,
}

enum SavedTabState {
    Idle,
    Loading,
    Failed(Message),
}

impl SavedTab {
    pub(in crate::app) fn is_loading(&self) -> bool {
        matches!(self.state, SavedTabState::Loading)
    }
}

impl Client {
    pub(in crate::app) fn restore_tabs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let paths = std::mem::take(&mut self.saved.open_tabs);
        let mut seen = HashSet::with_capacity(paths.len());
        for path in paths.into_iter().filter(|path| seen.insert(path.clone())) {
            let id = self.session.next_id();
            self.session.tabs.push(Tab::Saved(SavedTab {
                id,
                path,
                state: SavedTabState::Idle,
            }));
        }
        let active = self
            .session
            .tabs
            .iter()
            .find(|tab| Some(tab.root()) == self.saved.active_tab.as_deref())
            .or_else(|| self.session.tabs.first());
        if let Some(tab) = active {
            self.show_tab(tab.id(), window, cx);
        }
    }

    pub(super) fn load_saved_tab(&mut self, id: u64, window: &Window, cx: &Context<Self>) {
        let Some(Tab::Saved(tab)) = self.session.entry_mut(id) else {
            return;
        };
        if tab.is_loading() {
            return;
        }
        tab.state = SavedTabState::Loading;
        let request = OpenRepo::Open(tab.path.clone());
        let limit = self.saved.preferences.history_limit();
        Self::spawn_background(
            window,
            cx,
            move || request.load(limit, None),
            move |this, result, window, cx| {
                let Some(Tab::Saved(tab)) = this.session.entry_mut(id) else {
                    return;
                };
                match result {
                    Ok((repo, history)) => {
                        let id = this.insert_tab(repo, history, limit, Some(id), window, cx);
                        if this.session.active == Some(id) {
                            this.select_initial_commit(id, window, cx);
                        }
                        this.persist_session();
                    }
                    Err(error) => tab.state = SavedTabState::Failed(error.into()),
                }
                cx.notify();
            },
        );
    }

    pub(in crate::app) fn saved_tab_view(&self, tab: &SavedTab, cx: &Context<Self>) -> Div {
        let id = tab.id;
        let content = match &tab.state {
            SavedTabState::Failed(error) => div()
                .flex()
                .flex_col()
                .gap(px(8.))
                .child(error_text(error))
                .child(
                    button(
                        ("retry-saved-tab", id),
                        tr!("app.common.retry"),
                        Tone::Outline,
                        true,
                        cx.listener(move |this, _, window, cx| this.show_tab(id, window, cx)),
                    )
                    .self_start(),
                ),
            SavedTabState::Idle | SavedTabState::Loading => note(tr!("app.status.opening")).px_0(),
        };
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgb(PAPER))
            .child(
                div()
                    .w(px(560.))
                    .max_w_full()
                    .px(px(20.))
                    .child(
                        div()
                            .text_size(px(20.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .truncate()
                            .child(Repo::name_from_path(&tab.path)),
                    )
                    .child(note(tab.path.to_string_lossy().into_owned()).px_0())
                    .child(content),
            )
    }
}
