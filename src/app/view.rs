use std::{collections::HashSet, path::Path, sync::Arc};

use gpui_kit::{
    component::{
        Sizable,
        diff::{Diff, DiffMode},
        input::{Input, Textarea},
        resizable::{h_resizable, resizable_panel},
        scroll::ScrollableElement,
    },
    prelude::*,
};

use super::*;
use crate::graph::Rail;

const WHITE: u32 = 0xffffff;
const PAPER: u32 = 0xf6f7f8;
const HEADER: u32 = 0xf1f2f4;
const LINE: u32 = 0xd7d9de;
const TEXT: u32 = 0x252932;
const MUTED: u32 = 0x646b7c;
const SUBTLE: u32 = 0x959ba8;
const NAV: u32 = 0x363b51;
const TOOLBAR: u32 = 0x444b63;
const NAV_TEXT: u32 = 0xe0e4f0;
const SELECT: u32 = 0xdde6f8;
const HOVER: u32 = 0xeef2f9;
const TEAL: u32 = 0x1597b0;
const GREEN: u32 = 0x307542;
const GREEN_BG: u32 = 0xe5f2e5;
const RED: u32 = 0xad3e42;
const ROW: f32 = 25.;
const REF_WIDTH: f32 = 132.;

#[derive(Clone, Copy)]
enum Tone {
    Plain,
    Outline,
    Selected,
    Green,
    Dark,
}

fn button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    tone: Tone,
    enabled: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let (background, foreground, border, hover) = match tone {
        Tone::Plain => (None, TEXT, None, HOVER),
        Tone::Outline => (Some(WHITE), TEXT, Some(LINE), HOVER),
        Tone::Selected => (Some(SELECT), 0x234773, Some(0xb3c8eb), 0xd1dff6),
        Tone::Green => (Some(GREEN_BG), GREEN, Some(0xa7cba9), 0xd3ead4),
        Tone::Dark => (None, NAV_TEXT, None, 0x555f7c),
    };
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .min_w_0()
        .h(px(25.))
        .px(px(8.))
        .rounded(px(3.))
        .text_size(px(12.))
        .text_color(rgb(foreground))
        .when_some(background, |element, color| element.bg(rgb(color)))
        .when_some(border, |element, color| {
            element.border_1().border_color(rgb(color))
        })
        .child(div().truncate().child(label.into()))
        .when(enabled, |element| {
            element
                .cursor_pointer()
                .hover(|style| style.bg(rgb(hover)))
                .on_click(on_click)
        })
        .when(!enabled, |element| element.opacity(0.48))
}

fn heading(label: impl Into<SharedString>) -> Div {
    div()
        .text_size(px(10.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(MUTED))
        .child(label.into())
}

fn note(message: impl Into<SharedString>) -> Div {
    div()
        .px(px(12.))
        .py(px(10.))
        .text_size(px(12.))
        .text_color(rgb(MUTED))
        .child(message.into())
}

fn error_text(message: &str) -> Div {
    div()
        .px(px(12.))
        .py(px(8.))
        .bg(rgb(0xfcebec))
        .text_color(rgb(RED))
        .text_size(px(12.))
        .child(message.to_owned())
}

impl Render for Client {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tab = self.current();
        let body = if let Some(tab) = tab {
            self.workspace(tab, window, cx).into_any_element()
        } else {
            self.welcome(cx).into_any_element()
        };
        div()
            .id("ritkraken")
            .track_focus(&self.focus)
            .key_context("RitKraken")
            .on_action(cx.listener(|this, _: &OpenRepos, window, cx| this.open_manager(window, cx)))
            .on_action(cx.listener(|this, _: &Refresh, window, cx| {
                if this.manager.is_none() && !this.branch_dialog {
                    this.refresh(window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &Escape, window, cx| this.escape(window, cx)))
            .on_action(cx.listener(|this, _: &Submit, window, cx| this.submit(window, cx)))
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(rgb(WHITE))
            .text_color(rgb(TEXT))
            .text_size(px(12.))
            .child(self.tab_bar(cx))
            .child(self.toolbar(tab, cx))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(body),
            )
            .child(self.status_bar(tab))
            .when_some(self.manager, |element, mode| {
                element.child(self.manager_view(mode, window, cx))
            })
            .when(self.branch_dialog, |element| {
                element.child(self.branch_view(cx))
            })
    }
}

impl Client {
    fn tab_bar(&self, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .items_center()
            .flex_shrink_0()
            .h(px(34.))
            .bg(rgb(NAV))
            .text_color(rgb(NAV_TEXT))
            .child(
                div()
                    .w(px(32.))
                    .flex_shrink_0()
                    .flex()
                    .justify_center()
                    .child(
                        div()
                            .size(px(12.))
                            .rounded_full()
                            .border_2()
                            .border_color(rgb(0x74d4dd))
                            .bg(rgb(0x245962)),
                    ),
            )
            .child(button(
                "open-repos",
                "Open",
                Tone::Dark,
                true,
                cx.listener(|this, _, window, cx| {
                    this.open_manager(window, cx);
                }),
            ))
            .child(
                div().flex_1().min_w_0().h_full().child(
                    div()
                        .id("repo-tabs")
                        .flex()
                        .h_full()
                        .overflow_x_scroll()
                        .children(self.tabs.iter().map(|tab| {
                            let id = tab.id;
                            let active = self.active == Some(id);
                            div()
                                .id(("repo-tab", id))
                                .flex()
                                .items_center()
                                .h_full()
                                .w(px(156.))
                                .flex_shrink_0()
                                .gap(px(5.))
                                .pl(px(9.))
                                .pr(px(3.))
                                .border_l_1()
                                .border_color(rgb(0x50576e))
                                .bg(rgb(if active { TOOLBAR } else { NAV }))
                                .hover(|style| style.bg(rgb(TOOLBAR)))
                                .cursor_pointer()
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.show_tab(id, window, cx)
                                }))
                                .child(div().text_color(rgb(0x93a4c8)).child("◈"))
                                .child(div().flex_1().min_w_0().truncate().child(tab.repo.name()))
                                .child(button(
                                    ("close-tab", id),
                                    "×",
                                    Tone::Dark,
                                    !tab.busy,
                                    cx.listener(move |this, _, window, cx| {
                                        cx.stop_propagation();
                                        this.close_tab(id, window, cx);
                                    }),
                                ))
                        }))
                        .child(button(
                            "new-tab",
                            "+",
                            Tone::Dark,
                            true,
                            cx.listener(|this, _, window, cx| {
                                this.open_manager(window, cx);
                            }),
                        )),
                ),
            )
            .child(
                div()
                    .px(px(12.))
                    .text_color(rgb(0xaab5d2))
                    .child("RitKraken"),
            )
    }

    fn toolbar(&self, tab: Option<&RepoTab>, cx: &mut Context<Self>) -> Div {
        let available = tab.is_some_and(|tab| !tab.busy);
        let (repo, branch) = tab.map_or_else(
            || ("Repositories".to_owned(), String::new()),
            |tab| (tab.repo.name(), tab.history.snapshot.branch.clone()),
        );
        let control = |label: &'static str, icon: &'static str| {
            div()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(1.))
                .child(
                    div()
                        .text_size(px(10.))
                        .text_color(rgb(0xc5cde0))
                        .child(label),
                )
                .child(div().text_size(px(16.)).child(icon))
        };
        div()
            .h(px(43.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_between()
            .px(px(10.))
            .bg(rgb(TOOLBAR))
            .text_color(rgb(NAV_TEXT))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .min_w_0()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .min_w_0()
                            .child(heading("REPOSITORY").text_color(rgb(0xb3bdd7)))
                            .child(
                                div()
                                    .max_w(px(200.))
                                    .truncate()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(repo),
                            ),
                    )
                    .when(!branch.is_empty(), |row| {
                        row.child(
                            div()
                                .text_size(px(18.))
                                .text_color(rgb(0x9da8c3))
                                .child("›"),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .min_w_0()
                                .child(heading("BRANCH").text_color(rgb(0xb3bdd7)))
                                .child(div().max_w(px(270.)).truncate().child(branch)),
                        )
                    }),
            )
            .when(tab.is_some(), |toolbar| {
                toolbar.child(
                    div()
                        .flex()
                        .h_full()
                        .items_center()
                        .gap(px(4.))
                        .child(
                            div()
                                .id("toolbar-refresh")
                                .w(px(63.))
                                .h_full()
                                .flex()
                                .justify_center()
                                .when(available, |e| {
                                    e.cursor_pointer().hover(|s| s.bg(rgb(0x555f7c))).on_click(
                                        cx.listener(|this, _, window, cx| this.refresh(window, cx)),
                                    )
                                })
                                .when(!available, |e| e.opacity(0.45))
                                .child(control("Refresh", "↻")),
                        )
                        .child(
                            div()
                                .id("toolbar-branch")
                                .w(px(63.))
                                .h_full()
                                .flex()
                                .justify_center()
                                .when(available, |e| {
                                    e.cursor_pointer().hover(|s| s.bg(rgb(0x555f7c))).on_click(
                                        cx.listener(|this, _, window, cx| {
                                            this.branch_dialog = true;
                                            this.branch_name.update(cx, |input, cx| {
                                                input.set_value("", window, cx);
                                                input.focus_handle(cx).focus(window, cx);
                                            });
                                            cx.notify();
                                        }),
                                    )
                                })
                                .when(!available, |e| e.opacity(0.45))
                                .child(control("Branch", "⑂")),
                        )
                        .child(
                            div()
                                .id("toolbar-wip")
                                .w(px(63.))
                                .h_full()
                                .flex()
                                .justify_center()
                                .cursor_pointer()
                                .hover(|s| s.bg(rgb(0x555f7c)))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    if let Some(id) = this.active {
                                        this.select(
                                            id,
                                            None,
                                            Some(ScrollStrategy::Top),
                                            window,
                                            cx,
                                        );
                                    }
                                }))
                                .child(control("Work in progress", "◌")),
                        ),
                )
            })
            .child(
                div()
                    .min_w(px(58.))
                    .text_right()
                    .text_color(rgb(0xaeb8d3))
                    .child("Local Git"),
            )
    }

    fn status_bar(&self, tab: Option<&RepoTab>) -> Div {
        let (color, message) = if self.opening {
            (MUTED, "Opening repository…".to_owned())
        } else if let Some(tab) = tab {
            if tab.busy {
                (MUTED, "Running Git…".to_owned())
            } else if let Some((error, message)) = &tab.notice {
                (
                    if *error { RED } else { GREEN },
                    message.lines().next().unwrap_or(message).to_owned(),
                )
            } else {
                (MUTED, tab.repo.root.to_string_lossy().into_owned())
            }
        } else {
            (MUTED, "Open a repository to begin".to_owned())
        };
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(10.))
            .h(px(24.))
            .flex_shrink_0()
            .px(px(10.))
            .bg(rgb(PAPER))
            .border_t_1()
            .border_color(rgb(LINE))
            .text_size(px(10.))
            .child(
                div()
                    .flex_1()
                    .truncate()
                    .text_color(rgb(color))
                    .child(message),
            )
            .when_some(tab, |element, tab| {
                let snapshot = &tab.history.snapshot;
                element.child(div().text_color(rgb(MUTED)).child(format!(
                    "{}{} commits",
                    snapshot.commits.len(),
                    if snapshot.history_limited { "+" } else { "" }
                )))
            })
            .child(
                div()
                    .text_color(rgb(MUTED))
                    .child(env!("CARGO_PKG_VERSION")),
            )
    }

    fn workspace(
        &self,
        tab: &RepoTab,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let width = window.viewport_size().width;
        let right = (width * 0.30).clamp(px(296.), px(570.));
        let left = (width * 0.175).clamp(px(168.), px(320.));
        let center = if let Some(diff) = &tab.diff {
            self.diff_view(tab, diff, cx).into_any_element()
        } else {
            self.graph_view(tab, cx).into_any_element()
        };
        h_resizable("workspace-panes")
            .child(
                resizable_panel()
                    .size(left)
                    .size_range(px(140.)..px(490.))
                    .child(self.sidebar(tab, cx)),
            )
            .child(
                resizable_panel()
                    .size_range(px(280.)..px(4000.))
                    .child(center),
            )
            .child(
                resizable_panel()
                    .size(right)
                    .size_range(px(272.)..px(1000.))
                    .child(self.right_panel(tab, cx)),
            )
    }

    fn sidebar(&self, tab: &RepoTab, cx: &mut Context<Self>) -> Div {
        let filter = tab.filter.read(cx).value().to_lowercase();
        let id = tab.id;
        let refs = &tab.history.snapshot.refs;
        let count = refs
            .iter()
            .filter(|reference| reference.name.to_lowercase().contains(&filter))
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
                    .child(div().child(format!("Viewing {count}/{}", refs.len())))
                    .child(button(
                        "reset-branch-filter",
                        "Show All",
                        Tone::Plain,
                        !filter.is_empty(),
                        cx.listener(move |this, _, window, cx| {
                            if let Some(tab) = this.tab_mut(id) {
                                tab.filter
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
                    .child(Input::new(&tab.filter).small()),
            )
            .child(
                div().flex_1().min_h_0().child(
                    div()
                        .flex()
                        .flex_col()
                        .size_full()
                        .children(
                            [
                                (RefKind::Local, "LOCAL"),
                                (RefKind::Remote, "REMOTE"),
                                (RefKind::Tag, "TAGS"),
                            ]
                            .into_iter()
                            .map(|(kind, title)| self.ref_group(tab, kind, title, &filter, cx)),
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
        let refs: Vec<_> = tab
            .history
            .snapshot
            .refs
            .iter()
            .filter(|reference| reference.kind == kind)
            .collect();
        let matches: Vec<_> = refs
            .iter()
            .copied()
            .filter(|reference| reference.name.to_lowercase().contains(filter))
            .collect();
        let open = !tab.closed_groups.contains(&kind);
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
                        if !tab.closed_groups.remove(&kind) {
                            tab.closed_groups.insert(kind);
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
                let collapsed = tab.closed_folders.contains(&key) && filter.is_empty();
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
                                    if !tab.closed_folders.remove(&key) {
                                        tab.closed_folders.insert(key.clone());
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
            let checked = kind == RefKind::Local && reference.name == tab.history.snapshot.branch;
            let selected = tab.selected.as_ref() == Some(&reference.id);
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
                                Some(Edit::Switch(branch.clone())),
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

    fn graph_view(&self, tab: &RepoTab, cx: &mut Context<Self>) -> Stateful<Div> {
        let id = tab.id;
        let history = tab.history.clone();
        let selected = tab.selected.clone();
        let show_wip = tab.show_wip();
        let graph_width = (history.width as f32 * 16. + 24.).clamp(64., 172.);
        let count = history.rows.len() + usize::from(show_wip);
        let view = cx.weak_entity();
        let focus = tab.graph_focus.clone();
        let list = uniform_list(("history", id), count, move |range, _, _| {
            range
                .map(|row| {
                    let selection = row
                        .checked_sub(usize::from(show_wip))
                        .map(|index| history.snapshot.commits[index].id.clone());
                    let element = history_row(
                        &history,
                        selection.as_deref(),
                        row,
                        show_wip,
                        graph_width,
                        selected == selection,
                    );
                    let view = view.clone();
                    let focus = focus.clone();
                    element.on_click(move |_, window, cx| {
                        let _ = view.update(cx, |this, cx| {
                            focus.focus(window, cx);
                            this.select(id, selection.clone(), None, window, cx);
                        });
                    })
                })
                .collect::<Vec<_>>()
        })
        .track_scroll(&tab.scroll)
        .size_full();
        div()
            .id(("graph-view", id))
            .key_context("CommitGraph")
            .track_focus(&tab.graph_focus)
            .on_action(
                cx.listener(|this, _: &NextRow, window, cx| this.move_selection(1, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &PreviousRow, window, cx| {
                    this.move_selection(-1, window, cx)
                }),
            )
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(WHITE))
            .child(
                div()
                    .flex()
                    .items_center()
                    .h(px(22.))
                    .flex_shrink_0()
                    .bg(rgb(HEADER))
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .child(column("BRANCH / TAG", REF_WIDTH))
                    .child(column("GRAPH", graph_width).flex_shrink_0())
                    .child(
                        heading("COMMIT MESSAGE")
                            .min_w(px(95.))
                            .flex_1()
                            .pl(px(7.))
                            .truncate(),
                    )
                    .child(column("AUTHOR", 111.))
                    .child(column("DATE", 84.))
                    .child(column("SHA", 62.)),
            )
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(list)
                    .vertical_scrollbar(&tab.scroll),
            )
    }

    fn right_panel(&self, tab: &RepoTab, cx: &mut Context<Self>) -> Div {
        let panel = if tab.selected.is_none() {
            self.wip_panel(tab, cx)
        } else {
            self.commit_panel(tab, cx)
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .border_l_1()
            .border_color(rgb(LINE))
            .bg(rgb(PAPER))
            .when_some(
                tab.notice.as_ref().filter(|(error, _)| *error),
                |element, (_, message)| {
                    element.child(
                        div()
                            .max_h(px(82.))
                            .child(error_text(message))
                            .overflow_y_scrollbar(),
                    )
                },
            )
            .child(panel)
    }

    fn wip_panel(&self, tab: &RepoTab, cx: &mut Context<Self>) -> Div {
        let snapshot = &tab.history.snapshot;
        let status = &snapshot.status;
        let id = tab.id;
        let enabled = tab.commit_enabled(cx);
        let label = if status.staged.is_empty() {
            "Stage files/changes to commit".into()
        } else {
            format!(
                "Commit changes to {} file{}",
                status.staged.len(),
                if status.staged.len() == 1 { "" } else { "s" }
            )
        };
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap(px(5.))
                    .h(px(31.))
                    .px(px(6.))
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .child(format!("{} file changes on", change_count(tab)))
                    .child(
                        div()
                            .px(px(4.))
                            .min_w_0()
                            .truncate()
                            .bg(rgb(0xbbe4ea))
                            .child(snapshot.branch.clone()),
                    ),
            )
            .child(self.file_mode(tab, cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .child(self.file_section(
                        tab,
                        "Unstaged Files",
                        &status.unstaged,
                        Scope::Worktree,
                        cx,
                    ))
                    .child(self.file_section(
                        tab,
                        "Staged Files",
                        &status.staged,
                        Scope::Index,
                        cx,
                    )),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .p(px(9.))
                    .border_t_1()
                    .border_color(rgb(LINE))
                    .child(
                        div()
                            .font_weight(FontWeight::MEDIUM)
                            .child("Commit Message"),
                    )
                    .child(Input::new(&tab.summary).small())
                    .child(Textarea::new(&tab.description).small().h(px(63.)))
                    .child(
                        button(
                            ("commit", id),
                            label,
                            Tone::Green,
                            enabled,
                            cx.listener(|this, _, window, cx| this.submit(window, cx)),
                        )
                        .h(px(34.)),
                    ),
            )
    }

    fn file_mode(&self, tab: &RepoTab, cx: &mut Context<Self>) -> Div {
        let id = tab.id;
        div()
            .h(px(34.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .child(button(
                ("path-mode", id),
                "☰  Path",
                if tab.file_tree {
                    Tone::Outline
                } else {
                    Tone::Selected
                },
                true,
                cx.listener(move |this, _, _, cx| {
                    if let Some(tab) = this.tab_mut(id) {
                        tab.file_tree = false;
                        cx.notify();
                    }
                }),
            ))
            .child(button(
                ("tree-mode", id),
                "▦  Tree",
                if tab.file_tree {
                    Tone::Selected
                } else {
                    Tone::Outline
                },
                true,
                cx.listener(move |this, _, _, cx| {
                    if let Some(tab) = this.tab_mut(id) {
                        tab.file_tree = true;
                        cx.notify();
                    }
                }),
            ))
    }

    fn file_section(
        &self,
        tab: &RepoTab,
        title: &'static str,
        files: &[Change],
        scope: Scope,
        cx: &mut Context<Self>,
    ) -> Div {
        let id = tab.id;
        let is_staged = scope == Scope::Index;
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .h(px(29.))
                    .flex_shrink_0()
                    .px(px(8.))
                    .border_t_1()
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .child(
                        div()
                            .font_weight(FontWeight::MEDIUM)
                            .child(format!("{title} ({})", files.len())),
                    )
                    .when(!files.is_empty(), |e| {
                        e.child(
                            button(
                                title,
                                if is_staged {
                                    "Unstage all changes"
                                } else {
                                    "Stage all changes"
                                },
                                Tone::Green,
                                !tab.busy,
                                cx.listener(move |this, _, window, cx| {
                                    let edit = if is_staged {
                                        Edit::Unstage(None)
                                    } else {
                                        Edit::Stage(None)
                                    };
                                    this.update_repo(id, Some(edit), window, cx);
                                }),
                            )
                            .h(px(21.)),
                        )
                    }),
            )
            .child(self.file_list(tab, files, scope, cx))
    }

    fn file_list(
        &self,
        tab: &RepoTab,
        files: &[Change],
        scope: Scope,
        cx: &mut Context<Self>,
    ) -> Div {
        let mut rows: Vec<AnyElement> = Vec::with_capacity(files.len());
        let mut folders = HashSet::new();
        let scope_label = match &scope {
            Scope::Worktree => "working",
            Scope::Index => "index",
            Scope::Commit { .. } => "commit",
        };
        let mut ordered: Vec<_> = files.iter().collect();
        if tab.file_tree {
            ordered.sort_by(|a, b| a.path.cmp(&b.path));
        }
        for (index, change) in ordered.into_iter().enumerate() {
            let components: Vec<_> = change.path.components().collect();
            let mut depth = 0;
            if tab.file_tree {
                let mut folder = PathBuf::new();
                for component in components.iter().take(components.len().saturating_sub(1)) {
                    folder.push(component.as_os_str());
                    if folders.insert(folder.clone()) {
                        rows.push(
                            div()
                                .h(px(23.))
                                .flex()
                                .items_center()
                                .pl(px(8. + depth as f32 * 13.))
                                .text_color(rgb(MUTED))
                                .truncate()
                                .child(format!("⌄  {}", component.as_os_str().to_string_lossy()))
                                .into_any_element(),
                        );
                    }
                    depth += 1;
                }
            }
            let label = if tab.file_tree {
                change
                    .path
                    .file_name()
                    .unwrap_or(change.path.as_os_str())
                    .to_string_lossy()
                    .into_owned()
            } else if let Some(previous) = &change.previous {
                format!("{} → {}", previous.display(), change.path.display())
            } else {
                change.path.to_string_lossy().into_owned()
            };
            let file = FileTarget {
                scope: scope.clone(),
                change: change.clone(),
            };
            let active = tab.diff.as_ref().is_some_and(|diff| diff.file == file);
            let (mark, color) = change_mark(change.code);
            let id = tab.id;
            let change = change.clone();
            let is_staged = scope == Scope::Index;
            let row = div()
                .id(SharedString::from(format!("{scope_label}-file-{index}")))
                .h(px(26.))
                .flex_shrink_0()
                .flex()
                .items_center()
                .gap(px(6.))
                .pl(px(8. + depth as f32 * 13.))
                .pr(px(5.))
                .cursor_pointer()
                .bg(rgb(if active { SELECT } else { PAPER }))
                .hover(|s| s.bg(rgb(HOVER)))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.open_diff(id, file.clone(), window, cx)
                }))
                .child(div().text_color(rgb(color)).flex_shrink_0().child(mark))
                .child(div().flex_1().min_w_0().truncate().child(label))
                .when(!matches!(scope, Scope::Commit { .. }), |e| {
                    e.child(
                        button(
                            ("file-stage", index),
                            if is_staged { "Unstage" } else { "Stage" },
                            Tone::Plain,
                            !tab.busy,
                            cx.listener(move |this, _, window, cx| {
                                cx.stop_propagation();
                                let edit = if is_staged {
                                    Edit::Unstage(Some(change.clone()))
                                } else {
                                    Edit::Stage(Some(change.clone()))
                                };
                                this.update_repo(id, Some(edit), window, cx);
                            }),
                        )
                        .h(px(21.))
                        .text_size(px(11.))
                        .text_color(rgb(GREEN)),
                    )
                });
            rows.push(row.into_any_element());
        }
        div().flex_1().min_h_0().child(
            div()
                .size_full()
                .flex()
                .flex_col()
                .children(rows)
                .when(files.is_empty(), |e| e.child(note("No files")))
                .overflow_y_scrollbar(),
        )
    }

    fn commit_panel(&self, tab: &RepoTab, cx: &mut Context<Self>) -> Div {
        let id = tab.id;
        let oid = tab.selected.as_deref().unwrap_or_default();
        let Some(commit) = tab
            .history
            .position
            .get(oid)
            .and_then(|index| tab.history.snapshot.commits.get(*index))
        else {
            return div().child(note("This commit is outside the loaded history."));
        };
        let detail = match &tab.detail {
            Load::Pending => note("Loading commit…"),
            Load::Failed(error) => error_text(error),
            Load::Ready(detail) => {
                let scope = Scope::Commit {
                    id: oid.to_owned(),
                    parent: commit.parents.first().cloned(),
                };
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .m(px(8.))
                            .max_h(px(163.))
                            .rounded(px(3.))
                            .border_1()
                            .border_color(rgb(LINE))
                            .bg(rgb(WHITE))
                            .child(
                                div()
                                    .p(px(10.))
                                    .text_size(px(14.))
                                    .child(detail.message.clone()),
                            )
                            .overflow_y_scrollbar(),
                    )
                    .child(
                        div()
                            .px(px(10.))
                            .pb(px(7.))
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(px(4.))
                            .child(
                                div()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .child(
                                        div()
                                            .truncate()
                                            .font_weight(FontWeight::MEDIUM)
                                            .child(commit.author.clone()),
                                    )
                                    .child(
                                        div()
                                            .text_color(rgb(MUTED))
                                            .text_size(px(11.))
                                            .child(format!("authored {}", commit.date)),
                                    ),
                            )
                            .when_some(commit.parents.first().cloned(), |e, parent| {
                                e.child(button(
                                    ("parent", id),
                                    format!("parent: {}", git::short_id(&parent)),
                                    Tone::Plain,
                                    tab.history.position.contains_key(&parent),
                                    cx.listener(move |this, _, window, cx| {
                                        this.select(
                                            id,
                                            Some(parent.clone()),
                                            Some(ScrollStrategy::Center),
                                            window,
                                            cx,
                                        );
                                    }),
                                ))
                            }),
                    )
                    .child(
                        div()
                            .px(px(10.))
                            .pb(px(2.))
                            .text_color(rgb(MUTED))
                            .child(format!("{} changed files", detail.changes.len())),
                    )
                    .child(self.file_mode(tab, cx))
                    .child(self.file_list(tab, &detail.changes, scope, cx))
            }
        };
        let clipboard_oid = oid.to_owned();
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .when(tab.has_changes(), |e| {
                e.child(
                    div()
                        .h(px(34.))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .gap(px(6.))
                        .px(px(5.))
                        .bg(rgb(0xaac4f7))
                        .child(div().truncate().child(format!(
                            "{} file changes in working directory",
                            change_count(tab)
                        )))
                        .child(
                            button(
                                ("view-changes", id),
                                "View changes",
                                Tone::Outline,
                                true,
                                cx.listener(move |this, _, window, cx| {
                                    this.select(id, None, Some(ScrollStrategy::Top), window, cx);
                                }),
                            )
                            .h(px(21.)),
                        ),
                )
            })
            .child(
                div()
                    .h(px(31.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap(px(6.))
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .child(format!("commit: {}", git::short_id(oid)))
                    .child(
                        button(
                            ("copy-sha", id),
                            "Copy SHA",
                            Tone::Plain,
                            true,
                            move |_, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(
                                    clipboard_oid.clone(),
                                ));
                            },
                        )
                        .text_size(px(10.)),
                    ),
            )
            .child(detail)
    }

    fn diff_view(&self, tab: &RepoTab, diff: &DiffView, cx: &mut Context<Self>) -> Div {
        let id = tab.id;
        let file = diff.file.clone();
        let scope = match file.scope {
            Scope::Worktree => "Working directory",
            Scope::Index => "Staged",
            Scope::Commit { .. } => "Commit",
        };
        let viewer = if let Some(viewer) = &diff.viewer {
            Diff::new(viewer)
                .size_full()
                .soft_wrap(false)
                .into_any_element()
        } else {
            match &diff.patch {
                Load::Pending => note("Loading diff…").into_any_element(),
                Load::Failed(error) => error_text(error).into_any_element(),
                Load::Ready(_) => note(
                    diff.message
                        .clone()
                        .unwrap_or_else(|| "No textual changes.".into()),
                )
                .into_any_element(),
            }
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(5.))
                    .px(px(7.))
                    .h(px(34.))
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .bg(rgb(HEADER))
                    .child(button(
                        ("close-diff", id),
                        "‹  Graph",
                        Tone::Outline,
                        true,
                        cx.listener(|this, _, window, cx| this.close_diff(window, cx)),
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .font_weight(FontWeight::MEDIUM)
                            .child(file.change.path.to_string_lossy().into_owned()),
                    )
                    .child(div().text_size(px(10.)).text_color(rgb(MUTED)).child(scope))
                    .children(
                        [(DiffMode::Unified, "Inline"), (DiffMode::Split, "Split")]
                            .into_iter()
                            .map(|(mode, label)| {
                                button(
                                    label,
                                    label,
                                    if tab.diff_mode == mode {
                                        Tone::Selected
                                    } else {
                                        Tone::Outline
                                    },
                                    diff.viewer.is_some(),
                                    cx.listener(move |this, _, _, cx| {
                                        this.set_diff_mode(id, mode, cx)
                                    }),
                                )
                            }),
                    ),
            )
            .when(!diff.hunks.is_empty(), |element| {
                let Load::Ready(patch) = &diff.patch else {
                    return element;
                };
                let action = if file.scope == Scope::Index {
                    "Unstage hunk"
                } else {
                    "Stage hunk"
                };
                element.child(
                    div()
                        .flex_shrink_0()
                        .h(px(34.))
                        .min_w_0()
                        .border_b_1()
                        .border_color(rgb(LINE))
                        .bg(rgb(PAPER))
                        .child(
                            div()
                                .id(("hunks", id))
                                .size_full()
                                .flex()
                                .items_center()
                                .gap(px(5.))
                                .px(px(7.))
                                .overflow_x_scroll()
                                .children(diff.hunks.iter().enumerate().map(|(index, header)| {
                                    let patch = patch.clone();
                                    let file = file.clone();
                                    let position =
                                        header.split("@@").nth(1).unwrap_or(header).trim();
                                    button(
                                        ("hunk-action", index),
                                        format!("{action} {}  ({position})", index + 1),
                                        Tone::Green,
                                        !tab.busy,
                                        cx.listener(move |this, _, window, cx| {
                                            this.update_repo(
                                                id,
                                                Some(Edit::Hunk(
                                                    file.clone(),
                                                    patch.clone(),
                                                    index,
                                                )),
                                                window,
                                                cx,
                                            );
                                        }),
                                    )
                                    .flex_shrink_0()
                                    .h(px(23.))
                                })),
                        ),
                )
            })
            .child(div().flex_1().min_h_0().overflow_hidden().child(viewer))
    }

    fn welcome(&self, cx: &mut Context<Self>) -> Div {
        div()
            .size_full()
            .flex()
            .justify_center()
            .bg(rgb(PAPER))
            .child(
                div()
                    .w(px(660.))
                    .max_w_full()
                    .mt(px(95.))
                    .px(px(20.))
                    .flex()
                    .flex_col()
                    .gap(px(9.))
                    .child(
                        div()
                            .text_size(px(26.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Repositories"),
                    )
                    .child(div().h(px(3.)).w(px(110.)).bg(rgb(TEAL)))
                    .child(
                        div()
                            .text_color(rgb(MUTED))
                            .child("Open, clone, or initialize a local Git repository."),
                    )
                    .child(
                        button(
                            "welcome-open",
                            "Open Repository Management",
                            Tone::Outline,
                            true,
                            cx.listener(|this, _, window, cx| this.open_manager(window, cx)),
                        )
                        .w(px(212.))
                        .h(px(32.)),
                    )
                    .when(!self.saved.recent.is_empty(), |element| {
                        element
                            .child(heading("RECENT REPOSITORIES").mt(px(12.)))
                            .children(self.saved.recent.iter().take(5).enumerate().map(
                                |(index, path)| {
                                    let root = path.clone();
                                    button(
                                        ("welcome-recent", index),
                                        repo_name(path),
                                        Tone::Plain,
                                        true,
                                        cx.listener(move |this, _, window, cx| {
                                            this.open_repo(
                                                OpenRepo::Open(root.clone()),
                                                window,
                                                cx,
                                            );
                                        }),
                                    )
                                    .justify_start()
                                },
                            ))
                    }),
            )
    }

    fn manager_view(&self, mode: ManagerMode, window: &Window, cx: &mut Context<Self>) -> Div {
        let dimensions = window.viewport_size();
        let width = (dimensions.width - px(44.)).min(px(900.));
        let height = (dimensions.height - px(62.)).min(px(650.));
        let open = mode == ManagerMode::Open;
        div()
            .absolute()
            .inset_0()
            .occlude()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x20253655))
            .child(
                div()
                    .w(width)
                    .h(height)
                    .flex()
                    .flex_col()
                    .bg(rgb(WHITE))
                    .rounded(px(4.))
                    .shadow_lg()
                    .border_1()
                    .border_color(rgb(0xaeb4bf))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .h(px(39.))
                            .flex_shrink_0()
                            .px(px(13.))
                            .bg(rgb(HEADER))
                            .border_b_1()
                            .border_color(rgb(LINE))
                            .child(
                                div()
                                    .text_size(px(16.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .child("Repository Management"),
                            )
                            .child(button(
                                "close-manager",
                                "×",
                                Tone::Plain,
                                !self.opening,
                                cx.listener(|this, _, window, cx| this.escape(window, cx)),
                            )),
                    )
                    .when_some(self.manager_error.as_deref(), |element, error| {
                        element.child(error_text(error))
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .flex()
                            .child(
                                div()
                                    .w(px(157.))
                                    .flex_shrink_0()
                                    .flex()
                                    .flex_col()
                                    .bg(rgb(HEADER))
                                    .border_r_1()
                                    .border_color(rgb(LINE))
                                    .children(
                                        [
                                            (ManagerMode::Open, "Open"),
                                            (ManagerMode::Clone, "Clone"),
                                            (ManagerMode::Init, "Init"),
                                        ]
                                        .into_iter()
                                        .map(
                                            |(target, title)| {
                                                button(
                                                    title,
                                                    title,
                                                    if mode == target {
                                                        Tone::Selected
                                                    } else {
                                                        Tone::Plain
                                                    },
                                                    !self.opening,
                                                    cx.listener(move |this, _, _, cx| {
                                                        this.manager = Some(target);
                                                        this.manager_error = None;
                                                        cx.notify();
                                                    }),
                                                )
                                                .w_full()
                                                .justify_start()
                                                .h(px(47.))
                                                .pl(px(15.))
                                                .rounded(px(0.))
                                                .text_size(px(18.))
                                            },
                                        ),
                                    ),
                            )
                            .when(open, |body| {
                                body.child(self.open_sources(cx)).child(self.open_list(cx))
                            })
                            .when(!open, |body| body.child(self.repository_form(mode, cx))),
                    ),
            )
    }

    fn open_sources(&self, cx: &mut Context<Self>) -> Div {
        div()
            .w(px(216.))
            .flex_shrink_0()
            .flex()
            .flex_col()
            .min_h_0()
            .bg(rgb(PAPER))
            .border_r_1()
            .border_color(rgb(LINE))
            .child(
                button(
                    "browse-repo",
                    "▱  Open a Repository",
                    Tone::Plain,
                    !self.opening,
                    cx.listener(|this, _, window, cx| this.browse(false, window, cx)),
                )
                .h(px(34.))
                .justify_start(),
            )
            .children(
                [
                    (RepoList::Recent, "◷  Recently Opened"),
                    (RepoList::Favorites, "☆  Favorites"),
                ]
                .into_iter()
                .map(|(list, label)| {
                    button(
                        label,
                        label,
                        if self.repo_list == list {
                            Tone::Selected
                        } else {
                            Tone::Plain
                        },
                        !self.opening,
                        cx.listener(move |this, _, window, cx| {
                            this.change_repo_list(list.clone(), window, cx)
                        }),
                    )
                    .h(px(34.))
                    .justify_start()
                    .rounded(px(0.))
                }),
            )
            .child(
                div()
                    .mt(px(6.))
                    .px(px(8.))
                    .h(px(31.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_t_1()
                    .border_color(rgb(LINE))
                    .child(div().child("Project Directories"))
                    .child(
                        button(
                            "add-project",
                            "+",
                            Tone::Green,
                            !self.opening,
                            cx.listener(|this, _, window, cx| this.browse(true, window, cx)),
                        )
                        .h(px(21.)),
                    ),
            )
            .child(
                div().flex_1().min_h_0().child(
                    div()
                        .size_full()
                        .flex()
                        .flex_col()
                        .children(self.saved.projects.iter().enumerate().map(|(index, path)| {
                            let list = RepoList::Project(path.clone());
                            button(
                                ("project", index),
                                repo_name(path),
                                if self.repo_list == list {
                                    Tone::Selected
                                } else {
                                    Tone::Plain
                                },
                                !self.opening,
                                cx.listener(move |this, _, window, cx| {
                                    this.change_repo_list(list.clone(), window, cx)
                                }),
                            )
                            .h(px(29.))
                            .justify_start()
                        }))
                        .overflow_y_scrollbar(),
                ),
            )
    }

    fn open_list(&self, cx: &mut Context<Self>) -> Div {
        let (label, repositories) = match &self.repo_list {
            RepoList::Recent => ("Recently Opened".to_owned(), &self.saved.recent),
            RepoList::Favorites => ("Favorites".to_owned(), &self.saved.favorites),
            RepoList::Project(path) => (repo_name(path), &self.project_repos),
        };
        let filter = self.search.read(cx).value().to_lowercase();
        div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(7.))
                    .px(px(9.))
                    .h(px(40.))
                    .border_b_1()
                    .border_color(rgb(LINE))
                    .child(div().truncate().text_size(px(16.)).child(label))
                    .child(div().w(px(190.)).child(Input::new(&self.search).small())),
            )
            .child(
                div().flex_1().min_h_0().child(
                    div()
                        .size_full()
                        .flex()
                        .flex_col()
                        .px(px(8.))
                        .when(repositories.is_empty(), |e| {
                            e.child(note("No repositories in this list yet."))
                        })
                        .children(
                            repositories
                                .iter()
                                .enumerate()
                                .filter(|(_, path)| {
                                    repo_name(path).to_lowercase().contains(&filter)
                                })
                                .map(|(index, path)| {
                                    let root = path.clone();
                                    let favorite = self.saved.favorites.contains(path);
                                    let star_path = path.clone();
                                    div()
                                        .id(("open-recent", index))
                                        .h(px(47.))
                                        .flex_shrink_0()
                                        .flex()
                                        .items_center()
                                        .px(px(8.))
                                        .gap(px(8.))
                                        .cursor_pointer()
                                        .hover(|s| s.bg(rgb(HOVER)))
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.open_repo(
                                                OpenRepo::Open(root.clone()),
                                                window,
                                                cx,
                                            );
                                        }))
                                        .child(
                                            div()
                                                .flex_1()
                                                .min_w_0()
                                                .flex()
                                                .flex_col()
                                                .gap(px(2.))
                                                .child(
                                                    div()
                                                        .truncate()
                                                        .font_weight(FontWeight::MEDIUM)
                                                        .child(repo_name(path)),
                                                )
                                                .child(
                                                    div()
                                                        .truncate()
                                                        .text_size(px(11.))
                                                        .text_color(rgb(MUTED))
                                                        .child(path.to_string_lossy().into_owned()),
                                                ),
                                        )
                                        .child(button(
                                            ("favorite", index),
                                            if favorite { "★" } else { "☆" },
                                            Tone::Plain,
                                            true,
                                            cx.listener(move |this, _, _, cx| {
                                                cx.stop_propagation();
                                                this.toggle_favorite(star_path.clone(), cx);
                                            }),
                                        ))
                                }),
                        )
                        .overflow_y_scrollbar(),
                ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(5.))
                    .p(px(9.))
                    .border_t_1()
                    .border_color(rgb(LINE))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(Input::new(&self.path).small()),
                    )
                    .child(button(
                        "open-path",
                        "Open",
                        Tone::Green,
                        !self.opening,
                        cx.listener(|this, _, window, cx| this.open_from_input(window, cx)),
                    )),
            )
    }

    fn repository_form(&self, mode: ManagerMode, cx: &mut Context<Self>) -> Div {
        let clone = mode == ManagerMode::Clone;
        div()
            .flex_1()
            .min_w_0()
            .p(px(20.))
            .flex()
            .flex_col()
            .gap(px(9.))
            .child(div().text_size(px(18.)).child(if clone {
                "Clone a Repository"
            } else {
                "Initialize a Repository"
            }))
            .when(clone, |e| {
                e.child(heading("REPOSITORY URL"))
                    .child(Input::new(&self.url))
            })
            .child(heading("DESTINATION DIRECTORY"))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(7.))
                    .child(div().flex_1().min_w_0().child(Input::new(&self.path)))
                    .child(button(
                        "browse-destination",
                        "Browse…",
                        Tone::Outline,
                        !self.opening,
                        cx.listener(|this, _, window, cx| this.browse(false, window, cx)),
                    )),
            )
            .child(
                button(
                    "repository-submit",
                    if clone { "Clone" } else { "Initialize" },
                    Tone::Green,
                    !self.opening,
                    cx.listener(|this, _, window, cx| this.open_from_input(window, cx)),
                )
                .w(px(130.))
                .h(px(31.)),
            )
    }

    fn branch_view(&self, cx: &mut Context<Self>) -> Div {
        div()
            .absolute()
            .inset_0()
            .occlude()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x20253655))
            .child(
                div()
                    .w(px(390.))
                    .p(px(15.))
                    .rounded(px(4.))
                    .shadow_lg()
                    .bg(rgb(WHITE))
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(
                        div()
                            .text_size(px(16.))
                            .child("Create and check out a branch"),
                    )
                    .child(Input::new(&self.branch_name))
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(6.))
                            .child(button(
                                "cancel-branch",
                                "Cancel",
                                Tone::Outline,
                                true,
                                cx.listener(|this, _, window, cx| this.escape(window, cx)),
                            ))
                            .child(button(
                                "create-branch",
                                "Create Branch",
                                Tone::Green,
                                true,
                                cx.listener(|this, _, window, cx| {
                                    let branch =
                                        this.branch_name.read(cx).value().trim().to_owned();
                                    if let Some(id) = this.active.filter(|_| !branch.is_empty()) {
                                        this.branch_dialog = false;
                                        this.focus.focus(window, cx);
                                        this.update_repo(
                                            id,
                                            Some(Edit::Branch(branch)),
                                            window,
                                            cx,
                                        );
                                    }
                                }),
                            )),
                    ),
            )
    }
}

fn column(label: &'static str, width: f32) -> Div {
    heading(label)
        .w(px(width))
        .min_w_0()
        .truncate()
        .pl(px(6.))
        .border_r_1()
        .border_color(rgb(LINE))
}

fn history_row(
    history: &Arc<History>,
    oid: Option<&str>,
    row: usize,
    show_wip: bool,
    graph_width: f32,
    selected: bool,
) -> Stateful<Div> {
    let background = if selected { SELECT } else { WHITE };
    let index = row.checked_sub(usize::from(show_wip));
    let mut element = div()
        .id(("commit-row", row))
        .w_full()
        .h(px(ROW))
        .flex()
        .items_center()
        .cursor_pointer()
        .bg(rgb(background))
        .hover(|style| style.bg(rgb(if selected { SELECT } else { HOVER })));
    let graph = history.clone();
    let drawing = canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            paint_graph(&graph, index, show_wip, bounds, rgb(background), window)
        },
    )
    .w(px(graph_width))
    .h_full()
    .flex_shrink_0();
    if let (Some(oid), Some(index)) = (oid, index) {
        let commit = &history.snapshot.commits[index];
        let rail = history.rows[index].node;
        let labels: Vec<_> = history.labels_for(oid).collect();
        let first = labels
            .iter()
            .find(|reference| {
                reference.kind == RefKind::Local && reference.name == history.snapshot.branch
            })
            .copied()
            .or_else(|| labels.first().copied());
        let refs = div()
            .w(px(REF_WIDTH))
            .min_w_0()
            .flex()
            .items_center()
            .px(px(5.))
            .gap(px(2.))
            .when_some(first, |element, reference| {
                let current =
                    reference.name == history.snapshot.branch && reference.kind == RefKind::Local;
                element
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .px(px(4.))
                            .rounded(px(2.))
                            .text_size(px(11.))
                            .bg(rgb(if current { 0x9edce5 } else { 0xe2eaf7 }))
                            .child(format!(
                                "{}{}",
                                if current { "✓ " } else { "" },
                                reference.name
                            )),
                    )
                    .when(labels.len() > 1, |e| {
                        e.child(
                            div()
                                .flex_shrink_0()
                                .text_size(px(10.))
                                .text_color(rgb(MUTED))
                                .child(format!("+{}", labels.len() - 1)),
                        )
                    })
            });
        element = element
            .child(refs)
            .child(drawing)
            .child(
                div()
                    .flex_1()
                    .min_w(px(95.))
                    .truncate()
                    .pl(px(6.))
                    .border_l_1()
                    .border_color(rgb(rail.color))
                    .child(commit.subject.clone()),
            )
            .child(metadata(commit.author.clone(), 111.))
            .child(metadata(commit.date.clone(), 84.))
            .child(metadata(git::short_id(&commit.id).to_owned(), 62.));
    } else {
        let count = distinct_file_count(&history.snapshot.status);
        element = element
            .child(div().w(px(REF_WIDTH)).min_w_0())
            .child(drawing)
            .child(
                div().flex_1().min_w(px(95.)).truncate().pl(px(6.)).child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(
                            div()
                                .px(px(6.))
                                .border_1()
                                .border_color(rgb(LINE))
                                .rounded(px(3.))
                                .text_color(rgb(MUTED))
                                .child("// WIP"),
                        )
                        .child(div().text_color(rgb(MUTED)).text_size(px(11.)).child(
                            if count == 0 {
                                "Working directory".into()
                            } else {
                                format!("{count} changes")
                            },
                        )),
                ),
            )
            .child(metadata(String::new(), 111.))
            .child(metadata(String::new(), 84.))
            .child(metadata(String::new(), 62.));
    }
    element
}

fn metadata(text: String, width: f32) -> Div {
    div()
        .w(px(width))
        .min_w_0()
        .pl(px(6.))
        .pr(px(3.))
        .truncate()
        .text_size(px(11.))
        .text_color(rgb(MUTED))
        .child(text)
}

fn paint_graph(
    history: &History,
    index: Option<usize>,
    show_wip: bool,
    bounds: Bounds<Pixels>,
    background: Rgba,
    window: &mut Window,
) {
    let origin = bounds.origin;
    let top = origin.y;
    let bottom = top + bounds.size.height;
    let middle = top + bounds.size.height / 2.;
    let head_at_top = history
        .snapshot
        .commits
        .first()
        .is_some_and(|commit| history.snapshot.head.as_deref() == Some(commit.id.as_str()));
    let step = ((bounds.size.width - px(22.)) / history.width.max(2).saturating_sub(1) as f32)
        .min(px(16.));
    let lane_x = |lane: usize| origin.x + px(14.) + step * lane as f32;
    let paint = |start: Point<Pixels>, end: Point<Pixels>, color: u32, window: &mut Window| {
        let mut path = PathBuilder::stroke(px(1.65));
        path.move_to(start);
        if start.x == end.x {
            path.line_to(end);
        } else {
            let bend = start.y + (end.y - start.y) * 0.64;
            path.cubic_bezier_to(end, point(start.x, bend), point(end.x, bend));
        }
        if let Ok(path) = path.build() {
            window.paint_path(path, rgb(color));
        }
    };
    if let Some(row) = index.map(|index| &history.rows[index]) {
        for &Rail { lane, color } in &row.through {
            paint(
                point(lane_x(lane), top),
                point(lane_x(lane), bottom),
                color,
                window,
            );
        }
        let x = lane_x(row.node.lane);
        if row.incoming {
            paint(point(x, top), point(x, middle), row.node.color, window);
        } else if show_wip && index == Some(0) && head_at_top {
            for offset in [1., 4., 7.] {
                dot(window, point(x, top + px(offset)), px(0.85), rgb(TEAL));
            }
        }
        for parent in &row.parents {
            paint(
                point(x, middle),
                point(lane_x(parent.lane), bottom),
                parent.color,
                window,
            );
        }
        dot(window, point(x, middle), px(5.6), background);
        dot(window, point(x, middle), px(4.4), rgb(row.node.color));
    } else {
        let lane = history
            .snapshot
            .head
            .as_ref()
            .and_then(|id| history.position.get(id))
            .map_or(0, |index| history.rows[*index].node.lane);
        let x = lane_x(lane);
        if head_at_top {
            dot(window, point(x, bottom - px(3.)), px(0.85), rgb(TEAL));
        }
        for index in 0..12 {
            let angle = index as f32 * std::f32::consts::TAU / 12.;
            dot(
                window,
                point(x + px(angle.cos() * 6.), middle + px(angle.sin() * 6.)),
                px(0.85),
                rgb(TEAL),
            );
        }
    }
}

fn dot(window: &mut Window, center: Point<Pixels>, radius: Pixels, color: Rgba) {
    let bounds = Bounds::new(
        point(center.x - radius, center.y - radius),
        size(radius * 2., radius * 2.),
    );
    window.paint_quad(fill(bounds, color).corner_radii(radius));
}

fn change_count(tab: &RepoTab) -> usize {
    distinct_file_count(&tab.history.snapshot.status)
}

fn distinct_file_count(status: &git::Status) -> usize {
    status
        .staged
        .iter()
        .chain(&status.unstaged)
        .map(|change| &change.path)
        .collect::<HashSet<_>>()
        .len()
}

fn change_mark(code: u8) -> (&'static str, u32) {
    match code {
        b'A' | b'?' => ("+", 0x2f964e),
        b'D' => ("−", RED),
        b'R' | b'C' => ("↪", 0x7453af),
        b'U' => ("!", RED),
        _ => ("✎", 0xc88732),
    }
}

fn repo_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy()
        .into_owned()
}
