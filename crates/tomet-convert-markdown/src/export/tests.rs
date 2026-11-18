use super::*;
use tomet_ast::{
    Data, Declaration, Element, File, Group, Name, Paragraph, Placement, RawText, Text, Tuple,
    Value,
};
use tomet_scalar::Scalar;
use tomet_span::Span;

/// Wraps a flat inline sequence as `Element.content` (`Vec<Block>`) -- every fixture below is just one paragraph's worth.
fn wrap_content(inlines: Vec<Inline>) -> Vec<Block> {
    vec![Block::Paragraph(Paragraph::new(inlines, Span::dummy()))]
}

fn doc_from_blocks(blocks: Vec<Block>) -> File {
    File {
        header: Vec::new(),
        body: blocks,
        span: Span::dummy(),
    }
}

#[test]
fn a_backtick_span_survives_export_unescaped() {
    // Tomet keeps `` `x` `` as literal text (the parser only shields
    // the run from further markup), so it reaches the writer looking
    // like prose. Escaping it turned every `` `spec/` `` in the docs
    // into a literal ``\`spec/\``.
    let doc = tomet_ast_parser::parse_file("地の文に `code` と `spec/` があります。\n").unwrap();
    assert_eq!(
        to_markdown(&doc, &carried(&doc)).trim(),
        "地の文に `code` と `spec/` があります。"
    );
}

#[test]
fn ruby_exports_as_raw_html() {
    let doc = tomet_ast_parser::parse_file("a @ruby[漢字](rt:\"かんじ\") b\n").unwrap();
    assert_eq!(
        to_markdown(&doc, &carried(&doc)).trim(),
        "a <ruby>漢字<rt>かんじ</rt></ruby> b"
    );
}

#[test]
fn an_unpaired_backtick_is_still_escaped() {
    // No closing backtick on the line -- not a span, so it stays an
    // escaped literal.
    let doc = tomet_ast_parser::parse_file("値段は 100` です\n").unwrap();
    assert_eq!(
        to_markdown(&doc, &carried(&doc)).trim(),
        "値段は 100\\` です"
    );
}

#[test]
fn a_backtick_span_is_not_escaped_inside_a_table_cell() {
    let doc =
        tomet_ast_parser::parse_file("@table()[\n[ Dir ][ Lang ]\n[ `spec/` ][ 日本語 ]\n]{}\n")
            .unwrap();
    assert!(
        to_markdown(&doc, &carried(&doc)).contains("| `spec/` |"),
        "got: {}",
        to_markdown(&doc, &carried(&doc))
    );
}

#[test]
fn directives_have_no_markdown_output() {
    // Every directive kind is listed: one missing falls through to the
    // generic passthrough and emits a `<div data-tm-kind="settings">`,
    // as `settings` and the binding element would.
    //
    // Dropping `import` from `BUILTIN_KINDS` would make it a `Custom`
    // kind, and the div would come straight back.
    for src in [
        "::export(path:out.md)\n",
        "::use(deck)\n",
        "::meta(type: note)\n",
    ] {
        let doc = tomet_ast_parser::parse_file(src).unwrap();
        assert_eq!(to_markdown(&doc, &carried(&doc)).trim(), "", "for {src:?}");
    }
}

#[test]
fn adjacent_meta_blocks_have_no_visible_output() {
    // A declaration is a block of its own, and one that renders as nothing:
    // several in a row must leave no stray blank line behind in the exported
    // Markdown.
    fn meta_declaration(tag: &str) -> Declaration {
        let mut declaration = tomet_tree::declaration_new("meta");
        declaration.data = Data {
            args: Some(Tuple::from_unnamed([Value::Scalar(Scalar::Text(
                tag.to_string(),
            ))])),
            value: Some(Group::from_map(Tuple::from_named([(
                "key".to_string(),
                Value::Scalar(Scalar::Text("value".to_string())),
            )]))),
        };
        declaration
    }
    let doc = File {
        header: vec![
            meta_declaration("json"),
            meta_declaration("yaml"),
            meta_declaration("toml"),
        ],
        body: vec![Block::Element(heading_element(
            1,
            vec![Inline::Text(Text::new("next", Span::dummy()))],
        ))],
        span: Span::dummy(),
    };
    assert_eq!(to_markdown(&doc, &carried(&doc)), "# next\n\n");
}

fn heading_element(level: i64, content: Vec<Inline>) -> Element {
    Element {
        name: Name::bare("heading"),

        placement: Placement::Block,

        data: Data {
            args: Some(Tuple::from_unnamed([Value::Scalar(Scalar::Int(level))])),
            value: None,
        },

        content: Some(wrap_content(content)),

        id: None,

        connects: Vec::new(),

        span: Span::dummy(),
    }
}

#[test]
fn heading_and_paragraph() {
    let doc = doc_from_blocks(vec![
        Block::Element(heading_element(
            2,
            vec![Inline::Text(Text::new("Title", Span::dummy()))],
        )),
        Block::Paragraph(Paragraph::new(
            vec![Inline::Text(Text::new("Hello.", Span::dummy()))],
            Span::dummy(),
        )),
    ]);
    assert_eq!(to_markdown(&doc, &carried(&doc)), "## Title\n\nHello.\n\n");
}

#[test]
fn nested_inline_heading_does_not_export_as_bare_hash_line() {
    // `@heading(2)[...]` nested inside a paragraph (inline position)
    // must not emit a bare `## text` mid-paragraph -- that wouldn't
    // parse back as a heading on re-import anyway. It falls through to
    // generic/custom rendering instead (raw HTML passthrough, per this
    // module's documented fallback for constructs with no CommonMark
    // form).
    let doc = doc_from_blocks(vec![Block::Paragraph(Paragraph::new(
        vec![
            Inline::Text(Text::new("before ", Span::dummy())),
            Inline::Element(heading_element(
                2,
                vec![Inline::Text(Text::new("Nested", Span::dummy()))],
            )),
            Inline::Text(Text::new(" after", Span::dummy())),
        ],
        Span::dummy(),
    ))]);
    let md = to_markdown(&doc, &carried(&doc));
    assert!(!md.contains("## Nested"), "got: {md:?}");
}

#[test]
fn bullet_and_ordered_list() {
    let doc = doc_from_blocks(vec![Block::List(tomet_tree::list(
        true,
        vec![
            tomet_tree::list_item(
                None,
                wrap_content(vec![Inline::Text(Text::new("one", Span::dummy()))]),
                None,
                None,
                None,
                Vec::new(),
                None,
                Span::dummy(),
            ),
            tomet_tree::list_item(
                None,
                wrap_content(vec![Inline::Text(Text::new("two", Span::dummy()))]),
                None,
                None,
                None,
                Vec::new(),
                None,
                Span::dummy(),
            ),
        ],
        Span::dummy(),
    ))]);
    assert_eq!(to_markdown(&doc, &carried(&doc)), "1. one\n2. two\n\n");
}

#[test]
fn emphasis_and_strong() {
    let doc = doc_from_blocks(vec![Block::Paragraph(Paragraph::new(
        vec![
            Inline::Element(Element {
                name: Name::bare("em"),

                placement: Placement::Inline,

                data: Data {
                    args: None,
                    value: None,
                },

                content: Some(wrap_content(vec![Inline::Text(Text::new(
                    "a",
                    Span::dummy(),
                ))])),

                id: None,

                connects: Vec::new(),

                span: Span::dummy(),
            }),
            Inline::Text(Text::new(" ", Span::dummy())),
            Inline::Element(Element {
                name: Name::bare("strong"),

                placement: Placement::Inline,

                data: Data {
                    args: None,
                    value: None,
                },

                content: Some(wrap_content(vec![Inline::Text(Text::new(
                    "b",
                    Span::dummy(),
                ))])),

                id: None,

                connects: Vec::new(),

                span: Span::dummy(),
            }),
        ],
        Span::dummy(),
    ))]);
    assert_eq!(to_markdown(&doc, &carried(&doc)), "*a* **b**\n\n");
}

#[test]
fn link_round_trips() {
    let el = Element {
        name: Name::bare("link"),

        placement: Placement::Inline,

        data: Data {
            args: Some(Tuple::from_named([(
                "target".to_string(),
                Value::Scalar(Scalar::Text("https://example.com".to_string())),
            )])),
            value: None,
        },

        content: Some(wrap_content(vec![Inline::Text(Text::new(
            "Wiki",
            Span::dummy(),
        ))])),

        id: None,

        connects: Vec::new(),

        span: Span::dummy(),
    };
    let doc = doc_from_blocks(vec![Block::Paragraph(Paragraph::new(
        vec![Inline::Element(el)],
        Span::dummy(),
    ))]);
    assert_eq!(
        to_markdown(&doc, &carried(&doc)),
        "[Wiki](https://example.com)\n\n"
    );
}

#[test]
fn embed_becomes_image() {
    let el = Element {
        name: Name::bare("embed"),

        placement: Placement::Inline,

        data: Data {
            args: Some(Tuple::from_named([(
                "target".to_string(),
                Value::Scalar(Scalar::Text("pic.png".to_string())),
            )])),
            value: None,
        },

        content: Some(wrap_content(vec![Inline::Text(Text::new(
            "a cat",
            Span::dummy(),
        ))])),

        id: None,

        connects: Vec::new(),

        span: Span::dummy(),
    };
    let doc = doc_from_blocks(vec![Block::Paragraph(Paragraph::new(
        vec![Inline::Element(el)],
        Span::dummy(),
    ))]);
    assert_eq!(to_markdown(&doc, &carried(&doc)), "![a cat](pic.png)\n\n");
}

#[test]
fn raw_block_uses_fence_and_lang() {
    let el = Element {
        name: Name::bare("raw"),

        placement: Placement::Block,

        data: Data {
            args: Some(Tuple::from_named([(
                "lang".to_string(),
                Value::Scalar(Scalar::Text("rust".to_string())),
            )])),
            value: None,
        },

        content: Some(wrap_content(vec![Inline::Text(Text::new(
            "fn main() {}",
            Span::dummy(),
        ))])),

        id: None,

        connects: Vec::new(),

        span: Span::dummy(),
    };
    let doc = doc_from_blocks(vec![Block::Element(el)]);
    assert_eq!(
        to_markdown(&doc, &carried(&doc)),
        "```rust\nfn main() {}\n```\n\n"
    );
}

#[test]
fn raw_export_fence_selection_and_roundtrip() {
    let cases = [
        ("multiline", "line 1\nline 2\nline 3", "```"),
        ("backticks_inline", "code with `inline`", "```"),
        ("fence_inside", "before\n```\nafter", "````"),
        ("four_backticks", "before\n````\nafter", "`````"),
        ("leading_spaces", "   four spaces\n  two spaces", "```"),
        ("empty", "", "```"),
    ];
    for (_name, code, expected_fence) in cases {
        let doc = make_raw_block(code, Some("tmt"));
        let md = to_markdown(&doc, &carried(&doc));
        assert!(
            md.starts_with(&format!("{expected_fence}tmt\n")),
            "expected fence {expected_fence} for {code:?}, got:\n{md}"
        );
        let re_doc = crate::import::from_markdown(&md);
        match &re_doc.body[0] {
            Block::Element(el) => {
                let roundtripped = el
                    .content
                    .as_deref()
                    .map(blocks_to_plain)
                    .unwrap_or_default();
                assert_eq!(roundtripped, code, "roundtrip mismatch for code {code:?}");
            }
            other => panic!("expected Block::Element, got {other:?}"),
        }
    }
}

fn make_raw_block(code: &str, lang: Option<&str>) -> File {
    let el = Element {
        name: Name::bare("raw"),
        placement: Placement::Block,
        data: Data {
            args: lang.map(|l| {
                Tuple::from_named([(
                    "lang".to_string(),
                    Value::Scalar(Scalar::Text(l.to_string())),
                )])
            }),
            value: None,
        },
        content: Some(wrap_content(vec![Inline::Raw(RawText::new(
            code,
            Span::dummy(),
        ))])),
        id: None,
        connects: Vec::new(),
        span: Span::dummy(),
    };
    doc_from_blocks(vec![Block::Element(el)])
}

#[test]
fn raw_inline_uses_backticks() {
    let el = Element {
        name: Name::bare("raw"),

        placement: Placement::Inline,

        data: Data {
            args: None,
            value: None,
        },

        content: Some(wrap_content(vec![Inline::Raw(RawText::new(
            "foo()",
            Span::dummy(),
        ))])),

        id: None,

        connects: Vec::new(),

        span: Span::dummy(),
    };
    let doc = doc_from_blocks(vec![Block::Paragraph(Paragraph::new(
        vec![
            Inline::Text(Text::new("call ", Span::dummy())),
            Inline::Element(el),
            Inline::Text(Text::new(" now", Span::dummy())),
        ],
        Span::dummy(),
    ))]);
    assert_eq!(to_markdown(&doc, &carried(&doc)), "call `foo()` now\n\n");
}

#[test]
fn thematic_break() {
    let doc = doc_from_blocks(vec![Block::Element(tomet_tree::element_new(Name::bare(
        "hr",
    )))]);
    assert_eq!(to_markdown(&doc, &carried(&doc)), "---\n\n");
}

#[test]
fn a_declaration_exports_as_nothing() {
    let mut declaration = tomet_tree::declaration_new("export");
    declaration.data.args = Some(Tuple::from_named([(
        "type".to_string(),
        Value::Scalar(Scalar::Text("commonmark".to_string())),
    )]));
    let doc = File {
        header: vec![declaration],
        body: Vec::new(),
        span: Span::dummy(),
    };
    assert_eq!(to_markdown(&doc, &carried(&doc)), "");
}

/// The pair this asserts is the point: a bold line, a blank line, then
/// the rule. `Title\n---` is a setext heading, so the divider would come
/// out as an `<h2>`.
#[test]
fn titled_thematic_break_is_not_a_setext_heading() {
    let mut el = tomet_tree::element_new(Name::bare("hr"));
    el.content = Some(wrap_content(vec![Inline::Text(Text::new(
        "Title",
        Span::dummy(),
    ))]));
    let doc = doc_from_blocks(vec![Block::Element(el)]);
    assert_eq!(to_markdown(&doc, &carried(&doc)), "**Title**\n\n---\n\n");
}

#[test]
fn wikilink_exports_to_markdown() {
    let mut el1 = tomet_tree::element_new(Name::bare("link"));
    el1.data.args = Some(Tuple::from_named([(
        "target".to_string(),
        Value::Scalar(Scalar::Text("ref:name".to_string())),
    )]));

    let mut el2 = tomet_tree::element_new(Name::bare("link"));
    el2.data.args = Some(Tuple::from_named([(
        "target".to_string(),
        Value::Scalar(Scalar::Text("ref:name".to_string())),
    )]));
    el2.content = Some(wrap_content(vec![Inline::Text(Text::new(
        "display",
        Span::dummy(),
    ))]));

    let doc = doc_from_blocks(vec![Block::Paragraph(Paragraph::new(
        vec![
            Inline::Element(el1),
            Inline::Text(Text::new(" and ", Span::dummy())),
            Inline::Element(el2),
        ],
        Span::dummy(),
    ))]);
    assert_eq!(
        to_markdown(&doc, &carried(&doc)),
        "[[name]] and [[name|display]]\n\n"
    );
}

/// A paragraph holding one `@link(target)`, with `label` as its content when
/// there is one.
fn link_doc(target: &str, label: Option<&str>) -> File {
    let mut el = tomet_tree::element_new(Name::bare("link"));
    el.data.args = Some(Tuple::from_named([(
        "target".to_string(),
        Value::Scalar(Scalar::Text(target.to_string())),
    )]));
    if let Some(label) = label {
        el.content = Some(wrap_content(vec![Inline::Text(Text::new(
            label,
            Span::dummy(),
        ))]));
    }
    doc_from_blocks(vec![Block::Paragraph(Paragraph::new(
        vec![Inline::Element(el)],
        Span::dummy(),
    ))])
}

/// A path with no label is a link and not bare text -- HTML makes an
/// anchor of the same thing. The converter writes the target it is handed;
/// where a `tmt:` link points is decided before rendering.
#[test]
fn an_unlabeled_path_link_is_a_link() {
    assert_eq!(
        export(&link_doc("tmt:docs/a.tmt#intro", None)),
        "[docs/a.tmt#intro](docs/a.tmt#intro)\n\n"
    );
    assert_eq!(
        export(&link_doc("file:docs/a.png", None)),
        "[docs/a.png](docs/a.png)\n\n"
    );
}

#[test]
fn a_labeled_path_link_keeps_its_label() {
    assert_eq!(
        export(&link_doc("tmt:docs/a.tmt#intro", Some("the intro"))),
        "[the intro](docs/a.tmt#intro)\n\n"
    );
}

/// A URL on its own is already a link to a reader and to most renderers, so
/// it stays as it was.
#[test]
fn an_unlabeled_url_stays_bare() {
    assert_eq!(
        export(&link_doc("https://example.com", None)),
        "https://example.com\n\n"
    );
}

#[test]
fn table_exports_to_markdown() {
    let mut el = tomet_tree::element_new(Name::bare("table"));
    el.content = Some(wrap_content(vec![Inline::Text(Text::new(
        "[ col1 ][ col2 ]\n[ val1 ][ val2 ]",
        Span::dummy(),
    ))]));
    let doc = doc_from_blocks(vec![Block::Element(el)]);
    assert_eq!(
        to_markdown(&doc, &carried(&doc)),
        "| col1 | col2 |\n| --- | --- |\n| val1 | val2 |\n\n"
    );
}

/// This crate does not evaluate `{$ ... }`, and this test is what is
/// left of the one that said it did.
///
/// The expanded macro is asserted where the expansion happens, in
/// `tomet-transform`'s `resolve_expressions`, before any converter
/// sees the tree -- because converters each deciding it would produce
/// different answers for the same document.
///
/// What reaches here is a document nobody prepared, and the only
/// honest rendering of that is what was written.
#[test]
fn an_unprepared_interpolation_renders_as_its_own_source() {
    // The source's own line wrapping survives to the markdown output
    // (`inline_to_md` renders a `SoftBreak` as a literal newline,
    // not a space -- see its own comment) rather than being folded
    // into one long line.
    let doc =
        tomet_ast_parser::parse_file("Issue: $gh(42)\nFooter: $(copyright)\nMath: $add(10, 5)\n")
            .unwrap();
    assert_eq!(
        to_markdown(&doc, &carried(&doc)),
        "Issue: $gh(42)\nFooter: $(copyright)\nMath: $add(10, 5)\n\n"
    );
}

#[test]
fn inline_footnote_exports_to_markdown() {
    let doc =
        tomet_ast_parser::parse_file("This has a footnote @ftn[a short note] in text.\n").unwrap();
    let md = to_markdown(&doc, &carried(&doc));
    assert!(
        md.contains("This has a footnote [^1] in text."),
        "got: {md}"
    );
    assert!(md.contains("[^1]: a short note"), "got: {md}");
}

#[test]
fn separated_footnote_with_multiple_references() {
    let src = "\
Prose A @ftn(shared).
Prose B @ftn(shared).

@ftn(shared)[Shared footnote explanation.]
";
    let doc = tomet_ast_parser::parse_file(src).unwrap();
    let md = to_markdown(&doc, &carried(&doc));
    assert!(md.contains("Prose A [^1]."), "got: {md}");
    assert!(md.contains("Prose B [^1]."), "got: {md}");
    assert!(
        md.contains("[^1]: Shared footnote explanation."),
        "got: {md}"
    );
    // Ensure definition block was not rendered in place
    assert!(!md.contains("@ftn"), "got: {md}");
}

#[test]
fn tag_elements_export_to_markdown() {
    let src = "@tag(rust, tomet) and @tag(spec)\n";
    let doc = tomet_ast_parser::parse_file(src).unwrap();
    let md = to_markdown(&doc, &carried(&doc));
    assert_eq!(md.trim(), "#rust #tomet and #spec");
}

#[test]
fn task_elements_export_to_markdown() {
    let src = "\
-@task(x)[ Done task ]
-@task(todo)[ Todo task ]
-@task(\" \")[ Unchecked task ]
";
    let doc = tomet_ast_parser::parse_file(src).unwrap();
    let md = to_markdown(&doc, &carried(&doc));
    assert_eq!(
        md.trim(),
        "- [x] Done task\n- [ ] Todo task\n- [ ] Unchecked task"
    );
}

#[test]
fn standalone_task_element_exports_to_markdown() {
    let src = "@task(done)[ Single task ]\n";
    let doc = tomet_ast_parser::parse_file(src).unwrap();
    let md = to_markdown(&doc, &carried(&doc));
    assert_eq!(md.trim(), "[x] Single task");
}

#[test]
fn a_key_that_is_not_an_attribute_name_adds_no_attribute() {
    let doc = tomet_ast_parser::parse_file("@x(\"a onclick=alert(1)\": 1, plain: 2)\n").unwrap();
    let md = to_markdown(&doc, &carried(&doc));
    assert!(md.contains(" data-plain=\"2\""), "{md}");
    assert!(!md.contains("onclick="), "{md}");
}

#[test]
fn a_comment_exports_as_an_html_comment() {
    let doc = tomet_ast_parser::parse_file("@comment[ A block comment ]\n").unwrap();
    assert_eq!(
        to_markdown(&doc, &carried(&doc)),
        "<!-- A block comment -->\n\n"
    );

    let doc = tomet_ast_parser::parse_file("before @comment[ an inline comment ] after\n").unwrap();
    assert_eq!(
        to_markdown(&doc, &carried(&doc)).trim(),
        "before <!-- an inline comment --> after"
    );
}

#[test]
fn a_comment_written_in_std_exports_the_same() {
    let doc = tomet_ast_parser::parse_file("@std.comment[ Generated from file.tmt. ]\n").unwrap();
    assert_eq!(
        to_markdown(&doc, &carried(&doc)),
        "<!-- Generated from file.tmt. -->\n\n"
    );
}

#[test]
fn a_comment_cannot_end_early() {
    let doc = tomet_ast_parser::parse_file("@comment[ foo --> bar ]\n").unwrap();
    assert_eq!(
        to_markdown(&doc, &carried(&doc)),
        "<!-- foo -- > bar -->\n\n"
    );
}

#[test]
fn a_type_seat_is_shown_as_written_in_a_block_and_in_text() {
    let doc =
        tomet_ast_parser::parse_file("<$list(string, length: 6)>\n\nid <$uuid> and <$date>\n")
            .unwrap();
    // The `<` is no HTML tag's, so it needs no escape; `_` and the like are Markdown's.
    assert_eq!(
        to_markdown(&doc, &carried(&doc)),
        "<$list(string, length: 6)>\n\nid <$uuid> and <$date>\n\n"
    );
}

#[test]
fn list_marker_rules_1_to_3_unordered() {
    // 1. Lowercase `x` is a checked task item.
    let doc = tomet_ast_parser::parse_file("- (x) done\n").unwrap();
    assert_eq!(to_markdown(&doc, &carried(&doc)), "- [x] done\n\n");

    // 2. Empty tuple is an unchecked task item.
    let doc = tomet_ast_parser::parse_file("- () unchecked\n- ( ) space unchecked\n").unwrap();
    assert_eq!(
        to_markdown(&doc, &carried(&doc)),
        "- [ ] unchecked\n- [ ] space unchecked\n\n"
    );

    // 3. Other scalar markers become bold labels.
    let doc = tomet_ast_parser::parse_file("- (feat) New feature\n").unwrap();
    assert_eq!(
        to_markdown(&doc, &carried(&doc)),
        "- **feat**: New feature\n\n"
    );

    // Explicit key `marker:` produces the same output.
    let doc =
        tomet_ast_parser::parse_file("- (marker: x) done\n- (marker: feat) New feature\n").unwrap();
    assert_eq!(
        to_markdown(&doc, &carried(&doc)),
        "- [x] done\n- **feat**: New feature\n\n"
    );
}

#[test]
fn list_marker_rule_4_ordered_list() {
    let doc = tomet_ast_parser::parse_file("-. (x) done\n-. () todo\n-. (feat) feature\n").unwrap();
    assert_eq!(
        to_markdown(&doc, &carried(&doc)),
        "1. [x] done\n2. [ ] todo\n3. **feat**: feature\n\n"
    );
}

#[test]
fn list_marker_rule_5_multiline_item() {
    let doc =
        tomet_ast_parser::parse_file("- (feat)| first line\n        | second line\n").unwrap();
    assert_eq!(
        to_markdown(&doc, &carried(&doc)),
        "- **feat**: first line\nsecond line\n\n"
    );

    let doc = tomet_ast_parser::parse_file("- (x)| first line\n     | second line\n").unwrap();
    assert_eq!(
        to_markdown(&doc, &carried(&doc)),
        "- [x] first line\nsecond line\n\n"
    );

    let doc = tomet_ast_parser::parse_file("- ()| first line\n    | second line\n").unwrap();
    assert_eq!(
        to_markdown(&doc, &carried(&doc)),
        "- [ ] first line\nsecond line\n\n"
    );
}

#[test]
fn list_marker_rule_6_item_id_and_nested_sublist() {
    let doc = tomet_ast_parser::parse_file(
        "- (feat)#(item-id) feature text\n  - (x) child done\n  - () child todo\n",
    )
    .unwrap();
    assert_eq!(
        to_markdown(&doc, &carried(&doc)),
        "- **feat**: feature text\n  - [x] child done\n  - [ ] child todo\n\n"
    );
}

#[test]
fn list_marker_scalar_types_number_boolean_and_quoted() {
    // Number marker (12)
    let doc = tomet_ast_parser::parse_file("- (12) number item\n").unwrap();
    assert_eq!(
        to_markdown(&doc, &carried(&doc)),
        "- **12**: number item\n\n"
    );

    // Boolean markers
    let doc =
        tomet_ast_parser::parse_file("- (true) boolean true\n- (false) boolean false\n").unwrap();
    assert_eq!(
        to_markdown(&doc, &carried(&doc)),
        "- **true**: boolean true\n- **false**: boolean false\n\n"
    );

    // Quoted label
    let doc = tomet_ast_parser::parse_file(
        "- (\"Core Capabilities\") quoted text\n- (\"12:01\") quoted time\n",
    )
    .unwrap();
    assert_eq!(
        to_markdown(&doc, &carried(&doc)),
        "- **Core Capabilities**: quoted text\n- **12:01**: quoted time\n\n"
    );

    // Empty string label produces no label
    let doc = tomet_ast_parser::parse_file("- (\"\") empty label\n").unwrap();
    assert_eq!(to_markdown(&doc, &carried(&doc)), "- empty label\n\n");
}

#[test]
fn list_marker_label_escaping() {
    let doc = tomet_ast_parser::parse_file("- (\"*bold*\") star\n- (\"_em_\") underscore\n- (\"[bracket]\") bracket\n- (\"`code`\") code\n").unwrap();
    assert_eq!(
        to_markdown(&doc, &carried(&doc)),
        "- **\\*bold\\***: star\n- **\\_em\\_**: underscore\n- **\\[bracket\\]**: bracket\n- **`code`**: code\n\n"
    );
}

#[test]
fn list_marker_undecided_shapes_remain_unchanged() {
    // Uppercase X is a label
    let doc = tomet_ast_parser::parse_file("- (X) uppercase x\n").unwrap();
    assert_eq!(
        to_markdown(&doc, &carried(&doc)),
        "- **X**: uppercase x\n\n"
    );

    // done is a label
    let doc = tomet_ast_parser::parse_file("- (done) done label\n").unwrap();
    assert_eq!(
        to_markdown(&doc, &carried(&doc)),
        "- **done**: done label\n\n"
    );

    // todo is a label
    let doc = tomet_ast_parser::parse_file("- (todo) todo label\n").unwrap();
    assert_eq!(
        to_markdown(&doc, &carried(&doc)),
        "- **todo**: todo label\n\n"
    );

    // Tuple marker
    let doc = tomet_ast_parser::parse_file("- (marker: (1, 2)) tuple marker\n").unwrap();
    assert_eq!(to_markdown(&doc, &carried(&doc)), "- tuple marker\n\n");

    // Marker with other keys beside it: marker is shown, other keys are not shown
    let doc =
        tomet_ast_parser::parse_file("- (marker: x, priority: high) with extra keys\n").unwrap();
    assert_eq!(
        to_markdown(&doc, &carried(&doc)),
        "- [x] with extra keys\n\n"
    );

    let doc =
        tomet_ast_parser::parse_file("- (feat, priority: high) with extra keys label\n").unwrap();
    assert_eq!(
        to_markdown(&doc, &carried(&doc)),
        "- **feat**: with extra keys label\n\n"
    );

    // Two-space marker gives today's output (label)
    let doc = tomet_ast_parser::parse_file("- (\"  \") two spaces\n").unwrap();
    assert_eq!(
        to_markdown(&doc, &carried(&doc)),
        "- **  **: two spaces\n\n"
    );

    // Tab marker gives today's output (label)
    let doc = tomet_ast_parser::parse_file("- (\"\t\") tab\n").unwrap();
    assert_eq!(to_markdown(&doc, &carried(&doc)), "- **\t**: tab\n\n");

    // Second unnamed value: first is marker, second is not shown
    let doc = tomet_ast_parser::parse_file("- (a, b) two unnamed values\n").unwrap();
    assert_eq!(
        to_markdown(&doc, &carried(&doc)),
        "- **a**: two unnamed values\n\n"
    );

    // No marker key in tuple
    let doc = tomet_ast_parser::parse_file("- (priority: high) no marker key\n").unwrap();
    assert_eq!(to_markdown(&doc, &carried(&doc)), "- no marker key\n\n");

    // Time trap without quotes (parses as entry 12: 01, no marker key)
    let doc = tomet_ast_parser::parse_file("- (12:01) bare time trap\n").unwrap();
    assert_eq!(to_markdown(&doc, &carried(&doc)), "- bare time trap\n\n");
}

#[test]
fn list_marker_single_space_is_unchecked_task_item() {
    // Unordered
    let doc = tomet_ast_parser::parse_file("- (\" \") open\n").unwrap();
    assert_eq!(to_markdown(&doc, &carried(&doc)), "- [ ] open\n\n");

    // Ordered
    let doc = tomet_ast_parser::parse_file("-. (\" \") open\n").unwrap();
    assert_eq!(to_markdown(&doc, &carried(&doc)), "1. [ ] open\n\n");

    // Nested
    let doc = tomet_ast_parser::parse_file("- (\" \") parent\n  - (\" \") child\n").unwrap();
    assert_eq!(
        to_markdown(&doc, &carried(&doc)),
        "- [ ] parent\n  - [ ] child\n\n"
    );

    // ( ) and (" ") give the same output
    let doc_empty_unordered = tomet_ast_parser::parse_file("- ( ) task\n").unwrap();
    let doc_space_unordered = tomet_ast_parser::parse_file("- (\" \") task\n").unwrap();
    assert_eq!(
        to_markdown(&doc_empty_unordered, &carried(&doc_empty_unordered)),
        to_markdown(&doc_space_unordered, &carried(&doc_space_unordered))
    );

    let doc_empty_ordered = tomet_ast_parser::parse_file("-. ( ) task\n").unwrap();
    let doc_space_ordered = tomet_ast_parser::parse_file("-. (\" \") task\n").unwrap();
    assert_eq!(
        to_markdown(&doc_empty_ordered, &carried(&doc_empty_ordered)),
        to_markdown(&doc_space_ordered, &carried(&doc_space_ordered))
    );

    let doc_empty_nested = tomet_ast_parser::parse_file("- ( ) parent\n  - ( ) child\n").unwrap();
    let doc_space_nested =
        tomet_ast_parser::parse_file("- (\" \") parent\n  - (\" \") child\n").unwrap();
    assert_eq!(
        to_markdown(&doc_empty_nested, &carried(&doc_empty_nested)),
        to_markdown(&doc_space_nested, &carried(&doc_space_nested))
    );
}

#[test]
fn list_marker_round_trip_task_lists() {
    // - [ ] a and - [x] b
    let md = "- [ ] a\n- [x] b\n";
    let doc = crate::import::from_markdown(md);
    assert_eq!(to_markdown(&doc, &carried(&doc)), "- [ ] a\n- [x] b\n\n");

    // Ordered 1. [ ] a
    let md_ordered = "1. [ ] a\n";
    let doc_ordered = crate::import::from_markdown(md_ordered);
    assert_eq!(
        to_markdown(&doc_ordered, &carried(&doc_ordered)),
        "1. [ ] a\n\n"
    );

    // Nested task list
    let md_nested = "- [ ] parent\n  - [x] child\n";
    let doc_nested = crate::import::from_markdown(md_nested);
    assert_eq!(
        to_markdown(&doc_nested, &carried(&doc_nested)),
        "- [ ] parent\n  - [x] child\n\n"
    );
}

#[test]
fn list_marker_x_done_and_todo_are_labels() {
    // Unordered
    let doc_x = tomet_ast_parser::parse_file("- (X) uppercase x\n").unwrap();
    assert_eq!(
        to_markdown(&doc_x, &carried(&doc_x)),
        "- **X**: uppercase x\n\n"
    );

    let doc_done = tomet_ast_parser::parse_file("- (done) done label\n").unwrap();
    assert_eq!(
        to_markdown(&doc_done, &carried(&doc_done)),
        "- **done**: done label\n\n"
    );

    let doc_todo = tomet_ast_parser::parse_file("- (todo) todo label\n").unwrap();
    assert_eq!(
        to_markdown(&doc_todo, &carried(&doc_todo)),
        "- **todo**: todo label\n\n"
    );

    // Ordered
    let doc_x_ord = tomet_ast_parser::parse_file("-. (X) uppercase x\n").unwrap();
    assert_eq!(
        to_markdown(&doc_x_ord, &carried(&doc_x_ord)),
        "1. **X**: uppercase x\n\n"
    );

    let doc_done_ord = tomet_ast_parser::parse_file("-. (done) done label\n").unwrap();
    assert_eq!(
        to_markdown(&doc_done_ord, &carried(&doc_done_ord)),
        "1. **done**: done label\n\n"
    );

    let doc_todo_ord = tomet_ast_parser::parse_file("-. (todo) todo label\n").unwrap();
    assert_eq!(
        to_markdown(&doc_todo_ord, &carried(&doc_todo_ord)),
        "1. **todo**: todo label\n\n"
    );

    // x stays checked and X is the label **X**
    let doc_both = tomet_ast_parser::parse_file("- (x) checked\n- (X) uppercase\n").unwrap();
    assert_eq!(
        to_markdown(&doc_both, &carried(&doc_both)),
        "- [x] checked\n- **X**: uppercase\n\n"
    );

    let doc_both_ord = tomet_ast_parser::parse_file("-. (x) checked\n-. (X) uppercase\n").unwrap();
    assert_eq!(
        to_markdown(&doc_both_ord, &carried(&doc_both_ord)),
        "1. [x] checked\n2. **X**: uppercase\n\n"
    );
}

/// What a document read with no vault has in scope: the vocabularies tomet carries.
fn carried(doc: &tomet_ast::File) -> tomet_semantics::Bindings {
    tomet_semantics::LoadedVocabularies::default().bindings_for(doc)
}

/// [`to_markdown`] with what a vault-less caller binds.
fn export(doc: &File) -> String {
    to_markdown(doc, &carried(doc))
}
