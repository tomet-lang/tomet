//! Collapsing consecutive standalone closing `)` lines of nested `(...)` groups according to
//! `map.collapse_closing_parens`.

use std::collections::HashSet;

use tomet_ast_parser::parse_file;
use tomet_config::PrinterConfig;
use tomet_cst::SyntaxKind as K;
use tomet_cst_parser::parse_cst;

use crate::clean::collect_raw_spans;
use crate::edit::{Edit, apply_keeping_the_document, is_formattable_args_group};

/// Collapses consecutive standalone closing `)` lines of nested `(...)` groups onto a single
/// standalone line (`))))`), aligned to the leading whitespace of the outermost closing `)` line,
/// when `map.collapse_closing_parens` is on; off, `src` is returned unchanged.
///
/// Only runs of two or more consecutive lines where each line holds solely horizontal whitespace
/// and a single closing `)` of a `(...)` group (`Args`, `Tuple`, `Call`, etc.) are collapsed.
/// Blank lines, comments, commas, other tokens, `[...]`, `{...}`, string literals, and raw/fenced
/// code blocks are never altered.
///
/// AST equivalence is verified after editing: if re-parsing fails or changes the `File`, `src` is
/// returned unchanged.
pub fn format_closing_parens(src: &str, config: &PrinterConfig) -> String {
    if !config.format.map_collapse_closing_parens {
        return src.to_string();
    }
    let Ok(doc) = parse_file(src) else {
        return src.to_string();
    };

    let mut raw_spans = Vec::new();
    collect_raw_spans(&doc, &mut raw_spans);

    let r_paren_offsets = collect_closing_paren_offsets(src, &raw_spans);
    if r_paren_offsets.len() < 2 {
        return src.to_string();
    }

    let edits = collect_collapse_edits(src, &r_paren_offsets);
    if edits.is_empty() {
        return src.to_string();
    }

    apply_keeping_the_document(src, edits).unwrap_or_else(|| src.to_string())
}

fn collect_closing_paren_offsets(src: &str, raw_spans: &[(usize, usize)]) -> HashSet<usize> {
    let root = parse_cst(src);
    let mut offsets = HashSet::new();
    for node in root.descendants() {
        if node.kind() != K::ARGS || !is_formattable_args_group(&node) {
            continue;
        }
        let Some(open) = node.first_token().filter(|tok| tok.kind() == K::L_PAREN) else {
            continue;
        };
        let Some(close) = node.last_token().filter(|tok| tok.kind() == K::R_PAREN) else {
            continue;
        };
        let _ = open;
        let offset = usize::from(close.text_range().start());
        if !raw_spans
            .iter()
            .any(|&(start, end)| offset >= start && offset < end)
        {
            offsets.insert(offset);
        }
    }
    offsets
}

struct ClosingLine<'a> {
    start: usize,
    end: usize,
    indent: &'a str,
}

fn collect_collapse_edits(src: &str, r_paren_offsets: &HashSet<usize>) -> Vec<Edit> {
    let mut edits = Vec::new();
    let mut run: Vec<ClosingLine<'_>> = Vec::new();
    let mut line_start = 0;

    for raw_line in src.split('\n') {
        let body = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        let trimmed_start = body.trim_start_matches([' ', '\t']);
        let indent_len = body.len() - trimmed_start.len();
        let is_standalone = trimmed_start.trim_end_matches([' ', '\t']) == ")"
            && r_paren_offsets.contains(&(line_start + indent_len));

        if is_standalone {
            run.push(ClosingLine {
                start: line_start,
                end: line_start + body.len(),
                indent: &body[..indent_len],
            });
        } else {
            flush_run(&mut run, &mut edits);
        }

        line_start += raw_line.len() + 1;
    }
    flush_run(&mut run, &mut edits);
    edits
}

fn flush_run(run: &mut Vec<ClosingLine<'_>>, edits: &mut Vec<Edit>) {
    if run.len() >= 2 {
        let first = &run[0];
        let last = &run[run.len() - 1];
        let collapsed = format!("{}{}", last.indent, ")".repeat(run.len()));
        edits.push((first.start, last.end, collapsed));
    }
    run.clear();
}

#[cfg(test)]
mod tests {
    use super::*;
    use tomet_config::FormatConfig;

    fn collapsed(src: &str) -> String {
        let config = PrinterConfig {
            format: FormatConfig {
                map_collapse_closing_parens: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let out = format_closing_parens(src, &config);
        assert_eq!(
            parse_file(&out).unwrap(),
            parse_file(src).unwrap(),
            "changed the document: {out:?}"
        );
        assert_eq!(
            format_closing_parens(&out, &config),
            out,
            "not idempotent: {out:?}"
        );
        out
    }

    #[test]
    fn disabled_by_default_returns_source_unchanged() {
        let src = "::format(\n  a: (\n    b: 1\n  )\n)\n";
        assert_eq!(format_closing_parens(src, &PrinterConfig::default()), src);
    }

    #[test]
    fn collapses_nested_maps_to_outermost_indent() {
        let src = "\
::format(
  element: (
    table: (adjust_width: auto)
    callout: (
      style: (
        content: block
      )
    )
  )
)
";
        let want = "\
::format(
  element: (
    table: (adjust_width: auto)
    callout: (
      style: (
        content: block
))))
";
        assert_eq!(collapsed(src), want);
    }

    #[test]
    fn collapses_indented_maps_inside_sections_and_lists() {
        let src = "\
=[ Config ]

- [
    @card(
      meta: (
        nested: (
          a: 1
        )
      )
    )
  ]
";
        let want = "\
=[ Config ]

- [
    @card(
      meta: (
        nested: (
          a: 1
    )))
  ]
";
        assert_eq!(collapsed(src), want);
    }

    #[test]
    fn comments_commas_and_blank_lines_break_a_run() {
        let with_comment_between = "\
::format(
  a: (
    b: 1
  )
  // middle
)
";
        assert_eq!(collapsed(with_comment_between), with_comment_between);

        let with_trailing_comment = "\
::format(
  a: (
    b: 1
  ) // end a
)
";
        assert_eq!(collapsed(with_trailing_comment), with_trailing_comment);

        let with_comma = "\
::format(
  a: (
    b: 1
  ),
)
";
        assert_eq!(collapsed(with_comma), with_comma);

        let with_blank_line = "\
::format(
  a: (
    b: 1
  )

)
";
        assert_eq!(collapsed(with_blank_line), with_blank_line);

        // Run after a comment still collapses the consecutive tail.
        let partial_break = "\
::format(
  a: (
    b: (
      c: 1
    )
    // comment
  )
)
";
        let want_partial = "\
::format(
  a: (
    b: (
      c: 1
    )
    // comment
))
";
        assert_eq!(collapsed(partial_break), want_partial);
    }

    #[test]
    fn leaves_brackets_braces_strings_and_code_blocks_untouched() {
        let brackets_and_braces = "\
::meta(a: 1){
  b: 2
}

@box[
  @inner[
    hello
  ]
]{
  x: 1
}
";
        assert_eq!(collapsed(brackets_and_braces), brackets_and_braces);

        // Nested `(...)` inside `{...}` still collapses before `}`.
        let nested_in_braces = "\
::meta(a: 1){
  b: (
    c: (
      d: 1
    )
  )
}
";
        let want_nested_in_braces = "\
::meta(a: 1){
  b: (
    c: (
      d: 1
  ))
}
";
        assert_eq!(collapsed(nested_in_braces), want_nested_in_braces);

        let fenced = "\
```tmt
::format(
  a: (
    b: 1
  )
)
```
";
        assert_eq!(collapsed(fenced), fenced);

        let raw_element = "\
@raw[
(
  )
)
]

@memo[
```tmt
@foo(
  a: (
    b: 1
  )
)
```
]
";
        assert_eq!(collapsed(raw_element), raw_element);

        let string_literal = "\
::meta(
  s: \"
    )
  )
\"
)
";
        assert_eq!(collapsed(string_literal), string_literal);

        let prose_parens = "\
@box[
  (
    )
  )
]
";
        assert_eq!(collapsed(prose_parens), prose_parens);
    }

    #[test]
    fn already_collapsed_parens_are_idempotent() {
        let src = "\
::format(
  element: (
    callout: (
      style: (
        content: block
))))
";
        assert_eq!(collapsed(src), src);

        let partial = "\
::format(
  element: (
    callout: (
      style: (
        content: block
      )))
)
";
        assert_eq!(collapsed(partial), partial);
    }

    #[test]
    fn single_standalone_paren_is_untouched() {
        let src = "::format(\n  a: 1\n)\n";
        assert_eq!(collapsed(src), src);
    }

    #[test]
    fn invalid_source_is_returned_unchanged() {
        let src = "::format(\n  a: (\n    b: 1\n";
        let config = PrinterConfig {
            format: FormatConfig {
                map_collapse_closing_parens: true,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(format_closing_parens(src, &config), src);
    }
}
