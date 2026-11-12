use ritkraken_diff::{DiffChangeKind as Kind, DiffFile, DiffOverview};

#[test]
fn unchanged_source_has_no_change_markers() {
    assert!(
        DiffOverview::from_files(&[DiffFile::unchanged("same.txt", "one\ntwo\n")])
            .spans()
            .is_empty()
    );
}

#[test]
fn additions_and_replacements_keep_modified_source_line_numbers() {
    let files = DiffFile::parse("diff --git a/file.txt b/file.txt\n--- a/file.txt\n+++ b/file.txt\n@@ -1,4 +1,5 @@\n a\n+new\n b\n-old\n+changed\n c\n").unwrap();
    let overview = DiffOverview::from_files(&files);
    let spans = overview.spans();
    assert_eq!(spans.len(), 2);
    assert_eq!((spans[0].lines(), spans[0].kind()), (2..3, Kind::Added));
    assert_eq!((spans[1].lines(), spans[1].kind()), (4..5, Kind::Modified));
}

#[test]
fn deletions_anchor_at_the_following_modified_line_including_eof() {
    let files =
        DiffFile::parse("--- a/file.txt\n+++ b/file.txt\n@@ -1,3 +1,2 @@\n a\n b\n-c\n").unwrap();
    let overview = DiffOverview::from_files(&files);
    assert_eq!(overview.spans().len(), 1);
    assert_eq!(overview.spans()[0].lines(), 3..4);
    assert_eq!(overview.spans()[0].kind(), Kind::Removed);
}

#[test]
fn deleting_every_line_has_one_deletion_anchor() {
    let files =
        DiffFile::parse("--- a/file.txt\n+++ b/file.txt\n@@ -1,2 +0,0 @@\n-first\n-second\n")
            .unwrap();
    let overview = DiffOverview::from_files(&files);
    assert_eq!(overview.spans().len(), 1);
    assert_eq!(overview.spans()[0].lines(), 1..2);
    assert_eq!(overview.spans()[0].kind(), Kind::Removed);
}

#[test]
fn contiguous_changes_are_retained_as_a_single_span() {
    let mut patch = String::from("--- /dev/null\n+++ b/large.txt\n@@ -0,0 +1,4096 @@\n");
    for line in 0..4096 {
        patch.push_str(&format!("+line {line}\n"));
    }
    let overview = DiffOverview::from_files(&DiffFile::parse(&patch).unwrap());
    assert_eq!(overview.spans().len(), 1);
    assert_eq!(overview.spans()[0].lines(), 1..4097);
    assert_eq!(overview.spans()[0].kind(), Kind::Added);
}

#[test]
fn metadata_from_multiple_files_stays_sorted_and_separate() {
    let files = DiffFile::parse("--- /dev/null\n+++ b/zeta.txt\n@@ -0,0 +1 @@\n+z\n--- /dev/null\n+++ b/alpha.txt\n@@ -0,0 +1 @@\n+a\n").unwrap();
    let overview = DiffOverview::from_files(&files);
    assert_eq!(overview.spans().len(), 2);
    assert_eq!(overview.spans()[0].path().as_ref(), "alpha.txt");
    assert_eq!(overview.spans()[1].path().as_ref(), "zeta.txt");
}
