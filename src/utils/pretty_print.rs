use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use crate::ui::theme::SyntaxColors;

pub fn pretty_json(input: &str) -> String {
    match serde_json::from_str::<serde_json::Value>(input) {
        Ok(value) => serde_json::to_string_pretty(&value).unwrap_or_else(|_| input.to_string()),
        Err(_) => input.to_string(),
    }
}

/// Converts a JSON string into syntax-highlighted ratatui Lines.
pub fn highlight_json<'a>(input: &str, syntax: &SyntaxColors, fg: Color) -> Vec<Line<'a>> {
    let pretty = pretty_json(input);
    pretty
        .lines()
        .map(|line| highlight_json_line(line, syntax, fg))
        .collect()
}

fn highlight_json_line<'a>(line: &str, syntax: &SyntaxColors, fg: Color) -> Line<'a> {
    let mut spans: Vec<Span<'a>> = Vec::new();
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        let ch = chars[i];
        match ch {
            // Whitespace / indentation
            ' ' | '\t' => {
                let start = i;
                while i < len && (chars[i] == ' ' || chars[i] == '\t') {
                    i += 1;
                }
                let s: String = chars[start..i].iter().collect();
                spans.push(Span::styled(s, Style::default().fg(fg)));
            }
            // String (key or value)
            '"' => {
                let start = i;
                i += 1;
                while i < len && chars[i] != '"' {
                    if chars[i] == '\\' {
                        i += 1; // skip escaped char
                    }
                    i += 1;
                }
                if i < len {
                    i += 1; // closing quote
                }
                let s: String = chars[start..i].iter().collect();

                // Determine if this is a key (followed by ':') or a string value
                let mut j = i;
                while j < len && chars[j] == ' ' {
                    j += 1;
                }
                let is_key = j < len && chars[j] == ':';

                let color = if is_key {
                    syntax.json_key
                } else {
                    syntax.json_string
                };
                spans.push(Span::styled(s, Style::default().fg(color)));
            }
            // Colon
            ':' => {
                spans.push(Span::styled(":".to_string(), Style::default().fg(fg)));
                i += 1;
            }
            // Comma
            ',' => {
                spans.push(Span::styled(",".to_string(), Style::default().fg(fg)));
                i += 1;
            }
            // Braces / Brackets
            '{' | '}' | '[' | ']' => {
                spans.push(Span::styled(ch.to_string(), Style::default().fg(fg)));
                i += 1;
            }
            // Numbers or keywords (true/false/null)
            _ => {
                let start = i;
                while i < len && !matches!(chars[i], ',' | '}' | ']' | ' ' | '\t' | ':') {
                    i += 1;
                }
                let token: String = chars[start..i].iter().collect();
                let color = match token.as_str() {
                    "true" | "false" => syntax.json_boolean,
                    "null" => syntax.json_null,
                    _ => {
                        // Check if it's a number
                        if token.parse::<f64>().is_ok() {
                            syntax.json_number
                        } else {
                            fg
                        }
                    }
                };
                spans.push(Span::styled(token, Style::default().fg(color)));
            }
        }
    }

    if spans.is_empty() {
        Line::from("")
    } else {
        Line::from(spans)
    }
}

pub fn pretty_xml(input: &str) -> String {
    // Simple XML indentation
    let mut result = String::new();
    let mut depth = 0;
    let mut tag_content = String::new();

    for c in input.chars() {
        match c {
            '<' => {
                if !tag_content.trim().is_empty() {
                    result.push_str(&"  ".repeat(depth));
                    result.push_str(tag_content.trim());
                    result.push('\n');
                }
                tag_content.clear();
                tag_content.push(c);
            }
            '>' => {
                tag_content.push(c);

                let tag = tag_content.trim();
                if tag.starts_with("</") {
                    depth = depth.saturating_sub(1);
                    result.push_str(&"  ".repeat(depth));
                } else if tag.ends_with("/>") || tag.starts_with("<?") || tag.starts_with("<!") {
                    result.push_str(&"  ".repeat(depth));
                } else {
                    result.push_str(&"  ".repeat(depth));
                    depth += 1;
                }
                result.push_str(tag);
                result.push('\n');
                tag_content.clear();
            }
            _ => {
                tag_content.push(c);
            }
        }
    }

    if !tag_content.trim().is_empty() {
        result.push_str(tag_content.trim());
    }

    result
}

