use super::*;

#[test]
fn renders_links_container_as_definition_list() {
    let doc = parse_document("@links {\n  (1)[ note ]\n  (anotation1)[ note2 ]\n}\n").unwrap();
    let body = render_body(&doc);
    assert_eq!(
        body,
        "<dl class=\"tm-links\">\n<dt id=\"link-1\">1</dt>\n<dd>note</dd>\n<dt id=\"link-anotation1\">anotation1</dt>\n<dd>note2</dd>\n</dl>\n"
    );
}

#[test]
fn renders_link_with_explicit_target_key() {
    let doc = parse_document("@link(target:https://example.com)[Wiki]\n").unwrap();
    let body = render_body(&doc);
    assert_eq!(
        body,
        "<a class=\"tm-url\" href=\"https://example.com\">Wiki</a>\n"
    );
}

#[test]
fn renders_link_from_bare_scheme_uri_positional_target() {
    // `https://...` is shape-unambiguous, so the positional shorthand
    // (no `target:` key at all) works: `builtin_positional_arg_key`
    // normalizes it under `target` before `link_target` sees it.
    let doc = parse_document("@link(https://example.com)[Wiki]\n").unwrap();
    let body = render_body(&doc);
    assert_eq!(
        body,
        "<a class=\"tm-url\" href=\"https://example.com\">Wiki</a>\n"
    );
}

#[test]
fn renders_link_from_bare_absolute_path_positional_target() {
    // Same positional mechanism, other shape: a leading `/` is also
    // shape-unambiguous (`target_scheme` defaults it to `File`).
    let doc = parse_document("@link(/readme.md)[Readme]\n").unwrap();
    let body = render_body(&doc);
    assert_eq!(
        body,
        "<a class=\"tm-file\" href=\"/readme.md\">Readme</a>\n"
    );
}

#[test]
fn renders_id_link_as_a_same_document_anchor() {
    let doc = parse_document("@link(target:id:greeting)[Hello]\n").unwrap();
    let body = render_body(&doc);
    assert_eq!(
        body,
        "<a class=\"tm-id\" href=\"#link-greeting\">Hello</a>\n"
    );
}

#[test]
fn renders_embed_as_img() {
    let doc = parse_document("@embed(target:assets/pic.png)[a cat]\n").unwrap();
    let body = render_body(&doc);
    assert_eq!(body, "<img src=\"assets/pic.png\" alt=\"a cat\">\n");
}

#[test]
fn renders_embed_unresolved() {
    let doc = parse_document("@embed(target:\"unresolved:missing.png\")[missing cat]\n").unwrap();
    let body = render_body(&doc);
    assert_eq!(
        body,
        "<img class=\"tm-embed tm-embed-unresolved\" src=\"missing.png\" alt=\"missing cat\" aria-disabled=\"true\">\n"
    );
}

#[test]
fn renders_embed_with_positional_src_arg() {
    let doc = parse_document("@embed(\"assets/pic.png\")[a cat]\n").unwrap();
    let body = render_body(&doc);
    assert_eq!(body, "<img src=\"assets/pic.png\" alt=\"a cat\">\n");
}

#[test]
fn test_renders_unresolved_link_placeholder() {
    let doc = parse_document("- @link(\"unresolved:Future Note\")[Future Note]\n").unwrap();
    let body = render_body(&doc);
    assert_eq!(
        body,
        "<ul>\n<li><a class=\"tm-ref tm-ref-unresolved\" aria-disabled=\"true\" data-ref=\"Future Note\">Future Note</a></li>\n</ul>\n"
    );
}
