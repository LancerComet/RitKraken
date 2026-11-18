use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use ritkraken_git as git;

use super::super::History;

pub(crate) const REBASE_ID_PREFIX: &str = "rebase:";

pub(crate) fn rebase_row_id(oid: &str) -> String {
    format!("{REBASE_ID_PREFIX}{oid}")
}

pub(crate) fn real_oid(id: &str) -> &str {
    id.strip_prefix(REBASE_ID_PREFIX).unwrap_or(id)
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::app) enum RebaseViewMode {
    #[default]
    TreeEdit,
    ResultPreview,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::app) struct PlannedRowMeta {
    pub(in crate::app) entry_index: usize,
    pub(in crate::app) action: git::RebaseAction,
    pub(in crate::app) oid: String,
    pub(in crate::app) message: String,
    pub(in crate::app) folded_count: usize,
    pub(in crate::app) has_squash_message: bool,
    pub(in crate::app) folded_oids: Vec<String>,
}

#[derive(Clone)]
pub(in crate::app) struct ProjectedRebase {
    pub(in crate::app) history: Arc<History>,
    pub(in crate::app) row_meta: HashMap<String, PlannedRowMeta>,
    pub(in crate::app) validation_error: Option<&'static str>,
    pub(in crate::app) kept_count: usize,
    pub(in crate::app) folded_count: usize,
    pub(in crate::app) dropped_count: usize,
    pub(in crate::app) projected_tip: Option<String>,
}

fn subject_of(message: &str) -> String {
    message
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or_default()
        .trim()
        .to_owned()
}

fn is_leader_action(action: git::RebaseAction) -> bool {
    matches!(
        action,
        git::RebaseAction::Pick | git::RebaseAction::Reword | git::RebaseAction::Edit
    )
}

fn is_fold_action(action: git::RebaseAction) -> bool {
    matches!(action, git::RebaseAction::Squash | git::RebaseAction::Fixup)
}

fn validate_plan(plan: &git::RebasePlan) -> Option<&'static str> {
    let mut seen_leader = false;
    for entry in &plan.entries {
        if is_leader_action(entry.action) {
            seen_leader = true;
        } else if is_fold_action(entry.action) && !seen_leader {
            return Some("app.rebase.error_leading_squash");
        }
    }
    for entry in &plan.entries {
        if entry.action == git::RebaseAction::Reword && entry.message.trim().is_empty() {
            return Some("app.rebase.error_empty_message");
        }
    }
    let mut index = 0;
    while index < plan.entries.len() {
        let entry = &plan.entries[index];
        if !is_leader_action(entry.action) {
            index += 1;
            continue;
        }
        let mut followers = Vec::new();
        let mut has_squash = false;
        let mut cursor = index + 1;
        while cursor < plan.entries.len() {
            let follower = &plan.entries[cursor];
            if is_leader_action(follower.action) {
                break;
            }
            if is_fold_action(follower.action) {
                if follower.action == git::RebaseAction::Squash {
                    has_squash = true;
                }
                followers.push((follower.action, follower.message.as_str()));
            }
            cursor += 1;
        }
        if has_squash
            && entry.action != git::RebaseAction::Edit
            && git::compose_squash_message(&entry.message, followers)
                .trim()
                .is_empty()
        {
            return Some("app.rebase.error_empty_message");
        }
        index = cursor;
    }
    None
}

fn collect_retained_original_ids<'a>(
    base_history: &'a History,
    replaced_ids: &HashSet<&'a str>,
) -> HashSet<&'a str> {
    let snapshot = &base_history.snapshot;
    let mut queue = VecDeque::new();
    for reference in &snapshot.refs {
        if !snapshot.is_current_branch(reference) && replaced_ids.contains(reference.id.as_str()) {
            queue.push_back(reference.id.as_str());
        }
    }
    for commit in &snapshot.commits {
        if replaced_ids.contains(commit.id.as_str()) {
            continue;
        }
        for parent in &commit.parents {
            if replaced_ids.contains(parent.as_str()) {
                queue.push_back(parent.as_str());
            }
        }
    }
    let mut retained = HashSet::new();
    while let Some(id) = queue.pop_front() {
        if !replaced_ids.contains(id) || !retained.insert(id) {
            continue;
        }
        let Some(&pos) = base_history.position.get(id) else {
            continue;
        };
        for parent in &snapshot.commits[pos].parents {
            if replaced_ids.contains(parent.as_str()) {
                queue.push_back(parent.as_str());
            }
        }
    }
    retained
}

fn build_preview_rows(
    plan: &git::RebasePlan,
    row_meta: &mut HashMap<String, PlannedRowMeta>,
) -> (Vec<git::Commit>, Option<String>) {
    let mut chronological = Vec::new();
    let mut prev_id = plan.base.clone();
    let mut projected_tip = None;
    let mut index = 0;

    while index < plan.entries.len() {
        let leader = &plan.entries[index];
        if !is_leader_action(leader.action) {
            index += 1;
            continue;
        }

        let mut folded_indices = Vec::new();
        let mut cursor = index + 1;
        while cursor < plan.entries.len() {
            let follower = &plan.entries[cursor];
            if is_leader_action(follower.action) {
                break;
            }
            if is_fold_action(follower.action) {
                folded_indices.push(cursor);
            }
            cursor += 1;
        }

        let has_squash = folded_indices
            .iter()
            .any(|&idx| plan.entries[idx].action == git::RebaseAction::Squash);
        let message = if has_squash {
            git::compose_squash_message(
                &leader.message,
                folded_indices
                    .iter()
                    .map(|&idx| (plan.entries[idx].action, plan.entries[idx].message.as_str())),
            )
        } else {
            leader.message.clone()
        };

        let row_id = rebase_row_id(&leader.commit.id);
        let mut commit = leader.commit.clone();
        commit.id = row_id.clone();
        commit.parents = vec![prev_id.clone()];
        commit.subject = subject_of(&message);

        let folded_oids = folded_indices
            .iter()
            .map(|&idx| plan.entries[idx].commit.id.clone())
            .collect();
        row_meta.insert(
            row_id.clone(),
            PlannedRowMeta {
                entry_index: index,
                action: leader.action,
                oid: leader.commit.id.clone(),
                message,
                folded_count: folded_indices.len(),
                has_squash_message: has_squash,
                folded_oids,
            },
        );
        chronological.push(commit);
        prev_id = row_id.clone();
        projected_tip = Some(row_id);
        index = cursor;
    }

    chronological.reverse();
    (chronological, projected_tip)
}

fn build_tree_edit_rows(
    plan: &git::RebasePlan,
    row_meta: &mut HashMap<String, PlannedRowMeta>,
) -> (Vec<git::Commit>, Option<String>) {
    let mut chronological = Vec::with_capacity(plan.entries.len());
    let mut prev_id = plan.base.clone();
    let mut projected_tip = None;

    for (entry_index, entry) in plan.entries.iter().enumerate() {
        let row_id = rebase_row_id(&entry.commit.id);
        let mut commit = entry.commit.clone();
        commit.id = row_id.clone();
        commit.parents = vec![prev_id.clone()];
        commit.subject = subject_of(&entry.message);

        row_meta.insert(
            row_id.clone(),
            PlannedRowMeta {
                entry_index,
                action: entry.action,
                oid: entry.commit.id.clone(),
                message: entry.message.clone(),
                folded_count: 0,
                has_squash_message: false,
                folded_oids: Vec::new(),
            },
        );
        if entry.action != git::RebaseAction::Drop {
            projected_tip = Some(row_id.clone());
        }
        prev_id = row_id;
        chronological.push(commit);
    }

    chronological.reverse();
    (chronological, projected_tip)
}

pub(in crate::app) fn project_history(
    base_history: &Arc<History>,
    plan: &git::RebasePlan,
    mode: RebaseViewMode,
) -> ProjectedRebase {
    let base_snapshot = &base_history.snapshot;
    let validation_error = validate_plan(plan);
    let mut kept_count = 0;
    let mut folded_count = 0;
    let mut dropped_count = 0;
    for entry in &plan.entries {
        if is_leader_action(entry.action) {
            kept_count += 1;
        } else if is_fold_action(entry.action) {
            folded_count += 1;
        } else {
            dropped_count += 1;
        }
    }

    let mut row_meta = HashMap::new();
    let (mut commits, projected_tip) = match mode {
        RebaseViewMode::TreeEdit => build_tree_edit_rows(plan, &mut row_meta),
        RebaseViewMode::ResultPreview => build_preview_rows(plan, &mut row_meta),
    };

    let replaced_ids: HashSet<&str> = plan
        .entries
        .iter()
        .map(|entry| entry.commit.id.as_str())
        .collect();
    let retained_ids = collect_retained_original_ids(base_history, &replaced_ids);

    for commit in &base_snapshot.commits {
        let id = commit.id.as_str();
        if !replaced_ids.contains(id) || retained_ids.contains(id) {
            commits.push(commit.clone());
        }
    }

    let mut snapshot = base_snapshot.clone();
    let tip_target = projected_tip.clone().unwrap_or_else(|| plan.base.clone());
    snapshot.commits = commits;
    snapshot.head = Some(tip_target.clone());
    for reference in &mut snapshot.refs {
        if base_snapshot.is_current_branch(reference) {
            reference.id = tip_target.clone();
            reference.object_id = tip_target.clone();
        }
    }

    let history = History::projected(
        snapshot,
        base_history.menu.clone(),
        base_history.worktree_summary.clone(),
    );
    ProjectedRebase {
        history,
        row_meta,
        validation_error,
        kept_count,
        folded_count,
        dropped_count,
        projected_tip,
    }
}

pub(in crate::app) fn find_preview_row_for_entry(
    row_meta: &HashMap<String, PlannedRowMeta>,
    entry_index: usize,
    plan: &git::RebasePlan,
) -> Option<String> {
    if let Some(entry) = plan.entries.get(entry_index) {
        let direct = rebase_row_id(&entry.commit.id);
        if row_meta.contains_key(&direct) {
            return Some(direct);
        }
        if is_fold_action(entry.action) {
            for idx in (0..entry_index).rev() {
                if is_leader_action(plan.entries[idx].action) {
                    let candidate = rebase_row_id(&plan.entries[idx].commit.id);
                    if row_meta.contains_key(&candidate) {
                        return Some(candidate);
                    }
                }
            }
        }
        for idx in (0..entry_index).rev() {
            let candidate = rebase_row_id(&plan.entries[idx].commit.id);
            if row_meta.contains_key(&candidate) {
                return Some(candidate);
            }
        }
        for idx in (entry_index + 1)..plan.entries.len() {
            let candidate = rebase_row_id(&plan.entries[idx].commit.id);
            if row_meta.contains_key(&candidate) {
                return Some(candidate);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::changes::ChangeSummary;
    use git::{
        Commit, CommitMenuSnapshot, MenuMetadata, RebaseAction, RebaseEntry, RebasePlan, RefKind,
        Reference, Snapshot, Status,
    };

    fn make_commit(id: &str, parents: &[&str], subject: &str) -> Commit {
        Commit {
            id: id.to_owned(),
            parents: parents.iter().map(|p| (*p).to_owned()).collect(),
            subject: subject.to_owned(),
            author: "Alice".to_owned(),
            author_email: "alice@example.com".to_owned(),
            date: "2026-11-14".to_owned(),
        }
    }

    fn base_history(commits: Vec<Commit>, refs: Vec<Reference>) -> Arc<History> {
        let snapshot = Snapshot {
            branch: Some("feat".to_owned()),
            head: Some(commits.first().map(|c| c.id.clone()).unwrap_or_default()),
            status: Status::default(),
            refs,
            remotes: vec!["origin".to_owned()],
            upstream: None,
            has_stash: false,
            stashes: Vec::new(),
            sequence: None,
            menu_metadata: MenuMetadata::default(),
            commits,
            history_limited: false,
        };
        History::projected(
            snapshot,
            Arc::new(CommitMenuSnapshot::default()),
            ChangeSummary::new(std::iter::empty()),
        )
    }

    fn make_entry(commit: &Commit, action: RebaseAction) -> RebaseEntry {
        RebaseEntry {
            action,
            commit: commit.clone(),
            message: format!("{}\n", commit.subject),
        }
    }

    fn local_ref(name: &str, id: &str, checked_out: bool) -> Reference {
        Reference {
            kind: RefKind::Local,
            name: name.to_owned(),
            id: id.to_owned(),
            object_id: id.to_owned(),
            checked_out,
        }
    }

    fn remote_ref(name: &str, id: &str) -> Reference {
        Reference {
            kind: RefKind::Remote,
            name: name.to_owned(),
            id: id.to_owned(),
            object_id: id.to_owned(),
            checked_out: false,
        }
    }

    #[test]
    fn unpushed_branch_replaces_old_commits_cleanly() {
        let c3 = make_commit("c3", &["c2"], "third");
        let c2 = make_commit("c2", &["c1"], "second");
        let c1 = make_commit("c1", &["base"], "first");
        let base = make_commit("base", &[], "base");
        let refs = vec![
            local_ref("feat", "c3", true),
            local_ref("main", "base", false),
        ];
        let history = base_history(vec![c3.clone(), c2.clone(), c1.clone(), base], refs);
        let plan = RebasePlan {
            base: "base".to_owned(),
            upstream: "base".to_owned(),
            entries: vec![
                make_entry(&c1, RebaseAction::Pick),
                make_entry(&c2, RebaseAction::Reword),
                make_entry(&c3, RebaseAction::Pick),
            ],
        };

        let projected = project_history(&history, &plan, RebaseViewMode::TreeEdit);
        assert_eq!(projected.validation_error, None);
        let ids: Vec<&str> = projected
            .history
            .snapshot
            .commits
            .iter()
            .map(|c| c.id.as_str())
            .collect();
        assert_eq!(ids, vec!["rebase:c3", "rebase:c2", "rebase:c1", "base"]);
        assert_eq!(
            projected.history.snapshot.head.as_deref(),
            Some("rebase:c3")
        );
        assert_eq!(projected.history.snapshot.refs[0].id, "rebase:c3");
        assert_eq!(projected.history.snapshot.refs[1].id, "base");
    }

    #[test]
    fn pushed_origin_branch_keeps_old_commits_on_sibling_lane() {
        let c2 = make_commit("c2", &["c1"], "second");
        let c1 = make_commit("c1", &["base"], "first");
        let base = make_commit("base", &[], "base");
        let refs = vec![
            local_ref("feat", "c2", true),
            remote_ref("origin/feat", "c2"),
        ];
        let history = base_history(vec![c2.clone(), c1.clone(), base], refs);
        let plan = RebasePlan {
            base: "base".to_owned(),
            upstream: "base".to_owned(),
            entries: vec![
                make_entry(&c1, RebaseAction::Pick),
                make_entry(&c2, RebaseAction::Pick),
            ],
        };

        let projected = project_history(&history, &plan, RebaseViewMode::TreeEdit);
        let ids: Vec<&str> = projected
            .history
            .snapshot
            .commits
            .iter()
            .map(|c| c.id.as_str())
            .collect();
        assert_eq!(ids, vec!["rebase:c2", "rebase:c1", "c2", "c1", "base"]);
        assert_eq!(projected.history.snapshot.refs[0].id, "rebase:c2");
        assert_eq!(projected.history.snapshot.refs[1].id, "c2");
    }

    #[test]
    fn result_preview_folds_squash_and_fixup_downward_and_skips_drop() {
        let c4 = make_commit("c4", &["c3"], "dropped");
        let c3 = make_commit("c3", &["c2"], "fixup typo");
        let c2 = make_commit("c2", &["c1"], "squash part");
        let c1 = make_commit("c1", &["base"], "leader");
        let base = make_commit("base", &[], "base");
        let refs = vec![local_ref("feat", "c4", true)];
        let history = base_history(
            vec![c4.clone(), c3.clone(), c2.clone(), c1.clone(), base],
            refs,
        );
        let plan = RebasePlan {
            base: "base".to_owned(),
            upstream: "base".to_owned(),
            entries: vec![
                make_entry(&c1, RebaseAction::Pick),
                make_entry(&c2, RebaseAction::Squash),
                make_entry(&c3, RebaseAction::Fixup),
                make_entry(&c4, RebaseAction::Drop),
            ],
        };

        let preview = project_history(&history, &plan, RebaseViewMode::ResultPreview);
        let ids: Vec<&str> = preview
            .history
            .snapshot
            .commits
            .iter()
            .map(|c| c.id.as_str())
            .collect();
        assert_eq!(ids, vec!["rebase:c1", "base"]);
        let meta = preview.row_meta.get("rebase:c1").expect("leader meta");
        assert_eq!(meta.folded_count, 2);
        assert!(meta.has_squash_message);
        assert!(meta.message.contains("leader"));
        assert!(meta.message.contains("squash part"));
        assert!(!meta.message.contains("fixup typo"));
    }

    #[test]
    fn top_drop_shifts_branch_pill_downward() {
        let c2 = make_commit("c2", &["c1"], "drop me");
        let c1 = make_commit("c1", &["base"], "keep me");
        let base = make_commit("base", &[], "base");
        let refs = vec![local_ref("feat", "c2", true)];
        let history = base_history(vec![c2.clone(), c1.clone(), base], refs);
        let plan = RebasePlan {
            base: "base".to_owned(),
            upstream: "base".to_owned(),
            entries: vec![
                make_entry(&c1, RebaseAction::Pick),
                make_entry(&c2, RebaseAction::Drop),
            ],
        };

        let projected = project_history(&history, &plan, RebaseViewMode::TreeEdit);
        assert_eq!(
            projected.history.snapshot.head.as_deref(),
            Some("rebase:c1")
        );
        assert_eq!(projected.history.snapshot.refs[0].id, "rebase:c1");
    }

    #[test]
    fn leading_squash_reports_validation_error() {
        let c1 = make_commit("c1", &["base"], "first");
        let base = make_commit("base", &[], "base");
        let history = base_history(vec![c1.clone(), base], Vec::new());
        let plan = RebasePlan {
            base: "base".to_owned(),
            upstream: "base".to_owned(),
            entries: vec![make_entry(&c1, RebaseAction::Squash)],
        };

        let projected = project_history(&history, &plan, RebaseViewMode::TreeEdit);
        assert_eq!(
            projected.validation_error,
            Some("app.rebase.error_leading_squash")
        );
    }
}
