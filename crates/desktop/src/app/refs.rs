use std::collections::HashSet;

use gpui_kit::{
    component::{
        Sizable,
        input::{Input, InputState},
        scroll::ScrollableElement,
    },
    prelude::*,
    *,
};
use ritkraken_git::{RefKind, Reference};

use super::{
    Client, RepoOperation, RepoTab,
    widgets::{
        GREEN, HOVER, LINE, MUTED, PAPER, SELECT, SUBTLE, TEXT, Tone, button, heading, icon,
    },
};

const REF_GROUPS: [(RefKind, &str, &str, &str); 3] = [
    (
        RefKind::Local,
        "LOCAL",
        "Local branches",
        "icons/laptop.svg",
    ),
    (
        RefKind::Remote,
        "REMOTE",
        "Remote branches",
        "icons/cloud.svg",
    ),
    (RefKind::Tag, "TAGS", "Tags", "icons/tag.svg"),
];

pub(super) struct ReferenceState {
    pub(super) filter: Entity<InputState>,
    closed_groups: HashSet<RefKind>,
    closed_folders: HashSet<String>,
    pub(super) collapsed: bool,
    _filter: Subscription,
}

impl ReferenceState {
    pub(super) fn new(window: &mut Window, cx: &mut Context<Client>) -> Self {
        let filter =
            cx.new(|cx| InputState::new(window, cx).placeholder("Filter branches and tags"));
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
            .children(REF_GROUPS.into_iter().map(|(kind, _, title, glyph)| {
                let count = tab.history.data.snapshot.refs_of_kind(kind).count();
                div()
                    .id(title)
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
                    .child(icon(glyph, MUTED))
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
                            .child(div().child(format!("Viewing {count}/{}", refs.len()))),
                    )
                    .child(button(
                        "reset-branch-filter",
                        "Show All",
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
                        .children(REF_GROUPS.into_iter().map(|(kind, title, _, _)| {
                            self.ref_group(tab, kind, title, &filter, cx)
                        }))
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
                .child(heading(format!(
                    "{}  {title}",
                    if open { "⌄" } else { "›" }
                )))
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(0x315aae))
                        .child(format!("{}/{}", matches.len(), refs.len())),
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
                    group = group.child(
                        div()
                            .id(SharedString::from(key.clone()))
                            .flex()
                            .items_center()
                            .h(px(22.))
                            .pl(px(10. + 13. * depth as f32))
                            .min_w_0()
                            .cursor_pointer()
                            .hover(|s| s.bg(rgb(HOVER)))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(tab) = this.tab_mut(tab_id) {
                                    if !tab.refs.closed_folders.remove(&key) {
                                        tab.refs.closed_folders.insert(key.clone());
                                    }
                                    cx.notify();
                                }
                            }))
                            .child(div().text_color(rgb(MUTED)).truncate().child(format!(
                                "{}  {}",
                                if collapsed { "›" } else { "⌄" },
                                parts[depth]
                            ))),
                    );
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
                    .pl(px(10. + (parts.len() - 1) as f32 * 13.))
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
                            .text_color(rgb(if checked { GREEN } else { SUBTLE }))
                            .child(if checked {
                                "✓"
                            } else if kind == RefKind::Tag {
                                "◇"
                            } else {
                                "⑂"
                            }),
                    )
                    .child(
                        div()
                            .truncate()
                            .child(parts.last().copied().unwrap_or(&reference.name).to_owned()),
                    ),
            );
        }
        group
    }
}

fn matches_ref_filter(reference: &Reference, filter: &str) -> bool {
    reference.name.to_lowercase().contains(filter)
}
