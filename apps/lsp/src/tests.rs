use std::str::FromStr;

use lsp_types::{
    CompletionItemKind, DiagnosticSeverity, HoverContents, Position, Uri,
};

use crate::{
    completions_for, completions_for_with_uri, definition_for, diagnostics_for,
    document_symbols_for, format_edits, hover_for,
};

#[test]
fn valid_document_has_no_diagnostics() {
    assert_eq!(diagnostics_for("#[ Hello ]\n"), Vec::new());
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
    let diags = diagnostics_for("#[ One ]{id: a}\n#[ Two ]{id: a}\n");
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
    let text = "=[ Target ]{id: target1}\n\n@deck.ref(id: target1)\n";
    let uri = Uri::from_str("file:///test.tmt").unwrap();
    let def = definition_for(text, Position::new(2, 4), &uri);
    assert!(def.is_some());
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

    // `#` is the heading marker and offers no elements.
    assert!(
        completions_for("#", Position::new(0, 1))
            .iter()
            .all(|i| { i.label != "config" && i.label != "link" })
    );

    let interp_items = completions_for("${", Position::new(0, 2));
    assert!(interp_items.iter().any(|i| i.label == "add(...)"));
}

#[test]
fn exact_cst_diagnostic_range_on_duplicate_id() {
    let text = "#[ A ]{id: my_id}\n\n#[ B ]{id: my_id}\n";
    let diags = diagnostics_for(text);
    assert_eq!(diags.len(), 1);
    let diag = &diags[0];
    assert_eq!(diag.range.start.line, 2);
    assert_eq!(diag.range.start.character, 11);
    assert_eq!(diag.range.end.character, 16);
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
    let hover =
        hover_for(text, Position::new(1, 25), None).expect("hover found for table header");
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
    let hover =
        hover_for(text, Position::new(0, 2), None).expect("hover found for table overview");
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
    let text = "@config{\n  macros: {\n    gh: \"https://github.com/tomet/tomet/issues/${1}\"\n    greet: \"Hello, ${1} ${2}!\"\n    copyright: \"(C) 2026 Tomet Projects\"\n  }\n}\n\n$gh(42)\n\n$greet(\"Alice\", \"Bob\")\n\n${copyright}\n\n$emoji(\"sparkles\")\n";

    // Hover on $gh(42) (line 8, char 2)
    let hover = hover_for(text, Position::new(8, 2), None).expect("hover found for $gh");
    if let HoverContents::Markup(m) = hover.contents {
        assert!(m.value.contains("Macro Result"));
        assert!(m.value.contains("[https://github.com/tomet/tomet/issues/42](https://github.com/tomet/tomet/issues/42)"));
    } else {
        panic!("expected markup contents");
    }

    // Hover on $greet("Alice", "Bob") (line 10, char 3)
    let hover2 = hover_for(text, Position::new(10, 3), None).expect("hover found for $greet");
    if let HoverContents::Markup(m) = hover2.contents {
        assert!(m.value.contains("Macro Result"));
        assert!(m.value.contains("```text\nHello, Alice Bob!\n```"));
    } else {
        panic!("expected markup contents");
    }

    // Hover on ${copyright} (line 12, char 3)
    let hover3 =
        hover_for(text, Position::new(12, 3), None).expect("hover found for ${copyright}");
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
    let text = "$undefined_macro(123)\n";
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
        "@config(format:json)+++\n{\n  \"macros\": {\n    \"youtube_video\": \"https://www.youtube.com/watch?v=${1}\"\n  }\n}\n+++\n",
    )
    .unwrap();

    let doc_path = dir.join("sub/note.tmt");
    std::fs::create_dir_all(doc_path.parent().unwrap()).unwrap();
    let doc_text =
        "@settings(file:\"file:default.config.tmt\")\n\n$youtube_video(\"Pm_h6FnF8HU\")\n";
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
    let embed_doc = "@settings(file:\"file:default.config.tmt\")\n\n@embed($youtube_video(\"Pm_h6FnF8HU\"))[Flo Rida]\n";
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
        "@config(format:json)+++\n{\n  \"macros\": {\n    \"youtube_video\": \"https://www.youtube.com/watch?v=${1}\",\n    \"twitter_post\": \"https://x.com/${1}/status/${2}\"\n  }\n}\n+++\n",
    )
    .unwrap();

    // Note has NO header at all!
    let doc_path = dir.join("10-19 Journal/12 Daily/2024/12/$2024-12-26.tmt");
    std::fs::create_dir_all(doc_path.parent().unwrap()).unwrap();
    let doc_text = "@embed($youtube_video(\"Pm_h6FnF8HU\"))[Low]\n\n@embed($twitter_post(\"kosekibijou\", \"1807568682631254496\"))[Bijou]\n";
    let uri =
        Uri::from_str(&format!("file://{}", doc_path.display()).replace(' ', "%20")).unwrap();

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
    let doc_text = "@config(import: \"custom.config.tmt\")\n\n$wiki(\"Rust\")\n";
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

    let hover_kind =
        hover_for(doc_text, Position::new(1, 3), None).expect("hover found for #kind");
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
