use super::*;
use tomet_parser::parse_document;

fn typst(src: &str) -> String {
    to_typst(&parse_document(src).unwrap())
}

#[test]
fn renders_headings_by_level() {
    assert_eq!(typst("=[ One ]\n"), "= One\n\n");
    assert_eq!(typst("==[ Two ]\n"), "== Two\n\n");
}

#[test]
fn renders_paragraph_text() {
    assert_eq!(typst("hello world\n"), "hello world\n\n");
}

#[test]
fn escapes_markup_sensitive_characters_in_text() {
    assert_eq!(typst("a * b # c\n"), "a \\* b \\# c\n\n");
}

#[test]
fn renders_emphasis_strong_and_mark() {
    assert_eq!(
        typst("a *em* b **strong** c @mark[mark]\n"),
        "a _em_ b *strong* c #highlight[mark]\n\n"
    );
}

#[test]
fn renders_ruby_as_base_and_reading() {
    assert_eq!(
        typst("a @ruby[漢字](rt:\"かんじ\") b\n"),
        "a 漢字(かんじ) b\n\n"
    );
}

#[test]
fn renders_unordered_and_ordered_lists() {
    assert_eq!(typst("- one\n- two\n"), "- one\n- two\n\n");
    assert_eq!(typst("-. one\n-. two\n"), "1. one\n2. two\n\n");
}

#[test]
fn renders_raw_with_lang_using_a_safe_fence() {
    assert_eq!(
        typst("@raw(lang:rust)[fn main() {}]\n"),
        "```rust\nfn main() {}\n```\n\n"
    );
}

#[test]
fn renders_raw_with_positional_lang_arg() {
    assert_eq!(
        typst("@raw(\"rust\")[fn main() {}]\n"),
        "```rust\nfn main() {}\n```\n\n"
    );
}

#[test]
fn renders_inline_raw_as_backtick_span() {
    assert_eq!(typst("call `foo()` now\n"), "call `foo()` now\n\n");
}

#[test]
fn renders_thematic_break_as_a_full_width_line() {
    assert_eq!(typst("---\n"), "#line(length: 100%)\n\n");
}

#[test]
fn renders_titled_thematic_break() {
    assert_eq!(typst("---[ Title ]---\n"), "Title\n#line(length: 100%)\n\n");
}

#[test]
fn renders_blockquote() {
    assert_eq!(
        typst("@quote[ some quoted text ]\n"),
        "#quote(block: true)[some quoted text]\n\n"
    );
}

#[test]
fn renders_link_with_url_target() {
    assert_eq!(
        typst("@link(target:https://example.com)[Wiki]\n"),
        "#link(\"https://example.com\")[Wiki]\n\n"
    );
}

#[test]
fn renders_id_link_as_a_bare_label_reference() {
    assert_eq!(
        typst("@link(target:id:greeting)[Hello]\n"),
        "#link(<greeting>)[Hello]\n\n"
    );
}

#[test]
fn renders_embed_as_image_with_alt() {
    assert_eq!(
        typst("@embed(target:assets/pic.png)[a cat]\n"),
        "#image(\"assets/pic.png\", alt: \"a cat\")\n\n"
    );
}

#[test]
fn meta_and_config_have_no_visible_output() {
    assert_eq!(typst("@meta{ key: value }\n"), "");
    assert_eq!(typst("@config(format:json)\n"), "");
}

#[test]
fn renders_table() {
    let src = "@table()[\n[ h1 ][ h2 ]\n[ a ][ b ]\n]{}\n";
    assert_eq!(
        typst(src),
        "#table(\n  columns: 2,\n  [*h1*], [*h2*],\n  [a], [b],\n)\n\n"
    );
}

#[test]
fn renders_typed_element_generically_with_a_kind_comment() {
    assert_eq!(
        typst("@caution[ be careful ]\n"),
        "// tomet:caution\nbe careful\n\n"
    );
}

#[test]
fn renders_interp_as_inert_raw_text() {
    assert_eq!(
        typst("Footer: ${copyright}\n"),
        "Footer: `${copyright}`\n\n"
    );
}

#[test]
fn renders_inline_footnote() {
    let src = "Prose with @footnote[a note] here.\n";
    assert_eq!(typst(src), "Prose with #footnote[a note] here.\n\n");
}

#[test]
fn renders_separated_footnote_with_multiple_carets() {
    let src = "\
Prose A ^(shared).
Prose B ^(shared).

@footnote(shared)[Shared footnote explanation.]
";
    let out = typst(src);
    assert!(
        out.contains("Prose A #footnote[Shared footnote explanation.] <fn-shared>."),
        "got: {out}"
    );
    assert!(
        out.contains("Prose B #footnote(<fn-shared>)."),
        "got: {out}"
    );
    assert!(!out.contains("@footnote"), "got: {out}");
}

#[test]
fn renders_tag_elements() {
    let src = "@tag(rust, tomet) and @tag(spec)\n";
    assert_eq!(
        typst(src),
        "#box(fill: luma(240), inset: (x: 3pt, y: 0pt), radius: 2pt)[\\#rust] #box(fill: luma(240), inset: (x: 3pt, y: 0pt), radius: 2pt)[\\#tomet] and #box(fill: luma(240), inset: (x: 3pt, y: 0pt), radius: 2pt)[\\#spec]\n\n"
    );
}
