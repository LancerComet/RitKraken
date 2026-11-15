use std::{collections::HashSet, path::PathBuf};

use ritkraken_git::{Change, FileTarget, Scope, Status};

/// Selection belongs to a file list, independently of the open file preview.
#[derive(Clone, Default)]
pub(in crate::app) struct FileSelection {
    scope: Option<Scope>,
    anchor: Option<PathBuf>,
    paths: HashSet<PathBuf>,
}

impl FileSelection {
    pub(super) fn contains(&self, file: &FileTarget) -> bool {
        self.scope.as_ref() == Some(&file.scope) && self.paths.contains(&file.change.path)
    }

    pub(in crate::app) fn preview(&mut self, file: &FileTarget) {
        if !self.contains(file) {
            self.scope = Some(file.scope.clone());
            self.anchor = Some(file.change.path.clone());
            self.paths = HashSet::from([file.change.path.clone()]);
        }
    }

    /// `files` is the displayed file order, excluding directory headings.
    pub(super) fn click(
        &mut self,
        file: &FileTarget,
        files: &[Change],
        range: bool,
        toggle: bool,
    ) -> bool {
        if self.scope.as_ref() != Some(&file.scope) {
            *self = Self::default();
            self.scope = Some(file.scope.clone());
        }
        self.retain_visible(files);
        let end = files
            .iter()
            .position(|entry| entry.path == file.change.path);
        let start = self
            .anchor
            .as_ref()
            .and_then(|anchor| files.iter().position(|entry| &entry.path == anchor));
        if range && let (Some(start), Some(end)) = (start, end) {
            if !toggle {
                self.paths.clear();
            }
            self.paths.extend(
                files[start.min(end)..=start.max(end)]
                    .iter()
                    .map(|file| file.path.clone()),
            );
        } else {
            self.anchor = Some(file.change.path.clone());
            if toggle && !self.paths.insert(file.change.path.clone()) {
                self.paths.remove(&file.change.path);
            } else if !toggle {
                self.paths = HashSet::from([file.change.path.clone()]);
            }
        }
        self.contains(file)
    }

    pub(super) fn selected(&self, scope: &Scope, files: &[Change]) -> Vec<FileTarget> {
        if self.scope.as_ref() != Some(scope) {
            return Vec::new();
        }
        files
            .iter()
            .filter(|change| self.paths.contains(&change.path))
            .map(|change| FileTarget {
                scope: scope.clone(),
                change: change.clone(),
            })
            .collect()
    }

    pub(super) fn clear(&mut self) {
        *self = Self::default();
    }

    pub(in crate::app) fn refresh_worktree(&mut self, status: &Status) {
        match self.scope {
            Some(Scope::Worktree) => self.retain_visible(&status.unstaged),
            Some(Scope::Index) => self.retain_visible(&status.staged),
            _ => {}
        }
    }

    fn retain_visible(&mut self, files: &[Change]) {
        let available: HashSet<_> = files.iter().map(|file| file.path.as_path()).collect();
        self.paths.retain(|path| available.contains(path.as_path()));
        if self
            .anchor
            .as_ref()
            .is_some_and(|path| !available.contains(path.as_path()))
        {
            self.anchor = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files() -> Vec<Change> {
        ["d.txt", "a.txt", "sub/c.txt", "b.txt"]
            .map(|path| Change {
                path: path.into(),
                previous: None,
                code: b'M',
            })
            .to_vec()
    }

    fn target(files: &[Change], index: usize) -> FileTarget {
        FileTarget {
            scope: Scope::Worktree,
            change: files[index].clone(),
        }
    }

    fn selected(selection: &FileSelection, files: &[Change]) -> Vec<PathBuf> {
        selection
            .selected(&Scope::Worktree, files)
            .into_iter()
            .map(|file| file.change.path)
            .collect()
    }

    #[test]
    fn shift_extends_and_shrinks_from_the_anchor_in_display_order() {
        let files = files();
        let mut selection = FileSelection::default();
        selection.click(&target(&files, 1), &files, false, false);
        selection.click(&target(&files, 3), &files, true, false);
        assert_eq!(
            selected(&selection, &files),
            files[1..]
                .iter()
                .map(|f| f.path.clone())
                .collect::<Vec<_>>()
        );
        selection.click(&target(&files, 2), &files, true, false);
        assert_eq!(
            selected(&selection, &files),
            vec![files[1].path.clone(), files[2].path.clone()]
        );
        selection.click(&target(&files, 0), &files, true, false);
        assert_eq!(
            selected(&selection, &files),
            vec![files[0].path.clone(), files[1].path.clone()]
        );
    }

    #[test]
    fn toggle_and_additive_ranges_preserve_nonadjacent_files() {
        let files = files();
        let mut selection = FileSelection::default();
        selection.click(&target(&files, 0), &files, false, false);
        selection.click(&target(&files, 2), &files, false, true);
        selection.click(&target(&files, 3), &files, true, true);
        assert_eq!(selected(&selection, &files).len(), 3);
        assert!(!selection.click(&target(&files, 2), &files, false, true));
        assert_eq!(
            selected(&selection, &files),
            vec![files[0].path.clone(), files[3].path.clone()]
        );
    }

    #[test]
    fn removed_files_cannot_reappear_as_a_stale_selection() {
        let files = files();
        let mut selection = FileSelection::default();
        let mut first = target(&files, 0);
        first.scope = Scope::Index;
        selection.preview(&first);
        let mut second = target(&files, 1);
        second.scope = Scope::Index;
        selection.click(&second, &files, true, false);
        selection.refresh_worktree(&Status {
            unstaged: files[..2].to_vec(),
            staged: files[2..].to_vec(),
        });
        assert!(selection.selected(&Scope::Index, &files).is_empty());
        selection.refresh_worktree(&Status {
            unstaged: Vec::new(),
            staged: files.clone(),
        });
        assert!(selection.selected(&Scope::Index, &files).is_empty());
    }

    #[test]
    fn preview_and_refresh_preserve_selection_by_path_but_scopes_are_isolated() {
        let mut files = files();
        let mut selection = FileSelection::default();
        selection.click(&target(&files, 0), &files, false, false);
        selection.click(&target(&files, 2), &files, true, false);
        files[1].code = b'A';
        selection.preview(&target(&files, 1));
        files.remove(0);
        assert_eq!(selected(&selection, &files).len(), 2);
        selection.click(&target(&files, 2), &files, true, false);
        assert_eq!(selected(&selection, &files), vec![files[2].path.clone()]);
        let mut staged = target(&files, 1);
        staged.scope = Scope::Index;
        selection.click(&staged, &files, true, true);
        assert!(selected(&selection, &files).is_empty());
        assert_eq!(selection.selected(&Scope::Index, &files).len(), 1);
    }
}
