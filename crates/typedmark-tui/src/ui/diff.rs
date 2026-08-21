//! Line/char-level diffing for before/after source previews (Migration/
//! BatchMeta/StructuralGrep tabs). Takes only plain text -- no `App`
//! dependency, not owned by any single tab.

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

pub(super) fn generate_colored_diff(original: &str, modified: &str) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    if original == modified {
        for line in original.lines() {
            lines.push(Line::from(Span::styled(
                format!("  {line}"),
                Style::default().fg(Color::DarkGray),
            )));
        }
        return lines;
    }

    let ops = compute_line_diff(original, modified);
    let mut idx = 0;
    while idx < ops.len() {
        match (&ops[idx], ops.get(idx + 1)) {
            (DiffOp::Delete(old_line), Some(DiffOp::Insert(new_line))) => {
                let (old_spans, new_spans) = compute_char_diff(old_line, new_line);
                lines.push(Line::from(old_spans));
                lines.push(Line::from(new_spans));
                idx += 2;
            }
            (DiffOp::Delete(line), _) => {
                lines.push(Line::from(Span::styled(
                    format!("- {line}"),
                    Style::default().fg(Color::Red),
                )));
                idx += 1;
            }
            (DiffOp::Insert(line), _) => {
                lines.push(Line::from(Span::styled(
                    format!("+ {line}"),
                    Style::default().fg(Color::Green),
                )));
                idx += 1;
            }
            (DiffOp::Keep(line), _) => {
                lines.push(Line::from(Span::styled(
                    format!("  {line}"),
                    Style::default().fg(Color::DarkGray),
                )));
                idx += 1;
            }
        }
    }
    lines
}

fn compute_char_diff(old_line: &str, new_line: &str) -> (Vec<Span<'static>>, Vec<Span<'static>>) {
    let old_chars: Vec<char> = old_line.chars().collect();
    let new_chars: Vec<char> = new_line.chars().collect();
    let n = old_chars.len();
    let m = new_chars.len();

    let mut table = vec![vec![0usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            if old_chars[i] == new_chars[j] {
                table[i][j] = table[i + 1][j + 1] + 1;
            } else {
                table[i][j] = table[i + 1][j].max(table[i][j + 1]);
            }
        }
    }

    let mut old_spans: Vec<Span<'static>> =
        vec![Span::styled("- ", Style::default().fg(Color::Red))];
    let mut new_spans: Vec<Span<'static>> =
        vec![Span::styled("+ ", Style::default().fg(Color::Green))];

    let mut i = 0;
    let mut j = 0;

    let normal_red = Style::default().fg(Color::Red);
    let highlight_red = Style::default()
        .fg(Color::White)
        .bg(Color::Red)
        .add_modifier(Modifier::BOLD);

    let normal_green = Style::default().fg(Color::Green);
    let highlight_green = Style::default()
        .fg(Color::White)
        .bg(Color::Green)
        .add_modifier(Modifier::BOLD);

    let mut cur_old_text = String::new();
    let mut cur_old_hi = false;

    let mut cur_new_text = String::new();
    let mut cur_new_hi = false;

    while i < n && j < m {
        if old_chars[i] == new_chars[j] {
            if cur_old_hi {
                old_spans.push(Span::styled(
                    std::mem::take(&mut cur_old_text),
                    highlight_red,
                ));
                cur_old_hi = false;
            }
            cur_old_text.push(old_chars[i]);

            if cur_new_hi {
                new_spans.push(Span::styled(
                    std::mem::take(&mut cur_new_text),
                    highlight_green,
                ));
                cur_new_hi = false;
            }
            cur_new_text.push(new_chars[j]);

            i += 1;
            j += 1;
        } else if table[i + 1][j] >= table[i][j + 1] {
            if !cur_old_hi && !cur_old_text.is_empty() {
                old_spans.push(Span::styled(std::mem::take(&mut cur_old_text), normal_red));
            }
            cur_old_hi = true;
            cur_old_text.push(old_chars[i]);
            i += 1;
        } else {
            if !cur_new_hi && !cur_new_text.is_empty() {
                new_spans.push(Span::styled(
                    std::mem::take(&mut cur_new_text),
                    normal_green,
                ));
            }
            cur_new_hi = true;
            cur_new_text.push(new_chars[j]);
            j += 1;
        }
    }

    while i < n {
        if !cur_old_hi && !cur_old_text.is_empty() {
            old_spans.push(Span::styled(std::mem::take(&mut cur_old_text), normal_red));
        }
        cur_old_hi = true;
        cur_old_text.push(old_chars[i]);
        i += 1;
    }

    while j < m {
        if !cur_new_hi && !cur_new_text.is_empty() {
            new_spans.push(Span::styled(
                std::mem::take(&mut cur_new_text),
                normal_green,
            ));
        }
        cur_new_hi = true;
        cur_new_text.push(new_chars[j]);
        j += 1;
    }

    if !cur_old_text.is_empty() {
        let style = if cur_old_hi {
            highlight_red
        } else {
            normal_red
        };
        old_spans.push(Span::styled(cur_old_text, style));
    }

    if !cur_new_text.is_empty() {
        let style = if cur_new_hi {
            highlight_green
        } else {
            normal_green
        };
        new_spans.push(Span::styled(cur_new_text, style));
    }

    (old_spans, new_spans)
}

#[derive(Debug, PartialEq, Eq)]
enum DiffOp<'a> {
    Keep(&'a str),
    Delete(&'a str),
    Insert(&'a str),
}

fn compute_line_diff<'a>(old_text: &'a str, new_text: &'a str) -> Vec<DiffOp<'a>> {
    let old_lines: Vec<&str> = old_text.lines().collect();
    let new_lines: Vec<&str> = new_text.lines().collect();
    let n = old_lines.len();
    let m = new_lines.len();

    let mut table = vec![vec![0usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            if old_lines[i] == new_lines[j] {
                table[i][j] = table[i + 1][j + 1] + 1;
            } else {
                table[i][j] = table[i + 1][j].max(table[i][j + 1]);
            }
        }
    }

    let mut ops = Vec::new();
    let mut i = 0;
    let mut j = 0;
    while i < n && j < m {
        if old_lines[i] == new_lines[j] {
            ops.push(DiffOp::Keep(old_lines[i]));
            i += 1;
            j += 1;
        } else if table[i + 1][j] >= table[i][j + 1] {
            ops.push(DiffOp::Delete(old_lines[i]));
            i += 1;
        } else {
            ops.push(DiffOp::Insert(new_lines[j]));
            j += 1;
        }
    }
    while i < n {
        ops.push(DiffOp::Delete(old_lines[i]));
        i += 1;
    }
    while j < m {
        ops.push(DiffOp::Insert(new_lines[j]));
        j += 1;
    }

    ops
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_char_diff_highlights_changes() {
        let (old_spans, new_spans) = compute_char_diff("hello world", "hello brave world");
        assert!(!old_spans.is_empty());
        assert!(!new_spans.is_empty());
        let new_text: String = new_spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(new_text, "+ hello brave world");
    }
}
