use super::*;
use tomet_config::{FieldConfig, FormatConfig, GroupOrder};

#[test]
fn format_source_with_config_leaves_meta_block_completely_untouched() {
    let src = "@meta{title: Hello}\n\n#[ Hello ]\n\nSome body text.\n";
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
    let src = "@link(\"https://example.com\")[Example]{rel: \"nofollow\"}:as(button)\n";
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
        "@link[Example](\"https://example.com\"){rel: \"nofollow\"}:as(button)\n"
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
