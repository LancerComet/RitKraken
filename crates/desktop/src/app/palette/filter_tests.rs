use ritkraken_git::{FileTarget, Scope};

use super::{
    model::{Candidate, Command, Target, filter},
    tests::{change, reference, snapshot},
};

#[test]
fn filter_matches_all_tokens_across_title_and_details_without_case() {
    let candidates = [
        Candidate {
            title: "Fix parser".into(),
            details: "Alice ABC123".into(),
            target: Target::Commit("abc123".into()),
        },
        Candidate {
            title: "Parser".into(),
            details: "Bob".into(),
            target: Target::Commit("def456".into()),
        },
    ];
    for (query, expected) in [
        ("  PARSER alice ", vec![0]),
        ("abc123 FIX", vec![0]),
        ("parser", vec![0, 1]),
        ("parser missing", vec![]),
        ("", vec![0, 1]),
    ] {
        assert_eq!(filter(&candidates, query), expected, "{query}");
    }
}

#[test]
fn prefixes_restrict_results_to_the_requested_kind() {
    let snapshot = snapshot();
    let candidates = [
        Candidate {
            title: "shared".into(),
            details: String::new(),
            target: Target::Command(Command::Refresh),
        },
        Candidate {
            title: "shared".into(),
            details: String::new(),
            target: Target::Branch(ritkraken_git::RefRequest::checkout(
                &snapshot,
                reference("feature", false),
            )),
        },
        Candidate {
            title: "shared".into(),
            details: String::new(),
            target: Target::Commit("id".into()),
        },
        Candidate {
            title: "shared".into(),
            details: String::new(),
            target: Target::File(FileTarget {
                scope: Scope::Worktree,
                change: change("shared", b' '),
            }),
        },
    ];
    for (query, expected) in [
        ("> shared", vec![0]),
        ("@shared", vec![1]),
        ("# SHARED", vec![2]),
        ("/shared", vec![3]),
        ("/", vec![3]),
        ("shared", vec![0, 1, 2, 3]),
    ] {
        assert_eq!(filter(&candidates, query), expected, "{query}");
    }
}
