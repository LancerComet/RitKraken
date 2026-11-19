use std::path::PathBuf;

use gpui_kit::{
    component::{
        Sizable,
        button::Button,
        input::Input,
        menu::{DropdownMenu as _, PopupMenuItem},
        scroll::ScrollableElement,
    },
    prelude::*,
    *,
};
use ritkraken_git::Repo;
use unicase::UniCase;

use crate::{
    app::{
        Client,
        theme::{Palette, palette},
        widgets::{Tone, button, icon, note},
    },
    i18n::tr,
};

use super::{ICON_SIZE, RepoList, remove_recent_button};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum NameOrder {
    Ascending,
    Descending,
}

struct RepositoryRow<'a> {
    index: usize,
    path: &'a PathBuf,
    name: String,
    folded_name: String,
}

fn repository_rows<'a>(
    repositories: &'a [PathBuf],
    filter: &str,
    order: Option<NameOrder>,
) -> Vec<RepositoryRow<'a>> {
    let filter = UniCase::new(filter).to_folded_case();
    let mut rows: Vec<_> = repositories
        .iter()
        .enumerate()
        .filter_map(|(index, path)| {
            let name = Repo::name_from_path(path);
            let folded_name = UniCase::new(name.as_str()).to_folded_case();
            folded_name.contains(&filter).then_some(RepositoryRow {
                index,
                path,
                name,
                folded_name,
            })
        })
        .collect();
    if let Some(order) = order {
        rows.sort_by(|a, b| match order {
            NameOrder::Ascending => a.folded_name.cmp(&b.folded_name),
            NameOrder::Descending => b.folded_name.cmp(&a.folded_name),
        });
    }
    rows
}

impl Client {
    pub(super) fn open_list(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let p = palette(cx);
        let (label, repositories) = match &self.manager.list {
            RepoList::Recent => (tr!("app.repositories.recent"), &self.saved.recent),
            RepoList::Favorites => (tr!("app.repositories.favorites"), &self.saved.favorites),
            RepoList::Project(path) => (Repo::name_from_path(path), &self.manager.projects),
        };
        let is_recent = matches!(self.manager.list, RepoList::Recent);
        let project = matches!(self.manager.list, RepoList::Project(_));
        let order = self
            .manager
            .name_order
            .or(project.then_some(NameOrder::Ascending));
        let filter = self.manager.search.read(cx).value();
        let rem_size = window.rem_size();
        let control_height = rems(1.5).to_pixels(rem_size) + px(2.);
        let compact_text_size = rems(0.875).to_pixels(rem_size) - px(1.);
        let search = Styled::h(Input::new(&self.manager.search).small(), control_height)
            .when(filter.is_empty(), |input| {
                input.text_size(compact_text_size)
            });
        div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .px(px(12.))
                    .h(px(44.))
                    .flex_shrink_0()
                    .border_b_1()
                    .border_color(rgb(p.border))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(15.))
                            .font_weight(FontWeight::MEDIUM)
                            .child(label),
                    )
                    .child(div().w(px(190.)).min_w_0().child(search))
                    .child(name_sort_picker(
                        order,
                        project,
                        control_height,
                        compact_text_size,
                        p,
                        cx,
                    )),
            )
            .child(
                div().flex_1().min_h_0().child(
                    div()
                        .size_full()
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .p(px(8.))
                        .when(repositories.is_empty(), |e| {
                            e.child(note(tr!("app.repositories.empty_list"), cx))
                        })
                        .children(
                            repository_rows(repositories, &filter, order)
                                .into_iter()
                                .map(|row| self.repository_row(row, is_recent, p, cx)),
                        )
                        .overflow_y_scrollbar(),
                ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .p(px(10.))
                    .border_t_1()
                    .border_color(rgb(p.border))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(Input::new(&self.manager.path).small()),
                    )
                    .child(
                        button(
                            "open-path",
                            tr!("app.common.open"),
                            Tone::Green,
                            !self.session.opening,
                            cx,
                            cx.listener(|this, _, window, cx| this.open_from_input(window, cx)),
                        )
                        .rounded_full()
                        .px(px(14.)),
                    ),
            )
    }

    fn repository_row(
        &self,
        row: RepositoryRow<'_>,
        is_recent: bool,
        p: Palette,
        cx: &Context<Self>,
    ) -> Stateful<Div> {
        let RepositoryRow {
            index, path, name, ..
        } = row;
        let root = path.clone();
        let favorite = self.saved.favorites.contains(path);
        let star_path = path.clone();
        div()
            .id(("open-recent", index))
            .h(px(48.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .px(px(10.))
            .gap(px(8.))
            .rounded(px(10.))
            .cursor_pointer()
            .hover(|s| s.bg(rgb(p.hover)))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.open_recent_repo(root.clone(), window, cx);
            }))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(div().truncate().font_weight(FontWeight::MEDIUM).child(name))
                    .child(
                        div()
                            .truncate()
                            .text_size(px(11.))
                            .text_color(rgb(p.text_muted))
                            .child(path.to_string_lossy().into_owned()),
                    ),
            )
            .child(
                button(
                    ("favorite", index),
                    if favorite { "★" } else { "☆" },
                    Tone::Plain,
                    true,
                    cx,
                    cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.toggle_favorite(star_path.clone(), cx);
                    }),
                )
                .size(px(28.))
                .px(px(0.))
                .rounded_full()
                .text_size(ICON_SIZE),
            )
            .when(is_recent, |row| {
                let remove_path = path.clone();
                let remove_group: SharedString = format!("manager-remove-recent-{index}").into();
                row.child(
                    remove_recent_button(
                        ("manager-remove-recent", index),
                        remove_group,
                        p,
                        cx.listener(move |this, _, window, cx| {
                            this.remove_recent_repo(&remove_path, window, cx);
                        }),
                    )
                    .size(px(28.))
                    .rounded_full(),
                )
            })
    }
}

fn name_sort_picker(
    order: Option<NameOrder>,
    project: bool,
    height: Pixels,
    text_size: Pixels,
    p: Palette,
    cx: &Context<Client>,
) -> impl IntoElement {
    let view = cx.weak_entity();
    let label = match order {
        None => tr!("app.repositories.sort"),
        Some(NameOrder::Ascending) => "A–Z".into(),
        Some(NameOrder::Descending) => "Z–A".into(),
    };
    Button::new("repository-name-sort")
        .outline()
        .small()
        .h(height)
        .flex_shrink_0()
        .accessibility_label(tr!("app.repositories.sort"))
        .child(div().text_size(text_size).child(label))
        .child(icon("icons/chevron-down.svg", p.text_muted).size(ICON_SIZE))
        .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, _, _| {
            [
                (None, "app.repositories.sort_default"),
                (
                    Some(NameOrder::Ascending),
                    "app.repositories.sort_ascending",
                ),
                (
                    Some(NameOrder::Descending),
                    "app.repositories.sort_descending",
                ),
            ]
            .into_iter()
            .filter(|(choice, _)| choice.is_some() || !project)
            .fold(menu, |menu, (choice, key)| {
                let view = view.clone();
                menu.item(
                    PopupMenuItem::element(move |_, _| div().text_size(text_size).child(tr!(key)))
                        .checked(choice == order)
                        .on_click(move |_, _, cx| {
                            let _ = view.update(cx, |this, cx| {
                                this.manager.name_order = choice;
                                cx.notify();
                            });
                        }),
                )
            })
        })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{NameOrder, repository_rows};

    #[test]
    fn name_sort_ignores_case_and_preserves_equal_names_in_both_directions() {
        let paths = ["beta", "ALPHA", "alpha", "Charlie"].map(PathBuf::from);
        let names = |filter, order| {
            repository_rows(&paths, filter, order)
                .into_iter()
                .map(|row| row.name)
                .collect::<Vec<_>>()
        };
        assert_eq!(names("", None), ["beta", "ALPHA", "alpha", "Charlie"]);
        assert_eq!(
            names("", Some(NameOrder::Ascending)),
            ["ALPHA", "alpha", "beta", "Charlie"]
        );
        assert_eq!(
            names("", Some(NameOrder::Descending)),
            ["Charlie", "beta", "ALPHA", "alpha"]
        );
        assert_eq!(
            names("aLpH", Some(NameOrder::Descending)),
            ["ALPHA", "alpha"]
        );
    }

    #[test]
    fn search_and_sort_share_unicode_case_folding() {
        let paths = ["Straße", "STRASSE"].map(PathBuf::from);
        let rows = repository_rows(&paths, "strasse", Some(NameOrder::Ascending));
        assert_eq!(
            rows.iter().map(|row| row.name.as_str()).collect::<Vec<_>>(),
            ["Straße", "STRASSE"]
        );
    }
}
