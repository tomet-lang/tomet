use lsp_types::{Position, Range, Uri};
use tomet_ast::Span;
use tomet_lexer::LineIndex;

/// Converts between byte offsets into one document's text and LSP positions
/// (a 0-based line and a UTF-16 character offset within it).
///
/// Spans and parse errors carry byte offsets only. Build one of these per
/// request, from the text the request is about, and pass it down: building it
/// is one pass over the text, and every conversion after that is cheap.
pub struct TextPositions<'a> {
    text: &'a str,
    index: LineIndex<'a>,
}

impl<'a> TextPositions<'a> {
    pub fn new(text: &'a str) -> Self {
        TextPositions {
            text,
            index: LineIndex::new(text),
        }
    }

    /// The same text's line index, for callers that need lines and columns in
    /// `char`s rather than LSP units.
    pub fn index(&self) -> &LineIndex<'a> {
        &self.index
    }

    /// The LSP position of byte `offset`.
    pub fn position(&self, offset: usize) -> Position {
        let offset = self.index.clamp(offset);
        let line = self.index.line_col(offset).line;
        let start = self.index.line_start(line).unwrap_or(0);
        let character = self.text[start..offset].encode_utf16().count();
        Position::new((line - 1) as u32, character as u32)
    }

    /// The LSP range of an AST [`Span`].
    pub fn range(&self, span: &Span) -> Range {
        Range::new(self.position(span.start), self.position(span.end))
    }

    /// The LSP range of a CST [`tomet_cst::TextRange`].
    pub fn text_range(&self, range: tomet_cst::TextRange) -> Range {
        Range::new(
            self.position(usize::from(range.start())),
            self.position(usize::from(range.end())),
        )
    }

    /// The end of the line `line` (0-based), before its line break. A line
    /// past the end of the text resolves to the end of the text.
    pub fn line_end(&self, line: u32) -> Position {
        let end = match self.index.line_start(line as usize + 2) {
            Some(next) => next.saturating_sub(1),
            None => self.text.len(),
        };
        self.position(end)
    }

    /// The byte offset of an LSP position. A character past the end of its
    /// line resolves to the end of the line; a line past the end of the text
    /// resolves to the end of the text.
    pub fn offset(&self, pos: Position) -> usize {
        let Some(start) = self.index.line_start(pos.line as usize + 1) else {
            return self.text.len();
        };
        let line = self.text[start..].split('\n').next().unwrap_or("");
        let mut units = 0u32;
        for (i, c) in line.char_indices() {
            if units >= pos.character {
                return start + i;
            }
            units += c.len_utf16() as u32;
        }
        start + line.len()
    }

    /// The 0-based line of byte `offset`.
    pub fn line(&self, offset: usize) -> u32 {
        self.position(offset).line
    }
}

/// Whether byte `offset` falls within an AST [`Span`]. A dummy span (a node
/// the parser did not give a location) contains nothing.
pub(crate) fn span_contains(span: &Span, offset: usize) -> bool {
    !span.is_dummy() && span.contains_offset(offset)
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
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16)
        {
            out.push(b);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

pub(crate) fn get_line_prefix(text: &str, pos: Position) -> &str {
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
