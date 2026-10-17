use std::str::FromStr;

use lsp_types::{CompletionItemKind, DiagnosticSeverity, HoverContents, Position, Uri};

use crate::{
    completions_for, completions_for_with_uri, definition_for, diagnostics_for,
    document_symbols_for, folding_ranges_for, format_edits, hover_for,
};

#[test]
fn valid_document_has_no_diagnostics() {
    assert_eq!(diagnostics_for("=[ Hello ]\n"), Vec::new());
}

#[test]
fn invalid_document_reports_one_diagnostic() {
    let diags = diagnostics_for("@caution[ unterminated\n");
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].severity, Some(DiagnosticSeverity::ERROR));
    assert_eq!(diags[0].source.as_deref(), Some("tomet"));
}

#[test]
fn duplicate_id_document_reports_validator_diagnostic() {
    let diags = diagnostics_for("=[ One ]#(a)\n=[ Two ]#(a)\n");
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].severity, Some(DiagnosticSeverity::ERROR));
    assert_eq!(diags[0].source.as_deref(), Some("tomet"));
    assert!(diags[0].message.contains("duplicate id `a`"));
}

#[test]
fn hover_returns_element_info() {
    let text = "@callout(type: info)[ Message ]\n";
    let hover = hover_for(text, Position::new(0, 2), None).expect("hover found");
    if let HoverContents::Markup(m) = hover.contents {
        assert!(m.value.contains("callout"));
    } else {
        panic!("expected markup contents");
    }
}

#[test]
fn document_symbols_returns_headings_and_elements() {
    let text = "=[ Heading ]\n\n@info[ Note ]\n";
    let symbols = document_symbols_for(text);
    assert_eq!(symbols.len(), 2);
    assert_eq!(symbols[0].name, "= Heading");
    assert_eq!(symbols[1].name, "@info");
}

#[test]
fn definition_finds_matching_id() {
    let text = "=[ Target ]#(target1)\n\n@deck.ref(id: target1)\n";
    let uri = Uri::from_str("file:///test.tmt").unwrap();
    let def = definition_for(text, Position::new(2, 4), &uri);
    assert!(def.is_some());
}

/// Where a go-to-definition on `(line, col)` of `text` lands, as a start line.
fn definition_line(text: &str, line: u32, col: u32) -> Option<u32> {
    let uri = Uri::from_str("file:///test.tmt").unwrap();
    match definition_for(text, Position::new(line, col), &uri)? {
        lsp_types::GotoDefinitionResponse::Scalar(location) => Some(location.range.start.line),
        other => panic!("expected one location, got {other:?}"),
    }
}

/// A reference is written the way the language writes one -- a link to an
/// anchor, or `#x` in an expression -- and lands on the node with that id.
#[test]
fn definition_follows_the_references_the_language_writes() {
    let target = "=[ Intro ]#(intro)\n\n";
    for (label, text, line, col) in [
        (
            "link to an anchor",
            format!("{target}@link(#intro)\n"),
            2,
            8,
        ),
        ("link by scheme", format!("{target}@link(id:intro)\n"), 2, 8),
        (
            "id in an expression",
            format!("{target}${{#intro}}\n"),
            2,
            3,
        ),
        (
            "same document by name",
            format!("{target}${{self#intro}}\n"),
            2,
            3,
        ),
        (
            "inside a call and a chain",
            format!("{target}${{include(#intro).len()}}\n"),
            2,
            10,
        ),
    ] {
        assert_eq!(definition_line(&text, line, col), Some(0), "{label}");
    }
}

/// With two references in one expression, the one under the cursor wins.
#[test]
fn definition_takes_the_reference_under_the_cursor() {
    let text = "=[ A ]#(a)\n\n=[ B ]#(b)\n\n${add(#a, #b)}\n";
    assert_eq!(definition_line(text, 4, 7), Some(0), "on #a");
    assert_eq!(definition_line(text, 4, 11), Some(2), "on #b");
}

/// What is not a reference to this document's own node does not jump:
/// a bare name (which the expression language does not allow), a node that
/// is not there, and another document.
#[test]
fn definition_does_not_jump_where_there_is_nothing_to_jump_to() {
    let target = "=[ Intro ]#(intro)\n\n";
    assert_eq!(
        definition_line(&format!("{target}${{intro}}\n"), 2, 3),
        None,
        "a bare name is not an id"
    );
    assert_eq!(
        definition_line(&format!("{target}${{#nope}}\n"), 2, 3),
        None,
        "no such id"
    );
    assert_eq!(
        definition_line(&format!("{target}${{tmt(\"b.tmt\")#intro}}\n"), 2, 20),
        None,
        "another document"
    );
    assert_eq!(
        definition_line(&format!("{target}@link(tmt:b.tmt#intro)\n"), 2, 8),
        None,
        "another document"
    );
}

#[test]
fn completions_returns_items() {
    let items = completions_for("", Position::new(0, 0));
    assert!(!items.is_empty());
    // Each built-in is offered with the sigil its shape requires, so
    // an accepted completion is one that validates.
    assert!(items.iter().any(|i| i.label == "@meta"));
    assert!(items.iter().any(|i| i.label == "@link"));
    // `callout` joined `BUILTIN_KINDS`, so it is offered again. It
    // had been special-cased in eight places while the list did not
    // know about it -- including the Markdown reader, which produced
    // an element validation then called unknown.
    assert!(items.iter().any(|i| i.label == "@callout"));
    // `memo` really is not built in -- nothing special-cases it --
    // so it is still not offered.
    assert!(!items.iter().any(|i| i.label.ends_with("memo")));
}

#[test]
fn completions_trigger_prefix() {
    // One sigil, so `@` offers every built-in whatever its shape.
    // Where each may be placed is said in the detail line instead.
    let at_items = completions_for("@", Position::new(0, 1));
    assert!(at_items.iter().any(|i| i.label == "link"));
    assert!(at_items.iter().any(|i| i.label == "config"));
    let config = at_items.iter().find(|i| i.label == "config").unwrap();
    assert!(config.detail.as_deref().unwrap().ends_with("(block)"));
    // `link` takes either shape -- a link alone on a line is how you
    // show one file -- so its detail carries no placement note.
    let link = at_items.iter().find(|i| i.label == "link").unwrap();
    let detail = link.detail.as_deref().unwrap();
    assert!(!detail.ends_with("(inline)") && !detail.ends_with("(block)"));
    let em = at_items.iter().find(|i| i.label == "em").unwrap();
    assert!(em.detail.as_deref().unwrap().ends_with("(inline)"));

    // `#` triggers nothing -- only `@` does.
    assert!(
        completions_for("#", Position::new(0, 1))
            .iter()
            .all(|i| { i.label != "config" && i.label != "link" })
    );

    let interp_items = completions_for("${", Position::new(0, 2));
    assert!(interp_items.iter().any(|i| i.label == "add(...)"));
    // The reserved values start an expression too, and the functions that
    // changed from call to call are no longer offered where rendering is.
    assert!(interp_items.iter().any(|i| i.label == "self"));
    assert!(interp_items.iter().any(|i| i.label == "macro"));
    assert!(
        !interp_items
            .iter()
            .any(|i| i.label == "uuid(...)" || i.label == "date(...)")
    );
}

/// What is offered after `${` is what the evaluator knows, all of it -- so a
/// function added to the language is offered without the editor being told.
#[test]
fn an_expression_is_offered_every_function_and_reserved_name_the_evaluator_has() {
    let items = completions_for("${", Position::new(0, 2));
    for (name, _) in tomet_expr::FUNCTIONS {
        assert!(
            items.iter().any(|i| i.label == format!("{name}(...)")),
            "{name} is not offered"
        );
    }
    for (name, _) in tomet_expr::RESERVED_NAMES {
        assert!(
            items.iter().any(|i| i.label == *name),
            "{name} is not offered"
        );
    }
    // The two the editor used to leave out.
    assert!(items.iter().any(|i| i.label == "include(...)"));
    assert!(items.iter().any(|i| i.label == "addr(...)"));
}

/// An address starts with one of the schemes the address parser reads, under
/// the spelling it reads (`tmt:`, not the `tm:` it used to be), and a
/// function is offered inside `${...}` only: outside it, one cannot be written.
#[test]
fn outside_an_expression_the_address_schemes_are_offered_and_no_functions() {
    let items = completions_for("", Position::new(0, 0));
    let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
    for scheme in ["tmt", "id", "ref", "file", "dir", "url"] {
        assert!(labels.contains(&scheme), "{scheme} is not offered");
    }
    assert!(!labels.contains(&"tm"), "`tm:` is no longer a scheme");
    assert!(!labels.contains(&"unresolved"), "not something to type");
    assert!(
        !labels.iter().any(|l| l.ends_with("(...)")),
        "no function belongs outside `${{...}}`: {labels:?}"
    );
}

#[test]
fn exact_cst_diagnostic_range_on_duplicate_id() {
    let text = "=[ A ]#(my_id)\n\n=[ B ]#(my_id)\n";
    let diags = diagnostics_for(text);
    assert_eq!(diags.len(), 1);
    let diag = &diags[0];
    assert_eq!(diag.range.start.line, 2);
    assert_eq!(diag.range.start.character, 8);
    assert_eq!(diag.range.end.character, 13);
}

#[test]
fn format_edits_formats_tables() {
    let text = "@table[\n[ 殻 ][ 主量子数 n ][ 電子数 2n² ][ 小軌道 ]\n[ K殻 ][ 1 ][ 2 ][ 1s @br(2) ]\n]\n";
    let edits = format_edits(text, None);
    assert_eq!(edits.len(), 1);
    assert!(
        edits[0]
            .new_text
            .contains("[ 殻  ][ 主量子数 n ][ 電子数 2n² ][")
    );
}

#[test]
fn hover_on_table_header_cell() {
    let text = "@table(align: list(left, right, right, left))[\n[ 殻 ][ 主量子数 n ][ 電子数 2n² ][ 小軌道 ]\n[ K殻 ][ 1 ][ 2 ][ 1s @br(2) ]\n]\n";
    // Position on line 1, inside "[ 電子数 2n² ]" (e.g. character 25)
    let hover = hover_for(text, Position::new(1, 25), None).expect("hover found for table header");
    if let HoverContents::Markup(m) = hover.contents {
        assert!(m.value.contains("Table Header (Column 3)"));
        assert!(m.value.contains("電子数 2n²"));
        assert!(m.value.contains("right"));
        assert!(m.value.contains("Total Columns"));
        assert!(m.value.contains("4"));
    } else {
        panic!("expected markup contents");
    }
}

#[test]
fn hover_on_table_data_cell() {
    let text = "@table(align: list(left, right, right, left))[\n[ 殻 ][ 主量子数 n ][ 電子数 2n² ][ 小軌道 ]\n[ K殻 ][ 1 ][ 2 ][ 1s @br(2) ]\n]\n";
    // Position on line 2, inside "[ 2 ]" (column 3, character 15)
    let hover =
        hover_for(text, Position::new(2, 15), None).expect("hover found for table data cell");
    if let HoverContents::Markup(m) = hover.contents {
        assert!(m.value.contains("Table Cell (Column 3, Row 2)"));
        assert!(m.value.contains("電子数 2n²"));
        assert!(m.value.contains("Value"));
        assert!(m.value.contains("2"));
        assert!(m.value.contains("right"));
    } else {
        panic!("expected markup contents");
    }
}

#[test]
fn hover_on_table_overview() {
    let text = "@table(align: list(left, right, right, left))[\n[ 殻 ][ 主量子数 n ][ 電子数 2n² ][ 小軌道 ]\n[ K殻 ][ 1 ][ 2 ][ 1s @br(2) ]\n]\n";
    let hover = hover_for(text, Position::new(0, 2), None).expect("hover found for table overview");
    if let HoverContents::Markup(m) = hover.contents {
        assert!(m.value.contains("Table"));
        assert!(m.value.contains("Rows"));
        assert!(m.value.contains("2"));
        assert!(m.value.contains("Columns"));
        assert!(m.value.contains("4"));
        assert!(m.value.contains("電子数 2n²"));
    } else {
        panic!("expected markup contents");
    }
}

#[test]
fn completions_with_multibyte_characters() {
    // "海岸@" where '海' and '岸' are 3 bytes each
    let text = "海岸@\n";
    // Position at character 3 (after '@')
    let items = completions_for(text, Position::new(0, 3));
    assert!(!items.is_empty());
    assert!(items.iter().any(|i| i.label == "link"));

    // Position at character 2 (inside '岸' in byte terms, but character 2 in LSP)
    let items2 = completions_for(text, Position::new(0, 2));
    assert!(!items2.is_empty());
}

#[test]
fn hover_on_macro_evaluation() {
    let text = "@config{\n  macros: {\n    gh: \"https://github.com/tomet/tomet/issues/${1}\"\n    greet: \"Hello, ${1} ${2}!\"\n    copyright: \"(C) 2026 Tomet Projects\"\n  }\n}\n\n$macro.gh(42)\n\n$macro.greet(\"Alice\", \"Bob\")\n\n${macro.copyright()}\n\n$emoji(\"sparkles\")\n";

    // Hover on $macro.gh(42) (line 8, char 2)
    let hover = hover_for(text, Position::new(8, 2), None).expect("hover found for $gh");
    if let HoverContents::Markup(m) = hover.contents {
        assert!(m.value.contains("Macro Result"));
        assert!(m.value.contains(
            "[https://github.com/tomet/tomet/issues/42](https://github.com/tomet/tomet/issues/42)"
        ));
    } else {
        panic!("expected markup contents");
    }

    // Hover on $macro.greet("Alice", "Bob") (line 10, char 3)
    let hover2 = hover_for(text, Position::new(10, 3), None).expect("hover found for $greet");
    if let HoverContents::Markup(m) = hover2.contents {
        assert!(m.value.contains("Macro Result"));
        assert!(m.value.contains("```text\nHello, Alice Bob!\n```"));
    } else {
        panic!("expected markup contents");
    }

    // Hover on ${macro.copyright()} (line 12, char 3)
    let hover3 =
        hover_for(text, Position::new(12, 3), None).expect("hover found for ${macro.copyright()}");
    if let HoverContents::Markup(m) = hover3.contents {
        assert!(m.value.contains("Macro Result"));
        assert!(m.value.contains("(C) 2026 Tomet Projects"));
    } else {
        panic!("expected markup contents");
    }

    // Hover on $emoji("sparkles") (line 14, char 2)
    let hover4 = hover_for(text, Position::new(14, 2), None).expect("hover found for $emoji");
    if let HoverContents::Markup(m) = hover4.contents {
        assert!(m.value.contains("Macro Result"));
        assert!(m.value.contains("✨"));
    } else {
        panic!("expected markup contents");
    }
}

#[test]
fn hover_on_undefined_macro_shows_error() {
    let text = "$macro.undefined_macro(123)\n";
    let hover =
        hover_for(text, Position::new(0, 5), None).expect("hover found for undefined macro");
    if let HoverContents::Markup(m) = hover.contents {
        assert!(m.value.contains("Macro"));
        assert!(m.value.contains("Error"));
        assert!(m.value.contains("undefined_macro"));
    } else {
        panic!("expected markup contents");
    }
}

#[test]
fn hover_on_macro_defined_in_settings_file_ref() {
    let unique = format!(
        "tomet_test_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let dir = std::env::temp_dir().join(unique);
    std::fs::create_dir_all(&dir).unwrap();
    let config_path = dir.join("default.config.tmt");
    std::fs::write(
        &config_path,
        "@config{\n  macros: {\n    youtube_video: \"https://www.youtube.com/watch?v=${1}\"\n  }\n}\n",
    )
    .unwrap();

    let doc_path = dir.join("sub/note.tmt");
    std::fs::create_dir_all(doc_path.parent().unwrap()).unwrap();
    let doc_text =
        "@settings(file:\"file:default.config.tmt\")\n\n$macro.youtube_video(\"Pm_h6FnF8HU\")\n";
    let uri = Uri::from_str(&format!("file://{}", doc_path.display())).unwrap();

    let hover = hover_for(doc_text, Position::new(2, 5), Some(&uri))
        .expect("hover found for external config macro");
    if let HoverContents::Markup(m) = hover.contents {
        assert!(m.value.contains("Macro Result"));
        assert!(
            m.value
                .contains("https://www.youtube.com/watch?v=Pm_h6FnF8HU")
        );
    } else {
        panic!("expected markup contents");
    }

    // Test embed with macro
    let embed_doc = "@settings(file:\"file:default.config.tmt\")\n\n@embed($macro.youtube_video(\"Pm_h6FnF8HU\"))[Flo Rida]\n";
    let hover2 = hover_for(embed_doc, Position::new(2, 10), Some(&uri))
        .expect("hover found for embed macro");
    if let HoverContents::Markup(m) = hover2.contents {
        assert!(m.value.contains("Macro Result"));
        assert!(
            m.value
                .contains("https://www.youtube.com/watch?v=Pm_h6FnF8HU")
        );
    } else {
        panic!("expected markup contents");
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn hover_on_macro_auto_discovered_from_workspace_config() {
    let unique = format!(
        "tomet_auto_cfg_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let dir = std::env::temp_dir().join(unique);
    std::fs::create_dir_all(&dir).unwrap();
    let config_path = dir.join("default.config.tmt");
    std::fs::write(
        &config_path,
        "@config{\n  macros: {\n    youtube_video: \"https://www.youtube.com/watch?v=${1}\"\n    twitter_post: \"https://x.com/${1}/status/${2}\"\n  }\n}\n",
    )
    .unwrap();

    // Note has NO header at all!
    let doc_path = dir.join("10-19 Journal/12 Daily/2024/12/$2024-12-26.tmt");
    std::fs::create_dir_all(doc_path.parent().unwrap()).unwrap();
    let doc_text = "@embed($macro.youtube_video(\"Pm_h6FnF8HU\"))[Low]\n\n@embed($macro.twitter_post(\"kosekibijou\", \"1807568682631254496\"))[Bijou]\n";
    let uri = Uri::from_str(&format!("file://{}", doc_path.display()).replace(' ', "%20")).unwrap();

    // Hover on youtube_video
    let hover = hover_for(doc_text, Position::new(0, 10), Some(&uri))
        .expect("hover found for auto-discovered youtube macro");
    if let HoverContents::Markup(m) = hover.contents {
        assert!(m.value.contains("Macro Result"));
        assert!(
            m.value
                .contains("https://www.youtube.com/watch?v=Pm_h6FnF8HU")
        );
    } else {
        panic!("expected markup contents");
    }

    // Hover on twitter_post (two arguments)
    let hover2 = hover_for(doc_text, Position::new(2, 10), Some(&uri))
        .expect("hover found for auto-discovered twitter macro");
    if let HoverContents::Markup(m) = hover2.contents {
        assert!(m.value.contains("Macro Result"));
        assert!(
            m.value
                .contains("https://x.com/kosekibijou/status/1807568682631254496")
        );
    } else {
        panic!("expected markup contents");
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn hover_on_macro_defined_in_config_import() {
    let unique = format!(
        "tomet_import_cfg_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let dir = std::env::temp_dir().join(unique);
    std::fs::create_dir_all(&dir).unwrap();
    let config_path = dir.join("custom.config.tmt");
    std::fs::write(
        &config_path,
        "@config{\n  macros: {\n    wiki: \"https://ja.wikipedia.org/wiki/${1}\"\n  }\n}\n",
    )
    .unwrap();

    let doc_path = dir.join("note.tmt");
    let doc_text = "@config(import: \"custom.config.tmt\")\n\n$macro.wiki(\"Rust\")\n";
    let uri = Uri::from_str(&format!("file://{}", doc_path.display())).unwrap();

    let hover = hover_for(doc_text, Position::new(2, 5), Some(&uri))
        .expect("hover found for @config(import:...) macro");
    if let HoverContents::Markup(m) = hover.contents {
        assert!(m.value.contains("Macro Result"));
        assert!(m.value.contains("https://ja.wikipedia.org/wiki/Rust"));
    } else {
        panic!("expected markup contents");
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn hover_on_kind_and_version() {
    let doc_text = "@version(1.0)\n@kind(j.daily)\n\n#[ Title ]\n";
    let hover_ver =
        hover_for(doc_text, Position::new(0, 3), None).expect("hover found for #version");
    if let HoverContents::Markup(m) = hover_ver.contents {
        assert!(m.value.contains("Tomet Version"));
        assert!(m.value.contains("1"));
    } else {
        panic!("expected markup contents");
    }

    let hover_kind = hover_for(doc_text, Position::new(1, 3), None).expect("hover found for #kind");
    if let HoverContents::Markup(m) = hover_kind.contents {
        assert!(m.value.contains("Document Kind"));
        assert!(m.value.contains("j.daily"));
    } else {
        panic!("expected markup contents");
    }
}

#[test]
fn completions_suggest_kind_and_version() {
    // `kind` and `version` are block directives, offered under the one
    // element sigil like everything else.
    let items = completions_for("@", Position::new(0, 1));
    assert!(items.iter().any(|i| i.label == "kind"));
    assert!(items.iter().any(|i| i.label == "version"));
}

#[test]
fn completions_for_file_path() {
    let current = std::env::current_dir().unwrap();
    let test_file = current.join("test_dummy.tmt");
    let uri = Uri::from_str(&format!("file://{}", test_file.to_string_lossy())).unwrap();

    let doc_text = "@link(file: ./";
    let items = completions_for_with_uri(doc_text, Position::new(0, 14), Some(&uri));
    assert!(!items.is_empty(), "path completions should return entries");
    assert!(
        items.iter().any(|i| i.label == "Cargo.toml"),
        "should suggest Cargo.toml"
    );
    assert!(
        items
            .iter()
            .any(|i| i.label == "src/" && i.kind == Some(CompletionItemKind::FOLDER)),
        "should suggest src/ directory"
    );
}

#[test]
fn completions_for_dir_only_suggests_directories() {
    let current = std::env::current_dir().unwrap();
    let test_file = current.join("test_dummy.tmt");
    let uri = Uri::from_str(&format!("file://{}", test_file.to_string_lossy())).unwrap();

    let doc_text = "@dir(./";
    let items = completions_for_with_uri(doc_text, Position::new(0, 7), Some(&uri));
    assert!(
        !items.is_empty(),
        "dir completions should return directories"
    );
    assert!(
        items.iter().any(|i| i.label == "src/"),
        "should suggest src/ directory"
    );
    assert!(
        !items.iter().any(|i| i.label == "Cargo.toml"),
        "should NOT suggest Cargo.toml file for @dir"
    );
}

#[test]
fn completions_for_nested_directory_path() {
    let current = std::env::current_dir().unwrap();
    let test_file = current.join("test_dummy.tmt");
    let uri = Uri::from_str(&format!("file://{}", test_file.to_string_lossy())).unwrap();

    let doc_text = "@link(file: ./src/";
    let items = completions_for_with_uri(doc_text, Position::new(0, 18), Some(&uri));
    assert!(!items.is_empty(), "nested directory should return entries");
    assert!(
        items.iter().any(|i| i.label == "lib.rs"),
        "should suggest src/lib.rs"
    );
}

#[test]
fn folding_ranges_for_hierarchical_sections() {
    let doc_text = r#"= Section 1
Paragraph in section 1.

== Section 1.1
Paragraph in section 1.1.

= Section 2
Paragraph in section 2.
"#;
    let ranges = folding_ranges_for(doc_text);
    assert!(
        !ranges.is_empty(),
        "should find folding ranges for sections"
    );

    // Section 1: line 0 to line 4
    let sec1 = ranges.iter().find(|r| r.start_line == 0);
    assert!(
        sec1.is_some(),
        "Section 1 should have a folding range starting at line 0"
    );
    assert_eq!(sec1.unwrap().end_line, 4);

    // Section 1.1: line 3 to line 4
    let sec1_1 = ranges.iter().find(|r| r.start_line == 3);
    assert!(
        sec1_1.is_some(),
        "Section 1.1 should have a folding range starting at line 3"
    );
    assert_eq!(sec1_1.unwrap().end_line, 4);

    // Section 2: line 6 to line 7
    let sec2 = ranges.iter().find(|r| r.start_line == 6);
    assert!(
        sec2.is_some(),
        "Section 2 should have a folding range starting at line 6"
    );
    assert_eq!(sec2.unwrap().end_line, 7);
}

#[test]
fn folding_ranges_for_comments_and_elements() {
    let doc_text = r#"/*
 multi-line
 block comment
*/

// line comment 1
// line comment 2

```rust
fn main() {
    println!("hello");
}
```
"#;
    let ranges = folding_ranges_for(doc_text);
    // Block comment: line 0 to 3
    let comment_range = ranges.iter().find(|r| r.start_line == 0);
    assert!(comment_range.is_some());
    assert_eq!(comment_range.unwrap().end_line, 3);

    // Line comments: line 5 to 6
    let line_comments = ranges.iter().find(|r| r.start_line == 5);
    assert!(line_comments.is_some());
    assert_eq!(line_comments.unwrap().end_line, 6);

    // Codeblock element: line 8 to 12
    let code_range = ranges.iter().find(|r| r.start_line == 8);
    assert!(code_range.is_some());
    assert_eq!(code_range.unwrap().end_line, 12);
}

/// A section and a list inside an element's `[...]` fold on their own, as
/// well as the element they are in.
#[test]
fn what_an_element_holds_folds_too() {
    let text = "@card[\n=[ Inner ]\n\nfirst\n\nsecond\n\n- a\n- b\n]\n";
    let ranges = folding_ranges_for(text);
    let starts: Vec<(u32, u32)> = ranges.iter().map(|r| (r.start_line, r.end_line)).collect();
    assert!(
        starts.iter().any(|&(start, _)| start == 0),
        "the element: {starts:?}"
    );
    assert!(
        starts.iter().any(|&(start, _)| start == 1),
        "the section inside it: {starts:?}"
    );
    assert!(
        starts.iter().any(|&(start, end)| start == 7 && end == 8),
        "the list inside the section: {starts:?}"
    );
}
