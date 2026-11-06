use std::sync::Arc;

use gpui::{AnyElement, App, IntoElement, SharedString, Window};

use crate::{DiffFile, DiffLinePosition, DiffSide};

type Renderer<T> = Box<dyn Fn(T, &mut Window, &mut App) -> AnyElement>;

pub(crate) struct ChangeControls {
    pub hunk: Renderer<usize>,
    pub line: Renderer<DiffLinePosition>,
}

impl ChangeControls {
    pub fn new<H: IntoElement, L: IntoElement>(
        hunk: impl Fn(usize, &mut Window, &mut App) -> H + 'static,
        line: impl Fn(DiffLinePosition, &mut Window, &mut App) -> L + 'static,
    ) -> Self {
        Self {
            hunk: Box::new(move |index, window, cx| hunk(index, window, cx).into_any_element()),
            line: Box::new(move |position, window, cx| {
                line(position, window, cx).into_any_element()
            }),
        }
    }
}

pub(crate) struct HunkHeading {
    pub pair: usize,
    pub label: SharedString,
    pub has_changes: bool,
}

pub(crate) fn hunk_headings(full: &DiffFile, compact: &DiffFile) -> Option<Arc<[HunkHeading]>> {
    if full.path() != compact.path() {
        return None;
    }
    // Git joins hunks in a full-context patch. Find the first display pair
    // belonging to each original hunk without discarding the intervening lines.
    let mut pairs = full.pairs().iter().enumerate();
    compact
        .hunks()
        .iter()
        .enumerate()
        .map(|(index, hunk)| {
            let [original, modified] = [DiffSide::Original, DiffSide::Modified].map(|side| {
                let range = hunk.lines(side);
                (!range.is_empty())
                    .then(|| compact.line_number(side, range.start))
                    .and_then(|line| full.line_index(side, line))
            });
            let (pair, _) = pairs.find(|(_, pair)| {
                original.is_some_and(|index| pair.original() == Some(index))
                    || modified.is_some_and(|index| pair.modified() == Some(index))
            })?;
            Some(HunkHeading {
                pair,
                label: hunk.label().clone(),
                has_changes: compact.hunk_has_changes(index),
            })
        })
        .collect::<Option<Vec<_>>>()
        .map(Arc::from)
}
