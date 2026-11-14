use ritkraken_diff::{DiffConflictResolution as Choice, DiffFile};

#[test]
fn partial_choices_preserve_unresolved_markers_and_manual_edits() {
    let text = "manual edit\n<<<<<<< HEAD\nleft\n=======\nright\n>>>>>>> source\nbetween\n<<<<<<< HEAD\nours\n=======\ntheirs\n>>>>>>> source\nend\n";
    let file = DiffFile::parse_conflicts("file.txt", text).unwrap();
    assert_eq!(file.conflicts().len(), 2);
    let partial = file
        .conflict_output(&[Some(Choice::Incoming), None])
        .unwrap();
    assert_eq!(
        partial,
        "manual edit\nright\nbetween\n<<<<<<< HEAD\nours\n=======\ntheirs\n>>>>>>> source\nend\n"
    );
    let remaining = DiffFile::parse_conflicts("file.txt", &partial).unwrap();
    assert_eq!(
        remaining.conflict_output(&[Some(Choice::Both)]).unwrap(),
        "manual edit\nright\nbetween\nours\ntheirs\nend\n"
    );
}

#[test]
fn choices_preserve_unicode_crlf_diff3_empty_sides_and_no_final_newline() {
    for (text, expected) in [
        (
            "<<<<<<< HEAD\r\n当前\r\n||||||| base\r\n原本\r\n=======\r\n传入\r\n>>>>>>> source\r\n末尾",
            "当前\r\n传入\r\n末尾",
        ),
        ("<<<<<<< HEAD\n=======\n新建\n>>>>>>> source", "新建\n"),
    ] {
        let file = DiffFile::parse_conflicts("資料.txt", text).unwrap();
        assert_eq!(
            file.conflict_output(&[Some(Choice::Both)]).unwrap(),
            expected
        );
        assert_eq!(file.conflict_output(&[None]).unwrap(), text);
    }
}
