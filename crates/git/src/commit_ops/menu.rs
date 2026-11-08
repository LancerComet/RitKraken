use std::{collections::HashMap, fs, io, path::Path, sync::Arc};

use super::{
    CommitMenuState,
    topology::{RelationInfo, read_relations, visible_relations},
};
use crate::{
    Commit, Repo, Result, Snapshot,
    protocol::{path_from_bytes, without_final_newline},
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct GraphVersion {
    references: Vec<u8>,
    shallow: Option<Vec<u8>>,
    grafts: Option<Vec<u8>>,
}

impl GraphVersion {
    pub(crate) fn read(references: Vec<u8>, directory: &Path) -> Result<Self> {
        let common = match optional_file(&directory.join("commondir"))? {
            Some(bytes) => directory.join(path_from_bytes(without_final_newline(&bytes))),
            None => directory.to_path_buf(),
        };
        Ok(Self {
            references,
            shallow: optional_file(&common.join("shallow"))?,
            grafts: optional_file(&common.join("info").join("grafts"))?,
        })
    }
}

fn optional_file(path: &Path) -> Result<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

#[derive(Clone, Debug, Default)]
pub struct MenuMetadata {
    pub(crate) version: GraphVersion,
    pub(crate) remote_url: Option<String>,
    pub(crate) editing_head: Option<String>,
}

#[derive(Debug, Eq, PartialEq)]
struct MenuKey {
    version: GraphVersion,
    head: Option<String>,
    visible: Vec<String>,
}

/// Immutable relationship facts prepared along with repository history. Only
/// visible rows are retained; a status-only refresh reuses the same index.
#[derive(Debug)]
pub struct CommitMenuSnapshot {
    key: MenuKey,
    relations: HashMap<String, RelationInfo>,
}

impl Repo {
    pub fn commit_menu_snapshot(
        &self,
        snapshot: &Snapshot,
        previous: Option<&Arc<CommitMenuSnapshot>>,
    ) -> Result<Arc<CommitMenuSnapshot>> {
        let key = MenuKey {
            version: snapshot.menu_metadata.version.clone(),
            head: snapshot.head.clone(),
            visible: snapshot
                .commits
                .iter()
                .map(|commit| commit.id.clone())
                .collect(),
        };
        if let Some(previous) = previous.filter(|previous| previous.key == key) {
            return Ok(previous.clone());
        }
        let relations = match snapshot.head.as_deref() {
            Some(head) if !snapshot.commits.is_empty() => {
                match visible_relations(head, &snapshot.commits, &key.visible) {
                    Some(relations) => relations,
                    None => read_relations(self, head, &key.visible)?,
                }
            }
            _ => HashMap::new(),
        };
        Ok(Arc::new(CommitMenuSnapshot { key, relations }))
    }
}

impl CommitMenuSnapshot {
    /// Pure in-memory construction: opening a menu does not read files or run Git.
    pub fn state(&self, snapshot: &Snapshot, commit: &Commit) -> Option<CommitMenuState> {
        let info = self.relations.get(&commit.id)?;
        let head = snapshot.head.clone()?;
        let tracked_remote = snapshot
            .upstream
            .as_ref()
            .and_then(crate::Upstream::tracking_name);
        Some(CommitMenuState {
            commit: commit.clone(),
            head: head.clone(),
            branch: snapshot.branch.clone(),
            relation: info.relation,
            linear_rebase: info.linear,
            references: snapshot
                .refs
                .iter()
                .filter(|reference| reference.id == commit.id)
                .cloned()
                .collect(),
            dirty: !snapshot.status.is_empty(),
            conflicted: snapshot.status.has_conflicts(),
            staged_count: snapshot.status.staged.len(),
            sequence: snapshot.sequence,
            editing_rebase_commit: snapshot.menu_metadata.editing_head.as_deref()
                == Some(head.as_str()),
            remote_url: snapshot.menu_metadata.remote_url.clone(),
            remote_names: snapshot.remotes.clone(),
            tracked_remote,
        })
    }
}
