use std::ops::Range;

use super::{FileTarget, Patch, Scope, hunk};
use crate::{
    Repo, Result, RuleError,
    command::{run_read, run_with_input},
    protocol::text,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PatchSide {
    Original,
    Modified,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PatchSelection {
    Hunk(usize),
    Line { side: PatchSide, number: usize },
}

impl Repo {
    pub fn apply_partial(
        &self,
        file: &FileTarget,
        shown: &Patch,
        selection: PatchSelection,
    ) -> Result<()> {
        let patch = self.patch(file)?;
        if patch.bytes != shown.bytes {
            return Err(RuleError::StalePatch.into());
        }
        let (hunk, ranges) = patch
            .selected_hunk(selection)
            .ok_or(RuleError::UnstageableSelection)?;
        let header = IndexHeader::parse(&patch.bytes[..patch.header])
            .ok_or(RuleError::UnstageableSelection)?;
        let reverse = file.scope == Scope::Index;
        let base = if reverse { header.new } else { header.old };
        let next = {
            let indexed = run_read(self.git().args(["cat-file", "blob", base]))?;
            let (span, replacement) = index_edit(
                &indexed,
                hunk,
                ranges,
                selection,
                reverse,
                shown.ignores_edge_whitespace(),
            )
            .ok_or(RuleError::UnstageableSelection)?;
            run_with_input(
                self.git()
                    .args(["hash-object", "-w", "--stdin", "--no-filters"]),
                &[&indexed[..span.start], &replacement, &indexed[span.end..]],
            )?
        };
        let index_line = format!("index {base}..{}\n", text(&next).trim_end());
        // Git verifies the binary patch's full preimage OID under the index lock.
        // Both blobs already exist, so the patch needs only their IDs and Git's paths.
        run_with_input(
            self.git().args(["apply", "--cached", "--binary", "-"]),
            &[
                header.file,
                index_line.as_bytes(),
                header.paths,
                b"Binary files ",
                header.original_path,
                b" and ",
                header.modified_path,
                b" differ\n",
            ],
        )?;
        Ok(())
    }
}

struct IndexHeader<'a> {
    file: &'a [u8],
    paths: &'a [u8],
    original_path: &'a [u8],
    modified_path: &'a [u8],
    old: &'a str,
    new: &'a str,
}

impl<'a> IndexHeader<'a> {
    fn parse(header: &'a [u8]) -> Option<Self> {
        let mut lines = header.split_inclusive(|byte| *byte == b'\n');
        let file = lines
            .next()
            .filter(|line| line.starts_with(b"diff --git "))?;
        let index_line = lines.next()?;
        let index = std::str::from_utf8(index_line)
            .ok()?
            .strip_prefix("index ")?;
        let (ids, mode) = index.trim_end().split_once(' ')?;
        if !matches!(mode, "100644" | "100755") {
            return None;
        }
        let (old, new) = ids.split_once("..")?;
        let paths = &header[file.len() + index_line.len()..];
        let original_path = header_path(lines.next()?, b"--- ")?;
        let modified_path = header_path(lines.next()?, b"+++ ")?;
        (lines.next().is_none()).then_some(Self {
            file,
            paths,
            original_path,
            modified_path,
            old,
            new,
        })
    }
}

fn header_path<'a>(line: &'a [u8], prefix: &[u8]) -> Option<&'a [u8]> {
    let path = line.strip_prefix(prefix)?.strip_suffix(b"\n")?;
    let path = path.strip_suffix(b"\t").unwrap_or(path);
    (path != b"/dev/null").then_some(path)
}

pub(super) fn is_text_modification(header: &[u8]) -> bool {
    IndexHeader::parse(header).is_some()
}

impl Patch {
    fn selected_hunk(&self, selection: PatchSelection) -> Option<(&[u8], [Range<usize>; 2])> {
        let parse = |range: &Range<usize>| {
            let hunk = &self.bytes[range.clone()];
            Some((hunk, hunk::ranges(hunk)?))
        };
        match selection {
            PatchSelection::Hunk(index) => parse(self.hunks.get(index)?),
            PatchSelection::Line { side, number } => self
                .hunks
                .iter()
                .filter_map(parse)
                .find(|(_, ranges)| ranges[side as usize].contains(&number)),
        }
    }
}

fn index_edit(
    indexed: &[u8],
    hunk: &[u8],
    [old, new]: [Range<usize>; 2],
    selection: PatchSelection,
    reverse: bool,
    ignore_edge_whitespace: bool,
) -> Option<(Range<usize>, Vec<u8>)> {
    let base = if reverse { &new } else { &old };
    let mut preimage = indexed.split_inclusive(|byte| *byte == b'\n');
    let ending: &[u8] = if preimage
        .clone()
        .next()
        .is_some_and(|line| line.ends_with(b"\r\n"))
    {
        b"\r\n"
    } else {
        b"\n"
    };
    let start = (1..base.start).try_fold(0, |offset, _| Some(offset + preimage.next()?.len()))?;
    let mut old_line = old.start;
    let mut new_line = new.start;
    let mut end = start;
    let mut replacement = Vec::new();
    let mut matched = false;
    hunk::visit_lines(hunk, ignore_edge_whitespace, |[original, modified]| {
        let changed = original.is_none() || modified.is_none();
        let selected = changed
            && match selection {
                PatchSelection::Hunk(_) => true,
                PatchSelection::Line {
                    side: PatchSide::Original,
                    number,
                } => original.is_some() && number == old_line,
                PatchSelection::Line {
                    side: PatchSide::Modified,
                    number,
                } => modified.is_some() && number == new_line,
            };
        matched |= selected;
        old_line += usize::from(original.is_some());
        new_line += usize::from(modified.is_some());

        let (indexed_line, other_line) = if reverse {
            (modified, original)
        } else {
            (original, modified)
        };
        if let Some(content) = indexed_line {
            if preimage.next()? != content {
                return None;
            }
            end += content.len();
        }
        if let Some(content) = if selected { other_line } else { indexed_line } {
            terminate_line(&mut replacement, ending);
            replacement.extend_from_slice(content);
        }
        Some(())
    })?;
    if end < indexed.len() {
        terminate_line(&mut replacement, ending);
    }
    (matched && old_line == old.end && new_line == new.end).then_some((start..end, replacement))
}

fn terminate_line(bytes: &mut Vec<u8>, ending: &[u8]) {
    // A former EOF line needs a separator if another line now follows it.
    if bytes.last().is_some_and(|last| *last != b'\n') {
        bytes.extend_from_slice(ending);
    }
}
