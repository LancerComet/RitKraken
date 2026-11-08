use std::collections::HashMap;

use super::CommitRelation;
use crate::{
    Commit, Repo, Result, RuleError,
    command::{run_with_input, without_optional_locks},
};

#[derive(Clone, Copy, Debug)]
pub(super) struct RelationInfo {
    pub relation: CommitRelation,
    pub linear: bool,
}

struct Node<'a> {
    id: &'a str,
    parents: Vec<&'a str>,
}

pub(super) fn visible_relations(
    head: &str,
    commits: &[Commit],
    visible: &[String],
) -> Option<HashMap<String, RelationInfo>> {
    // Reachability proven inside a topological prefix remains valid when older
    // rows are omitted. An absent common ancestor is not proof of unrelated
    // histories, so unresolved pairs use one batched metadata read instead.
    let nodes: Vec<_> = commits
        .iter()
        .map(|commit| Node {
            id: commit.id.as_str(),
            parents: commit.parents.iter().map(String::as_str).collect(),
        })
        .collect();
    let (relations, complete) = classify(&nodes, head, visible).ok()?;
    complete.then_some(relations)
}

pub(super) fn read_relations(
    repo: &Repo,
    head: &str,
    visible: &[String],
) -> Result<HashMap<String, RelationInfo>> {
    let mut input = Vec::new();
    for id in std::iter::once(head).chain(visible.iter().map(String::as_str)) {
        if !matches!(id.len(), 40 | 64) || !id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(RuleError::InvalidCommit.into());
        }
        input.extend_from_slice(id.as_bytes());
        input.push(b'\n');
    }
    let mut git = repo.git();
    git.args(["rev-list", "--topo-order", "--parents", "--stdin"]);
    let bytes = run_with_input(without_optional_locks(&mut git), &[&input])?;
    let text = std::str::from_utf8(&bytes).map_err(|_| RuleError::InvalidCommit)?;
    let nodes: Vec<_> = text
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_ascii_whitespace();
            Some(Node {
                id: fields.next()?,
                parents: fields.collect(),
            })
        })
        .collect();
    let (relations, complete) = classify(&nodes, head, visible)?;
    if !complete {
        return Err(RuleError::InvalidCommit.into());
    }
    Ok(relations)
}

fn classify(
    nodes: &[Node<'_>],
    head: &str,
    visible: &[String],
) -> Result<(HashMap<String, RelationInfo>, bool)> {
    let positions: HashMap<_, _> = nodes
        .iter()
        .enumerate()
        .map(|(index, node)| (node.id, index))
        .collect();
    let Some(&head_index) = positions.get(head) else {
        return Ok((HashMap::new(), false));
    };
    let mut ancestors = vec![false; nodes.len()];
    let mut pending = vec![head_index];
    while let Some(index) = pending.pop() {
        if std::mem::replace(&mut ancestors[index], true) {
            continue;
        }
        pending.extend(
            nodes[index]
                .parents
                .iter()
                .filter_map(|parent| positions.get(parent).copied()),
        );
    }
    // rev-list's topological order is child-first. In reverse order all parent
    // facts are ready, allowing both masks to be built in O(commits + edges).
    let mut descendants = vec![false; nodes.len()];
    let mut related = vec![false; nodes.len()];
    let mut complete_ancestry = vec![false; nodes.len()];
    for (index, node) in nodes.iter().enumerate().rev() {
        complete_ancestry[index] = node.parents.iter().all(|parent| {
            positions
                .get(parent)
                .is_some_and(|&parent| complete_ancestry[parent])
        });
        descendants[index] = index == head_index
            || node
                .parents
                .iter()
                .filter_map(|parent| positions.get(parent))
                .any(|&parent| descendants[parent]);
        related[index] = ancestors[index]
            || node
                .parents
                .iter()
                .filter_map(|parent| positions.get(parent))
                .any(|&parent| related[parent]);
    }
    let mut linear = vec![false; nodes.len()];
    let mut cursor = head_index;
    while nodes[cursor].parents.len() == 1 {
        let Some(&parent) = positions.get(nodes[cursor].parents[0]) else {
            break;
        };
        if nodes[parent].parents.len() > 1 {
            break;
        }
        linear[parent] = true;
        cursor = parent;
    }
    let mut complete = true;
    let relations = visible
        .iter()
        .map(|id| {
            let &index = positions.get(id.as_str()).ok_or(RuleError::InvalidCommit)?;
            let relation = if index == head_index {
                CommitRelation::Head
            } else if ancestors[index] {
                CommitRelation::Ancestor
            } else if descendants[index] {
                CommitRelation::Descendant
            } else if related[index] {
                CommitRelation::Diverged
            } else {
                complete &= complete_ancestry[index] && complete_ancestry[head_index];
                CommitRelation::Unrelated
            };
            Ok((
                id.clone(),
                RelationInfo {
                    relation,
                    linear: linear[index],
                },
            ))
        })
        .collect::<Result<HashMap<_, _>>>()?;
    Ok((relations, complete))
}
