use gpui_kit::{component::tooltip::Tooltip, prelude::*, *};
use ritkraken_git::{Commit, CommitDetail, CommitPerson, short_id};

use crate::{
    app::{
        Client, RepoTab,
        avatar::PersonAvatar,
        widgets::{MUTED, Tone, button},
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
                        &commit.author,
                        &commit.author_email,
                        tr!("app.commit.authored", date = &commit.date),
                    ))
                    .when_some(commit.parents.first().cloned(), |element, parent| {
                        element.child(
                            button(
                                ("parent", id),
                                tr!("app.commit.parent", sha = short_id(&parent)),
                                Tone::Plain,
                                tab.history.data.position.contains_key(&parent),
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
                            .flex_shrink_0()
                            .h(px(22.))
                            .text_size(px(11.)),
                        )
                    }),
            )
            .when(!detail.co_authors.is_empty(), |element| {
                element.child(co_authors(&detail.co_authors))
            })
            .when(distinct_committer, |element| {
                element.child(person_row(
                    &committer.name,
                    &committer.email,
                    tr!("app.commit.committed", date = &detail.committed_date),
                ))
            })
    }
}

fn person_row(name: &str, email: &str, timestamp: String) -> Div {
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
                .child(
                    div()
                        .truncate()
                        .font_weight(FontWeight::MEDIUM)
                        .child(name.to_owned()),
                )
                .child(
                    div()
                        .truncate()
                        .text_color(rgb(MUTED))
                        .text_size(px(11.))
                        .child(timestamp),
                ),
        )
}

fn co_authors(authors: &[CommitPerson]) -> Div {
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
                .text_color(rgb(MUTED))
                .text_size(px(11.))
                .child(tr!("app.commit.co_authors")),
        )
        .child(div().flex().min_w_0().flex_wrap().gap(px(4.)).children(
            authors.iter().enumerate().map(|(index, person)| {
                let label = format!("{} <{}>", person.name, person.email);
                div()
                    .id(("co-author", index))
                    .flex_shrink_0()
                    .child(PersonAvatar::new(
                        &person.name,
                        &person.email,
                        px(20.),
                        None,
                    ))
                    .tooltip(move |window, cx| Tooltip::new(label.clone()).build(window, cx))
            }),
        ))
}
