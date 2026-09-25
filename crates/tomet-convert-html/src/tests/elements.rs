use super::*;

#[test]
fn meta_element_has_no_visible_output() {
    let doc = parse_document("@meta(format:yaml)+++\nkey: value\n+++\n").unwrap();
    let body = render_body(&doc);
    assert_eq!(body, "");
}

#[test]
fn config_element_has_no_visible_output() {
    let doc = parse_document("@config(format:json)\n").unwrap();
    let body = render_body(&doc);
    assert_eq!(body, "");
}

#[test]
fn adjacent_meta_blocks_have_no_visible_output() {
    // `@meta` is documented as `placement: head` / `singleton: true`
    // (see `docs/spec/builtin-settings.tmt`) -- three of
    // them in one file, as used below, is not actually valid Tomet
    // and would eventually be rejected by `tomet-validator`. But
    // `tomet-parser` itself no longer folds them into one
    // `Block::Paragraph` (each `@meta(...)` on its own line, with no
    // blank line before the next, now parses as its own
    // `Block::Element` -- see `document.rs`'s `Stop::Paragraph`), and
    // `tomet-html` has no validation layer of its own, so this
    // just confirms it still renders each one as empty output rather
    // than leaking a stray whitespace-only `<p>` -- garbage in,
    // harmless out.
    let doc = parse_document(
            "@meta(format:json)+++\n{\"key\":\"value\"}\n+++\n@meta(format:yaml)+++\nkey:value\n+++\n@meta(format:toml)+++\nkey = \"value\"\n+++\n\n=[ next ]\n",
        )
        .unwrap();
    let body = render_body(&doc);
    assert_eq!(body, "<h1>next</h1>\n");
}

#[test]
fn renders_typed_element_generically() {
    let doc = parse_document("@caution[ be careful ]\n").unwrap();
    let body = render_body(&doc);
    assert_eq!(
        body,
        "<div class=\"tm-element tm-caution\">be careful</div>\n"
    );
}

/// `doc.icon` is `ElementKind::Custom("doc.icon")` -- this crate has no
/// dedicated handling for it, so it only draws anything when a caller
/// installs `RenderOptions::custom_element`. Also proves the hook sees
/// `name`/`pkg` already resolved by key, not the parser's positional
/// sentinel -- `doc.icon`'s `name` is a bare positional argument, and
/// nothing in this document declares `doc`'s vocabulary (it can't:
/// `doc` is a `RESERVED_NAMESPACES` entry).
#[test]
fn custom_element_hook_renders_doc_icon() {
    let doc = parse_document("a @doc.icon(\"star\", pkg:\"lucide\") b\n").unwrap();
    let options = RenderOptions {
        custom_element: Some(CustomElementRenderer::new(|ctx| {
            if ctx.kind != "doc.icon" {
                return None;
            }
            let args = ctx.normalized_args();
            let get = |key: &str| {
                let Some(Value::Map(entries)) = &args else {
                    return None;
                };
                entries
                    .iter()
                    .find(|(k, _)| k == key)
                    .and_then(|(_, v)| match v {
                        Value::String(s) => Some(s.clone()),
                        _ => None,
                    })
            };
            let name = get("name")?;
            let pkg = get("pkg").unwrap_or_default();
            Some(format!("<i data-icon=\"{name}\" data-pkg=\"{pkg}\"></i>"))
        })),
        ..RenderOptions::default()
    };
    let body = render_body_with(&doc, &options);
    assert_eq!(
        body,
        "<p>a <i data-icon=\"star\" data-pkg=\"lucide\"></i> b</p>\n"
    );
}

/// A hook that declines an element (returns `None`) falls back to the
/// existing generic rendering, same as having no hook at all.
#[test]
fn custom_element_hook_falling_through_matches_generic_rendering() {
    let doc = parse_document("@doc.icon(\"star\", pkg:\"lucide\")\n").unwrap();
    let without_hook = render_body(&doc);
    let options = RenderOptions {
        custom_element: Some(CustomElementRenderer::new(|_ctx| None)),
        ..RenderOptions::default()
    };
    let with_declining_hook = render_body_with(&doc, &options);
    assert_eq!(with_declining_hook, without_hook);
}

#[test]
fn meta_and_config_with_positional_format_arg_have_no_visible_output() {
    let doc = parse_document("@meta(\"json\")+++\n{\"key\": \"value\"}\n+++\n@config(\"json\")\n")
        .unwrap();
    let body = render_body(&doc);
    assert_eq!(body, "");
}

/// This crate does not evaluate `${...}` any more, and this test is
/// what is left of the one that said it did.
///
/// It used to assert that `$gh(42)` came out as the expanded URL. Four
/// converters each decided that for themselves and three of them
/// decided differently -- this one evaluated without an
/// `EvaluationContext` or the vault's config, so `${self.path}`
/// resolved in CommonMark and not here. Evaluation moved to
/// `tomet-transform`'s `resolve_interpolations`, which runs before any
/// converter sees the tree; the expansion those assertions were about
/// is pinned there now.
///
/// What reaches here is a document nobody prepared, and the only
/// honest rendering of that is what was written.
#[test]
fn an_unprepared_interpolation_renders_as_its_own_source() {
    let doc =
        parse_document("Issue: $gh(42)\nFooter: ${copyright}\nMath: ${add(10, 5)}\n").unwrap();
    assert_eq!(
        render_body(&doc),
        "<p>Issue: ${gh(42)} Footer: ${copyright} Math: ${add(10, 5)}</p>\n"
    );
}

#[test]
fn test_renders_wrap_sections() {
    let src = "=[ Chapter 1 ]\n\nParagraph in chapter.\n\n==[ Section 1.1 ]\n\nNested paragraph.\n";
    let doc = parse_document(src).unwrap();
    let options = RenderOptions {
        wrap_sections: true,
        ..RenderOptions::default()
    };
    let html = render_body_with(&doc, &options);

    assert!(html.contains("<section class=\"tmt-section level-1\">\n<h1>Chapter 1</h1>\n<p>Paragraph in chapter.</p>\n<section class=\"tmt-section level-2\">\n<h2>Section 1.1</h2>\n<p>Nested paragraph.</p>\n</section>\n</section>\n"), "got {html}");
}

#[test]
fn test_renders_tag_element() {
    let src = "Prose with #(rust, tomet) tags.\n";
    let doc = parse_document(src).unwrap();
    let html = render_body(&doc);

    assert!(html.contains("<span class=\"tmt-tag-list\"><span class=\"tmt-tag\">#rust</span> <span class=\"tmt-tag\">#tomet</span></span>"), "got {html}");
}
