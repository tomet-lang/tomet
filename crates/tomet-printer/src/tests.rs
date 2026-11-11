use super::*;
use crate::value::render_value_inner_with_config;
use tomet_ast::{
    Bare, Block, Data, Entry, File, Group, Inline, Name, Paragraph, Section, Text, Value,
};
use tomet_config::{FieldConfig, FormatConfig, MetaConfig, load_config_from_str};
use tomet_span::Span;
use tomet_style::{ContentLayout, GroupOrder, ParagraphSeparator};
use tomet_tree::element_new;

#[test]
fn test_printer_config_space_inside_brackets() {
    let doc = tomet_ast_parser::parse_file("=[Title]\n").unwrap();
    let cfg = PrinterConfig {
        format: FormatConfig {
            heading_brackets: Some(tomet_style::BracketSpacing::Padded),
            ..Default::default()
        },
        ..Default::default()
    };
    let printed = document_to_tm_with_config(&doc, &cfg);
    assert!(printed.contains("=[ Title ]"));
}

#[test]
fn nested_inline_heading_does_not_reserialize_as_hash_sugar() {
    // `@heading(2)[...]` nested inside another element's content must
    // round-trip as a plain `@heading(...)` element, never as `##[...]`
    // -- only `render_block`'s top-level dispatch special-cases
    // headings; the shared, recursively-called `render_element` has no
    // heading arm at all (mirrors `tomet-html`/
    // `tomet-markdown`'s equivalent nested-heading tests).
    let doc = tomet_ast_parser::parse_file("@memo[@heading(2)[Nested]]\n").unwrap();
    let printed = document_to_tm(&doc);
    assert!(!printed.contains("##["), "got: {printed:?}");
    assert!(printed.contains("@heading(2)[Nested]"), "got: {printed:?}");
}

#[test]
fn a_named_connect_round_trips_on_a_plain_element() {
    let doc = tomet_ast_parser::parse_file("@section[ x ]::rule(allow: (card))\n").unwrap();
    let printed = document_to_tm(&doc);
    assert!(printed.contains(":rule(allow: (card))"), "got: {printed:?}");
}

#[test]
fn stacked_connects_round_trip_in_order() {
    let doc = tomet_ast_parser::parse_file("@x(a: 1)::as(y)::rule(allow: (card))\n").unwrap();
    let printed = document_to_tm(&doc);
    assert!(
        printed.contains(":as(y)::rule(allow: (card))"),
        "got: {printed:?}"
    );
}

#[test]
fn a_named_connect_round_trips_on_a_heading() {
    let doc = tomet_ast_parser::parse_file("=[ h ]::rule(allow: (card))\n").unwrap();
    let printed = document_to_tm(&doc);
    assert!(printed.contains(":rule(allow: (card))"), "got: {printed:?}");
}

/// Defensive coverage for the early-returning special cases in
/// `render_element` (`hr`/`raw`/`callout`), which would otherwise
/// silently drop a `connects` field the parser never actually attaches
/// to them in practice today -- a `::rule(...)` after e.g. a raw block is
/// syntactically legal even if unlikely. (`::meta` is a declaration, which
/// takes no connect.)
#[test]
fn connects_survive_the_specially_printed_kinds() {
    for src in [
        "@hr()::rule(allow: (card))\n",
        "@raw[x]::rule(allow: (card))\n",
    ] {
        let doc = tomet_ast_parser::parse_file(src).unwrap();
        let el = match &doc.body[0] {
            Block::Element(el) => el,
            other => panic!("expected element, got {other:?}"),
        };
        assert_eq!(el.connects.len(), 1, "source: {src:?}, doc: {doc:?}");
        let printed = document_to_tm(&doc);
        assert!(
            printed.contains(":rule(allow: (card))"),
            "source: {src:?}, got: {printed:?}"
        );
    }
}

#[test]
fn connects_survive_a_specially_styled_callout() {
    let doc = tomet_ast_parser::parse_file("@callout(info)[x]::rule(allow: (card))\n").unwrap();
    let cfg = PrinterConfig {
        format: FormatConfig {
            callout_content_style: Some(ContentLayout::Block),
            ..Default::default()
        },
        ..Default::default()
    };
    let printed = document_to_tm_with_config(&doc, &cfg);
    assert!(printed.contains(":rule(allow: (card))"), "got: {printed:?}");
}

#[test]
fn test_iso8601_timestamp_rendering_in_meta() {
    let md = "---\nmodified: 2026-06-17T05:52:44\n---\n\n# Document\n";
    let doc = tomet_markdown::from_markdown(md);
    let cfg = PrinterConfig {
        meta: MetaConfig {
            field_layout: Some(tomet_style::GroupLayout::OnePerLine),
            ..Default::default()
        },
        ..Default::default()
    };
    let printed = document_to_tm_with_config(&doc, &cfg);
    assert!(printed.contains("modified: 2026-06-17T05:52:44"));
    assert!(!printed.contains("modified: \"2026-06-17T05:52:44\""));
}

#[test]
fn test_wikilink_rendering() {
    let md = "Check [[name]] and [[name|display]] here.\n";
    let doc = tomet_markdown::from_markdown(md);
    let printed = document_to_tm(&doc);
    assert!(printed.contains("@link(target: \"ref:name\")"));
    assert!(printed.contains("@link(target: \"ref:name\")[display]"));
}

#[test]
fn test_rfc3339_and_aliases_always_newline_meta_field_formatting() {
    let md = "---\naliases:\n  - rust\n  - tomet\ncreated: 2026-06-17T05:52:44\nmodified: 2026-06-17T05:52:44\n---\n\n# Document\n";
    let doc = tomet_markdown::from_markdown(md);

    let mut meta_fields = std::collections::BTreeMap::new();
    meta_fields.insert(
        "aliases".to_string(),
        FieldConfig {
            field_type: Some("list".to_string()),
            always_newline: true,
            ..Default::default()
        },
    );
    meta_fields.insert(
        "created".to_string(),
        FieldConfig {
            field_type: Some("datetime".to_string()),
            format: Some("rfc3339".to_string()),
            ..Default::default()
        },
    );
    meta_fields.insert(
        "modified".to_string(),
        FieldConfig {
            field_type: Some("datetime".to_string()),
            format: Some("rfc3339".to_string()),
            ..Default::default()
        },
    );

    let cfg = PrinterConfig {
        meta: MetaConfig {
            field_layout: Some(tomet_style::GroupLayout::OnePerLine),
            fields: meta_fields,
            ..Default::default()
        },
        ..Default::default()
    };

    let printed = document_to_tm_with_config(&doc, &cfg);
    assert!(printed.contains("created: 2026-06-17T05:52:44Z"));
    assert!(printed.contains("modified: 2026-06-17T05:52:44Z"));
    assert!(printed.contains("aliases:\n    - rust\n    - tomet"));

    let md_empty = "---\naliases: []\nflags: []\n---\n\n# Document\n";
    let doc_empty = tomet_markdown::from_markdown(md_empty);
    let printed_empty = document_to_tm_with_config(&doc_empty, &cfg);
    // "aliases" has its own `always_newline` field config, which
    // special-cases an empty list to `[]`. "flags" has no field config, so
    // it falls through to the native `(...)` seq spelling.
    assert!(printed_empty.contains("aliases: []"));
    assert!(printed_empty.contains("flags: ()"));
}

#[test]
fn test_rfc3339_with_offset_formatting() {
    let md = "---\ncreated: 2026-06-17T05:52:44\n---\n\n# Document\n";
    let doc = tomet_markdown::from_markdown(md);

    let mut meta_fields = std::collections::BTreeMap::new();
    meta_fields.insert(
        "created".to_string(),
        FieldConfig {
            field_type: Some("datetime".to_string()),
            format: Some("rfc3339".to_string()),
            offset: Some("+09:00".to_string()),
            ..Default::default()
        },
    );

    let cfg = PrinterConfig {
        meta: MetaConfig {
            field_layout: Some(tomet_style::GroupLayout::OnePerLine),
            fields: meta_fields,
            ..Default::default()
        },
        ..Default::default()
    };

    let printed = document_to_tm_with_config(&doc, &cfg);
    assert!(printed.contains("created: 2026-06-17T05:52:44+09:00"));
}

#[test]
fn test_null_value_rendering() {
    let rendered = render_value_inner_with_config(&Value::Null, &PrinterConfig::default());
    assert_eq!(rendered, "");
    let md = "---\ntitle:\n---\n\n# Document\n";
    let doc = tomet_markdown::from_markdown(md);
    let cfg = PrinterConfig {
        meta: MetaConfig {
            field_layout: Some(tomet_style::GroupLayout::OnePerLine),
            ..Default::default()
        },
        ..Default::default()
    };
    let printed = document_to_tm_with_config(&doc, &cfg);
    assert!(printed.contains("title:\n"));
    assert!(!printed.contains("title: null"));
    assert!(!printed.contains("title: \"\""));
}

#[test]
fn test_wikilink_no_space_setting() {
    let md = "Check [[target]] and [[target|display]].\n";
    let doc = tomet_markdown::from_markdown(md);

    let cfg_default = PrinterConfig::default();
    let printed_default = document_to_tm_with_config(&doc, &cfg_default);
    assert!(printed_default.contains("@link(target: \"ref:target\")"));
    assert!(printed_default.contains("@link(target: \"ref:target\")[display]"));

    let cfg_no_space = PrinterConfig {
        format: FormatConfig {
            link_target_colon: Some(tomet_style::ColonSpacing::Tight),
            ..Default::default()
        },
        ..Default::default()
    };
    let printed_no_space = document_to_tm_with_config(&doc, &cfg_no_space);
    assert!(printed_no_space.contains("@link(target:\"ref:target\")"));
    assert!(printed_no_space.contains("@link(target:\"ref:target\")[display]"));
}

#[test]
fn test_nanoid_generation_and_ensure_document_id() {
    let settings_src = r#"::field(
  id: {
    type: nanoid
    length: 8
    prefix: "doc-"
  }
)
"#;
    let cfg = load_config_from_str(settings_src).expect("failed to parse settings");
    let id_cfg = cfg.meta.fields.get("id").expect("id config present");
    assert_eq!(id_cfg.field_type.as_deref(), Some("nanoid"));
    assert_eq!(id_cfg.length, Some(8));
    assert_eq!(id_cfg.prefix.as_deref(), Some("doc-"));

    let mut doc = tomet_markdown::from_markdown("# Test Note\nHello world\n");
    ensure_document_id_with_config(&mut doc, &cfg);

    let printed = document_to_tm_with_config(&doc, &cfg);
    assert!(printed.contains("::id(doc-"), "{printed}");
    assert!(!printed.contains("::meta"), "{printed}");
}

#[test]
fn test_nanoid_force_and_overwrite_behavior() {
    let settings_src = r#"::field(
  id: {
    type: nanoid
    length: 8
    prefix: "doc-"
    force: true
    overwrite: true
  }
)
"#;
    let cfg = load_config_from_str(settings_src).expect("failed to parse settings");
    let id_cfg = cfg.meta.fields.get("id").unwrap();
    assert_eq!(id_cfg.force, Some(true));
    assert_eq!(id_cfg.overwrite, Some(true));

    // Case 1: Valid existing ID is kept
    let md1 = "---\nid: doc-12345678\n---\n# Note 1\n";
    let mut doc1 = tomet_markdown::from_markdown(md1);
    ensure_document_id_with_config(&mut doc1, &cfg);
    let printed1 = document_to_tm_with_config(&doc1, &cfg);
    assert!(printed1.contains("::id(doc-12345678)"), "{printed1}");

    // Case 2: Invalid format existing ID is overwritten when overwrite: true
    let md2 = "---\nid: invalid-slug\n---\n# Note 2\n";
    let mut doc2 = tomet_markdown::from_markdown(md2);
    ensure_document_id_with_config(&mut doc2, &cfg);
    let printed2 = document_to_tm_with_config(&doc2, &cfg);
    assert!(!printed2.contains("invalid-slug"));
    assert!(printed2.contains("::id(doc-"), "{printed2}");

    // Case 3: When force: true and overwrite: false, invalid existing ID is preserved
    let cfg_no_overwrite = PrinterConfig {
        meta: MetaConfig {
            fields: std::collections::BTreeMap::from([(
                "id".to_string(),
                FieldConfig {
                    field_type: Some("nanoid".to_string()),
                    length: Some(8),
                    prefix: Some("doc-".to_string()),
                    force: Some(true),
                    overwrite: Some(false),
                    ..Default::default()
                },
            )]),
            ..Default::default()
        },
        ..Default::default()
    };
    let mut doc3 = tomet_markdown::from_markdown(md2);
    ensure_document_id_with_config(&mut doc3, &cfg_no_overwrite);
    let printed3 = document_to_tm_with_config(&doc3, &cfg_no_overwrite);
    assert!(printed3.contains("::id(invalid-slug)"), "{printed3}");
}

/// The id a vault's `id` settings ask for: what they come to as a type, and nothing for what
/// is not a form of id.
#[test]
fn the_id_settings_come_to_a_type_that_is_a_form_of_id() {
    let printed = |field: FieldConfig| {
        let cfg = PrinterConfig {
            meta: MetaConfig {
                fields: std::collections::BTreeMap::from([("id".to_string(), field)]),
                ..Default::default()
            },
            ..Default::default()
        };
        let mut doc = tomet_markdown::from_markdown("# Note\n");
        ensure_document_id_with_config(&mut doc, &cfg);
        document_to_tm_with_config(&doc, &cfg)
    };
    let field = |field_type: &str, prefix: Option<&str>, length: Option<usize>| FieldConfig {
        field_type: Some(field_type.to_string()),
        prefix: prefix.map(String::from),
        length,
        ..Default::default()
    };

    // The length of the id a document was given, the prefix included.
    let id_len = |printed: &str| {
        let id = printed.split("::id(").nth(1)?;
        Some(id.split(')').next()?.len())
    };

    // A nanoid is as long as the vault says and 8 when it does not.
    assert_eq!(
        id_len(&printed(field("nanoid", Some("n-"), Some(12)))),
        Some(2 + 12)
    );
    assert_eq!(id_len(&printed(field("nanoid", None, None))), Some(8));

    // A uuid has no length, and a length the vault gives for it is not asked.
    assert_eq!(
        id_len(&printed(field("uuid", Some("u-"), Some(3)))),
        Some(2 + 36)
    );

    // No type, a type that is no form of id, a name that is no type, or a prefix the type does
    // not take: nothing is made.
    for none in [
        FieldConfig::default(),
        field("string", None, None),
        field("int", Some("x"), None),
        field("person", None, None),
    ] {
        assert!(!printed(none.clone()).contains("::id("), "{none:?}");
    }
}

#[test]
fn test_obsidian_callout_blockquote_printing() {
    let md = "> [!info] 2025/04/29 11:09\n> コレさすがに草www\n";
    let doc = tomet_markdown::from_markdown(md);
    let printed = document_to_tm_with_config(&doc, &PrinterConfig::default());
    assert!(printed.contains("@callout(info, title: \"2025/04/29 11:09\")["));
    assert!(printed.contains("コレさすがに草www"));

    let md_plain = "> Plain quote text\n";
    let doc_plain = tomet_markdown::from_markdown(md_plain);
    let printed_plain = document_to_tm_with_config(&doc_plain, &PrinterConfig::default());
    assert!(printed_plain.contains("@quote["));
    assert!(printed_plain.contains("Plain quote text"));
    assert!(!printed_plain.contains("@quote("));
}

#[test]
fn test_callout_content_style_formatting() {
    let md = "> [!info] 2025/04/29 11:09\n> コレさすがに草www\n> お前なら[[2025-04-26|どうするんだ]]？\n";
    let doc = tomet_markdown::from_markdown(md);

    // Test "block" style
    let cfg_block = PrinterConfig {
        format: FormatConfig {
            callout_content_style: Some(ContentLayout::Block),
            ..Default::default()
        },
        ..Default::default()
    };
    let printed_block = document_to_tm_with_config(&doc, &cfg_block);
    assert!(printed_block.contains("@callout(info, title: \"2025/04/29 11:09\")\n[ コレさすがに草www\n  お前なら@link(target: \"ref:2025-04-26\")[どうするんだ]？\n]"));

    // Test "box" style
    let cfg_box = PrinterConfig {
        format: FormatConfig {
            callout_content_style: Some(ContentLayout::Box),
            ..Default::default()
        },
        ..Default::default()
    };
    let printed_box = document_to_tm_with_config(&doc, &cfg_box);
    assert!(printed_box.contains("@callout(info, title: \"2025/04/29 11:09\")\n[ コレさすがに草www\n  お前なら@link(target: \"ref:2025-04-26\")[どうするんだ]？ ]"));

    // Test "expanded" style
    let cfg_expanded = PrinterConfig {
        format: FormatConfig {
            callout_content_style: Some(ContentLayout::Expanded),
            ..Default::default()
        },
        ..Default::default()
    };
    let printed_expanded = document_to_tm_with_config(&doc, &cfg_expanded);
    assert!(printed_expanded.contains("@callout(info, title: \"2025/04/29 11:09\")[\n  コレさすがに草www\n  お前なら@link(target: \"ref:2025-04-26\")[どうするんだ]？\n]"));
}

#[test]
fn test_list_multiline_style_formatting() {
    let md = "1. いや、まずこういう話をするときの前提として、\n   Vtuberでくくってるやつがまず、ゴミだ。\n   確実に脳が言っている割合が高い。イメージだけで物事を語る。それってあなたの間奏ですよね。\n";
    let doc = tomet_markdown::from_markdown(md);

    let cfg_box = PrinterConfig {
        format: FormatConfig {
            list_multiline_style_content: Some(ContentLayout::Box),
            ..Default::default()
        },
        ..Default::default()
    };
    let printed_box = document_to_tm_with_config(&doc, &cfg_box);
    assert!(printed_box.contains("-. [ いや、まずこういう話をするときの前提として、\n     Vtuberでくくってるやつがまず、ゴミだ。\n     確実に脳が言っている割合が高い。イメージだけで物事を語る。それってあなたの間奏ですよね。 ]"));

    let cfg_block = PrinterConfig {
        format: FormatConfig {
            list_multiline_style_content: Some(ContentLayout::Block),
            ..Default::default()
        },
        ..Default::default()
    };
    let printed_block = document_to_tm_with_config(&doc, &cfg_block);
    assert!(printed_block.contains("-. [ いや、まずこういう話をするときの前提として、\n     Vtuberでくくってるやつがまず、ゴミだ。\n     確実に脳が言っている割合が高い。イメージだけで物事を語る。それってあなたの間奏ですよね。\n     ]"));
}

#[test]
fn test_single_line_block_style_formatting() {
    let md_callout = "> [!info] Single Line\n> 一行テキスト\n";
    let doc_callout = tomet_markdown::from_markdown(md_callout);
    let cfg = PrinterConfig {
        format: FormatConfig {
            callout_content_style: Some(ContentLayout::Block),
            ..Default::default()
        },
        ..Default::default()
    };
    let printed = document_to_tm_with_config(&doc_callout, &cfg);
    assert!(printed.contains("@callout(info, title: \"Single Line\")\n[ 一行テキスト ]"));
}

#[test]
fn test_link_no_space_setting() {
    let md = "[Google](https://google.com)\n";
    let doc = tomet_markdown::from_markdown(md);

    let cfg_default = PrinterConfig::default();
    let printed_default = document_to_tm_with_config(&doc, &cfg_default);
    assert!(printed_default.contains("@link(target: \"https://google.com\")[Google]"));

    let cfg_nospace = PrinterConfig {
        format: FormatConfig {
            link_target_colon: Some(tomet_style::ColonSpacing::Tight),
            ..Default::default()
        },
        ..Default::default()
    };
    let printed_nospace = document_to_tm_with_config(&doc, &cfg_nospace);
    assert!(printed_nospace.contains("@link(target:\"https://google.com\")[Google]"));
}

#[test]
fn inline_raw_prints_as_backticks() {
    let doc = tomet_ast_parser::parse_file("call `foo()` now\n").unwrap();
    let printed = document_to_tm(&doc);
    assert_eq!(printed.trim(), "call `foo()` now");
}

#[test]
fn test_codeblock_formatting() {
    let md = "```shell\nirm \"https://christitus.com/win\" | iex\n```\n";
    let doc = tomet_markdown::from_markdown(md);
    let printed = document_to_tm(&doc);
    assert_eq!(
        printed.trim(),
        "```shell\nirm \"https://christitus.com/win\" | iex\n```"
    );
}

#[test]
fn test_container_children_indentation() {
    let note = |marker: i64, text: &str| {
        Entry::Bare(Bare {
            data: Data {
                args: Some(Value::Int(marker)),
                value: None,
            },
            content: Some(vec![Block::Paragraph(Paragraph::new(
                vec![Inline::Text(text.into())],
                Span::dummy(),
            ))]),
            id: None,
            span: Span::dummy(),
        })
    };

    let mut links = element_new(Name::bare("links"));
    links.data.value = Some(Group(vec![note(1, "note 1"), note(2, "note 2")]));

    let doc = File::new(Vec::new(), vec![Block::Element(links)], Span::dummy());
    let printed = document_to_tm(&doc);
    assert!(printed.contains("@links(){\n  (1)[note 1]\n  (2)[note 2]\n}"));

    // Verify re-parsing
    let re_parsed = tomet_ast_parser::parse_file(&printed).expect("valid doc");
    assert_eq!(re_parsed.body.len(), 1);
}

#[test]
fn test_group_order_content_first() {
    let mut el = element_new(Name::bare("link"));
    el.data.args = Some(Value::String("https://example.com".to_string()));
    el.content = Some(vec![Block::Paragraph(Paragraph::new(
        vec![Inline::Text("Example".into())],
        Span::dummy(),
    ))]);

    let doc = File::new(Vec::new(), vec![Block::Element(el)], Span::dummy());

    let mut config = PrinterConfig {
        format: tomet_config::FormatConfig {
            group_order: Some(GroupOrder::ContentFirst),
            ..Default::default()
        },
        ..Default::default()
    };
    let printed = document_to_tm_with_config(&doc, &config);
    assert_eq!(printed.trim(), "@link[Example](\"https://example.com\")");

    config.format.group_order = Some(GroupOrder::ArgsFirst);
    let printed_args_first = document_to_tm_with_config(&doc, &config);
    assert_eq!(
        printed_args_first.trim(),
        "@link(\"https://example.com\")[Example]"
    );

    let config_link_only = PrinterConfig {
        format: tomet_config::FormatConfig {
            link_group_order: Some(GroupOrder::ContentFirst),
            ..Default::default()
        },
        ..Default::default()
    };
    let printed_link_only = document_to_tm_with_config(&doc, &config_link_only);
    assert_eq!(
        printed_link_only.trim(),
        "@link[Example](\"https://example.com\")"
    );
}

#[test]
fn test_tag_printing() {
    let doc = tomet_ast_parser::parse_file("@tag(rust, tomet)\n").expect("valid doc");
    let printed = document_to_tm(&doc);
    assert_eq!(printed.trim(), "@tag(rust, tomet)");
}

#[test]
fn test_id_printing_is_tight_on_a_plain_element_but_spaced_on_a_section() {
    // A plain element's own groups all attach tight to each other
    // (no spaces between `(args)[content]{value}`), and `#(id)`
    // matches that -- but a section already puts a space before its
    // own trailing `{value}` for readability, and `#(id)` matches
    // *that* convention there instead, for the same reason.
    let doc = tomet_ast_parser::parse_file("@memo(a: 1)#(myid)\n").expect("valid doc");
    assert_eq!(document_to_tm(&doc).trim(), "@memo(a: 1)#(myid)");

    let doc = tomet_ast_parser::parse_file("=[ Title ]#(intro)\n").expect("valid doc");
    assert_eq!(document_to_tm(&doc).trim(), "=[Title] #(intro)");
}

#[test]
fn test_old_hash_tag_sugar_prints_as_plain_text() {
    let doc = tomet_ast_parser::parse_file("#(rust, tomet)\n").expect("valid doc");
    let printed = document_to_tm(&doc);
    assert_eq!(printed.trim(), "#(rust, tomet)");
}

#[test]
fn test_section_blocks_blank_lines() {
    let sec = Section {
        level: 1,

        title: vec![Inline::Text("Title".into())],

        data: Data {
            args: None,
            value: None,
        },

        id: None,

        connects: Vec::new(),

        blocks: vec![
            Block::Paragraph(tomet_ast::Paragraph::new(
                vec![Inline::Text("Paragraph 1".into())],
                Span::dummy(),
            )),
            Block::Paragraph(tomet_ast::Paragraph::new(
                vec![Inline::Text("Paragraph 2".into())],
                Span::dummy(),
            )),
        ],

        span: Span::dummy(),
    };
    let doc = File::new(Vec::new(), vec![Block::Section(sec)], Span::dummy());
    let printed = document_to_tm(&doc);
    assert_eq!(printed, "=[Title]\nParagraph 1\n\nParagraph 2\n");
}

#[test]
fn test_headingless_section_printing() {
    let sec = Section {
        level: 1,

        title: Vec::new(),

        data: Data {
            args: None,
            value: None,
        },

        id: None,

        connects: Vec::new(),

        blocks: vec![Block::Paragraph(tomet_ast::Paragraph::new(
            vec![Inline::Text("Content".into())],
            Span::dummy(),
        ))],

        span: Span::dummy(),
    };
    let doc = File::new(Vec::new(), vec![Block::Section(sec)], Span::dummy());
    let printed = document_to_tm(&doc);
    assert_eq!(printed, "=\nContent\n");
}

/// Formatting must not change what a document says: `1.10` stays `1.10`, it
/// does not become the `1.1` an `f64` prints.
#[test]
fn a_number_reading_would_change_is_printed_as_written() {
    let src = "::meta(rev: 1.10, ratio: 2.5, big: 12345678901234567890, zero: -0)\n\n@x(1.10)\n";
    let doc = tomet_ast_parser::parse_file(src).unwrap();
    let printed = document_to_tm_with_config(&doc, &PrinterConfig::default());
    for kept in [
        "rev: 1.10",
        "ratio: 2.5",
        "big: 12345678901234567890",
        "zero: -0",
        "@x(1.10)",
    ] {
        assert!(printed.contains(kept), "{kept} in {printed}");
    }
    let again = tomet_ast_parser::parse_file(&printed).unwrap();
    assert_eq!(
        document_to_tm_with_config(&again, &PrinterConfig::default()),
        printed
    );
}

#[test]
fn a_semicolon_that_is_text_at_the_start_of_a_line_is_printed_with_a_backslash() {
    // A bare `;` at the start of a line would break the block when read back,
    // so a text `;` there is written `\;` -- after a soft break and at the
    // start of a paragraph alike. In the middle of a line it needs nothing.
    for src in ["a\n\\;b\n", "\\;a\n", "a;b\n"] {
        let doc = tomet_ast_parser::parse_file(src).unwrap();
        let printed = document_to_tm(&doc);
        assert_eq!(printed, src, "{src:?}");
        let again = tomet_ast_parser::parse_file(&printed).unwrap();
        assert_eq!(again.body.len(), doc.body.len(), "{src:?}");
    }
}

#[test]
fn escaped_line_start_constructs_round_trip() {
    for src in [
        "\\;a\n",
        "a\n\\;b\n",
        "\\= Heading\n",
        "\\==[Title]\n",
        "\\---\n",
        "\\- item\n",
        "\\-. ordered\n",
        "\\``not line raw\n",
        "\\```not fence\n",
        "\\::meta(a: 1)\n",
        "\\@standalone\n",
        "first line\n\\= second line\n\\- third line\n\\---\n",
    ] {
        let before = tomet_ast_parser::parse_file(src)
            .unwrap_or_else(|e| panic!("{src:?} failed to parse: {e}"));
        let printed = document_to_tm(&before);
        let after = tomet_ast_parser::parse_file(&printed).unwrap_or_else(|e| {
            panic!("{src:?} printed as {printed:?}, which failed to parse: {e}")
        });
        assert_eq!(
            before, after,
            "{src:?} printed as {printed:?} and read back differently"
        );
    }
}

#[test]
fn escaped_inline_constructs_round_trip() {
    for src in [
        "\\*em*\n",
        "*em\\*\n",
        "\\**strong**\n",
        "**strong\\**\n",
        "\\_em_\n",
        "\\__strong__\n",
        "\\~~strike~~\n",
        "use \\`code\\` literally\n",
        "text \\// not a comment\n",
        "text \\/* not a block comment */\n",
        "call \\@name(x) and \\@tag[y]{z: 1} here\n",
        "value \\$(x) and \\$fn(1)\n",
        "hole \\<?slot> and \\<?(n)>\n",
        "seat \\<$date> and \\<$list(string, length: 6)> and \\<$5>\n",
        "@note[a \\[ b \\] c]\n",
        "literal \\\\* and \\\\\n",
        "unmarked <PATH> and snake_case_ident and me@example.com stay plain\n",
    ] {
        let before = tomet_ast_parser::parse_file(src)
            .unwrap_or_else(|e| panic!("{src:?} failed to parse: {e}"));
        let printed = document_to_tm(&before);
        let after = tomet_ast_parser::parse_file(&printed).unwrap_or_else(|e| {
            panic!("{src:?} printed as {printed:?}, which failed to parse: {e}")
        });
        assert_eq!(
            before, after,
            "{src:?} printed as {printed:?} and read back differently"
        );
    }
}

#[test]
fn text_shaped_like_a_type_seat_is_printed_escaped() {
    // Plain text, as an AST that was never source can hold it, is printed so that it reads back as
    // the same text, and not as a seat.
    for text in [
        "<$date>",
        "<$list(string, length: 6)>",
        // The `$` of an interpolation after the `<` is escaped on its own.
        "<$f(x)>",
        "a <$uuid> b",
        "<$5>",
    ] {
        let doc = File::new(
            Vec::new(),
            vec![Block::Paragraph(Paragraph::new(
                vec![Inline::Text(Text::new(text, Span::dummy()))],
                Span::dummy(),
            ))],
            Span::dummy(),
        );
        let printed = document_to_tm(&doc);
        assert!(
            printed.contains("\\<"),
            "{text:?} is printed as {printed:?}"
        );
        let after = tomet_ast_parser::parse_file(&printed).unwrap_or_else(|e| {
            panic!("{text:?} printed as {printed:?}, which failed to parse: {e}")
        });
        let [Block::Paragraph(p)] = after.body.as_slice() else {
            panic!(
                "{text:?} printed as {printed:?}, which reads as {:?}",
                after.body
            );
        };
        assert!(
            matches!(p.content.as_slice(), [Inline::Text(t)] if t.value == text),
            "{text:?} printed as {printed:?} and read back as {:?}",
            p.content
        );
    }
}

#[test]
fn type_seats_round_trip() {
    for src in [
        // A block of its own.
        "<$uuid>\n",
        "<$uuid()>\n",
        "<$date(default: auto)>\n",
        // In running text.
        "id: <$uuid> and <$list(string, length: 6)> end\n",
        "<$uuid> starts the line\n",
        // After a seat the text is text: nothing attaches to it.
        "<$x>[t] and <$x>(a) and <$x>{b: 1}\n",
        // As a value.
        "@card(created: <$datetime(default: auto)>, n: <$nanoid(length: 8, prefix: \"note-\")>)\n",
        "@card(<$date>)\n",
        "@card((<$int>, <$string>)){ tags: <$list(string)> }\n",
        "::meta(id: <$uuid(default: auto)>)\n",
        // In a heading, a list and the content of an element.
        "=[ <$string> ]\n",
        "== Name: <$string>\n",
        "- <$int>\n- name <$string>\n- <$list(string, length: 2)> {a: 1}\n",
        "== <$nanoid(length: 8)>\n",
        "=[ <$x(a: 1)> ]#(id)\n",
        "@card[\n<$string>\n\nmore <$int>\n]\n",
        // A `>` inside the arguments.
        "<$nanoid(prefix: \">\")>\n",
    ] {
        let before = tomet_ast_parser::parse_file(src)
            .unwrap_or_else(|e| panic!("{src:?} failed to parse: {e}"));
        let printed = document_to_tm(&before);
        let after = tomet_ast_parser::parse_file(&printed).unwrap_or_else(|e| {
            panic!("{src:?} printed as {printed:?}, which failed to parse: {e}")
        });
        assert_eq!(
            before, after,
            "{src:?} printed as {printed:?} and read back differently"
        );
        // Printed twice, the same.
        assert_eq!(document_to_tm(&after), printed, "{src:?}");
    }
}

#[test]
fn a_string_shaped_like_a_type_seat_is_printed_quoted() {
    for src in [
        "@a(\"<$date>\")\n",
        "@a(k: \"x <$list(int)> y\")\n",
        "::meta(t: \"<$uuid()>\")\n",
    ] {
        let before = tomet_ast_parser::parse_file(src).unwrap();
        let printed = document_to_tm(&before);
        assert_eq!(printed, src, "{src:?}");
        assert_eq!(tomet_ast_parser::parse_file(&printed).unwrap(), before);
    }
}

#[test]
fn a_name_and_a_call_with_no_arguments_stay_two_forms_when_printed() {
    assert_eq!(
        document_to_tm(&tomet_ast_parser::parse_file("<$uuid>").unwrap()),
        "<$uuid>\n"
    );
    assert_eq!(
        document_to_tm(&tomet_ast_parser::parse_file("<$uuid()>").unwrap()),
        "<$uuid()>\n"
    );
}

#[test]
fn inline_raw_with_backtick_falls_back_to_at_raw() {
    let md = "Use `` `code` `` here.\n";
    let doc = tomet_markdown::from_markdown(md);
    let printed = document_to_tm(&doc);
    assert_eq!(printed.trim(), "Use @raw[\\`code`] here.");
    let reparsed = tomet_ast_parser::parse_file(&printed).expect("reparses cleanly");
    assert_eq!(document_to_tm(&reparsed), printed);
}

#[test]
fn a_block_value_in_a_group_keeps_every_block_it_holds() {
    // `::macro(name: [ ... ])` is a `{...}` group, whose `key: [...]` values are printed with
    // every block they hold, not as paragraph inlines only (an element in one would come out as `[]`).
    for src in [
        "::macro(card: [ @card(title: $(1))[ $(2) ] ])\n",
        "::macro(two: [\none $(1)\n\n@card[ x ]\n])\n",
        "::macro(note: [ Note: $(msg) ])\n",
    ] {
        let before = tomet_ast_parser::parse_file(src).unwrap();
        let printed = document_to_tm(&before);
        assert!(!printed.contains("[]"), "{src:?} printed as {printed:?}");
        let after = tomet_ast_parser::parse_file(&printed).expect("the print reparses");
        assert_eq!(before, after, "{src:?} printed as {printed:?}");
        assert_eq!(
            document_to_tm(&after),
            printed,
            "{src:?}: printing is not stable"
        );
    }
}

// --- a nexus is always printed on a primary group --------------------------

/// An element with neither `(args)` nor `[content]` is printed `@name()`: bare, or with only its
/// `{value}`, `#(id)` or a `::connect`, it would not read back.
#[test]
fn an_element_without_a_primary_group_is_printed_with_an_empty_one() {
    let value = Group::from_map(Value::Map(vec![("a".to_string(), Value::Int(1))]));

    let bare = element_new(Name::bare("memo"));
    let mut with_value = element_new(Name::bare("memo"));
    with_value.data.value = Some(value.clone());
    let mut with_id = element_new(Name::bare("memo"));
    with_id.id = Some(tomet_ast::Id("x".into()));
    let mut with_connect = element_new(Name::bare("memo"));
    with_connect.connects.push(tomet_ast::Connect {
        name: Name::bare("rule"),
        data: Data {
            args: Some(Value::Map(vec![("k".to_string(), Value::Int(1))])),
            value: None,
        },
        content: None,
        span: Span::dummy(),
    });

    for (el, expected) in [
        (bare, "@memo()"),
        (with_value, "@memo(){ a: 1 }"),
        (with_id, "@memo()#(x)"),
        (with_connect, "@memo()::rule(k: 1)"),
    ] {
        let doc = File::new(Vec::new(), vec![Block::Element(el)], Span::dummy());
        let printed = document_to_tm(&doc);
        assert_eq!(printed.trim(), expected);
        let again = tomet_ast_parser::parse_file(&printed).expect(expected);
        assert!(matches!(&again.body[0], Block::Element(_)), "{expected}");
    }
}

#[test]
fn an_element_that_has_a_primary_group_gets_no_extra_one() {
    for src in ["@memo(a: 1)\n", "@memo[x]\n", "@memo(a: 1){ b: 2 }#(i)\n"] {
        let doc = tomet_ast_parser::parse_file(src).unwrap();
        assert_eq!(document_to_tm(&doc), src);
    }
}

#[test]
fn a_named_connect_is_printed_on_a_primary_group() {
    let mut el = element_new(Name::bare("memo"));
    el.data.args = Some(Value::Map(vec![("a".to_string(), Value::Int(1))]));
    el.connects.push(tomet_ast::Connect {
        name: Name::bare("rule"),
        data: Data {
            args: None,
            value: Some(Group::from_map(Value::Map(vec![(
                "k".to_string(),
                Value::Int(1),
            )]))),
        },
        content: None,
        span: Span::dummy(),
    });
    let doc = File::new(Vec::new(), vec![Block::Element(el)], Span::dummy());
    let printed = document_to_tm(&doc);
    assert_eq!(printed.trim(), "@memo(a: 1)::rule(){ k: 1 }");
    let again = tomet_ast_parser::parse_file(&printed).unwrap();
    let Block::Element(back) = &again.body[0] else {
        panic!("expected an element")
    };
    assert_eq!(back.connects.len(), 1);
}

#[test]
fn a_callout_without_a_primary_group_is_printed_with_an_empty_one() {
    for style in ["expanded", "block", "box"] {
        let cfg = PrinterConfig {
            format: FormatConfig {
                callout_content_style: ContentLayout::from_name(style),
                ..Default::default()
            },
            ..Default::default()
        };
        let doc = File::new(
            Vec::new(),
            vec![Block::Element(element_new(Name::bare("callout")))],
            Span::dummy(),
        );
        let printed = document_to_tm_with_config(&doc, &cfg);
        assert_eq!(printed.trim(), "@callout()", "{style}");
        assert!(tomet_ast_parser::parse_file(&printed).is_ok(), "{style}");
    }
}

#[test]
fn a_declaration_without_a_primary_group_is_printed_with_an_empty_one() {
    let decl = tomet_ast::Declaration {
        name: Name::bare("thing"),
        data: Data::default(),
        content: None,
        id: None,
        span: Span::dummy(),
    };
    let doc = File::new(vec![decl], Vec::new(), Span::dummy());
    let printed = document_to_tm(&doc);
    assert_eq!(printed.trim(), "::thing()");
    assert_eq!(
        tomet_ast_parser::parse_file(&printed).unwrap().header.len(),
        1
    );
}

/// `@hr` has its own spelling, `---`; with a group it has none, and is a nexus like any other.
#[test]
fn a_thematic_break_is_printed_as_dashes_and_never_as_a_bare_hr() {
    for src in ["---\n", "---[ title ]---\n", "@hr()\n"] {
        let doc = tomet_ast_parser::parse_file(src).unwrap();
        let printed = document_to_tm(&doc);
        assert!(
            !printed.contains("@hr\n") && !printed.trim_end().ends_with("@hr"),
            "{printed:?}"
        );
        let again = tomet_ast_parser::parse_file(&printed).unwrap();
        assert!(matches!(&again.body[0], Block::Element(el) if el.name == Name::bare("hr")));
    }
    let mut hr = element_new(Name::bare("hr"));
    hr.data.value = Some(Group::from_map(Value::Map(vec![(
        "a".to_string(),
        Value::Int(1),
    )])));
    let doc = File::new(Vec::new(), vec![Block::Element(hr)], Span::dummy());
    assert_eq!(document_to_tm(&doc).trim(), "@hr(){ a: 1 }");
}

/// A nexus with a primary group and a pipe run, a nameless connect, and the sugar kept for the
/// forms that are not nexuses all print back as they were read.
#[test]
fn the_forms_that_stay_valid_round_trip() {
    for src in [
        "@table\n| a | b\n",
        "@el(a: 1)::{b: 2}\n",
        "- item ::{a: 1}\n",
        "= heading {a: 1}\n",
        "- #(id)\n",
        "==#(id)\n",
    ] {
        let doc = tomet_ast_parser::parse_file(src).unwrap_or_else(|e| panic!("{src:?}: {e}"));
        let printed = document_to_tm(&doc);
        let again = tomet_ast_parser::parse_file(&printed)
            .unwrap_or_else(|e| panic!("{src:?} printed as {printed:?}: {e}"));
        assert_eq!(document_to_tm(&again), printed, "{src:?}");
    }
}

// --- declarations are printed on their parentheses -----------------------------

/// The twelve standard declarations that hold their content in `(...)` are printed as
/// `::name(...)`, in the same shape they were read, and the print reads back to the same tree.
#[test]
fn a_declaration_of_entries_is_printed_in_parentheses_and_reads_back() {
    for src in [
        "::meta(title: Hello, draft: true)\n",
        "::meta()\n",
        "::macro(gh: \"https://x/$(1)\")\n",
        "::macro()\n",
        "::var(n: {type: int, default: 3})\n",
        "::format(link: {no_space: true})\n",
        "::field(tags: {type: list(string)})\n",
        "::workspace(ignore: (a, b))\n",
        "::export(type: html)\n",
        "::blueprint(daily: @file(b.tmt))\n",
        "::vocabulary(deck: @file(deck.tmt))\n",
        "::dictionary(tomet: @file(dict.tmt))\n",
        "::migration(source: obsidian)\n",
        "::element(card: {}, ref: {})\n",
    ] {
        let before = tomet_ast_parser::parse_file(src).unwrap();
        let printed = document_to_tm(&before);
        assert_eq!(printed, src, "{src:?} is printed as written");
        let after = tomet_ast_parser::parse_file(&printed).expect("the print reparses");
        assert_eq!(before.header[0].data, after.header[0].data, "{src:?}");
    }
}

/// A declaration keeps a `{...}` body after its parentheses; the printer does not know which ones mean it.
#[test]
fn a_declaration_with_a_body_keeps_its_braces() {
    for src in [
        "::vocabulary(deck){ version: 1.0.0, open: true }\n",
        "::blueprint(note){ description: \"A note\" }\n",
    ] {
        let before = tomet_ast_parser::parse_file(src).unwrap();
        assert!(before.header[0].data.value.is_some(), "{src:?}");
        let printed = document_to_tm(&before);
        assert_eq!(printed, src, "{src:?} is printed as written");
        assert_eq!(tomet_ast_parser::parse_file(&printed).unwrap(), before);
    }
}

/// A declaration built with no group at all is still printed so that it reads back: `::meta()`.
#[test]
fn a_declaration_without_a_group_is_printed_with_an_empty_one() {
    let doc = File::new(
        vec![tomet_tree::declaration_new("macro")],
        Vec::new(),
        Span::dummy(),
    );
    assert_eq!(document_to_tm(&doc), "::macro()\n");
}

fn printed_with_separator(src: &str, separator: Option<ParagraphSeparator>) -> String {
    let doc = tomet_ast_parser::parse_file(src).unwrap();
    let cfg = PrinterConfig {
        format: FormatConfig {
            paragraph_separator: separator,
            ..Default::default()
        },
        ..Default::default()
    };
    let printed = document_to_tm_with_config(&doc, &cfg);
    // Whichever way the paragraphs are set off, they read back as the same document.
    assert_eq!(
        tomet_ast_parser::parse_file(&printed).unwrap(),
        doc,
        "printed: {printed:?}"
    );
    printed
}

#[test]
fn paragraphs_are_set_off_by_a_blank_line_unless_a_semicolon_is_asked_for() {
    let src = "a\n\nb\n\nc\n";
    assert_eq!(printed_with_separator(src, None), "a\n\nb\n\nc\n");
    assert_eq!(
        printed_with_separator(src, Some(ParagraphSeparator::Blank)),
        "a\n\nb\n\nc\n"
    );
    assert_eq!(
        printed_with_separator(src, Some(ParagraphSeparator::Semicolon)),
        "a\n;b\n;c\n"
    );
}

#[test]
fn a_semicolon_also_sets_off_the_elements_that_stand_like_a_paragraph() {
    let src = "a\n\n@file(x)\n\n@file(y)\n";
    assert_eq!(
        printed_with_separator(src, Some(ParagraphSeparator::Semicolon)),
        "a\n;@file(x)\n;@file(y)\n"
    );
}

#[test]
fn a_semicolon_inside_a_section_sets_off_its_paragraphs_but_not_the_heading() {
    let printed = printed_with_separator(
        "=[ t ]\n\na\n\nb\n\n=[ u ]\n\nc\n",
        Some(ParagraphSeparator::Semicolon),
    );
    assert!(printed.contains("a\n;b\n"), "got: {printed:?}");
    assert!(!printed.contains(";=["), "got: {printed:?}");
    assert!(!printed.contains(";c"), "got: {printed:?}");
}

#[test]
fn a_list_keeps_its_blank_line_when_a_semicolon_is_asked_for() {
    let src = "a\n\n- x\n\nb\n";
    assert_eq!(
        printed_with_separator(src, Some(ParagraphSeparator::Semicolon)),
        printed_with_separator(src, None)
    );
}

#[test]
fn a_paragraph_that_starts_with_a_semicolon_is_still_escaped_after_one() {
    let printed = printed_with_separator("a\n\n\\;b\n", Some(ParagraphSeparator::Semicolon));
    assert_eq!(printed, "a\n;\\;b\n");
}
