//! The spaces inside the brackets of a heading's title (`heading.space_inside_brackets`) and after
//! the colon of a `target` that is the only argument of an element (`link.no_space`).

use tomet_config::PrinterConfig;
use tomet_cst::{SyntaxElement, SyntaxKind as K, SyntaxNode};
use tomet_cst_parser::parse_cst;
use tomet_style::{BracketSpacing, ColonSpacing};

use crate::edit::{Edit, apply_keeping_the_document};

/// Spaces the title of each heading written between brackets (`=[ title ]`) the way
/// `heading.space_inside_brackets` asks. Unset, `src` is returned as written; so is a heading that
/// is not between brackets (`== title`), one with no title, and one whose title runs over lines.
///
/// Only the white space inside the brackets changes, and that is checked, not assumed.
pub fn format_heading_brackets(src: &str, config: &PrinterConfig) -> String {
    let Some(spacing) = config.format.heading_brackets else {
        return src.to_string();
    };
    let root = parse_cst(src);
    let edits: Vec<Edit> = root
        .descendants()
        .filter(|node| node.kind() == K::SECTION_HEADING)
        .filter_map(|heading| heading_edit(&heading, spacing))
        .collect();
    apply(src, edits)
}

fn heading_edit(heading: &SyntaxNode, spacing: BracketSpacing) -> Option<Edit> {
    let content = heading
        .children()
        .find(|child| child.kind() == K::CONTENT)?;
    let text = content.text().to_string();
    let title = text.strip_prefix('[')?.strip_suffix(']')?;
    if title.contains('\n') || title.trim().is_empty() {
        return None;
    }
    let spaced = spacing.wrap(title.trim());
    let range = content.text_range();
    (spaced != text).then(|| (usize::from(range.start()), usize::from(range.end()), spaced))
}

/// Spaces the colon of each `target` that is the only argument of an element (`@link(target: x)`)
/// the way `link.no_space` asks.
/// Unset, `src` is returned as written; so is a `target` whose value is on the next line.
///
/// Only the white space after the colon changes, and that is checked, not assumed.
pub fn format_link_targets(src: &str, config: &PrinterConfig) -> String {
    let Some(spacing) = config.format.link_target_colon else {
        return src.to_string();
    };
    let root = parse_cst(src);
    let edits: Vec<Edit> = root
        .descendants()
        .filter(|node| node.kind() == K::ARGS)
        .filter_map(|args| target_edit(&args, spacing))
        .collect();
    apply(src, edits)
}

/// The change to the space after the colon of the only entry of `args`, when it is `target` and
/// `args` are an element's.
fn target_edit(args: &SyntaxNode, spacing: ColonSpacing) -> Option<Edit> {
    let element = args.parent()?;
    if !matches!(element.kind(), K::BLOCK_FORM | K::INLINE_FORM) {
        return None;
    }
    let is_at = element
        .children()
        .find(|child| child.kind() == K::SIGIL)?
        .first_token()
        .is_some_and(|token| token.kind() == K::AT);
    let mut entries = args.children();
    let entry = entries.next()?;
    if !is_at || entries.next().is_some() || entry.kind() != K::MAP_ENTRY {
        return None;
    }
    let mut parts = entry.children_with_tokens();
    let key = parts.next()?;
    if key.kind() != K::IDENT || key.as_token()?.text() != "target" {
        return None;
    }
    let colon = parts.next()?;
    if colon.kind() != K::COLON {
        return None;
    }
    let colon_end = usize::from(colon.text_range().end());
    let (end, current) = match parts.next()? {
        // The value is on the next line: nothing to space.
        SyntaxElement::Token(token) if token.kind() == K::NEWLINE => return None,
        SyntaxElement::Token(token) if token.kind() == K::WHITESPACE => (
            usize::from(token.text_range().end()),
            token.text().to_string(),
        ),
        _ => (colon_end, String::new()),
    };
    if current.contains('\n') {
        return None;
    }
    let wanted = spacing.after_colon();
    (current != wanted).then(|| (colon_end, end, wanted.to_string()))
}

fn apply(src: &str, edits: Vec<Edit>) -> String {
    if edits.is_empty() {
        return src.to_string();
    }
    apply_keeping_the_document(src, edits).unwrap_or_else(|| src.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headings(spacing: bool, src: &str) -> String {
        let mut config = PrinterConfig::default();
        config.format.heading_brackets = Some(if spacing {
            BracketSpacing::Padded
        } else {
            BracketSpacing::Tight
        });
        format_heading_brackets(src, &config)
    }

    fn links(tight: bool, src: &str) -> String {
        let mut config = PrinterConfig::default();
        config.format.link_target_colon = Some(if tight {
            ColonSpacing::Tight
        } else {
            ColonSpacing::Spaced
        });
        format_link_targets(src, &config)
    }

    #[test]
    fn unset_the_source_is_returned_as_written() {
        let config = PrinterConfig::default();
        let src = "=[title]\n\n@link(target:\"a\")[T]\n";
        assert_eq!(format_heading_brackets(src, &config), src);
        assert_eq!(format_link_targets(src, &config), src);
    }

    #[test]
    fn a_heading_is_padded_or_tight() {
        assert_eq!(headings(true, "=[title]\n"), "=[ title ]\n");
        assert_eq!(headings(true, "==[  title  ]\n"), "==[ title ]\n");
        assert_eq!(headings(false, "=[ title ]\n"), "=[title]\n");
        assert_eq!(headings(false, "=[title]\n"), "=[title]\n");
    }

    #[test]
    fn a_heading_that_is_not_between_brackets_is_left_alone() {
        for src in ["== title\n", "=[]\n", "=[ ]\n", "=[ a\nb ]\n"] {
            assert_eq!(headings(true, src), src, "{src:?}");
            assert_eq!(headings(false, src), src, "{src:?}");
        }
    }

    #[test]
    fn a_link_target_is_spaced_or_tight() {
        assert_eq!(
            links(true, "@link(target: \"a\")[T]\n"),
            "@link(target:\"a\")[T]\n"
        );
        assert_eq!(
            links(false, "@link(target:\"a\")[T]\n"),
            "@link(target: \"a\")[T]\n"
        );
        assert_eq!(
            links(false, "see @link(target:   \"a\") here\n"),
            "see @link(target: \"a\") here\n"
        );
    }

    #[test]
    fn only_a_target_alone_in_an_elements_args_is_touched() {
        for src in [
            "@link(target:\"a\", title: \"t\")[T]\n",
            "@link(title: \"t\")[T]\n",
            "::x(target:\"a\")\n",
            "@link(\n  target:\n    \"a\"\n)[T]\n",
        ] {
            assert_eq!(links(false, src), src, "{src:?}");
        }
    }
}
