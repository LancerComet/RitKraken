mod view;

use std::{ops::Range, rc::Rc};

use gpui::SharedString;

use crate::{DiffFile, DiffSide, diff::DisplayRow};

pub(crate) use view::scrollbar;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffChangeKind {
    Removed = 1,
    Added = 2,
    Modified = 3,
}

impl DiffChangeKind {
    fn from_flags(flags: u8) -> Self {
        match flags {
            1 => Self::Removed,
            2 => Self::Added,
            _ => Self::Modified,
        }
    }
}

/// A change on the modified source. Deletions are anchored at the next line.
#[derive(Clone, Debug)]
pub struct DiffOverviewSpan {
    path: SharedString,
    lines: Range<usize>,
    kind: DiffChangeKind,
}

impl DiffOverviewSpan {
    pub fn path(&self) -> &SharedString {
        &self.path
    }
    pub fn lines(&self) -> Range<usize> {
        self.lines.clone()
    }
    pub fn kind(&self) -> DiffChangeKind {
        self.kind
    }
}

/// Retained change metadata, shared with a full source viewer without diffing again.
#[derive(Clone, Default)]
pub struct DiffOverview {
    spans: Rc<[DiffOverviewSpan]>,
}

impl DiffOverview {
    pub fn from_files(files: &[DiffFile]) -> Self {
        let mut spans: Vec<DiffOverviewSpan> = Vec::new();
        for file in files {
            let mut previous = 0;
            for pair in file.pairs() {
                let modified = pair
                    .modified()
                    .map(|ix| file.line_number(DiffSide::Modified, ix));
                if let Some(line) = modified {
                    previous = line;
                }
                if !pair.is_changed() {
                    continue;
                }
                let flags = u8::from(pair.original().is_some())
                    | (u8::from(pair.modified().is_some()) << 1);
                if flags == 0 {
                    continue;
                }
                let line = modified
                    .unwrap_or_else(|| previous.saturating_add(1))
                    .max(1);
                let mut kind = DiffChangeKind::from_flags(flags);
                if let Some(last) = spans.last_mut().filter(|last| last.path == *file.path()) {
                    if last.lines.contains(&line) {
                        if last.kind == kind {
                            continue;
                        }
                        kind = DiffChangeKind::from_flags(last.kind as u8 | flags);
                        last.lines.end = line;
                        if last.lines.is_empty() {
                            spans.pop();
                        }
                    } else if last.lines.end == line && last.kind == kind {
                        last.lines.end = line.saturating_add(1);
                        continue;
                    }
                }
                spans.push(DiffOverviewSpan {
                    path: file.path().clone(),
                    lines: line..line.saturating_add(1),
                    kind,
                });
            }
        }
        spans.sort_by(|a, b| a.path.cmp(&b.path).then(a.lines.start.cmp(&b.lines.start)));
        Self {
            spans: spans.into(),
        }
    }

    pub fn spans(&self) -> &[DiffOverviewSpan] {
        &self.spans
    }

    fn file_flags(&self, path: &SharedString) -> u8 {
        let first = self.spans.partition_point(|span| span.path < *path);
        self.spans[first..]
            .iter()
            .take_while(|span| span.path == *path)
            .fold(0, |flags, span| flags | span.kind as u8)
    }

    fn flags_at(&self, path: &SharedString, line: usize, last_line: usize) -> u8 {
        let end = self.spans.partition_point(|span| span.path <= *path);
        let ix =
            self.spans[..end].partition_point(|span| span.path < *path || span.lines.start <= line);
        let mut flags = ix
            .checked_sub(1)
            .and_then(|ix| self.spans.get(ix))
            .filter(|span| span.path == *path && span.lines.contains(&line))
            .map_or(0, |span| span.kind as u8);
        if line == last_line {
            for span in &self.spans[ix..end] {
                flags |= span.kind as u8;
            }
        }
        flags
    }
}

#[derive(Clone)]
struct Segment {
    rows: Range<usize>,
    flags: u8,
}

#[derive(Clone, Default)]
pub(crate) struct Overview {
    segments: Rc<[Segment]>,
    row_count: usize,
    cache: view::PaintCache,
}

impl Overview {
    pub(crate) fn new(
        rows: &[DisplayRow],
        files: &[DiffFile],
        source: Option<&DiffOverview>,
    ) -> Self {
        let mut segments: Vec<Segment> = Vec::new();
        for (ix, row) in rows.iter().enumerate() {
            let flags = match row {
                DisplayRow::Code { file, modified, .. } if source.is_some() => {
                    modified.map_or(0, |line| {
                        source.unwrap().flags_at(
                            files[*file].path(),
                            files[*file].line_number(DiffSide::Modified, line),
                            files[*file].lines_count(DiffSide::Modified),
                        )
                    })
                }
                DisplayRow::Code {
                    original,
                    modified,
                    changed: true,
                    ..
                } => u8::from(original.is_some()) | (u8::from(modified.is_some()) << 1),
                DisplayRow::File(file)
                    if rows.get(ix + 1).is_none_or(|next| next.file() != *file) =>
                {
                    source.map_or_else(
                        || file_flags(&files[*file]),
                        |source| source.file_flags(files[*file].path()),
                    )
                }
                DisplayRow::Notice(file) => source.map_or_else(
                    || file_flags(&files[*file]),
                    |source| source.file_flags(files[*file].path()),
                ),
                _ => 0,
            };
            if flags == 0 {
                continue;
            }
            if let Some(last) = segments
                .last_mut()
                .filter(|last| last.rows.end == ix && last.flags == flags)
            {
                last.rows.end += 1;
            } else {
                segments.push(Segment {
                    rows: ix..ix + 1,
                    flags,
                });
            }
        }
        Self {
            segments: segments.into(),
            row_count: rows.len(),
            cache: view::PaintCache::default(),
        }
    }
}

fn file_flags(file: &DiffFile) -> u8 {
    let flags = file
        .pairs()
        .iter()
        .filter(|pair| pair.is_changed())
        .fold(0, |flags, pair| {
            flags | u8::from(pair.original().is_some()) | (u8::from(pair.modified().is_some()) << 1)
        });
    if flags == 0 && file.is_binary() {
        3
    } else {
        flags
    }
}
