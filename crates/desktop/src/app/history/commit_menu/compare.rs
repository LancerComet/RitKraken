use gpui_kit::{prelude::*, *};
use ritkraken_diff::{DiffState, DiffTypography};
use ritkraken_git::{CommitMenuState, Repo, short_id};

use crate::{
    app::{
        Client, Load,
        modals::{ContentModal, ModalHandle},
        preferences::Preferences,
        review::{DiffLayout, parse_patch},
        widgets::{error_text, note},
    },
    i18n::tr,
};

pub(super) struct CommitComparison {
    data: Load<Entity<DiffState>>,
    typography: DiffTypography,
    layout: DiffLayout,
    wrap_lines: bool,
}

impl CommitComparison {
    pub fn open(
        repo: Repo,
        context: &CommitMenuState,
        preferences: &Preferences,
        window: &mut Window,
        cx: &mut Context<Client>,
    ) {
        let layout = preferences.diff_layout;
        let wrap_lines = preferences.wrap_lines;
        let typography = preferences.editor.diff_typography();
        let (from, to) = (context.commit.id.clone(), context.head.clone());
        let title = tr!(
            "app.commit_menu.comparison",
            from = short_id(&from),
            to = short_id(&to)
        );
        let viewer = cx.new(|cx| {
            Client::spawn_background(
                window,
                cx,
                move || repo.compare_commits(&from, &to),
                |this: &mut Self, result, _, cx| {
                    this.data = result
                        .map_err(crate::app::Message::from)
                        .and_then(|bytes| parse_patch(&bytes, false))
                        .map(|files| {
                            cx.new(|cx| {
                                let mut state = DiffState::new(files, cx);
                                this.layout.apply(&mut state, cx);
                                state
                            })
                        })
                        .into();
                    cx.notify();
                },
            );
            Self {
                data: Load::Pending,
                typography,
                layout,
                wrap_lines,
            }
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
}

impl Render for CommitComparison {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(match &self.data {
            Load::Pending => note(tr!("app.commit_menu.loading")).into_any_element(),
            Load::Failed(error) => error_text(error).into_any_element(),
            Load::Ready(state) => self
                .layout
                .viewer(state, self.wrap_lines, self.typography.clone())
                .header_visible(true)
                .into_any_element(),
        })
    }
}
