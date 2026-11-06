use std::{collections::HashMap, ops::Range};

use similar::{Algorithm, DiffTag, capture_diff_slices};

pub struct AlignedLines {
    pub original: Range<usize>,
    pub modified: Range<usize>,
    pub equivalent: bool,
}

pub fn edge_whitespace_key(line: &[u8]) -> (&[u8], bool) {
    let content =
        std::str::from_utf8(line).map_or_else(|_| line.trim_ascii(), |text| text.trim().as_bytes());
    // Keep the final newline significant even when the line's text is empty.
    (content, line.ends_with(b"\n"))
}

/// Aligns one run of deleted and added lines. Callers retain the original
/// bytes on both sides of equivalent lines.
pub fn align_changed_lines<'a>(
    mut original: impl ExactSizeIterator<Item = &'a [u8]>,
    mut modified: impl ExactSizeIterator<Item = &'a [u8]>,
    ignore_edge_whitespace: bool,
) -> Vec<AlignedLines> {
    let whole_change = AlignedLines {
        original: 0..original.len(),
        modified: 0..modified.len(),
        equivalent: false,
    };
    if !ignore_edge_whitespace || original.len() == 0 || modified.len() == 0 {
        return vec![whole_change];
    }

    let mut keys = HashMap::new();
    let mut ids = |lines: &mut dyn Iterator<Item = &'a [u8]>| {
        let ids = lines
            .map(|line| {
                let next_id = keys.len();
                *keys.entry(edge_whitespace_key(line)).or_insert(next_id)
            })
            .collect::<Vec<_>>();
        (ids, keys.len())
    };
    let (original, original_key_count) = ids(&mut original);
    let (modified, _) = ids(&mut modified);
    if modified.iter().all(|id| *id >= original_key_count) {
        return vec![whole_change];
    }

    capture_diff_slices(Algorithm::Patience, &original, &modified)
        .into_iter()
        .map(|op| {
            let (tag, original, modified) = op.as_tag_tuple();
            AlignedLines {
                original,
                modified,
                equivalent: tag == DiffTag::Equal,
            }
        })
        .collect()
}
