mod menu;

pub(super) use menu::RefMenu;

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
use ritkraken_git::{RefKind, RefRequest, Reference};

use crate::i18n::{LocalizedInput, tr};

use super::{
    Client, RepoTab,
    theme::{Palette, palette},
    widgets::{Tone, bento_card, button, heading, icon},
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
        let p = palette(cx);
        let id = tab.id;
        bento_card(p)
            .w(px(42.))
            .flex_shrink_0()
            .h_full()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(4.))
            .py(px(6.))
            .child(
                button(
                    "expand-sidebar",
                    "",
                    Tone::Plain,
                    true,
                    cx,
                    cx.listener(move |this, _, _, cx| {
                        this.set_sidebar(id, false, None, cx);
                    }),
                )
                .size(px(30.))
                .px(px(0.))
                .rounded_full()
                .child(icon("icons/chevron-right.svg", p.text)),
            )
            .children(REF_GROUPS.into_iter().map(|(kind, _, title)| {
                let count = tab.active_history().snapshot.refs_of_kind(kind).count();
                div()
                    .id(title)
                    .aria_label(tr!(title))
                    .w(px(32.))
                    .h(px(42.))
                    .rounded(px(10.))
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .hover(|style| style.bg(rgb(p.hover)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.set_sidebar(id, false, Some(kind), cx)
                    }))
                    .child(icon(ref_icon(kind), p.text_muted))
                    .child(
                        div()
                            .text_size(px(10.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(p.accent))
                            .child(count.to_string()),
                    )
            }))
    }

    pub(super) fn sidebar(&self, tab: &RepoTab, cx: &mut Context<Self>) -> Div {
        let p = palette(cx);
        let filter = tab.refs.filter.read(cx).value().to_lowercase();
        let id = tab.id;
        let refs = &tab.active_history().snapshot.refs;
        let count = refs
            .iter()
            .filter(|reference| matches_ref_filter(reference, &filter))
            .count();
        bento_card(p)
            .size_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .h(px(28.))
                    .my(px(4.))
                    .px(px(9.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.))
                            .child(
                                button(
                                    "collapse-sidebar",
                                    "",
                                    Tone::Plain,
                                    true,
                                    cx,
                                    cx.listener(move |this, _, _, cx| {
                                        this.set_sidebar(id, true, None, cx);
                                    }),
                                )
                                .size(px(24.))
                                .px(px(0.))
                                .rounded_full()
                                .child(icon("icons/chevron-left.svg", p.text)),
                            )
                            .child(div().text_color(rgb(p.text)).child(tr!(
                                "app.refs.viewing",
                                count = count,
                                total = refs.len()
                            ))),
                    )
                    .child(
                        button(
                            "reset-branch-filter",
                            tr!("app.refs.show_all"),
                            Tone::Plain,
                            !filter.is_empty(),
                            cx,
                            cx.listener(move |this, _, window, cx| {
                                if let Some(tab) = this.tab_mut(id) {
                                    tab.refs
                                        .filter
                                        .update(cx, |input, cx| input.set_value("", window, cx));
                                    cx.notify();
                                }
                            }),
                        )
                        .rounded_full(),
                    ),
            )
            .child(
                div()
                    .px(px(8.))
                    .pb(px(8.))
                    .child(Input::new(&tab.refs.filter).small()),
            )
            .child(
                div().flex_1().min_h_0().child(
                    div()
                        .flex()
                        .flex_col()
                        .size_full()
                        .children(REF_GROUPS.into_iter().map(|(kind, title, _)| {
                            self.ref_group(tab, kind, title, &filter, p, cx)
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
        p: Palette,
        cx: &mut Context<Self>,
    ) -> Div {
        let snapshot = &tab.active_history().snapshot;
        let refs: Vec<_> = snapshot.refs_of_kind(kind).collect();
        let matches: Vec<_> = refs
            .iter()
            .copied()
            .filter(|reference| matches_ref_filter(reference, filter))
            .collect();
        let open = !tab.refs.closed_groups.contains(&kind);
        let tab_id = tab.id;
        let mut group = div()
            .flex()
            .flex_col()
            .pb(px(4.))
            .when(kind != RefKind::Local, |group| group.pt(px(6.)))
            .child(
                div()
                    .id(title)
                    .group("ref-group-heading")
                    .h(px(28.))
                    .px(px(9.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .hover(|s| s.bg(rgb(p.hover)))
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
                        .child(disclosure_chevron(open, p))
                        .child(sidebar_icon(ref_icon(kind), p))
                        .child(heading(tr!(title), cx).text_size(px(12.))),
                )
                .child(
                    div()
                        .relative()
                        .h(px(21.))
                        .flex()
                        .items_center()
                        .child(
                            div()
                                .px(px(7.))
                                .py(px(1.))
                                .rounded_full()
                                .bg(rgb(p.surface_dim))
                                .text_size(px(10.5))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(rgb(p.accent))
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
                                    cx,
                                    cx.listener(move |this, _, window, cx| {
                                        cx.stop_propagation();
                                        this.open_add_remote(tab_id, window, cx);
                                    }),
                                )
                                .absolute()
                                .right_0()
                                .top_0()
                                .size(px(21.))
                                .px(px(0.))
                                .rounded_full()
                                .child(icon("icons/plus.svg", p.text).size(px(14.)))
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
        let menu = RefMenu::new(tab_id, cx.weak_entity());
        for reference in matches {
            let parts: Vec<_> = reference.name.split('/').collect();
            let mut hidden = false;
            for depth in 0..parts.len().saturating_sub(1) {
                let key = format!("{title}/{}", parts[..=depth].join("/"));
                let collapsed = tab.refs.closed_folders.contains(&key) && filter.is_empty();
                if seen.insert(key.clone()) {
                    let folder = ref_folder(
                        tab_id,
                        key,
                        parts[depth].to_owned(),
                        depth,
                        kind == RefKind::Remote && depth == 0,
                        Some(!collapsed),
                        p,
                        cx,
                    );
                    group = group.child(if kind == RefKind::Remote && depth == 0 {
                        menu.remote_target(folder, &parts[0])
                    } else {
                        folder.into_any_element()
                    });
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
            let target = reference.clone();
            let row = div()
                .id(SharedString::from(format!(
                    "{title}/ref/{}",
                    reference.name
                )))
                .h(px(27.))
                .mx(px(8.))
                .pr(px(8.))
                .pl(px(6.))
                .rounded(px(8.))
                .flex()
                .items_center()
                .gap(px(6.))
                .cursor_pointer()
                .min_w_0()
                .when(checked, |e| e.bg(rgb(p.success_bg)))
                .when(!checked && selected, |e| e.bg(rgb(p.accent_soft)))
                .hover(|s| {
                    s.bg(rgb(if checked {
                        p.success_hover
                    } else if selected {
                        p.accent_soft_hover
                    } else {
                        p.hover
                    }))
                })
                .on_click(cx.listener(move |this, click: &ClickEvent, window, cx| {
                    if click.click_count() == 2
                        && matches!(kind, RefKind::Local | RefKind::Remote)
                        && !checked
                    {
                        let Some(tab) = this.tab(tab_id) else { return };
                        let request =
                            RefRequest::checkout(&tab.history.data.snapshot, target.clone());
                        this.run_ref_action(tab_id, request, window, cx);
                    } else {
                        this.select(
                            tab_id,
                            Some(target.id.clone()),
                            Some(ScrollStrategy::Center),
                            window,
                            cx,
                        );
                    }
                }))
                .child(
                    div()
                        .size(SIDEBAR_ICON_SIZE)
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .when(checked, |badge| {
                            badge
                                .bg(rgb(p.success))
                                .child(icon("icons/check.svg", p.accent_fg).size(px(11.)))
                        }),
                )
                .child(
                    sidebar_icon(
                        if kind == RefKind::Tag {
                            ref_icon(kind)
                        } else {
                            "icons/git-branch.svg"
                        },
                        p,
                    )
                    .text_color(rgb(p.text_subtle))
                    .ml(px((parts.len() - 1) as f32 * 13.)),
                )
                .child(
                    div()
                        .truncate()
                        .text_color(rgb(if checked {
                            p.success
                        } else if selected {
                            p.accent_soft_fg
                        } else {
                            p.text
                        }))
                        .font_weight(if checked {
                            FontWeight::MEDIUM
                        } else {
                            FontWeight::NORMAL
                        })
                        .child(parts.last().copied().unwrap_or(&reference.name).to_owned()),
                );
            group = group.child(menu.target(row, Some(reference), &[]));
        }
        if kind == RefKind::Remote {
            for remote in &snapshot.remotes {
                let prefix = format!("{remote}/");
                if remote.to_lowercase().contains(filter)
                    && !refs
                        .iter()
                        .any(|reference| reference.name.starts_with(&prefix))
                {
                    group = group.child(menu.remote_target(
                        ref_folder(
                            tab_id,
                            format!("{title}/{remote}"),
                            remote.clone(),
                            0,
                            true,
                            None,
                            p,
                            cx,
                        ),
                        remote,
                    ));
                }
            }
        }
        group
    }
}

#[allow(clippy::too_many_arguments)]
fn ref_folder(
    tab_id: u64,
    key: String,
    label: String,
    depth: usize,
    remote: bool,
    expanded: Option<bool>,
    p: Palette,
    cx: &mut Context<Client>,
) -> Stateful<Div> {
    div()
        .id(SharedString::from(key.clone()))
        .flex()
        .items_center()
        .gap(px(6.))
        .h(px(26.))
        .mx(px(8.))
        .rounded(px(8.))
        .pl(px(6. + 13. * depth as f32))
        .pr(px(6.))
        .min_w_0()
        .hover(|style| style.bg(rgb(p.hover)))
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
                .when_some(expanded, |slot, open| {
                    slot.child(disclosure_chevron(open, p))
                }),
        )
        .child(sidebar_icon(
            if remote {
                ref_icon(RefKind::Remote)
            } else {
                "icons/folder.svg"
            },
            p,
        ))
        .child(div().text_color(rgb(p.text_muted)).truncate().child(label))
}

fn sidebar_icon(path: &'static str, p: Palette) -> Svg {
    icon(path, p.text_muted).size(SIDEBAR_ICON_SIZE)
}

fn disclosure_chevron(open: bool, p: Palette) -> Svg {
    icon("icons/chevron-right.svg", p.text_muted)
        .size(px(12.))
        .when(open, |arrow| {
            arrow.with_transformation(Transformation::rotate(radians(std::f32::consts::FRAC_PI_2)))
        })
}

fn matches_ref_filter(reference: &Reference, filter: &str) -> bool {
    reference.name.to_lowercase().contains(filter)
}
