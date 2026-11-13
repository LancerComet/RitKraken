use ritkraken_diff::{DiffFile, DiffSearch, DiffSide};

#[test]
fn identical_context_is_one_result_with_source_byte_offsets() {
    let mut search = DiffSearch::new(&[DiffFile::unchanged(
        "a.rs",
        "before\n\t中文 needle needle\n",
    )]);
    search.set_query("needle", false);
    assert_eq!(search.matches().len(), 2);
    assert_eq!(search.matches()[0].position.line(), 2);
    assert_eq!(search.matches()[0].range, 8..14);
    assert_eq!(search.matches()[1].range, 15..21);
}

#[test]
fn removed_and_added_versions_are_both_searchable() {
    let files = DiffFile::parse(
        "--- a/a.txt\n+++ b/a.txt\n@@ -20,2 +20,2 @@\n context\n-old needle\n+new needle\n",
    )
    .unwrap();
    let mut search = DiffSearch::new(&files);
    search.set_query("needle", false);
    let hits = search.matches();
    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].position.side(), DiffSide::Original);
    assert_eq!(hits[1].position.side(), DiffSide::Modified);
    assert!(hits.iter().all(|hit| hit.position.line() == 21));
    search.set_query("context", false);
    assert_eq!(search.matches().len(), 1);
}

#[test]
fn literal_search_and_case_mode_are_independent_of_layout() {
    let mut search = DiffSearch::new(&[DiffFile::unchanged("a", "[Aa] A.a a.a 中文\n")]);
    search.set_query("a.a", false);
    assert_eq!(search.matches().len(), 2);
    search.set_query("a.a", true);
    assert_eq!(search.matches().len(), 1);
    search.set_query("中文", false);
    assert_eq!(search.matches().len(), 1);
    assert_eq!(search.matches()[0].range, 13..19);
}

#[test]
fn navigation_wraps_in_both_directions() {
    let mut search = DiffSearch::new(&[DiffFile::unchanged("a", "hit\nhit\nhit\n")]);
    search.set_query("hit", false);
    assert_eq!(search.label(), "1/3");
    assert_eq!(search.next_match(true).unwrap().position.line(), 3);
    assert_eq!(search.label(), "3/3");
    assert_eq!(search.next_match(false).unwrap().position.line(), 1);
    assert_eq!(search.next_match(false).unwrap().position.line(), 2);
    search.set_query("hit", false);
    assert_eq!(search.label(), "2/3");
}

#[test]
fn no_match_and_empty_query_clear_previous_results() {
    let mut search = DiffSearch::new(&[DiffFile::unchanged("a", "hit\n")]);
    for query in ["hit", "absent", "", "hit\n"] {
        search.set_query(query, false);
        assert_eq!(search.matches().len(), usize::from(query == "hit"));
    }
    assert_eq!(search.label(), "0/0");
    assert!(search.next_match(true).is_none());
}

#[test]
fn multiple_files_and_lines_beyond_folded_context_are_indexed() {
    let first = DiffFile::unchanged("first", "hit\n");
    let second = DiffFile::unchanged("second", &format!("{}hit\n", "unchanged\n".repeat(2000)));
    let mut search = DiffSearch::new(&[first, second]);
    search.set_query("hit", false);
    assert_eq!(search.matches().len(), 2);
    assert_eq!(search.matches()[1].position.path().as_str(), "second");
    assert_eq!(search.matches()[1].position.line(), 2001);
}

#[test]
fn empty_and_binary_documents_have_no_matches() {
    let mut search = DiffSearch::new(&[]);
    search.set_query("anything", false);
    assert!(search.matches().is_empty());
    let files = DiffFile::parse(
        "diff --git a/image.png b/image.png\nBinary files a/image.png and b/image.png differ\n",
    )
    .unwrap();
    let mut search = DiffSearch::new(&files);
    search.set_query("png", false);
    assert!(search.matches().is_empty());
}

#[test]
fn refreshing_text_keeps_the_current_occurrence_within_a_line() {
    let mut old = DiffSearch::new(&[DiffFile::unchanged("a", "hithit\nsecond hit\n")]);
    old.set_query("hit", false);
    let anchor = old.next_match(false).unwrap().clone();
    let mut refreshed =
        DiffSearch::new(&[DiffFile::unchanged("a", "hithit\nsecond hit\nnew hit\n")]);
    refreshed.set_query("hit", false);
    refreshed.restore_current(&anchor);
    assert_eq!(refreshed.label(), "2/4");
    assert_eq!(refreshed.current().unwrap().range, 3..6);
    assert_eq!(refreshed.next_match(false).unwrap().position.line(), 2);
}

#[test]
fn refreshing_a_removed_match_stays_in_its_file_and_handles_zero_results() {
    let mut old = DiffSearch::new(&[
        DiffFile::unchanged("a", "hit\n"),
        DiffFile::unchanged("b", "text\nhit\n"),
    ]);
    old.set_query("hit", false);
    let anchor = old.next_match(false).unwrap().clone();
    let mut refreshed = DiffSearch::new(&[
        DiffFile::unchanged("b", "text\nremoved\nhit\n"),
        DiffFile::unchanged("a", "hit\n"),
    ]);
    refreshed.set_query("hit", false);
    refreshed.restore_current(&anchor);
    let current = refreshed.current().unwrap();
    assert_eq!(current.position.path().as_str(), "b");
    assert_eq!(current.position.line(), 3);
    refreshed.set_query("absent", false);
    refreshed.restore_current(&anchor);
    assert!(refreshed.current().is_none());
}
