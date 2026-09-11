//! Bidirectional conversion between CommonMark and `tomet_ast`.
//!
//! The mapping and its known-lossy cases are documented where they are
//! implemented: `import`'s module doc for Markdown -> Tomet, `export`'s
//! for the other direction.

mod export;
mod import;

pub use export::to_markdown;
pub use import::{ImportOptions, from_markdown, from_markdown_with_options};

#[cfg(test)]
mod roundtrip_tests {
    use super::*;
    use tomet_html::render_body;

    /// Round-trip through both directions and assert the *rendered HTML*
    /// is equivalent, which is the fidelity bar this crate targets -- the
    /// intermediate `.tmt` shape doesn't have to be identical, just render
    /// the same.
    fn assert_html_round_trips(markdown: &str) {
        let doc = from_markdown(markdown);
        let re_markdown = to_markdown(&doc);
        let doc2 = from_markdown(&re_markdown);
        assert_eq!(
            render_body(&doc),
            render_body(&doc2),
            "markdown was:\n{markdown}\nre-emitted as:\n{re_markdown}"
        );
    }

    #[test]
    fn prose_with_inline_formatting_round_trips() {
        assert_html_round_trips(
            "# Title\n\nSome *emphasis* and **strong** and a [link](https://example.com/x).\n",
        );
    }

    #[test]
    fn lists_round_trip() {
        assert_html_round_trips("- one\n- two\n- three\n");
        assert_html_round_trips("1. one\n2. two\n");
    }

    #[test]
    fn code_block_round_trips() {
        assert_html_round_trips("```rust\nfn main() {\n    println!(\"hi\");\n}\n```\n");
    }

    #[test]
    fn image_round_trips() {
        assert_html_round_trips("![a cat](assets/pic.png)\n");
    }

    #[test]
    fn thematic_break_round_trips() {
        assert_html_round_trips("above\n\n---\n\nbelow\n");
    }

    #[test]
    fn single_paragraph_blockquote_round_trips() {
        assert_html_round_trips("> quoted text\n");
    }

    #[test]
    fn table_round_trips() {
        assert_html_round_trips("| col1 | col2 |\n| --- | --- |\n| val1 | val2 |\n");
    }
}
