//! How the inline items of a prose run, and the words of a text, are merged.

use crate::diff3;
use crate::markers::format_inline_conflict_span;
use crate::trivia::merge_ws;
use tomet_edit::{InlineItem, parse_inline_items};

fn split_trailing_newline(items: &[InlineItem]) -> (&[InlineItem], &str) {
    if let Some(InlineItem::Trivia(nl)) = items.last()
        && (nl == "\n" || nl == "\r\n")
    {
        (&items[..items.len() - 1], nl.as_str())
    } else {
        (items, "")
    }
}

pub(crate) fn merge_inline_items(
    b_inlines: &[InlineItem],
    l_inlines: &[InlineItem],
    r_inlines: &[InlineItem],
) -> (String, Vec<InlineItem>, bool) {
    let (b_body, b_nl) = split_trailing_newline(b_inlines);
    let (l_body, l_nl) = split_trailing_newline(l_inlines);
    let (r_body, r_nl) = split_trailing_newline(r_inlines);
    let merged_nl = merge_ws(b_nl, l_nl, r_nl);

    let mut out_str = String::new();
    let mut clean = true;

    for region in diff3::merge3(b_body, l_body, r_body) {
        match region {
            diff3::Region::Same(items) => {
                for item in items {
                    out_str.push_str(item.raw_str());
                }
            }
            diff3::Region::Conflict { base, mine, theirs } => {
                if let ([InlineItem::Text(bt)], [InlineItem::Text(lt)], [InlineItem::Text(rt)]) =
                    (base.as_slice(), mine.as_slice(), theirs.as_slice())
                {
                    let (merged_t, sub_clean) = merge_text_tokens(bt, lt, rt);
                    out_str.push_str(&merged_t);
                    clean &= sub_clean;
                    continue;
                }

                clean = false;
                let mine_raw: String = mine.iter().map(InlineItem::raw_str).collect();
                let theirs_raw: String = theirs.iter().map(InlineItem::raw_str).collect();
                out_str.push_str(&format_inline_conflict_span(&mine_raw, &theirs_raw));
            }
        }
    }

    out_str.push_str(&merged_nl);
    let parsed_inlines = parse_inline_items(&out_str);
    (out_str, parsed_inlines, clean)
}

fn tokenize_prose(s: &str) -> Vec<&str> {
    #[derive(Clone, Copy, PartialEq)]
    enum Class {
        Ws,
        Word,
        Punct,
    }

    let classify = |c: char| {
        if c.is_whitespace() {
            Class::Ws
        } else if c.is_alphanumeric() || c == '_' || c == '-' || c == '\'' {
            Class::Word
        } else {
            Class::Punct
        }
    };

    let mut tokens = Vec::new();
    let mut start = 0;
    let mut prev_class: Option<Class> = None;
    for (idx, ch) in s.char_indices() {
        let cls = classify(ch);
        if let Some(prev) = prev_class
            && (prev != cls || cls == Class::Punct)
        {
            tokens.push(&s[start..idx]);
            start = idx;
        }
        prev_class = Some(cls);
    }
    if start < s.len() {
        tokens.push(&s[start..]);
    }
    tokens
}

fn merge_text_tokens(base: &str, local: &str, remote: &str) -> (String, bool) {
    let b_toks = tokenize_prose(base);
    let l_toks = tokenize_prose(local);
    let r_toks = tokenize_prose(remote);

    let mut out = String::new();
    let mut clean = true;

    for region in diff3::merge3(&b_toks, &l_toks, &r_toks) {
        match region {
            diff3::Region::Same(toks) => {
                for t in toks {
                    out.push_str(t);
                }
            }
            diff3::Region::Conflict { mine, theirs, .. } => {
                clean = false;
                out.push_str(&format_inline_conflict_span(
                    &mine.concat(),
                    &theirs.concat(),
                ));
            }
        }
    }

    (out, clean)
}
