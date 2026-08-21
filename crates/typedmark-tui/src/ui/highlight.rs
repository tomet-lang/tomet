//! `.tm` syntax highlighting for the Explorer tab's file preview /
//! inline editor pane, with a plain-text fallback for non-`.tm` files
//! and tree-sitter failures. Takes only `Path`/`&str` -- no `App`
//! dependency, not owned by any single tab.

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use std::path::Path;
use tree_sitter::StreamingIterator;

pub(super) fn highlight_source_file(path: &Path, src: &str) -> Vec<Line<'static>> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let is_tm = ext.eq_ignore_ascii_case("tm") || ext.eq_ignore_ascii_case("tmt");

    if is_tm {
        highlight_typedmark_tree_sitter(src)
    } else {
        fallback_typedmark_highlight(src)
    }
}

fn highlight_typedmark_tree_sitter(src: &str) -> Vec<Line<'static>> {
    let mut parser = tree_sitter::Parser::new();
    let language: tree_sitter::Language = tree_sitter_typedmark::LANGUAGE.into();

    if parser.set_language(&language).is_err() {
        return fallback_typedmark_highlight(src);
    }
    let tree = match parser.parse(src, None) {
        Some(t) => t,
        None => return fallback_typedmark_highlight(src),
    };

    let query = match tree_sitter::Query::new(&language, tree_sitter_typedmark::HIGHLIGHTS_QUERY) {
        Ok(q) => q,
        Err(_) => return fallback_typedmark_highlight(src),
    };

    let src_bytes = src.as_bytes();
    let src_len = src_bytes.len();
    let mut byte_styles = vec![Style::default(); src_len];

    let mut cursor = tree_sitter::QueryCursor::new();
    let mut captures = cursor.captures(&query, tree.root_node(), src_bytes);
    while let Some((m, cap_idx)) = captures.next() {
        let capture = m.captures[*cap_idx];
        let cap_name = query.capture_names()[capture.index as usize];
        let style = match cap_name {
            "comment" => Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
            "tag" => Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
            "title" | "markup.heading" => Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
            "string" | "string.special" => Style::default().fg(Color::Green),
            "punctuation.special" | "punctuation.bracket" | "punctuation.delimiter" => {
                Style::default().fg(Color::Yellow)
            }
            "property" => Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
            "constant" | "markup.list.checked" => Style::default().fg(Color::Green),
            "markup.list.unnumbered" | "markup.list.numbered" => Style::default().fg(Color::Blue),
            "text.literal" => Style::default().fg(Color::Gray),
            _ => Style::default(),
        };

        let range = capture.node.start_byte()..capture.node.end_byte();
        for idx in range {
            if idx < src_len {
                byte_styles[idx] = style;
            }
        }
    }

    let mut lines = Vec::new();
    let mut current_pos = 0;

    for line_str in src.lines() {
        let line_len = line_str.len();
        let line_end = current_pos + line_len;

        if line_len == 0 {
            lines.push(Line::from(""));
        } else {
            let mut spans = Vec::new();
            let mut chunk_start = current_pos;
            let mut current_style = byte_styles[current_pos];

            for b_idx in current_pos..line_end {
                let s = byte_styles[b_idx];
                if s != current_style {
                    let chunk_str = &src[chunk_start..b_idx];
                    spans.push(Span::styled(chunk_str.to_string(), current_style));
                    chunk_start = b_idx;
                    current_style = s;
                }
            }
            if chunk_start < line_end {
                let chunk_str = &src[chunk_start..line_end];
                spans.push(Span::styled(chunk_str.to_string(), current_style));
            }
            lines.push(Line::from(spans));
        }

        current_pos = line_end;
        if current_pos < src_len && src_bytes[current_pos] == b'\r' {
            current_pos += 1;
        }
        if current_pos < src_len && src_bytes[current_pos] == b'\n' {
            current_pos += 1;
        }
    }

    lines
}

fn fallback_typedmark_highlight(src: &str) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let mut in_code_block = false;

    for line_str in src.lines() {
        let trimmed = line_str.trim();

        if trimmed.starts_with("```") {
            in_code_block = !in_code_block;
            lines.push(Line::from(Span::styled(
                line_str.to_string(),
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            )));
            continue;
        }

        if in_code_block {
            lines.push(Line::from(Span::styled(
                line_str.to_string(),
                Style::default().fg(Color::Gray),
            )));
            continue;
        }

        if trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with('*') {
            lines.push(Line::from(Span::styled(
                line_str.to_string(),
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::ITALIC),
            )));
            continue;
        }

        if trimmed.starts_with('#') {
            lines.push(Line::from(Span::styled(
                line_str.to_string(),
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            )));
            continue;
        }

        if trimmed.starts_with('@') || trimmed.starts_with('<') {
            lines.push(highlight_typedmark_line(line_str));
            continue;
        }

        if trimmed.starts_with('>') {
            lines.push(Line::from(Span::styled(
                line_str.to_string(),
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::ITALIC),
            )));
            continue;
        }

        if trimmed.starts_with("- ")
            || trimmed.starts_with("* ")
            || (trimmed.len() >= 3
                && trimmed.as_bytes()[0].is_ascii_digit()
                && trimmed.as_bytes()[1] == b'.'
                && trimmed.as_bytes()[2] == b' ')
        {
            let prefix_len = line_str.find(|c: char| c != ' ').unwrap_or(0);
            let indent = &line_str[..prefix_len];
            let rest = &line_str[prefix_len..];

            let (marker, content) = rest.split_at(2);
            lines.push(Line::from(vec![
                Span::raw(indent.to_string()),
                Span::styled(
                    marker.to_string(),
                    Style::default()
                        .fg(Color::Blue)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(content.to_string()),
            ]));
            continue;
        }

        if let Some((k, v)) = line_str.split_once(':') {
            if !k.contains(' ')
                && !k.starts_with("http")
                && (v.trim_start().starts_with('"')
                    || v.trim_start().starts_with('{')
                    || v.trim_start().starts_with('[')
                    || !v.trim().is_empty())
            {
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("{k}:"),
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(v.to_string()),
                ]));
                continue;
            }
        }

        lines.push(Line::from(Span::raw(line_str.to_string())));
    }

    lines
}

fn highlight_typedmark_line(line: &str) -> Line<'static> {
    let mut spans = Vec::new();
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        let ch = chars[i];

        if ch == '@' || ch == '<' || (ch == '$' && i + 1 < len && chars[i + 1] == '{') {
            let start = i;
            if ch == '@' || ch == '<' {
                i += 1;
                while i < len
                    && (chars[i].is_alphanumeric()
                        || chars[i] == '_'
                        || chars[i] == '-'
                        || chars[i] == '>')
                {
                    let end_sigil = chars[i] == '>';
                    i += 1;
                    if end_sigil {
                        break;
                    }
                }
            } else {
                i += 2;
            }
            let text: String = chars[start..i].iter().collect();
            spans.push(Span::styled(
                text,
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ));
            continue;
        }

        if ch == '(' {
            let start = i;
            i += 1;
            let mut depth = 1;
            while i < len && depth > 0 {
                if chars[i] == '(' {
                    depth += 1;
                } else if chars[i] == ')' {
                    depth -= 1;
                }
                i += 1;
            }
            let text: String = chars[start..i].iter().collect();
            spans.push(Span::styled(text, Style::default().fg(Color::Green)));
            continue;
        }

        if ch == '{' {
            let start = i;
            i += 1;
            let mut depth = 1;
            while i < len && depth > 0 {
                if chars[i] == '{' {
                    depth += 1;
                } else if chars[i] == '}' {
                    depth -= 1;
                }
                i += 1;
            }
            let text: String = chars[start..i].iter().collect();
            spans.push(Span::styled(text, Style::default().fg(Color::LightGreen)));
            continue;
        }

        if ch == '"' || ch == '\'' {
            let quote = ch;
            let start = i;
            i += 1;
            while i < len && chars[i] != quote {
                if chars[i] == '\\' && i + 1 < len {
                    i += 1;
                }
                i += 1;
            }
            if i < len {
                i += 1;
            }
            let text: String = chars[start..i].iter().collect();
            spans.push(Span::styled(text, Style::default().fg(Color::Green)));
            continue;
        }

        let start = i;
        while i < len
            && chars[i] != '@'
            && chars[i] != '<'
            && chars[i] != '('
            && chars[i] != '{'
            && chars[i] != '"'
            && chars[i] != '\''
            && !(chars[i] == '$' && i + 1 < len && chars[i + 1] == '{')
        {
            i += 1;
        }
        let text: String = chars[start..i].iter().collect();
        spans.push(Span::raw(text));
    }

    Line::from(spans)
}
