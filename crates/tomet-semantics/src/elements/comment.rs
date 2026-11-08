//! =[ comment ]
//!
//! `@comment[ ... ]`: a note for whoever reads the exported source.
//!
//! What a comment *says* is decided once, here: [`comment_text`]. An exporter does not read the
//! tree for it; it takes the text and writes it in the comment syntax of its target, which is the
//! only thing that differs between targets. [`comment_element`] is the other direction: what an
//! importer builds from the text of a comment it found.

use std::cell::RefCell;
use tomet_ast::{
    Block, Element, ElementView, Inline, Name, Paragraph, Placement, RawText, SoftBreak, Text,
};
use tomet_span::Span;
use tomet_tree::{Node, Place, Visit, element_new, walk_node_scoped};

/// The text of a comment's `[content]`, as it reads in source.
///
/// - Blocks (paragraphs, a section's title and body, list items) are separated by a blank line.
///   A paragraph inside an inline element is not a block of its own and is not separated.
/// - A line break, soft or hard, is a newline: the author wrapped the line there.
/// - A code span keeps its backticks, with a fence long enough for what is in it.
/// - An `${...}` nobody evaluated is written as it was, the way a reader of the source would
///   find it. A hole is left out: it has no text until it is filled.
/// - Markup around text (`@em[x]`) contributes its text. `(args)`, `{value}` and `:connect`s are
///   not part of what the content says, and are not read.
///
/// An element with no content has no text: the empty string.
pub fn comment_text(el: ElementView<'_>) -> String {
    let state = RefCell::new(State::default());
    let mut enter = |node: Node<'_>, place: Place| {
        // Only the places where an element's content sits. Anything else (`(args)`, a `{value}`
        // entry, a `:connect`) describes the element and is not text of it.
        if !matches!(
            place,
            Place::Top
                | Place::Content
                | Place::Body
                | Place::Text
                | Place::Title
                | Place::Item
                | Place::Sublist
        ) {
            return Visit::Skip;
        }
        let mut state = state.borrow_mut();
        match node {
            Node::Block(Block::Hole(_)) | Node::Inline(Inline::Hole(_)) => return Visit::Skip,
            Node::Block(Block::Interp(interp)) => {
                state.block_starts();
                state.push(&format!("${{{}}}", interp.expr));
            }
            Node::Block(_) | Node::Item(_) | Node::List(_) => state.block_starts(),
            Node::Inline(Inline::Text(text)) => state.push(&text.value),
            Node::Inline(Inline::Raw(raw)) => state.push(&code_span(&raw.value)),
            Node::Inline(Inline::SoftBreak(_) | Inline::LineBreak(_)) => state.push("\n"),
            Node::Inline(Inline::Interp(interp)) => state.push(&format!("${{{}}}", interp.expr)),
            Node::Inline(Inline::Element(_)) => state.inline_elements += 1,
            Node::Declaration(_) | Node::Value(_) | Node::Entry(_) | Node::Connect(_) => {}
        }
        Visit::<()>::Descend
    };
    let mut leave = |node: Node<'_>, _: Place| {
        if matches!(node, Node::Inline(Inline::Element(_))) {
            state.borrow_mut().inline_elements -= 1;
        }
    };
    for block in el.content.unwrap_or_default() {
        let _ = walk_node_scoped(Node::Block(block), Place::Content, &mut enter, &mut leave);
    }
    state.into_inner().out
}

/// What [`comment_text`] has read so far.
#[derive(Default)]
struct State {
    out: String,
    /// A block has started since the last text, so the next text follows a blank line.
    blank_line_due: bool,
    /// How many inline elements enclose the node being read. Inside one, a paragraph is the
    /// element's single run of text and not a block of the comment.
    inline_elements: usize,
}

impl State {
    fn block_starts(&mut self) {
        if self.inline_elements == 0 {
            self.blank_line_due = true;
        }
    }

    /// Appends `text`, after the blank line a block start is owed, if there is anything before.
    /// The blank line is written with the text and not with the block start, so a block that says
    /// nothing leaves no blank line behind.
    fn push(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        if self.blank_line_due && !self.out.is_empty() {
            self.out.push_str("\n\n");
        }
        self.blank_line_due = false;
        self.out.push_str(text);
    }
}

/// `value` as a code span: the fence is one backtick longer than the longest run inside, and a
/// value that starts or ends with a backtick (or with a space on both sides) is padded by one space
/// so the fence is not read as part of it.
fn code_span(value: &str) -> String {
    let longest = value.split(|c| c != '`').map(str::len).max().unwrap_or(0);
    let fence = "`".repeat(longest + 1);
    let padded = value.starts_with('`')
        || value.ends_with('`')
        || (value.starts_with(' ') && value.ends_with(' ') && !value.trim().is_empty());
    let pad = if padded { " " } else { "" };
    format!("{fence}{pad}{value}{pad}{fence}")
}

/// A `@comment` that says `text`, standing at `placement`: the element an importer builds from
/// the text of a comment it found.
///
/// The inverse of [`comment_text`] for what that can say: a blank line starts a paragraph, a line
/// break inside one is a soft break, and a run of backticks closed by a run of the same length is a
/// code span (the way CommonMark reads one; a run with no partner is only text). The element is
/// spelled bare, as `std`'s elements are. An empty text is a comment with no content.
pub fn comment_element(text: &str, placement: Placement) -> Element {
    let mut el = element_new(Name::bare("comment"));
    el.placement = placement;
    if !text.is_empty() {
        el.content = Some(text.split("\n\n").map(paragraph).collect());
    }
    el
}

fn paragraph(text: &str) -> Block {
    let mut inlines = Vec::new();
    let mut rest = text;
    let mut plain = String::new();
    while let Some(start) = rest.find('`') {
        let run = rest[start..].bytes().take_while(|b| *b == b'`').count();
        let after = &rest[start + run..];
        match code_span_end(after, run) {
            Some(end) => {
                plain.push_str(&rest[..start]);
                push_lines(&mut inlines, std::mem::take(&mut plain));
                inlines.push(code_span_element(unpad(&after[..end])));
                rest = &after[end + run..];
            }
            None => {
                plain.push_str(&rest[..start + run]);
                rest = after;
            }
        }
    }
    plain.push_str(rest);
    push_lines(&mut inlines, plain);
    Block::Paragraph(Paragraph::new(inlines, Span::dummy()))
}

/// A code span as the parser has it: an inline `@raw` that holds the code, which the element, and
/// not the text, is what puts a fence around when it is written.
fn code_span_element(code: &str) -> Inline {
    let mut raw = element_new(Name::bare("raw"));
    raw.placement = Placement::Inline;
    raw.content = Some(vec![Block::Paragraph(Paragraph::new(
        vec![Inline::Raw(RawText::new(code, Span::dummy()))],
        Span::dummy(),
    ))]);
    Inline::Element(raw)
}

/// Where in `after` the run of `fence` backticks that closes a code span begins: the first run of
/// exactly that length. A longer or shorter run inside is part of the code.
fn code_span_end(after: &str, fence: usize) -> Option<usize> {
    let mut at = 0;
    while let Some(found) = after[at..].find('`') {
        let start = at + found;
        let run = after[start..].bytes().take_while(|b| *b == b'`').count();
        if run == fence {
            return Some(start);
        }
        at = start + run;
    }
    None
}

/// A code span's value without the one space on each side that [`code_span`] puts there so a
/// fence is not read as part of it. CommonMark strips them only when both are there and the value
/// is not all spaces.
fn unpad(value: &str) -> &str {
    if value.trim().is_empty() {
        return value;
    }
    value
        .strip_prefix(' ')
        .and_then(|inner| inner.strip_suffix(' '))
        .unwrap_or(value)
}

/// `text` as inlines: its lines as text, with a soft break between two.
fn push_lines(inlines: &mut Vec<Inline>, text: String) {
    if text.is_empty() {
        return;
    }
    for (i, line) in text.split('\n').enumerate() {
        if i > 0 {
            inlines.push(Inline::SoftBreak(SoftBreak {
                span: Span::dummy(),
            }));
        }
        let line = line.strip_suffix('\r').unwrap_or(line);
        if !line.is_empty() {
            inlines.push(Inline::Text(Text::new(line, Span::dummy())));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tomet_ast::File;

    fn first_element(file: &File) -> &Element {
        match &file.body[0] {
            Block::Element(el) => el,
            Block::Paragraph(p) => p
                .content
                .iter()
                .find_map(|inline| match inline {
                    Inline::Element(el) => Some(el),
                    _ => None,
                })
                .expect("the paragraph holds an element"),
            other => panic!("expected an element, got {other:?}"),
        }
    }

    fn text_of(src: &str) -> String {
        let file = tomet_ast_parser::parse_file(src).unwrap();
        comment_text(first_element(&file).as_view())
    }

    #[test]
    fn a_comment_says_its_text() {
        assert_eq!(
            text_of("@comment[ Generated from a.tmt. ]\n"),
            "Generated from a.tmt."
        );
    }

    #[test]
    fn a_comment_with_no_content_says_nothing() {
        assert_eq!(text_of("@comment()\n"), "");
        assert_eq!(text_of("@comment[]\n"), "");
    }

    #[test]
    fn blocks_are_a_blank_line_apart() {
        assert_eq!(text_of("@comment[\n  one\n\n  two\n]\n"), "one\n\ntwo");
    }

    #[test]
    fn a_wrapped_line_stays_wrapped() {
        assert_eq!(text_of("@comment[\n  one\n  two\n]\n"), "one\ntwo");
    }

    #[test]
    fn markup_around_text_gives_its_text_and_no_blank_line() {
        // The paragraph an inline element holds is not a block of the comment.
        assert_eq!(text_of("@comment[ a @em[ b ] c ]\n"), "a b c");
    }

    #[test]
    fn a_code_span_keeps_its_backticks() {
        assert_eq!(
            text_of("@comment[ run `tomet export .` now ]\n"),
            "run `tomet export .` now"
        );
    }

    #[test]
    fn the_fence_of_a_code_span_outgrows_what_is_inside() {
        assert_eq!(code_span("a"), "`a`");
        assert_eq!(code_span("a`b"), "``a`b``");
        assert_eq!(code_span("`a"), "`` `a ``");
        assert_eq!(code_span(" a "), "`  a  `");
        assert_eq!(code_span("  "), "`  `");
    }

    #[test]
    fn an_unevaluated_interpolation_is_written_as_it_was() {
        assert_eq!(
            text_of("@comment[ from ${macro.x(1)} ]\n"),
            "from ${macro.x(1)}"
        );
    }

    #[test]
    fn what_describes_the_element_is_not_its_text() {
        assert_eq!(text_of("@comment(by: @em[ nobody ])[ said ]\n"), "said");
    }

    #[test]
    fn an_inline_comment_is_read_the_same() {
        assert_eq!(text_of("before @comment[ note ] after\n"), "note");
    }

    #[test]
    fn a_code_span_in_the_text_comes_back_as_one() {
        let el = comment_element("run `tomet export .` now", Placement::Block);
        let Some([Block::Paragraph(p)]) = el.content.as_deref() else {
            panic!("expected one paragraph, got {:?}", el.content);
        };
        let [
            Inline::Text(before),
            Inline::Element(span),
            Inline::Text(after),
        ] = &p.content[..]
        else {
            panic!("expected text, a code span, text; got {:?}", p.content);
        };
        assert_eq!(
            (before.value.as_str(), after.value.as_str()),
            ("run ", " now")
        );
        assert_eq!(span.name, Name::bare("raw"));
        assert_eq!(span.placement, Placement::Inline);
        assert_eq!(comment_text(el.as_view()), "run `tomet export .` now");
    }

    #[test]
    fn a_fence_is_closed_by_a_run_of_its_own_length_and_nothing_else() {
        for (text, code) in [
            ("a `` ` `` b", "`"),
            ("a ``x`y`` b", "x`y"),
            ("a ` x ` b", "x"),
            ("a `  ` b", "  "),
        ] {
            let el = comment_element(text, Placement::Block);
            let Some([Block::Paragraph(p)]) = el.content.as_deref() else {
                panic!("expected one paragraph, got {:?}", el.content);
            };
            let held = p.content.iter().find_map(|inline| match inline {
                Inline::Element(span) => match span.content.as_deref() {
                    Some([Block::Paragraph(p)]) => match &p.content[..] {
                        [Inline::Raw(r)] => Some(r.value.as_str()),
                        _ => None,
                    },
                    _ => None,
                },
                _ => None,
            });
            assert_eq!(held, Some(code), "{text:?} -> {:?}", p.content);
        }
    }

    #[test]
    fn a_backtick_with_no_partner_is_only_text() {
        let el = comment_element("it`s a tick", Placement::Block);
        let Some([Block::Paragraph(p)]) = el.content.as_deref() else {
            panic!("expected one paragraph, got {:?}", el.content);
        };
        assert!(matches!(&p.content[..], [Inline::Text(t)] if t.value == "it`s a tick"));
    }

    #[test]
    fn a_comment_element_is_what_the_text_reads_back_as() {
        for text in [
            "",
            "one",
            "one\ntwo",
            "one\n\ntwo",
            "one\n\ntwo\nthree",
            "run `tomet export .` now",
            "a `` ` `` b",
            "a ``x`y`` b",
            "it`s a tick",
            "`a` and `b`\n\n`c`",
        ] {
            let el = comment_element(text, Placement::Block);
            assert_eq!(comment_text(el.as_view()), text, "for {text:?}");
        }
        assert_eq!(
            comment_element("x", Placement::Inline).placement,
            Placement::Inline
        );
    }
}
