use hitt::testing::diff::{diff_text, DiffTag};

#[test]
fn identical_text_no_changes() {
    let text = "line one\nline two\nline three\n";
    let result = diff_text(text, text);
    assert_eq!(result.additions, 0);
    assert_eq!(result.deletions, 0);
    assert!(result.unchanged > 0);
}

#[test]
fn added_lines() {
    let left = "line one\nline two\n";
    let right = "line one\nline two\nline three\n";
    let result = diff_text(left, right);
    assert!(result.additions > 0);
    assert_eq!(result.deletions, 0);
    // Verify an Insert tag exists
    assert!(result.lines.iter().any(|l| l.tag == DiffTag::Insert));
}

#[test]
fn removed_lines() {
    let left = "line one\nline two\nline three\n";
    let right = "line one\nline two\n";
    let result = diff_text(left, right);
    assert_eq!(result.additions, 0);
    assert!(result.deletions > 0);
    assert!(result.lines.iter().any(|l| l.tag == DiffTag::Delete));
}

#[test]
fn mixed_changes() {
    let left = "alpha\nbeta\ngamma\n";
    let right = "alpha\ndelta\ngamma\nepsilon\n";
    let result = diff_text(left, right);
    assert!(result.additions > 0);
    assert!(result.deletions > 0);
}

#[test]
fn empty_inputs() {
    let result = diff_text("", "");
    assert_eq!(result.additions, 0);
    assert_eq!(result.deletions, 0);
    assert_eq!(result.unchanged, 0);
}

#[test]
fn left_empty_all_additions() {
    let result = diff_text("", "new line\n");
    assert!(result.additions > 0);
    assert_eq!(result.deletions, 0);
}

#[test]
fn right_empty_all_deletions() {
    let result = diff_text("old line\n", "");
    assert_eq!(result.additions, 0);
    assert!(result.deletions > 0);
}
