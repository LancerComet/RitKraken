use gpui_kit::{
    component::{button::Button, input::Textarea, scroll::ScrollableElement},
    prelude::*,
    *,
};
use ritkraken_git::{RebaseAction, short_id};

use super::dialog::CommitDialog;
use crate::{
    app::{
        Load,
        widgets::{
            LINE, MUTED, SELECT, Tone, button, error_text,
            menu::{DropdownMenu as _, PopupMenuItem, menu_style},
            note,
        },
    },
    i18n::tr,
};

impl CommitDialog {
    pub(super) fn sync_plan_message(&mut self, cx: &App) {
        if let Some(Load::Ready(plan)) = &mut self.plan
            && let Some(entry) = plan.entries.get_mut(self.selected)
            && entry.action == RebaseAction::Reword
        {
            entry.message = self.message.read(cx).value().to_string();
        }
    }

    pub(super) fn select_plan_entry(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sync_plan_message(cx);
        self.show_plan_entry(index, window, cx);
    }

    fn show_plan_entry(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Load::Ready(plan)) = &self.plan else {
            return;
        };
        let Some(entry) = plan.entries.get(index) else {
            return;
        };
        self.selected = index;
        let value = entry.message.clone();
        self.message
            .update(cx, |input, cx| input.set_value(value, window, cx));
        cx.notify();
    }

    fn set_plan_action(
        &mut self,
        index: usize,
        action: RebaseAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sync_plan_message(cx);
        let Some(Load::Ready(plan)) = &mut self.plan else {
            return;
        };
        if let Some(entry) = plan.entries.get_mut(index) {
            entry.action = action;
        }
        self.show_plan_entry(index, window, cx);
    }

    fn move_plan_entry(
        &mut self,
        index: usize,
        delta: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sync_plan_message(cx);
        let Some(Load::Ready(plan)) = &mut self.plan else {
            return;
        };
        let destination = index.saturating_add_signed(delta);
        if destination >= plan.entries.len() {
            return;
        }
        plan.entries.swap(index, destination);
        self.show_plan_entry(destination, window, cx);
    }

    pub(super) fn planner_view(&self, cx: &mut Context<Self>) -> Div {
        let Some(Load::Ready(plan)) = &self.plan else {
            return div().flex_1().child(match &self.plan {
                Some(Load::Failed(error)) => error_text(error),
                _ => note(tr!("app.commit_menu.loading_plan")),
            });
        };
        let count = plan.entries.len();
        let reword = plan
            .entries
            .get(self.selected)
            .is_some_and(|entry| entry.action == RebaseAction::Reword);
        let list = uniform_list(
            "interactive-rebase-commits",
            count,
            cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                let Some(Load::Ready(plan)) = &this.plan else {
                    return Vec::new();
                };
                range
                    .map(|index| {
                        let entry = &plan.entries[index];
                        let selected = this.selected == index;
                        let view = cx.weak_entity();
                        let dropdown = Button::new(("rebase-action", index))
                            .label(action_label(entry.action))
                            .text_size(px(12.))
                            .w(px(122.))
                            .h(px(26.))
                            .dropdown_menu(move |menu, window, _| {
                                RebaseAction::ALL.into_iter().fold(
                                    menu_style(menu, window),
                                    |menu, action| {
                                        let view = view.clone();
                                        menu.item(
                                            PopupMenuItem::new(action_label(action)).on_click(
                                                move |_, window, cx| {
                                                    let _ = view.update(cx, |this, cx| {
                                                        this.set_plan_action(
                                                            index, action, window, cx,
                                                        )
                                                    });
                                                },
                                            ),
                                        )
                                    },
                                )
                            });
                        div()
                            .id(("rebase-row", index))
                            .h(px(36.))
                            .w_full()
                            .px(px(6.))
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .bg(rgb(if selected { SELECT } else { 0xffffff }))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.select_plan_entry(index, window, cx)
                            }))
                            .child(dropdown)
                            .child(
                                div()
                                    .w(px(60.))
                                    .text_color(rgb(MUTED))
                                    .child(short_id(&entry.commit.id).to_owned()),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .child(entry.commit.subject.clone()),
                            )
                            .child(button(
                                ("rebase-up", index),
                                "↑",
                                Tone::Outline,
                                index > 0,
                                cx.listener(move |this, _, window, cx| {
                                    cx.stop_propagation();
                                    this.move_plan_entry(index, -1, window, cx);
                                }),
                            ))
                            .child(button(
                                ("rebase-down", index),
                                "↓",
                                Tone::Outline,
                                index + 1 < count,
                                cx.listener(move |this, _, window, cx| {
                                    cx.stop_propagation();
                                    this.move_plan_entry(index, 1, window, cx);
                                }),
                            ))
                    })
                    .collect::<Vec<_>>()
            }),
        )
        .track_scroll(&self.scroll)
        .size_full();
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(div().text_color(rgb(MUTED)).child(tr!(
                "app.commit_menu.rebase_range",
                base = short_id(&plan.base),
                head = short_id(&self.context.head),
                count = count
            )))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .px(px(6.))
                    .py(px(8.))
                    .bg(rgb(0xf1f2f4))
                    .child(div().w(px(122.)).child(tr!("app.commit_menu.action")))
                    .child(div().w(px(60.)).child(tr!("app.history.sha")))
                    .child(div().flex_1().child(tr!("app.history.message"))),
            )
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .border_1()
                    .border_color(rgb(LINE))
                    .pr(px(18.))
                    .child(list)
                    .vertical_scrollbar(&self.scroll),
            )
            .when(reword, |element| {
                element.child(
                    div()
                        .flex_shrink_0()
                        .child(Textarea::new(&self.message).text_size(px(12.)).h(px(130.))),
                )
            })
            .child(
                div()
                    .text_color(rgb(MUTED))
                    .child(tr!("app.commit_menu.plan_hint")),
            )
    }
}

fn action_label(action: RebaseAction) -> String {
    tr!(match action {
        RebaseAction::Pick => "app.commit_menu.pick",
        RebaseAction::Reword => "app.commit_menu.reword",
        RebaseAction::Edit => "app.commit_menu.edit",
        RebaseAction::Squash => "app.commit_menu.squash",
        RebaseAction::Fixup => "app.commit_menu.fixup",
        RebaseAction::Drop => "app.commit_menu.drop",
    })
}
