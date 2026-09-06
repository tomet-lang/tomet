//! Post-import pass over inline text: detects `[[wiki]]`/`![[embed]]`
//! links and bare `http(s)://`/`mailto:` URLs (pulldown-cmark doesn't
//! parse either as its own event), and escapes any Tomet sigil
//! characters (`@`/`$`/`^`/`//`/`/* */`) left in plain text so they
//! round-trip as literal text rather than being misread as `.tmt`
//! syntax on export.

use tomet_ast::{Block, Document, Element, ElementValue, Inline, Sigil, Span, Text, Value};
use tomet_semantics::list_ordered;
use tomet_tree::element_new;

pub(super) fn post_process_document_wikilinks(doc: &mut Document) {
    for block in &mut doc.blocks {
        post_process_block_wikilinks(block);
    }
}

fn post_process_block_wikilinks(block: &mut Block) {
    match block {
        Block::Paragraph(p) => {
            p.content = post_process_inlines_wikilinks(std::mem::take(&mut p.content));
        }
        Block::Element(el) if list_ordered(el).is_some() => {
            if let Some(ElementValue::Group(entries)) = &mut el.value {
                for item in entries.iter_mut().filter_map(|e| match e {
                    tomet_ast::Entry::Element(el) => Some(el),
                    tomet_ast::Entry::Pair(..) => None,
                }) {
                    if let Some(content) = item.content.take() {
                        item.content = Some(post_process_inlines_wikilinks(content));
                    }
                    if let Some(children) = &mut item.children {
                        for child in children {
                            post_process_block_wikilinks(child);
                        }
                    }
                }
            }
        }
        Block::Element(el) => {
            post_process_element_wikilinks(el);
        }
    }
}

fn post_process_element_wikilinks(el: &mut Element) {
    if el.sigil.is_bare_named("codeblock") {
        return;
    }
    if let Some(content) = el.content.take() {
        el.content = Some(post_process_inlines_wikilinks(content));
    }
}

fn post_process_inlines_wikilinks(inlines: Vec<Inline>) -> Vec<Inline> {
    let mut new_inlines = Vec::new();
    for inline in inlines {
        match inline {
            Inline::Text(t) => {
                new_inlines.extend(parse_urls_and_wikilinks(&t.value));
            }
            Inline::Element(mut el) => {
                post_process_element_wikilinks(&mut el);
                new_inlines.push(Inline::Element(el));
            }
        }
    }
    new_inlines
}

fn find_next_url(text: &str) -> Option<(usize, usize)> {
    let mut search_from = 0;
    while search_from < text.len() {
        let rest = &text[search_from..];
        let rel_http = rest.find("http://");
        let rel_https = rest.find("https://");
        let rel_mailto = rest.find("mailto:");

        let first_rel = match (rel_http, rel_https, rel_mailto) {
            (None, None, None) => return None,
            (a, b, c) => [a, b, c].into_iter().flatten().min().unwrap(),
        };

        let idx = search_from + first_rel;

        if idx > 0 {
            let prev_char = text[..idx].chars().next_back().unwrap();
            if prev_char.is_alphanumeric() {
                search_from = idx + 1;
                continue;
            }
        }

        let url_sub = &text[idx..];
        let end_rel = url_sub
            .find(|c: char| c.is_whitespace() || c < ' ')
            .unwrap_or(url_sub.len());

        let url_end = idx + end_rel;
        return Some((idx, url_end));
    }
    None
}

fn parse_urls_and_wikilinks(text: &str) -> Vec<Inline> {
    let mut result = Vec::new();
    let mut remaining = text;

    while !remaining.is_empty() {
        let url_match = find_next_url(remaining);
        let wiki_start = remaining.find("[[");

        match (url_match, wiki_start) {
            (Some((u_start, u_end)), None) => {
                if u_start > 0 {
                    let seg = enclose_sigils_in_backticks(&remaining[..u_start]);
                    if !seg.is_empty() {
                        result.push(Inline::Text(Text::new(seg, Span::dummy())));
                    }
                }
                let url_str = &remaining[u_start..u_end];
                let mut el = element_new(Sigil::named("link"));
                el.args = Some(Value::Map(vec![(
                    "target".to_string(),
                    Value::String(url_str.to_string()),
                )]));
                result.push(Inline::Element(el));
                remaining = &remaining[u_end..];
            }
            (None, Some(start_idx)) => {
                parse_one_wikilink(&mut result, &mut remaining, start_idx);
            }
            (Some((u_start, u_end)), Some(start_idx)) => {
                if u_start < start_idx {
                    if u_start > 0 {
                        let seg = enclose_sigils_in_backticks(&remaining[..u_start]);
                        if !seg.is_empty() {
                            result.push(Inline::Text(Text::new(seg, Span::dummy())));
                        }
                    }
                    let url_str = &remaining[u_start..u_end];
                    let mut el = element_new(Sigil::named("link"));
                    el.args = Some(Value::Map(vec![(
                        "target".to_string(),
                        Value::String(url_str.to_string()),
                    )]));
                    result.push(Inline::Element(el));
                    remaining = &remaining[u_end..];
                } else {
                    parse_one_wikilink(&mut result, &mut remaining, start_idx);
                }
            }
            (None, None) => {
                let seg = enclose_sigils_in_backticks(remaining);
                if !seg.is_empty() {
                    result.push(Inline::Text(Text::new(seg, Span::dummy())));
                }
                break;
            }
        }
    }

    result
}

fn parse_one_wikilink(result: &mut Vec<Inline>, remaining: &mut &str, start_idx: usize) {
    if let Some(end_idx) = remaining[start_idx + 2..].find("]]") {
        let actual_end_idx = start_idx + 2 + end_idx;

        let is_embed = start_idx > 0 && remaining.as_bytes()[start_idx - 1] == b'!';
        let text_end_idx = if is_embed { start_idx - 1 } else { start_idx };

        if text_end_idx > 0 {
            let segment = enclose_sigils_in_backticks(&remaining[..text_end_idx]);
            if !segment.is_empty() {
                result.push(Inline::Text(Text::new(segment, Span::dummy())));
            }
        }

        // `![[embed]]` (image-style) keeps the plain name as `target` --
        // `<embed>` never strips a scheme prefix at render time, it just
        // uses `target` as-is for `src`/etc. `[[wiki]]` (regular) is a real
        // `@link`, so it needs the `ref:` scheme prefix embedded in the
        // string for `target_scheme` (downstream) to recognize it as a
        // wikilink-style, search-by-name reference rather than a plain
        // relative file path.
        let sigil = if is_embed {
            Sigil::named("embed")
        } else {
            Sigil::named("link")
        };
        let target_key_value = |raw: &str| -> String {
            if is_embed {
                raw.to_string()
            } else {
                format!("ref:{raw}")
            }
        };

        let inner = &remaining[start_idx + 2..actual_end_idx];
        let unescaped_inner = inner.replace(r"\|", "|");
        let wikilink_el = if let Some((target, display)) = unescaped_inner.split_once('|') {
            let target = target.trim();
            let display = display.trim();
            let mut el = element_new(sigil);
            el.args = Some(Value::Map(vec![(
                "target".to_string(),
                Value::String(target_key_value(target)),
            )]));
            el.content = Some(vec![Inline::Text(Text::new(
                enclose_sigils_in_backticks(display),
                Span::dummy(),
            ))]);
            el
        } else {
            let target = unescaped_inner.trim();
            let mut el = element_new(sigil);
            el.args = Some(Value::Map(vec![(
                "target".to_string(),
                Value::String(target_key_value(target)),
            )]));
            el
        };

        result.push(Inline::Element(wikilink_el));
        *remaining = &remaining[actual_end_idx + 2..];
    } else {
        let segment = enclose_sigils_in_backticks(&remaining[..start_idx + 2]);
        if !segment.is_empty() {
            result.push(Inline::Text(Text::new(segment, Span::dummy())));
        }
        *remaining = &remaining[start_idx + 2..];
    }
}

fn enclose_sigils_in_backticks(text: &str) -> String {
    if !text.contains('@')
        && !text.contains('$')
        && !text.contains('^')
        && !text.contains('/')
        && !text.contains('*')
        && !text.contains('_')
    {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len() + 8);
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '@' => out.push_str("`@`"),
            '$' => out.push_str("`$`"),
            '^' => out.push_str("`^`"),
            '/' => {
                if chars.peek() == Some(&'/') && !out.ends_with(':') {
                    let mut slashes = String::from("/");
                    while chars.peek() == Some(&'/') {
                        slashes.push(chars.next().unwrap());
                    }
                    out.push('`');
                    out.push_str(&slashes);
                    out.push('`');
                } else if chars.peek() == Some(&'*') {
                    chars.next();
                    out.push_str("`/*`");
                } else {
                    out.push('/');
                }
            }
            '*' => {
                if chars.peek() == Some(&'/') {
                    chars.next();
                    out.push_str("`*/`");
                } else {
                    out.push('*');
                }
            }
            '_' => {
                let prev = out.chars().next_back();
                let next = chars.peek().copied();
                if prev.map_or(true, |c| !c.is_alphanumeric())
                    && next.map_or(false, |c| c.is_alphanumeric())
                {
                    out.push_str("`_`");
                } else {
                    out.push('_');
                }
            }
            c => out.push(c),
        }
    }
    out
}
