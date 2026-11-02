//! Rendering inline content and block-permitted content runs.

use tomet_ast::{Block, Inline, Placement};
use tomet_config::PrinterConfig;

/// Renders a run of inlines.
///
/// A `[content]` group holds `Inline`s, but one of them may be
/// block-placed -- an element written at a line start inside the group,
/// the way `@references[` holds its entries. Placement is spelled with
/// line breaks rather than with a sigil, so such an element has to start
/// and end its own line here or it would come back from the parser as
/// part of the running text.
pub(crate) fn render_inlines(inlines: &[Inline], config: &PrinterConfig) -> String {
    let mut s = String::new();
    for inline in inlines {
        match inline {
            Inline::Text(t) => {
                // A `;` that starts a line would break the block when read
                // back; `\;` keeps it text.
                if t.value.starts_with(';') && s.ends_with('\n') {
                    s.push('\\');
                }
                s.push_str(&t.value);
            }
            Inline::Raw(t) => s.push_str(&t.value),
            // A literal newline, not `softbreak_join`'s space/nothing.
            // Several callers (`render_list_with_indent`'s
            // `list_multiline_style_content`, `render_callout`'s
            // `callout_content_style`) split this function's output on
            // `'\n'` to lay wrapped content out across multiple indented
            // lines -- a feature that used to ride on markdown import
            // embedding a literal `'\n'` straight into `Text.value` for a
            // softbreak. Folding to a space here would silently turn that
            // into always-one-line output.
            Inline::SoftBreak(_) | Inline::LineBreak(_) => s.push('\n'),
            Inline::Interp(interp) => s.push_str(&crate::element::render_interp(interp)),
            Inline::Element(el) => push_node(
                &mut s,
                el.placement == Placement::Block,
                crate::element::render_element(el.as_view(), config),
            ),
            Inline::Hole(hole) => push_node(
                &mut s,
                hole.placement == Placement::Block,
                crate::element::render_hole(hole, config),
            ),
        }
    }
    s
}

/// An element or a hole, `rendered`, in the run being built in `s`: a
/// block-placed one starts and ends its own line.
fn push_node(s: &mut String, block: bool, rendered: String) {
    if block && !s.is_empty() && !s.ends_with('\n') {
        s.push('\n');
    }
    s.push_str(&rendered);
    if block {
        s.push('\n');
    }
}

/// Renders an `Element.content` (`Vec<Block>`, see `tmtroot/docs/spec/
/// feature/content-shape.tmt`) back to source text.
///
/// The common case -- exactly one plain paragraph, which is what ordinary
/// inline usage (`@link(...)[Tomet]`, `@em[text]`, ...) always parses to --
/// prints byte-for-byte what `render_inlines` on that paragraph's own
/// content always has, so this is not a behaviour change for anything that
/// doesn't actually use the new block-permitting content. Multiple blocks,
/// or a block that isn't a paragraph, go through `render_blocks`, the
/// joining a document and a section use for their own blocks.
pub(crate) fn render_content_blocks(blocks: &[Block], config: &PrinterConfig) -> String {
    if let [Block::Paragraph(p)] = blocks {
        return render_inlines(&p.content, config);
    }
    let mut out = String::new();
    crate::block::render_blocks(blocks, config, &mut out);
    out
}
