use std::{
    collections::{BTreeSet, HashMap},
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
    // Each lane stores sorted, disjoint spans for the rows between its endpoints.
    lanes: Vec<Vec<Span>>,
}

impl GraphLayout {
    pub(super) fn new(commits: &[Commit]) -> Self {
        let mut lanes = Lanes::default();
        let mut pending: HashMap<&str, Vec<Track>> = HashMap::new();
        let mut rows = Vec::with_capacity(commits.len());

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
            let node = incoming
                .first()
                .copied()
                .unwrap_or_else(|| lanes.reserve(0));

            let outgoing: Vec<_> = commit
                .parents
                .iter()
                .enumerate()
                .map(|(parent_index, parent)| {
                    let rail = if parent_index == 0 {
                        node
                    } else {
                        lanes.reserve(node.lane + 1)
                    };
                    pending
                        .entry(parent)
                        .or_default()
                        .push(Track { rail, start: index });
                    rail
                })
                .collect();

            // Keep incoming branch lanes reserved through this row's junction.
            lanes
                .free
                .extend(incoming.iter().skip(1).map(|rail| rail.lane));
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
                })
            })
    }
}

#[derive(Clone, Copy)]
struct Track {
    rail: Rail,
    start: usize,
}

#[derive(Debug)]
struct Span {
    rows: Range<usize>,
    color: u32,
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
            self.spans.push(Vec::new());
            self.spans.len() - 1
        };
        let color = COLORS[self.next_color];
        self.next_color = (self.next_color + 1) % COLORS.len();
        Rail { lane, color }
    }

    fn record(&mut self, track: Track, end: usize) {
        let rows = track.start + 1..end;
        if !rows.is_empty() {
            self.spans[track.rail.lane].push(Span {
                rows,
                color: track.rail.color,
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
        let graph = GraphLayout::new(&commits);
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
        let graph = GraphLayout::new(&[
            commit("merge", &["main", "topic"]),
            commit("main", &["root"]),
            commit("topic", &["root"]),
            commit("root", &[]),
            commit("unrelated", &[]),
        ]);

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
        let partial = GraphLayout::new(&commits[..2]);
        let full = GraphLayout::new(&commits);

        for index in 0..partial.rows.len() {
            assert_eq!(partial.rows[index].node, full.rows[index].node);
            assert_eq!(partial.rows[index].outgoing, full.rows[index].outgoing);
            assert_eq!(
                partial.through(index, 0..2).collect::<Vec<_>>(),
                full.through(index, 0..2).collect::<Vec<_>>()
            );
        }
    }
}
