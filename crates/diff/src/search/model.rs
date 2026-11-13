use std::{collections::HashMap, ops::Range, rc::Rc};

use gpui_base::input::SearchMatcher;

use crate::{DiffFile, DiffLinePosition, DiffSide, Rope};

type LineKey = (usize, DiffSide, usize);
type Highlights = HashMap<LineKey, Vec<(usize, Range<usize>)>>;

struct SearchLine {
    file: usize,
    side: DiffSide,
    index: usize,
    counterpart: Option<usize>,
    source: Range<usize>,
}

/// A literal match in source text. Byte offsets exclude the line ending.
#[derive(Clone, Debug)]
pub struct DiffSearchMatch {
    pub position: DiffLinePosition,
    pub range: Range<usize>,
    pub(crate) file: usize,
    pub(crate) line_index: usize,
}

/// Search over both versions, counting identical context once. Its index is
/// independent of folding, viewport, wrapping, and the selected diff layout.
pub struct DiffSearch {
    files: Vec<DiffFile>,
    lines: Vec<SearchLine>,
    matcher: SearchMatcher,
    query: String,
    case_sensitive: bool,
    matches: Vec<DiffSearchMatch>,
    highlights: Rc<Highlights>,
}

impl DiffSearch {
    pub fn new(files: &[DiffFile]) -> Self {
        let mut text = String::new();
        let mut lines = Vec::new();
        for (file_index, file) in files.iter().enumerate() {
            for pair in file.pairs() {
                let identical = pair.original().zip(pair.modified()).is_some_and(|(a, b)| {
                    file.lines(DiffSide::Original)[a].text()
                        == file.lines(DiffSide::Modified)[b].text()
                });
                for (side, index) in [
                    (DiffSide::Original, pair.original().filter(|_| !identical)),
                    (DiffSide::Modified, pair.modified()),
                ] {
                    let Some(index) = index else { continue };
                    let start = text.len();
                    text.push_str(file.lines(side)[index].text());
                    lines.push(SearchLine {
                        file: file_index,
                        side,
                        index,
                        counterpart: identical.then_some(pair.original()).flatten(),
                        source: start..text.len(),
                    });
                    // A separator prevents matches spanning unrelated diff lines.
                    text.push('\n');
                }
            }
        }
        let mut matcher = SearchMatcher::new();
        matcher.update(&Rope::from(text));
        Self {
            files: files.to_vec(),
            lines,
            matcher,
            query: String::new(),
            case_sensitive: false,
            matches: Vec::new(),
            highlights: Rc::default(),
        }
    }

    /// Find a single-line literal, with optional ASCII case folding. Unicode
    /// text is matched without altering its source byte offsets.
    pub fn set_query(&mut self, query: &str, case_sensitive: bool) {
        let query = if query.contains(['\n', '\r']) {
            ""
        } else {
            query
        };
        if self.query == query && self.case_sensitive == case_sensitive {
            return;
        }
        self.query = query.to_owned();
        self.case_sensitive = case_sensitive;
        self.matcher.update_query(query, !case_sensitive);
        self.matches.clear();
        let mut highlights: Highlights = HashMap::new();
        let mut line_index = 0;
        for (match_index, range) in self.matcher.matched_ranges().iter().enumerate() {
            while self.lines[line_index].source.end < range.end {
                line_index += 1;
            }
            let line = &self.lines[line_index];
            let file = &self.files[line.file];
            let source_range = range.start - line.source.start..range.end - line.source.start;
            highlights
                .entry((line.file, line.side, line.index))
                .or_default()
                .push((
                    match_index,
                    file.lines(line.side)[line.index].display_range(source_range.clone()),
                ));
            if let Some(original) = line.counterpart {
                highlights
                    .entry((line.file, DiffSide::Original, original))
                    .or_default()
                    .push((
                        match_index,
                        file.lines(DiffSide::Original)[original]
                            .display_range(source_range.clone()),
                    ));
            }
            self.matches.push(DiffSearchMatch {
                position: DiffLinePosition::new(
                    file.path().clone(),
                    line.side,
                    file.line_number(line.side, line.index),
                ),
                range: source_range,
                file: line.file,
                line_index: line.index,
            });
        }
        self.highlights = Rc::new(highlights);
    }

    pub fn matches(&self) -> &[DiffSearchMatch] {
        &self.matches
    }

    pub fn current(&self) -> Option<&DiffSearchMatch> {
        self.matcher
            .current()
            .and_then(|index| self.matches.get(index))
    }

    pub fn next_match(&mut self, backwards: bool) -> Option<&DiffSearchMatch> {
        if backwards {
            self.matcher.next_back();
        } else {
            self.matcher.next();
        }
        self.current()
    }

    pub fn label(&self) -> String {
        self.matcher.label()
    }

    /// Keep the nearest occurrence in the same file when its text is refreshed.
    pub fn restore_current(&mut self, previous: &DiffSearchMatch) {
        let nearest = self.matches.iter().enumerate().min_by_key(|(_, hit)| {
            (
                hit.position.path() != previous.position.path(),
                hit.position.line().abs_diff(previous.position.line()),
                hit.position.side() != previous.position.side(),
                hit.range.start.abs_diff(previous.range.start),
            )
        });
        if let Some((index, _)) = nearest {
            let offset = self.matcher.matched_ranges()[index].end;
            self.matcher.update_cursor_by_offset(offset);
        }
    }

    pub(crate) fn snapshot(&self) -> SearchHighlights {
        SearchHighlights {
            lines: self.highlights.clone(),
            current: self.matcher.current(),
        }
    }
}

#[derive(Clone, Default)]
pub(crate) struct SearchHighlights {
    lines: Rc<Highlights>,
    current: Option<usize>,
}

impl SearchHighlights {
    pub(crate) fn line(
        &self,
        file: usize,
        side: DiffSide,
        index: usize,
    ) -> impl Iterator<Item = (Range<usize>, bool)> + '_ {
        self.lines
            .get(&(file, side, index))
            .into_iter()
            .flatten()
            .map(|(match_index, range)| (range.clone(), self.current == Some(*match_index)))
    }
}
