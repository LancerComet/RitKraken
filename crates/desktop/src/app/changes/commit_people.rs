use gpui_kit::{component::tooltip::Tooltip, prelude::*, *};
use ritkraken_git::{Commit, CommitDetail, CommitPerson, short_id};

use crate::{
    app::{
        Client, RepoTab,
        avatar::PersonAvatar,
        theme::{Palette, palette},
        widgets::{Tone, button},
    },
    i18n::tr,
};

impl Client {
    pub(super) fn commit_people(
        &self,
        tab: &RepoTab,
        commit: &Commit,
        detail: &CommitDetail,
        cx: &Context<Self>,
    ) -> Div {
        let p = palette(cx);
        let id = tab.id;
        let committer = &detail.committer;
        let distinct_committer = committer.name != commit.author
            || !committer.email.eq_ignore_ascii_case(&commit.author_email);
        div()
            .flex()
            .flex_col()
            .gap(px(7.))
            .px(px(9.))
            .py(px(8.))
            .child(
                div()
                    .flex()
                    .items_start()
                    .justify_between()
                    .gap(px(5.))
                    .child(person_row(
                        "commit-author-name",
                        &commit.author,
                        &commit.author_email,
                        tr!("app.commit.authored", date = &commit.date),
                        p,
                    ))
                    .when_some(commit.parents.first().cloned(), |element, parent| {
                        element.child(
                            button(
                                ("parent", id),
                                tr!(
                                    "app.commit.parent",
                                    sha = short_id(crate::app::history::rebase::real_oid(&parent))
                                ),
                                Tone::Plain,
                                tab.active_history().position.contains_key(&parent),
                                cx,
                                cx.listener(move |this, _, window, cx| {
                                    this.select(
                                        id,
                                        Some(parent.clone()),
                                        Some(ScrollStrategy::Center),
                                        window,
                                        cx,
                                    );
                                }),
                            )
                            .rounded_full()
                            .flex_shrink_0()
                            .h(px(22.))
                            .text_size(px(11.)),
                        )
                    }),
            )
            .when(!detail.co_authors.is_empty(), |element| {
                element.child(co_authors(&detail.co_authors, p))
            })
            .when(distinct_committer, |element| {
                element.child(person_row(
                    "commit-committer-name",
                    &committer.name,
                    &committer.email,
                    tr!("app.commit.committed", date = &detail.committed_date),
                    p,
                ))
            })
    }
}

fn person_row(id: &'static str, name: &str, email: &str, timestamp: String, p: Palette) -> Div {
    div()
        .min_w_0()
        .flex()
        .items_center()
        .gap(px(7.))
        .child(PersonAvatar::new(name, email, px(31.), None))
        .child(
            div()
                .min_w_0()
                .flex()
                .flex_col()
                .child(with_person_tooltip(
                    div()
                        .id(id)
                        .truncate()
                        .font_weight(FontWeight::MEDIUM)
                        .child(name.to_owned()),
                    name,
                    email,
                ))
                .child(
                    div()
                        .truncate()
                        .text_color(rgb(p.text_muted))
                        .text_size(px(11.))
                        .child(timestamp),
                ),
        )
}

fn co_authors(authors: &[CommitPerson], p: Palette) -> Div {
    div()
        .flex()
        .items_start()
        .gap(px(5.))
        .child(
            div()
                .flex()
                .items_center()
                .flex_shrink_0()
                .h(px(20.))
                .text_color(rgb(p.text_muted))
                .text_size(px(11.))
                .child(tr!("app.commit.co_authors")),
        )
        .child(div().flex().min_w_0().flex_wrap().gap(px(4.)).children(
            authors.iter().enumerate().map(|(index, person)| {
                with_person_tooltip(
                    div()
                        .id(("co-author", index))
                        .flex_shrink_0()
                        .child(PersonAvatar::new(
                            &person.name,
                            &person.email,
                            px(20.),
                            None,
                        )),
                    &person.name,
                    &person.email,
                )
            }),
        ))
}

fn with_person_tooltip(element: Stateful<Div>, name: &str, email: &str) -> Stateful<Div> {
    let label = format!("{name} <{email}>");
    element.tooltip(move |window, cx| Tooltip::new(label.clone()).build(window, cx))
}
