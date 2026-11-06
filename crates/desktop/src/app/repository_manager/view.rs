use gpui_kit::{
    component::{Sizable, input::Input, scroll::ScrollableElement},
    prelude::*,
    *,
};
use ritkraken_git::Repo;

use crate::i18n::tr;

use super::{ManagerMode, OpenRepo, RepoList};
use crate::app::{
    Client,
    widgets::{
        HEADER, HOVER, LINE, MUTED, PAPER, TEAL, Tone, WHITE, button, error_text, heading,
        modal_backdrop, note,
    },
};

impl Client {
    pub(in crate::app) fn welcome(&self, cx: &mut Context<Self>) -> Div {
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
                            .child(tr!("app.repositories.title")),
                    )
                    .child(div().h(px(3.)).w(px(110.)).bg(rgb(TEAL)))
                    .child(
                        div()
                            .text_color(rgb(MUTED))
                            .child(tr!("app.repositories.welcome_hint")),
                    )
                    .child(
                        button(
                            "welcome-open",
                            tr!("app.repositories.open_manager"),
                            Tone::Outline,
                            true,
                            cx.listener(|this, _, window, cx| this.open_manager(window, cx)),
                        )
                        .w(px(212.))
                        .h(px(32.)),
                    )
                    .when(!self.saved.recent.is_empty(), |element| {
                        element
                            .child(heading(tr!("app.repositories.recent_heading")).mt(px(12.)))
                            .children(self.saved.recent.iter().take(5).enumerate().map(
                                |(index, path)| {
                                    let root = path.clone();
                                    button(
                                        ("welcome-recent", index),
                                        Repo::name_from_path(path),
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

    pub(in crate::app) fn manager_view(
        &self,
        mode: ManagerMode,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let dimensions = window.viewport_size();
        let width = (dimensions.width - px(44.)).min(px(900.));
        let height = (dimensions.height - px(62.)).min(px(650.));
        let open = mode == ManagerMode::Open;
        modal_backdrop().child(
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
                                .child(tr!("app.repositories.manager")),
                        )
                        .child(button(
                            "close-manager",
                            "×",
                            Tone::Plain,
                            !self.session.opening,
                            cx.listener(|this, _, window, cx| this.escape(window, cx)),
                        )),
                )
                .when_some(self.manager.error.as_ref(), |element, error| {
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
                                        (ManagerMode::Open, "app.common.open"),
                                        (ManagerMode::Clone, "app.repositories.clone"),
                                        (ManagerMode::Init, "app.repositories.init_tab"),
                                    ]
                                    .into_iter()
                                    .map(|(target, key)| {
                                        button(
                                            key,
                                            tr!(key),
                                            Tone::Plain.selected_when(mode == target),
                                            !self.session.opening,
                                            cx.listener(move |this, _, _, cx| {
                                                this.manager.mode = Some(target);
                                                this.manager.error = None;
                                                cx.notify();
                                            }),
                                        )
                                        .w_full()
                                        .justify_start()
                                        .h(px(47.))
                                        .pl(px(15.))
                                        .rounded(px(0.))
                                        .text_size(px(18.))
                                    }),
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
                    format!("▱  {}", tr!("app.repositories.open_repository")),
                    Tone::Plain,
                    !self.session.opening,
                    cx.listener(|this, _, window, cx| this.browse(false, window, cx)),
                )
                .h(px(34.))
                .justify_start(),
            )
            .children(
                [
                    (RepoList::Recent, "◷", "app.repositories.recent"),
                    (RepoList::Favorites, "☆", "app.repositories.favorites"),
                ]
                .into_iter()
                .map(|(list, glyph, key)| {
                    self.repo_list_button(key, format!("{glyph}  {}", tr!(key)), list, cx)
                        .h(px(34.))
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
                    .child(div().child(tr!("app.repositories.projects")))
                    .child(
                        button(
                            "add-project",
                            "+",
                            Tone::Green,
                            !self.session.opening,
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
                            self.repo_list_button(
                                ("project", index),
                                Repo::name_from_path(path),
                                list,
                                cx,
                            )
                            .h(px(29.))
                        }))
                        .overflow_y_scrollbar(),
                ),
            )
    }

    fn repo_list_button(
        &self,
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        list: RepoList,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        button(
            id,
            label,
            Tone::Plain.selected_when(self.manager.list == list),
            !self.session.opening,
            cx.listener(move |this, _, window, cx| this.change_repo_list(list.clone(), window, cx)),
        )
        .justify_start()
    }

    fn open_list(&self, cx: &mut Context<Self>) -> Div {
        let (label, repositories) = match &self.manager.list {
            RepoList::Recent => (tr!("app.repositories.recent"), &self.saved.recent),
            RepoList::Favorites => (tr!("app.repositories.favorites"), &self.saved.favorites),
            RepoList::Project(path) => (Repo::name_from_path(path), &self.manager.projects),
        };
        let filter = self.manager.search.read(cx).value().to_lowercase();
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
                    .child(
                        div()
                            .w(px(190.))
                            .child(Input::new(&self.manager.search).small()),
                    ),
            )
            .child(
                div().flex_1().min_h_0().child(
                    div()
                        .size_full()
                        .flex()
                        .flex_col()
                        .px(px(8.))
                        .when(repositories.is_empty(), |e| {
                            e.child(note(tr!("app.repositories.empty_list")))
                        })
                        .children(
                            repositories
                                .iter()
                                .enumerate()
                                .map(|(index, path)| (index, path, Repo::name_from_path(path)))
                                .filter(|(_, _, name)| name.to_lowercase().contains(&filter))
                                .map(|(index, path, name)| {
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
                                                        .child(name),
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
                            .child(Input::new(&self.manager.path).small()),
                    )
                    .child(button(
                        "open-path",
                        tr!("app.common.open"),
                        Tone::Green,
                        !self.session.opening,
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
            .child(div().text_size(px(18.)).child(tr!(if clone {
                "app.repositories.clone_repository"
            } else {
                "app.repositories.init_repository"
            })))
            .when(clone, |e| {
                e.child(heading(tr!("app.repositories.url_heading")))
                    .child(Input::new(&self.manager.url))
            })
            .child(heading(tr!("app.repositories.destination")))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(7.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(Input::new(&self.manager.path)),
                    )
                    .child(button(
                        "browse-destination",
                        tr!("app.common.browse"),
                        Tone::Outline,
                        !self.session.opening,
                        cx.listener(|this, _, window, cx| this.browse(false, window, cx)),
                    )),
            )
            .child(
                button(
                    "repository-submit",
                    tr!(if clone {
                        "app.repositories.clone"
                    } else {
                        "app.repositories.initialize"
                    }),
                    Tone::Green,
                    !self.session.opening,
                    cx.listener(|this, _, window, cx| this.open_from_input(window, cx)),
                )
                .w(px(130.))
                .h(px(31.)),
            )
    }
}
