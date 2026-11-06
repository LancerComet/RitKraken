use std::collections::HashMap;

use ritkraken_git::{Commit, Reference, Snapshot};

pub const COLORS: &[u32] = &[
    0x1597b0, 0x2f78e4, 0x9c55cc, 0x31a66d, 0xe19036, 0xcf5578, 0x6573cb, 0x208e8a,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rail {
    pub lane: usize,
    pub color: u32,
}

#[derive(Debug)]
pub struct Row {
    pub node: Rail,
    pub incoming: bool,
    pub through: Vec<Rail>,
    pub parents: Vec<Rail>,
}

#[derive(Debug)]
pub struct History {
    pub snapshot: Snapshot,
    pub rows: Vec<Row>,
    pub position: HashMap<String, usize>,
    pub labels: HashMap<String, Vec<usize>>,
    pub width: usize,
}

impl History {
    pub fn new(snapshot: Snapshot) -> Self {
        let (rows, width) = layout(&snapshot.commits);
        let position = snapshot
            .commits
            .iter()
            .enumerate()
            .map(|(index, commit)| (commit.id.clone(), index))
            .collect();
        let mut labels: HashMap<String, Vec<usize>> = HashMap::new();
        for (index, reference) in snapshot.refs.iter().enumerate() {
            labels.entry(reference.id.clone()).or_default().push(index);
        }
        Self {
            snapshot,
            rows,
            position,
            labels,
            width,
        }
    }

    pub fn labels_for(&self, id: &str) -> impl Iterator<Item = &Reference> {
        self.labels
            .get(id)
            .into_iter()
            .flatten()
            .map(|index| &self.snapshot.refs[*index])
    }
}

#[derive(Clone, Copy)]
struct Lane<'a> {
    id: &'a str,
    color: u32,
}

fn layout(commits: &[Commit]) -> (Vec<Row>, usize) {
    let mut lanes: Vec<Option<Lane<'_>>> = Vec::new();
    let mut color_index = 0;
    let mut next_color = || {
        let color = COLORS[color_index % COLORS.len()];
        color_index += 1;
        color
    };
    let mut width = 1;
    let mut rows = Vec::with_capacity(commits.len());
    for commit in commits {
        let incoming_lane = find(&lanes, &commit.id);
        let lane = incoming_lane.unwrap_or_else(|| free(&mut lanes, 0));
        let color = lanes[lane]
            .take()
            .map_or_else(&mut next_color, |old| old.color);
        let node = Rail { lane, color };
        let through = lanes
            .iter()
            .enumerate()
            .filter_map(|(lane, waiting)| {
                waiting.map(|waiting| Rail {
                    lane,
                    color: waiting.color,
                })
            })
            .collect();
        let mut parents = Vec::with_capacity(commit.parents.len());
        for (index, parent) in commit.parents.iter().enumerate() {
            let rail = if let Some(lane) = find(&lanes, parent) {
                Rail {
                    lane,
                    color: lanes[lane].expect("active lane").color,
                }
            } else {
                let parent_lane = if index == 0 {
                    lane
                } else {
                    free(&mut lanes, lane + 1)
                };
                let parent_color = if index == 0 { color } else { next_color() };
                lanes[parent_lane] = Some(Lane {
                    id: parent,
                    color: parent_color,
                });
                Rail {
                    lane: parent_lane,
                    color: parent_color,
                }
            };
            parents.push(rail);
        }
        width = width.max(lanes.len());
        while matches!(lanes.last(), Some(None)) {
            lanes.pop();
        }
        rows.push(Row {
            node,
            incoming: incoming_lane.is_some(),
            through,
            parents,
        });
    }
    (rows, width)
}

fn find(lanes: &[Option<Lane<'_>>], id: &str) -> Option<usize> {
    lanes
        .iter()
        .position(|waiting| waiting.is_some_and(|waiting| waiting.id == id))
}

fn free(lanes: &mut Vec<Option<Lane<'_>>>, start: usize) -> usize {
    if let Some(offset) = lanes[start..].iter().position(Option::is_none) {
        start + offset
    } else {
        lanes.push(None);
        lanes.len() - 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_parent_rejoins_the_existing_lane() {
        let commit = |id: &str, parents: &[&str]| Commit {
            id: id.into(),
            parents: parents.iter().map(|parent| (*parent).into()).collect(),
            author: String::new(),
            date: String::new(),
            subject: String::new(),
        };
        let (rows, width) = layout(&[
            commit("merge", &["main", "topic"]),
            commit("main", &["root"]),
            commit("topic", &["root"]),
            commit("root", &[]),
        ]);

        assert_eq!(width, 2);
        assert_eq!(rows[0].parents[0], rows[1].node);
        assert_eq!(rows[0].parents[1], rows[2].node);
        assert_eq!(rows[1].parents[0], rows[3].node);
        assert_eq!(rows[2].parents[0], rows[3].node);
        assert_eq!(rows[2].through, vec![rows[3].node]);
    }
}
