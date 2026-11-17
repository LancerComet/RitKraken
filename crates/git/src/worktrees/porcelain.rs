use super::Worktree;
use crate::protocol::{path_from_bytes, text};

pub(super) fn parse(bytes: &[u8]) -> Vec<Worktree> {
    let mut entries = Vec::new();
    let mut current: Option<Worktree> = None;
    for field in bytes.split(|byte| *byte == 0) {
        if let Some(path) = field.strip_prefix(b"worktree ") {
            if let Some(entry) = current.take() {
                entries.push(entry);
            }
            current = Some(Worktree {
                path: path_from_bytes(path),
                head: None,
                branch: None,
                bare: false,
                locked: None,
                prunable: None,
                dirty: false,
                is_current: false,
                is_main: entries.is_empty(),
            });
        } else if let Some(entry) = &mut current {
            if let Some(head) = field.strip_prefix(b"HEAD ") {
                // An unborn branch is represented by an all-zero object id.
                entry.head = head.iter().any(|byte| *byte != b'0').then(|| text(head));
            } else if let Some(branch) = field.strip_prefix(b"branch refs/heads/") {
                entry.branch = Some(text(branch));
            } else if field == b"bare" {
                entry.bare = true;
            } else if field == b"locked" {
                entry.locked = Some(String::new());
            } else if let Some(reason) = field.strip_prefix(b"locked ") {
                entry.locked = Some(text(reason));
            } else if field == b"prunable" {
                entry.prunable = Some(String::new());
            } else if let Some(reason) = field.strip_prefix(b"prunable ") {
                entry.prunable = Some(text(reason));
            }
        }
    }
    if let Some(entry) = current {
        entries.push(entry);
    }
    entries
}
