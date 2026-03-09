use similar::{ChangeTag, TextDiff};

use crate::core::response::Response;

#[derive(Debug, Clone)]
pub struct DiffResult {
    pub lines: Vec<DiffLine>,
    pub additions: usize,
    pub deletions: usize,
    pub unchanged: usize,
}

#[derive(Debug, Clone)]
pub struct DiffLine {
    pub tag: DiffTag,
    pub content: String,
    pub old_line: Option<usize>,
    pub new_line: Option<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DiffTag {
    Equal,
    Insert,
    Delete,
}

pub fn diff_responses(left: &Response, right: &Response) -> DiffResult {
    let left_text = left.body_text().unwrap_or("");
    let right_text = right.body_text().unwrap_or("");
    diff_text(left_text, right_text)
}

pub fn diff_text(left: &str, right: &str) -> DiffResult {
    let diff = TextDiff::from_lines(left, right);
    let mut lines = Vec::new();
    let mut additions = 0;
    let mut deletions = 0;
    let mut unchanged = 0;
    let mut old_line = 1usize;
    let mut new_line = 1usize;

    for change in diff.iter_all_changes() {
        let (tag, old_ln, new_ln) = match change.tag() {
            ChangeTag::Equal => {
                unchanged += 1;
                let result = (DiffTag::Equal, Some(old_line), Some(new_line));
                old_line += 1;
                new_line += 1;
                result
            }
            ChangeTag::Insert => {
                additions += 1;
                let result = (DiffTag::Insert, None, Some(new_line));
                new_line += 1;
                result
            }
            ChangeTag::Delete => {
                deletions += 1;
                let result = (DiffTag::Delete, Some(old_line), None);
                old_line += 1;
                result
            }
        };

        lines.push(DiffLine {
            tag,
            content: change.to_string_lossy().trim_end_matches('\n').to_string(),
            old_line: old_ln,
            new_line: new_ln,
        });
    }

    DiffResult {
        lines,
        additions,
        deletions,
        unchanged,
    }
}
