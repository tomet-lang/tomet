use super::*;
use tomet_config::{FieldConfig, FormatConfig};
use tomet_style::{GroupOrder, ParagraphSeparator};

#[test]
fn format_source_with_config_leaves_meta_block_completely_untouched() {
    let src = "::meta(title: Hello)\n\n#[ Hello ]\n\nSome body text.\n";
    let mut config = PrinterConfig::default();
    config.meta.fields.insert(
        "id".to_string(),
        FieldConfig {
            field_type: Some("nanoid".to_string()),
            length: Some(8),
            prefix: Some("doc-".to_string()),
            ..Default::default()
        },
    );

    let out = format_source_with_config(src, &config);
    assert_eq!(out, src);
}

#[test]
fn format_source_with_config_does_not_insert_meta_when_missing() {
    let src = "#[ Hello ]\n\nSome body text.\n";
    let mut config = PrinterConfig::default();
    config.meta.fields.insert(
        "id".to_string(),
        FieldConfig {
            field_type: Some("nanoid".to_string()),
            length: Some(8),
            prefix: Some("doc-".to_string()),
            ..Default::default()
        },
    );

    let out = format_source_with_config(src, &config);
    assert_eq!(out, src);
}

#[test]
fn strips_trailing_whitespace() {
    assert_eq!(format_source("a  \nb\t\n"), "a\nb\n");
}

#[test]
fn normalizes_line_endings() {
    assert_eq!(format_source("a\r\nb\r\n"), "a\nb\n");
    assert_eq!(format_source("a\rb\r"), "a\nb\n");
}

#[test]
fn drops_leading_blank_lines() {
    assert_eq!(format_source("\n\n\na\n"), "a\n");
}

#[test]
fn collapses_multiple_blank_lines_to_one() {
    assert_eq!(format_source("a\n\n\n\nb\n"), "a\n\nb\n");
}

#[test]
fn drops_trailing_blank_lines_and_ensures_one_final_newline() {
    assert_eq!(format_source("a\n\n\n"), "a\n");
    assert_eq!(format_source("a"), "a\n");
}

#[test]
fn empty_input_stays_empty() {
    assert_eq!(format_source(""), "");
    assert_eq!(format_source("\n\n  \n"), "");
}

#[test]
fn already_formatted_input_is_unchanged() {
    let src = "#[ Hello ]\n\n- one\n- two\n";
    assert_eq!(format_source(src), src);
}

#[test]
fn raw_content_is_preserved_losslessly() {
    let src = "@memo[\n```\nline one  \n\nline two\n```\n]\n";
    assert_eq!(format_source(src), src);
}

#[test]
fn raw_content_in_brackets_is_preserved_losslessly() {
    let src = "@raw(lang:rust)[\nfn foo() {\n    let a = 1;  \n\n    let b = 2;\n}\n]\n";
    assert_eq!(format_source(src), src);
}

#[test]
fn raw_content_in_a_holes_content_is_preserved_losslessly() {
    // A hole holds content like an element does, so a raw element in it is raw.
    let src = "<?(body)>[@raw(lang:rust)[\nfn foo() {\n    let a = 1;  \n\n    let b = 2;\n}\n]]\n";
    assert_eq!(format_source(src), src);
}

#[test]
fn fenced_code_block_content_is_preserved_losslessly() {
    let src = "```rust\nfn foo() {\n    let a = 1;  \n\n    let b = 2;\n}\n```\n";
    assert_eq!(format_source(src), src);
}

#[test]
fn test_format_tables_with_config_left_align() {
    let src = "@table[\n[ 殻 ][ 主量子数 n ][ 電子数 2n² ][ 小軌道 ]\n[ K殻 ][ 1 ][ 2 ][ 1s @br(2) ]\n[ L殻 ][ 2 ][ 8 ][ 2s+2p @br(2+6) ]\n]\n";
    let config = PrinterConfig {
        format: FormatConfig {
            table_adjust_width: Some("auto".to_string()),
            table_max_col_width: Some(20),
            table_align: Some("left".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };

    let out = format_source_with_config(src, &config);
    assert!(out.contains("[ 殻  ][ 主量子数 n ][ 電子数 2n² ][ 小軌道         ]"));
    assert!(out.contains("[ K殻 ][ 1          ][ 2          ][ 1s @br(2)      ]"));
}

#[test]
fn test_format_tables_with_config_right_align() {
    let src = "@table[\n[ 殻 ][ 主量子数 n ][ 電子数 2n² ][ 小軌道 ]\n[ K殻 ][ 1 ][ 2 ][ 1s @br(2) ]\n[ L殻 ][ 2 ][ 8 ][ 2s+2p @br(2+6) ]\n]\n";
    let config = PrinterConfig {
        format: FormatConfig {
            table_adjust_width: Some("auto".to_string()),
            table_max_col_width: Some(20),
            table_align: Some("right".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };

    let out = format_source_with_config(src, &config);
    assert!(out.contains("[  殻 ][ 主量子数 n ][ 電子数 2n² ][         小軌道 ]"));
    assert!(out.contains("[ K殻 ][          1 ][          2 ][      1s @br(2) ]"));
}

#[test]
fn test_format_tables_with_config_center_align() {
    let src = "@table[\n[ 殻 ][ 主量子数 n ][ 電子数 2n² ][ 小軌道 ]\n[ K殻 ][ 1 ][ 2 ][ 1s @br(2) ]\n[ L殻 ][ 2 ][ 8 ][ 2s+2p @br(2+6) ]\n]\n";
    let config = PrinterConfig {
        format: FormatConfig {
            table_adjust_width: Some("auto".to_string()),
            table_max_col_width: Some(20),
            table_align: Some("center".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };

    let out = format_source_with_config(src, &config);
    assert!(out.contains("[ 殻  ][ 主量子数 n ][ 電子数 2n² ][     小軌道     ]"));
    assert!(out.contains("[ K殻 ][     1      ][     2      ][   1s @br(2)    ]"));
}

#[test]
fn test_format_tables_with_per_table_align_arg() {
    let src = "@table(align: [left, right, right, left])[\n[ 殻 ][ 主量子数 n ][ 電子数 2n² ][ 小軌道 ]\n[ K殻 ][ 1 ][ 2 ][ 1s @br(2) ]\n]\n";
    let config = PrinterConfig {
        format: FormatConfig {
            table_adjust_width: Some("auto".to_string()),
            table_max_col_width: Some(20),
            ..Default::default()
        },
        ..Default::default()
    };

    let out = format_source_with_config(src, &config);
    assert!(out.contains("[ 殻  ][ 主量子数 n ][ 電子数 2n² ][ 小軌道    ]"));
    assert!(out.contains("[ K殻 ][          1 ][          2 ][ 1s @br(2) ]"));
}

#[test]
fn test_format_tables_unicode_superscript_and_cjk_width() {
    let src = "@table(align: [right])[\n[ 電子数 2n² ]\n[ 2 ]\n]\n";
    let config = PrinterConfig {
        format: FormatConfig {
            table_adjust_width: Some("auto".to_string()),
            table_max_col_width: Some(20),
            ..Default::default()
        },
        ..Default::default()
    };

    let out = format_source_with_config(src, &config);
    assert!(out.contains("[ 電子数 2n² ]"));
    assert!(out.contains("[          2 ]"));
}

#[test]
fn test_format_tables_with_pipe_syntax() {
    let src = "@table\n|[ feature ][ lsp ][ vscode ][ zed ][ neovim ][ helix ]\n|[ highlight ][ o ][ o ][ o ][ o ][ o ]\n|[ suggestion ][ ][ ][ ][ ][ ]\n|[ auto complete ][ ][ ][ ][ ][ ]\n";
    let config = PrinterConfig {
        format: FormatConfig {
            table_adjust_width: Some("auto".to_string()),
            table_max_col_width: Some(20),
            ..Default::default()
        },
        ..Default::default()
    };

    let out = format_source_with_config(src, &config);
    assert!(out.contains("|[ feature       ][ lsp ][ vscode ][ zed ][ neovim ][ helix ]"));
    assert!(out.contains("|[ highlight     ][ o   ][ o      ][ o   ][ o      ][ o     ]"));
    assert!(out.contains("|[ suggestion    ][     ][        ][     ][        ][       ]"));
    assert!(out.contains("|[ auto complete ][     ][        ][     ][        ][       ]"));
}

#[test]
fn test_format_element_group_order_content_first() {
    let src = "@link(\"https://example.com\")[Example]\n";
    let config = PrinterConfig {
        format: FormatConfig {
            group_order: Some(GroupOrder::ContentFirst),
            ..Default::default()
        },
        ..Default::default()
    };
    let out = format_source_with_config(src, &config);
    assert_eq!(out, "@link[Example](\"https://example.com\")\n");
}

#[test]
fn test_format_element_group_order_args_first() {
    let src = "@link[Example](\"https://example.com\")\n";
    let config = PrinterConfig {
        format: FormatConfig {
            group_order: Some(GroupOrder::ArgsFirst),
            ..Default::default()
        },
        ..Default::default()
    };
    let out = format_source_with_config(src, &config);
    assert_eq!(out, "@link(\"https://example.com\")[Example]\n");
}

#[test]
fn test_format_element_group_order_none_preserves_order() {
    let src1 = "@link(\"https://example.com\")[Example]\n";
    let config = PrinterConfig::default();
    let out1 = format_source_with_config(src1, &config);
    assert_eq!(out1, src1);

    let src2 = "@link[Example](\"https://example.com\")\n";
    let out2 = format_source_with_config(src2, &config);
    assert_eq!(out2, src2);
}

#[test]
fn test_format_element_group_order_nested() {
    let src = "@parent(p_arg)[\n  @child(c_arg)[Child Text]\n]\n";
    let config = PrinterConfig {
        format: FormatConfig {
            group_order: Some(GroupOrder::ContentFirst),
            ..Default::default()
        },
        ..Default::default()
    };
    let out = format_source_with_config(src, &config);
    assert_eq!(out, "@parent[\n  @child[Child Text](c_arg)\n](p_arg)\n");
}

#[test]
fn test_format_element_group_order_with_value_and_connects() {
    let src = "@link(\"https://example.com\")[Example]{rel: \"nofollow\"}::as(button)\n";
    let config = PrinterConfig {
        format: FormatConfig {
            group_order: Some(GroupOrder::ContentFirst),
            ..Default::default()
        },
        ..Default::default()
    };
    let out = format_source_with_config(src, &config);
    assert_eq!(
        out,
        "@link[Example](\"https://example.com\"){rel: \"nofollow\"}::as(button)\n"
    );
}

#[test]
fn test_format_element_group_order_in_section_data() {
    let src = "=[ Title ]{ key: @link(\"https://example.com\")[Example] }\n";
    let config = PrinterConfig {
        format: FormatConfig {
            group_order: Some(GroupOrder::ContentFirst),
            ..Default::default()
        },
        ..Default::default()
    };
    let out = format_source_with_config(src, &config);
    assert_eq!(
        out,
        "=[ Title ]{ key: @link[Example](\"https://example.com\") }\n"
    );
}

#[test]
fn test_format_element_group_order_preserves_code_blocks() {
    let src = "```tomet\n@link(\"url\")[text]\n```\n";
    let config = PrinterConfig {
        format: FormatConfig {
            group_order: Some(GroupOrder::ContentFirst),
            ..Default::default()
        },
        ..Default::default()
    };
    let out = format_source_with_config(src, &config);
    assert_eq!(out, src);
}

#[test]
fn test_format_element_group_order_link_only() {
    let src = "@link(\"https://example.com\")[Example]\n\n@card(foo: 1)[Bar Content]\n";
    let config = PrinterConfig {
        format: FormatConfig {
            link_group_order: Some(GroupOrder::ContentFirst),
            ..Default::default()
        },
        ..Default::default()
    };
    let out = format_source_with_config(src, &config);
    assert_eq!(
        out,
        "@link[Example](\"https://example.com\")\n\n@card(foo: 1)[Bar Content]\n"
    );
}

#[test]
fn test_format_element_group_order_link_override_global() {
    let src = "@link(\"https://example.com\")[Example]\n\n@card[Bar Content](foo: 1)\n";
    let config = PrinterConfig {
        format: FormatConfig {
            group_order: Some(GroupOrder::ArgsFirst),
            link_group_order: Some(GroupOrder::ContentFirst),
            ..Default::default()
        },
        ..Default::default()
    };
    let out = format_source_with_config(src, &config);
    assert_eq!(
        out,
        "@link[Example](\"https://example.com\")\n\n@card(foo: 1)[Bar Content]\n"
    );
}

#[test]
fn format_element_group_order_scaling_is_linear_not_quadratic() {
    let config = PrinterConfig {
        format: FormatConfig {
            link_group_order: Some(GroupOrder::ContentFirst),
            ..Default::default()
        },
        ..Default::default()
    };

    let make_doc = |n: usize| -> String {
        let mut s = String::new();
        for i in 0..n {
            s.push_str(&format!(
                "Paragraph with @link(ref:\"target_{i}\")[Link {i}] and some text.\n\n"
            ));
        }
        s
    };

    let doc_small = make_doc(20);
    let doc_large = make_doc(80); // 4x elements and ~4x document size

    // Warm up
    let _ = format_source_with_config(&doc_small, &config);
    let _ = format_source_with_config(&doc_large, &config);

    // Measure medians over several iterations to avoid noise
    let mut times_small = Vec::new();
    let mut times_large = Vec::new();
    for _ in 0..9 {
        let t0 = std::time::Instant::now();
        let _ = format_source_with_config(&doc_small, &config);
        times_small.push(t0.elapsed());

        let t0 = std::time::Instant::now();
        let _ = format_source_with_config(&doc_large, &config);
        times_large.push(t0.elapsed());
    }

    times_small.sort();
    times_large.sort();
    let med_small = times_small[4].as_nanos() as f64;
    let med_large = times_large[4].as_nanos() as f64;

    let ratio = med_large / med_small.max(1.0);

    // For a 4x increase in input size:
    // - Linear O(N) scaling yields ratio ~ 4.0.
    // - Quadratic O(N^2) scaling (e.g. re-parsing per swapped element) yields ratio ~ 16.0.
    // The test asserts ratio < 10.0, which decisively fails on O(N^2) while leaving
    // plenty of headroom for linear scaling noise.
    assert!(
        ratio < 10.0,
        "Scaling ratio {ratio:.2} exceeds threshold 10.0 for 4x size increase (small: {med_small:.0}ns, large: {med_large:.0}ns)"
    );
}

fn separator_config(separator: Option<ParagraphSeparator>) -> PrinterConfig {
    PrinterConfig {
        format: FormatConfig {
            paragraph_separator: separator,
            ..Default::default()
        },
        ..Default::default()
    }
}

/// `src` formatted with `separator`, which must leave the document it parses to as it was.
fn separated(src: &str, separator: Option<ParagraphSeparator>) -> String {
    let out = format_source_with_config(src, &separator_config(separator));
    assert_eq!(
        tomet_ast_parser::parse_file(&out).unwrap(),
        tomet_ast_parser::parse_file(src).unwrap(),
        "changed the document: {out:?}"
    );
    out
}

#[test]
fn paragraphs_are_left_as_written_unless_a_separator_is_configured() {
    for src in ["a\n\nb\n", "a\n;b\n", "a\n\n; b\n"] {
        assert_eq!(separated(src, None), format_source(src));
    }
}

#[test]
fn a_blank_line_becomes_a_semicolon_and_back() {
    let semicolon = Some(ParagraphSeparator::Semicolon);
    let blank = Some(ParagraphSeparator::Blank);
    assert_eq!(separated("a\n\nb\n\n\nc\n", semicolon), "a\n;b\n;c\n");
    assert_eq!(separated("a\n;b\n;c\n", blank), "a\n\nb\n\nc\n");
    // A `;` that is followed by a space, and one that follows a blank line already.
    assert_eq!(separated("a\n\n; b\n", semicolon), "a\n;b\n");
    assert_eq!(separated("a\n\n; b\n", blank), "a\n\nb\n");
}

#[test]
fn the_separator_is_idempotent() {
    for separator in [ParagraphSeparator::Blank, ParagraphSeparator::Semicolon] {
        let config = separator_config(Some(separator));
        let once = format_source_with_config("a\n\nb\n;c\n\n\n@x(1)\n", &config);
        assert_eq!(format_source_with_config(&once, &config), once);
    }
}

#[test]
fn the_elements_that_stand_like_a_paragraph_are_set_off_too() {
    assert_eq!(
        separated(
            "a\n\n@file(x)\n\n@file(y)\n",
            Some(ParagraphSeparator::Semicolon)
        ),
        "a\n;@file(x)\n;@file(y)\n"
    );
    assert_eq!(
        separated(";@file(x)\n;@file(y)\n", Some(ParagraphSeparator::Blank)),
        "@file(x)\n\n@file(y)\n"
    );
}

#[test]
fn a_heading_a_list_and_a_comment_keep_what_they_have() {
    let src = "a\n\n=[ t ]\n\nb\n\n- x\n\nc\n\n// note\n\nd\n";
    let out = separated(src, Some(ParagraphSeparator::Semicolon));
    assert!(out.contains("a\n\n=[ t ]"), "got: {out:?}");
    assert!(out.contains("// note\n\nd"), "got: {out:?}");
    assert!(!out.contains(";=["), "got: {out:?}");
}

#[test]
fn paragraphs_in_a_section_an_element_and_a_list_item_are_set_off_too() {
    let semicolon = Some(ParagraphSeparator::Semicolon);
    assert_eq!(
        separated("=[ t ]\n\na\n\nb\n", semicolon),
        "=[ t ]\n\na\n;b\n"
    );
    assert_eq!(
        separated("@x[\n  a\n\n  b\n]\n", semicolon),
        "@x[\n  a\n  ;b\n]\n"
    );
}

#[test]
fn what_follows_a_list_is_not_a_paragraph_of_it() {
    // `b` is not in the item, however it is indented: it follows the list, which keeps its blank.
    let src = "- a\n\n  b\n";
    assert_eq!(
        separated(src, Some(ParagraphSeparator::Semicolon)),
        format_source(src)
    );
}

#[test]
fn a_document_that_does_not_parse_is_left_alone() {
    let src = "a\n\n@x(\n\nb\n";
    assert_eq!(
        format_paragraph_separators(src, &separator_config(Some(ParagraphSeparator::Semicolon))),
        src
    );
}

fn aligned(src: &str) -> String {
    let config = PrinterConfig {
        format: FormatConfig {
            map_align_values: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let out = format_source_with_config(src, &config);
    assert_eq!(
        tomet_ast_parser::parse_file(&out).unwrap(),
        tomet_ast_parser::parse_file(src).unwrap(),
        "changed the document: {out:?}"
    );
    assert_eq!(
        format_source_with_config(&out, &config),
        out,
        "not idempotent"
    );
    out
}

#[test]
fn values_are_left_as_written_unless_alignment_is_asked_for() {
    let src = "::x(\n  a: 1\n  longer: 2\n)\n";
    assert_eq!(
        format_source_with_config(src, &PrinterConfig::default()),
        src
    );
}

#[test]
fn the_values_of_a_multi_line_group_are_aligned_after_the_colon() {
    let src = "::format(
  meta: { always_newline: true }
  heading: { space_inside_brackets: true }
  table: { adjust_width: auto, max_col_width: 50 }
  link: { no_space: true, group_order: \"[]()\" }
  quote: { always_newline: true }
  callout: { style: { content: block } }
  list: { multiline: { style: { content: box } } }
)
";
    let want = "::format(
  meta:    { always_newline: true }
  heading: { space_inside_brackets: true }
  table:   { adjust_width: auto, max_col_width: 50 }
  link:    { no_space: true, group_order: \"[]()\" }
  quote:   { always_newline: true }
  callout: { style: { content: block } }
  list:    { multiline: { style: { content: box } } }
)
";
    assert_eq!(aligned(src), want);
    // The other style of alignment, the colon aligned, is brought to this one.
    assert_eq!(aligned(&want.replace("meta:    ", "meta   : ")), want);
}

#[test]
fn a_blank_line_a_comment_and_a_multi_line_value_end_a_run() {
    let src = "::x(
  a: 1
  bbb: 2

  c: 3
  dddd: 4
  // a note
  e: 5
  ffffff: 6
  list:
    - x
  g: 7
  hh: 8
)
";
    let want = "::x(
  a:   1
  bbb: 2

  c:    3
  dddd: 4
  // a note
  e:      5
  ffffff: 6
  list:
    - x
  g:  7
  hh: 8
)
";
    assert_eq!(aligned(src), want);
}

#[test]
fn a_wide_key_counts_for_its_display_width() {
    assert_eq!(
        aligned("::x(\n  \"見出し\": 1\n  abcdefghij: 2\n  a: 3\n)\n"),
        "::x(\n  \"見出し\":   1\n  abcdefghij: 2\n  a:          3\n)\n"
    );
}

#[test]
fn a_key_wider_than_the_limit_is_left_out_of_the_alignment() {
    let src = "::x(
  a: 1
  a_key_that_is_far_too_long: 2
  bb: 3
)
";
    // The default limit is 20 columns: the long key is `key: value`, and the rest still align.
    assert_eq!(
        aligned(src),
        "::x(
  a:  1
  a_key_that_is_far_too_long: 2
  bb: 3
)
"
    );
    // With the limit raised it is aligned to like any other.
    let config = PrinterConfig {
        format: FormatConfig {
            map_align_values: true,
            map_max_key_width: Some(40),
            ..Default::default()
        },
        ..Default::default()
    };
    assert_eq!(
        format_source_with_config(src, &config),
        "::x(
  a:                          1
  a_key_that_is_far_too_long: 2
  bb:                         3
)
"
    );
    // And lowered, only the keys that fit are aligned.
    let config = PrinterConfig {
        format: FormatConfig {
            map_align_values: true,
            map_max_key_width: Some(1),
            ..Default::default()
        },
        ..Default::default()
    };
    assert_eq!(format_source_with_config(src, &config), src);
}

#[test]
fn a_line_the_parser_reads_as_a_string_is_not_an_entry() {
    // A bare non-ASCII key is no key: `見出し: 1` is one string, and its spacing is its value.
    let src = "::x(\n  見出し: 1\n  abcdefgh: 2\n  a: 3\n)\n";
    assert_eq!(
        aligned(src),
        "::x(\n  見出し: 1\n  abcdefgh: 2\n  a:        3\n)\n"
    );
}

#[test]
fn only_the_entries_of_a_group_written_over_several_lines_are_aligned() {
    let src = "::x(a: 1, longer: 2)\n\n@y(a: 1, bb: 2)[text]\n";
    assert_eq!(aligned(src), src);
    // The group of an element is aligned as that of a declaration is; a nested one on its own.
    assert_eq!(
        aligned("@y(\n  a: 1\n  bbb: @z(\n    c: 1\n    dd: 2\n  )\n  e: 3\n)[text]\n"),
        "@y(\n  a: 1\n  bbb: @z(\n    c:  1\n    dd: 2\n  )\n  e: 3\n)[text]\n"
    );
}

#[test]
fn document_metadata_is_not_aligned() {
    let src = "::meta(\n  title: T\n  id: x\n)\n\ntext\n";
    assert_eq!(aligned(src), src);
}
