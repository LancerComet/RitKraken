use std::rc::Rc;

use gpui_kit::{component::tooltip::Tooltip, prelude::*, *};
use ritkraken_diff::{Diff, DiffSide};
use ritkraken_git::{PatchSelection, PatchSide};

use super::FileReview;
use crate::app::{
    Client, Load, RepoOperation, RepoTab,
    changes::{StageAction, StageTarget},
    widgets::button,
};

pub(super) fn attach_controls(
    diff: Diff,
    tab: &RepoTab,
    review: &FileReview,
    cx: &Context<Client>,
) -> Diff {
    let (Load::Ready(patch), Some(action)) =
        (&review.patch, StageAction::for_scope(&review.file.scope))
    else {
        return diff;
    };
    if review.show_source || patch.hunk_count() == 0 {
        return diff;
    }

    let enabled = !tab.busy;
    let id = tab.id;
    let request = tab.review.request;
    let file = review.file.clone();
    let patch = patch.clone();
    let submit = Rc::new(
        cx.listener(move |this, selection: &PatchSelection, window, cx| {
            if this
                .tab(id)
                .is_some_and(|tab| tab.review.request == request)
            {
                let edit = RepoOperation::StagePatch(file.clone(), patch.clone(), *selection);
                this.update_repo(id, Some(edit), window, cx);
            }
        }),
    );
    let control = move |id: ElementId, label: SharedString, selection, cx: &App| {
        let submit = submit.clone();
        button(
            id,
            label,
            action.tone(),
            enabled,
            cx,
            move |_, window, cx| {
                submit(&selection, window, cx);
            },
        )
        .rounded_full()
        .flex_shrink_0()
    };

    let hunk_control = control.clone();
    let hunk_label: SharedString = action.label(StageTarget::Hunk).into();
    let line_label: SharedString = action.label(StageTarget::Line).into();
    let symbol = match action {
        StageAction::Stage => "+",
        StageAction::Unstage => "−",
    };
    diff.render_change_controls(
        move |index, _, cx| {
            hunk_control(
                ("hunk-stage-control", index).into(),
                hunk_label.clone(),
                PatchSelection::Hunk(index),
                cx,
            )
            .h(px(20.))
        },
        move |position, _, cx| {
            let side = match position.side() {
                DiffSide::Original => PatchSide::Original,
                DiffSide::Modified => PatchSide::Modified,
            };
            let label = line_label.clone();
            control(
                "line-stage-control".into(),
                symbol.into(),
                PatchSelection::Line {
                    side,
                    number: position.line(),
                },
                cx,
            )
            .size(px(17.))
            .px(px(0.))
            .aria_label(label.clone())
            .invisible()
            .group_hover("diff-line", |style| style.visible())
            .tooltip(move |window, cx| Tooltip::new(label.clone()).build(window, cx))
        },
    )
}
