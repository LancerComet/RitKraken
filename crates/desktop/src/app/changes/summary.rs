use gpui_kit::{component::tooltip::Tooltip, prelude::*, *};
use ritkraken_git::Change;

use crate::{
    app::{theme::Palette, widgets::icon},
    i18n::tr,
};

pub(in crate::app) struct ChangeMark {
    pub glyph: &'static str,
    pub icon_path: Option<&'static str>,
    pub color: u32,
}

pub(in crate::app) fn change_mark(code: u8, p: Palette) -> ChangeMark {
    ChangeKind::from_code(code).map_or(
        ChangeMark {
            glyph: "",
            icon_path: None,
            color: p.text_muted,
        },
        |kind| kind.mark(p),
    )
}

#[derive(Clone, Copy)]
enum ChangeKind {
    Added,
    Deleted,
    Renamed,
    Modified,
    Conflicted,
}

impl ChangeKind {
    const COUNT: usize = Self::Conflicted as usize + 1;

    fn from_code(code: u8) -> Option<Self> {
        Some(match code {
            b' ' => return None,
            b'A' | b'?' => Self::Added,
            b'D' => Self::Deleted,
            b'R' | b'C' => Self::Renamed,
            b'U' => Self::Conflicted,
            _ => Self::Modified,
        })
    }

    fn mark(self, p: Palette) -> ChangeMark {
        let (glyph, icon_path, color) = match self {
            Self::Added => ("+", "icons/plus.svg", p.success),
            Self::Deleted => ("−", "icons/minus.svg", p.danger),
            Self::Renamed => ("↪", "icons/corner-up-right.svg", p.renamed),
            Self::Modified => ("✎", "icons/pencil.svg", p.warning),
            Self::Conflicted => ("!", "icons/circle-alert.svg", p.danger),
        };
        ChangeMark {
            glyph,
            icon_path: Some(icon_path),
            color,
        }
    }

    fn label_key(self) -> &'static str {
        match self {
            Self::Added => "app.commit.added",
            Self::Deleted => "app.commit.deleted",
            Self::Renamed => "app.commit.renamed",
            Self::Modified => "app.commit.modified",
            Self::Conflicted => "app.commit.conflicted",
        }
    }
}

#[derive(Clone, Copy)]
pub(in crate::app) enum SummaryStyle {
    Compact,
    Labeled,
}

impl SummaryStyle {
    fn order(self) -> [ChangeKind; ChangeKind::COUNT] {
        use ChangeKind::*;
        match self {
            Self::Compact => [Modified, Added, Deleted, Renamed, Conflicted],
            Self::Labeled => [Added, Deleted, Renamed, Modified, Conflicted],
        }
    }
}

#[derive(Clone)]
pub(in crate::app) struct ChangeSummary([usize; ChangeKind::COUNT]);

impl ChangeSummary {
    pub(in crate::app) fn new<'a>(changes: impl IntoIterator<Item = &'a Change>) -> Self {
        let mut counts = [0; ChangeKind::COUNT];
        for change in changes {
            if let Some(kind) = ChangeKind::from_code(change.code) {
                counts[kind as usize] += 1;
            }
        }
        Self(counts)
    }

    pub(in crate::app) fn total(&self) -> usize {
        self.0.iter().sum()
    }

    pub(in crate::app) fn view(&self, style: SummaryStyle, p: Palette) -> Div {
        let compact = matches!(style, SummaryStyle::Compact);
        div()
            .flex()
            .items_center()
            .gap(px(if compact { 6. } else { 9. }))
            .text_size(px(if compact { 12. } else { 11. }))
            .when(!compact, |element| {
                element.h(px(32.)).flex_shrink_0().px(px(10.))
            })
            .children(style.order().into_iter().filter_map(|kind| {
                let count = self.0[kind as usize];
                (count > 0).then(|| count_item(kind, count, compact, p))
            }))
    }
}

fn count_item(kind: ChangeKind, count: usize, compact: bool, p: Palette) -> AnyElement {
    let mark = kind.mark(p);
    let key = kind.label_key();
    let label = tr!(key, count = count);
    let item = div().flex().items_center().gap(px(3.));
    if compact {
        item.id(key)
            .flex_shrink_0()
            .text_color(rgb(p.text))
            .aria_label(label.clone())
            .tooltip(move |window, cx| Tooltip::new(label.clone()).build(window, cx))
            .children(
                mark.icon_path
                    .map(|path| icon(path, mark.color).size(px(11.))),
            )
            .child(count.to_string())
            .into_any_element()
    } else {
        item.child(div().text_color(rgb(mark.color)).child(mark.glyph))
            .child(label)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use ritkraken_git::{Change, Status};

    use super::{ChangeKind, ChangeSummary};

    #[test]
    fn worktree_counts_staged_and_unstaged_paths_once() {
        let change = |path: &str, code| Change {
            path: path.into(),
            previous: None,
            code,
        };
        let status = Status {
            staged: vec![change("new", b'A'), change("modified", b'M')],
            unstaged: vec![
                change("new", b'M'),
                change("modified", b'M'),
                change("untracked", b'?'),
                change("type-changed", b'T'),
                change("removed", b'D'),
                change("moved", b'R'),
                change("unmerged", b'U'),
            ],
        };
        let summary = ChangeSummary::new(status.distinct_changes());
        assert_eq!(summary.total(), 7);
        assert_eq!(summary.0[ChangeKind::Added as usize], 2);
        assert_eq!(summary.0[ChangeKind::Modified as usize], 2);
        assert_eq!(summary.0[ChangeKind::Deleted as usize], 1);
        assert_eq!(summary.0[ChangeKind::Renamed as usize], 1);
        assert_eq!(summary.0[ChangeKind::Conflicted as usize], 1);
    }
}
