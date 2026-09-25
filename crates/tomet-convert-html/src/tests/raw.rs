use super::*;

#[test]
fn renders_raw_with_lang() {
    let doc = parse_document("@raw(lang:rust)[fn main() {}]\n").unwrap();
    let body = render_body(&doc);
    assert_eq!(
        body,
        "<pre><code class=\"language-rust\">fn main() {}</code></pre>\n"
    );
}

#[test]
fn fenced_raw_block_renders_the_same_as_bracket_raw() {
    let doc = parse_document("```rust\nfn main() {}\n```\n").unwrap();
    let body = render_body(&doc);
    assert_eq!(
        body,
        "<pre><code class=\"language-rust\">fn main() {}</code></pre>\n"
    );
}

#[test]
fn raw_content_stays_literal_not_interpreted_as_markup() {
    let doc = parse_document("```rust\nlet x = *ptr; let y = @T; @deco `q`\n```\n").unwrap();
    let body = render_body(&doc);
    assert_eq!(
        body,
        "<pre><code class=\"language-rust\">let x = *ptr; let y = @T; @deco `q`</code></pre>\n"
    );
}

#[test]
fn raw_with_a_nested_bracket_is_not_truncated_early() {
    let doc = parse_document("@raw(lang:rust)[let v = [1, 2, 3];]\n").unwrap();
    let body = render_body(&doc);
    assert_eq!(
        body,
        "<pre><code class=\"language-rust\">let v = [1, 2, 3];</code></pre>\n"
    );
}

#[test]
fn renders_inline_backtick_as_code() {
    let doc = parse_document("call `foo()` and `@kind(doc.index)` now\n").unwrap();
    let body = render_body(&doc);
    assert_eq!(
        body,
        "<p>call <code>foo()</code> and <code>@kind(doc.index)</code> now</p>\n"
    );
}

#[test]
fn renders_explicit_inline_raw() {
    let doc = parse_document("use @raw[Ctrl+C] or @raw(rust)[x = 1] here\n").unwrap();
    let body = render_body(&doc);
    assert_eq!(
        body,
        "<p>use <code>Ctrl+C</code> or <code class=\"language-rust\">x = 1</code> here</p>\n"
    );
}

#[test]
fn raw_value_group_is_id_cssclass_metadata_not_code() {
    let doc =
        parse_document("@raw(lang:rust){id:snippet1, cssclass:card}[fn main() {}]\n").unwrap();
    let body = render_body(&doc);
    assert_eq!(
        body,
        "<pre id=\"snippet1\" class=\"card\"><code class=\"language-rust\">fn main() {}</code></pre>\n"
    );
}

#[test]
fn renders_raw_with_positional_lang_arg() {
    let doc = parse_document("@raw(\"rust\")[fn main() {}]\n").unwrap();
    let body = render_body(&doc);
    assert_eq!(
        body,
        "<pre><code class=\"language-rust\">fn main() {}</code></pre>\n"
    );
}
