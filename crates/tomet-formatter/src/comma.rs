//! Formatting optional line-ending and trailing commas in multi-line `(...)` groups
//! according to `map.multiline_comma`.

use tomet_ast_parser::parse_file;
use tomet_config::PrinterConfig;
use tomet_cst::{SyntaxElement, SyntaxKind as K, SyntaxNode, SyntaxToken};
use tomet_cst_parser::parse_cst;
use tomet_style::MultilineComma;

use crate::clean::collect_raw_spans;
use crate::edit::{Edit, apply_keeping_the_document, is_formattable_args_group};

/// Formats optional line-ending and trailing commas in multi-line `(...)` groups according to
/// `map.multiline_comma` (`never`, `between`, or `always`). Unset, `src` is returned unchanged.
///
/// Only `(...)` groups (`K::ARGS`) that span multiple lines (at least one direct newline inside
/// the group) and hold at least one entry are formatted. Single-line groups, same-line separators
/// between entries on the same line (`a: 1, b: 2`), `|` pipe content values, raw/fenced code
/// blocks, interpolation expressions, `$name(...)`, `<$type(...)>`, and error recovery nodes are
/// preserved untouched.
///
/// AST equivalence is verified after editing: if re-parsing fails or changes the `File`, `src` is
/// returned unchanged.
pub fn format_multiline_commas(src: &str, config: &PrinterConfig) -> String {
    let Some(style) = config.format.map_multiline_comma else {
        return src.to_string();
    };
    let Ok(doc) = parse_file(src) else {
        return src.to_string();
    };

    let mut raw_spans = Vec::new();
    collect_raw_spans(&doc, &mut raw_spans);

    let root = parse_cst(src);
    let mut edits = Vec::new();
    for node in root.descendants() {
        if node.kind() != K::ARGS || !is_formattable_args_group(&node) {
            continue;
        }
        let start = usize::from(node.text_range().start());
        if raw_spans.iter().any(|&(s, e)| start >= s && start < e) {
            continue;
        }
        collect_group_edits(&node, style, &mut edits);
    }

    if edits.is_empty() {
        return src.to_string();
    }

    edits.sort_by_key(|(start, end, _)| (*start, *end));
    apply_keeping_the_document(src, edits).unwrap_or_else(|| src.to_string())
}

fn collect_group_edits(args: &SyntaxNode, style: MultilineComma, edits: &mut Vec<Edit>) {
    if args.first_token().is_none_or(|t| t.kind() != K::L_PAREN)
        || args.last_token().is_none_or(|t| t.kind() != K::R_PAREN)
    {
        return;
    }
    if args
        .children()
        .any(|c| matches!(c.kind(), K::ERROR | K::TOO_DEEP))
    {
        return;
    }
    if !args
        .children_with_tokens()
        .any(|el| el.kind() == K::NEWLINE)
    {
        return;
    }

    let mut groups: Vec<(SyntaxNode, Vec<SyntaxElement>)> = Vec::new();
    for el in args.children_with_tokens() {
        match &el {
            SyntaxElement::Node(n) if matches!(n.kind(), K::MAP_ENTRY | K::SEQ_ITEM) => {
                groups.push((n.clone(), Vec::new()));
            }
            _ => {
                if let Some((_, between)) = groups.last_mut() {
                    if el.kind() == K::R_PAREN {
                        break;
                    }
                    between.push(el);
                }
            }
        }
    }

    let total = groups.len();
    if total == 0 {
        return;
    }

    for (i, (entry, between)) in groups.iter().enumerate() {
        let has_newline_after = between.iter().any(|el| el.kind() == K::NEWLINE);
        let is_same_line_separator = i + 1 < total && !has_newline_after;
        let want_comma = is_same_line_separator || style.trailing_for(i, total);

        let commas: Vec<SyntaxToken> = between
            .iter()
            .filter_map(|el| el.as_token().filter(|t| t.kind() == K::COMMA).cloned())
            .collect();

        if want_comma {
            if commas.is_empty()
                && !entry_ends_with_pipe(entry)
                && let Some(pos) = last_non_trivia_end(entry)
            {
                edits.push((pos, pos, ",".to_string()));
            }
        } else if !commas.is_empty() {
            collect_comma_removals(entry, between, &commas, edits);
        }
    }
}

fn collect_comma_removals(
    entry: &SyntaxNode,
    between: &[SyntaxElement],
    commas: &[SyntaxToken],
    edits: &mut Vec<Edit>,
) {
    for (idx, comma) in commas.iter().enumerate() {
        let comma_start = usize::from(comma.text_range().start());
        let comma_end = usize::from(comma.text_range().end());
        let mut remove_start = comma_start;

        // Absorb horizontal whitespace immediately before the first comma when it sits directly
        // on the entry's line (either as the last token of `entry` or before the comma in
        // `between`, never indentation after a newline).
        if idx == 0
            && let Some(pos_in_between) = between.iter().position(|el| {
                el.as_token()
                    .is_some_and(|t| t.text_range() == comma.text_range())
            })
        {
            if pos_in_between > 0
                && let Some(prev_ws) = between[pos_in_between - 1]
                    .as_token()
                    .filter(|t| t.kind() == K::WHITESPACE)
            {
                let preceded_by_newline =
                    pos_in_between >= 2 && between[pos_in_between - 2].kind() == K::NEWLINE;
                if !preceded_by_newline {
                    remove_start = usize::from(prev_ws.text_range().start());
                }
            } else if pos_in_between == 0
                && let Some(trailing_ws) = entry.last_token().filter(|t| t.kind() == K::WHITESPACE)
            {
                remove_start = usize::from(trailing_ws.text_range().start());
            }
        }

        edits.push((remove_start, comma_end, String::new()));
    }
}

fn last_non_trivia(entry: &SyntaxNode) -> Option<SyntaxElement> {
    entry
        .children_with_tokens()
        .filter(|el| !el.kind().is_trivia())
        .last()
}

fn last_non_trivia_end(entry: &SyntaxNode) -> Option<usize> {
    last_non_trivia(entry).map(|el| usize::from(el.text_range().end()))
}

fn entry_ends_with_pipe(entry: &SyntaxNode) -> bool {
    last_non_trivia(entry)
        .and_then(|el| el.into_node())
        .is_some_and(|node| {
            node.kind() == K::CONTENT && node.first_token().is_some_and(|t| t.kind() == K::PIPE)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tomet_config::FormatConfig;

    fn formatted(style: MultilineComma, src: &str) -> String {
        let config = PrinterConfig {
            format: FormatConfig {
                map_multiline_comma: Some(style),
                ..Default::default()
            },
            ..Default::default()
        };
        let out = format_multiline_commas(src, &config);
        assert_eq!(
            parse_file(&out).unwrap(),
            parse_file(src).unwrap(),
            "changed the document: {out:?}"
        );
        assert_eq!(
            format_multiline_commas(&out, &config),
            out,
            "not idempotent: {out:?}"
        );
        out
    }

    #[test]
    fn unset_returns_source_unchanged() {
        let src = "::format(\n  a: 1,\n  b: 2\n)\n";
        assert_eq!(format_multiline_commas(src, &PrinterConfig::default()), src);
    }

    #[test]
    fn formats_never_between_and_always_on_maps_and_sequences() {
        let src =
            "::meta(\n  a: 1,\n  b: 2\n  c: (x, y,)\n)\n\n@tags(\n  one\n  two,\n  three,\n)\n";
        assert_eq!(
            formatted(MultilineComma::Never, src),
            "::meta(\n  a: 1\n  b: 2\n  c: (x, y,)\n)\n\n@tags(\n  one\n  two\n  three\n)\n"
        );
        assert_eq!(
            formatted(MultilineComma::Between, src),
            "::meta(\n  a: 1,\n  b: 2,\n  c: (x, y,)\n)\n\n@tags(\n  one,\n  two,\n  three\n)\n"
        );
        assert_eq!(
            formatted(MultilineComma::Always, src),
            "::meta(\n  a: 1,\n  b: 2,\n  c: (x, y,),\n)\n\n@tags(\n  one,\n  two,\n  three,\n)\n"
        );
    }

    #[test]
    fn single_line_empty_and_hugging_groups_are_untouched() {
        for src in [
            "::meta(a: 1, b: 2)\n",
            "::meta()\n",
            "::meta(\n)\n",
            "::meta(\n  // only a comment\n)\n",
            "::meta(s: \"line1\\nline2\")\n",
            "::meta(a: (\n  x: 1\n))\n",
        ] {
            for style in MultilineComma::ALL {
                let out = formatted(style, src);
                if src == "::meta(a: (\n  x: 1\n))\n" && style == MultilineComma::Always {
                    assert_eq!(out, "::meta(a: (\n  x: 1,\n))\n");
                } else {
                    assert_eq!(out, src, "{style:?} altered {src:?}");
                }
            }
        }
    }

    #[test]
    fn preserves_same_line_comments_and_standalone_comments() {
        let no_commas = "\
::meta(
  a: 1 // first
  // middle note
  b: 2 /* block */
  c: 3 // last
)
";
        let with_all_commas = "\
::meta(
  a: 1, // first
  // middle note
  b: 2, /* block */
  c: 3, // last
)
";
        let with_between_commas = "\
::meta(
  a: 1, // first
  // middle note
  b: 2, /* block */
  c: 3 // last
)
";
        assert_eq!(
            formatted(MultilineComma::Always, no_commas),
            with_all_commas
        );
        assert_eq!(
            formatted(MultilineComma::Between, no_commas),
            with_between_commas
        );
        assert_eq!(formatted(MultilineComma::Never, with_all_commas), no_commas);
        // Trailing space before a removed comma is also cleaned up.
        assert_eq!(
            formatted(
                MultilineComma::Never,
                "::meta(\n  a: 1 , // note\n  b: 2\n)\n"
            ),
            "::meta(\n  a: 1 // note\n  b: 2\n)\n"
        );
    }

    #[test]
    fn preserves_required_same_line_separators_inside_multiline_groups() {
        let src = "::format(\n  a: 1, b: 2\n  c: 3,\n)\n";
        assert_eq!(
            formatted(MultilineComma::Never, src),
            "::format(\n  a: 1, b: 2\n  c: 3\n)\n"
        );
        assert_eq!(
            formatted(MultilineComma::Between, src),
            "::format(\n  a: 1, b: 2,\n  c: 3\n)\n"
        );
        assert_eq!(
            formatted(MultilineComma::Always, src),
            "::format(\n  a: 1, b: 2,\n  c: 3,\n)\n"
        );
    }

    #[test]
    fn formats_nested_groups_and_connects_and_holes() {
        let src = "\
::format(
  element: (
    callout: (
      style: (
        content: block
      )
    )
  )
)

@card(
  a: 1
)::meta(
  b: 2
)

<?matcher(
  x: 1
  y: 2
)>
";
        assert_eq!(
            formatted(MultilineComma::Always, src),
            "\
::format(
  element: (
    callout: (
      style: (
        content: block,
      ),
    ),
  ),
)

@card(
  a: 1,
)::meta(
  b: 2,
)

<?matcher(
  x: 1,
  y: 2,
)>
"
        );
        assert_eq!(
            formatted(MultilineComma::Between, src),
            "\
::format(
  element: (
    callout: (
      style: (
        content: block
      )
    )
  )
)

@card(
  a: 1
)::meta(
  b: 2
)

<?matcher(
  x: 1,
  y: 2
)>
"
        );
    }

    #[test]
    fn excludes_raw_code_blocks_interpolations_type_seats_and_errors() {
        let src = "\
```tmt
::meta(
  a: 1
  b: 2
)
```

@raw[
fn f(
  a: 1
  b: 2
)
]

@memo[
```tmt
@foo(
  a: 1
  b: 2
)
```
]

$fn(
  a,
  b
)

<$enum(
  one,
  two
)>

{$ (
  1,
  2
) }
";
        for style in MultilineComma::ALL {
            assert_eq!(formatted(style, src), src, "{style:?}");
        }

        let invalid = "::meta(\n  a: 1\n  b: 2\n";
        let config = PrinterConfig {
            format: FormatConfig {
                map_multiline_comma: Some(MultilineComma::Always),
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(format_multiline_commas(invalid, &config), invalid);
    }

    #[test]
    fn skips_pipe_content_entries() {
        let src = "::meta(\n  about: | One rule.\n  next: 1\n)\n";
        assert_eq!(
            formatted(MultilineComma::Always, src),
            "::meta(\n  about: | One rule.\n  next: 1,\n)\n"
        );
    }
}
