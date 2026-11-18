use crate::{Repo, Result, RuleError, command::run_write, protocol::text};

/// One entry of `git stash list`, newest first.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Stash {
    /// Position in the stash list: `stash@{index}`.
    pub index: usize,
    pub id: String,
    /// The commit that was checked out when the stash was made.
    pub base: String,
    pub message: String,
    /// Commits Git creates to hold the stashed index and untracked files.
    pub(crate) helpers: Vec<String>,
}

impl Stash {
    pub fn selector(&self) -> String {
        format!("stash@{{{}}}", self.index)
    }
}

impl Repo {
    pub(crate) fn stashes(&self) -> Result<Vec<Stash>> {
        let output = self.read(&["stash", "list", "-z", "--format=%H%x1f%P%x1f%gs"])?;
        Ok(output
            .split(|byte| *byte == 0)
            .filter(|record| !record.is_empty())
            .enumerate()
            .filter_map(|(index, record)| {
                let record = text(record);
                let mut fields = record.splitn(3, '\u{1f}');
                let id = fields.next()?.trim().to_owned();
                let mut parents = fields.next()?.split_whitespace().map(str::to_owned);
                let base = parents.next()?;
                Some(Stash {
                    index,
                    id,
                    base,
                    helpers: parents.collect(),
                    message: fields.next().unwrap_or_default().trim().to_owned(),
                })
            })
            .collect())
    }

    /// Applies the stash, restoring its staged changes as staged, and drops it
    /// afterwards when `remove` is set. A conflict keeps the stash.
    pub fn apply_stash(&self, stash: &Stash, remove: bool) -> Result<()> {
        self.ensure_stash(stash, RuleError::StaleStash)?;
        if !self.apply_file_stash(&stash.id)? {
            run_write(
                self.git()
                    .args(["stash", "apply", "--index", &stash.selector()]),
            )?;
        }
        if remove {
            // The list may have changed while Git was applying the stash.
            self.ensure_stash(stash, RuleError::StashListChanged)?;
            run_write(self.git().args(["stash", "drop", &stash.selector()]))?;
        }
        Ok(())
    }

    pub fn drop_stash(&self, stash: &Stash) -> Result<()> {
        self.ensure_stash(stash, RuleError::StaleStash)?;
        run_write(self.git().args(["stash", "drop", &stash.selector()]))
    }

    /// Applies and drops the most recent stash.
    pub fn pop_stash(&self) -> Result<()> {
        let latest = self
            .stashes()?
            .into_iter()
            .next()
            .ok_or(RuleError::StaleStash)?;
        self.apply_stash(&latest, true)
    }

    /// Selectors shift when stashes are added or dropped; act only on the
    /// stash the user chose.
    /// Checks the list itself: Git reports a selector past the end of the
    /// list as a failure rather than as a missing reference.
    fn ensure_stash(&self, stash: &Stash, error: RuleError) -> Result<()> {
        let current = self.stashes()?;
        if current
            .get(stash.index)
            .is_some_and(|entry| entry.id == stash.id)
        {
            Ok(())
        } else {
            Err(error.into())
        }
    }
}
