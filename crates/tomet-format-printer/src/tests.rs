use super::*;
use tomet_ast::{Block, Document, ElementValue, Inline, Paragraph, Section, Sigil, Value};
use tomet_config::{FieldConfig, FormatConfig, GroupOrder, MetaConfig, load_config_from_str};
use tomet_field_utils::generate_id_for_field;
use tomet_style::render_value_inner;
use tomet_tree::element_new;

#[test]
fn test_printer_config_space_inside_brackets() {
    let doc = tomet_parser::parse_document("=[Title]\n").unwrap();
    let cfg = PrinterConfig {
        format: FormatConfig {
            heading_space_inside_brackets: true,
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
    let doc = tomet_parser::parse_document("@memo[@heading(2)[Nested]]\n").unwrap();
    let printed = document_to_tm(&doc);
    assert!(!printed.contains("##["), "got: {printed:?}");
    assert!(printed.contains("@heading(2)[Nested]"), "got: {printed:?}");
}

#[test]
fn a_named_connect_round_trips_on_a_plain_element() {
    let doc = tomet_parser::parse_document("@section[ x ]:rule(allow: list(card))\n").unwrap();
    let printed = document_to_tm(&doc);
    assert!(
        printed.contains(":rule(allow: list(card))"),
        "got: {printed:?}"
    );
}

#[test]
fn stacked_connects_round_trip_in_order() {
    let doc = tomet_parser::parse_document("@x(a: 1):as(y):rule(allow: list(card))\n").unwrap();
    let printed = document_to_tm(&doc);
    assert!(
        printed.contains(":as(y):rule(allow: list(card))"),
        "got: {printed:?}"
    );
}

#[test]
fn a_named_connect_round_trips_on_a_heading() {
    let doc = tomet_parser::parse_document("=[ h ]:rule(allow: list(card))\n").unwrap();
    let printed = document_to_tm(&doc);
    assert!(
        printed.contains(":rule(allow: list(card))"),
        "got: {printed:?}"
    );
}

/// Defensive coverage for the four early-returning special cases in
/// `render_element` (`hr`/`meta`/`codeblock`/`callout`), which would
/// otherwise silently drop a `connects` field the parser never
/// actually attaches to them in practice today -- a `:rule(...)`
/// after e.g. a codeblock is syntactically legal even if unlikely.
#[test]
fn connects_survive_the_four_specially_printed_kinds() {
    for src in [
        "@hr:rule(allow: list(card))\n",
        "@meta{a: 1}:rule(allow: list(card))\n",
        "@raw[x]:rule(allow: list(card))\n",
    ] {
        let doc = tomet_parser::parse_document(src).unwrap();
        let el = match &doc.blocks[0] {
            Block::Element(el) => el,
            other => panic!("expected element, got {other:?}"),
        };
        assert_eq!(el.connects.len(), 1, "source: {src:?}, doc: {doc:?}");
        let printed = document_to_tm(&doc);
        assert!(
            printed.contains(":rule(allow: list(card))"),
            "source: {src:?}, got: {printed:?}"
        );
    }
}

#[test]
fn connects_survive_a_specially_styled_callout() {
    let doc = tomet_parser::parse_document("@callout(info)[x]:rule(allow: list(card))\n").unwrap();
    let cfg = PrinterConfig {
        format: FormatConfig {
            callout_content_style: Some("block".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };
    let printed = document_to_tm_with_config(&doc, &cfg);
    assert!(
        printed.contains(":rule(allow: list(card))"),
        "got: {printed:?}"
    );
}

#[test]
fn test_iso8601_timestamp_rendering_in_meta() {
    let md = "---\nmodified: 2026-06-17T05:52:44\n---\n\n# Document\n";
    let doc = tomet_markdown::from_markdown(md);
    let cfg = PrinterConfig {
        meta: MetaConfig {
            always_newline: true,
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
            always_newline: true,
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
    // it falls through to the native `list(...)` seq spelling.
    assert!(printed_empty.contains("aliases: []"));
    assert!(printed_empty.contains("flags: list()"));
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
            always_newline: true,
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
    let rendered = render_value_inner(&Value::Null);
    assert_eq!(rendered, "");
    let md = "---\ntitle:\n---\n\n# Document\n";
    let doc = tomet_markdown::from_markdown(md);
    let cfg = PrinterConfig {
        meta: MetaConfig {
            always_newline: true,
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
            link_no_space: true,
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
    let settings_src = r#"@settings{
  meta: {
    id: {
      type: nanoid
      length: 8
      prefix: "doc-"
    }
  }
}
"#;
    let cfg = load_config_from_str(settings_src).expect("failed to parse settings");
    let id_cfg = cfg.meta.fields.get("id").expect("id config present");
    assert_eq!(id_cfg.field_type.as_deref(), Some("nanoid"));
    assert_eq!(id_cfg.length, Some(8));
    assert_eq!(id_cfg.prefix.as_deref(), Some("doc-"));

    let gen_id = generate_id_for_field(id_cfg);
    assert!(gen_id.starts_with("doc-"));
    assert_eq!(gen_id.len(), 4 + 8); // "doc-" + 8 chars

    let mut doc = tomet_markdown::from_markdown("# Test Note\nHello world\n");
    ensure_document_id_with_config(&mut doc, &cfg);

    let printed = document_to_tm_with_config(&doc, &cfg);
    assert!(printed.contains("id: doc-"));
}

#[test]
fn test_nanoid_force_and_overwrite_behavior() {
    let settings_src = r#"@settings{
  meta: {
    id: {
      type: nanoid
      length: 8
      prefix: "doc-"
      force: true
      overwrite: true
    }
  }
}
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
    assert!(printed1.contains("id: doc-12345678"));

    // Case 2: Invalid format existing ID is overwritten when overwrite: true
    let md2 = "---\nid: invalid-slug\n---\n# Note 2\n";
    let mut doc2 = tomet_markdown::from_markdown(md2);
    ensure_document_id_with_config(&mut doc2, &cfg);
    let printed2 = document_to_tm_with_config(&doc2, &cfg);
    assert!(!printed2.contains("invalid-slug"));
    assert!(printed2.contains("id: doc-"));

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
    assert!(printed3.contains("id: invalid-slug"));
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
            callout_content_style: Some("block".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };
    let printed_block = document_to_tm_with_config(&doc, &cfg_block);
    assert!(printed_block.contains("@callout(info, title: \"2025/04/29 11:09\")\n[ コレさすがに草www\n  お前なら@link(target: \"ref:2025-04-26\")[どうするんだ]？\n]"));

    // Test "box" style
    let cfg_box = PrinterConfig {
        format: FormatConfig {
            callout_content_style: Some("box".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };
    let printed_box = document_to_tm_with_config(&doc, &cfg_box);
    assert!(printed_box.contains("@callout(info, title: \"2025/04/29 11:09\")\n[ コレさすがに草www\n  お前なら@link(target: \"ref:2025-04-26\")[どうするんだ]？ ]"));

    // Test "expanded" style
    let cfg_expanded = PrinterConfig {
        format: FormatConfig {
            callout_content_style: Some("expanded".to_string()),
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
            list_multiline_style_content: Some("box".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };
    let printed_box = document_to_tm_with_config(&doc, &cfg_box);
    assert!(printed_box.contains("-. [ いや、まずこういう話をするときの前提として、\n     Vtuberでくくってるやつがまず、ゴミだ。\n     確実に脳が言っている割合が高い。イメージだけで物事を語る。それってあなたの間奏ですよね。 ]"));

    let cfg_block = PrinterConfig {
        format: FormatConfig {
            list_multiline_style_content: Some("block".to_string()),
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
            callout_content_style: Some("block".to_string()),
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
            link_no_space: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let printed_nospace = document_to_tm_with_config(&doc, &cfg_nospace);
    assert!(printed_nospace.contains("@link(target:\"https://google.com\")[Google]"));
}

#[test]
fn inline_raw_prints_as_backticks() {
    let doc = tomet_parser::parse_document("call `foo()` now\n").unwrap();
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
    let mut child1 = element_new(Sigil::Bare);
    child1.args = Some(Value::Int(1));
    child1.content = Some(vec![Block::Paragraph(Paragraph::new(
        vec![Inline::Text("note 1".into())],
        tomet_ast::Span::dummy(),
    ))]);

    let mut child2 = element_new(Sigil::Bare);
    child2.args = Some(Value::Int(2));
    child2.content = Some(vec![Block::Paragraph(Paragraph::new(
        vec![Inline::Text("note 2".into())],
        tomet_ast::Span::dummy(),
    ))]);

    let mut links = element_new(Sigil::named("links"));
    links.value = Some(ElementValue::from_children(vec![child1, child2]));

    let doc = Document::new(vec![Block::Element(links)], tomet_ast::Span::dummy());
    let printed = document_to_tm(&doc);
    assert!(printed.contains("@links{\n  (1)[note 1]\n  (2)[note 2]\n}"));

    // Verify re-parsing
    let re_parsed = tomet_parser::parse_document(&printed).expect("valid doc");
    assert_eq!(re_parsed.blocks.len(), 1);
}

#[test]
fn test_group_order_content_first() {
    let mut el = element_new(Sigil::named("link"));
    el.args = Some(Value::String("https://example.com".to_string()));
    el.content = Some(vec![Block::Paragraph(Paragraph::new(
        vec![Inline::Text("Example".into())],
        tomet_ast::Span::dummy(),
    ))]);

    let doc = Document::new(vec![Block::Element(el)], tomet_ast::Span::dummy());

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
    let doc = tomet_parser::parse_document("@tag(rust, tomet)\n").expect("valid doc");
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
    let doc = tomet_parser::parse_document("@memo(a: 1)#(myid)\n").expect("valid doc");
    assert_eq!(document_to_tm(&doc).trim(), "@memo(a: 1)#(myid)");

    let doc = tomet_parser::parse_document("=[ Title ]#(intro)\n").expect("valid doc");
    assert_eq!(document_to_tm(&doc).trim(), "=[Title] #(intro)");
}

#[test]
fn test_old_hash_tag_sugar_prints_as_plain_text() {
    let doc = tomet_parser::parse_document("#(rust, tomet)\n").expect("valid doc");
    let printed = document_to_tm(&doc);
    assert_eq!(printed.trim(), "#(rust, tomet)");
}

#[test]
fn test_section_blocks_blank_lines() {
    let sec = Section {
        level: 1,
        title: vec![Inline::Text("Title".into())],
        args: None,
        value: None,
        id: None,
        connects: Vec::new(),
        blocks: vec![
            Block::Paragraph(tomet_ast::Paragraph::new(
                vec![Inline::Text("Paragraph 1".into())],
                tomet_ast::Span::dummy(),
            )),
            Block::Paragraph(tomet_ast::Paragraph::new(
                vec![Inline::Text("Paragraph 2".into())],
                tomet_ast::Span::dummy(),
            )),
        ],
        span: tomet_ast::Span::dummy(),
    };
    let doc = Document::new(vec![Block::Section(sec)], tomet_ast::Span::dummy());
    let printed = document_to_tm(&doc);
    assert_eq!(printed, "=[Title]\nParagraph 1\nParagraph 2\n");
}

#[test]
fn test_headingless_section_printing() {
    let sec = Section {
        level: 1,
        title: Vec::new(),
        args: None,
        value: None,
        id: None,
        connects: Vec::new(),
        blocks: vec![Block::Paragraph(tomet_ast::Paragraph::new(
            vec![Inline::Text("Content".into())],
            tomet_ast::Span::dummy(),
        ))],
        span: tomet_ast::Span::dummy(),
    };
    let doc = Document::new(vec![Block::Section(sec)], tomet_ast::Span::dummy());
    let printed = document_to_tm(&doc);
    assert_eq!(printed, "=\nContent\n");
}
