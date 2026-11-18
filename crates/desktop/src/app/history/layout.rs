use std::{
    collections::{BTreeSet, HashMap, HashSet},
    ops::Range,
};

use ritkraken_git::Commit;

const COLORS: &[u32] = &[
    0x079dbb, 0x1676e8, 0x8e24c7, 0xb823b0, 0xd42677, 0xdc3c37, 0xc17a17, 0x188c64, 0x6075cd,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Rail {
    pub lane: usize,
    pub color: u32,
    pub color_index: usize,
    /// Drawn dotted: the link from a stash to the commit it was made on.
    pub dashed: bool,
}

#[derive(Debug)]
pub(super) struct Row {
    pub node: Rail,
    pub incoming: Vec<Rail>,
    pub outgoing: Vec<Rail>,
}

#[derive(Debug)]
pub(super) struct GraphLayout {
    pub rows: Vec<Row>,
    pub worktree: Option<Row>,
    // Each lane stores sorted, disjoint spans for the rows between its endpoints.
    lanes: Vec<Vec<Span>>,
}

impl GraphLayout {
    pub(super) fn new(commits: &[Commit], stashes: &HashSet<&str>) -> Self {
        Self::build(commits, false, None, stashes)
    }

    pub(super) fn with_worktree(
        commits: &[Commit],
        head: Option<&str>,
        stashes: &HashSet<&str>,
    ) -> Self {
        Self::build(commits, true, head, stashes)
    }

    fn build(
        commits: &[Commit],
        worktree: bool,
        head: Option<&str>,
        stashes: &HashSet<&str>,
    ) -> Self {
        let mut lanes = Lanes::default();
        let mut pending: HashMap<&str, Vec<Track>> = HashMap::new();
        let mut rows = Vec::with_capacity(commits.len());

        // Treat WIP as a virtual child of HEAD. Its lane stays reserved through
        // every newer commit, instead of joining whichever branch is listed first.
        let worktree = worktree.then(|| {
            let node = lanes.reserve(0);
            if let Some(head) = head {
                pending.entry(head).or_default().push(Track {
                    rail: node,
                    start: None,
                });
            } else {
                lanes.free.insert(node.lane);
            }
            Row {
                node,
                incoming: Vec::new(),
                outgoing: head.map(|_| node).into_iter().collect(),
            }
        });

        for (index, commit) in commits.iter().enumerate() {
            let mut incoming: Vec<_> = pending
                .remove(commit.id.as_str())
                .into_iter()
                .flatten()
                .map(|track| {
                    lanes.record(track, index);
                    track.rail
                })
                .collect();
            incoming.sort_unstable_by_key(|rail| rail.lane);
            let stash = stashes.contains(commit.id.as_str());
            // A dotted stash link ends here; the commit keeps a solid lane of
            // its own. Stashes start right of the first lane so the main line
            // stays straight.
            let node = incoming
                .iter()
                .find(|rail| !rail.dashed)
                .copied()
                .unwrap_or_else(|| lanes.reserve(usize::from(stash)));

            let outgoing: Vec<_> = commit
                .parents
                .iter()
                .enumerate()
                .map(|(parent_index, parent)| {
                    let rail = if parent_index == 0 {
                        Rail {
                            dashed: stash,
                            ..node
                        }
                    } else {
                        lanes.reserve(node.lane + 1)
                    };
                    pending.entry(parent).or_default().push(Track {
                        rail,
                        start: Some(index),
                    });
                    rail
                })
                .collect();

            // Keep incoming branch lanes reserved through this row's junction.
            lanes.free.extend(
                incoming
                    .iter()
                    .filter(|rail| rail.lane != node.lane)
                    .map(|rail| rail.lane),
            );
            if outgoing.is_empty() {
                lanes.free.insert(node.lane);
            }
            rows.push(Row {
                node,
                incoming,
                outgoing,
            });
        }

        for track in pending.into_values().flatten() {
            lanes.record(track, commits.len());
        }
        Self {
            rows,
            worktree,
            lanes: lanes.spans,
        }
    }

    pub(super) fn lane_count(&self) -> usize {
        self.lanes.len()
    }

    pub(super) fn through(
        &self,
        row: usize,
        lanes: Range<usize>,
    ) -> impl Iterator<Item = Rail> + '_ {
        self.lanes[lanes.clone()]
            .iter()
            .zip(lanes)
            .filter_map(move |(spans, lane)| {
                let span = spans.get(spans.partition_point(|span| span.rows.end <= row))?;
                span.rows.contains(&row).then_some(Rail {
                    lane,
                    color: span.color,
                    color_index: span.color_index,
                    dashed: span.dashed,
                })
            })
    }
}

#[derive(Clone, Copy)]
struct Track {
    rail: Rail,
    start: Option<usize>,
}

#[derive(Debug)]
struct Span {
    rows: Range<usize>,
    color: u32,
    color_index: usize,
    dashed: bool,
}

#[derive(Default)]
struct Lanes {
    spans: Vec<Vec<Span>>,
    free: BTreeSet<usize>,
    next_color: usize,
}

impl Lanes {
    fn reserve(&mut self, start: usize) -> Rail {
        let lane = if let Some(lane) = self.free.range(start..).next().copied() {
            self.free.remove(&lane);
            lane
        } else {
            // Lanes left of `start` that do not exist yet stay free for later rows.
            while self.spans.len() < start {
                self.free.insert(self.spans.len());
                self.spans.push(Vec::new());
            }
            self.spans.push(Vec::new());
            self.spans.len() - 1
        };
        let color_index = self.next_color;
        let color = COLORS[color_index];
        self.next_color = (self.next_color + 1) % COLORS.len();
        Rail {
            lane,
            color,
            color_index,
            dashed: false,
        }
    }

    fn record(&mut self, track: Track, end: usize) {
        let rows = track.start.map_or(0, |start| start + 1)..end;
        if !rows.is_empty() {
            self.spans[track.rail.lane].push(Span {
                rows,
                color: track.rail.color,
                color_index: track.rail.color_index,
                dashed: track.rail.dashed,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commit(id: &str, parents: &[&str]) -> Commit {
        Commit {
            id: id.into(),
            parents: parents.iter().map(|parent| (*parent).into()).collect(),
            author: String::new(),
            author_email: String::new(),
            date: String::new(),
            subject: String::new(),
        }
    }

    #[test]
    fn sibling_branches_stay_separate_until_their_common_parent() {
        let mut commits: Vec<_> = (0..6)
            .map(|index| commit(&format!("tip-{index}"), &["base"]))
            .collect();
        commits.push(commit("base", &[]));
        let graph = GraphLayout::new(&commits, &HashSet::new());
        let tips: Vec<_> = graph.rows[..6].iter().map(|row| row.node).collect();

        assert_eq!(graph.lane_count(), 6);
        for (index, &tip) in tips.iter().enumerate() {
            assert_eq!(tip.lane, index);
            assert_eq!(tip.color, COLORS[index]);
            assert_eq!(graph.rows[index].outgoing, [tip]);
            assert_eq!(
                graph.through(index, 0..6).collect::<Vec<_>>(),
                tips[..index]
            );
        }
        assert_eq!(graph.rows[6].node, tips[0]);
        assert_eq!(graph.rows[6].incoming, tips);
        assert!(graph.through(6, 0..6).next().is_none());
    }

    #[test]
    fn merge_edges_reach_their_own_parent_rows() {
        let graph = GraphLayout::new(
            &[
                commit("merge", &["main", "topic"]),
                commit("main", &["root"]),
                commit("topic", &["root"]),
                commit("root", &[]),
                commit("unrelated", &[]),
            ],
            &HashSet::new(),
        );

        let rows = &graph.rows;
        assert_eq!(graph.lane_count(), 2);
        assert_eq!(rows[0].outgoing, [rows[1].node, rows[2].node]);
        assert_eq!(graph.through(1, 0..2).collect::<Vec<_>>(), [rows[2].node]);
        assert_eq!(graph.through(2, 0..2).collect::<Vec<_>>(), [rows[1].node]);
        assert_eq!(rows[3].incoming, [rows[1].node, rows[2].node]);
        assert_eq!(rows[4].node.lane, 0);
        assert!(rows[4].incoming.is_empty());
    }

    #[test]
    fn loading_older_rows_preserves_the_existing_tracks() {
        let commits = [
            commit("one", &["base"]),
            commit("two", &["base"]),
            commit("base", &[]),
        ];
        let partial = GraphLayout::new(&commits[..2], &HashSet::new());
        let full = GraphLayout::new(&commits, &HashSet::new());

        for index in 0..partial.rows.len() {
            assert_eq!(partial.rows[index].node, full.rows[index].node);
            assert_eq!(partial.rows[index].outgoing, full.rows[index].outgoing);
            assert_eq!(
                partial.through(index, 0..2).collect::<Vec<_>>(),
                full.through(index, 0..2).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn worktree_bypasses_newer_commits_and_joins_its_actual_head() {
        let graph = GraphLayout::with_worktree(
            &[commit("master", &["base"]), commit("base", &[])],
            Some("base"),
            &HashSet::new(),
        );
        let worktree = graph.worktree.as_ref().unwrap();
        assert_eq!(worktree.outgoing, [worktree.node]);
        assert_ne!(worktree.node.lane, graph.rows[0].node.lane);
        assert!(graph.rows[0].incoming.is_empty());
        assert_eq!(
            graph.through(0, 0..graph.lane_count()).collect::<Vec<_>>(),
            [worktree.node]
        );
        assert_eq!(graph.rows[1].node, worktree.node);
        assert!(graph.rows[1].incoming.contains(&worktree.node));
        assert!(graph.rows[1].incoming.contains(&graph.rows[0].node));
    }

    #[test]
    fn worktree_at_the_newest_head_has_one_direct_connection() {
        let graph = GraphLayout::with_worktree(
            &[commit("head", &["base"]), commit("base", &[])],
            Some("head"),
            &HashSet::new(),
        );
        let worktree = graph.worktree.as_ref().unwrap();
        assert_eq!(graph.rows[0].incoming, [worktree.node]);
        assert_eq!(graph.rows[0].node, worktree.node);
        assert_eq!(graph.lane_count(), 1);
    }

    #[test]
    fn worktree_connection_survives_loading_a_head_outside_loaded_history() {
        let commits = [
            commit("newer", &["middle"]),
            commit("middle", &["head"]),
            commit("head", &[]),
        ];
        let partial = GraphLayout::with_worktree(&commits[..1], Some("head"), &HashSet::new());
        let full = GraphLayout::with_worktree(&commits, Some("head"), &HashSet::new());
        let worktree = partial.worktree.as_ref().unwrap().node;
        assert_eq!(partial.rows[0].node, full.rows[0].node);
        assert!(
            partial
                .through(0, 0..partial.lane_count())
                .any(|rail| rail == worktree)
        );
        assert!(
            full.through(1, 0..full.lane_count())
                .any(|rail| rail == worktree)
        );
        assert!(full.rows[2].incoming.contains(&worktree));
    }

    #[test]
    fn unborn_worktree_has_no_invented_commit_connection() {
        let graph = GraphLayout::with_worktree(&[], None, &HashSet::new());
        let worktree = graph.worktree.as_ref().unwrap();
        assert!(worktree.outgoing.is_empty());
        assert!(graph.rows.is_empty());
        assert!(GraphLayout::new(&[], &HashSet::new()).worktree.is_none());
    }

    #[test]
    fn stash_links_are_dotted_only_down_to_their_base_commit() {
        let commits = [
            commit("stash", &["base"]),
            commit("base", &["root"]),
            commit("root", &[]),
        ];
        let graph = GraphLayout::new(&commits, &HashSet::from(["stash"]));
        let [stash, base, root] = &graph.rows[..] else {
            panic!("three rows");
        };
        assert_ne!(stash.node.lane, 0, "the main line keeps the first lane");
        assert!(stash.outgoing.iter().all(|rail| rail.dashed));
        assert!(base.incoming.iter().any(|rail| rail.dashed));
        assert!(!base.node.dashed);
        assert_eq!(base.node.lane, 0);
        assert!(base.outgoing.iter().all(|rail| !rail.dashed));
        assert!(!root.node.dashed);
    }
}
