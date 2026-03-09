use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Widget};

use crate::testing::diff::{DiffResult, DiffTag};
use crate::ui::theme::Theme;

pub fn render_diff_viewer(diff: &DiffResult, theme: &Theme, area: Rect, buf: &mut Buffer) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_set(theme.border_set())
        .title(format!(
            " Diff: +{} -{} ~{} ",
            diff.additions, diff.deletions, diff.unchanged
        ))
        .title_style(
            Style::default()
                .fg(theme.colors.accent)
                .add_modifier(Modifier::BOLD),
        )
        .border_style(theme.focused_border_style());

    let inner = block.inner(area);
    block.render(area, buf);

    // Split into two columns for side-by-side
    let columns =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(inner);

    // Left column (old)
    let left_block = Block::default()
        .borders(Borders::RIGHT)
        .title(" Old ")
        .title_style(theme.muted_style());
    let left_inner = left_block.inner(columns[0]);
    left_block.render(columns[0], buf);

    // Right column (new)
    let right_block = Block::default()
        .title(" New ")
        .title_style(theme.muted_style());
    let right_inner = right_block.inner(columns[1]);
    right_block.render(columns[1], buf);

    let mut left_lines = Vec::new();
    let mut right_lines = Vec::new();

    for line in &diff.lines {
        match line.tag {
            DiffTag::Equal => {
                let ln = line.old_line.unwrap_or(0);
                left_lines.push(Line::from(vec![
                    Span::styled(format!("{ln:4} "), theme.muted_style()),
                    Span::raw(&line.content),
                ]));
                right_lines.push(Line::from(vec![
                    Span::styled(
                        format!("{:4} ", line.new_line.unwrap_or(0)),
                        theme.muted_style(),
                    ),
                    Span::raw(&line.content),
                ]));
            }
            DiffTag::Delete => {
                let ln = line.old_line.unwrap_or(0);
                left_lines.push(Line::from(vec![
                    Span::styled(format!("{ln:4}-"), Style::default().fg(theme.colors.error)),
                    Span::styled(&line.content, Style::default().fg(theme.colors.error)),
                ]));
                right_lines.push(Line::raw(""));
            }
            DiffTag::Insert => {
                left_lines.push(Line::raw(""));
                let ln = line.new_line.unwrap_or(0);
                right_lines.push(Line::from(vec![
                    Span::styled(
                        format!("{ln:4}+"),
                        Style::default().fg(theme.colors.success),
                    ),
                    Span::styled(&line.content, Style::default().fg(theme.colors.success)),
                ]));
            }
        }
    }

    let left_para = Paragraph::new(left_lines);
    left_para.render(left_inner, buf);

    let right_para = Paragraph::new(right_lines);
    right_para.render(right_inner, buf);
}
