use gpui_kit::{
    component::{input::Input, scroll::ScrollableElement},
    prelude::*,
    *,
};
use ritkraken_git::Repo;

use crate::i18n::tr;

use super::{ICON_SIZE, ManagerMode, RepoList};
use crate::app::{
    Client,
    theme::palette,
    widgets::{
        Tone, bento_card, button, button_content, error_text, heading, icon,
        loading::{loading_page, opening_message},
        modal::Modal,
    },
};

impl Client {
    pub(in crate::app) fn welcome(&self, cx: &mut Context<Self>) -> Div {
        if self.session.opening {
            return loading_page(opening_message(), cx);
        }
        let p = palette(cx);
        bento_card(p)
            .size_full()
            .flex()
            .justify_center()
            .text_color(rgb(p.text))
            .child(
                div()
                    .w(px(660.))
                    .max_w_full()
                    .mt(px(88.))
                    .px(px(20.))
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(
                        div()
                            .text_size(px(28.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(tr!("app.repositories.title")),
                    )
                    .child(div().h(px(4.)).w(px(96.)).rounded_full().bg(rgb(p.accent)))
                    .child(
                        div()
                            .text_color(rgb(p.text_muted))
                            .child(tr!("app.repositories.welcome_hint")),
                    )
                    .child(
                        button(
                            "welcome-open",
                            tr!("app.repositories.open_manager"),
                            Tone::Primary,
                            true,
                            cx,
                            cx.listener(|this, _, window, cx| this.open_manager(window, cx)),
                        )
                        .rounded_full()
                        .w(px(220.))
                        .h(px(36.))
                        .mt(px(4.)),
                    )
                    .when(!self.saved.recent.is_empty(), |element| {
                        element
                            .child(heading(tr!("app.repositories.recent_heading"), cx).mt(px(14.)))
                            .children(self.saved.recent.iter().take(5).enumerate().map(
                                |(index, path)| {
                                    let root = path.clone();
                                    button(
                                        ("welcome-recent", index),
                                        Repo::name_from_path(path),
                                        Tone::Plain,
                                        true,
                                        cx,
                                        cx.listener(move |this, _, window, cx| {
                                            this.open_recent_repo(root.clone(), window, cx);
                                        }),
                                    )
                                    .rounded(px(10.))
                                    .h(px(32.))
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
    ) -> Modal {
        let p = palette(cx);
        let open = mode == ManagerMode::Open;
        Modal::new(
            "repository-manager-modal",
            tr!("app.repositories.manager"),
            &self.modal_focus,
            !self.session.opening,
            cx.listener(|this, _, window, cx| this.escape(window, cx)),
        )
        .when_some(self.manager.error.as_ref(), |element, error| {
            element.child(error_text(error, cx).mx(px(12.)).mt(px(10.)))
        })
        .child(
            div()
                .flex_1()
                .min_h_0()
                .flex()
                .child(
                    div()
                        .w(px(166.))
                        .flex_shrink_0()
                        .flex()
                        .flex_col()
                        .gap(px(4.))
                        .py(px(10.))
                        .px(px(8.))
                        .bg(rgb(p.surface_high))
                        .border_r_1()
                        .border_color(rgb(p.border))
                        .children(
                            [
                                (
                                    ManagerMode::Open,
                                    "app.common.open",
                                    "icons/folder-open.svg",
                                ),
                                (
                                    ManagerMode::Clone,
                                    "app.repositories.clone",
                                    "icons/download.svg",
                                ),
                                (
                                    ManagerMode::Init,
                                    "app.repositories.init_tab",
                                    "icons/folder-plus.svg",
                                ),
                            ]
                            .into_iter()
                            .map(|(target, key, glyph)| {
                                let tone = Tone::Plain.selected_when(mode == target);
                                button_content(
                                    key,
                                    icon(glyph, tone.foreground(p)).size(px(16.)),
                                    tone,
                                    !self.session.opening,
                                    cx,
                                    cx.listener(move |this, _, _, cx| {
                                        this.manager.mode = Some(target);
                                        this.manager.error = None;
                                        cx.notify();
                                    }),
                                )
                                .child(div().truncate().child(tr!(key)))
                                .gap(px(8.))
                                .w_full()
                                .justify_start()
                                .h(px(40.))
                                .pl(px(14.))
                                .rounded_full()
                                .text_size(px(13.))
                            }),
                        ),
                )
                .when(open, |body| {
                    body.child(self.open_sources(cx))
                        .child(self.open_list(window, cx))
                })
                .when(!open, |body| body.child(self.repository_form(mode, cx))),
        )
    }

    fn open_sources(&self, cx: &mut Context<Self>) -> Div {
        let p = palette(cx);
        div()
            .w(px(218.))
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap(px(2.))
            .py(px(6.))
            .px(px(6.))
            .min_h_0()
            .bg(rgb(p.surface_dim))
            .border_r_1()
            .border_color(rgb(p.border))
            .child(
                button_content(
                    "browse-repo",
                    source_label(tr!("app.repositories.open_repository"), Some("▱")),
                    Tone::Plain,
                    !self.session.opening,
                    cx,
                    cx.listener(|this, _, window, cx| this.browse(false, window, cx)),
                )
                .h(px(34.))
                .rounded(px(8.))
                .justify_start(),
            )
            .children(
                [
                    (RepoList::Recent, "app.repositories.recent"),
                    (RepoList::Favorites, "app.repositories.favorites"),
                ]
                .into_iter()
                .map(|(list, key)| {
                    self.repo_list_button(key, tr!(key), list, cx)
                        .h(px(34.))
                        .rounded(px(8.))
                }),
            )
            .child(
                div()
                    .mt(px(6.))
                    .px(px(8.))
                    .h(px(32.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_t_1()
                    .border_color(rgb(p.border))
                    .child(
                        div()
                            .text_color(rgb(p.text_muted))
                            .font_weight(FontWeight::MEDIUM)
                            .child(tr!("app.repositories.projects")),
                    )
                    .child(
                        button(
                            "add-project",
                            "+",
                            Tone::Green,
                            !self.session.opening,
                            cx,
                            cx.listener(|this, _, window, cx| this.browse(true, window, cx)),
                        )
                        .size(px(22.))
                        .px(px(0.))
                        .rounded_full()
                        .text_size(ICON_SIZE),
                    ),
            )
            .child(
                div().flex_1().min_h_0().child(
                    div()
                        .size_full()
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .children(self.saved.projects.iter().enumerate().map(|(index, path)| {
                            let list = RepoList::Project(path.clone());
                            self.repo_list_button(
                                ("project", index),
                                Repo::name_from_path(path),
                                list,
                                cx,
                            )
                            .h(px(30.))
                            .rounded(px(8.))
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
        let glyph = match list {
            RepoList::Recent => Some("◷"),
            RepoList::Favorites => Some("☆"),
            RepoList::Project(_) => None,
        };
        button_content(
            id,
            source_label(label, glyph),
            Tone::Plain.selected_when(self.manager.list == list),
            !self.session.opening,
            cx,
            cx.listener(move |this, _, window, cx| this.change_repo_list(list.clone(), window, cx)),
        )
        .justify_start()
    }

    fn repository_form(&self, mode: ManagerMode, cx: &mut Context<Self>) -> Div {
        let clone = mode == ManagerMode::Clone;
        div()
            .flex_1()
            .min_w_0()
            .p(px(22.))
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(
                div()
                    .text_size(px(18.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(tr!(if clone {
                        "app.repositories.clone_repository"
                    } else {
                        "app.repositories.init_repository"
                    })),
            )
            .when(clone, |e| {
                e.child(heading(tr!("app.repositories.url_heading"), cx))
                    .child(Input::new(&self.manager.url))
            })
            .child(heading(tr!("app.repositories.destination"), cx))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(Input::new(&self.manager.path)),
                    )
                    .child(
                        button(
                            "browse-destination",
                            tr!("app.common.browse"),
                            Tone::Outline,
                            !self.session.opening,
                            cx,
                            cx.listener(|this, _, window, cx| this.browse(false, window, cx)),
                        )
                        .rounded_full()
                        .px(px(14.)),
                    ),
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
                    cx,
                    cx.listener(|this, _, window, cx| this.open_from_input(window, cx)),
                )
                .rounded_full()
                .w(px(138.))
                .h(px(34.))
                .mt(px(4.)),
            )
    }
}

fn source_label(label: impl Into<SharedString>, glyph: Option<&'static str>) -> Div {
    div()
        .min_w_0()
        .flex()
        .items_center()
        .gap(px(6.))
        .when_some(glyph, |element, glyph| {
            element.child(
                div()
                    .w(ICON_SIZE)
                    .flex_shrink_0()
                    .text_size(ICON_SIZE)
                    .text_center()
                    .child(glyph),
            )
        })
        .child(div().truncate().child(label.into()))
}
