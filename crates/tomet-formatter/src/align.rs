//! Aligning the values of the entries of a multi-line `(...)` group according to
//! `map.align_values`.

use std::collections::BTreeMap;

use tomet_ast_parser::parse_file;
use tomet_config::PrinterConfig;
use tomet_style::ValueAlignment;
use tomet_tree::{Node, Visit, walk};

use crate::reorder::find_matching;

/// Aligns, in `src`, the values of consecutive `key: value` lines of each `(...)` group written
/// over several lines, when `map.align_values` is on; off, `src` is returned as written.
///
/// ```text
/// ::format(
///   meta:    { always_newline: true }
///   heading: { space_inside_brackets: true }
/// )
/// ```
///
/// The colon stays on the key and the padding follows it. A run is the lines that hold one whole
/// `key: value` entry at the group's own depth, with the same indentation, and it ends at a blank
/// line, a comment, an entry whose value runs over several lines, or any other line. Width is
/// display width, so a wide key counts for two columns a character, and a key wider than
/// `map.max_key_width` is left out of the alignment: written `key: value`, it neither widens the
/// others nor ends the run. Only the groups of declarations and elements are aligned.
///
/// Only spacing changes, and that is checked, not assumed: if the rewritten text does not parse to
/// the `File` it started from, `src` is returned unchanged.
pub fn format_map_alignment(src: &str, config: &PrinterConfig) -> String {
    if !config.format.map_align_values {
        return src.to_string();
    }
    let Ok(doc) = parse_file(src) else {
        return src.to_string();
    };

    let alignment = ValueAlignment {
        widest_key: config.format.max_key_width(),
    };
    let mut edits = BTreeMap::new();
    let _ = walk(&doc, &mut |node, _| {
        if let Some((open, close)) = args_group(src, node) {
            align_group(src, open, close, alignment, &mut edits);
        }
        Visit::<()>::Descend
    });
    if edits.is_empty() {
        return src.to_string();
    }

    let mut out = String::with_capacity(src.len());
    let mut at = 0;
    for (start, (end, text)) in &edits {
        out.push_str(&src[at..*start]);
        out.push_str(text);
        at = *end;
    }
    out.push_str(&src[at..]);

    match parse_file(&out) {
        Ok(after) if after == doc => out,
        _ => src.to_string(),
    }
}

/// The byte offsets of the `(` and `)` of the args of `node`, when it is a declaration or an element
/// written with them.
fn args_group(src: &str, node: Node<'_>) -> Option<(usize, usize)> {
    let span = match node {
        Node::Declaration(d) => {
            d.data.args.as_ref()?;
            d.span
        }
        _ => {
            let element = node.element_view()?;
            element.data.args.as_ref()?;
            element.span
        }
    };
    if span.is_dummy() {
        return None;
    }

    // The head -- `::name`, `@name` -- then `(args)`, or `[content]` first and `(args)` after it.
    let rest = src.get(span.start..span.end)?;
    let sigil = if rest.starts_with("::") {
        2
    } else if rest.starts_with('@') {
        1
    } else {
        return None;
    };
    let name_len: usize = rest[sigil..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
        .map(char::len_utf8)
        .sum();
    let mut open = span.start + sigil + name_len;
    if src.as_bytes().get(open) == Some(&b'[') {
        open = find_matching(src, open, '[', ']')? + 1;
    }
    if src.as_bytes().get(open) != Some(&b'(') {
        return None;
    }
    let close = find_matching(src, open, '(', ')')?;
    (close < span.end).then_some((open, close))
}

/// How far into a value a scan of its lines has got.
#[derive(Default)]
struct Scan {
    depth: usize,
    quote: Option<char>,
    /// A closing bracket with nothing open: the group is not read the way it was assumed to be.
    broken: bool,
}

impl Scan {
    fn at_top(&self) -> bool {
        self.depth == 0 && self.quote.is_none()
    }

    /// Takes in `line`; whether a `,` stood at the top level of it.
    fn take(&mut self, line: &str) -> bool {
        let mut comma = false;
        let mut prev: Option<char> = None;
        let mut chars = line.chars().peekable();
        while let Some(c) = chars.next() {
            if let Some(quote) = self.quote {
                if c == '\\' && quote == '"' {
                    chars.next();
                } else if c == quote {
                    self.quote = None;
                }
            } else {
                match c {
                    '"' => self.quote = Some('"'),
                    // An apostrophe inside a word (`don't`) is text, not a quote.
                    '\'' if !prev.is_some_and(char::is_alphanumeric) => self.quote = Some('\''),
                    '(' | '[' | '{' => self.depth += 1,
                    ')' | ']' | '}' => match self.depth.checked_sub(1) {
                        Some(depth) => self.depth = depth,
                        None => self.broken = true,
                    },
                    ',' if self.depth == 0 => comma = true,
                    '/' if chars.peek() == Some(&'/') && prev.is_none_or(char::is_whitespace) => {
                        break;
                    }
                    _ => {}
                }
            }
            prev = Some(c);
        }
        comma
    }
}

/// One line of a run: where it is, and its parts.
struct Row<'a> {
    start: usize,
    text: &'a str,
    indent: &'a str,
    key: &'a str,
    value: &'a str,
}

/// Records, keyed by where the line starts, the new text of each line of the group between `open`
/// and `close` that alignment changes.
fn align_group(
    src: &str,
    open: usize,
    close: usize,
    alignment: ValueAlignment,
    edits: &mut BTreeMap<usize, (usize, String)>,
) {
    let inner = &src[open + 1..close];
    let Some(first_newline) = inner.find('\n') else {
        return;
    };
    if inner.contains("/*") {
        return;
    }

    let mut found = Vec::new();
    let mut scan = Scan::default();
    let mut run: Vec<Row<'_>> = Vec::new();
    let mut offset = open + 1 + first_newline + 1;
    for text in inner[first_newline + 1..].split('\n') {
        let start = offset;
        offset += text.len() + 1;
        match classify(text, start, &mut scan) {
            Some(row) => {
                if run.last().is_some_and(|last| last.indent != row.indent) {
                    align_run(&mut run, alignment, &mut found);
                }
                run.push(row);
            }
            None => align_run(&mut run, alignment, &mut found),
        }
    }
    align_run(&mut run, alignment, &mut found);

    // A group whose brackets or quotes do not close is not read as assumed: change nothing in it.
    if scan.at_top() && !scan.broken {
        edits.extend(found);
    }
}

/// Takes `text` into `scan`, and returns it as a [`Row`] when it is one whole `key: value` entry
/// at the group's own depth.
fn classify<'a>(text: &'a str, start: usize, scan: &mut Scan) -> Option<Row<'a>> {
    let trimmed = text.trim_start_matches([' ', '\t']);
    if scan.quote.is_none() && trimmed.starts_with("//") {
        return None;
    }
    let indent = &text[..text.len() - trimmed.len()];
    let entry = scan.at_top().then(|| entry_parts(trimmed)).flatten();
    let comma = scan.take(trimmed);
    let (key, value) = entry?;
    (scan.at_top() && !comma).then_some(Row {
        start,
        text,
        indent,
        key,
        value,
    })
}

/// `key` and `value` of a line that is `key: value`, the key bare (ASCII) or quoted.
fn entry_parts(line: &str) -> Option<(&str, &str)> {
    let key_len = if let Some(quoted) = line.strip_prefix('"') {
        let mut chars = quoted.char_indices();
        loop {
            match chars.next()? {
                (_, '\\') => {
                    chars.next();
                }
                (i, '"') => break i + 2,
                _ => {}
            }
        }
    } else {
        // The parser reads a bare key as ASCII only: `見出し: 1` is a string, not an entry.
        line.find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.')))
            .unwrap_or(line.len())
    };
    if key_len == 0 {
        return None;
    }
    let (key, after) = line.split_at(key_len);
    let after = after.trim_start_matches([' ', '\t']).strip_prefix(':')?;
    // `::` is a connect, and the colon of an entry is followed by a space.
    if !after.starts_with([' ', '\t']) {
        return None;
    }
    let value = after.trim();
    (!value.is_empty() && !value.starts_with("//")).then_some((key, value))
}

/// Aligns the rows of a finished run, and empties it.
fn align_run(
    run: &mut Vec<Row<'_>>,
    alignment: ValueAlignment,
    found: &mut Vec<(usize, (usize, String))>,
) {
    let column = alignment.column(run.iter().map(|row| row.key));
    for row in run.drain(..) {
        let pad = alignment.padding(row.key, column);
        let text = format!("{}{}:{}{}", row.indent, row.key, " ".repeat(pad), row.value);
        if text != row.text {
            found.push((row.start, (row.start + row.text.len(), text)));
        }
    }
}
