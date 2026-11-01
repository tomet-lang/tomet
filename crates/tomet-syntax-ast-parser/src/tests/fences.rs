use super::*;

/// The `Inline::Raw` text of `doc`'s first block, which must be a `@name`
/// element whose `[content]` is a single nested backtick-fenced `@raw`
/// element -- the replacement for the old `+++` raw-body fence: `[...]`
/// holds the same blocks the document root does, so a fenced code block
/// nests inside it exactly as it would at the top level.
fn nested_raw_body(doc: &tomet_ast::File) -> &str {
    let outer_content = if let Some(d) = doc.header.first() {
        d.content.as_deref()
    } else if let Some(Block::Element(outer)) = doc.body.first() {
        outer.content.as_deref()
    } else {
        panic!("expected an element or a declaration, got {doc:?}");
    };
    let content = outer_content.unwrap_or(&[]);
    let Some(Block::Element(raw)) = content.first() else {
        panic!("expected a nested raw element, got {content:?}");
    };
    match raw.content.as_deref() {
        Some([Block::Paragraph(p)]) => match p.content.as_slice() {
            [Inline::Raw(text)] => &text.value,
            other => panic!("expected a single raw inline, got {other:?}"),
        },
        other => panic!("expected a single paragraph, got {other:?}"),
    }
}

#[test]
fn a_fenced_raw_body_nested_in_content_preserves_brackets_and_newlines_losslessly() {
    let doc =
        parse_file("@memo[\n```\nline one\nline two with * and [brackets] inside\n```\n]\n")
            .unwrap();
    assert_eq!(
        nested_raw_body(&doc),
        "line one\nline two with * and [brackets] inside"
    );
}

#[test]
fn a_fenced_raw_body_in_content_is_not_confused_by_an_apostrophe() {
    // Free-form prose gives no guarantee its `'`/`"` occurrences are
    // balanced; a fence has no such problem -- it ends at a line, so
    // nothing inside it can be miscounted.
    let doc = parse_file("@memo[\n```\ndon't forget [this]\n```\n]\n").unwrap();
    assert_eq!(nested_raw_body(&doc), "don't forget [this]");
}

#[test]
fn a_fenced_raw_body_in_content_is_not_confused_by_an_unquoted_brace() {
    let doc = parse_file("@memo[\n```\na: \"}\"\nb: 1\n```\n]\n").unwrap();
    assert_eq!(nested_raw_body(&doc), "a: \"}\"\nb: 1");
}

#[test]
fn interpolation_does_not_expand_inside_a_fenced_raw_body_in_content() {
    // `default.config.tmt` stores macro templates such as
    // `"https://github.com/.../${1}"` that must reach
    // `tomet-transform`'s `MacroPattern::from_template` verbatim.
    let doc = parse_file("::config[\n```\n{\"gh\": \"x/${1}\"}\n```\n]\n").unwrap();
    assert_eq!(nested_raw_body(&doc), "{\"gh\": \"x/${1}\"}");
}

#[test]
fn content_raw_is_now_just_an_ordinary_argument() {
    // `(content:raw)` no longer changes how `[...]` is lexed -- that
    // was one of the four places the parser consulted an element's
    // own arguments. Line breaks collapse per the usual rule.
    let doc = parse_file("@memo(content:raw)[\nline one\nline two\n]\n").unwrap();
    match &doc.body[0] {
        Block::Element(el) => {
            assert_eq!(
                el.content,
                wrap(vec![
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
    let doc = parse_file("```rust\nfn main() {}\n```\n").unwrap();
    match &doc.body[0] {
        Block::Element(el) => {
            assert_eq!(el.name, Name::bare("raw"));
            assert_eq!(
                el.data.args,
                Some(Value::Map(vec![(
                    "lang".to_string(),
                    Value::String("rust".to_string())
                )]))
            );
            assert_eq!(el.content, wrap(vec![Inline::Raw("fn main() {}".into())]));
        }
        other => panic!("expected a raw element, got {other:?}"),
    }
}

#[test]
fn parses_a_fenced_code_block_without_lang() {
    let doc = parse_file("```\nplain\n```\n").unwrap();
    match &doc.body[0] {
        Block::Element(el) => {
            assert_eq!(el.name, Name::bare("raw"));
            assert_eq!(el.data.args, None);
            assert_eq!(el.content, wrap(vec![Inline::Raw("plain".into())]));
        }
        other => panic!("expected a raw element, got {other:?}"),
    }
}

#[test]
fn fenced_code_block_and_bracket_raw_produce_the_same_ast() {
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
    let fenced = parse_file("```rust\nfn main() {}\n```\n").unwrap();
    let bracket = parse_file("@raw(lang:rust)[fn main() {}]\n").unwrap();
    match (&fenced.body[0], &bracket.body[0]) {
        (Block::Element(a), Block::Element(b)) => {
            assert_eq!(a.name, b.name);
            assert_eq!(a.data.args, b.data.args);
            assert_eq!(a.data.value, b.data.value);
            assert_eq!(a.content, wrap(vec![Inline::Raw("fn main() {}".into())]));
            assert_eq!(b.content, wrap(vec![Inline::Text("fn main() {}".into())]));
        }
        other => panic!("expected two raw elements, got {other:?}"),
    }
}

#[test]
fn fenced_code_block_closing_fence_needs_at_least_the_opening_backtick_count() {
    // A closing fence with fewer backticks than the opening one doesn't
    // close it -- it's just consumed as ordinary body content, same as
    // CommonMark.
    let doc = parse_file("````\ncode\n```\nmore code\n````\n").unwrap();
    match &doc.body[0] {
        Block::Element(el) => {
            assert_eq!(
                el.content,
                wrap(vec![Inline::Raw("code\n```\nmore code".into())])
            );
        }
        other => panic!("expected a raw element, got {other:?}"),
    }
}

#[test]
fn fenced_code_block_closing_fence_can_have_more_backticks_than_opening() {
    let doc = parse_file("```\ncode\n`````\n").unwrap();
    match &doc.body[0] {
        Block::Element(el) => {
            assert_eq!(el.content, wrap(vec![Inline::Raw("code".into())]));
        }
        other => panic!("expected a raw element, got {other:?}"),
    }
}

#[test]
fn fenced_code_block_body_can_contain_short_backtick_runs() {
    let doc = parse_file("```\nsee `foo` and ``bar``\n```\n").unwrap();
    match &doc.body[0] {
        Block::Element(el) => {
            assert_eq!(
                el.content,
                wrap(vec![Inline::Raw("see `foo` and ``bar``".into())])
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
    let doc = parse_file("```\nline one\nline two").unwrap();
    match &doc.body[0] {
        Block::Element(el) => {
            assert_eq!(
                el.content,
                wrap(vec![Inline::Raw("line one\nline two".into())])
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
    let doc = parse_file("keep `this open\n\nnext paragraph\n").unwrap();
    assert_eq!(doc.body.len(), 2);
    match (&doc.body[0], &doc.body[1]) {
        (Block::Paragraph(a), Block::Paragraph(b)) => {
            assert_eq!(&a.content, &vec![Inline::Text("keep `this open".into())]);
            assert_eq!(&b.content, &vec![Inline::Text("next paragraph".into())]);
        }
        other => panic!("expected two paragraphs, got {other:?}"),
    }
}

#[test]
fn backtick_span_still_closes_normally_on_the_same_line() {
    let doc = parse_file("call `foo()` now\n").unwrap();
    match &doc.body[0] {
        Block::Paragraph(p) => {
            assert_eq!(p.content.len(), 3);
            assert_eq!(p.content[0], Inline::Text("call ".into()));
            match &p.content[1] {
                Inline::Element(el) => {
                    assert_eq!(el.name, Name::bare("raw"));
                    assert_eq!(el.placement, tomet_ast::Placement::Inline);
                    assert_eq!(el.content, wrap(vec![Inline::Raw("foo()".into())]));
                }
                other => panic!("expected inline @raw element, got {other:?}"),
            }
            assert_eq!(p.content[2], Inline::Text(" now".into()));
        }
        other => panic!("expected a paragraph, got {other:?}"),
    }
}

fn line_raw_text(src: &str) -> String {
    let doc = parse_file(src).unwrap();
    let [Block::Element(el)] = doc.body.as_slice() else {
        panic!("expected one raw element, got {:?}", doc.body);
    };
    assert_eq!(el.name, Name::bare("raw"));
    assert_eq!(el.placement, Placement::Block);
    assert_eq!(el.data.args, None);
    let Some([Block::Paragraph(p)]) = el.content.as_deref() else {
        panic!()
    };
    let [Inline::Raw(raw)] = p.content.as_slice() else {
        panic!()
    };
    raw.value.clone()
}

#[test]
fn two_backticks_at_the_start_of_a_line_make_the_whole_line_raw() {
    assert_eq!(line_raw_text("`` a *b* @c(d) ${e}\n"), "a *b* @c(d) ${e}");
    // One space after the backticks is the separator; a second is text.
    assert_eq!(line_raw_text("``  indented\n"), " indented");
    assert_eq!(line_raw_text("``no space\n"), "no space");
    // It wins over an inline raw that stands at the start of the line.
    assert_eq!(line_raw_text("``a`b``\n"), "a`b``");
}

#[test]
fn consecutive_line_raw_lines_are_one_raw_block() {
    assert_eq!(
        line_raw_text("`` one\n`` two\n``\n``three\n"),
        "one\ntwo\n\nthree"
    );
    // A blank line ends it.
    let doc = parse_file("`` one\n\n`` two\n").unwrap();
    assert_eq!(doc.body.len(), 2);
}

#[test]
fn a_line_raw_breaks_the_paragraph_before_it_and_not_a_fence_after_it() {
    let doc = parse_file("text\n`` raw\n```\nfence\n```\n").unwrap();
    assert!(matches!(doc.body[0], Block::Paragraph(_)));
    assert_eq!(doc.body.len(), 3, "{:?}", doc.body);
}

#[test]
fn two_backticks_inside_a_line_are_still_an_inline_raw() {
    let doc = parse_file("a ``b`c`` d\n").unwrap();
    let [Block::Paragraph(p)] = doc.body.as_slice() else {
        panic!()
    };
    assert!(
        p.content
            .iter()
            .any(|i| matches!(i, Inline::Element(el) if el.name == Name::bare("raw")))
    );
    // And in a `[content]`, where it is the first thing after the bracket.
    let doc = parse_file("@x[ ``b`c`` ]\n").unwrap();
    let Block::Element(el) = &doc.body[0] else {
        panic!()
    };
    assert!(matches!(el.content.as_deref(), Some([Block::Paragraph(_)])));
}

#[test]
fn a_line_raw_in_a_pipe_run_loses_the_markers() {
    let doc = parse_file("@quote\n| `` one\n| `` two\n").unwrap();
    let Block::Element(q) = &doc.body[0] else {
        panic!()
    };
    let Some([Block::Element(raw)]) = q.content.as_deref() else {
        panic!("{:?}", q.content)
    };
    let Some([Block::Paragraph(p)]) = raw.content.as_deref() else {
        panic!()
    };
    let [Inline::Raw(t)] = p.content.as_slice() else {
        panic!()
    };
    assert_eq!(t.value, "one\ntwo");
}
