//! Shared AST helpers and formatting utilities for specification generators.

use tomet_ast::{
    Block, Element, Inline, List, ListItem, Paragraph, Placement, RawText, Sigil, Span, Text, Value,
};
use tomet_tree::{ElementExt, element_new};

/// Returns a dummy source span for synthesized AST nodes.
pub fn dummy() -> Span {
    Span::dummy()
}

/// Builds a plain text inline node.
pub fn text(s: &str) -> Inline {
    Inline::Text(Text::new(s, dummy()))
}

/// Builds an inline `@raw` code span node.
pub fn code(s: &str) -> Inline {
    let mut el = element_new(Sigil::named("raw")).with_placement(Placement::Inline);
    el.content = Some(vec![paragraph(vec![Inline::Raw(RawText::new(s, dummy()))])]);
    Inline::Element(el)
}

/// Wraps inline nodes into a paragraph block.
pub fn paragraph(content: Vec<Inline>) -> Block {
    Block::Paragraph(Paragraph::new(content, dummy()))
}

/// Builds a block-level `@raw(lang: "text")` code block element.
pub fn code_block(body: &str) -> Element {
    let mut el = element_new(Sigil::named("raw")).with_placement(Placement::Block);
    el.args = Some(Value::Map(vec![(
        "lang".to_string(),
        Value::String("text".to_string()),
    )]));
    el.content = Some(vec![paragraph(vec![Inline::Raw(RawText::new(
        body,
        dummy(),
    ))])]);
    el
}

/// Builds an unordered list from an iterator of list item elements.
pub fn list(items: impl Iterator<Item = Element>) -> List {
    tomet_tree::list(
        false,
        items
            .map(|element| ListItem {
                element,
                sublist: None,
            })
            .collect(),
        dummy(),
    )
}

/// Parses text with `` `code` `` spans turned into inline `@raw` code spans,
/// and escapes element-like trigger words (`@`, `$`, `^`).
pub fn inlines(s: &str) -> Vec<Inline> {
    if !s.matches('`').count().is_multiple_of(2) {
        return vec![text(s)];
    }
    let mut out = Vec::new();
    for (i, part) in s.split('`').enumerate() {
        if part.is_empty() {
            continue;
        }
        if i % 2 == 1 {
            out.push(code(part));
        } else {
            out.extend(guard_triggers(part));
        }
    }
    out
}

/// Splits plain text so every element-looking word (`@foo`, `$bar`) is wrapped in a code span.
pub fn guard_triggers(s: &str) -> Vec<Inline> {
    let mut out = Vec::new();
    let mut plain = String::new();
    let mut rest = s;
    while let Some((at, c)) = rest
        .char_indices()
        .find(|&(i, c)| matches!(c, '@' | '$') && starts_element(&rest[i + c.len_utf8()..]))
    {
        plain.push_str(&rest[..at]);
        let word_end = rest[at..]
            .find(char::is_whitespace)
            .map_or(rest.len(), |n| at + n);
        let word = rest[at..word_end].trim_end_matches(['.', ',', ';', ':', '!', '?']);
        if !plain.is_empty() {
            out.push(text(&std::mem::take(&mut plain)));
        }
        out.push(code(word));
        rest = &rest[at + word.len()..];
        let _ = c;
    }
    plain.push_str(rest);
    if !plain.is_empty() {
        out.push(text(&plain));
    }
    out
}

fn starts_element(after: &str) -> bool {
    after
        .chars()
        .next()
        .is_some_and(|c| c.is_alphabetic() || matches!(c, '_' | '(' | '{'))
}

/// Splits prose on blank lines (`\n\n`) and normalizes internal whitespace per paragraph.
pub fn paragraphs(s: &str) -> Vec<String> {
    s.split("\n\n")
        .map(|p| p.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|p| !p.is_empty())
        .collect()
}
