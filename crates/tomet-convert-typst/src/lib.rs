//! `tomet_ast::Document` -> Typst markup source, hand-written (same
//! reasoning as `tomet-convert-markdown`: the output space is much
//! smaller than the input space, so no need for a crate here).
//!
//! One-directional (`Document -> Typst` only, no importer) and lossy for
//! constructs with no clean Typst equivalent:
//! - Heading `id`/`cssclass` attrs have no Typst markup form and are
//!   dropped, same as `tomet-convert-markdown`'s heading handling.
//! - `mark` renders as `#highlight[...]`, a Typst builtin only available
//!   since Typst 0.13 -- older Typst versions will fail to compile it.
//! - `${...}` interpolation round-trips as inert raw text (`` `${...}` ``)
//!   rather than being evaluated: Typst has its own `#`/`{}` expression
//!   syntax with unrelated semantics, so naively emitting `${...}`
//!   unescaped would risk Typst trying (and failing) to parse it as its
//!   own code.
//! - Any `<T>`/`@name` element with no dedicated mapping below (including
//!   `icon`, `bare`, and anything a hand-authored `.tmt` file might use
//!   that the mapping doesn't special-case) falls back to rendering just
//!   its inner content, annotated with a leading `// tomet:{kind}` line
//!   comment in block position. Unlike CommonMark, Typst's normal (non-
//!   HTML-export) compile mode has no raw-passthrough escape hatch, so
//!   this fallback is lossier than `tomet-convert-markdown`'s by
//!   necessity -- there is nowhere to put the dropped `args`/`kind`
//!   metadata except that comment.
//! - `escape_text` only escapes markup-sensitive characters that Typst
//!   recognizes anywhere in a line (`*_`` `#$<>@[]~\`); it does not guard
//!   against `=`/`-`/`+`/`/` at the very start of a line being
//!   misinterpreted as heading/list/term-list syntax -- a known gap, not
//!   yet hit by any of this crate's own tests.
//! - Same-document `id:` links only resolve against `@links{}` container
//!   entries (which emit a matching Typst label), not against arbitrary
//!   element ids -- mirroring `tomet-convert-html`'s `#link-{id}`
//!   convention, which is likewise anchored to `@links{}` entries rather
//!   than headings.

use tomet_ast::{
    Block, Document, Element, ElementValue, Inline, InterpExpr, InterpExprKind, Literal, Value,
};
use tomet_semantics::{
    path_target,
    TargetScheme, classify_std_lenient, heading_level, is_directive, link_target, list_items,
    list_ordered, normalized_element_args, parse_table_rows, target_scheme,
};

pub fn to_typst(doc: &Document) -> String {
    let mut out = String::new();
    for block in &doc.blocks {
        render_block(block, &mut out);
    }
    out
}

fn render_block(block: &Block, out: &mut String) {
    match block {
        Block::Paragraph(p) => {
            // Same reasoning as `tomet-convert-markdown`'s `render_block`:
            // an all-invisible-element paragraph (e.g. adjacent
            // `@meta(...)` lines with no blank line between them) must
            // not leave a stray blank paragraph behind.
            let text = inline_to_typst(&p.content);
            if !text.trim().is_empty() {
                out.push_str(&text);
                out.push_str("\n\n");
            }
        }
        Block::Element(el) if list_ordered(el).is_some() => render_list(el, out),
        Block::Element(el) => {
            let text = element_to_typst(el, false);
            if !text.is_empty() {
                out.push_str(&text);
                out.push_str("\n\n");
            }
        }
    }
}

fn render_list(el: &Element, out: &mut String) {
    render_list_with_indent(el, 0, out);
    out.push('\n');
}

fn render_list_with_indent(el: &Element, indent: usize, out: &mut String) {
    let ordered = list_ordered(el).unwrap_or(false);
    let indent_str = "  ".repeat(indent);
    for (i, item) in list_items(el).iter().enumerate() {
        out.push_str(&indent_str);
        let marker = if ordered {
            format!("{}. ", i + 1)
        } else {
            "- ".to_string()
        };
        out.push_str(&marker);
        // `args` (the `(...)` marker `Value`) has no Typst markup
        // equivalent -- dropped on export, same as this crate's other
        // documented lossy cases (see the module doc).
        out.push_str(&inline_to_typst(item.content.as_deref().unwrap_or(&[])));
        out.push('\n');
        if let Some(children) = &item.children {
            for child in children {
                if let Block::Element(sub) = child {
                    if list_ordered(sub).is_some() {
                        render_list_with_indent(sub, indent + 1, out);
                    }
                }
            }
        }
    }
}

fn inline_to_typst(inlines: &[Inline]) -> String {
    let mut out = String::new();
    for inline in inlines {
        match inline {
            Inline::Text(t) => out.push_str(&escape_text(&t.value)),
            Inline::Element(el) => out.push_str(&element_to_typst(el, true)),
        }
    }
    out
}

fn element_to_typst(el: &Element, inline: bool) -> String {
    let kind = classify_std_lenient(el);
    match kind.as_str() {
        // Directives -- see `tomet_semantics::is_directive`.
        _ if is_directive(&kind) => String::new(),
        // Block-position only, mirroring `tomet-convert-markdown`'s
        // heading handling -- a nested/inline `@heading(...)`
        // (`inline == true`) falls through to the generic fallback
        // instead of emitting a bare `= text` mid-paragraph (which
        // wouldn't parse back as a heading anyway).
        "heading" if !inline => render_heading(el),
        "hr" => render_hr(el),
        "em" => format!("_{}_", content_to_typst(el)),
        "strong" => format!("*{}*", content_to_typst(el)),
        "mark" => format!("#highlight[{}]", content_to_typst(el)),
        "strikeout" => format!("#strike[{}]", content_to_typst(el)),
        "codeblock" => render_code_block(el),
        "quote" => render_quote(el, inline),
        "callout" => render_callout(el),
        "table" => render_table(el),
        "link" => render_link(el),
        "file" | "dir" => render_path(el, inline),
        "embed" => render_embed(el),
        "links" => render_links_container(el),
        "interp" => render_interp(el),
        _ => render_generic(el, kind.as_str(), inline),
    }
}

fn render_heading(el: &Element) -> String {
    let level = heading_level(el).unwrap_or(1) as usize;
    let content = el.content.as_deref().unwrap_or(&[]);
    format!("{} {}", "=".repeat(level), inline_to_typst(content))
}

/// A bare `---` break renders as a full-width rule; a titled one
/// (`---[ Title ]---`) has no single-construct Typst equivalent, so it's
/// lossy: the title text on its own line followed by the rule.
fn render_hr(el: &Element) -> String {
    match &el.content {
        Some(title) if !title.is_empty() => {
            format!("{}\n#line(length: 100%)", inline_to_typst(title))
        }
        _ => "#line(length: 100%)".to_string(),
    }
}

/// Typst has no bare `>` markup shorthand; `#quote` is the built-in
/// function, and its own `block:` parameter is exactly the distinction
/// Tomet draws by position.
fn render_quote(el: &Element, inline: bool) -> String {
    if inline {
        return format!("#quote[{}]", content_to_typst(el));
    }
    format!("#quote(block: true)[{}]", content_to_typst(el))
}

/// No dedicated `callout` `ElementKind` exists (it classifies as
/// `Custom("callout")`, same as `tomet-convert-markdown`'s `render_callout`
/// -- see that crate's `element_to_md` for the precedent this mirrors).
/// Typst has no built-in admonition/callout construct, so this renders a
/// plain bordered block with a bold `[variant] title` header line -- a
/// first-pass approximation, not a byte-for-byte callout package match.
fn render_callout(el: &Element) -> String {
    let (variant, title) = callout_variant_and_title(el);
    let body = content_to_typst(el);
    let header = match title {
        Some(t) => format!("*[{variant}] {t}*"),
        None => format!("*[{variant}]*"),
    };
    format!("#block(inset: 8pt, stroke: (left: 2pt))[{header}\n\n{body}]")
}

fn callout_variant_and_title(el: &Element) -> (String, Option<String>) {
    let mut variant = None;
    let mut title = None;

    if let Some(args) = &el.args {
        match args {
            Value::String(s) => variant = Some(s.clone()),
            Value::Map(entries) => {
                for (k, v) in entries {
                    if k == "variant" {
                        if let Value::String(s) = v {
                            variant = Some(s.clone());
                        }
                    } else if k == "title" {
                        if let Value::String(s) = v {
                            title = Some(s.clone());
                        }
                    }
                }
                if variant.is_none() && !entries.is_empty() {
                    if let Value::String(s) = &entries[0].1 {
                        variant = Some(s.clone());
                    }
                }
            }
            Value::Seq(items) => {
                if let Some(Value::String(s)) = items.first() {
                    variant = Some(s.clone());
                }
            }
            _ => {}
        }
    }

    (variant.unwrap_or_else(|| "note".to_string()), title)
}

/// `<codeblock>(lang:xxx)[code]` -- Typst uses the same triple-backtick
/// raw-block fence syntax as CommonMark, so this reuses the same
/// backtick-run-counting fence logic `tomet-convert-markdown`'s
/// `fence_for` uses, for the same reason: a fence must be at least one
/// backtick longer than the longest backtick run already inside the code,
/// or it would terminate early on re-parse. Reads `lang` via
/// `normalized_element_args` (not raw `el.args`) so a positional lang
/// arg (`<codeblock>("rust")[...]`) resolves the same as a named
/// `lang:rust` one.
fn render_code_block(el: &Element) -> String {
    let args = normalized_element_args(el);
    let lang = args
        .as_ref()
        .and_then(as_map)
        .and_then(|m| map_get(m, "lang"))
        .map(value_to_plain)
        .unwrap_or_default();
    let code = el
        .content
        .as_ref()
        .map(|a| inlines_to_plain(a))
        .unwrap_or_default();
    let fence = fence_for(&code);
    format!("{fence}{lang}\n{code}\n{fence}")
}

fn fence_for(code: &str) -> String {
    let longest_run = code
        .split(|c: char| c != '`')
        .map(|run| run.len())
        .max()
        .unwrap_or(0);
    "`".repeat((longest_run + 1).max(3))
}

/// `@table()[...]` -> Typst's `#table(...)` function call. Row 0 is
/// treated as a header (its cells bolded) unless `header:false` is set,
/// same default/opt-out convention `tomet-convert-html`'s
/// `render_table_element` uses for its `<thead>` split.
fn render_table(el: &Element) -> String {
    let inlines = match &el.content {
        Some(content) => content,
        None => return String::new(),
    };
    let rows = parse_table_rows(inlines);
    if rows.is_empty() {
        return String::new();
    }

    let mut col_count = 0;
    for row in &rows {
        col_count = col_count.max(row.cells.len());
    }
    if col_count == 0 {
        return String::new();
    }

    let args = normalized_element_args(el);
    let has_header = args
        .as_ref()
        .and_then(as_map)
        .and_then(|m| map_get(m, "header"))
        .map(|v| match v {
            Value::Bool(b) => *b,
            _ => true,
        })
        .unwrap_or(true);

    let mut lines = Vec::new();
    for (ri, row) in rows.iter().enumerate() {
        let mut cells = Vec::new();
        for ci in 0..col_count {
            let text = if ci < row.cells.len() {
                inline_to_typst(&row.cells[ci].content)
            } else {
                String::new()
            };
            let text = if ri == 0 && has_header {
                format!("*{text}*")
            } else {
                text
            };
            cells.push(format!("[{text}]"));
        }
        lines.push(format!("  {},", cells.join(", ")));
    }

    format!("#table(\n  columns: {col_count},\n{}\n)", lines.join("\n"))
}

/// `@link(target:..)` -- the target string's own scheme prefix (see
/// `tomet_semantics::target_scheme`) decides which shape it exports as:
/// a same-document label reference (`id:`, resolving against a
/// `@links{}` entry's emitted label -- see `render_links_container`) or
/// a plain `#link("target")[text]` for everything else (Typst has no
/// separate wikilink-style shorthand for `ref:` targets, so those fall
/// into the same general case). The scheme prefix itself is stripped
/// before rendering -- it's addressing metadata, not part of the visible
/// target.
/// `@file(x)`/`@dir(x)` -> Typst raw text, the same shape CommonMark
/// gets. A path is named, not navigated to, so there is no `#link` here.
fn render_path(el: &Element, inline: bool) -> String {
    let path = path_target(el, &classify_std_lenient(el)).unwrap_or_default();
    let content = match &el.content {
        Some(content) if !content.is_empty() => Some(inline_to_typst(content)),
        _ => None,
    };
    if inline {
        return format!("`{}`", content.unwrap_or(path));
    }
    match content {
        Some(content) => format!("`{path}` {content}"),
        None => format!("`{path}`"),
    }
}

fn render_link(el: &Element) -> String {
    let raw_target = link_target(el, &classify_std_lenient(el)).unwrap_or_default();
    let (scheme, target) = target_scheme(&raw_target);
    let text = match &el.content {
        Some(content) if !content.is_empty() => inline_to_typst(content),
        _ => String::new(),
    };
    match scheme {
        TargetScheme::Id => {
            let text = if text.is_empty() {
                target.to_string()
            } else {
                text
            };
            format!("#link(<link-{target}>)[{text}]")
        }
        TargetScheme::Unresolved => {
            if text.is_empty() {
                target.to_string()
            } else {
                text
            }
        }
        _ => {
            let text = if text.is_empty() {
                target.to_string()
            } else {
                text
            };
            format!("#link(\"{}\")[{text}]", escape_typst_string(target))
        }
    }
}

/// Strips `target`'s scheme prefix the same way `render_link` does -- see
/// `tomet-convert-html`'s `render_embed_element` for why `<embed>` needs
/// this too, not just a raw passthrough.
fn render_embed(el: &Element) -> String {
    let raw_target = link_target(el, &classify_std_lenient(el)).unwrap_or_default();
    let (_, src) = target_scheme(&raw_target);
    let alt = el
        .content
        .as_ref()
        .map(|a| inlines_to_plain(a))
        .unwrap_or_default();
    if alt.is_empty() {
        format!("#image(\"{}\")", escape_typst_string(src))
    } else {
        format!(
            "#image(\"{}\", alt: \"{}\")",
            escape_typst_string(src),
            escape_typst_string(&alt)
        )
    }
}

/// `@links{}` definition entries have no Typst equivalent construct, so
/// this renders a bullet list -- each entry also carries a `<link-{id}>`
/// label so `render_link`'s `id:`-scheme case has something to resolve
/// against, mirroring `tomet-convert-html`'s `<dt id="link-{id}">`
/// anchor convention.
fn render_links_container(el: &Element) -> String {
    let mut out = String::new();
    if let Some(children) = el.value.as_ref().map(|v| v.as_children()) {
        for (i, child) in children.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            let id = child.args.as_ref().map(value_to_plain).unwrap_or_default();
            let content = child
                .content
                .as_ref()
                .map(|a| inline_to_typst(a))
                .unwrap_or_default();
            out.push_str(&format!("- *{id}*: {content} <link-{id}>"));
        }
    }
    out
}

/// Re-renders an `InterpExpr` back to `${...}`-shaped source text, wrapped
/// in a raw span (`` `${...}` ``) so Typst's own `$`-prefixed math mode
/// never tries to parse it -- see the module doc's note on why `${...}`
/// is kept inert here rather than evaluated.
fn render_interp(el: &Element) -> String {
    match &el.value {
        Some(ElementValue::Interp(expr)) => format!("`${{{}}}`", render_interp_expr(expr)),
        _ => String::new(),
    }
}

fn render_interp_expr(expr: &InterpExpr) -> String {
    match &expr.kind {
        InterpExprKind::Identifier(name) => name.clone(),
        InterpExprKind::Literal(Literal::Int(i)) => i.to_string(),
        InterpExprKind::Literal(Literal::Float(x)) => x.to_string(),
        InterpExprKind::Literal(Literal::String(s)) => format!("{s:?}"),
        InterpExprKind::Call { callee, args } => {
            let args = args.iter().map(render_interp_expr).collect::<Vec<_>>();
            format!("{}({})", render_interp_expr(callee), args.join(", "))
        }
        InterpExprKind::Member { object, member } => {
            format!("{}.{member}", render_interp_expr(object))
        }
        InterpExprKind::NamedArg { name, value } => {
            format!("{name}: {}", render_interp_expr(value))
        }
    }
}

/// Anything with no dedicated Typst mapping (a hand-authored `<T>`/`@name`
/// element the mapping above doesn't special-case) renders just its inner
/// content -- Typst's normal compile mode has no raw-passthrough escape
/// hatch, so unlike `tomet-convert-markdown`'s equivalent fallback,
/// `args`/`kind` metadata can't be preserved in the output itself. In
/// block position only (never inline, to avoid corrupting running text) a
/// leading `// tomet:{kind}` line comment records what was dropped.
fn render_generic(el: &Element, kind: &str, inline: bool) -> String {
    let content = content_to_typst(el);
    if content.is_empty() {
        return String::new();
    }
    if inline {
        content
    } else {
        format!("// tomet:{kind}\n{content}")
    }
}

fn content_to_typst(el: &Element) -> String {
    el.content
        .as_ref()
        .map(|a| inline_to_typst(a))
        .unwrap_or_default()
}

/// Flattens inline content to plain text -- used where Typst syntax can't
/// itself carry markup (a raw code block's contents, an image's `alt`).
fn inlines_to_plain(inlines: &[Inline]) -> String {
    let mut s = String::new();
    for inline in inlines {
        match inline {
            Inline::Text(t) => s.push_str(&t.value),
            Inline::Element(el) => {
                if let Some(content) = &el.content {
                    s.push_str(&inlines_to_plain(content));
                }
            }
        }
    }
    s
}

/// Escapes markup-sensitive characters for plain running text. Does not
/// handle line-start-only markers (`=`/`-`/`+`/`/`) -- see the module doc.
fn escape_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(
            c,
            '\\' | '*' | '_' | '`' | '#' | '$' | '<' | '>' | '@' | '[' | ']' | '~'
        ) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Escapes a value going inside a Typst `"..."` string literal (function
/// arguments like `#link("...")`/`#image("...")`), distinct from
/// `escape_text`'s markup escaping.
fn escape_typst_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, '\\' | '"') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

fn value_to_plain(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::String(s) => s.clone(),
        Value::Seq(items) => items
            .iter()
            .map(value_to_plain)
            .collect::<Vec<_>>()
            .join(", "),
        // A map has no single scalar representation; callers that need
        // per-key access use `as_map`/`map_get` instead.
        Value::Map(_) => String::new(),
    }
}

fn as_map(v: &Value) -> Option<&Vec<(String, Value)>> {
    match v {
        Value::Map(m) => Some(m),
        _ => None,
    }
}

fn map_get<'a>(map: &'a [(String, Value)], key: &str) -> Option<&'a Value> {
    map.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tomet_parser::parse_document;

    fn typst(src: &str) -> String {
        to_typst(&parse_document(src).unwrap())
    }

    #[test]
    fn renders_headings_by_level() {
        assert_eq!(typst("#[ One ]\n"), "= One\n\n");
        assert_eq!(typst("##[ Two ]\n"), "== Two\n\n");
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
            typst("a *em* b **strong** c ==mark==\n"),
            "a _em_ b *strong* c #highlight[mark]\n\n"
        );
    }

    #[test]
    fn renders_unordered_and_ordered_lists() {
        assert_eq!(typst("- one\n- two\n"), "- one\n- two\n\n");
        assert_eq!(typst("-. one\n-. two\n"), "1. one\n2. two\n\n");
    }

    #[test]
    fn renders_codeblock_with_lang_using_a_safe_fence() {
        assert_eq!(
            typst("@codeblock(lang:rust)[fn main() {}]\n"),
            "```rust\nfn main() {}\n```\n\n"
        );
    }

    #[test]
    fn renders_codeblock_with_positional_lang_arg() {
        assert_eq!(
            typst("@codeblock(\"rust\")[fn main() {}]\n"),
            "```rust\nfn main() {}\n```\n\n"
        );
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
    fn renders_id_link_against_links_container_label() {
        assert_eq!(
            typst("@link(target:id:greeting)[Hello]\n"),
            "#link(<link-greeting>)[Hello]\n\n"
        );
    }

    #[test]
    fn renders_links_container_as_labeled_bullet_list() {
        assert_eq!(
            typst("@links {\n  (greeting)[ note ]\n}\n"),
            "- *greeting*: note <link-greeting>\n\n"
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
        assert_eq!(typst("@meta(format:yaml)+++\nkey: value\n+++\n"), "");
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
}
