//! Extracts tags from an element (`@tag(...)`, `@tag[...]`).

use tomet_ast::{Block, Element, Inline, Value};

/// Extracts a list of tag names from a tag element.
/// Inspects `args` (as string, seq, or map) and `content` (as plain text).
pub fn extract_tags(el: &Element) -> Vec<String> {
    let mut tags = Vec::new();
    if let Some(args) = &el.args {
        extract_tags_from_value(args, &mut tags);
    }
    if tags.is_empty()
        && let Some(content) = &el.content
    {
        let s = blocks_to_plain(content);
        if !s.trim().is_empty() {
            tags.push(s.trim().to_string());
        }
    }
    tags
}

fn extract_tags_from_value(v: &Value, tags: &mut Vec<String>) {
    match v {
        Value::String(s) => {
            if !s.is_empty() {
                tags.push(s.clone());
            }
        }
        Value::Map(entries) => {
            for (_, val) in entries {
                extract_tags_from_value(val, tags);
            }
        }
        Value::Seq(items) => {
            for item in items {
                extract_tags_from_value(item, tags);
            }
        }
        _ => {}
    }
}

fn inlines_to_plain(inlines: &[Inline]) -> String {
    let mut s = String::new();
    for inline in inlines {
        match inline {
            Inline::Text(t) => s.push_str(&t.value),
            Inline::Raw(t) => s.push_str(&t.value),
            Inline::SoftBreak(_) => s.push(' '),
            Inline::LineBreak(_) => s.push(' '),
            Inline::Element(el) => {
                if let Some(content) = &el.content {
                    s.push_str(&blocks_to_plain(content));
                }
            }
        }
    }
    s
}

/// [`inlines_to_plain`] over `Element.content`'s `Vec<Block>` shape --
/// `@tag[...]`'s content is inline-only in practice (a single `Paragraph`),
/// but this walks whatever shape actually parsed rather than assuming it.
fn blocks_to_plain(blocks: &[Block]) -> String {
    let mut s = String::new();
    for block in blocks {
        match block {
            Block::Paragraph(p) => s.push_str(&inlines_to_plain(&p.content)),
            Block::Element(el) => {
                if let Some(content) = &el.content {
                    s.push_str(&blocks_to_plain(content));
                }
            }
            Block::Section(sec) => {
                s.push_str(&inlines_to_plain(&sec.title));
                s.push_str(&blocks_to_plain(&sec.blocks));
            }
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use tomet_ast::{Paragraph, Sigil, Span, Text};
    use tomet_tree::element_new;

    #[test]
    fn extracts_tags_from_args_string() {
        let mut el = element_new(Sigil::named("tag"));
        el.args = Some(Value::String("rust".to_string()));
        assert_eq!(extract_tags(&el), vec!["rust"]);
    }

    #[test]
    fn extracts_tags_from_args_seq() {
        let mut el = element_new(Sigil::named("tag"));
        el.args = Some(Value::Seq(vec![
            Value::String("rust".to_string()),
            Value::String("tomet".to_string()),
        ]));
        assert_eq!(extract_tags(&el), vec!["rust", "tomet"]);
    }

    #[test]
    fn extracts_tags_from_content() {
        let mut el = element_new(Sigil::named("tag"));
        el.content = Some(vec![Block::Paragraph(Paragraph::new(
            vec![Inline::Text(Text::new("docs", Span::dummy()))],
            Span::dummy(),
        ))]);
        assert_eq!(extract_tags(&el), vec!["docs"]);
    }
}
