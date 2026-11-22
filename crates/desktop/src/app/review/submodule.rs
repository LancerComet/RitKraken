use std::path::PathBuf;

use gpui_kit::{prelude::*, *};
use ritkraken_git::{Scope, SubmoduleInfo, short_id};

use crate::i18n::tr;

use super::FileReview;
use crate::app::{
    Client, RepoOperation, RepoTab,
    theme::{Palette, palette},
    widgets::{Tone, button, note},
};

fn commit_row(
    label: impl Into<SharedString>,
    oid: Option<&str>,
    subject: Option<&str>,
    p: Palette,
) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(8.))
        .child(
            div()
                .w(px(110.))
                .flex_shrink_0()
                .text_color(rgb(p.text_muted))
                .child(label.into()),
        )
        .child(
            div()
                .font_family("monospace")
                .flex_shrink_0()
                .child(match oid {
                    Some(oid) => SharedString::from(short_id(oid)),
                    None => tr!("app.review.submodule_none").into(),
                }),
        )
        .when_some(subject, |row, subject| {
            row.child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(p.text_muted))
                    .child(subject.to_owned()),
            )
        })
}

fn update_button(
    id: u64,
    element_id: &'static str,
    label: String,
    path: PathBuf,
    busy: bool,
    cx: &mut Context<Client>,
) -> Stateful<Div> {
    button(
        (element_id, id),
        label,
        Tone::Primary,
        !busy,
        cx,
        cx.listener(move |this, _, window, cx| {
            this.update_repo(
                id,
                Some(RepoOperation::UpdateSubmodule(path.clone())),
                window,
                cx,
            )
        }),
    )
    .rounded_full()
    .h(px(26.))
    .px(px(12.))
}

pub(super) fn panel(
    tab: &RepoTab,
    review: &FileReview,
    info: &SubmoduleInfo,
    cx: &mut Context<Client>,
) -> Div {
    let p = palette(cx);
    let id = tab.id;
    let change = &info.change;
    let path = info.path.clone();
    let relative = review.file.change.path.clone();
    let mut content = div()
        .size_full()
        .flex()
        .flex_col()
        .p(px(16.))
        .gap(px(10.))
        .child(
            div()
                .text_size(px(14.))
                .font_weight(FontWeight::SEMIBOLD)
                .child(tr!("app.review.submodule")),
        );
    if change.old == change.new {
        content = content.child(commit_row(
            tr!("app.review.submodule_current"),
            change.new.as_deref(),
            info.new_subject.as_deref().or(info.old_subject.as_deref()),
            p,
        ));
    } else {
        content = content
            .child(commit_row(
                tr!("app.review.submodule_old"),
                change.old.as_deref(),
                info.old_subject.as_deref(),
                p,
            ))
            .child(commit_row(
                tr!("app.review.submodule_new"),
                change.new.as_deref(),
                info.new_subject.as_deref(),
                p,
            ));
    }
    if change.modified {
        content = content.child(note(tr!("app.review.submodule_modified"), cx));
    }
    if change.untracked {
        content = content.child(note(tr!("app.review.submodule_untracked"), cx));
    }
    if !info.initialized {
        content = content.child(note(tr!("app.review.submodule_not_initialized"), cx));
    }
    let mut actions = div().flex().items_center().gap(px(8.)).child(
        button(
            ("open-submodule", id),
            tr!("app.review.submodule_open"),
            Tone::Primary,
            info.initialized,
            cx,
            cx.listener(move |this, _, window, cx| {
                this.open_recent_repo(path.clone(), window, cx)
            }),
        )
        .rounded_full()
        .h(px(26.))
        .px(px(12.)),
    );
    if !info.initialized {
        actions = actions.child(update_button(
            id,
            "init-submodule",
            tr!("app.review.submodule_init"),
            relative.clone(),
            tab.busy,
            cx,
        ));
    } else if review.file.scope == Scope::Worktree && change.old != change.new {
        let hint = change
            .old
            .as_deref()
            .map(|oid| tr!("app.review.submodule_update_hint", id = short_id(oid)));
        actions = actions
            .child(update_button(
                id,
                "update-submodule",
                tr!("app.review.submodule_update"),
                relative.clone(),
                tab.busy,
                cx,
            ))
            .when_some(hint, |row, hint| {
                row.child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(p.text_muted))
                        .child(hint),
                )
            });
    }
    content.child(actions)
}
