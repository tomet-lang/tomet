//! Rendering inline content and block-permitted content runs.

use tomet_ast::{Block, Inline, Placement};
use tomet_config::PrinterConfig;

/// Extracts the verbatim text of a `raw` element's content when it consists of
/// a single paragraph of raw/plain text and line breaks.
///
/// Used when printing `raw` as a backtick code span (`` `...` ``) or fenced
/// code block (```` ```...``` ````), where backslash escapes are not interpreted
/// and must not be inserted.
pub(crate) fn raw_content_text(blocks: &[Block]) -> Option<String> {
    let [Block::Paragraph(p)] = blocks else {
        return None;
    };
    let mut out = String::new();
    for inline in &p.content {
        match inline {
            Inline::Raw(r) => out.push_str(&r.value),
            Inline::Text(t) => out.push_str(&t.value),
            Inline::SoftBreak(_) | Inline::LineBreak(_) => out.push('\n'),
            Inline::Interp(_) | Inline::Element(_) | Inline::Hole(_) => return None,
        }
    }
    Some(out)
}

/// Renders a run of inlines.
///
/// A `[content]` group holds `Inline`s, but one of them may be
/// block-placed -- an element written at a line start inside the group,
/// the way `@references[` holds its entries. Placement is spelled with
/// line breaks rather than with a sigil, so such an element has to start
/// and end its own line here or it would come back from the parser as
/// part of the running text.
#[deny(clippy::wildcard_enum_match_arm)]
pub(crate) fn render_inlines(inlines: &[Inline], config: &PrinterConfig) -> String {
    let mut s = String::new();
    let mut idx = 0;
    while idx < inlines.len() {
        let prev_was_form = idx > 0
            && match &inlines[idx - 1] {
                Inline::Element(el) => {
                    el.placement == Placement::Inline && !el.name.is_bare_named("raw")
                }
                Inline::Hole(hole) => hole.placement == Placement::Inline,
                Inline::Text(_)
                | Inline::Raw(_)
                | Inline::SoftBreak(_)
                | Inline::LineBreak(_)
                | Inline::Interp(_) => false,
            };
        match &inlines[idx] {
            Inline::Text(t) => {
                if matches!(inlines.get(idx + 1), Some(Inline::Text(_) | Inline::Raw(_))) {
                    let mut combined = t.value.clone();
                    idx += 1;
                    while let Some(nt) = inlines.get(idx).and_then(text_or_raw_str) {
                        combined.push_str(nt);
                        idx += 1;
                    }
                    escape_and_push_text(&mut s, &combined, prev_was_form, &inlines[idx..]);
                    continue;
                }
                escape_and_push_text(&mut s, &t.value, prev_was_form, &inlines[idx + 1..]);
            }
            // `Inline::Raw` inside a backtick span or fence is handled via
            // `raw_content_text` in `element.rs`. Any `Inline::Raw` that reaches
            // `render_inlines` is inside a bracketed `@raw[...]` fallback, where
            // normal inline parsing applies.
            Inline::Raw(t) => {
                if matches!(inlines.get(idx + 1), Some(Inline::Text(_) | Inline::Raw(_))) {
                    let mut combined = t.value.clone();
                    idx += 1;
                    while let Some(nt) = inlines.get(idx).and_then(text_or_raw_str) {
                        combined.push_str(nt);
                        idx += 1;
                    }
                    escape_and_push_text(&mut s, &combined, prev_was_form, &inlines[idx..]);
                    continue;
                }
                escape_and_push_text(&mut s, &t.value, prev_was_form, &inlines[idx + 1..]);
            }
            // A literal newline, not `softbreak_join`'s space/nothing.
            // Several callers (`render_list_with_indent`'s
            // `list_multiline_style_content`, `render_callout`'s
            // `callout_content_style`) split this function's output on
            // `'\n'` to lay wrapped content out across multiple indented
            // lines -- a feature that used to ride on markdown import
            // embedding a literal `'\n'` straight into `Text.value` for a
            // softbreak. Folding to a space here would silently turn that
            // into always-one-line output.
            Inline::SoftBreak(_) | Inline::LineBreak(_) => s.push('\n'),
            Inline::Interp(interp) => s.push_str(&crate::element::render_interp(interp)),
            Inline::Element(el) => push_node(
                &mut s,
                el.placement == Placement::Block,
                crate::element::render_element(el.as_view(), config),
            ),
            Inline::Hole(hole) => push_node(
                &mut s,
                hole.placement == Placement::Block,
                crate::element::render_hole(hole, config),
            ),
        }
        idx += 1;
    }
    s
}

fn text_or_raw_str(inline: &Inline) -> Option<&str> {
    match inline {
        Inline::Text(t) => Some(&t.value),
        Inline::Raw(r) => Some(&r.value),
        Inline::SoftBreak(_)
        | Inline::LineBreak(_)
        | Inline::Interp(_)
        | Inline::Element(_)
        | Inline::Hole(_) => None,
    }
}

/// An element or a hole, `rendered`, in the run being built in `s`: a
/// block-placed one starts and ends its own line.
fn push_node(s: &mut String, block: bool, rendered: String) {
    if block && !s.is_empty() && !s.ends_with('\n') {
        s.push('\n');
    }
    s.push_str(&rendered);
    if block {
        s.push('\n');
    }
}

/// Renders an `Element.content` (`Vec<Block>`, see `tmtroot/docs/spec/
/// feature/content-shape.tmt`) back to source text.
///
/// The common case -- exactly one plain paragraph, which is what ordinary
/// inline usage (`@link(...)[Tomet]`, `@em[text]`, ...) always parses to --
/// prints byte-for-byte what `render_inlines` on that paragraph's own
/// content always has, so this is not a behaviour change for anything that
/// doesn't actually use the new block-permitting content. Multiple blocks,
/// or a block that isn't a paragraph, go through `render_blocks`, the
/// joining a document and a section use for their own blocks.
pub(crate) fn render_content_blocks(blocks: &[Block], config: &PrinterConfig) -> String {
    if let [Block::Paragraph(p)] = blocks {
        return render_inlines(&p.content, config);
    }
    let mut out = String::new();
    crate::block::render_blocks(blocks, config, &mut out);
    out
}

/// Appends `text` to `s`, inserting backslash escapes before sequences that
/// would otherwise be parsed as block or inline syntax.
fn escape_and_push_text(s: &mut String, text: &str, prev_was_form: bool, rest_inlines: &[Inline]) {
    if text.is_empty() {
        return;
    }
    let bytes = text.as_bytes();
    let unmatched = if bytes.contains(&b'[') || bytes.contains(&b']') {
        Some(unmatched_brackets(bytes))
    } else {
        None
    };

    let has_next_on_line = rest_inlines
        .first()
        .is_some_and(|inl| !matches!(inl, Inline::SoftBreak(_) | Inline::LineBreak(_)));
    let next_on_line_has_backtick = rest_inlines_have_backtick_on_line(rest_inlines);

    let at_line_start = s.is_empty() || s.ends_with('\n');
    let mut line_escape_idx = if at_line_start {
        let line_end = text.find('\n').unwrap_or(text.len());
        line_start_trigger_offset(&text[..line_end])
    } else {
        None
    };

    let mut form_attach_idx = if prev_was_form {
        form_attachment_offset(text)
    } else {
        None
    };

    let mut i = 0;
    while i < bytes.len() {
        if Some(i) == line_escape_idx {
            line_escape_idx = None;
            s.push('\\');
            let b = bytes[i];
            if matches!(b, b'=' | b'-' | b'`' | b':') {
                let start = i;
                while i < bytes.len() && bytes[i] == b {
                    i += 1;
                }
                s.push_str(&text[start..i]);
                continue;
            }
            s.push(b as char);
            i += 1;
            continue;
        }

        if Some(i) == form_attach_idx {
            form_attach_idx = None;
            s.push('\\');
            let b = bytes[i];
            s.push(b as char);
            i += 1;
            continue;
        }

        let b = bytes[i];

        if b == b'\n' {
            s.push('\n');
            i += 1;
            let line_end = text[i..].find('\n').map_or(text.len(), |off| i + off);
            line_escape_idx = line_start_trigger_offset(&text[i..line_end]).map(|off| i + off);
            continue;
        }

        if b == b'\\' {
            let start = i;
            while i < bytes.len() && bytes[i] == b'\\' {
                i += 1;
            }
            let run_len = i - start;
            let next_b = bytes.get(i).copied();
            if run_len >= 2 || next_b.is_none_or(|nb| nb.is_ascii_punctuation()) {
                for _ in 0..run_len {
                    s.push_str("\\\\");
                }
            } else {
                s.push('\\');
            }
            continue;
        }

        if text[i..].starts_with("/*") {
            s.push_str("\\/*");
            i += 2;
            continue;
        }

        if text[i..].starts_with("//") {
            let preceded_by_ws_or_bol = s.is_empty()
                || s.ends_with(['\n', ' ', '\t'])
                || (i > 0 && matches!(bytes[i - 1], b'\n' | b' ' | b'\t'));
            if preceded_by_ws_or_bol {
                s.push_str("\\//");
            } else {
                s.push_str("//");
            }
            i += 2;
            continue;
        }

        if b == b'`' {
            let start = i;
            while i < bytes.len() && bytes[i] == b'`' {
                i += 1;
            }
            let line_end = text[i..].find('\n').map_or(text.len(), |off| i + off);
            let more_backticks = text[i..line_end].contains('`')
                || (line_end == text.len() && next_on_line_has_backtick);
            if more_backticks {
                s.push('\\');
            }
            s.push_str(&text[start..i]);
            continue;
        }

        if matches!(b, b'*' | b'_' | b'~') {
            let start = i;
            while i < bytes.len() && bytes[i] == b {
                i += 1;
            }
            let run_len = i - start;
            if b == b'~' && run_len < 2 {
                s.push_str(&text[start..i]);
                continue;
            }
            let prev_ch = s.chars().next_back();
            let after_ch = if i < text.len() {
                text[i..].chars().next()
            } else if has_next_on_line {
                Some('@')
            } else {
                None
            };
            let is_boundary = matches!(after_ch, None | Some(' ' | '\t' | '\n' | '\r'));
            let can_open =
                !is_boundary && (b != b'_' || !prev_ch.is_some_and(|c| c.is_alphanumeric()));
            if can_open {
                s.push('\\');
            }
            s.push_str(&text[start..i]);
            continue;
        }

        if b == b'@' {
            if is_at_form_trigger(&text[i + 1..], has_next_on_line) {
                s.push_str("\\@");
            } else {
                s.push('@');
            }
            i += 1;
            continue;
        }

        if b == b'$' {
            if is_dollar_interp_trigger(&text[i + 1..]) {
                s.push_str("\\$");
            } else {
                s.push('$');
            }
            i += 1;
            continue;
        }

        if text[i..].starts_with("<?") {
            s.push_str("\\<?");
            i += 2;
            continue;
        }

        if matches!(b, b'[' | b']') && unmatched.as_ref().is_some_and(|u| u[i]) {
            s.push('\\');
            s.push(b as char);
            i += 1;
            continue;
        }

        let ch = text[i..].chars().next().expect("i < bytes.len()");
        s.push(ch);
        i += ch.len_utf8();
    }
}

/// Returns the byte offset within `line` (after leading spaces/tabs) if `line`
/// begins with a token sequence that would trigger a block or line-start construct.
fn line_start_trigger_offset(line: &str) -> Option<usize> {
    let trimmed = line.trim_start_matches([' ', '\t']);
    let indent = line.len() - trimmed.len();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.starts_with(';') {
        return Some(indent);
    }
    if trimmed.starts_with('=') {
        let after = trimmed.trim_start_matches('=');
        if after.is_empty()
            || after.starts_with([' ', '\t', '\r', '(', '[', '{', '|'])
            || after.starts_with("#(")
            || after.starts_with("//")
            || after.starts_with("/*")
        {
            return Some(indent);
        }
    }
    if trimmed.starts_with("---") {
        let after = trimmed.trim_start_matches('-');
        if after.is_empty()
            || after.starts_with([' ', '\t', '\r', '['])
            || after.starts_with("//")
            || after.starts_with("/*")
        {
            return Some(indent);
        }
    }
    if trimmed.starts_with('-') {
        let after = trimmed
            .strip_prefix("-.")
            .or_else(|| trimmed.strip_prefix('-'))
            .unwrap_or("");
        if after.is_empty()
            || after.starts_with([' ', '\t', '\r', '@', '(', '[', '{', '|'])
            || after.starts_with("#(")
        {
            return Some(indent);
        }
    }
    if trimmed.starts_with("``") {
        return Some(indent);
    }
    if let Some(after) = trimmed.strip_prefix("::")
        && after.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
    {
        return Some(indent);
    }
    if let Some(after_at) = trimmed.strip_prefix('@')
        && let Some(after_name) = skip_tomet_name(after_at)
    {
        let after_ws = after_name.trim_start_matches([' ', '\t', '\r']);
        if after_ws.is_empty() || after_ws.starts_with("//") || after_ws.starts_with("/*") {
            return Some(indent);
        }
    }
    None
}

/// If `text` immediately follows an inline form (`@name...` or `<?...>`), returns
/// the byte offset of a leading group/connect opener that would otherwise attach
/// to that preceding form.
fn form_attachment_offset(text: &str) -> Option<usize> {
    let trimmed = text.trim_start_matches([' ', '\t']);
    let indent = text.len() - trimmed.len();
    if trimmed.starts_with(['(', '[', '{', '|']) || trimmed.starts_with("#(") {
        return Some(indent);
    }
    if indent == 0 && opens_connect(trimmed, false) {
        return Some(0);
    }
    None
}

/// Whether `s` starts a connect: `::name(...)`, `::{...}`, or the retired `:name(...)`, which
/// the parser still reads as one (to refuse it). On the line after its owner only the retired
/// one is: a `::` at the start of a line is a declaration.
fn opens_connect(s: &str, next_line: bool) -> bool {
    let Some(after_colon) = s.strip_prefix(':') else {
        return false;
    };
    let after_colon = match after_colon.strip_prefix(':') {
        Some(_) if next_line => return false,
        Some(rest) => rest,
        None => after_colon,
    };
    let after_name = skip_tomet_name(after_colon).unwrap_or(after_colon);
    after_name.starts_with(['(', '[', '{', '|'])
}

fn is_at_form_trigger(after_at: &str, has_next_on_line: bool) -> bool {
    let Some(after_name) = skip_tomet_name(after_at) else {
        return false;
    };
    let after_ws = after_name.trim_start_matches([' ', '\t']);
    if after_ws.is_empty() {
        return has_next_on_line;
    }
    if starts_with_form_group_or_connect(after_ws, false) {
        return true;
    }
    if let Some(next_line) = after_ws
        .strip_prefix("\r\n")
        .or_else(|| after_ws.strip_prefix('\n'))
    {
        let nl = next_line.trim_start_matches([' ', '\t']);
        if starts_with_form_group_or_connect(nl, true) {
            return true;
        }
    }
    false
}

fn starts_with_form_group_or_connect(s: &str, next_line: bool) -> bool {
    if s.starts_with(['(', '[', '{', '|']) || s.starts_with("#(") {
        return true;
    }
    opens_connect(s, next_line)
}

fn is_dollar_interp_trigger(after_dollar: &str) -> bool {
    if after_dollar.starts_with('{') {
        return true;
    }
    skip_tomet_name(after_dollar).is_some_and(|after_name| after_name.starts_with('('))
}

fn skip_tomet_name(s: &str) -> Option<&str> {
    let bytes = s.as_bytes();
    if bytes.is_empty() || !(bytes[0].is_ascii_alphabetic() || bytes[0] == b'_') {
        return None;
    }
    let mut i = 1;
    while i < bytes.len() {
        let b = bytes[i];
        if b.is_ascii_alphanumeric() || b == b'_' || b == b'-' {
            i += 1;
        } else if b == b'.'
            && i + 1 < bytes.len()
            && (bytes[i + 1].is_ascii_alphanumeric()
                || bytes[i + 1] == b'_'
                || bytes[i + 1] == b'-')
        {
            i += 2;
        } else {
            break;
        }
    }
    Some(&s[i..])
}

fn rest_inlines_have_backtick_on_line(rest: &[Inline]) -> bool {
    for inl in rest {
        match inl {
            Inline::SoftBreak(_) | Inline::LineBreak(_) => return false,
            Inline::Element(el) if el.placement == Placement::Block => return false,
            Inline::Hole(h) if h.placement == Placement::Block => return false,
            Inline::Element(el) if el.name.is_bare_named("raw") => return true,
            Inline::Text(t) => {
                let first_line = t.value.split('\n').next().unwrap_or("");
                if first_line.contains('`') {
                    return true;
                }
                if t.value.contains('\n') {
                    return false;
                }
            }
            Inline::Raw(r) => {
                let first_line = r.value.split('\n').next().unwrap_or("");
                if first_line.contains('`') {
                    return true;
                }
                if r.value.contains('\n') {
                    return false;
                }
            }
            Inline::Interp(_) | Inline::Element(_) | Inline::Hole(_) => {}
        }
    }
    false
}

fn unmatched_brackets(bytes: &[u8]) -> Vec<bool> {
    let mut unmatched = vec![false; bytes.len()];
    let mut open_stack = Vec::new();
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'[' {
            open_stack.push(i);
        } else if b == b']' && open_stack.pop().is_none() {
            unmatched[i] = true;
        }
    }
    for i in open_stack {
        unmatched[i] = true;
    }
    unmatched
}
