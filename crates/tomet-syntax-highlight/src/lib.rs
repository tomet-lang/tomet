//! =[ tomet-highlight ]
//!
//! What each part of a Tomet text is, for coloring it: [`highlight`] gives a
//! list of ranges, each with a [`Tag`] and group `depth`. Nothing here knows a color; an editor
//! maps tags to its own.
//!
//! =[ Where the answer comes from ]
//!
//! From the CST, [`tomet_cst_parser::parse_cst`], and from nothing else. A token
//! is a heading mark, a key or a string because of the node it stands in, not
//! because of how it looks: `"x"` is a string inside `@note("x")` and text in
//! a sentence; `true` is an atom in an argument and a word in a paragraph.
//! The CST never fails and keeps every character, so a text that is half
//! typed still gets its ranges.
//!
//! =[ The ranges ]
//!
//! - Offsets are bytes of the source, as the CST counts them. An editor that
//!   counts UTF-16 code units converts.
//! - They are in order, do not overlap, and each lies between two characters.
//! - Text with no tag has no range. A range is as long as it can be: adjacent
//!   tokens with the same tag and group depth are merged into one span.
//! - Where two kinds of node meet, the inner one decides: the `code` inside
//!   `*em*` is code.
//! - Each span carries a `depth: u8` representing the nesting depth of enclosing
//!   groups (`0` for top-level, `1` for the first enclosing group, etc.).
//!
//! =[ The tags ]
//!
//! The names are plain words, so they can be used as they are for a style
//! name. See [`Tag`] for what each one covers.

use std::collections::HashSet;

use tomet_cst::{
    AstNode, CstSlot, CstSlotMark, NodeOrToken, SyntaxKind as K, SyntaxNode, SyntaxToken, WalkEvent,
};

/// What a range of the text is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tag {
    /// The `=` run of a heading, `=[ Title ]` or `== Title`.
    Heading,
    /// An element's name with its sigil: `@name`, `::name`, `$`.
    TagName,
    /// The head of a hole, `<?int` and the `>` that closes it. The groups inside keep their own
    /// tags, so the whole box reads as one range: `<?int(n)>`.
    Hole,
    /// The head of a type seat, `<$list` and the `>` that closes it, as for a hole: the `(args)`
    /// inside keep their own tags, so the box reads as one range, `<$uuid>`.
    TypeSeat,
    /// A name tag, `<PATH>`: a label for the reader that tomet gives no meaning.
    Label,
    /// A list item's `-` (and the `.` of an ordered one).
    List,
    /// A string in a value: `"text"`.
    String,
    /// A number in a value.
    Number,
    /// `true`, `false` or `null` in a value.
    Atom,
    /// `// to the end of the line`.
    LineComment,
    /// `/* between */`.
    BlockComment,
    /// The key of a `key: value` entry.
    PropertyName,
    /// The paired delimiters `(`, `)`, `[`, `]`, `{`, `}` of a group.
    Bracket,
    /// Non-bracket delimiters such as colons, commas, `#` and `|` margins.
    Punctuation,
    /// The `---` of a thematic break.
    ContentSeparator,
    /// An inline `` `code` ``, backticks included.
    Monospace,
    /// The fences of a code block.
    ProcessingInstruction,
    /// `*em*` and `_em_`.
    Emphasis,
    /// `**strong**` and `__strong__`.
    Strong,
    /// `~~struck~~`.
    Strikethrough,
    /// A bare `https://`, `http://` or `mailto:` address.
    Link,
}

impl Tag {
    /// The tag as a word: `heading`, `tagName`, `lineComment`, ...
    pub const fn as_str(self) -> &'static str {
        match self {
            Tag::Heading => "heading",
            Tag::TagName => "tagName",
            Tag::Hole => "hole",
            Tag::TypeSeat => "typeSeat",
            Tag::Label => "label",
            Tag::List => "list",
            Tag::String => "string",
            Tag::Number => "number",
            Tag::Atom => "atom",
            Tag::LineComment => "lineComment",
            Tag::BlockComment => "blockComment",
            Tag::PropertyName => "propertyName",
            Tag::Bracket => "bracket",
            Tag::Punctuation => "punctuation",
            Tag::ContentSeparator => "contentSeparator",
            Tag::Monospace => "monospace",
            Tag::ProcessingInstruction => "processingInstruction",
            Tag::Emphasis => "emphasis",
            Tag::Strong => "strong",
            Tag::Strikethrough => "strikethrough",
            Tag::Link => "link",
        }
    }
}

/// A range of the source and what it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HighlightSpan {
    /// Byte offset of the first character.
    pub from: usize,
    /// Byte offset just past the last character.
    pub to: usize,
    pub tag: Tag,
    /// The nesting depth of enclosing groups (`0` for top-level, `1` for the
    /// first enclosing group, `2` for nested groups, etc.).
    pub depth: u8,
}

/// The ranges of `src` that have a [`Tag`], in order and without overlap.
pub fn highlight(src: &str) -> Vec<HighlightSpan> {
    let root = tomet_cst_parser::parse_cst(src);
    let mut spans: Vec<HighlightSpan> = Vec::new();
    let mut fences: HashSet<usize> = HashSet::new();

    for event in root.preorder_with_tokens() {
        let element = match event {
            WalkEvent::Enter(element) => element,
            WalkEvent::Leave(_) => continue,
        };
        let token = match element {
            NodeOrToken::Node(node) => {
                if node.kind() == K::CODE_BLOCK {
                    collect_fences(&node, &mut fences);
                }
                continue;
            }
            NodeOrToken::Token(token) => token,
        };
        let range = token.text_range();
        let (from, to) = (usize::from(range.start()), usize::from(range.end()));
        let Some(tag) = tag_of(&token, &fences) else {
            continue;
        };
        let depth = group_depth(&token);
        match spans.last_mut() {
            Some(last) if last.tag == tag && last.depth == depth && last.to == from => {
                last.to = to;
            }
            _ => spans.push(HighlightSpan {
                from,
                to,
                tag,
                depth,
            }),
        }
    }
    spans
}

/// The backticks that open and close a code block, by where they start. The
/// others in it are code.
fn collect_fences(block: &SyntaxNode, fences: &mut HashSet<usize>) {
    let tokens: Vec<SyntaxToken> = block
        .children_with_tokens()
        .filter_map(NodeOrToken::into_token)
        .collect();
    let opening = tokens
        .iter()
        .take_while(|t| t.kind() == K::BACKTICK)
        .count();
    let closed = tokens.len() >= 2 * opening
        && tokens[tokens.len() - opening..]
            .iter()
            .all(|t| t.kind() == K::BACKTICK);
    for (i, token) in tokens.iter().enumerate() {
        if i < opening || (closed && i >= tokens.len() - opening) {
            fences.insert(usize::from(token.text_range().start()));
        }
    }
}

/// What a token is, if it is anything. White space takes only the tag of the
/// span it stands inside, so a `*two words*` is one range and a space between
/// two keys is not.
fn tag_of(token: &SyntaxToken, fences: &HashSet<usize>) -> Option<Tag> {
    let kind = token.kind();
    if matches!(kind, K::WHITESPACE | K::NEWLINE) {
        return inline_span(token);
    }
    own_tag(token, fences).or_else(|| inline_span(token))
}

fn own_tag(token: &SyntaxToken, fences: &HashSet<usize>) -> Option<Tag> {
    let kind = token.kind();
    let parent = token.parent()?;
    let parent_kind = parent.kind();

    match parent_kind {
        K::CODE_SPAN => return None,
        K::FENCE => {
            if kind == K::BACKTICK {
                return Some(Tag::ProcessingInstruction);
            }
            if kind == K::MINUS {
                return Some(Tag::ContentSeparator);
            }
            return None;
        }
        K::LINE_MARKER => {
            if kind == K::BACKTICK {
                return Some(Tag::ProcessingInstruction);
            }
            return None;
        }
        K::CODE_BLOCK => {
            let at = usize::from(token.text_range().start());
            return fences.contains(&at).then_some(Tag::ProcessingInstruction);
        }
        K::SIGIL => return Some(Tag::TagName),
        K::NAME => {
            if let Some(p) = parent.parent() {
                match p.kind() {
                    K::SIGIL => return Some(Tag::TagName),
                    K::SLOT => return Some(slot_tag(&p)),
                    _ => {}
                }
            }
            return None;
        }
        // The tokens of a box's own head; its groups are nodes of their own.
        K::SLOT => return Some(slot_tag(&parent)),
        _ => {}
    }

    match kind {
        K::COMMENT => Some(if token.text().starts_with("/*") {
            Tag::BlockComment
        } else {
            Tag::LineComment
        }),
        K::EQUAL if parent_kind == K::SECTION_HEADING => Some(Tag::Heading),
        K::MINUS if parent_kind == K::THEMATIC_BREAK => Some(Tag::ContentSeparator),
        K::MINUS | K::DOT if parent_kind == K::LIST_ITEM => {
            is_list_marker(token).then_some(Tag::List)
        }
        K::MARGIN => Some(Tag::Punctuation),
        K::PIPE => is_pipe_opener(token).then_some(Tag::Punctuation),
        K::L_PAREN | K::R_PAREN | K::L_BRACE | K::R_BRACE | K::L_BRACKET | K::R_BRACKET
            if is_group_bracket(parent_kind) =>
        {
            Some(Tag::Bracket)
        }
        K::COLON | K::COMMA | K::HASH if is_group_punctuation(parent_kind) => {
            Some(Tag::Punctuation)
        }
        _ if is_key(token) => Some(Tag::PropertyName),
        K::STRING_LITERAL | K::RAW_STRING_LITERAL if in_value(token) => Some(Tag::String),
        K::INT_NUMBER | K::FLOAT_NUMBER if in_value(token) => Some(Tag::Number),
        K::TRUE_KW | K::FALSE_KW | K::NULL_KW if in_value(token) => Some(Tag::Atom),
        _ => None,
    }
}

/// The tag of the head of the box `slot`, which its mark tells: a hole, a type seat or a label.
fn slot_tag(slot: &SyntaxNode) -> Tag {
    match CstSlot::cast(slot.clone()).map(|slot| slot.mark()) {
        Some(CstSlotMark::Hole) => Tag::Hole,
        Some(CstSlotMark::TypeSeat) => Tag::TypeSeat,
        Some(CstSlotMark::NameTag) | None => Tag::Label,
    }
}

/// The nodes whose paired delimiters `()`, `[]`, `{}` are group brackets.
/// Those of a paragraph are prose.
fn is_group_bracket(kind: K) -> bool {
    matches!(
        kind,
        K::ARGS | K::VALUE_DATA | K::CONTENT | K::MAP_LITERAL | K::ID_GROUP | K::INTERP_EXPR
    )
}

/// The nodes whose colons, commas and `#` are punctuation delimiters. Those of a
/// paragraph are prose.
fn is_group_punctuation(kind: K) -> bool {
    matches!(
        kind,
        K::ARGS
            | K::VALUE_DATA
            | K::CONTENT
            | K::MAP_LITERAL
            | K::ID_GROUP
            | K::CONNECT
            | K::MAP_ENTRY
            | K::INTERP_EXPR
    )
}

/// The nesting depth of enclosing groups (`ARGS`, `CONTENT`, `VALUE_DATA`,
/// `MAP_LITERAL`, `ID_GROUP`, `INTERP_EXPR`). Top-level tokens have depth `0`.
fn group_depth(token: &SyntaxToken) -> u8 {
    let count = token
        .parent_ancestors()
        .filter(|node| is_group_bracket(node.kind()))
        .count();
    count.min(u8::MAX as usize) as u8
}

/// The `-` (and the `.` straight after it) that opens a list item; the rest of
/// the item's tokens are its text.
fn is_list_marker(token: &SyntaxToken) -> bool {
    match token.prev_sibling_or_token() {
        None => token.kind() == K::MINUS,
        Some(NodeOrToken::Token(prev)) => {
            token.kind() == K::DOT
                && prev.kind() == K::MINUS
                && prev.prev_sibling_or_token().is_none()
        }
        Some(NodeOrToken::Node(_)) => false,
    }
}

/// The `|` that opens a run, and not a bar in the text. (The ones that begin a run's later lines are
/// `MARGIN` tokens in the tree.)
fn is_pipe_opener(token: &SyntaxToken) -> bool {
    token
        .parent()
        .is_some_and(|p| p.kind() == K::CONTENT && p.first_token().as_ref() == Some(token))
}

/// The key of a `key: value` entry: what stands before its colon.
fn is_key(token: &SyntaxToken) -> bool {
    let Some(parent) = token.parent() else {
        return false;
    };
    if parent.kind() != K::MAP_ENTRY || token.kind() == K::COLON {
        return false;
    }
    let mut before = token.prev_sibling_or_token();
    while let Some(element) = before {
        if element.kind() == K::COLON {
            return false;
        }
        before = element.prev_sibling_or_token();
    }
    !token.kind().is_trivia()
}

/// Is the token a value -- in an argument list, a `{ }` group or an
/// expression -- and not words in a sentence?
fn in_value(token: &SyntaxToken) -> bool {
    for node in token.parent_ancestors() {
        match node.kind() {
            K::ARGS
            | K::VALUE_DATA
            | K::ID_GROUP
            | K::MAP_LITERAL
            | K::MAP_ENTRY
            | K::SEQ_ITEM
            | K::INTERP_EXPR => return true,
            K::PARAGRAPH
            | K::LIST_ITEM
            | K::SECTION_HEADING
            | K::THEMATIC_BREAK
            | K::CONTENT
            | K::EMPHASIS
            | K::STRONG
            | K::STRIKEOUT
            | K::AUTOLINK
            | K::CODE_SPAN
            | K::CODE_BLOCK => return false,
            _ => {}
        }
    }
    false
}

/// The tag of the innermost span of running text the token stands in.
fn inline_span(token: &SyntaxToken) -> Option<Tag> {
    token.parent_ancestors().find_map(|node| match node.kind() {
        K::CODE_SPAN => Some(Tag::Monospace),
        K::AUTOLINK => Some(Tag::Link),
        K::EMPHASIS => Some(Tag::Emphasis),
        K::STRONG => Some(Tag::Strong),
        K::STRIKEOUT => Some(Tag::Strikethrough),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `(text, tag)` for every range of `src`.
    fn tagged(src: &str) -> Vec<(&str, &'static str)> {
        highlight(src)
            .iter()
            .map(|s| (&src[s.from..s.to], s.tag.as_str()))
            .collect()
    }

    fn has(src: &str, text: &str, tag: &str) -> bool {
        tagged(src).contains(&(text, tag))
    }

    /// Is the character at byte `at` inside a punctuation range?
    fn punct_at(src: &str, at: usize) -> bool {
        highlight(src)
            .iter()
            .any(|s| s.tag == Tag::Punctuation && s.from <= at && at < s.to)
    }

    #[test]
    fn a_heading_is_its_equals_signs() {
        assert!(has("=[ Title ]\n", "=", "heading"));
        assert!(has("== Title\n", "==", "heading"));
        assert!(has("=| Title\n | more\n", "=", "heading"));
    }

    #[test]
    fn the_old_hash_is_not_a_heading() {
        // `#(id)` is an id, and a `#[` heading is prose.
        assert!(!has("#[ Title ]\n", "#", "heading"));
        assert!(punct_at("@x(a)#(id)\n", 5), "the # of the id");
    }

    #[test]
    fn an_element_is_its_sigil_and_name() {
        assert!(has("@note[ text ]\n", "@note", "tagName"));
        assert!(has("::config(a: 1)\n", "::config", "tagName"));
        assert!(has("a @link(ref: x)[y] b\n", "@link", "tagName"));
    }

    #[test]
    fn a_hole_is_its_box_and_a_tag_is_a_label() {
        // The head of a hole is one range with the `>`; the groups in between keep their own tags.
        assert_eq!(
            tagged("a <?int(n, \"x\")> b\n"),
            [
                ("<?int", "hole"),
                ("(", "bracket"),
                (",", "punctuation"),
                ("\"x\"", "string"),
                (")", "bracket"),
                (">", "hole"),
            ]
        );
        assert!(has("<?>\n", "<?>", "hole"));
        // The content is outside the box, so its brackets are group brackets.
        assert_eq!(
            tagged("<?(id)>[a]\n"),
            [
                ("<?", "hole"),
                ("(", "bracket"),
                (")", "bracket"),
                (">", "hole"),
                ("[", "bracket"),
                ("]", "bracket"),
            ]
        );
        assert_eq!(
            tagged("--out <PATH> and <ファイル>\n"),
            [("<PATH>", "label"), ("<ファイル>", "label")]
        );
    }

    #[test]
    fn a_type_seat_is_its_box_and_its_mark_is_no_label() {
        // As a hole: the head is one range with the `>`, and the arguments keep their own tags.
        assert_eq!(
            tagged("a <$list(string, length: 6)> b\n"),
            [
                ("<$list", "typeSeat"),
                ("(", "bracket"),
                (",", "punctuation"),
                ("length", "propertyName"),
                (":", "punctuation"),
                ("6", "number"),
                (")", "bracket"),
                (">", "typeSeat"),
            ]
        );
        assert!(has("<$uuid>\n", "<$uuid>", "typeSeat"));
        // A seat is neither a hole nor a label, whichever way the mark is read.
        for (src, text) in [
            ("<$uuid>\n", "<$uuid>"),
            ("a <$date(default: auto)> b\n", "<$date"),
        ] {
            assert!(has(src, text, "typeSeat"), "{src:?}");
            assert!(
                !tagged(src)
                    .iter()
                    .any(|(t, tag)| t.contains('$') && matches!(*tag, "hole" | "label"))
            );
        }
        // What follows its `>` is not its: the brackets are prose.
        assert_eq!(tagged("<$x>[a]\n"), [("<$x>", "typeSeat")]);
    }

    #[test]
    fn text_that_is_no_box_has_no_tag() {
        for src in [
            "a < b\n",
            "x<=1\n",
            "<->\n",
            "<!doctype html>\n",
            "</x>\n",
            "<x/>\n",
            "<?int and\n",
            "<$x and\n",
            "<$ x>\n",
        ] {
            assert!(highlight(src).is_empty(), "`{src}` is plain text");
        }
    }

    #[test]
    fn list_markers() {
        assert!(has("- item\n", "-", "list"));
        assert!(has("-. item\n", "-.", "list"));
        assert!(has("-| item\n", "-", "list"));
        // A dot in the item's text is text.
        assert!(!has("- a.\n", ".", "list"));
    }

    #[test]
    fn a_thematic_break() {
        assert!(has("---\n", "---", "contentSeparator"));
        assert!(has("---[ Title ]---\n", "---", "contentSeparator"));
        // Both fences of the `|` form.
        let piped = "---\n| Title\n---\n";
        assert!(has(piped, "---", "contentSeparator"));
        assert!(!has(piped, "Title", "contentSeparator"));
    }

    #[test]
    fn entries_have_keys_and_values() {
        let src = "@x(name: \"v\", n: 3, f: 1.5, ok: true, no: null)\n";
        for key in ["name", "n", "f", "ok", "no"] {
            assert!(has(src, key, "propertyName"), "{key}");
        }
        assert!(has(src, "\"v\"", "string"));
        assert!(has(src, "3", "number"));
        assert!(has(src, "1.5", "number"));
        assert!(has(src, "true", "atom"));
        assert!(has(src, "null", "atom"));
    }

    #[test]
    fn a_word_in_a_sentence_is_not_a_value() {
        // The CST puts these in a paragraph, whatever the tokenizer called them.
        let src = "He said \"hello\", counted 3 and was true to it.\n";
        assert!(highlight(src).is_empty(), "{:?}", tagged(src));
    }

    #[test]
    fn groups_draw_delimiters() {
        let src = "@x(a: 1)[ t ]{ k: v }\n";
        for (at, c) in src.char_indices() {
            if "()[]{}".contains(c) {
                assert!(
                    highlight(src)
                        .iter()
                        .any(|s| s.tag == Tag::Bracket && s.from <= at && at < s.to),
                    "{c} at {at} should be bracket"
                );
            } else if ":".contains(c) {
                assert!(punct_at(src, at), "{c} at {at} should be punctuation");
            }
        }
    }

    #[test]
    fn heading_frame_depth_and_brackets() {
        let src = "=[ Title ]\n";
        let spans = highlight(src);
        assert_eq!(
            spans,
            [
                HighlightSpan {
                    from: 0,
                    to: 1,
                    tag: Tag::Heading,
                    depth: 0,
                },
                HighlightSpan {
                    from: 1,
                    to: 2,
                    tag: Tag::Bracket,
                    depth: 1,
                },
                HighlightSpan {
                    from: 9,
                    to: 10,
                    tag: Tag::Bracket,
                    depth: 1,
                },
            ]
        );
        // Title itself is untagged prose
        assert!(!has(src, "Title", "heading"));
        assert!(!has(src, "Title", "bracket"));
    }

    #[test]
    fn nested_group_depths() {
        // (1) Forms and content groups nesting: ( ... [ ... { ... } ] )
        let src1 = "@foo(a: [ @bar{ c: 1 } ])\n";
        let spans1 = highlight(src1);
        let tagged_depths1: Vec<(&str, &'static str, u8)> = spans1
            .iter()
            .map(|s| (&src1[s.from..s.to], s.tag.as_str(), s.depth))
            .collect();
        assert_eq!(
            tagged_depths1,
            [
                ("@foo", "tagName", 0),
                ("(", "bracket", 1),
                ("a", "propertyName", 1),
                (":", "punctuation", 1),
                ("[", "bracket", 2),
                ("@bar", "tagName", 2),
                ("{", "bracket", 3),
                ("c", "propertyName", 3),
                (":", "punctuation", 3),
                ("1", "number", 3),
                ("}", "bracket", 3),
                ("]", "bracket", 2),
                (")", "bracket", 1),
            ]
        );

        // (2) Nested map literals in values: ( ... { ... { ... } } )
        let src2 = "@foo(a: { b: { c: 1 } })\n";
        let spans2 = highlight(src2);
        let tagged_depths2: Vec<(&str, &'static str, u8)> = spans2
            .iter()
            .map(|s| (&src2[s.from..s.to], s.tag.as_str(), s.depth))
            .collect();
        assert_eq!(
            tagged_depths2,
            [
                ("@foo", "tagName", 0),
                ("(", "bracket", 1),
                ("a", "propertyName", 1),
                (":", "punctuation", 1),
                ("{", "bracket", 2),
                ("b", "propertyName", 2),
                (":", "punctuation", 2),
                ("{", "bracket", 3),
                ("c", "propertyName", 3),
                (":", "punctuation", 3),
                ("1", "number", 3),
                ("}", "bracket", 3),
                ("}", "bracket", 2),
                (")", "bracket", 1),
            ]
        );
    }

    #[test]
    fn commas_and_colons_retain_group_depth() {
        let src = "@bar(x: 1, y: 2)\n";
        let spans = highlight(src);
        let items: Vec<(&str, &'static str, u8)> = spans
            .iter()
            .map(|s| (&src[s.from..s.to], s.tag.as_str(), s.depth))
            .collect();
        assert_eq!(
            items,
            [
                ("@bar", "tagName", 0),
                ("(", "bracket", 1),
                ("x", "propertyName", 1),
                (":", "punctuation", 1),
                ("1", "number", 1),
                (",", "punctuation", 1),
                ("y", "propertyName", 1),
                (":", "punctuation", 1),
                ("2", "number", 1),
                (")", "bracket", 1),
            ]
        );
    }

    #[test]
    fn top_level_spans_have_depth_zero() {
        for (src, text, tag) in [
            ("== Title\n", "==", "heading"),
            ("// a comment\n", "// a comment", "lineComment"),
            ("/* block */\n", "/* block */", "blockComment"),
            ("- list item\n", "-", "list"),
            ("---\n", "---", "contentSeparator"),
        ] {
            let spans = highlight(src);
            let span = spans
                .iter()
                .find(|s| &src[s.from..s.to] == text && s.tag.as_str() == tag)
                .expect(src);
            assert_eq!(span.depth, 0, "depth of {text} in {src}");
        }
    }

    #[test]
    fn brackets_and_colons_in_a_sentence_are_prose() {
        assert!(highlight("see array[0] and a: b, c\n").is_empty());
    }

    #[test]
    fn comments() {
        assert!(has("a // line\n", "// line", "lineComment"));
        assert!(has("a /* block */ b\n", "/* block */", "blockComment"));
    }

    #[test]
    fn inline_code_is_monospace_and_not_read() {
        let src = "a `*x* // y` b\n";
        assert_eq!(tagged(src), [("`*x* // y`", "monospace")]);
    }

    #[test]
    fn a_code_block_has_fences_and_plain_code() {
        let src = "```rust\nlet x = \"a\"; // c\n```\n";
        assert_eq!(
            tagged(src),
            [
                ("```", "processingInstruction"),
                ("```", "processingInstruction")
            ]
        );
    }

    #[test]
    fn emphasis_spans_whole_words_and_whitespace() {
        assert_eq!(tagged("a *two words* b\n"), [("*two words*", "emphasis")]);
        assert_eq!(
            tagged("a **very strong** b\n"),
            [("**very strong**", "strong")]
        );
        assert_eq!(tagged("a ~~gone~~ b\n"), [("~~gone~~", "strikethrough")]);
        assert_eq!(tagged("a _em_s b\n"), [("_em_", "emphasis")]);
    }

    #[test]
    fn the_inner_span_decides() {
        let src = "**a `c` b**\n";
        assert_eq!(
            tagged(src),
            [("**a ", "strong"), ("`c`", "monospace"), (" b**", "strong")]
        );
    }

    #[test]
    fn a_url_is_a_link() {
        assert_eq!(
            tagged("see https://a.b/c, ok\n"),
            [("https://a.b/c", "link")]
        );
    }

    #[test]
    fn pipe_margins_are_punctuation_and_bars_in_text_are_not() {
        let src = "@quote\n| alpha | beta\n| gamma\n";
        let bars: Vec<usize> = highlight(src)
            .iter()
            .filter(|s| s.tag == Tag::Punctuation)
            .map(|s| s.from)
            .collect();
        // The opener and the margin of the second line; not the bar between words.
        assert_eq!(
            bars,
            [src.find("| alpha").unwrap(), src.find("| gamma").unwrap()]
        );
    }

    #[test]
    fn a_key_with_a_pipe_run_value() {
        let src = "@x{\n  about: | one, {two}\n         | three\n}\n";
        assert!(has(src, "about", "propertyName"));
        // The run is prose: `two` and its braces are text, not a map.
        assert!(!has(src, "two", "propertyName"));
    }

    #[test]
    fn ranges_are_ordered_whole_and_apart() {
        for src in [
            "=[ T ]\n@a(k: \"v\")[ *x* `c` https://a.b ]\n- i\n---\n| a | b |\n@q\n| x\n| y\n",
            "日本語の *強調* と `コード` と @note(題: \"値\")[ 本文 ]\n",
            "@x{\n  about: | あ、い\n         | う\n}\n",
            "unclosed *em and `code and @x(\n",
        ] {
            let spans = highlight(src);
            let mut end = 0;
            for s in &spans {
                assert!(s.from < s.to, "{src:?}: empty range {s:?}");
                assert!(
                    s.from >= end,
                    "{src:?}: out of order or overlapping at {s:?}"
                );
                assert!(
                    src.is_char_boundary(s.from) && src.is_char_boundary(s.to),
                    "{src:?}: {s:?} cuts a character"
                );
                end = s.to;
            }
            assert!(end <= src.len());
        }
    }

    #[test]
    fn neighbours_with_one_tag_are_one_range() {
        let spans = highlight("== T\n");
        assert_eq!(spans.len(), 1);
        assert_eq!((spans[0].from, spans[0].to), (0, 2));
    }

    #[test]
    fn nothing_for_nothing() {
        assert!(highlight("").is_empty());
        assert!(highlight("just some words\n").is_empty());
    }
}
