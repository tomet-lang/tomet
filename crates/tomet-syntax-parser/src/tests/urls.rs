use super::*;

#[test]
fn bare_url_autolink_strips_trailing_punctuation() {
    let doc =
        parse_document("Check https://example.com/foo! and https://example.com/bar.\n").unwrap();
    match &doc.blocks[0] {
        Block::Paragraph(p) => {
            assert_eq!(p.content.len(), 5);
            assert_eq!(p.content[0], Inline::Text("Check ".into()));
            if let Inline::Element(el) = &p.content[1] {
                assert_eq!(
                    el.args,
                    Some(Value::Map(vec![(
                        "target".into(),
                        Value::String("https://example.com/foo".into())
                    )]))
                );
            } else {
                panic!("expected url element");
            }
            assert_eq!(p.content[2], Inline::Text("! and ".into()));
            if let Inline::Element(el) = &p.content[3] {
                assert_eq!(
                    el.args,
                    Some(Value::Map(vec![(
                        "target".into(),
                        Value::String("https://example.com/bar".into())
                    )]))
                );
            } else {
                panic!("expected url element");
            }
            assert_eq!(p.content[4], Inline::Text(".".into()));
        }
        other => panic!("expected paragraph, got {other:?}"),
    }
}

#[test]
fn bare_url_autolink_handles_parentheses() {
    let doc = parse_document("Visit (https://example.com/foo) or https://example.com/path(bar)\n")
        .unwrap();
    match &doc.blocks[0] {
        Block::Paragraph(p) => {
            assert_eq!(p.content[0], Inline::Text("Visit (".into()));
            if let Inline::Element(el) = &p.content[1] {
                assert_eq!(
                    el.args,
                    Some(Value::Map(vec![(
                        "target".into(),
                        Value::String("https://example.com/foo".into())
                    )]))
                );
            } else {
                panic!("expected url element");
            }
            assert_eq!(p.content[2], Inline::Text(") or ".into()));
            if let Inline::Element(el) = &p.content[3] {
                assert_eq!(
                    el.args,
                    Some(Value::Map(vec![(
                        "target".into(),
                        Value::String("https://example.com/path(bar)".into())
                    )]))
                );
            } else {
                panic!("expected url element");
            }
        }
        other => panic!("expected paragraph, got {other:?}"),
    }
}

#[test]
fn bare_url_autolink_supports_http_https_mailto() {
    let doc = parse_document("http://a.com https://b.com mailto:user@example.com\n").unwrap();
    match &doc.blocks[0] {
        Block::Paragraph(p) => {
            assert_eq!(p.content.len(), 5);
            if let Inline::Element(el) = &p.content[0] {
                assert_eq!(
                    el.args,
                    Some(Value::Map(vec![(
                        "target".into(),
                        Value::String("http://a.com".into())
                    )]))
                );
            }
            assert_eq!(p.content[1], Inline::Text(" ".into()));
            if let Inline::Element(el) = &p.content[2] {
                assert_eq!(
                    el.args,
                    Some(Value::Map(vec![(
                        "target".into(),
                        Value::String("https://b.com".into())
                    )]))
                );
            }
            assert_eq!(p.content[3], Inline::Text(" ".into()));
            if let Inline::Element(el) = &p.content[4] {
                assert_eq!(
                    el.args,
                    Some(Value::Map(vec![(
                        "target".into(),
                        Value::String("mailto:user@example.com".into())
                    )]))
                );
            }
        }
        other => panic!("expected paragraph, got {other:?}"),
    }
}

#[test]
fn code_span_does_not_autolink_bare_url() {
    let doc = parse_document("`https://example.com`\n").unwrap();
    match &doc.blocks[0] {
        Block::Paragraph(p) => {
            assert_eq!(p.content.len(), 1);
            match &p.content[0] {
                Inline::Element(el) => {
                    assert_eq!(el.sigil, Sigil::named("raw"));
                    assert_eq!(el.placement, tomet_ast::Placement::Inline);
                    assert_eq!(
                        el.content,
                        Some(vec![Inline::Raw("https://example.com".into())])
                    );
                }
                other => panic!("expected inline @raw element, got {other:?}"),
            }
        }
        other => panic!("expected paragraph, got {other:?}"),
    }
}

#[test]
fn bare_absolute_path_parses_as_a_plain_scalar() {
    // A leading `/` can never start a map key, so this is
    // unambiguous and needs no scheme to disambiguate it.
    assert_eq!(
        parse_value("/readme.md").unwrap(),
        Value::String("/readme.md".into())
    );
    let doc = parse_document("@link(/etc/hosts)[Hosts]\n").unwrap();
    match &doc.blocks[0] {
        Block::Element(el) => {
            assert_eq!(el.args, Some(Value::String("/etc/hosts".into())));
        }
        other => panic!("expected element, got {other:?}"),
    }
}

#[test]
fn bare_scheme_uri_parses_as_one_scalar_not_a_map() {
    // Group B: `scheme://...` at a "fresh value" position (no `key:`
    // wrapper) used to be misread as `Map([(scheme, "//...")])` --
    // `://` immediately after the identifier now forces the whole
    // thing to be read as one scalar instead.
    assert_eq!(
        parse_value("https://example.com/path").unwrap(),
        Value::String("https://example.com/path".into())
    );
    assert_eq!(
        parse_value("file://some/where").unwrap(),
        Value::String("file://some/where".into())
    );
    let doc = parse_document("@link(https://example.com)[Site]\n").unwrap();
    match &doc.blocks[0] {
        Block::Element(el) => {
            assert_eq!(el.args, Some(Value::String("https://example.com".into())));
        }
        other => panic!("expected element, got {other:?}"),
    }
}

#[test]
fn explicit_key_colon_scheme_uri_is_still_a_map() {
    // The disambiguation only fires at a fresh-value position -- a
    // real `key: value` entry (e.g. `url: https://...`, tested above
    // in `preserves_colon_in_url_values`) is unaffected since it never
    // goes through `parse_map_body_or_scalar` for its value half.
    assert_eq!(
        parse_value("url:https://example.com").unwrap(),
        Value::Map(vec![(
            "url".into(),
            Value::String("https://example.com".into())
        )])
    );
}

#[test]
fn bare_url_autolink_preserves_query_param_placeholder() {
    // An autolink is found while scanning running text, so a line
    // holding nothing but a URL is a paragraph with one link in it --
    // not a block-placed element. Only an explicit `@name` standing on
    // its own line is a block.
    let doc = parse_document("http://127.0.0.1:8888/search?lang=ja&q=@query\n").unwrap();
    let Block::Paragraph(p) = &doc.blocks[0] else {
        panic!("expected a paragraph, got {:?}", doc.blocks[0]);
    };
    match &p.content[0] {
        Inline::Element(el) => {
            assert_eq!(
                el.args,
                Some(Value::Map(vec![(
                    "target".into(),
                    Value::String("http://127.0.0.1:8888/search?lang=ja&q=@query".into())
                )]))
            );
        }
        other => panic!("expected element, got {other:?}"),
    }
}
