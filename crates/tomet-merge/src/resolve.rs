//! Taking `@conflict` markers out of a document by choosing a side.
//!
//! A marker is replaced where it stands, by splicing the chosen side's text
//! into its span: everything outside the spans of the resolved markers is
//! copied from the source byte for byte, and the side is what was written,
//! not a rendering of it.

use crate::conflict::{ConflictError, ConflictKind, Found, Side, bracketed_inner, scan};
use crate::markers::{
    extract_conflict_block_side, extract_conflict_inline_side, unquote_scalar_display,
};

/// Which side of one `@conflict` a caller picked, or that they edited a
/// fresh replacement by hand, or kept both versions. `A`/`B` match
/// `@conflict`'s own structural, non-perspectival names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConflictResolution {
    A,
    B,
    /// Both sides, one after the other. In running text they are joined by a
    /// space unless one already has white space at the seam; blocks are
    /// separated by a blank line; two string values become one, `"a / b"`.
    Both,
    /// The user wrote something new that is neither side verbatim.
    Rewritten(String),
}

/// Resolves the markers of `src` in document order, the first marker by the
/// first resolution and so on. Markers past the end of `resolutions` stay.
pub fn resolve(src: &str, resolutions: &[ConflictResolution]) -> Result<String, ConflictError> {
    apply(src, |i| resolutions.get(i))
}

/// Resolves every marker of `src` the same way.
pub fn resolve_all(src: &str, resolution: ConflictResolution) -> Result<String, ConflictError> {
    apply(src, |_| Some(&resolution))
}

/// Resolves marker number `target_index` of `src`; the others stay.
pub fn resolve_single(
    src: &str,
    target_index: usize,
    resolution: ConflictResolution,
) -> Result<String, ConflictError> {
    let count = scan(src)?.len();
    if target_index >= count {
        return Err(ConflictError::IndexOutOfBounds {
            index: target_index,
            count,
        });
    }
    apply(src, |i| (i == target_index).then_some(&resolution))
}

fn apply<'a>(
    src: &str,
    pick: impl Fn(usize) -> Option<&'a ConflictResolution>,
) -> Result<String, ConflictError> {
    let found = scan(src)?;
    let mut out = String::with_capacity(src.len());
    let mut at = 0;
    for (index, marker) in found.iter().enumerate() {
        let Some(resolution) = pick(index) else {
            continue;
        };
        let span = &marker.item.span;
        let (text, eat_newline) = replacement(src, marker, resolution);
        out.push_str(&src[at..span.start]);
        out.push_str(&text);
        at = span.end;
        if eat_newline {
            // A block ends in a newline of its own; the marker's was spent.
            at += newline_len(&src[at..]);
        }
    }
    out.push_str(&src[at..]);
    Ok(out)
}

fn newline_len(s: &str) -> usize {
    if s.starts_with("\r\n") {
        2
    } else {
        usize::from(s.starts_with('\n'))
    }
}

/// The text that takes the marker's place, and whether the newline after the
/// marker goes with it.
fn replacement(src: &str, marker: &Found, resolution: &ConflictResolution) -> (String, bool) {
    match marker.item.kind {
        ConflictKind::Inline => (inline(marker, resolution), false),
        ConflictKind::Value => (value(marker, resolution), false),
        ConflictKind::Block => {
            let text = block(src, marker, resolution);
            let eat = !text.is_empty();
            (text, eat)
        }
    }
}

fn blocks_of(side: &Side) -> &str {
    match side {
        Side::Blocks(inner) => inner,
        Side::Value(raw) => raw,
    }
}

fn inline(marker: &Found, resolution: &ConflictResolution) -> String {
    let a = extract_conflict_inline_side(blocks_of(&marker.a));
    let b = extract_conflict_inline_side(blocks_of(&marker.b));
    match resolution {
        ConflictResolution::A => a,
        ConflictResolution::B => b,
        ConflictResolution::Both => {
            if a.is_empty() {
                b
            } else if b.is_empty() {
                a
            } else if a.ends_with(char::is_whitespace) || b.starts_with(char::is_whitespace) {
                format!("{a}{b}")
            } else {
                format!("{a} {b}")
            }
        }
        ConflictResolution::Rewritten(text) => text.clone(),
    }
}

fn block(src: &str, marker: &Found, resolution: &ConflictResolution) -> String {
    let a = extract_conflict_block_side(blocks_of(&marker.a));
    let b = extract_conflict_block_side(blocks_of(&marker.b));
    let text = match resolution {
        ConflictResolution::A => a,
        ConflictResolution::B => b,
        ConflictResolution::Both => {
            if a.is_empty() {
                b
            } else if b.is_empty() {
                a
            } else {
                format!("{a}\n{b}")
            }
        }
        ConflictResolution::Rewritten(text) if text.ends_with('\n') => text.clone(),
        ConflictResolution::Rewritten(text) => format!("{text}\n"),
    };
    indent_after_first_line(src, marker.item.span.start, &text)
}

/// The marker may stand indented (inside an element's `[ ]`); the lines of a
/// block that follow its first line are indented to match.
fn indent_after_first_line(src: &str, at: usize, text: &str) -> String {
    let line_start = src[..at].rfind('\n').map_or(0, |i| i + 1);
    let before = &src[line_start..at];
    if before.is_empty() || !before.chars().all(|c| c == ' ' || c == '\t') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    for (i, line) in text.split_inclusive('\n').enumerate() {
        if i > 0 && !line.trim_end_matches(['\r', '\n']).is_empty() {
            out.push_str(before);
        }
        out.push_str(line);
    }
    out
}

fn value(marker: &Found, resolution: &ConflictResolution) -> String {
    let (Side::Value(a), Side::Value(b)) = (&marker.a, &marker.b) else {
        unreachable!("a value marker has value sides")
    };
    // A value written `[ ... ]` stays one, whichever way it is resolved.
    let content = bracketed_inner(a).zip(bracketed_inner(b));
    match resolution {
        ConflictResolution::A => a.clone(),
        ConflictResolution::B => b.clone(),
        ConflictResolution::Both => match content {
            Some((a_inner, b_inner)) => {
                let (a_inner, b_inner) = (a_inner.trim(), b_inner.trim());
                if a_inner.contains('\n') || b_inner.contains('\n') {
                    format!("[\n{}\n\n{}\n  ]", indented(a_inner), indented(b_inner))
                } else {
                    format!("[ {a_inner} {b_inner} ]")
                }
            }
            None if a.starts_with('"') && b.starts_with('"') => quoted(&format!(
                "{} / {}",
                unquote_scalar_display(a),
                unquote_scalar_display(b)
            )),
            None => a.clone(),
        },
        ConflictResolution::Rewritten(text) if content.is_some() => {
            if text.contains('\n') {
                format!("[\n{}\n  ]", indented(text.trim_end()))
            } else {
                format!("[ {text} ]")
            }
        }
        ConflictResolution::Rewritten(text) => quoted(text),
    }
}

/// The lines of `text` indented as the lines of a `[ ... ]` value's content.
fn indented(text: &str) -> String {
    text.lines()
        .map(|l| {
            if l.trim().is_empty() {
                String::new()
            } else {
                format!("    {}", l.trim_start())
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn quoted(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{find_conflicts, has_conflicts, merge};
    use ConflictResolution::{A, B, Both, Rewritten};

    #[test]
    fn inline_a_b_both_rewritten() {
        let src = "alpha @conflict(a: [ foo ], b: [ bar ]) omega\n";
        assert_eq!(resolve_all(src, A).unwrap(), "alpha foo omega\n");
        assert_eq!(resolve_all(src, B).unwrap(), "alpha bar omega\n");
        assert_eq!(resolve_all(src, Both).unwrap(), "alpha foo bar omega\n");
        assert_eq!(
            resolve_all(src, Rewritten("zzz".into())).unwrap(),
            "alpha zzz omega\n"
        );
    }

    #[test]
    fn inline_both_does_not_double_a_space() {
        let src = "x @conflict(a: [ foo  ], b: [ bar ]) y\n";
        assert_eq!(resolve_all(src, Both).unwrap(), "x foo bar y\n");
    }

    #[test]
    fn a_chosen_side_keeps_its_spelling() {
        // The sides are spliced in as written, not printed again.
        let src = "a @conflict(a: [ *y* ], b: [ **z** ]) b\n";
        assert_eq!(resolve_all(src, A).unwrap(), "a *y* b\n");
        assert_eq!(resolve_all(src, B).unwrap(), "a **z** b\n");
        let src = "see @conflict(a: [ `x` ], b: [ @link(ref: p)[q] ]) end\n";
        assert_eq!(resolve_all(src, B).unwrap(), "see @link(ref: p)[q] end\n");
    }

    #[test]
    fn block_sides_replace_the_marker_and_its_newline() {
        let src = "before\n\n@conflict(\n  a: [\n    - one\n    - two\n  ]\n  b: [\n    = Heading\n  ]\n)\n\nafter\n";
        assert_eq!(
            resolve_all(src, A).unwrap(),
            "before\n\n- one\n- two\n\nafter\n"
        );
        assert_eq!(
            resolve_all(src, B).unwrap(),
            "before\n\n= Heading\n\nafter\n"
        );
        assert_eq!(
            resolve_all(src, Both).unwrap(),
            "before\n\n- one\n- two\n\n= Heading\n\nafter\n"
        );
        assert_eq!(
            resolve_all(src, Rewritten("new".into())).unwrap(),
            "before\n\nnew\n\nafter\n"
        );
    }

    #[test]
    fn an_indented_block_marker_keeps_its_indent() {
        let src = "@note[\n  @conflict(a: [\n    one\n\n    two\n  ], b: [ x ])\n]\n";
        assert_eq!(resolve_all(src, A).unwrap(), "@note[\n  one\n\n  two\n]\n");
    }

    #[test]
    fn value_sides() {
        let src = "@x{ k: @conflict(a: 2, b: 3), s: @conflict(a: \"one\", b: \"two\") }\n";
        assert_eq!(resolve_all(src, A).unwrap(), "@x{ k: 2, s: \"one\" }\n");
        assert_eq!(resolve_all(src, B).unwrap(), "@x{ k: 3, s: \"two\" }\n");
        assert_eq!(
            resolve_all(src, Both).unwrap(),
            "@x{ k: 2, s: \"one / two\" }\n"
        );
        assert_eq!(
            resolve_all(src, Rewritten("a \"b\"".into())).unwrap(),
            "@x{ k: \"a \\\"b\\\"\", s: \"a \\\"b\\\"\" }\n"
        );
    }

    #[test]
    fn single_and_in_order() {
        let src = "a @conflict(a: [ 1 ], b: [ 2 ]) b @conflict(a: [ 3 ], b: [ 4 ]) c\n";
        assert_eq!(
            resolve_single(src, 1, B).unwrap(),
            "a @conflict(a: [ 1 ], b: [ 2 ]) b 4 c\n"
        );
        assert_eq!(resolve(src, &[A, B]).unwrap(), "a 1 b 4 c\n");
        // Fewer resolutions than markers: the rest stay.
        assert_eq!(
            resolve(src, &[B]).unwrap(),
            "a 2 b @conflict(a: [ 3 ], b: [ 4 ]) c\n"
        );
    }

    #[test]
    fn a_missing_index_is_an_error() {
        let src = "a @conflict(a: [ 1 ], b: [ 2 ]) b\n";
        assert_eq!(
            resolve_single(src, 1, A),
            Err(ConflictError::IndexOutOfBounds { index: 1, count: 1 })
        );
        assert_eq!(
            resolve_single("none\n", 0, A),
            Err(ConflictError::IndexOutOfBounds { index: 0, count: 0 })
        );
    }

    #[test]
    fn a_broken_marker_is_never_resolved_past() {
        let src = "a @conflict(a: [ 1 ], b: [ 2 ]) b @conflict(a: 1\n";
        assert!(matches!(
            resolve_all(src, A),
            Err(ConflictError::Malformed { .. })
        ));
        assert!(matches!(
            resolve_single(src, 0, A),
            Err(ConflictError::Malformed { .. })
        ));
    }

    #[test]
    fn text_outside_the_markers_is_byte_identical() {
        let src = "=[  Title  ]   // keep me\n\n  odd   spacing *em* `c`  @conflict(a: [ x ], b: [ y ])  tail  \r\n\n```\nraw  \n```\n";
        let out = resolve_all(src, A).unwrap();
        assert_eq!(
            out,
            "=[  Title  ]   // keep me\n\n  odd   spacing *em* `c`  x  tail  \r\n\n```\nraw  \n```\n"
        );
    }

    #[test]
    fn resolving_every_marker_of_a_real_merge_leaves_none() {
        let cases = [
            (
                "alpha beta gamma\n",
                "alpha BETA gamma\n",
                "alpha Beta gamma\n",
            ),
            ("- one\n- two\n", "- uno\n- two\n", "- ein\n- two\n"),
            ("=[ One ]\n", "=[ Uno ]\n", "=[ Ein ]\n"),
            ("@x{ k: 1 }\n", "@x{ k: 2 }\n", "@x{ k: 3 }\n"),
            ("text\n", "- a list\n", "= A heading\n"),
            ("a *x* b\n", "a *y* b\n", "a **z** b\n"),
        ];
        for (base, local, remote) in cases {
            let m = merge(base, local, remote);
            assert!(has_conflicts(&m.text), "{base:?}");
            for side in [A, B, Both] {
                let out = resolve_all(&m.text, side.clone()).unwrap();
                assert!(!has_conflicts(&out), "{base:?} {side:?}: {out:?}");
            }
            // Choosing a side gives that side's own text back.
            assert_eq!(
                resolve_all(&m.text, A).unwrap().trim_end(),
                local.trim_end(),
                "{base:?}"
            );
            assert_eq!(
                resolve_all(&m.text, B).unwrap().trim_end(),
                remote.trim_end(),
                "{base:?}"
            );
            assert_eq!(find_conflicts(&m.text).unwrap().len(), 1);
        }
    }
}

#[cfg(test)]
mod pipe_value_tests {
    use super::*;
    use crate::{find_conflicts, has_conflicts, merge};
    use ConflictResolution::{A, B, Both, Rewritten};

    // `about: | one` / `| two` is a value written as a `|` run. A `|` run ends
    // where its markers stop, so inside `@conflict(a: ..., b: ...)` it would
    // swallow the `, b: ...`: the merge once wrote a document the parser
    // refuses. The run is written as the `[ ... ]` it means.
    const BASE: &str =
        "::elements{\n  line: {\n    about: | one\n           | two\n    k: 1\n  }\n}\n";

    fn conflicted() -> String {
        let m = merge(
            BASE,
            &BASE.replace("one", "ONE"),
            &BASE.replace("one", "Uno"),
        );
        assert!(!m.clean);
        m.text
    }

    #[test]
    fn the_marker_a_merge_writes_for_a_pipe_run_value_can_be_read_back() {
        let text = conflicted();
        assert!(tomet_parser::parse_document(&text).is_ok(), "{text}");
        let found = find_conflicts(&text).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].kind, ConflictKind::Value);
        assert_eq!(
            (found[0].a.as_str(), found[0].b.as_str()),
            ("ONE\ntwo", "Uno\ntwo")
        );
    }

    #[test]
    fn a_resolved_pipe_run_value_is_still_a_value_the_parser_reads() {
        let text = conflicted();
        for side in [
            A,
            B,
            Both,
            Rewritten("new\nlines".into()),
            Rewritten("one line".into()),
        ] {
            let out = resolve_all(&text, side.clone()).unwrap();
            assert!(!has_conflicts(&out), "{side:?}: {out}");
            assert!(
                tomet_parser::parse_document(&out).is_ok(),
                "{side:?}: {out}"
            );
            assert!(out.contains("about: ["), "{side:?}: {out}");
        }
    }
}
