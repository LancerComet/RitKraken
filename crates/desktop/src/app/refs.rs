use std::collections::HashSet;

use gpui_kit::{
    component::{
        Sizable,
        input::{Input, InputState},
        scroll::ScrollableElement,
        tooltip::Tooltip,
    },
    prelude::*,
    *,
};
use ritkraken_git::{RefKind, Reference};

use crate::i18n::{LocalizedInput, tr};

use super::{
    Client, RepoOperation, RepoTab,
    widgets::{
        GREEN, HOVER, LINE, MUTED, PAPER, SELECT, SUBTLE, TEXT, Tone, WHITE, button, heading, icon,
    },
};

const REF_GROUPS: [(RefKind, &str, &str); 3] = [
    (RefKind::Local, "app.refs.local", "app.refs.local_branches"),
    (
        RefKind::Remote,
        "app.refs.remote",
        "app.refs.remote_branches",
    ),
    (RefKind::Tag, "app.refs.tags", "app.refs.tags"),
];

const SIDEBAR_ICON_SIZE: Pixels = px(14.);

pub(super) fn ref_icon(kind: RefKind) -> &'static str {
    match kind {
        RefKind::Local => "icons/laptop.svg",
        RefKind::Remote => "icons/cloud.svg",
        RefKind::Tag => "icons/tag.svg",
    }
}

pub(super) struct ReferenceState {
    pub(super) filter: LocalizedInput,
    closed_groups: HashSet<RefKind>,
    closed_folders: HashSet<String>,
    pub(super) collapsed: bool,
    _filter: Subscription,
}

impl ReferenceState {
    pub(super) fn new(window: &mut Window, cx: &mut Context<Client>) -> Self {
        let filter = LocalizedInput::new("app.refs.filter", cx, |cx| InputState::new(window, cx));
        let subscription = cx.observe(&filter, |_, _, cx| cx.notify());
        Self {
            filter,
            closed_groups: HashSet::new(),
            closed_folders: HashSet::new(),
            collapsed: false,
            _filter: subscription,
        }
    }
}

impl Client {
    pub(super) fn set_sidebar(
        &mut self,
        id: u64,
        collapse: bool,
        reveal: Option<RefKind>,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = self.tab_mut(id) else { return };
        if let Some(diff) = tab.review.active.as_mut() {
            diff.collapsed_sidebar = collapse;
        } else {
            tab.refs.collapsed = collapse;
        }
        if let Some(kind) = reveal {
            tab.refs.closed_groups.remove(&kind);
        }
        cx.notify();
    }

    pub(super) fn branch_rail(&self, tab: &RepoTab, cx: &mut Context<Self>) -> Div {
        let id = tab.id;
        div()
            .w(px(34.))
            .flex_shrink_0()
            .h_full()
            .flex()
            .flex_col()
            .items_center()
            .border_r_1()
            .border_color(rgb(LINE))
            .bg(rgb(PAPER))
            .child(
                button(
                    "expand-sidebar",
                    "",
                    Tone::Plain,
                    true,
                    cx.listener(move |this, _, _, cx| {
                        this.set_sidebar(id, false, None, cx);
                    }),
                )
                .w(px(31.))
                .h(px(30.))
                .child(icon("icons/chevron-right.svg", TEXT)),
            )
            .children(REF_GROUPS.into_iter().map(|(kind, _, title)| {
                let count = tab.history.data.snapshot.refs_of_kind(kind).count();
                div()
                    .id(title)
                    .aria_label(tr!(title))
                    .w(px(31.))
                    .h(px(41.))
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .hover(|style| style.bg(rgb(HOVER)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.set_sidebar(id, false, Some(kind), cx)
                    }))
                    .child(icon(ref_icon(kind), MUTED))
                    .child(
                        div()
                            .text_size(px(10.))
                            .text_color(rgb(0x315aae))
                            .child(count.to_string()),
                    )
            }))
    }

    pub(super) fn sidebar(&self, tab: &RepoTab, cx: &mut Context<Self>) -> Div {
        let filter = tab.refs.filter.read(cx).value().to_lowercase();
        let id = tab.id;
        let refs = &tab.history.data.snapshot.refs;
        let count = refs
            .iter()
            .filter(|reference| matches_ref_filter(reference, &filter))
            .count();
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(PAPER))
            .border_r_1()
            .border_color(rgb(LINE))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .h(px(26.))
                    .my(px(3.))
                    .px(px(9.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(3.))
                            .child(
                                button(
                                    "collapse-sidebar",
                                    "",
                                    Tone::Plain,
                                    true,
                                    cx.listener(move |this, _, _, cx| {
                                        this.set_sidebar(id, true, None, cx);
                                    }),
                                )
                                .w(px(22.))
                                .h(px(22.))
                                .child(icon("icons/chevron-left.svg", TEXT)),
                            )
                            .child(div().child(tr!(
                                "app.refs.viewing",
                                count = count,
                                total = refs.len()
                            ))),
                    )
                    .child(button(
                        "reset-branch-filter",
                        tr!("app.refs.show_all"),
                        Tone::Plain,
                        !filter.is_empty(),
                        cx.listener(move |this, _, window, cx| {
                            if let Some(tab) = this.tab_mut(id) {
                                tab.refs
                                    .filter
                                    .update(cx, |input, cx| input.set_value("", window, cx));
                                cx.notify();
                            }
                        }),
                    )),
            )
            .child(
                div()
                    .px(px(8.))
                    .pb(px(7.))
                    .child(Input::new(&tab.refs.filter).small()),
            )
            .child(
                div().flex_1().min_h_0().child(
                    div()
                        .flex()
                        .flex_col()
                        .size_full()
                        .children(
                            REF_GROUPS.into_iter().map(|(kind, title, _)| {
                                self.ref_group(tab, kind, title, &filter, cx)
                            }),
                        )
                        .overflow_y_scrollbar(),
                ),
            )
    }

    fn ref_group(
        &self,
        tab: &RepoTab,
        kind: RefKind,
        title: &'static str,
        filter: &str,
        cx: &mut Context<Self>,
    ) -> Div {
        let snapshot = &tab.history.data.snapshot;
        let refs: Vec<_> = snapshot.refs_of_kind(kind).collect();
        let matches: Vec<_> = refs
            .iter()
            .copied()
            .filter(|reference| matches_ref_filter(reference, filter))
            .collect();
        let open = !tab.refs.closed_groups.contains(&kind);
        let tab_id = tab.id;
        let mut group = div().flex().flex_col().pb(px(5.)).child(
            div()
                .id(title)
                .group("ref-group-heading")
                .h(px(27.))
                .px(px(8.))
                .flex()
                .items_center()
                .justify_between()
                .border_t_1()
                .border_color(rgb(LINE))
                .hover(|s| s.bg(rgb(HOVER)))
                .cursor_pointer()
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(tab) = this.tab_mut(tab_id) {
                        if !tab.refs.closed_groups.remove(&kind) {
                            tab.refs.closed_groups.insert(kind);
                        }
                        cx.notify();
                    }
                }))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .child(disclosure_chevron(open))
                        .child(sidebar_icon(ref_icon(kind)))
                        .child(heading(tr!(title)).text_size(px(13.))),
                )
                .child(
                    div()
                        .relative()
                        .h(px(21.))
                        .flex()
                        .items_center()
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(rgb(0x315aae))
                                .child(format!("{}/{}", matches.len(), refs.len()))
                                .when(kind == RefKind::Remote, |count| {
                                    count
                                        .group_hover("ref-group-heading", |style| style.invisible())
                                }),
                        )
                        .when(kind == RefKind::Remote, |row| {
                            row.child(
                                button(
                                    "add-remote",
                                    "",
                                    Tone::Plain,
                                    !tab.busy,
                                    cx.listener(move |this, _, window, cx| {
                                        cx.stop_propagation();
                                        this.open_add_remote(tab_id, window, cx);
                                    }),
                                )
                                .absolute()
                                .right_0()
                                .top_0()
                                .w(px(21.))
                                .h(px(21.))
                                .px(px(0.))
                                .child(icon("icons/plus.svg", TEXT).size(px(14.)))
                                .aria_label(tr!("app.remotes.add"))
                                .invisible()
                                .group_hover("ref-group-heading", |style| style.visible())
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                .tooltip(|window, cx| {
                                    Tooltip::new(tr!("app.remotes.add")).build(window, cx)
                                }),
                            )
                        }),
                ),
        );
        if !open {
            return group;
        }
        let mut seen = HashSet::new();
        for reference in matches {
            let parts: Vec<_> = reference.name.split('/').collect();
            let mut hidden = false;
            for depth in 0..parts.len().saturating_sub(1) {
                let key = format!("{title}/{}", parts[..=depth].join("/"));
                let collapsed = tab.refs.closed_folders.contains(&key) && filter.is_empty();
                if seen.insert(key.clone()) {
                    group = group.child(ref_folder(
                        tab_id,
                        key,
                        parts[depth].to_owned(),
                        depth,
                        kind == RefKind::Remote && depth == 0,
                        Some(!collapsed),
                        cx,
                    ));
                }
                if collapsed {
                    hidden = true;
                    break;
                }
            }
            if hidden {
                continue;
            }
            let checked = snapshot.is_current_branch(reference);
            let selected = tab.history.selected.as_ref() == Some(&reference.id);
            let branch = reference.name.clone();
            let oid = reference.id.clone();
            group = group.child(
                div()
                    .id(SharedString::from(format!("{title}/ref/{branch}")))
                    .h(px(23.))
                    .mx(px(3.))
                    .pr(px(6.))
                    .pl(px(5.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .cursor_pointer()
                    .min_w_0()
                    .when(checked, |e| e.bg(rgb(0xcce6cc)))
                    .when(!checked && selected, |e| e.bg(rgb(SELECT)))
                    .hover(|s| s.bg(rgb(if checked { 0xbce0bf } else { HOVER })))
                    .on_click(cx.listener(move |this, click: &ClickEvent, window, cx| {
                        if click.click_count() == 2 && kind == RefKind::Local && !checked {
                            this.update_repo(
                                tab_id,
                                Some(RepoOperation::Switch(branch.clone())),
                                window,
                                cx,
                            );
                        } else {
                            this.select(
                                tab_id,
                                Some(oid.clone()),
                                Some(ScrollStrategy::Center),
                                window,
                                cx,
                            );
                        }
                    }))
                    .child(
                        div()
                            .size(px(12.))
                            .mx(px(1.))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(2.))
                            .when(checked, |badge| {
                                badge
                                    .bg(rgb(GREEN))
                                    .child(icon("icons/check.svg", WHITE).size(px(10.)))
                            }),
                    )
                    .child(
                        sidebar_icon(if kind == RefKind::Tag {
                            ref_icon(kind)
                        } else {
                            "icons/git-branch.svg"
                        })
                        .text_color(rgb(SUBTLE))
                        .ml(px((parts.len() - 1) as f32 * 13.)),
                    )
                    .child(
                        div()
                            .truncate()
                            .child(parts.last().copied().unwrap_or(&reference.name).to_owned()),
                    ),
            );
        }
        if kind == RefKind::Remote {
            for remote in &snapshot.remotes {
                let prefix = format!("{remote}/");
                if remote.to_lowercase().contains(filter)
                    && !refs
                        .iter()
                        .any(|reference| reference.name.starts_with(&prefix))
                {
                    group = group.child(ref_folder(
                        tab_id,
                        format!("{title}/{remote}"),
                        remote.clone(),
                        0,
                        true,
                        None,
                        cx,
                    ));
                }
            }
        }
        group
    }
}

fn ref_folder(
    tab_id: u64,
    key: String,
    label: String,
    depth: usize,
    remote: bool,
    expanded: Option<bool>,
    cx: &mut Context<Client>,
) -> Stateful<Div> {
    div()
        .id(SharedString::from(key.clone()))
        .flex()
        .items_center()
        .gap(px(6.))
        .h(px(22.))
        .pl(px(10. + 13. * depth as f32))
        .min_w_0()
        .hover(|style| style.bg(rgb(HOVER)))
        .when(expanded.is_some(), |row| {
            row.cursor_pointer()
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(tab) = this.tab_mut(tab_id) {
                        if !tab.refs.closed_folders.remove(&key) {
                            tab.refs.closed_folders.insert(key.clone());
                        }
                        cx.notify();
                    }
                }))
        })
        .child(
            div()
                .size(px(12.))
                .flex_shrink_0()
                .when_some(expanded, |slot, open| slot.child(disclosure_chevron(open))),
        )
        .child(sidebar_icon(if remote {
            ref_icon(RefKind::Remote)
        } else {
            "icons/folder.svg"
        }))
        .child(div().text_color(rgb(MUTED)).truncate().child(label))
}

fn sidebar_icon(path: &'static str) -> Svg {
    icon(path, MUTED).size(SIDEBAR_ICON_SIZE)
}

fn disclosure_chevron(open: bool) -> Svg {
    icon("icons/chevron-right.svg", MUTED)
        .size(px(12.))
        .when(open, |arrow| {
            arrow.with_transformation(Transformation::rotate(radians(std::f32::consts::FRAC_PI_2)))
        })
}

fn matches_ref_filter(reference: &Reference, filter: &str) -> bool {
    reference.name.to_lowercase().contains(filter)
}
