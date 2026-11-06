use std::ops::Range;

use super::align_changed_lines;

pub(super) fn ranges(hunk: &[u8]) -> Option<[Range<usize>; 2]> {
    let mut fields = hunk.strip_prefix(b"@@ ")?.split(|byte| *byte == b' ');
    let parse = |range: &[u8], prefix| {
        let range = std::str::from_utf8(range.strip_prefix(&[prefix])?).ok()?;
        let (start, count) = match range.split_once(',') {
            Some((start, count)) => (start.parse::<usize>().ok()?, count.parse::<usize>().ok()?),
            None => (range.parse::<usize>().ok()?, 1),
        };
        let start = start.checked_add(usize::from(count == 0))?;
        Some(start..start.checked_add(count)?)
    };
    Some([parse(fields.next()?, b'-')?, parse(fields.next()?, b'+')?])
}

pub(super) type Sides<'a> = [Option<&'a [u8]>; 2];

/// Projects equivalent lines as context and preserves each side's bytes.
pub(super) fn visit_lines<'a>(
    hunk: &'a [u8],
    ignore_edge_whitespace: bool,
    mut visit: impl FnMut(Sides<'a>) -> Option<()>,
) -> Option<()> {
    let mut input = hunk
        .split_inclusive(|byte| *byte == b'\n')
        .skip(1)
        .peekable();
    let mut original = Vec::new();
    let mut modified = Vec::new();
    while let Some(line) = input.next() {
        let (tag, mut content) = match line {
            b"\n" => (b' ', line),
            [tag @ (b' ' | b'-' | b'+'), content @ ..] => (*tag, content),
            _ => return None,
        };
        if input.peek() == Some(&&b"\\ No newline at end of file\n"[..]) {
            content = content.strip_suffix(b"\n")?;
            input.next();
        }
        match tag {
            b'-' => original.push(content),
            b'+' => modified.push(content),
            _ => {
                visit_changes(&original, &modified, ignore_edge_whitespace, &mut visit)?;
                original.clear();
                modified.clear();
                visit([Some(content), Some(content)])?;
            }
        }
    }
    visit_changes(&original, &modified, ignore_edge_whitespace, &mut visit)
}

fn visit_changes<'a>(
    original: &[&'a [u8]],
    modified: &[&'a [u8]],
    ignore_edge_whitespace: bool,
    visit: &mut impl FnMut(Sides<'a>) -> Option<()>,
) -> Option<()> {
    if original.is_empty() && modified.is_empty() {
        return Some(());
    }
    for group in align_changed_lines(
        original.iter().copied(),
        modified.iter().copied(),
        ignore_edge_whitespace,
    ) {
        let original = original[group.original].iter().copied();
        let modified = modified[group.modified].iter().copied();
        if group.equivalent {
            for (old, new) in original.zip(modified) {
                visit([Some(old), Some(new)])?;
            }
        } else {
            for line in original {
                visit([Some(line), None])?;
            }
            for line in modified {
                visit([None, Some(line)])?;
            }
        }
    }
    Some(())
}
