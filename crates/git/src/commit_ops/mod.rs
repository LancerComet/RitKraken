mod context;
mod execute;
mod inspect;
pub(crate) mod menu;
mod rebase;
mod topology;

use crate::{Commit, Reference};

pub use execute::PreparedCommitOperation;
pub use menu::{CommitMenuSnapshot, MenuMetadata};
pub use rebase::{
    GitEditor, RebaseAction, RebaseEntry, RebasePlan, compose_squash_message, run_git_editor,
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SequenceProgress {
    pub current_step: Option<usize>,
    pub total_steps: Option<usize>,
    pub branch: Option<String>,
    pub onto: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommitRelation {
    Head,
    Ancestor,
    Descendant,
    Diverged,
    Unrelated,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SequenceKind {
    Merge,
    Rebase,
    CherryPick,
    Revert,
    Other,
}

impl SequenceKind {
    pub fn can_skip(self) -> bool {
        matches!(self, Self::Rebase | Self::CherryPick | Self::Revert)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResetMode {
    Soft,
    Mixed,
    Hard,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommitAction {
    CreateBranch,
    CreateTag,
    Switch,
    Checkout,
    CherryPick,
    Revert,
    Merge,
    Rebase,
    InteractiveRebase,
    Amend,
    Reset,
    Continue,
    Skip,
    Abort,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommitBlock {
    Dirty,
    Conflict,
    InProgress,
}

/// Both the menu and the execution path use this eligibility result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommitAvailability {
    Hidden,
    Enabled,
    Disabled(CommitBlock),
}

#[derive(Clone, Debug)]
pub struct CommitMenuState {
    pub commit: Commit,
    pub head: String,
    pub branch: Option<String>,
    pub relation: CommitRelation,
    pub references: Vec<Reference>,
    pub dirty: bool,
    pub conflicted: bool,
    pub staged_count: usize,
    pub sequence: Option<SequenceKind>,
    pub editing_rebase_commit: bool,
    pub linear_rebase: bool,
    pub remote_url: Option<String>,
    pub remote_names: Vec<String>,
    pub tracked_remote: Option<String>,
}

#[derive(Clone, Debug)]
pub struct CommitContext {
    pub menu: CommitMenuState,
    pub message: String,
    pub(crate) staged_version: Vec<u8>,
}

impl std::ops::Deref for CommitContext {
    type Target = CommitMenuState;
    fn deref(&self) -> &Self::Target {
        &self.menu
    }
}

impl CommitMenuState {
    pub fn ensure_available(&self, action: CommitAction) -> crate::Result<()> {
        match self.availability(action) {
            CommitAvailability::Enabled => Ok(()),
            CommitAvailability::Hidden => Err(crate::RuleError::CommitUnavailable.into()),
            CommitAvailability::Disabled(reason) => Err(match reason {
                CommitBlock::Dirty => crate::RuleError::DirtyWorktree,
                CommitBlock::Conflict => crate::RuleError::UnresolvedConflicts,
                CommitBlock::InProgress => crate::RuleError::SequenceInProgress,
            }
            .into()),
        }
    }

    pub fn same_checkout(&self, other: &Self) -> bool {
        self.commit.id == other.commit.id
            && self.head == other.head
            && self.branch == other.branch
            && self.sequence == other.sequence
    }

    pub fn remote_branch_name<'a>(&self, name: &'a str) -> &'a str {
        crate::remote_branch(name, &self.remote_names)
            .map(|(_, branch)| branch)
            .unwrap_or_else(|| name.split_once('/').map_or(name, |(_, branch)| branch))
    }

    pub fn checkout_references(&self) -> impl Iterator<Item = &Reference> {
        self.references
            .iter()
            .filter(|reference| match reference.kind {
                crate::RefKind::Tag => false,
                crate::RefKind::Local => Some(&reference.name) != self.branch.as_ref(),
                crate::RefKind::Remote => {
                    !(self.relation == CommitRelation::Head
                        && Some(&reference.name) == self.tracked_remote.as_ref())
                        && !self.references.iter().any(|local| {
                            local.kind == crate::RefKind::Local
                                && local.name == self.remote_branch_name(&reference.name)
                        })
                }
            })
    }

    pub fn availability(&self, action: CommitAction) -> CommitAvailability {
        use CommitAction::*;
        use CommitAvailability::*;
        use CommitRelation::*;
        let local = self.branch.is_some();
        let visible = match action {
            CreateBranch | CreateTag => true,
            Switch => self.checkout_references().next().is_some(),
            Checkout => self.relation != Head,
            CherryPick => local && self.relation != Head,
            Revert => local && matches!(self.relation, Head | Ancestor),
            Merge => local && matches!(self.relation, Descendant | Diverged),
            Rebase => local && self.relation == Diverged,
            InteractiveRebase => {
                local && matches!(self.relation, Ancestor | Diverged) && self.linear_rebase
            }
            Amend => (local || self.editing_rebase_commit) && self.relation == Head,
            Reset => local && self.relation != Head,
            Continue | Abort => self
                .sequence
                .is_some_and(|kind| kind != SequenceKind::Other),
            Skip => self.sequence.is_some_and(SequenceKind::can_skip),
        };
        if !visible {
            return Hidden;
        }
        if matches!(action, Continue | Skip | Abort) {
            return if action == Continue && self.conflicted {
                Disabled(CommitBlock::Conflict)
            } else {
                Enabled
            };
        }
        if self.sequence.is_some() && !(action == Amend && self.editing_rebase_commit) {
            return Disabled(CommitBlock::InProgress);
        }
        if self.conflicted {
            return Disabled(CommitBlock::Conflict);
        }
        if self.dirty
            && matches!(
                action,
                CherryPick | Revert | Merge | Rebase | InteractiveRebase
            )
        {
            return Disabled(CommitBlock::Dirty);
        }
        Enabled
    }
}

#[derive(Clone, Debug)]
pub enum CommitOperation {
    CreateBranch {
        name: String,
        checkout: bool,
        track: Option<String>,
    },
    CreateTag(String),
    Switch(String),
    Checkout,
    CherryPick {
        mainline: Option<usize>,
    },
    Revert {
        mainline: Option<usize>,
    },
    Merge,
    Rebase,
    InteractiveRebase(RebasePlan),
    Amend {
        message: String,
        include_staged: bool,
    },
    Reset(ResetMode),
    Continue,
    Skip,
    Abort,
}

impl CommitOperation {
    pub fn action(&self) -> CommitAction {
        match self {
            Self::CreateBranch { .. } => CommitAction::CreateBranch,
            Self::CreateTag(_) => CommitAction::CreateTag,
            Self::Switch(_) => CommitAction::Switch,
            Self::Checkout => CommitAction::Checkout,
            Self::CherryPick { .. } => CommitAction::CherryPick,
            Self::Revert { .. } => CommitAction::Revert,
            Self::Merge => CommitAction::Merge,
            Self::Rebase => CommitAction::Rebase,
            Self::InteractiveRebase(_) => CommitAction::InteractiveRebase,
            Self::Amend { .. } => CommitAction::Amend,
            Self::Reset(_) => CommitAction::Reset,
            Self::Continue => CommitAction::Continue,
            Self::Skip => CommitAction::Skip,
            Self::Abort => CommitAction::Abort,
        }
    }
}
