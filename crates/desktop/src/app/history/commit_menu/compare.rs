use gpui_kit::{prelude::*, *};
use ritkraken_diff::DiffTypography;
use ritkraken_git::{CommitMenuState, Repo, short_id};

use crate::{
    app::{
        Client, Load, Message,
        compare::DiffFileBrowser,
        modals::{ContentModal, ModalHandle},
        preferences::Preferences,
        review::{DiffLayout, parse_patch},
        widgets::{error_text, info_text, note},
    },
    i18n::tr,
};

pub(super) struct CommitComparison {
    data: Load<Entity<DiffFileBrowser>>,
    typography: DiffTypography,
    layout: DiffLayout,
    wrap_lines: bool,
}

impl CommitComparison {
    pub fn open(
        repo: Repo,
        context: &CommitMenuState,
        preferences: &Preferences,
        tree: bool,
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
                move |this: &mut Self, result, _, cx| {
                    let (layout, wrap_lines, typography) =
                        (this.layout, this.wrap_lines, this.typography.clone());
                    this.data = result
                        .map_err(Message::from)
                        .and_then(|bytes| {
                            parse_patch(&bytes, false).map_err(|message| match message {
                                Message::Key("app.review.no_text_changes") => {
                                    Message::Key("app.compare.no_changes")
                                }
                                message => message,
                            })
                        })
                        .map(|files| {
                            cx.new(move |cx| {
                                DiffFileBrowser::new(
                                    files, tree, layout, wrap_lines, typography, cx,
                                )
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
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(match &self.data {
            Load::Pending => div()
                .p(px(12.))
                .child(note(tr!("app.commit_menu.loading"), cx))
                .into_any_element(),
            Load::Failed(message @ Message::Key("app.compare.no_changes")) => div()
                .p(px(12.))
                .child(info_text(message, cx))
                .into_any_element(),
            Load::Failed(error) => div()
                .p(px(12.))
                .child(error_text(error, cx))
                .into_any_element(),
            Load::Ready(browser) => browser.clone().into_any_element(),
        })
    }
}
