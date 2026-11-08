//! CST delimiter scanning and autolink probes: the lexical probes that say
//! where a formatting delimiter (`*em*`, `_em_`, `**strong**`, `__strong__`,
//! `~~strikeout~~`) or an autolink begins and ends.

use tomet_lexer::Cursor;

/// The inline delimiters and the element each one makes, longest first so
/// `**` is tried before `*`.
pub const DELIMITERS: [(&str, &str); 5] = [
    ("**", "strong"),
    ("__", "strong"),
    ("~~", "strikeout"),
    ("*", "em"),
    ("_", "em"),
];

pub fn char_before(cur: &Cursor) -> Option<char> {
    cur.src()[..cur.pos()].chars().next_back()
}

pub fn is_boundary(c: Option<char>) -> bool {
    matches!(c, None | Some(' ') | Some('\t') | Some('\n') | Some('\r'))
}

/// If `bytes[at..]` starts with a `\` escape in running text, returns its byte length:
/// `2` for `\\`, or `1 + run` for `\` followed by a run of the same ASCII punctuation byte.
pub fn escaped_run_len(bytes: &[u8], at: usize) -> Option<usize> {
    if bytes.get(at) != Some(&b'\\') {
        return None;
    }
    let &next = bytes.get(at + 1)?;
    if !next.is_ascii_punctuation() {
        return None;
    }
    if next == b'\\' {
        return Some(2);
    }
    let run = bytes[at + 1..].iter().take_while(|&&b| b == next).count();
    Some(1 + run)
}

/// Where a delimiter could close a span, and where a paragraph ends, found once for a text.
///
/// Whether an opener has a closer is then a lookup, not a scan to the end of the paragraph for every
/// opener, which would take time quadratic in the length of a paragraph with many unmatched `*word`s.
pub struct DelimiterIndex {
    /// The byte offset of every `\n` that is followed by nothing but blanks up to another `\n` or the
    /// end: the line break that ends a paragraph.
    paragraph_ends: Vec<usize>,
    /// For each of [`DELIMITERS`], every byte offset where it could close a span: it stands there and
    /// is not right after a blank (`~~` may be).
    closers: [Vec<usize>; DELIMITERS.len()],
}

impl DelimiterIndex {
    pub fn new(src: &str) -> Self {
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
        let mut closers: [Vec<usize>; DELIMITERS.len()] = Default::default();
        let mut q = 0;
        while q < bytes.len() {
            if let Some(len) = escaped_run_len(bytes, q) {
                q += len;
                continue;
            }
            for (d, (delim, _)) in DELIMITERS.iter().enumerate() {
                let delim = delim.as_bytes();
                if bytes[q..].starts_with(delim)
                    && (delim == b"~~" || (q > 0 && !blank(bytes[q - 1])))
                {
                    closers[d].push(q);
                }
            }
            q += 1;
        }
        Self {
            paragraph_ends,
            closers,
        }
    }

    /// Whether the delimiter `DELIMITERS[d]`, once opened (the opener ends at `open_end`), has a closer
    /// before its paragraph ends.
    pub fn has_closer(&self, d: usize, open_end: usize) -> bool {
        let limit = self
            .paragraph_ends
            .get(self.paragraph_ends.partition_point(|&e| e < open_end))
            .copied()
            .unwrap_or(usize::MAX);
        let closers = &self.closers[d];
        closers
            .get(closers.partition_point(|&q| q <= open_end))
            .is_some_and(|&q| q < limit)
    }
}

/// Whether `delim` opens a span at `cur`: it stands there, is not the middle
/// of a word (for `_`), has text right after it, and a closing `delim` follows
/// before the paragraph ends (`index` says).
pub fn delimited_probe(cur: &Cursor, delim: &str, index: &DelimiterIndex) -> bool {
    if !cur.starts_with(delim) {
        return false;
    }
    if delim.starts_with('_') && char_before(cur).is_some_and(|c| c.is_alphanumeric()) {
        return false;
    }
    let mut open = *cur;
    open.eat_str(delim);
    let delim_char = delim.chars().next().unwrap();
    let mut after_run = open;
    after_run.eat_while(|c| c == delim_char);
    if is_boundary(after_run.peek()) {
        return false;
    }
    let d = DELIMITERS
        .iter()
        .position(|(d, _)| *d == delim)
        .expect("a delimiter of the table");
    index.has_closer(d, open.pos())
}

/// The length of the scheme (`https://`, `http://`, `mailto:`) an autolink
/// starts with at `cur`, if one starts there and not in the middle of a word.
pub fn autolink_scheme_at(cur: &Cursor) -> Option<usize> {
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
pub fn autolink_end(
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
