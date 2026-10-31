//! Imports a concrete syntax tree (CST) into an [`EditDoc`].

use tomet_cst::{NodeOrToken, SyntaxElement, SyntaxKind as K, SyntaxNode};

use crate::doc::EditDoc;
use crate::node::{
    AttrEntry, AttrSlot, ChildItem, EditValue, EditValueKind, ElementPiece, ElementSlots,
    InlineItem, Node, NodeId, NodeKind,
};

/// Parses `src` into a CST and imports it into an [`EditDoc`].
pub fn import_source(src: &str) -> EditDoc {
    let root = tomet_parser::parse_cst(src);
    import_cst(&root)
}

/// Imports an existing Rowan CST [`SyntaxNode`] into an [`EditDoc`].
pub fn import_cst(root: &SyntaxNode) -> EditDoc {
    let mut doc = EditDoc::new();
    for element in root.children_with_tokens() {
        match element {
            NodeOrToken::Token(token) => {
                doc.add_root_child(ChildItem::Trivia(token.text().to_string()));
            }
            NodeOrToken::Node(node) => {
                let id = import_block_node(&mut doc, &node);
                doc.add_root_child(ChildItem::Node(id));
            }
        }
    }
    doc
}

pub(crate) fn import_block_node(doc: &mut EditDoc, node: &SyntaxNode) -> NodeId {
    let raw = node.text().to_string();
    let id = doc.alloc_id();

    let mut slots = ElementSlots::default();
    let mut inlines = Vec::new();
    let mut heading_prefix = String::new();
    let mut heading_suffix = String::new();

    let kind = match node.kind() {
        K::SECTION => {
            let (sec_kind, prefix, title_inlines, suffix) = import_section(doc, node);
            heading_prefix = prefix;
            inlines = title_inlines;
            heading_suffix = suffix;
            sec_kind
        }
        K::PARAGRAPH => {
            inlines = parse_inline_items(&raw);
            NodeKind::Paragraph { text: raw.clone() }
        }
        K::BLOCK_ELEMENT => {
            let (el_kind, el_slots) = import_block_element(doc, node);
            slots = el_slots;
            el_kind
        }
        K::LIST => import_list(doc, node),
        K::LIST_ITEM => import_list_item(doc, node),
        K::CODE_BLOCK => import_code_block(node),
        K::THEMATIC_BREAK => NodeKind::ThematicBreak { raw: raw.clone() },
        other => NodeKind::Other {
            kind_name: format!("{other:?}"),
        },
    };

    let mut node_item = Node::new(id, kind, raw);
    node_item.slots = slots;
    node_item.inlines = inlines;
    node_item.heading_prefix = heading_prefix;
    node_item.heading_suffix = heading_suffix;
    doc.insert_node(node_item);
    id
}

fn import_section(
    doc: &mut EditDoc,
    node: &SyntaxNode,
) -> (NodeKind, String, Vec<InlineItem>, String) {
    let mut heading_raw = String::new();
    let mut level = 1;
    let mut id_attr = None;
    let mut children = Vec::new();
    let mut heading_prefix = String::new();
    let mut heading_inlines = Vec::new();
    let mut heading_suffix = String::new();

    for element in node.children_with_tokens() {
        match element {
            NodeOrToken::Token(token) => {
                children.push(ChildItem::Trivia(token.text().to_string()));
            }
            NodeOrToken::Node(child_node) => {
                if child_node.kind() == K::SECTION_HEADING {
                    heading_raw = child_node.text().to_string();
                    level = count_heading_level(&heading_raw);
                    id_attr = child_node
                        .children()
                        .find(|c| c.kind() == K::ID_GROUP)
                        .map(|id| {
                            let t = id.text().to_string();
                            t.trim_start_matches("#(")
                                .trim_end_matches(')')
                                .trim()
                                .to_string()
                        });
                    let (p, inl, s) = split_section_heading(&child_node, &heading_raw);
                    heading_prefix = p;
                    heading_inlines = inl;
                    heading_suffix = s;
                } else {
                    let child_id = import_block_node(doc, &child_node);
                    children.push(ChildItem::Node(child_id));
                }
            }
        }
    }

    (
        NodeKind::Section {
            level,
            heading_raw,
            id_attr,
            children,
        },
        heading_prefix,
        heading_inlines,
        heading_suffix,
    )
}

fn split_section_heading(
    heading_node: &SyntaxNode,
    heading_raw: &str,
) -> (String, Vec<InlineItem>, String) {
    if let Some(content_node) = heading_node.children().find(|c| c.kind() == K::CONTENT) {
        let base_offset = usize::from(heading_node.text_range().start());
        let c_start = usize::from(content_node.text_range().start()) - base_offset;
        let c_end = usize::from(content_node.text_range().end()) - base_offset;
        let content_slice = &heading_raw[c_start..c_end];
        if let Some(after_open) = content_slice.strip_prefix('[')
            && let Some(inner) = after_open.strip_suffix(']') {
                let lead_ws_len = inner.len() - inner.trim_start().len();
                let trimmed = inner.trim();
                let trail_ws_start = lead_ws_len + trimmed.len();
                let prefix_end = c_start + 1 + lead_ws_len;
                let suffix_start = c_start + 1 + trail_ws_start;
                let prefix = heading_raw[..prefix_end].to_string();
                let title_str = &heading_raw[prefix_end..suffix_start];
                let suffix = heading_raw[suffix_start..].to_string();
                return (prefix, parse_inline_items(title_str), suffix);
            }
    }

    // Sugar form (`= Title\n` or `== Title ==\n`)
    let (line, newline) = if let Some(s) = heading_raw.strip_suffix("\r\n") {
        (s, "\r\n")
    } else if let Some(s) = heading_raw.strip_suffix('\n') {
        (s, "\n")
    } else {
        (heading_raw, "")
    };
    let eq_and_ws_len = line
        .char_indices()
        .find(|(_, c)| *c != '=' && *c != ' ' && *c != '\t')
        .map_or(line.len(), |(i, _)| i);
    let prefix = line[..eq_and_ws_len].to_string();
    let rest = &line[eq_and_ws_len..];
    let trimmed_rest = rest.trim_end_matches([' ', '\t', '=']);
    let suffix = format!("{}{newline}", &rest[trimmed_rest.len()..]);
    (prefix, parse_inline_items(trimmed_rest), suffix)
}

fn count_heading_level(raw: &str) -> usize {
    let count = raw.trim_start().chars().take_while(|c| *c == '=').count();
    if count == 0 { 1 } else { count }
}

fn import_block_element(doc: &mut EditDoc, node: &SyntaxNode) -> (NodeKind, ElementSlots) {
    let mut name = String::new();
    let mut declaration = false;
    let mut id_attr = None;

    let mut pieces = Vec::new();
    let mut args_slot = None;
    let mut data_slot = None;
    let mut content_children = None;

    for element in node.children_with_tokens() {
        match element {
            NodeOrToken::Token(token) => {
                pieces.push(ElementPiece::Trivia(token.text().to_string()));
            }
            NodeOrToken::Node(child) => match child.kind() {
                K::SIGIL => {
                    let sigil_text = child.text().to_string();
                    declaration = sigil_text.starts_with("::");
                    name = sigil_text
                        .trim_start_matches(['@', ':', '<'])
                        .trim_end_matches('>')
                        .to_string();
                    pieces.push(ElementPiece::Sigil(sigil_text));
                }
                K::ID_GROUP => {
                    let raw_id = child.text().to_string();
                    let parsed_id = raw_id
                        .trim_start_matches("#(")
                        .trim_end_matches(')')
                        .trim()
                        .to_string();
                    id_attr = Some(parsed_id);
                    pieces.push(ElementPiece::Id { raw: raw_id });
                }
                K::ARGS => {
                    args_slot = Some(import_attr_slot(&child));
                    pieces.push(ElementPiece::Args);
                }
                K::VALUE_DATA => {
                    data_slot = Some(import_attr_slot(&child));
                    pieces.push(ElementPiece::Data);
                }
                K::CONTENT => {
                    let raw_content = child.text().to_string();
                    // `|content` is `[content]` without the brackets: it opens
                    // with the `|` and has nothing to close it.
                    let opener = content_opener(&child);
                    let inner = raw_content.strip_prefix('[').unwrap_or(&raw_content);
                    let close_raw = if opener == K::L_BRACKET && inner.ends_with(']') {
                        "]".to_string()
                    } else {
                        String::new()
                    };
                    content_children = Some(import_content_children(doc, &child));
                    pieces.push(ElementPiece::Content {
                        open_raw: if opener == K::PIPE { "|" } else { "[" }.to_string(),
                        close_raw,
                        raw: raw_content,
                    });
                }
                K::CONNECT => {
                    pieces.push(ElementPiece::Connect(child.text().to_string()));
                }
                _ => {
                    pieces.push(ElementPiece::Trivia(child.text().to_string()));
                }
            },
        }
    }

    let slots = ElementSlots {
        pieces,
        id_edited: false,
        args_slot,
        data_slot,
        content_children,
        content_edited: false,
    };

    (
        NodeKind::BlockElement {
            name,
            declaration,
            id_attr,
        },
        slots,
    )
}

/// How a `CONTENT` group opens: `[` for `[content]`, `|` for `|content`.
fn content_opener(content_node: &SyntaxNode) -> K {
    match content_node.first_token().map(|t| t.kind()) {
        Some(K::PIPE) => K::PIPE,
        _ => K::L_BRACKET,
    }
}

fn import_content_children(doc: &mut EditDoc, content_node: &SyntaxNode) -> Vec<ChildItem> {
    let elems: Vec<SyntaxElement> = content_node.children_with_tokens().collect();
    let opener = content_opener(content_node);
    let start = if elems.first().is_some_and(|e| {
        matches!(e, NodeOrToken::Token(t) if t.kind() == K::L_BRACKET || t.kind() == K::PIPE)
    }) {
        1
    } else {
        0
    };
    // Only a bracketed group has a closing token of its own to leave off.
    let end = if opener == K::L_BRACKET
        && elems.len() > start
        && elems
            .last()
            .is_some_and(|e| matches!(e, NodeOrToken::Token(t) if t.kind() == K::R_BRACKET))
    {
        elems.len() - 1
    } else {
        elems.len()
    };

    let mut children = Vec::new();
    for elem in &elems[start..end] {
        match elem {
            NodeOrToken::Token(token) => {
                children.push(ChildItem::Trivia(token.text().to_string()));
            }
            NodeOrToken::Node(child_node) => {
                let child_id = import_block_node(doc, child_node);
                children.push(ChildItem::Node(child_id));
            }
        }
    }
    children
}

/// Imports an `ARGS`, `VALUE_DATA`, or `MAP_LITERAL` CST node into an [`AttrSlot`].
pub fn import_attr_slot(group_node: &SyntaxNode) -> AttrSlot {
    let raw = group_node.text().to_string();
    let elems: Vec<SyntaxElement> = group_node.children_with_tokens().collect();

    let (open_delim, start) = match elems.first() {
        Some(NodeOrToken::Token(t))
            if matches!(t.kind(), K::L_PAREN | K::L_BRACE | K::L_BRACKET) =>
        {
            (t.text().to_string(), 1)
        }
        _ => (String::new(), 0),
    };

    let (close_delim, end) = match elems.last() {
        Some(NodeOrToken::Token(t))
            if elems.len() > start
                && matches!(t.kind(), K::R_PAREN | K::R_BRACE | K::R_BRACKET) =>
        {
            (t.text().to_string(), elems.len() - 1)
        }
        _ => (String::new(), elems.len()),
    };

    let mut pending_trivia = String::new();
    let mut leading_trivia = String::new();
    let mut entries: Vec<AttrEntry> = Vec::new();

    for elem in &elems[start..end] {
        match elem {
            NodeOrToken::Token(t) => {
                pending_trivia.push_str(t.text());
            }
            NodeOrToken::Node(entry_node)
                if matches!(entry_node.kind(), K::MAP_ENTRY | K::SEQ_ITEM) =>
            {
                let (lead_ws, mut entry, trail_ws) = import_attr_entry(entry_node);
                pending_trivia.push_str(&lead_ws);
                if entries.is_empty() {
                    leading_trivia = std::mem::take(&mut pending_trivia);
                    entry.leading_sep = String::new();
                } else {
                    entry.leading_sep = std::mem::take(&mut pending_trivia);
                }
                entries.push(entry);
                pending_trivia.push_str(&trail_ws);
            }
            NodeOrToken::Node(other) => {
                pending_trivia.push_str(&other.text().to_string());
            }
        }
    }

    let trailing_trivia = if entries.is_empty() {
        leading_trivia = pending_trivia;
        String::new()
    } else {
        pending_trivia
    };

    AttrSlot {
        raw,
        open_delim,
        close_delim,
        edited: false,
        leading_trivia,
        trailing_trivia,
        entries,
    }
}

fn import_attr_entry(entry_node: &SyntaxNode) -> (String, AttrEntry, String) {
    let mut inner: Vec<SyntaxElement> = entry_node.children_with_tokens().collect();

    // 1. Pop trailing whitespace tokens so brace padding (e.g. ` ` before `}`) stays in group trivia.
    let mut trail_ws_parts = Vec::new();
    while inner
        .last()
        .is_some_and(|e| matches!(e, NodeOrToken::Token(t) if t.kind() == K::WHITESPACE))
    {
        if let Some(NodeOrToken::Token(t)) = inner.pop() {
            trail_ws_parts.push(t.text().to_string());
        }
    }
    trail_ws_parts.reverse();
    let trail_ws = trail_ws_parts.concat();

    // 2. Pop trailing inline comment (and any whitespace directly before it) if present.
    let mut trailing_comment = String::new();
    if inner
        .last()
        .is_some_and(|e| matches!(e, NodeOrToken::Token(t) if t.kind() == K::COMMENT))
    {
        let mut comment_parts = Vec::new();
        if let Some(NodeOrToken::Token(c)) = inner.pop() {
            comment_parts.push(c.text().to_string());
        }
        while inner
            .last()
            .is_some_and(|e| matches!(e, NodeOrToken::Token(t) if t.kind() == K::WHITESPACE))
        {
            if let Some(NodeOrToken::Token(w)) = inner.pop() {
                comment_parts.push(w.text().to_string());
            }
        }
        comment_parts.reverse();
        trailing_comment = comment_parts.concat();
    }

    // 3. Pop leading whitespace tokens if any.
    let mut lead_ws = String::new();
    while inner
        .first()
        .is_some_and(|e| matches!(e, NodeOrToken::Token(t) if t.kind() == K::WHITESPACE))
    {
        if let NodeOrToken::Token(t) = inner.remove(0) {
            lead_ws.push_str(t.text());
        }
    }

    let (key, key_raw, sep_raw, val_elems) = if entry_node.kind() == K::MAP_ENTRY {
        let mut idx = 0;
        let (key, key_raw) = match inner.get(idx) {
            Some(NodeOrToken::Token(t)) => {
                idx += 1;
                let kr = t.text().to_string();
                let k = if t.kind() == K::STRING_LITERAL {
                    unquote_key(&kr)
                } else {
                    kr.clone()
                };
                (k, kr)
            }
            _ => (String::new(), String::new()),
        };
        let mut sep = String::new();
        while let Some(NodeOrToken::Token(t)) = inner.get(idx) {
            if t.kind() == K::WHITESPACE || t.kind() == K::COLON {
                sep.push_str(t.text());
                idx += 1;
                if t.kind() == K::COLON {
                    while let Some(NodeOrToken::Token(ws)) = inner.get(idx) {
                        if ws.kind() == K::WHITESPACE {
                            sep.push_str(ws.text());
                            idx += 1;
                        } else {
                            break;
                        }
                    }
                    break;
                }
            } else {
                break;
            }
        }
        (key, key_raw, sep, &inner[idx..])
    } else {
        (String::new(), String::new(), String::new(), inner.as_slice())
    };

    let value = import_edit_value(val_elems);
    let raw = format!("{key_raw}{sep_raw}{}{trailing_comment}", value.raw);

    (
        lead_ws,
        AttrEntry {
            leading_sep: String::new(),
            key,
            key_raw,
            sep_raw,
            value,
            trailing_comment,
            raw,
            edited: false,
        },
        trail_ws,
    )
}

fn unquote_key(s: &str) -> String {
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        s[1..s.len() - 1].replace("\\\"", "\"").replace("\\\\", "\\")
    } else {
        s.to_string()
    }
}

fn import_edit_value(elems: &[SyntaxElement]) -> EditValue {
    let mut raw = String::new();
    for e in elems {
        match e {
            NodeOrToken::Token(t) => raw.push_str(t.text()),
            NodeOrToken::Node(n) => raw.push_str(&n.text().to_string()),
        }
    }

    if elems.len() == 1
        && let NodeOrToken::Node(node) = &elems[0] {
            match node.kind() {
                K::MAP_LITERAL => {
                    return EditValue {
                        raw,
                        edited: false,
                        kind: EditValueKind::Map(import_attr_slot(node)),
                    };
                }
                K::INLINE_ELEMENT => {
                    let sigil_raw = node
                        .children()
                        .find(|c| c.kind() == K::SIGIL)
                        .map(|c| c.text().to_string())
                        .unwrap_or_default();
                    let name = sigil_raw.trim_start_matches(['@', ':', '$']).to_string();
                    let args = node
                        .children()
                        .find(|c| c.kind() == K::ARGS)
                        .map(|c| import_attr_slot(&c));
                    let data = node
                        .children()
                        .find(|c| c.kind() == K::VALUE_DATA)
                        .map(|c| import_attr_slot(&c));
                    let content_raw = node
                        .children()
                        .find(|c| c.kind() == K::CONTENT)
                        .map(|c| c.text().to_string());
                    return EditValue {
                        raw,
                        edited: false,
                        kind: EditValueKind::Element {
                            sigil_raw,
                            name,
                            args,
                            data,
                            content_raw,
                        },
                    };
                }
                _ => {}
            }
        }

    EditValue {
        raw: raw.clone(),
        edited: false,
        kind: EditValueKind::Scalar(raw),
    }
}

/// Parses a raw value string into an [`EditValue`].
pub fn parse_edit_value_str(raw: &str) -> EditValue {
    let wrapped = format!("::v{{ k: {raw} }}");
    let cst = tomet_parser::parse_cst(&wrapped);
    if let Some(entry_node) = cst.descendants().find(|n| n.kind() == K::MAP_ENTRY) {
        let (_, entry, _) = import_attr_entry(&entry_node);
        if entry.value.raw == raw {
            return entry.value;
        }
    }
    EditValue {
        raw: raw.to_string(),
        edited: false,
        kind: EditValueKind::Scalar(raw.to_string()),
    }
}

/// Segments a paragraph or heading string into structured [`InlineItem`]s while
/// guaranteeing that concatenating each item's `raw_str()` reproduces `src` byte-for-byte.
pub fn parse_inline_items(src: &str) -> Vec<InlineItem> {
    if src.is_empty() {
        return Vec::new();
    }

    let (body, trailing_newline) = if let Some(stripped) = src.strip_suffix("\r\n") {
        (stripped, Some("\r\n"))
    } else if let Some(stripped) = src.strip_suffix('\n') {
        (stripped, Some("\n"))
    } else {
        (src, None)
    };

    // Find inline element byte ranges from the CST of `body`
    let cst = tomet_parser::parse_cst(body);
    let mut inline_elements: Vec<(usize, usize, SyntaxNode)> = Vec::new();
    for child in cst.children() {
        for sub in child.children() {
            if sub.kind() == K::INLINE_ELEMENT {
                let start = usize::from(sub.text_range().start());
                let end = usize::from(sub.text_range().end());
                if end <= body.len() {
                    inline_elements.push((start, end, sub));
                }
            }
        }
    }

    let mut items = Vec::new();
    let mut text_start = 0usize;
    let mut pos = 0usize;

    let flush_text = |items: &mut Vec<InlineItem>, text_start: &mut usize, up_to: usize| {
        if up_to > *text_start {
            items.push(InlineItem::Text(body[*text_start..up_to].to_string()));
        }
        *text_start = up_to;
    };

    while pos < body.len() {
        // 1. Check CST inline element starting at `pos`
        if let Some((_, end, el_node)) = inline_elements.iter().find(|(s, _, _)| *s == pos) {
            flush_text(&mut items, &mut text_start, pos);
            let el_raw = body[pos..*end].to_string();
            let mut dummy_doc = EditDoc::new();
            let (kind, slots) = import_block_element(&mut dummy_doc, el_node);
            let name = match kind {
                NodeKind::BlockElement { name, .. } => name,
                _ => String::new(),
            };
            items.push(InlineItem::Element {
                name,
                raw: el_raw,
                slots,
            });
            pos = *end;
            text_start = pos;
            continue;
        }

        let rest = &body[pos..];

        // 2. Block comment `/* ... */`
        if rest.starts_with("/*") {
            flush_text(&mut items, &mut text_start, pos);
            let end = rest
                .find("*/")
                .map_or(body.len(), |idx| pos + idx + 2);
            items.push(InlineItem::Trivia(body[pos..end].to_string()));
            pos = end;
            text_start = pos;
            continue;
        }

        // 3. Line comment `// ...` preceded by whitespace or line start
        if rest.starts_with("//") && is_ws_boundary(body[..pos].chars().next_back()) {
            flush_text(&mut items, &mut text_start, pos);
            let end = rest.find('\n').map_or(body.len(), |idx| pos + idx);
            items.push(InlineItem::Trivia(body[pos..end].to_string()));
            pos = end;
            text_start = pos;
            continue;
        }

        // 4. Backtick inline code `` `...` ``
        if rest.starts_with('`') {
            let fence_len = rest.chars().take_while(|c| *c == '`').count();
            let after_open = pos + fence_len;
            if let Some((inner_end, close_end)) = scan_backtick_close(body, after_open, fence_len) {
                flush_text(&mut items, &mut text_start, pos);
                items.push(InlineItem::Sugar {
                    kind: "raw".to_string(),
                    delim: "`".repeat(fence_len),
                    inner_raw: body[after_open..inner_end].to_string(),
                    raw: body[pos..close_end].to_string(),
                });
                pos = close_end;
                text_start = pos;
                continue;
            }
        }

        // 5. Delimited inline formatting (`**`, `__`, `~~`, `*`, `_`)
        if let Some((kind, delim, inner_start, inner_end, close_end)) =
            try_scan_delimited_sugar(body, pos)
        {
            flush_text(&mut items, &mut text_start, pos);
            items.push(InlineItem::Sugar {
                kind: kind.to_string(),
                delim: delim.to_string(),
                inner_raw: body[inner_start..inner_end].to_string(),
                raw: body[pos..close_end].to_string(),
            });
            pos = close_end;
            text_start = pos;
            continue;
        }

        let ch_len = rest.chars().next().map_or(1, char::len_utf8);
        pos += ch_len;
    }

    flush_text(&mut items, &mut text_start, body.len());

    if let Some(nl) = trailing_newline {
        items.push(InlineItem::Trivia(nl.to_string()));
    }

    items
}

fn is_ws_boundary(c: Option<char>) -> bool {
    matches!(c, None | Some(' ' | '\t' | '\n' | '\r'))
}

fn scan_backtick_close(body: &str, mut idx: usize, fence_len: usize) -> Option<(usize, usize)> {
    while idx < body.len() {
        let rest = &body[idx..];
        let ch = rest.chars().next()?;
        if ch == '\n' || ch == '\r' {
            return None;
        }
        if ch == '`' {
            let run_len = rest.chars().take_while(|c| *c == '`').count();
            if run_len == fence_len {
                return Some((idx, idx + run_len));
            }
            idx += run_len;
        } else {
            idx += ch.len_utf8();
        }
    }
    None
}

fn try_scan_delimited_sugar(
    body: &str,
    pos: usize,
) -> Option<(&'static str, &'static str, usize, usize, usize)> {
    const DELIMITERS: [(&str, &str); 5] = [
        ("**", "strong"),
        ("__", "strong"),
        ("~~", "strikeout"),
        ("*", "em"),
        ("_", "em"),
    ];
    let rest = &body[pos..];
    let prev_char = body[..pos].chars().next_back();
    for (delim, kind) in DELIMITERS {
        if !rest.starts_with(delim) {
            continue;
        }
        if delim.starts_with('_') && prev_char.is_some_and(char::is_alphanumeric) {
            continue;
        }
        let inner_start = pos + delim.len();
        let first_inner = body[inner_start..].chars().next();
        if is_ws_boundary(first_inner) {
            continue;
        }
        let mut cursor = inner_start;
        while cursor < body.len() {
            let tail = &body[cursor..];
            if tail.starts_with(delim)
                && (delim == "~~" || !is_ws_boundary(body[..cursor].chars().next_back()))
            {
                let close_end = cursor + delim.len();
                if delim.starts_with('_')
                    && body[close_end..].chars().next().is_some_and(char::is_alphanumeric)
                {
                    cursor += delim.len();
                    continue;
                }
                if cursor > inner_start {
                    return Some((kind, delim, inner_start, cursor, close_end));
                }
            }
            let ch = tail.chars().next()?;
            if ch == '\n' {
                return None;
            }
            cursor += ch.len_utf8();
        }
    }
    None
}

fn import_list(doc: &mut EditDoc, node: &SyntaxNode) -> NodeKind {
    let mut children = Vec::new();
    for element in node.children_with_tokens() {
        match element {
            NodeOrToken::Token(token) => {
                children.push(ChildItem::Trivia(token.text().to_string()));
            }
            NodeOrToken::Node(child_node) => {
                let child_id = import_block_node(doc, &child_node);
                children.push(ChildItem::Node(child_id));
            }
        }
    }
    NodeKind::List { children }
}

fn import_list_item(doc: &mut EditDoc, node: &SyntaxNode) -> NodeKind {
    let mut marker = String::new();
    let mut children = Vec::new();

    let id_attr = node.children().find(|c| c.kind() == K::ID_GROUP).map(|id| {
        let t = id.text().to_string();
        t.trim_start_matches("#(")
            .trim_end_matches(')')
            .trim()
            .to_string()
    });

    for element in node.children_with_tokens() {
        match element {
            NodeOrToken::Token(token) => {
                if marker.is_empty()
                    && (token.kind() == K::MINUS
                        || token.kind() == K::PLUS
                        || token.kind() == K::STAR)
                {
                    marker = token.text().to_string();
                }
                children.push(ChildItem::Trivia(token.text().to_string()));
            }
            NodeOrToken::Node(child_node) => {
                let child_id = import_block_node(doc, &child_node);
                children.push(ChildItem::Node(child_id));
            }
        }
    }

    NodeKind::ListItem {
        marker,
        text: node.text().to_string(),
        id_attr,
        children,
    }
}

fn import_code_block(node: &SyntaxNode) -> NodeKind {
    let code = node.text().to_string();
    NodeKind::CodeBlock { lang: None, code }
}
