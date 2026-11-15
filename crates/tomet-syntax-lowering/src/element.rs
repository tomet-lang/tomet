//! The values a group holds.
//!
//! Lowering of group values: `(args)` ([`lower_paren_value`]), `{value}`
//! ([`lower_value_group`]) and `#(id)` ([`lower_hash_id`]).
//! They fold CST nodes into AST values and groups without re-reading source text.

use crate::error::{Error, Result};
use crate::lower::error_node::lower_error;
use crate::lower::form::{LoweredSlot, lower_element, lower_slot, slot_of};
use crate::lower::groups::{lower_content, lower_embedded};
use tomet_ast::{Block, Connect, Data, Entry, Group, Id, Placement, Tuple, TupleEntry, Value};
use tomet_cst::{SyntaxElement, SyntaxKind as K, SyntaxNode, SyntaxToken};
use tomet_tree::ElementExt;

pub(crate) fn duplicate_key_error(pos: usize, key: &str) -> Error {
    err(pos, format!("duplicate key `{key}`"))
}

fn err(pos: usize, message: impl Into<String>) -> Error {
    Error {
        message: message.into(),
        line: 0,
        column: 0,
        offset: pos,
    }
}

/// What [`lower_groups`] reads: the groups every node takes, before the node
/// that owns them claims the ones it accepts.
#[derive(Default)]
pub(crate) struct Groups {
    pub data: Data,
    pub content: Option<Vec<Block>>,
    pub id: Option<Id>,
    pub connects: Vec<Connect>,
    /// The nodes that wrote `data.args` and `data.value` so far, in the order they were read:
    /// what a later nameless connect clashes with, when it writes an entry a second time.
    pub args_writers: Vec<SyntaxNode>,
    pub value_writers: Vec<SyntaxNode>,
}

/// Lowers an `ARGS` node (`(...)`) into a [`Tuple`].
///
/// An empty `()` is `Tuple::new()`.
/// A single positional value `(x)` is a tuple with one unnamed entry (no unboxing).
pub(crate) fn lower_paren_value(node: &SyntaxNode) -> Result<Tuple> {
    let closed = node.children_with_tokens().any(|e| e.kind() == K::R_PAREN)
        || (node.children().any(|c| c.kind() == K::ERROR)
            && node.text().to_string().trim_end().ends_with(')'));
    let entry_children: Vec<SyntaxNode> = node
        .children()
        .filter(|c| matches!(c.kind(), K::MAP_ENTRY | K::SEQ_ITEM | K::ERROR))
        .collect();

    if entry_children.is_empty() {
        if closed {
            return Ok(Tuple::new());
        } else {
            return Err(err(
                usize::from(node.text_range().end()),
                "expected a value",
            ));
        }
    }

    let entries = lower_tuple_entries(node, false, true, false)?;

    if !closed {
        return Err(err(usize::from(node.text_range().end()), "expected ')'"));
    }

    Ok(entries)
}

/// Lowers an `ID_GROUP` node (`#(id)`) into an [`Id`].
///
/// Deliberately narrower than [`lower_paren_value`]: an id is a single scalar.
pub(crate) fn lower_hash_id(node: &SyntaxNode) -> Result<Id> {
    let closed = node.children_with_tokens().any(|e| e.kind() == K::R_PAREN);
    let mut tokens_inside = Vec::new();
    let mut seen_lparen = false;
    for elem in node.children_with_tokens() {
        if elem.kind() == K::L_PAREN {
            seen_lparen = true;
            continue;
        }
        if elem.kind() == K::R_PAREN {
            break;
        }
        if seen_lparen {
            if let SyntaxElement::Token(t) = elem {
                if !t.kind().is_trivia() && t.kind() != K::COMMENT {
                    tokens_inside.push(t);
                }
            } else if let SyntaxElement::Node(n) = elem {
                if matches!(n.kind(), K::CONTENT) {
                    let pos = usize::from(n.text_range().start());
                    return Err(err(pos, "expected an id"));
                }
                for t in n.descendants_with_tokens().filter_map(|e| e.into_token()) {
                    if !t.kind().is_trivia() && t.kind() != K::COMMENT {
                        tokens_inside.push(t);
                    }
                }
            }
        }
    }

    if tokens_inside.is_empty() {
        if node.children().any(|c| c.kind() == K::ERROR)
            && node.text().to_string().trim_end().ends_with(')')
        {
            let full = node.text().to_string();
            let trimmed = full.trim();
            if let Some(inner) = trimmed.strip_prefix("#(").and_then(|s| s.strip_suffix(')')) {
                let id = inner.trim();
                if !id.is_empty() {
                    return Ok(Id(id.to_string()));
                }
            }
        }
        let pos = if closed {
            let rparen = node
                .children_with_tokens()
                .find(|e| e.kind() == K::R_PAREN)
                .unwrap();
            usize::from(rparen.text_range().start())
        } else {
            usize::from(node.text_range().end())
        };
        return Err(err(pos, "expected an id"));
    }

    if tokens_inside.len() == 1 && tokens_inside[0].kind() == K::STRING_LITERAL {
        let t = &tokens_inside[0];
        let id = unescape_quoted_string(t.text(), usize::from(t.text_range().start()))?;
        if id.is_empty() {
            return Err(err(usize::from(t.text_range().start()), "expected an id"));
        }
        let has_error = node.children().any(|c| c.kind() == K::ERROR);
        if !closed && (!has_error || !node.text().to_string().trim_end().ends_with(')')) {
            return Err(err(usize::from(node.text_range().end()), "expected ')'"));
        }
        return Ok(Id(id));
    }

    let mut paren_depth = 0usize;
    let mut bracket_depth = 0usize;
    let mut brace_depth = 0usize;
    for t in &tokens_inside {
        match t.kind() {
            K::L_PAREN => paren_depth += 1,
            K::R_PAREN if paren_depth > 0 => paren_depth -= 1,
            K::L_BRACKET => bracket_depth += 1,
            K::R_BRACKET if bracket_depth > 0 => bracket_depth -= 1,
            K::L_BRACE => brace_depth += 1,
            K::R_BRACE if brace_depth > 0 => brace_depth -= 1,
            K::COMMA | K::R_BRACKET | K::R_BRACE | K::NEWLINE
                if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 =>
            {
                return Err(err(usize::from(t.text_range().start()), "expected an id"));
            }
            _ => {}
        }
    }

    let mut raw = String::new();
    for t in &tokens_inside {
        raw.push_str(t.text());
    }
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.starts_with("//") {
        return Err(err(
            usize::from(node.text_range().start()),
            "expected an id",
        ));
    }
    let has_error = node.children().any(|c| c.kind() == K::ERROR);
    if !closed && (!has_error || !node.text().to_string().trim_end().ends_with(')')) {
        return Err(err(usize::from(node.text_range().end()), "expected ')'"));
    }
    Ok(Id(trimmed.to_string()))
}

/// Lowers a `{...}` value group (`VALUE_DATA`) into a [`Group`].
pub(crate) fn lower_value_group(node: &SyntaxNode) -> Result<Group> {
    let closed = node.children_with_tokens().any(|e| e.kind() == K::R_BRACE);

    let mut entries = Vec::new();
    let children: Vec<SyntaxNode> = node.children().collect();
    let mut i = 0;
    while i < children.len() {
        let child = &children[i];
        match child.kind() {
            K::ERROR => return Err(lower_error(child)),
            K::MAP_ENTRY => {
                let (key, value, offset) = lower_map_entry(child, false)?;
                let Some(key) = key else {
                    return Err(err(
                        offset,
                        "a '{...}' group holds 'key: value' entries or elements; \
                         write a bare value in '(args)' instead",
                    ));
                };
                if entries
                    .iter()
                    .any(|e| matches!(e, Entry::Pair(k, _) if *k == key))
                {
                    return Err(duplicate_key_error(offset, &key));
                }
                entries.push(Entry::Pair(key, value));
                i += 1;
            }
            K::SEQ_ITEM => {
                let first_non_trivia = child.children_with_tokens().find(|e| !e.kind().is_trivia());
                match first_non_trivia {
                    Some(SyntaxElement::Node(n)) if n.kind() == K::ARGS => {
                        let offset = usize::from(child.text_range().start());
                        return Err(err(
                            offset,
                            "a '{...}' group takes no nameless entry: write a named element, '@name(...)[...]', as an entry",
                        ));
                    }
                    Some(SyntaxElement::Node(n))
                        if matches!(n.kind(), K::INLINE_FORM | K::BLOCK_FORM) =>
                    {
                        let n_end = usize::from(n.text_range().end());
                        let mut trailing = child
                            .children_with_tokens()
                            .skip_while(|e| usize::from(e.text_range().start()) < n_end);
                        if let Some(t) = trailing.find(|e| !e.kind().is_trivia()) {
                            let pos = usize::from(t.text_range().start());
                            return Err(err(pos, "expected a value"));
                        }
                        let block = is_block_placed_in_group(&n);
                        let (value, _) = lower_embedded(&n)?;
                        let entry = match value {
                            Value::Hole(mut hole) => {
                                if block {
                                    hole.placement = Placement::Block;
                                }
                                Entry::Hole(*hole)
                            }
                            Value::Element(element) => Entry::Element(if block {
                                (*element).with_placement(Placement::Block)
                            } else {
                                *element
                            }),
                            // A seat is a value, and a `{...}` group holds entries.
                            Value::TypeSeat(seat) => {
                                return Err(err(
                                    seat.span.start,
                                    "a '{...}' group holds 'key: value' entries or elements, \
                                     not a type seat; write it as the value of a key \
                                     (`key: <$type>`) or in '(args)'",
                                ));
                            }
                            _ => unreachable!("lower_embedded of an element node"),
                        };
                        entries.push(entry);
                        i += 1;
                    }
                    _ => {
                        let offset = usize::from(child.text_range().start());
                        return Err(err(
                            offset,
                            "a '{...}' group holds 'key: value' entries or elements; \
                             write a bare value in '(args)' instead",
                        ));
                    }
                }
            }
            _ => {
                i += 1;
            }
        }
    }
    if !closed {
        return Err(err(
            usize::from(node.text_range().start()),
            "unterminated '{', expected '}'",
        ));
    }
    Ok(Group(entries))
}

/// Lowers a root value node from `parse_cst_value` into a single [`Value`].
pub(crate) fn lower_value(root: &SyntaxNode) -> Result<Value> {
    let has_entries = root
        .children()
        .any(|c| matches!(c.kind(), K::MAP_ENTRY | K::SEQ_ITEM | K::ERROR));
    if !has_entries {
        return Ok(Value::Tuple(Tuple::new()));
    }

    let tuple = lower_tuple_entries(root, false, true, false)?;
    if tuple.len() == 1 && tuple.has_no_named() {
        return Ok(tuple.into_entries().next().unwrap().into_value());
    }
    Ok(Value::Tuple(tuple))
}

/// Lowers a root value node from `parse_cst_value` into a tuple [`Value`], requiring named entries only.
pub(crate) fn lower_file_body(root: &SyntaxNode) -> Result<Value> {
    let has_entries = root
        .children()
        .any(|c| matches!(c.kind(), K::MAP_ENTRY | K::SEQ_ITEM | K::ERROR));
    if !has_entries {
        return Ok(Value::Tuple(Tuple::new()));
    }

    let entries = lower_tuple_entries(root, true, true, false)?;
    Ok(Value::Tuple(entries))
}

/// Lowers child `MAP_ENTRY` and `SEQ_ITEM` nodes into a [`Tuple`].
pub(crate) fn lower_tuple_entries(
    node: &SyntaxNode,
    keyed_only: bool,
    require_separator: bool,
    raw_scalars: bool,
) -> Result<Tuple> {
    let entry_children: Vec<SyntaxNode> = node
        .children()
        .filter(|c| matches!(c.kind(), K::MAP_ENTRY | K::SEQ_ITEM | K::ERROR))
        .collect();

    // Every entry with the key it is written with and where it stands. An entry written without
    // a key has the key of its position among the entries written without one; that is decided
    // here, on the text, because a tuple does not know how its entries were spelled.
    let mut written: Vec<(String, Value, usize)> = Vec::new();
    let mut unnamed_count = 0usize;
    let mut write = |key: String, value: Value, offset: usize| -> Result<()> {
        if written.iter().any(|(held, _, _)| *held == key) {
            return Err(duplicate_key_error(offset, &key));
        }
        written.push((key, value, offset));
        Ok(())
    };
    for (i, child) in entry_children.iter().enumerate() {
        match child.kind() {
            K::ERROR => {
                if child.children_with_tokens().any(|e| e.kind() == K::COMMENT)
                    && node.text().to_string().trim_end().ends_with(')')
                {
                    continue;
                }
                return Err(lower_error(child));
            }
            K::MAP_ENTRY => {
                let (key, value, offset) = lower_map_entry(child, raw_scalars)?;
                match key {
                    None => {
                        if keyed_only {
                            return Err(err(
                                offset,
                                "expected 'key: value' entry, found bare value",
                            ));
                        }
                        unnamed_count += 1;
                        write((unnamed_count - 1).to_string(), value, offset)?;
                    }
                    Some(key) => write(key, value, offset)?,
                }
            }
            K::SEQ_ITEM => {
                let offset = usize::from(child.text_range().start());
                let val = lower_single_value_from_children(
                    child.children_with_tokens(),
                    raw_scalars,
                    offset,
                )?;
                if keyed_only {
                    return Err(err(offset, "expected 'key: value' entry, found bare value"));
                }
                unnamed_count += 1;
                write((unnamed_count - 1).to_string(), val, offset)?;
            }
            _ => {}
        }

        if require_separator && i + 1 < entry_children.len() {
            let next_child = &entry_children[i + 1];
            let mut has_sep = false;
            let mut cur = child.last_token().and_then(|t| t.next_token());
            let next_start = next_child
                .first_token()
                .map(|t| usize::from(t.text_range().start()))
                .unwrap();
            while let Some(t) = cur {
                if usize::from(t.text_range().start()) >= next_start {
                    break;
                }
                if t.kind() == K::COMMA || t.kind() == K::NEWLINE {
                    has_sep = true;
                    break;
                }
                cur = t.next_token();
            }
            if !has_sep {
                let pos = usize::from(next_child.text_range().start());
                return Err(err(pos, "expected ',' or a newline between entries"));
            }
        }
    }
    let offsets: Vec<usize> = written.iter().map(|(_, _, offset)| *offset).collect();
    Tuple::try_from_entries(
        written
            .into_iter()
            .map(|(key, value, _)| TupleEntry::named(key, value)),
    )
    .map_err(|duplicate| duplicate_key_error(offsets[duplicate.index], &duplicate.key))
}

/// Lowers one `MAP_ENTRY` node into `(key, value, offset)`.
pub(crate) fn lower_map_entry(
    node: &SyntaxNode,
    raw_scalars: bool,
) -> Result<(Option<String>, Value, usize)> {
    let start = usize::from(node.text_range().start());
    let colon = node.children_with_tokens().find(|e| e.kind() == K::COLON);
    let Some(colon) = colon else {
        let val =
            lower_single_value_from_children(node.children_with_tokens(), raw_scalars, start)?;
        return Ok((None, val, start));
    };

    let colon_start = usize::from(colon.text_range().start());
    let colon_token = colon.as_token().unwrap();

    let mut key_tokens: Vec<SyntaxToken> = Vec::new();
    for elem in node.children_with_tokens() {
        if elem.kind() == K::COLON {
            break;
        }
        if let SyntaxElement::Token(t) = elem {
            if !t.kind().is_trivia() {
                key_tokens.push(t);
            }
        } else if let SyntaxElement::Node(n) = elem {
            for t in n.descendants_with_tokens().filter_map(|e| e.into_token()) {
                if !t.kind().is_trivia() {
                    key_tokens.push(t);
                }
            }
        }
    }

    let is_scheme_uri = key_tokens.len() == 1
        && key_tokens[0].kind() != K::STRING_LITERAL
        && usize::from(key_tokens[0].text_range().end()) == colon_start
        && colon_token.next_token().is_some_and(|t| {
            usize::from(t.text_range().start()) == colon_start + 1
                && (t.text().starts_with("//")
                    || (t.text() == "/"
                        && t.next_token().is_some_and(|t2| {
                            t2.text().starts_with('/')
                                && usize::from(t2.text_range().start()) == colon_start + 2
                        })))
        });

    if is_scheme_uri {
        let text = node.text().to_string();
        let trimmed = text.trim();
        return Ok((None, Value::String(trimmed.to_string()), start));
    }

    if key_tokens.iter().any(|t| t.kind() == K::DOLLAR) {
        return Err(err(start, "an expression cannot be a key"));
    }

    let key = if key_tokens.is_empty() {
        None
    } else if key_tokens[0].kind() == K::STRING_LITERAL {
        let token = &key_tokens[0];
        let offset = usize::from(token.text_range().start());
        let unescaped = unescape_quoted_string(token.text(), offset)?;
        if unescaped.is_empty() {
            return Err(err(offset, "a quoted key must not be empty"));
        }
        Some(unescaped)
    } else {
        let mut k = String::new();
        for t in &key_tokens {
            k.push_str(t.text());
        }
        let trimmed = k.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    };

    let elements_after_colon: Vec<SyntaxElement> = node
        .children_with_tokens()
        .skip_while(|e| e.kind() != K::COLON)
        .skip(1)
        .collect();

    let fallback_offset = usize::from(colon.text_range().end());
    let value =
        lower_single_value_from_children(elements_after_colon, raw_scalars, fallback_offset)?;
    Ok((key, value, start))
}

/// Lowers a run of syntax elements representing a single value.
fn lower_single_value_from_children(
    elements: impl IntoIterator<Item = SyntaxElement>,
    raw_scalars: bool,
    fallback_offset: usize,
) -> Result<Value> {
    let elements: Vec<SyntaxElement> = elements.into_iter().collect();
    let mut non_trivia_indices: Vec<usize> = Vec::new();
    for (i, elem) in elements.iter().enumerate() {
        match elem {
            SyntaxElement::Token(t) if t.kind().is_trivia() => {}
            _ => non_trivia_indices.push(i),
        }
    }

    if non_trivia_indices.is_empty() {
        return Err(err(fallback_offset, "expected a value"));
    }

    let first = &elements[non_trivia_indices[0]];
    if matches!(first.kind(), K::COMMA | K::R_BRACKET | K::R_BRACE) {
        return Err(err(
            usize::from(first.text_range().start()),
            "expected a value",
        ));
    }

    if non_trivia_indices.len() >= 2 {
        let first = &elements[non_trivia_indices[0]];
        let second = &elements[non_trivia_indices[1]];
        if first.kind() == K::IDENT && second.kind() == K::ARGS {
            let name_token = first.as_token().unwrap();
            let args_elem = second;
            if name_token.text_range().end() == args_elem.text_range().start() {
                let name = name_token.text();
                let args_node = args_elem.as_node().unwrap();
                let raw_args = name == "string";
                let args = lower_call_args(args_node, raw_args)?;
                let start = usize::from(name_token.text_range().start());
                let call_val = resolve_call(name, args).map_err(|m| err(start, m))?;
                if non_trivia_indices.len() > 2 {
                    let next_elem = &elements[non_trivia_indices[2]];
                    let pos = usize::from(next_elem.text_range().start());
                    return Err(err(pos, "expected ',' or a newline between entries"));
                }
                return Ok(call_val);
            }
        }

        let is_standalone = match first {
            SyntaxElement::Token(t) => t.kind() == K::STRING_LITERAL,
            SyntaxElement::Node(n) => matches!(
                n.kind(),
                K::CONTENT | K::SLOT | K::INLINE_FORM | K::BLOCK_FORM | K::INTERP_EXPR | K::ARGS
            ),
        };
        if is_standalone {
            let next_elem = &elements[non_trivia_indices[1]];
            let pos = usize::from(next_elem.text_range().start());
            if matches!(next_elem.kind(), K::R_BRACE | K::R_BRACKET) {
                return Err(err(pos, "expected ')'"));
            }
            let is_expr = first.kind() == K::INTERP_EXPR
                || (first.kind() == K::INLINE_FORM
                    && first.as_node().is_some_and(is_interpolation));
            if is_expr && next_elem.kind() == K::COLON {
                return Err(err(
                    usize::from(first.text_range().start()),
                    "an expression cannot be a key",
                ));
            }
            return Err(err(pos, "expected ',' or a newline between entries"));
        }
    }

    if non_trivia_indices.len() == 1 {
        let elem = &elements[non_trivia_indices[0]];
        match elem {
            SyntaxElement::Node(n) => match n.kind() {
                K::ERROR => return Err(lower_error(n)),
                K::CONTENT => return Ok(Value::Blocks(lower_content(n)?)),
                K::SLOT => return Ok(Value::String(n.text().to_string())),
                K::INLINE_FORM | K::BLOCK_FORM => {
                    return if is_interpolation(n) {
                        Ok(Value::Interp(crate::interp::lower_interpolation(n)?))
                    } else if let Some(slot) = slot_of(n) {
                        lower_slot(&slot).map(LoweredSlot::into_value)
                    } else {
                        Ok(Value::Element(Box::new(lower_element(n)?)))
                    };
                }
                K::INTERP_EXPR => return Ok(Value::Interp(crate::interp::lower_interpolation(n)?)),
                // A bare `(a, b)` or `(a, k: v)` is a tuple: a value has no infix
                // arithmetic, so no parenthesis here can group anything.
                K::ARGS => {
                    return Ok(Value::Tuple(lower_paren_value(n)?));
                }
                _ => {}
            },
            SyntaxElement::Token(t) if t.kind() == K::STRING_LITERAL => {
                let start = usize::from(t.text_range().start());
                let s = unescape_quoted_string(t.text(), start)?;
                return Ok(Value::String(s));
            }
            _ => {}
        }
    }

    let first_idx = *non_trivia_indices.first().unwrap();
    let start = usize::from(elements[first_idx].text_range().start());
    let mut raw = String::new();
    let mut stopped_at = None;
    let mut paren_depth = 0usize;
    let mut bracket_depth = 0usize;
    let mut brace_depth = 0usize;
    for elem in &elements[first_idx..] {
        if elem.kind() == K::COMMENT {
            break;
        }
        if !elem.kind().is_trivia() {
            match elem.kind() {
                K::L_PAREN => paren_depth += 1,
                K::R_PAREN if paren_depth > 0 => paren_depth -= 1,
                K::L_BRACKET => bracket_depth += 1,
                K::R_BRACKET if bracket_depth > 0 => bracket_depth -= 1,
                K::L_BRACE => brace_depth += 1,
                K::R_BRACE if brace_depth > 0 => brace_depth -= 1,
                K::R_BRACKET | K::R_BRACE | K::R_PAREN | K::COMMA
                    if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 =>
                {
                    stopped_at = Some(elem);
                    break;
                }
                _ => {}
            }
        }
        raw.push_str(&elem.to_string());
    }
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(err(start, "expected a value"));
    }
    if let Some(stopped) = stopped_at {
        let pos = usize::from(stopped.text_range().start());
        if matches!(stopped.kind(), K::R_BRACKET | K::R_BRACE) {
            return Err(err(pos, "expected ')'"));
        }
        return Err(err(pos, "expected ',' or a newline between entries"));
    }
    if raw_scalars {
        Ok(Value::String(trimmed.to_string()))
    } else {
        scalar_from_text(trimmed).map_err(|m| err(start, m))
    }
}

/// Lowers call arguments inside an `ARGS` node `(...)`.
fn lower_call_args(args_node: &SyntaxNode, raw_args: bool) -> Result<Tuple> {
    let closed = args_node
        .children_with_tokens()
        .any(|e| e.kind() == K::R_PAREN);
    if !closed {
        return Err(err(
            usize::from(args_node.text_range().end()),
            "expected ')'",
        ));
    }

    lower_tuple_entries(args_node, false, true, raw_args)
}

/// Unescapes a `"..."` string literal.
pub(crate) fn unescape_quoted_string(text: &str, offset: usize) -> Result<String> {
    let mut chars = text.chars();
    if chars.next() != Some('"') {
        return Err(err(offset, "expected '\"'"));
    }
    let mut out = String::new();
    let mut closed = false;
    while let Some(c) = chars.next() {
        if c == '"' {
            closed = true;
            break;
        }
        if c == '\\' {
            match chars.next() {
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some(other) => out.push(other),
                None => return Err(err(offset, "unterminated string")),
            }
        } else {
            out.push(c);
        }
    }
    if !closed {
        return Err(err(offset, "unterminated string"));
    }
    Ok(out)
}

/// Whether `node` is an interpolation: `{$ expr }` and `$(ref)` are an `INTERP_EXPR`, and
/// `$name(args)` is a sigil that opens with `$`.
pub(crate) fn is_interpolation(node: &SyntaxNode) -> bool {
    node.children().any(|n| n.kind() == K::INTERP_EXPR)
        || node
            .children()
            .find(|n| n.kind() == K::SIGIL)
            .and_then(|n| n.first_token())
            .is_some_and(|t| t.kind() == K::DOLLAR)
}

/// Whether an element inside a `{...}` group stands as a block on its own line.
fn is_block_placed_in_group(node: &SyntaxNode) -> bool {
    let mut at_start = false;
    let mut cur = node.first_token().and_then(|t| t.prev_token());
    loop {
        match cur {
            Some(t) if t.kind() == K::WHITESPACE => cur = t.prev_token(),
            Some(t) if t.kind() == K::NEWLINE => {
                at_start = true;
                break;
            }
            None => {
                at_start = true;
                break;
            }
            _ => break,
        }
    }
    if !at_start {
        return false;
    }
    let mut ends_line = false;
    let mut cur = node.last_token().and_then(|t| t.next_token());
    loop {
        match cur {
            Some(t) if t.kind() == K::WHITESPACE || t.kind() == K::COMMENT => cur = t.next_token(),
            Some(t) if t.kind() == K::NEWLINE => {
                ends_line = true;
                break;
            }
            None => {
                ends_line = true;
                break;
            }
            _ => break,
        }
    }
    ends_line
}

/// What a bare scalar's text is, by the JSON number grammar.
enum NumberShape {
    Int,
    Float,
}

fn classify_number(s: &str) -> Option<NumberShape> {
    let b = s.as_bytes();
    let digits = |mut i: usize| {
        let start = i;
        while b.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        (i > start).then_some(i)
    };
    let mut i = usize::from(b.first() == Some(&b'-'));
    match b.get(i)? {
        b'0' => i += 1,
        b'1'..=b'9' => i = digits(i)?,
        _ => return None,
    }
    let mut float = false;
    if b.get(i) == Some(&b'.') {
        i = digits(i + 1)?;
        float = true;
    }
    if matches!(b.get(i), Some(b'e' | b'E')) {
        i += 1;
        if matches!(b.get(i), Some(b'+' | b'-')) {
            i += 1;
        }
        i = digits(i)?;
        float = true;
    }
    (i == b.len()).then_some(if float {
        NumberShape::Float
    } else {
        NumberShape::Int
    })
}

pub fn scalar_from_text(s: &str) -> std::result::Result<Value, String> {
    match s {
        "null" => return Ok(Value::Null),
        "true" => return Ok(Value::Bool(true)),
        "false" => return Ok(Value::Bool(false)),
        _ => {}
    }
    match classify_number(s) {
        Some(NumberShape::Int) => Ok(match s.parse::<i64>() {
            Ok(i) if i.to_string() == s => Value::Int(i),
            _ => Value::Number(s.to_string()),
        }),
        Some(NumberShape::Float) => Ok(match s.parse::<f64>() {
            Ok(f) if f.is_finite() && Value::float_text(f) == s => Value::Float(f),
            _ => Value::Number(s.to_string()),
        }),
        None => Ok(Value::String(s.to_string())),
    }
}

pub fn resolve_call(name: &str, args: Tuple) -> std::result::Result<Value, String> {
    match name {
        "string" => construct_string(&args),
        "int" => construct_int(&args),
        "float" => construct_float(&args),
        _ => Ok(Value::Call(name.to_string(), args)),
    }
}

fn at_most_one<'a>(name: &str, args: &'a Tuple) -> std::result::Result<Option<&'a Value>, String> {
    if !args.has_no_named() {
        return Err(format!("{name}() takes no named fields"));
    }
    if args.len() > 1 {
        return Err(format!("{name}() takes at most one argument"));
    }
    Ok(args.nth_unnamed(0))
}

fn construct_string(args: &Tuple) -> std::result::Result<Value, String> {
    Ok(Value::String(match at_most_one("string", args)? {
        None => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Int(i)) => i.to_string(),
        Some(Value::Bool(b)) => b.to_string(),
        Some(Value::Null) => "null".to_string(),
        Some(Value::Number(text)) => text.clone(),
        Some(Value::Float(f)) if f.is_finite() => Value::float_text(*f),
        Some(Value::Float(f)) if f.is_nan() => "nan".to_string(),
        Some(Value::Float(f)) => (if *f > 0.0 { "inf" } else { "-inf" }).to_string(),
        Some(other) => return Err(format!("string() takes a scalar, got {other:?}")),
    }))
}

fn construct_int(args: &Tuple) -> std::result::Result<Value, String> {
    match at_most_one("int", args)? {
        None => Ok(Value::Int(0)),
        Some(Value::Int(i)) => Ok(Value::Int(*i)),
        Some(Value::Float(f)) => {
            let f = *f;
            if f.is_finite() && f >= i64::MIN as f64 && f < i64::MAX as f64 {
                Ok(Value::Int(f.trunc() as i64))
            } else {
                Err(format!("float `{f}` is out of range for a 64-bit integer"))
            }
        }
        Some(Value::Number(text)) => match text.parse::<f64>() {
            Ok(f) if f.is_finite() && f >= i64::MIN as f64 && f < i64::MAX as f64 => {
                Ok(Value::Int(f.trunc() as i64))
            }
            _ => Err(format!(
                "number `{text}` is out of range for a 64-bit integer"
            )),
        },
        Some(Value::String(s)) => s
            .trim()
            .parse::<i64>()
            .map(Value::Int)
            .map_err(|_| format!("int() cannot convert `{}` to an integer", s.trim())),
        Some(other) => Err(format!("int() takes a number or a string, got {other:?}")),
    }
}

fn construct_float(args: &Tuple) -> std::result::Result<Value, String> {
    match at_most_one("float", args)? {
        None => Ok(Value::Float(0.0)),
        Some(Value::Int(i)) => Ok(Value::Float(*i as f64)),
        Some(Value::Float(f)) => Ok(Value::Float(*f)),
        Some(Value::Number(text)) => match text.parse::<f64>() {
            Ok(f) if f.is_finite() => Ok(Value::Float(f)),
            _ => Err(format!("float `{text}` is out of range")),
        },
        Some(Value::String(s)) => {
            let text = s.trim();
            let spells_non_finite = text
                .trim_start_matches(['+', '-'])
                .starts_with(|c: char| c.is_ascii_alphabetic());
            match text.parse::<f64>() {
                Ok(f) if f.is_finite() || spells_non_finite => Ok(Value::Float(f)),
                Ok(_) => Err(format!("float `{text}` is out of range")),
                Err(_) => Err(format!("float() cannot convert `{text}` to a float")),
            }
        }
        Some(other) => Err(format!("float() takes a number or a string, got {other:?}")),
    }
}
