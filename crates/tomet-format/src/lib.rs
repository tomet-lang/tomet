//! Whitespace-hygiene and layout formatter for `.tmt` source text.
//!
//! [`format_source`] is AST-aware using source [`tomet_ast::Span`]
//! metadata. Normalizes whitespace policy (LF line endings, no trailing
//! whitespace, collapsed excess blank lines, exactly one final newline)
//! while losslessly preserving literal whitespace and blank lines inside
//! raw/verbatim content (a ``` fenced code block, and any element body
//! carried by a `+++` fence). It never changes the parsed `Document` --
//! the `does_not_change_the_parsed_document` test in the `tomet-tests`
//! package asserts this across the whole shared corpus, and its
//! `KNOWN_FORMAT_CHANGES_DOCUMENT` exception list is empty.
//!
//! The list held one entry until the `+++` fence replaced
//! `(content:raw)[...]`. A raw body used to be delimited by matched
//! brackets, so a source that wrote full-width `｛｝` where the grammar
//! wants ASCII `{}` did not opt into raw at all, left no raw span to
//! protect, and had the trailing-whitespace rule run over content meant
//! to be verbatim. A fence is delimited by a *line*, so no character
//! inside the body can end it early or make the formatter disagree with
//! the parser about where verbatim content begins. The bug class is gone
//! by construction rather than by exception.
//!
//! There is one deliberate exception left:
//! [`quote_bare_at_yaml_values`], run first, wraps a bare `@...`-led
//! value inside any `(format:yaml)+++ ... +++` body in `""`.
//! Unquoted, `@` is a reserved YAML indicator that can't start a plain
//! scalar (`embedded_format.rs` hands the body straight to `serde_yaml`,
//! which rejects it outright), so that shape doesn't have a successfully-
//! parsed `Document` to preserve in the first place -- this pass turns
//! an unparseable file into a parseable one with the obvious intended
//! reading, it doesn't change what an already-valid file means. Raw-text
//! based rather than AST-based for exactly that reason: there's nothing
//! to walk yet for the file this exists to fix.
//!
//! [`format_source_with_config`] formats tables according to `PrinterConfig`
//! (e.g. `table.adjust_width`, `table.max_col_width`, `table.align`) and
//! applies [`format_source`]. It never alters document metadata (`@meta`)
//! or injects structural elements.

use tomet_ast::{Block, Document, Element, Inline, Sigil, Value};
use tomet_config::{GroupOrder, PrinterConfig};
use tomet_parser::parse_document;

/// Format `src` in place (returns a new `String`). Idempotent:
/// `format_source(&format_source(src)) == format_source(src)`.
pub fn format_source(src: &str) -> String {
    let normalized = src.replace("\r\n", "\n").replace('\r', "\n");
    let normalized = quote_bare_at_yaml_values(&normalized);
    if normalized.trim().is_empty() {
        return String::new();
    }

    let mut raw_spans = Vec::new();
    if let Ok(doc) = parse_document(&normalized) {
        collect_raw_spans(&doc, &mut raw_spans);
    }
    collect_fence_spans(&normalized, &mut raw_spans);

    let is_offset_raw = |offset: usize| -> bool {
        raw_spans
            .iter()
            .any(|&(start, end)| offset >= start && offset < end)
    };

    let mut out_lines: Vec<String> = Vec::new();
    let mut pending_blank = false;
    let mut current_offset = 0;

    for line in normalized.split('\n') {
        let line_len = line.len();
        let line_end_offset = current_offset + line_len;

        // Check if any byte in this line falls inside raw content.
        let is_raw_line = (current_offset..=line_end_offset).any(is_offset_raw);

        if is_raw_line {
            if pending_blank && !out_lines.is_empty() {
                out_lines.push(String::new());
            }
            pending_blank = false;
            out_lines.push(line.to_string());
        } else {
            let trimmed = line.trim_end_matches([' ', '\t']);
            if trimmed.is_empty() {
                pending_blank = true;
            } else {
                if pending_blank && !out_lines.is_empty() {
                    out_lines.push(String::new());
                }
                pending_blank = false;
                out_lines.push(trimmed.to_string());
            }
        }

        // +1 accounts for the newline character in the normalized string
        current_offset += line_len + 1;
    }

    if out_lines.is_empty() {
        return String::new();
    }

    let mut out = out_lines.join("\n");
    out.push('\n');
    out
}

/// Finds every `(...){...}` body whose args declare `format:yaml` and
/// wraps any bare, unquoted `@...`-led value inside it in `"..."` -- see
/// the module doc comment for why. Raw-text scanning, not a real YAML
/// parse: cheap, and this only ever needs to recognize three "a value
/// starts here" shapes (`key:`, `- `, and `[`/`,` inside a flow
/// sequence), not the whole YAML grammar.
fn quote_bare_at_yaml_values(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut pos = 0;
    while let Some((body_start, body_end)) = find_next_format_yaml_body(&src[pos..]) {
        let (body_start, body_end) = (pos + body_start, pos + body_end);
        out.push_str(&src[pos..body_start]);
        quote_bare_at_values_in(src, body_start, body_end, &mut out);
        pos = body_end;
    }
    out.push_str(&src[pos..]);
    out
}

/// Byte range (relative to `s`) of the next `format:yaml`-tagged
/// element's `+++` fence body, or `None` if there isn't one.
///
/// The body used to be a `{...}` group found by brace matching. It is a
/// fence now, so the head is `(... format:yaml ...)+++` and the body runs
/// to the closing `+++` line -- which is both simpler and immune to the
/// stray-brace miscount the old scan could hit.
fn find_next_format_yaml_body(s: &str) -> Option<(usize, usize)> {
    let mut search_from = 0;
    loop {
        let rel_open_paren = s[search_from..].find('(')?;
        let open_paren = search_from + rel_open_paren;
        let Some(close_paren) = find_matching(s, open_paren, '(', ')') else {
            search_from = open_paren + 1;
            continue;
        };
        if args_declare_format_yaml(&s[open_paren + 1..close_paren]) {
            let after = &s[close_paren + 1..];
            let run = after.len() - after.trim_start_matches('+').len();
            if run >= 3 {
                let head_end = close_paren + 1 + run;
                // Skip to just past the newline ending the head line.
                let body_start = {
                    let nl = s[head_end..].find('\n')?;
                    head_end + nl + 1
                };
                let closer = "+".repeat(run);
                let mut scan = body_start;
                while scan < s.len() {
                    let line_end = s[scan..].find('\n').map_or(s.len(), |n| scan + n);
                    if s[scan..line_end].trim_end() == closer {
                        return Some((body_start, scan));
                    }
                    scan = line_end + 1;
                }
                return Some((body_start, s.len()));
            }
        }
        search_from = close_paren + 1;
    }
}

/// Whether an element's `(args)` body (already stripped of the
/// enclosing parens) declares a `format` key of `yaml`, e.g.
/// `format:yaml` or `id:foo, format: yaml`. Deliberately not full
/// `value.rs`-grade args parsing -- `format:yaml` args are always this
/// simple in practice, and this is only a gate for the raw-text scan
/// above, not something anything downstream relies on for correctness.
fn args_declare_format_yaml(args: &str) -> bool {
    args.split(',').any(|entry| {
        entry
            .split_once(':')
            .is_some_and(|(key, value)| key.trim() == "format" && value.trim() == "yaml")
    })
}

/// Byte offset of the `close` matching the `open` at `open_pos` in
/// `src` (which must be `open`), tracking nested `open`/`close` depth
/// and skipping over `"..."`/`'...'` quoted runs (so a stray bracket
/// character inside a string doesn't miscount). Mirrors
/// `tomet-syntax-parser::value::find_matching_delimiter`'s
/// algorithm (not reusable directly -- that one is `pub(crate)` and
/// works off this crate's own `Cursor` type). Byte-indexed but UTF-8
/// safe: every character this function's own logic branches on (`"`,
/// `'`, `\`, `open`, `close`) is ASCII, so walking any other byte one at
/// a time never lands on -- or returns -- a non-boundary offset.
fn find_matching(src: &str, open_pos: usize, open: char, close: char) -> Option<usize> {
    let bytes = src.as_bytes();
    let mut i = open_pos + open.len_utf8();
    let mut depth: u32 = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'"' || c == b'\'' {
            i += 1;
            while i < bytes.len() {
                if bytes[i] == b'\\' && c == b'"' && i + 1 < bytes.len() {
                    i += 2;
                    continue;
                }
                let closed = bytes[i] == c;
                i += 1;
                if closed {
                    break;
                }
            }
        } else if c == open as u8 {
            depth += 1;
            i += 1;
        } else if c == close as u8 {
            if depth == 0 {
                return Some(i);
            }
            depth -= 1;
            i += 1;
        } else {
            i += 1;
        }
    }
    None
}

/// Does the actual `@...` -> `"@..."` rewriting within one already-
/// located `format:yaml` body's span `[start, end)`, appending to `out`.
/// Skips existing `"..."`/`'...'` strings and `#...` comments verbatim
/// (so it never touches an `@` that's already quoted, or one that's
/// just commentary) and only quotes a value immediately after `key:`,
/// `- `, or `[`/`,` (a flow-sequence item) -- anywhere else, a leading
/// `@` is left alone.
fn quote_bare_at_values_in(src: &str, start: usize, end: usize, out: &mut String) {
    let bytes = src.as_bytes();
    let mut i = start;
    while i < end {
        let c = bytes[i];
        match c {
            b'#' => {
                let line_end = src[i..end].find('\n').map_or(end, |p| i + p);
                out.push_str(&src[i..line_end]);
                i = line_end;
            }
            b'"' | b'\'' => {
                let quote = c;
                let q_start = i;
                i += 1;
                while i < end {
                    if bytes[i] == b'\\' && quote == b'"' && i + 1 < end {
                        i += 2;
                        continue;
                    }
                    let closed = bytes[i] == quote;
                    i += 1;
                    if closed {
                        break;
                    }
                }
                out.push_str(&src[q_start..i]);
            }
            b':' | b',' | b'[' => {
                out.push(c as char);
                i += 1;
                while i < end && matches!(bytes[i], b' ' | b'\t') {
                    out.push(bytes[i] as char);
                    i += 1;
                }
                if i < end && bytes[i] == b'@' {
                    quote_one_value_at(src, &mut i, end, out);
                }
            }
            b'-' if i + 1 < end && matches!(bytes[i + 1], b' ' | b'\t') => {
                out.push('-');
                i += 1;
                while i < end && matches!(bytes[i], b' ' | b'\t') {
                    out.push(bytes[i] as char);
                    i += 1;
                }
                if i < end && bytes[i] == b'@' {
                    quote_one_value_at(src, &mut i, end, out);
                }
            }
            _ => {
                let ch = src[i..].chars().next().expect("i < end <= src.len()");
                out.push(ch);
                i += ch.len_utf8();
            }
        }
    }
}

/// Helper for [`quote_bare_at_values_in`]: having just seen a `@` at
/// `src[*i]` in a value position, advances `*i` past the end of that
/// scalar value, wrapping the scanned slice in `"..."` as it appends to
/// `out`. Stops at the first newline, unquoted `,`, `]`, `}`, or `#`
/// comment start.
fn quote_one_value_at(src: &str, i: &mut usize, end: usize, out: &mut String) {
    let val_start = *i;
    let bytes = src.as_bytes();
    let mut depth: u32 = 0;
    while *i < end {
        let b = bytes[*i];
        match b {
            b'\n' | b'\r' => break,
            b'#' if depth == 0 => break,
            b'(' | b'[' | b'{' => {
                depth += 1;
                *i += 1;
            }
            b')' | b']' | b'}' => {
                if depth == 0 {
                    break;
                }
                depth -= 1;
                *i += 1;
            }
            b',' if depth == 0 => break,
            _ => *i += 1,
        }
    }
    let val = src[val_start..*i].trim_end();
    out.push('"');
    out.push_str(val);
    out.push('"');
}

/// Formats tables in `src` according to `table.adjust_width`, `table.max_col_width`, and `table.align`.
pub fn format_tables_with_config(src: &str, config: &PrinterConfig) -> String {
    let mode = config.table_adjust_width.as_deref().unwrap_or("auto");
    if mode == "false" || mode == "off" {
        return src.to_string();
    }
    let max_col_width_limit = config.table_max_col_width.unwrap_or(20);

    let lines: Vec<&str> = src.lines().collect();
    let mut out_lines = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();

        let is_table_lead = trimmed.starts_with("@table");
        let mut is_bracket_table = false;
        let mut is_pipe_table = false;

        if is_table_lead {
            if trimmed.contains('[') {
                is_bracket_table = true;
            } else if trimmed.ends_with('|') {
                is_pipe_table = true;
            } else {
                let mut next_idx = i + 1;
                while next_idx < lines.len() && lines[next_idx].trim().is_empty() {
                    next_idx += 1;
                }
                if next_idx < lines.len() {
                    let next_trimmed = lines[next_idx].trim();
                    if next_trimmed.starts_with('|') && extract_row_cells(lines[next_idx]).is_some()
                    {
                        is_pipe_table = true;
                    } else if next_trimmed.starts_with('[') {
                        is_bracket_table = true;
                    }
                }
            }
        }

        if is_bracket_table || is_pipe_table {
            out_lines.push(line.to_string());
            i += 1;

            let mut table_rows: Vec<(String, Vec<String>)> = Vec::new();
            let mut raw_table_lines = Vec::new();

            while i < lines.len() {
                let t_line = lines[i];
                let t_trimmed = t_line.trim();

                if is_bracket_table {
                    if t_trimmed == "]"
                        || t_trimmed.starts_with("]{")
                        || t_trimmed.starts_with("] ")
                    {
                        break;
                    }
                } else if is_pipe_table && (t_trimmed.is_empty() || !t_trimmed.starts_with('|')) {
                    break;
                }

                if let Some((prefix, cells)) = extract_row_cells(t_line) {
                    table_rows.push((prefix, cells));
                    raw_table_lines.push(None);
                } else {
                    raw_table_lines.push(Some(t_line.to_string()));
                }
                i += 1;
            }

            let aligns = extract_table_alignments(line, config.table_align.as_deref());

            if !table_rows.is_empty() {
                let mut col_count = 0;
                for (_, cells) in &table_rows {
                    col_count = col_count.max(cells.len());
                }

                let mut max_lens = vec![0usize; col_count];
                for (_, cells) in &table_rows {
                    for (c_idx, cell) in cells.iter().enumerate() {
                        let len = text_display_width(cell.trim());
                        max_lens[c_idx] = max_lens[c_idx].max(len);
                    }
                }

                let mut target_widths: Vec<Option<usize>> = vec![None; col_count];
                let mut auto_align_active = true;

                for (c_idx, &max_len) in max_lens.iter().enumerate() {
                    match mode {
                        "true" | "all" => {
                            target_widths[c_idx] = Some(max_len.max(1));
                        }
                        "auto" => {
                            if auto_align_active && max_len <= max_col_width_limit {
                                target_widths[c_idx] = Some(max_len.max(1));
                            } else {
                                auto_align_active = false;
                                target_widths[c_idx] = None;
                            }
                        }
                        _ => {
                            target_widths[c_idx] = None;
                        }
                    }
                }

                let mut row_idx = 0;
                for raw in raw_table_lines {
                    if let Some(other_line) = raw {
                        out_lines.push(other_line);
                    } else {
                        let (prefix, cells) = &table_rows[row_idx];
                        row_idx += 1;
                        let mut formatted_cells = Vec::new();
                        for (c_idx, &target) in target_widths.iter().enumerate() {
                            let cell_text = cells.get(c_idx).map(|s| s.trim()).unwrap_or("");
                            let cell_len = text_display_width(cell_text);
                            let col_align = aligns.get(c_idx).map(|s| s.as_str()).unwrap_or("left");

                            let (left_spaces, right_spaces) = if let Some(target_w) = target {
                                let target_width = target_w + 2;
                                let extra = if target_width > cell_len {
                                    target_width - cell_len
                                } else {
                                    2
                                };
                                match col_align {
                                    "right" => {
                                        let left = extra.saturating_sub(1);
                                        let right = 1;
                                        (left, right)
                                    }
                                    "center" => {
                                        let left = extra / 2;
                                        let right = extra - left;
                                        (left, right)
                                    }
                                    _ => {
                                        let left = 1;
                                        let right = extra.saturating_sub(1);
                                        (left, right)
                                    }
                                }
                            } else {
                                (1, 1)
                            };

                            let left_str = " ".repeat(left_spaces);
                            let right_str = " ".repeat(right_spaces);
                            formatted_cells.push(format!("[{left_str}{cell_text}{right_str}]"));
                        }
                        out_lines.push(format!("{prefix}{}", formatted_cells.join("")));
                    }
                }
            }

            if is_bracket_table && i < lines.len() {
                out_lines.push(lines[i].to_string());
                i += 1;
            }
        } else {
            out_lines.push(line.to_string());
            i += 1;
        }
    }

    let mut res = out_lines.join("\n");
    if src.ends_with('\n') && !res.ends_with('\n') {
        res.push('\n');
    }
    res
}

fn extract_table_alignments(header_line: &str, default_align: Option<&str>) -> Vec<String> {
    let def = default_align.unwrap_or("left");
    if let Some(args_start) = header_line.find('(')
        && let Some(args_end) = header_line[args_start..].find(')')
    {
        let args = &header_line[args_start + 1..args_start + args_end];
        if let Some(pos) = args.find("align:") {
            let val = args[pos + 6..].trim();
            if val.starts_with('[') {
                if let Some(end_bracket) = val.find(']') {
                    let inner = &val[1..end_bracket];
                    let mut aligns = Vec::new();
                    for item in inner.split(',') {
                        let a = item.trim().trim_matches('"').trim_matches('\'');
                        if !a.is_empty() {
                            aligns.push(a.to_string());
                        }
                    }
                    if !aligns.is_empty() {
                        return aligns;
                    }
                }
            } else {
                let a = val
                    .split(',')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'');
                if !a.is_empty() {
                    return vec![a.to_string(); 50];
                }
            }
        }
    }
    vec![def.to_string(); 50]
}

fn text_display_width(s: &str) -> usize {
    unicode_width::UnicodeWidthStr::width(s)
}

fn extract_row_cells(line: &str) -> Option<(String, Vec<String>)> {
    let indent_len = line.len() - line.trim_start().len();
    let indent = &line[..indent_len];
    let rest = &line[indent_len..];

    let (prefix, cells_part) = if let Some(after_pipe) = rest.strip_prefix('|') {
        let spaces_len = after_pipe.len() - after_pipe.trim_start().len();
        let space_str = if spaces_len > 0 {
            &after_pipe[..spaces_len]
        } else {
            ""
        };
        (format!("{indent}|{space_str}"), after_pipe.trim_start())
    } else {
        (indent.to_string(), rest)
    };

    let trimmed = cells_part.trim();
    if !trimmed.starts_with('[') || !trimmed.ends_with(']') {
        return None;
    }

    let mut cells = Vec::new();
    let mut depth = 0;
    let mut cell_start = 0;
    let chars: Vec<(usize, char)> = cells_part.char_indices().collect();

    for &(byte_idx, c) in &chars {
        if c == '[' {
            if depth == 0 {
                cell_start = byte_idx + c.len_utf8();
            }
            depth += 1;
        } else if c == ']' {
            depth -= 1;
            if depth == 0 {
                let cell_content = &cells_part[cell_start..byte_idx];
                cells.push(cell_content.to_string());
            }
        }
    }

    if depth == 0 && !cells.is_empty() {
        Some((prefix, cells))
    } else {
        None
    }
}

/// Formats `src` by applying configured table layout formatting, element group order, and whitespace hygiene.
/// Never mutates metadata or inserts structural blocks.
pub fn format_source_with_config(src: &str, config: &PrinterConfig) -> String {
    let formatted_tables = format_tables_with_config(src, config);
    let formatted_elements = if config.group_order.is_some() || config.link_group_order.is_some() {
        format_element_group_order(&formatted_tables, config)
    } else {
        formatted_tables
    };
    format_source(&formatted_elements)
}

/// Formats element group order according to `config` (`PrinterConfig::group_order` / `PrinterConfig::link_group_order`).
///
/// Swaps `()` and `[]` for elements carrying both args and content groups based on configured element rules.
pub fn format_element_group_order(src: &str, config: &PrinterConfig) -> String {
    let mut current = src.to_string();
    let mut iterations = 0;
    const MAX_ITERATIONS: usize = 200;

    while iterations < MAX_ITERATIONS {
        if let Some(next) = find_and_swap_one_element(&current, config) {
            current = next;
            iterations += 1;
        } else {
            break;
        }
    }
    current
}

fn find_and_swap_one_element(src: &str, config: &PrinterConfig) -> Option<String> {
    let doc = parse_document(src).ok()?;
    for block in &doc.blocks {
        if let Some(swapped) = find_swap_in_block(block, src, config) {
            return Some(swapped);
        }
    }
    None
}

fn find_swap_in_block(block: &Block, src: &str, config: &PrinterConfig) -> Option<String> {
    match block {
        Block::Paragraph(p) => {
            for inline in &p.content {
                if let Some(swapped) = find_swap_in_inline(inline, src, config) {
                    return Some(swapped);
                }
            }
        }
        Block::Element(el) => {
            if let Some(swapped) = check_and_swap_element(el, src, config) {
                return Some(swapped);
            }
        }
        Block::Section(sec) => {
            for inline in &sec.title {
                if let Some(swapped) = find_swap_in_inline(inline, src, config) {
                    return Some(swapped);
                }
            }
            for conn in &sec.connects {
                if let Some(swapped) = check_and_swap_element(conn, src, config) {
                    return Some(swapped);
                }
            }
            for child in &sec.blocks {
                if let Some(swapped) = find_swap_in_block(child, src, config) {
                    return Some(swapped);
                }
            }
        }
    }
    None
}

fn find_swap_in_inline(inline: &Inline, src: &str, config: &PrinterConfig) -> Option<String> {
    match inline {
        Inline::Element(el) => check_and_swap_element(el, src, config),
        _ => None,
    }
}

fn check_and_swap_element(el: &Element, src: &str, config: &PrinterConfig) -> Option<String> {
    // 1. Check if this element itself needs swapping
    let elem_name = el.sigil.name().map(|n| n.name.as_str()).unwrap_or("");
    if let Some(order) = config.element_group_order(elem_name)
        && matches!(el.sigil, Sigil::Named(_) | Sigil::Caret(_))
        && el.args.is_some()
        && el.content.is_some()
        && !el.span.is_dummy()
    {
        let start = el.span.start.offset;
        let end = el.span.end.offset;
        if start < end
            && end <= src.len()
            && let Some(swapped) = try_swap_element_groups(src, start, end, order)
        {
            return Some(swapped);
        }
    }

    // 2. Otherwise recursively check children
    if let Some(inlines) = &el.content {
        for inline in inlines {
            if let Some(swapped) = find_swap_in_inline(inline, src, config) {
                return Some(swapped);
            }
        }
    }
    if let Some(children) = &el.children {
        for child in children {
            if let Some(swapped) = find_swap_in_block(child, src, config) {
                return Some(swapped);
            }
        }
    }
    if let Some(children) = el.value.as_ref().map(|v| v.as_children()) {
        for child in children {
            if let Some(swapped) = check_and_swap_element(child, src, config) {
                return Some(swapped);
            }
        }
    }
    for conn in &el.connects {
        if let Some(swapped) = check_and_swap_element(conn, src, config) {
            return Some(swapped);
        }
    }

    None
}

fn try_swap_element_groups(
    src: &str,
    start: usize,
    end: usize,
    order: GroupOrder,
) -> Option<String> {
    let bytes = src.as_bytes();
    let mut pos = start;

    // Skip sigil '@' or '^'
    if pos < end && (bytes[pos] == b'@' || bytes[pos] == b'^') {
        pos += 1;
        if pos < end && bytes[pos] == b'[' {
            let close = find_matching(src, pos, '[', ']')?;
            pos = close + 1;
        } else {
            while pos < end
                && (bytes[pos].is_ascii_alphanumeric() || matches!(bytes[pos], b'_' | b'-' | b'.'))
            {
                pos += 1;
            }
        }
    } else {
        return None;
    }

    // Skip whitespace between name and first group
    while pos < end && matches!(bytes[pos], b' ' | b'\t' | b'\n' | b'\r') {
        pos += 1;
    }
    if pos >= end {
        return None;
    }

    if bytes[pos] == b'(' {
        let open_paren = pos;
        let close_paren = find_matching(src, open_paren, '(', ')')?;
        let mut next_pos = close_paren + 1;
        while next_pos < end && matches!(bytes[next_pos], b' ' | b'\t' | b'\n' | b'\r') {
            next_pos += 1;
        }
        if next_pos < end && bytes[next_pos] == b'[' {
            let open_bracket = next_pos;
            let close_bracket = find_matching(src, open_bracket, '[', ']')?;

            if order == GroupOrder::ContentFirst {
                let args_part = &src[open_paren..=close_paren];
                let gap = &src[close_paren + 1..open_bracket];
                let content_part = &src[open_bracket..=close_bracket];

                let mut out = String::with_capacity(src.len());
                out.push_str(&src[..open_paren]);
                out.push_str(content_part);
                out.push_str(gap);
                out.push_str(args_part);
                out.push_str(&src[close_bracket + 1..]);
                return Some(out);
            }
        }
    } else if bytes[pos] == b'[' {
        let open_bracket = pos;
        let close_bracket = find_matching(src, open_bracket, '[', ']')?;
        let mut next_pos = close_bracket + 1;
        while next_pos < end && matches!(bytes[next_pos], b' ' | b'\t' | b'\n' | b'\r') {
            next_pos += 1;
        }
        if next_pos < end && bytes[next_pos] == b'(' {
            let open_paren = next_pos;
            let close_paren = find_matching(src, open_paren, '(', ')')?;

            if order == GroupOrder::ArgsFirst {
                let content_part = &src[open_bracket..=close_bracket];
                let gap = &src[close_bracket + 1..open_paren];
                let args_part = &src[open_paren..=close_paren];

                let mut out = String::with_capacity(src.len());
                out.push_str(&src[..open_bracket]);
                out.push_str(args_part);
                out.push_str(gap);
                out.push_str(content_part);
                out.push_str(&src[close_paren + 1..]);
                return Some(out);
            }
        }
    }

    None
}

fn is_raw_element(el: &Element) -> bool {
    if el.sigil.is_bare_named("raw") {
        return true;
    }
    if let Some(Value::Map(entries)) = &el.args
        && entries
            .iter()
            .any(|(k, v)| k == "content" && matches!(v, Value::String(s) if s == "raw"))
    {
        return true;
    }
    false
}

/// Byte ranges covered by `+++` fence bodies.
///
/// Found by scanning lines rather than through the AST: a fence is
/// line-oriented, so the text alone says exactly where each body starts
/// and ends, and `ElementValue::Raw` carries no span of its own. Anything
/// inside is verbatim -- trailing whitespace included, which is the whole
/// point of writing it in a fence.
fn collect_fence_spans(src: &str, out: &mut Vec<(usize, usize)>) {
    let mut offset = 0usize;
    let mut fence: Option<(usize, usize)> = None; // (run length, body start)
    for line in src.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\n', '\r']).trim_end();
        // A closing line is all `+`; an opening run sits at the *end* of
        // the element head, so the two are counted from opposite sides.
        let leading = trimmed.len() - trimmed.trim_start_matches('+').len();
        let trailing = trimmed.len() - trimmed.trim_end_matches('+').len();
        match fence {
            Some((open_run, body_start)) => {
                if leading >= open_run && leading == trimmed.len() && !trimmed.is_empty() {
                    out.push((body_start, offset));
                    fence = None;
                }
            }
            None => {
                if trailing >= 3 {
                    fence = Some((trailing, offset + line.len()));
                }
            }
        }
        offset += line.len();
    }
    // An unterminated fence runs to EOF, matching the parser.
    if let Some((_, body_start)) = fence {
        out.push((body_start, src.len()));
    }
}

fn collect_raw_spans(doc: &Document, out: &mut Vec<(usize, usize)>) {
    fn walk_element(el: &Element, out: &mut Vec<(usize, usize)>) {
        if is_raw_element(el)
            && let Some(inlines) = &el.content
        {
            for inline in inlines {
                let span = inline.span();
                if span.start.offset < span.end.offset {
                    out.push((span.start.offset, span.end.offset));
                }
            }
        }
        if let Some(inlines) = &el.content {
            for inline in inlines {
                if let Inline::Element(child_el) = inline {
                    walk_element(child_el, out);
                }
            }
        }
        if let Some(children) = &el.children {
            for child in children {
                walk_block(child, out);
            }
        }
        if let Some(children) = el.value.as_ref().map(|v| v.as_children()) {
            for child in children {
                walk_element(child, out);
            }
        }
    }

    fn walk_block(block: &Block, out: &mut Vec<(usize, usize)>) {
        match block {
            Block::Paragraph(p) => {
                for inline in &p.content {
                    if let Inline::Element(el) = inline {
                        walk_element(el, out);
                    }
                }
            }
            Block::Element(el) => walk_element(el, out),
            Block::Section(sec) => {
                for inline in &sec.title {
                    if let Inline::Element(el) = inline {
                        walk_element(el, out);
                    }
                }
                for conn in &sec.connects {
                    walk_element(conn, out);
                }
                for child in &sec.blocks {
                    walk_block(child, out);
                }
            }
        }
    }

    for block in &doc.blocks {
        walk_block(block, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tomet_config::FieldConfig;

    #[test]
    fn format_source_with_config_leaves_meta_block_completely_untouched() {
        let src = "@meta{title: Hello}\n\n#[ Hello ]\n\nSome body text.\n";
        let mut config = PrinterConfig::default();
        config.meta_fields.insert(
            "id".to_string(),
            FieldConfig {
                field_type: Some("nanoid".to_string()),
                length: Some(8),
                prefix: Some("doc-".to_string()),
                ..Default::default()
            },
        );

        let out = format_source_with_config(src, &config);
        assert_eq!(out, src);
    }

    #[test]
    fn format_source_with_config_does_not_insert_meta_when_missing() {
        let src = "#[ Hello ]\n\nSome body text.\n";
        let mut config = PrinterConfig::default();
        config.meta_fields.insert(
            "id".to_string(),
            FieldConfig {
                field_type: Some("nanoid".to_string()),
                length: Some(8),
                prefix: Some("doc-".to_string()),
                ..Default::default()
            },
        );

        let out = format_source_with_config(src, &config);
        assert_eq!(out, src);
    }

    #[test]
    fn strips_trailing_whitespace() {
        assert_eq!(format_source("a  \nb\t\n"), "a\nb\n");
    }

    #[test]
    fn normalizes_line_endings() {
        assert_eq!(format_source("a\r\nb\r\n"), "a\nb\n");
        assert_eq!(format_source("a\rb\r"), "a\nb\n");
    }

    #[test]
    fn drops_leading_blank_lines() {
        assert_eq!(format_source("\n\n\na\n"), "a\n");
    }

    #[test]
    fn collapses_multiple_blank_lines_to_one() {
        assert_eq!(format_source("a\n\n\n\nb\n"), "a\n\nb\n");
    }

    #[test]
    fn drops_trailing_blank_lines_and_ensures_one_final_newline() {
        assert_eq!(format_source("a\n\n\n"), "a\n");
        assert_eq!(format_source("a"), "a\n");
    }

    #[test]
    fn empty_input_stays_empty() {
        assert_eq!(format_source(""), "");
        assert_eq!(format_source("\n\n  \n"), "");
    }

    #[test]
    fn already_formatted_input_is_unchanged() {
        let src = "#[ Hello ]\n\n- one\n- two\n";
        assert_eq!(format_source(src), src);
    }

    #[test]
    fn raw_content_is_preserved_losslessly() {
        let src = "@memo+++\nline one  \n\nline two\n+++\n";
        assert_eq!(format_source(src), src);
    }

    #[test]
    fn raw_content_in_brackets_is_preserved_losslessly() {
        let src = "@raw(lang:rust)[\nfn foo() {\n    let a = 1;  \n\n    let b = 2;\n}\n]\n";
        assert_eq!(format_source(src), src);
    }

    #[test]
    fn fenced_code_block_content_is_preserved_losslessly() {
        let src = "```rust\nfn foo() {\n    let a = 1;  \n\n    let b = 2;\n}\n```\n";
        assert_eq!(format_source(src), src);
    }

    #[test]
    fn quotes_bare_at_led_yaml_values_so_the_document_parses() {
        let src = "@meta(format:yaml)+++\nprevious: @link(ref:x)\nnext: @link(ref:y)\nparent: [@link(ref:z), @link(ref:w)]\n+++\n";
        let expected = "@meta(format:yaml)+++\nprevious: \"@link(ref:x)\"\nnext: \"@link(ref:y)\"\nparent: [\"@link(ref:z)\", \"@link(ref:w)\"]\n+++\n";
        let out = format_source(src);
        assert_eq!(out, expected);
        tomet_parser::parse_document(&out)
            .unwrap_or_else(|e| panic!("formatted output should now parse: {e}"));
    }

    #[test]
    fn quote_bare_at_yaml_values_handles_block_list_items() {
        let src =
            "@meta(format:yaml)+++\nrefs:\n  - @link(ref:x)\n  - already \"@link(ref:y)\"\n+++\n";
        let out = format_source(src);
        assert!(out.contains("- \"@link(ref:x)\""));
        assert!(
            out.contains("- already \"@link(ref:y)\""),
            "existing quote untouched: {out:?}"
        );
    }

    #[test]
    fn quote_bare_at_yaml_values_leaves_already_quoted_and_unrelated_content_alone() {
        let src =
            "@meta(format:yaml)+++\nprevious: \"@link(ref:x)\"\n+++\n\n@link(ref:x)[some text]\n";
        assert_eq!(format_source(src), src);
    }

    #[test]
    fn quote_bare_at_yaml_values_is_idempotent() {
        let src = "@meta(format:yaml)+++\nprevious: @link(ref:x)\n+++\n";
        let once = format_source(src);
        let twice = format_source(&once);
        assert_eq!(once, twice);
    }

    #[test]
    fn test_format_tables_with_config_left_align() {
        let src = "@table[\n[ 殻 ][ 主量子数 n ][ 電子数 2n² ][ 小軌道 ]\n[ K殻 ][ 1 ][ 2 ][ 1s @br(2) ]\n[ L殻 ][ 2 ][ 8 ][ 2s+2p @br(2+6) ]\n]\n";
        let config = PrinterConfig {
            table_adjust_width: Some("auto".to_string()),
            table_max_col_width: Some(20),
            table_align: Some("left".to_string()),
            ..Default::default()
        };

        let out = format_source_with_config(src, &config);
        assert!(out.contains("[ 殻  ][ 主量子数 n ][ 電子数 2n² ][ 小軌道         ]"));
        assert!(out.contains("[ K殻 ][ 1          ][ 2          ][ 1s @br(2)      ]"));
    }

    #[test]
    fn test_format_tables_with_config_right_align() {
        let src = "@table[\n[ 殻 ][ 主量子数 n ][ 電子数 2n² ][ 小軌道 ]\n[ K殻 ][ 1 ][ 2 ][ 1s @br(2) ]\n[ L殻 ][ 2 ][ 8 ][ 2s+2p @br(2+6) ]\n]\n";
        let config = PrinterConfig {
            table_adjust_width: Some("auto".to_string()),
            table_max_col_width: Some(20),
            table_align: Some("right".to_string()),
            ..Default::default()
        };

        let out = format_source_with_config(src, &config);
        assert!(out.contains("[  殻 ][ 主量子数 n ][ 電子数 2n² ][         小軌道 ]"));
        assert!(out.contains("[ K殻 ][          1 ][          2 ][      1s @br(2) ]"));
    }

    #[test]
    fn test_format_tables_with_config_center_align() {
        let src = "@table[\n[ 殻 ][ 主量子数 n ][ 電子数 2n² ][ 小軌道 ]\n[ K殻 ][ 1 ][ 2 ][ 1s @br(2) ]\n[ L殻 ][ 2 ][ 8 ][ 2s+2p @br(2+6) ]\n]\n";
        let config = PrinterConfig {
            table_adjust_width: Some("auto".to_string()),
            table_max_col_width: Some(20),
            table_align: Some("center".to_string()),
            ..Default::default()
        };

        let out = format_source_with_config(src, &config);
        assert!(out.contains("[ 殻  ][ 主量子数 n ][ 電子数 2n² ][     小軌道     ]"));
        assert!(out.contains("[ K殻 ][     1      ][     2      ][   1s @br(2)    ]"));
    }

    #[test]
    fn test_format_tables_with_per_table_align_arg() {
        let src = "@table(align: [left, right, right, left])[\n[ 殻 ][ 主量子数 n ][ 電子数 2n² ][ 小軌道 ]\n[ K殻 ][ 1 ][ 2 ][ 1s @br(2) ]\n]\n";
        let config = PrinterConfig {
            table_adjust_width: Some("auto".to_string()),
            table_max_col_width: Some(20),
            ..Default::default()
        };

        let out = format_source_with_config(src, &config);
        assert!(out.contains("[ 殻  ][ 主量子数 n ][ 電子数 2n² ][ 小軌道    ]"));
        assert!(out.contains("[ K殻 ][          1 ][          2 ][ 1s @br(2) ]"));
    }

    #[test]
    fn test_format_tables_unicode_superscript_and_cjk_width() {
        let src = "@table(align: [right])[\n[ 電子数 2n² ]\n[ 2 ]\n]\n";
        let config = PrinterConfig {
            table_adjust_width: Some("auto".to_string()),
            table_max_col_width: Some(20),
            ..Default::default()
        };

        let out = format_source_with_config(src, &config);
        assert!(out.contains("[ 電子数 2n² ]"));
        assert!(out.contains("[          2 ]"));
    }

    #[test]
    fn test_format_tables_with_pipe_syntax() {
        let src = "@table\n|[ feature ][ lsp ][ vscode ][ zed ][ neovim ][ helix ]\n|[ highlight ][ o ][ o ][ o ][ o ][ o ]\n|[ suggestion ][ ][ ][ ][ ][ ]\n|[ auto complete ][ ][ ][ ][ ][ ]\n";
        let config = PrinterConfig {
            table_adjust_width: Some("auto".to_string()),
            table_max_col_width: Some(20),
            ..Default::default()
        };

        let out = format_source_with_config(src, &config);
        assert!(out.contains("|[ feature       ][ lsp ][ vscode ][ zed ][ neovim ][ helix ]"));
        assert!(out.contains("|[ highlight     ][ o   ][ o      ][ o   ][ o      ][ o     ]"));
        assert!(out.contains("|[ suggestion    ][     ][        ][     ][        ][       ]"));
        assert!(out.contains("|[ auto complete ][     ][        ][     ][        ][       ]"));
    }

    #[test]
    fn test_format_element_group_order_content_first() {
        let src = "@link(\"https://example.com\")[Example]\n";
        let config = PrinterConfig {
            group_order: Some(GroupOrder::ContentFirst),
            ..Default::default()
        };
        let out = format_source_with_config(src, &config);
        assert_eq!(out, "@link[Example](\"https://example.com\")\n");
    }

    #[test]
    fn test_format_element_group_order_args_first() {
        let src = "@link[Example](\"https://example.com\")\n";
        let config = PrinterConfig {
            group_order: Some(GroupOrder::ArgsFirst),
            ..Default::default()
        };
        let out = format_source_with_config(src, &config);
        assert_eq!(out, "@link(\"https://example.com\")[Example]\n");
    }

    #[test]
    fn test_format_element_group_order_none_preserves_order() {
        let src1 = "@link(\"https://example.com\")[Example]\n";
        let config = PrinterConfig::default();
        let out1 = format_source_with_config(src1, &config);
        assert_eq!(out1, src1);

        let src2 = "@link[Example](\"https://example.com\")\n";
        let out2 = format_source_with_config(src2, &config);
        assert_eq!(out2, src2);
    }

    #[test]
    fn test_format_element_group_order_nested() {
        let src = "@parent(p_arg)[\n  @child(c_arg)[Child Text]\n]\n";
        let config = PrinterConfig {
            group_order: Some(GroupOrder::ContentFirst),
            ..Default::default()
        };
        let out = format_source_with_config(src, &config);
        assert_eq!(out, "@parent[\n  @child[Child Text](c_arg)\n](p_arg)\n");
    }

    #[test]
    fn test_format_element_group_order_with_value_and_connects() {
        let src = "@link(\"https://example.com\")[Example]{rel: \"nofollow\"}:as(button)\n";
        let config = PrinterConfig {
            group_order: Some(GroupOrder::ContentFirst),
            ..Default::default()
        };
        let out = format_source_with_config(src, &config);
        assert_eq!(
            out,
            "@link[Example](\"https://example.com\"){rel: \"nofollow\"}:as(button)\n"
        );
    }

    #[test]
    fn test_format_element_group_order_preserves_code_blocks() {
        let src = "```tomet\n@link(\"url\")[text]\n```\n";
        let config = PrinterConfig {
            group_order: Some(GroupOrder::ContentFirst),
            ..Default::default()
        };
        let out = format_source_with_config(src, &config);
        assert_eq!(out, src);
    }

    #[test]
    fn test_format_element_group_order_link_only() {
        let src = "@link(\"https://example.com\")[Example]\n\n@card(foo: 1)[Bar Content]\n";
        let config = PrinterConfig {
            link_group_order: Some(GroupOrder::ContentFirst),
            ..Default::default()
        };
        let out = format_source_with_config(src, &config);
        assert_eq!(
            out,
            "@link[Example](\"https://example.com\")\n\n@card(foo: 1)[Bar Content]\n"
        );
    }

    #[test]
    fn test_format_element_group_order_link_override_global() {
        let src = "@link(\"https://example.com\")[Example]\n\n@card[Bar Content](foo: 1)\n";
        let config = PrinterConfig {
            group_order: Some(GroupOrder::ArgsFirst),
            link_group_order: Some(GroupOrder::ContentFirst),
            ..Default::default()
        };
        let out = format_source_with_config(src, &config);
        assert_eq!(
            out,
            "@link[Example](\"https://example.com\")\n\n@card(foo: 1)[Bar Content]\n"
        );
    }
}
