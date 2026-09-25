use super::*;

/// The `Raw` body of `doc`'s first block, or a panic.
fn raw_body(doc: &tomet_ast::Document) -> &str {
    match &doc.blocks[0] {
        Block::Element(el) => match &el.value {
            Some(ElementValue::Raw(body)) => body,
            other => panic!("expected a raw fence body, got {other:?}"),
        },
        other => panic!("expected an element, got {other:?}"),
    }
}

#[test]
fn a_fence_preserves_brackets_and_newlines_losslessly() {
    let doc =
        parse_document("@memo+++\nline one\nline two with * and [brackets] inside\n+++\n").unwrap();
    assert_eq!(
        raw_body(&doc),
        "line one\nline two with * and [brackets] inside"
    );
}

#[test]
fn a_fence_is_not_confused_by_an_apostrophe() {
    // The old `(content:raw)[...]` matched brackets and had to stay
    // quote-agnostic, because free-form prose gives no guarantee its
    // `'`/`"` occurrences are balanced. A fence has no such problem:
    // it ends at a line, so nothing inside it can be miscounted.
    let doc = parse_document("@memo+++\ndon't forget [this]\n+++\n").unwrap();
    assert_eq!(raw_body(&doc), "don't forget [this]");
}

#[test]
fn a_fence_body_is_not_confused_by_an_unquoted_brace() {
    // The bug the fence removes by construction: the old
    // `(format:yaml){...}` scanner tracked brace depth, so a `}`
    // inside otherwise legal YAML ended the body early.
    let doc = parse_document("@meta(format:yaml)+++\na: \"}\"\nb: 1\n+++\n").unwrap();
    assert_eq!(raw_body(&doc), "a: \"}\"\nb: 1");
}

#[test]
fn a_longer_fence_run_escapes_a_body_containing_a_fence() {
    let doc = parse_document("@memo++++\n+++\nstill inside\n++++\n").unwrap();
    assert_eq!(raw_body(&doc), "+++\nstill inside");
}

#[test]
fn an_unterminated_fence_runs_to_eof() {
    // Matching the backtick fence, and unlike `[...]`/`{...}` groups,
    // which error when unclosed.
    let doc = parse_document("@memo+++\nno closing line\n").unwrap();
    assert_eq!(raw_body(&doc), "no closing line");
}

#[test]
fn interpolation_does_not_expand_inside_a_fence() {
    // `default.config.tmt` stores macro templates such as
    // `"https://github.com/.../${1}"` that must reach
    // `tomet-transform`'s `MacroPattern::from_template` verbatim.
    let doc = parse_document("@config(format:json)+++\n{\"gh\": \"x/${1}\"}\n+++\n").unwrap();
    assert_eq!(raw_body(&doc), "{\"gh\": \"x/${1}\"}");
}

#[test]
fn content_raw_is_now_just_an_ordinary_argument() {
    // `(content:raw)` no longer changes how `[...]` is lexed -- that
    // was one of the four places the parser consulted an element's
    // own arguments. Line breaks collapse per the usual rule.
    let doc = parse_document("@memo(content:raw)[\nline one\nline two\n]\n").unwrap();
    match &doc.blocks[0] {
        Block::Element(el) => {
            assert_eq!(
                el.content,
                Some(vec![
                    Inline::Text("line one".into()),
                    sb(),
                    Inline::Text("line two".into()),
                ])
            );
        }
        other => panic!("expected an element, got {other:?}"),
    }
}

#[test]
fn parses_a_fenced_code_block_with_lang() {
    let doc = parse_document("```rust\nfn main() {}\n```\n").unwrap();
    match &doc.blocks[0] {
        Block::Element(el) => {
            assert_eq!(el.sigil, Sigil::named("raw"));
            assert_eq!(
                el.args,
                Some(Value::Map(vec![(
                    "lang".to_string(),
                    Value::String("rust".to_string())
                )]))
            );
            assert_eq!(el.content, Some(vec![Inline::Raw("fn main() {}".into())]));
        }
        other => panic!("expected a raw element, got {other:?}"),
    }
}

#[test]
fn parses_a_fenced_code_block_without_lang() {
    let doc = parse_document("```\nplain\n```\n").unwrap();
    match &doc.blocks[0] {
        Block::Element(el) => {
            assert_eq!(el.sigil, Sigil::named("raw"));
            assert_eq!(el.args, None);
            assert_eq!(el.content, Some(vec![Inline::Raw("plain".into())]));
        }
        other => panic!("expected a raw element, got {other:?}"),
    }
}

#[test]
fn fenced_code_block_and_bracket_codeblock_produce_the_same_ast() {
    // No longer *identical* content: the fenced spelling is always
    // captured verbatim (`Inline::Raw`, can hold real newlines), while
    // `@raw(...)[...]` is ordinary `[content]` -- the parser
    // does not special-case lexing by element name (see
    // `content_raw_is_now_just_an_ordinary_argument`), so it still goes
    // through the usual inline grammar and comes back as `Inline::Text`.
    // The two happened to produce byte-identical `Inline::Text` before
    // `Inline::Raw` existed only because this example has no markup
    // characters and fits on one line; a multi-line bracket body would
    // already have shown the gap (its line breaks fold/`SoftBreak`
    // like any other prose, not staying verbatim). What still matches
    // is sigil/args/value and the rendered text.
    let fenced = parse_document("```rust\nfn main() {}\n```\n").unwrap();
    let bracket = parse_document("@raw(lang:rust)[fn main() {}]\n").unwrap();
    match (&fenced.blocks[0], &bracket.blocks[0]) {
        (Block::Element(a), Block::Element(b)) => {
            assert_eq!(a.sigil, b.sigil);
            assert_eq!(a.args, b.args);
            assert_eq!(a.value, b.value);
            assert_eq!(a.content, Some(vec![Inline::Raw("fn main() {}".into())]));
            assert_eq!(b.content, Some(vec![Inline::Text("fn main() {}".into())]));
        }
        other => panic!("expected two raw elements, got {other:?}"),
    }
}

#[test]
fn fenced_code_block_closing_fence_needs_at_least_the_opening_backtick_count() {
    // A closing fence with fewer backticks than the opening one doesn't
    // close it -- it's just consumed as ordinary body content, same as
    // CommonMark.
    let doc = parse_document("````\ncode\n```\nmore code\n````\n").unwrap();
    match &doc.blocks[0] {
        Block::Element(el) => {
            assert_eq!(
                el.content,
                Some(vec![Inline::Raw("code\n```\nmore code".into())])
            );
        }
        other => panic!("expected a raw element, got {other:?}"),
    }
}

#[test]
fn fenced_code_block_closing_fence_can_have_more_backticks_than_opening() {
    let doc = parse_document("```\ncode\n`````\n").unwrap();
    match &doc.blocks[0] {
        Block::Element(el) => {
            assert_eq!(el.content, Some(vec![Inline::Raw("code".into())]));
        }
        other => panic!("expected a raw element, got {other:?}"),
    }
}

#[test]
fn fenced_code_block_body_can_contain_short_backtick_runs() {
    let doc = parse_document("```\nsee `foo` and ``bar``\n```\n").unwrap();
    match &doc.blocks[0] {
        Block::Element(el) => {
            assert_eq!(
                el.content,
                Some(vec![Inline::Raw("see `foo` and ``bar``".into())])
            );
        }
        other => panic!("expected a raw element, got {other:?}"),
    }
}

#[test]
fn unterminated_fenced_code_block_runs_to_eof_without_error() {
    // Deliberately asymmetric with `<raw>[...]`'s hard EOF error
    // (see `parse_fenced_code_block`'s doc comment) -- matches
    // CommonMark's own spec for an unclosed fence.
    let doc = parse_document("```\nline one\nline two").unwrap();
    match &doc.blocks[0] {
        Block::Element(el) => {
            assert_eq!(
                el.content,
                Some(vec![Inline::Raw("line one\nline two".into())])
            );
        }
        other => panic!("expected a raw element, got {other:?}"),
    }
}

#[test]
fn unterminated_inline_backtick_falls_back_to_literal_text() {
    // Regression: a single unterminated '`' used to swallow everything
    // up to the next stray backtick anywhere later in the source,
    // across paragraph boundaries, collapsing newlines to spaces along
    // the way. It must now fall back to a literal '`' and let the
    // paragraph end normally at the blank line.
    let doc = parse_document("keep `this open\n\nnext paragraph\n").unwrap();
    assert_eq!(doc.blocks.len(), 2);
    match (&doc.blocks[0], &doc.blocks[1]) {
        (Block::Paragraph(a), Block::Paragraph(b)) => {
            assert_eq!(&a.content, &vec![Inline::Text("keep `this open".into())]);
            assert_eq!(&b.content, &vec![Inline::Text("next paragraph".into())]);
        }
        other => panic!("expected two paragraphs, got {other:?}"),
    }
}

#[test]
fn backtick_span_still_closes_normally_on_the_same_line() {
    let doc = parse_document("call `foo()` now\n").unwrap();
    match &doc.blocks[0] {
        Block::Paragraph(p) => {
            assert_eq!(p.content.len(), 3);
            assert_eq!(p.content[0], Inline::Text("call ".into()));
            match &p.content[1] {
                Inline::Element(el) => {
                    assert_eq!(el.sigil, Sigil::named("raw"));
                    assert_eq!(el.placement, tomet_ast::Placement::Inline);
                    assert_eq!(el.content, Some(vec![Inline::Raw("foo()".into())]));
                }
                other => panic!("expected inline @raw element, got {other:?}"),
            }
            assert_eq!(p.content[2], Inline::Text(" now".into()));
        }
        other => panic!("expected a paragraph, got {other:?}"),
    }
}
