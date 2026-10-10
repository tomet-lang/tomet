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
    for (i, inline) in inlines.iter().enumerate() {
        match inline {
            Inline::Text(t) => s.push_str(&t.value),
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
            Inline::Element(el) => {
                let block = el.placement == Placement::Block;
                if block && !s.is_empty() && !s.ends_with('\n') {
                    s.push('\n');
                }
                let rendered = crate::element::render_element(el, config);
                // A joined element (`Placement::Inline` reached here only
                // via an explicit `\` continuation trigger --
                // `docs/spec/syntax.tmt`'s `##[ 継続 ]`) that opens a
                // fresh output line needs the trigger re-emitted, or a
                // reparse would isolate it back into its own block --
                // except the very first item, which nothing precedes
                // there, so no marker is ever needed for that one. Only a
                // `@name(...)`-shaped rendering is ever at risk of being
                // misread as a bare element opening its own line;
                // raw/backtick spans and other non-`@` sugar cannot be.
                if !block
                    && i > 0
                    && (s.is_empty() || s.ends_with('\n'))
                    && rendered.starts_with('@')
                {
                    s.push('\\');
                }
                s.push_str(&rendered);
                if block {
                    s.push('\n');
                }
            }
        }
    }
    s
}

/// Renders an `Element.content` (`Vec<Block>`, see `tmtroot/docs/spec/
/// feature/content-shape.tmt`) back to source text.
///
/// The common case -- exactly one plain paragraph, which is what ordinary
/// inline usage (`@link(...)[Tomet]`, `@em[text]`, ...) always parses to --
/// prints byte-for-byte what `render_inlines` on that paragraph's own
/// content always has, so this is not a behaviour change for anything that
/// doesn't actually use the new block-permitting content. Multiple blocks,
/// or a block that isn't a paragraph, fall back to `render_block` per
/// block, the same joining `render_section` already uses for its own
/// `blocks` (no blank line between them -- see `.agents/tasks/
/// man-import-and-printer-gaps.md`'s note on that, which this doesn't
/// change).
pub(crate) fn render_content_blocks(blocks: &[Block], config: &PrinterConfig) -> String {
    if let [Block::Paragraph(p)] = blocks {
        return render_inlines(&p.content, config);
    }
    let mut out = String::new();
    for block in blocks {
        crate::block::render_block(block, config, &mut out);
    }
    out
}
