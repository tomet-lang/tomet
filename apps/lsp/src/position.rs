use lsp_types::{Position, Range, Uri};
use tomet_ast::Span;

/// Converts an AST [`Span`] to an LSP [`Range`].
pub fn span_to_range(span: &Span) -> Range {
    let start_line = span.start.line.saturating_sub(1) as u32;
    let start_col = span.start.column.saturating_sub(1) as u32;
    let end_line = span.end.line.saturating_sub(1) as u32;
    let end_col = span.end.column.saturating_sub(1) as u32;
    Range::new(
        Position::new(start_line, start_col),
        Position::new(end_line, end_col),
    )
}

/// Checks if a 1-indexed `(line, col)` falls within an AST [`Span`].
pub(crate) fn span_contains(span: &Span, line: usize, col: usize) -> bool {
    if span.start.line == 0 || span.end.line == 0 {
        return false;
    }
    let start_ok = line > span.start.line || (line == span.start.line && col >= span.start.column);
    let end_ok = line < span.end.line || (line == span.end.line && col <= span.end.column);
    start_ok && end_ok
}

/// Converts a CST [`tomet_cst::TextRange`] to an LSP [`Range`].
pub fn text_range_to_lsp_range(text: &str, range: tomet_cst::TextRange) -> Range {
    let start_offset = usize::from(range.start());
    let end_offset = usize::from(range.end());
    let start_pos = offset_to_position(text, start_offset);
    let end_pos = offset_to_position(text, end_offset);
    Range::new(start_pos, end_pos)
}

pub(crate) fn offset_to_position(text: &str, offset: usize) -> Position {
    let mut line = 0;
    let mut col = 0;
    for (i, c) in text.char_indices() {
        if i >= offset {
            break;
        }
        if c == '\n' {
            line += 1;
            col = 0;
        } else {
            col += c.len_utf16() as u32;
        }
    }
    Position::new(line, col)
}

pub(crate) fn whole_document_range(text: &str) -> Range {
    let lines: Vec<&str> = text.split('\n').collect();
    let end_line = lines.len().saturating_sub(1) as u32;
    let end_char = lines.last().map(|l| l.chars().count()).unwrap_or(0) as u32;
    Range::new(Position::new(0, 0), Position::new(end_line, end_char))
}

pub(crate) fn uri_to_file_path(u: &Uri) -> Option<std::path::PathBuf> {
    let s = u.as_str();
    let stripped = s.strip_prefix("file://")?;
    let decoded = percent_decode_str(stripped);
    Some(std::path::PathBuf::from(decoded))
}

pub(crate) fn percent_decode_str(s: &str) -> String {
    let mut out = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

pub(crate) fn get_line_prefix<'a>(text: &'a str, pos: Position) -> &'a str {
    let line = text.lines().nth(pos.line as usize).unwrap_or("");
    let target_utf16 = pos.character as usize;
    let mut current_utf16 = 0;
    let mut byte_offset = line.len();

    for (b_idx, c) in line.char_indices() {
        if current_utf16 >= target_utf16 {
            byte_offset = b_idx;
            break;
        }
        current_utf16 += c.len_utf16();
    }
    &line[..byte_offset]
}
