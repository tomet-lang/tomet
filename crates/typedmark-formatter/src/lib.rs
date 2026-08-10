//! Whitespace-hygiene formatter for `.tm` source text.
//!
//! This is intentionally *not* a full AST-reprint (gofmt/prettier style):
//! `typedmark_ast::Document` carries no span/position info, so a lossless
//! reprint isn't possible today, and a hand-rolled duplicate of the parser's
//! grammar walk just to preserve incidental spacing isn't worth the
//! maintenance cost yet. Instead this only normalizes the whitespace policy
//! the repo's own `.editorconfig` already declares for every file (`[*]`:
//! LF line endings, no trailing whitespace, exactly one final newline), plus
//! collapsing excess blank lines. All of these are no-ops as far as
//! `typedmark_parser::parse_document` is concerned -- see the crate tests.

/// Format `src` in place (returns a new `String`). Idempotent:
/// `format_source(&format_source(src)) == format_source(src)`.
pub fn format_source(src: &str) -> String {
    let normalized = src.replace("\r\n", "\n").replace('\r', "\n");

    let mut out_lines: Vec<&str> = Vec::new();
    let mut pending_blank = false;
    for line in normalized.split('\n') {
        let trimmed = line.trim_end_matches([' ', '\t']);
        if trimmed.is_empty() {
            pending_blank = true;
            continue;
        }
        if pending_blank && !out_lines.is_empty() {
            out_lines.push("");
        }
        pending_blank = false;
        out_lines.push(trimmed);
    }

    if out_lines.is_empty() {
        return String::new();
    }
    let mut out = out_lines.join("\n");
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_trailing_whitespace() {
        assert_eq!(format_source("a  \nb\t\n"), "a\nb\n");
    }

    #[test]
    fn normalizes_line_endings() {
        assert_eq!(format_source("a\r\nb\r\n"), "a\nb\n");
        assert_eq!(format_source("a\rb\r"), "a\nb\n");
    }

    #[test]
    fn drops_leading_blank_lines() {
        assert_eq!(format_source("\n\n\na\n"), "a\n");
    }

    #[test]
    fn collapses_multiple_blank_lines_to_one() {
        assert_eq!(format_source("a\n\n\n\nb\n"), "a\n\nb\n");
    }

    #[test]
    fn drops_trailing_blank_lines_and_ensures_one_final_newline() {
        assert_eq!(format_source("a\n\n\n"), "a\n");
        assert_eq!(format_source("a"), "a\n");
    }

    #[test]
    fn empty_input_stays_empty() {
        assert_eq!(format_source(""), "");
        assert_eq!(format_source("\n\n  \n"), "");
    }

    #[test]
    fn already_formatted_input_is_unchanged() {
        let src = "#[ Hello ]\n\n- one\n- two\n";
        assert_eq!(format_source(src), src);
    }

    #[test]
    fn is_idempotent_on_repo_spec_examples() {
        for src in repo_examples() {
            let once = format_source(src);
            let twice = format_source(&once);
            assert_eq!(once, twice, "formatting is not idempotent for: {src:?}");
        }
    }

    #[test]
    fn does_not_change_the_parsed_document() {
        // Only fixtures that already parse under the current grammar --
        // `docs/cheatsheet.tm` has placeholder empty groups (e.g.
        // `<myfunc>()[]{}`) the parser doesn't accept yet, which is a
        // pre-existing grammar gap unrelated to formatting.
        for src in [
            include_str!("../../../docs/tmt/typedmark.tm"),
            include_str!("../../../docs/tmt/examples/image_meta.tm"),
            include_str!("../../../docs/roadmap.ja.tm"),
        ] {
            let before = typedmark_parser::parse_document(src)
                .unwrap_or_else(|e| panic!("fixture failed to parse: {e}"));
            let formatted = format_source(src);
            let after = typedmark_parser::parse_document(&formatted)
                .unwrap_or_else(|e| panic!("formatted output failed to parse: {e}"));
            assert_eq!(before, after, "formatting changed the parsed document");
        }
    }

    fn repo_examples() -> Vec<&'static str> {
        vec![
            include_str!("../../../docs/tmt/typedmark.tm"),
            include_str!("../../../docs/tmt/examples/image_meta.tm"),
            include_str!("../../../docs/cheatsheet.tm"),
            include_str!("../../../docs/roadmap.ja.tm"),
        ]
    }
}
