//! What the CST parser and lowering share about running text: how a run of source
//! becomes `Text` and `SoftBreak` items, trimming at the edges of an inline
//! sequence, and the probes that say where a formatting delimiter (`*em*`, `_em_`,
//! `**strong**`, `__strong__`, `~~strikeout~~`) or an autolink begins and ends.

use tomet_ast::{Inline, SoftBreak, Text};
use tomet_lexer::Cursor;
use tomet_span::Span;

/// Trims leading/trailing whitespace from a finished inline sequence.
///
/// A leading or trailing [`Inline::SoftBreak`] is whitespace by definition
/// (see its doc comment), so it is dropped outright, the same as an
/// all-whitespace edge `Text`. The two can alternate -- e.g. a run that
/// starts `"  \nfoo"` flushes as `[Text("  "), SoftBreak, Text("foo")]` --
/// so each side loops until it hits real content, matching what folding the
/// whole edge into one string and calling `trim_start`/`trim_end` on it used
/// to do in one step.
pub(crate) fn trim_edges(mut items: Vec<Inline>) -> Vec<Inline> {
    loop {
        match items.first() {
            Some(Inline::SoftBreak(_)) => {
                items.remove(0);
            }
            Some(Inline::Text(t)) => {
                let trimmed = t.value.trim_start();
                if trimmed.is_empty() {
                    items.remove(0);
                } else {
                    if trimmed.len() != t.value.len() {
                        let trimmed = trimmed.to_string();
                        if let Some(Inline::Text(t)) = items.first_mut() {
                            t.value = trimmed;
                        }
                    }
                    break;
                }
            }
            _ => break,
        }
    }
    loop {
        match items.last() {
            Some(Inline::SoftBreak(_)) => {
                items.pop();
            }
            Some(Inline::Text(t)) => {
                let trimmed = t.value.trim_end();
                if trimmed.is_empty() {
                    items.pop();
                } else {
                    if trimmed.len() != t.value.len() {
                        let trimmed = trimmed.to_string();
                        if let Some(Inline::Text(t)) = items.last_mut() {
                            t.value = trimmed;
                        }
                    }
                    break;
                }
            }
            _ => break,
        }
    }
    items
}

/// The inline delimiters and the element each one makes, longest first so
/// `**` is tried before `*`.
pub(crate) const DELIMITERS: [(&str, &str); 5] = [
    ("**", "strong"),
    ("__", "strong"),
    ("~~", "strikeout"),
    ("*", "em"),
    ("_", "em"),
];

pub(crate) fn char_before(cur: &Cursor) -> Option<char> {
    cur.src()[..cur.pos()].chars().next_back()
}

pub(crate) fn is_boundary(c: Option<char>) -> bool {
    matches!(c, None | Some(' ') | Some('\t') | Some('\n') | Some('\r'))
}

/// Where a delimiter could close a span, and where a paragraph ends, found once for a text.
///
/// Whether an opener has a closer is then a lookup. It used to be a scan to the end of the paragraph
/// for every opener, so a long paragraph with many unmatched `*word`s took time quadratic in its
/// length.
pub(crate) struct DelimiterIndex {
    /// The byte offset of every `\n` that is followed by nothing but blanks up to another `\n` or the
    /// end: the line break that ends a paragraph.
    paragraph_ends: Vec<usize>,
    /// For each of [`DELIMITERS`], every byte offset where it could close a span: it stands there and
    /// is not right after a blank (`~~` may be).
    closers: [Vec<usize>; DELIMITERS.len()],
}

impl DelimiterIndex {
    pub(crate) fn new(src: &str) -> Self {
        let bytes = src.as_bytes();
        let blank = |b: u8| matches!(b, b' ' | b'\t' | b'\n' | b'\r');
        let mut paragraph_ends = Vec::new();
        for (i, &b) in bytes.iter().enumerate() {
            if b != b'\n' {
                continue;
            }
            let mut j = i + 1;
            while j < bytes.len() && matches!(bytes[j], b' ' | b'\t') {
                j += 1;
            }
            if j >= bytes.len() || bytes[j] == b'\n' {
                paragraph_ends.push(i);
            }
        }
        let closers = std::array::from_fn(|d| {
            let (delim, _) = DELIMITERS[d];
            let delim = delim.as_bytes();
            (0..bytes.len())
                .filter(|&q| bytes[q..].starts_with(delim))
                .filter(|&q| delim == b"~~" || (q > 0 && !blank(bytes[q - 1])))
                .collect()
        });
        Self {
            paragraph_ends,
            closers,
        }
    }

    /// Whether the delimiter `DELIMITERS[d]`, once opened (the opener ends at `open_end`), has a closer
    /// before its paragraph ends.
    fn has_closer(&self, d: usize, open_end: usize) -> bool {
        let limit = self
            .paragraph_ends
            .get(self.paragraph_ends.partition_point(|&e| e < open_end))
            .copied()
            .unwrap_or(usize::MAX);
        let closers = &self.closers[d];
        closers
            .get(closers.partition_point(|&q| q < open_end))
            .is_some_and(|&q| q < limit)
    }
}

/// Whether `delim` opens a span at `cur`: it stands there, is not the middle
/// of a word (for `_`), has text right after it, and a closing `delim` follows
/// before the paragraph ends (`index` says).
pub(crate) fn delimited_probe(cur: &Cursor, delim: &str, index: &DelimiterIndex) -> bool {
    if !cur.starts_with(delim) {
        return false;
    }
    if delim.starts_with('_') && char_before(cur).is_some_and(|c| c.is_alphanumeric()) {
        return false;
    }
    let mut open = *cur;
    open.eat_str(delim);
    if is_boundary(open.peek()) {
        return false;
    }
    let d = DELIMITERS
        .iter()
        .position(|(d, _)| *d == delim)
        .expect("a delimiter of the table");
    index.has_closer(d, open.pos())
}

/// Whether only whitespace precedes `cur` on its line.
pub(crate) fn at_line_start(cur: &Cursor) -> bool {
    let src = cur.src();
    let before = &src[..cur.pos()];
    match before.rfind('\n') {
        Some(nl) => before[nl + 1..].trim().is_empty(),
        None => before.trim().is_empty(),
    }
}

/// Pushes a `Text`, merging into the previous item if it is also a `Text`.
///
/// The only time that happens is right after something was elided with
/// nothing pushed in its place -- a `/* */`/`//` comment being skipped is
/// the one case in this file. Flushing the text before and after a comment
/// as two separate nodes would leave `Vec<Inline>` with adjacent `Text`s
/// that mean nothing (no break, no content, sits between them), a state
/// [`Inline::SoftBreak`] very deliberately does *not* rely on `Text`
/// adjacency to mean "join these" -- see its doc comment. Merging here
/// keeps that invariant true instead of merely convenient: two `Text`s
/// are never adjacent in the finished tree without a reason.
pub(crate) fn push_text(items: &mut Vec<Inline>, value: String, span: Span) {
    if let Some(Inline::Text(prev)) = items.last_mut() {
        prev.value.push_str(&value);
        prev.span = Span::new(prev.span.start, span.end);
        return;
    }
    items.push(Inline::Text(Text::new(value, span)));
}

/// Appends `more` to `items`, merging across the join the same way
/// [`push_text`]/[`push_soft_break`] do within a single flush -- used when
/// lowering appends a freshly read sequence onto content it already built up
/// by hand (a list item's own trailing text after its groups), so the two
/// don't leave an unmerged `Text`/`Text` or `SoftBreak`/`SoftBreak` pair
/// sitting at the seam.
pub(crate) fn extend_merging(items: &mut Vec<Inline>, more: Vec<Inline>) {
    let mut more = more.into_iter();
    if let Some(first) = more.next() {
        match first {
            Inline::Text(t) => push_text(items, t.value, t.span),
            Inline::SoftBreak(b) => push_soft_break(items, b.span),
            other => items.push(other),
        }
    }
    items.extend(more);
}

/// Pushes a `SoftBreak`, widening the previous one instead if it is also a
/// `SoftBreak` sitting right before it.
///
/// A comment sandwiched between two line breaks -- `"keep\n// x\nkeep2"` --
/// flushes as two separate calls into `split_softbreaks`, one ending in a
/// break and the next starting with one, with nothing pushed for the
/// elided comment in between. Left alone that is two adjacent `SoftBreak`s
/// standing for what a reader sees as one join between `keep` and `keep2`.
/// Same reasoning as [`push_text`], for the other node kind that comment
/// elision can leave stuttering.
pub(crate) fn push_soft_break(items: &mut Vec<Inline>, span: Span) {
    if let Some(Inline::SoftBreak(prev)) = items.last_mut() {
        prev.span = Span::new(prev.span.start, span.end);
        return;
    }
    items.push(Inline::SoftBreak(SoftBreak { span }));
}

/// Splits one flushed run of source text into `Text`/[`Inline::SoftBreak`]
/// items.
///
/// A source line break used to be folded away here, right into a literal
/// `' '` in the text (or nothing, between two East-Asian-wide characters) --
/// see [`Inline::SoftBreak`]'s doc comment for why that destroyed information
/// this needs to keep. What a break swallows is unchanged: the newline, the
/// whitespace after it, and -- inside a `|` run -- the next line's own `|` margin
/// and the whitespace after that, so `x| a\n| b` still swallows exactly what
/// `x[ a\n  b ]` does. `margins` are the offsets of the `MARGIN` tokens the tree
/// holds, in order: a `|` that is not one of them is text. Only the
/// output differs: the swallowed gap becomes a `SoftBreak` node spanning it,
/// not a character. Deciding what that node renders as (a space, nothing,
/// a real newline) is left to whoever consumes the tree.
pub(crate) fn split_softbreaks(items: &mut Vec<Inline>, raw: &str, base: usize, margins: &[usize]) {
    let mut text_start = 0usize;
    let mut chars = raw.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if c != '\n' && c != '\r' {
            continue;
        }
        if i > text_start {
            let span = Span::new(base + text_start, base + i);
            push_text(items, raw[text_start..i].to_string(), span);
        }
        let mut gap_end = i + c.len_utf8();
        while let Some(&(j, wc)) = chars.peek() {
            if matches!(wc, ' ' | '\t' | '\n' | '\r') {
                gap_end = j + wc.len_utf8();
                chars.next();
            } else {
                break;
            }
        }
        // The margins of a `|` run that begin the next line, each with the blanks after it.
        while let Some(&(j, '|')) = chars.peek() {
            if margins.binary_search(&(base + j)).is_err() {
                break;
            }
            gap_end = j + '|'.len_utf8();
            chars.next();
            while let Some(&(k, wc)) = chars.peek() {
                if matches!(wc, ' ' | '\t') {
                    gap_end = k + wc.len_utf8();
                    chars.next();
                } else {
                    break;
                }
            }
        }
        let span = Span::new(base + i, base + gap_end);
        push_soft_break(items, span);
        text_start = gap_end;
    }
    if text_start < raw.len() {
        let span = Span::new(base + text_start, base + raw.len());
        push_text(items, raw[text_start..].to_string(), span);
    }
}

/// The length of the scheme (`https://`, `http://`, `mailto:`) an autolink
/// starts with at `cur`, if one starts there and not in the middle of a word.
pub(crate) fn autolink_scheme_at(cur: &Cursor) -> Option<usize> {
    if char_before(cur).is_some_and(|c| c.is_alphanumeric()) {
        return None;
    }
    if cur.starts_with("https://") {
        Some(8)
    } else if cur.starts_with("http://") || cur.starts_with("mailto:") {
        Some(7)
    } else {
        None
    }
}

/// Where the autolink that starts at `start` ends, given that the address
/// ran up to `scanned_end` (the first whitespace or the end of its group):
/// without the punctuation and the unmatched closers that belong to the
/// sentence around it. `None` if nothing is left after the scheme.
///
/// Shared with the CST parser, so the two cannot disagree on where it ends.
pub(crate) fn autolink_end(
    src: &str,
    start: usize,
    scheme_len: usize,
    scanned_end: usize,
) -> Option<usize> {
    if scanned_end - start <= scheme_len {
        return None;
    }
    let mut end_pos = scanned_end;

    while end_pos > start + scheme_len {
        let slice = &src[start..end_pos];
        let last_char = slice.chars().next_back().unwrap();
        if matches!(last_char, '.' | ',' | ';' | ':' | '!' | '?' | '"' | '\'') {
            end_pos -= last_char.len_utf8();
        } else if last_char == '>' {
            let open_count = slice.chars().filter(|&c| c == '<').count();
            let close_count = slice.chars().filter(|&c| c == '>').count();
            if close_count > open_count {
                end_pos -= 1;
            } else {
                break;
            }
        } else if last_char == ')' {
            let open_count = slice.chars().filter(|&c| c == '(').count();
            let close_count = slice.chars().filter(|&c| c == ')').count();
            if close_count > open_count {
                end_pos -= 1;
            } else {
                break;
            }
        } else if last_char == ']' {
            let open_count = slice.chars().filter(|&c| c == '[').count();
            let close_count = slice.chars().filter(|&c| c == ']').count();
            if close_count > open_count {
                end_pos -= 1;
            } else {
                break;
            }
        } else {
            break;
        }
    }

    (end_pos > start + scheme_len).then_some(end_pos)
}
