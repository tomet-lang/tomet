//! =[ tomet-highlight ]
//!
//! What each part of a Tomet text is, for coloring it: [`highlight`] gives a
//! list of ranges, each with a [`Tag`]. Nothing here knows a color; an editor
//! maps tags to its own.
//!
//! =[ Where the answer comes from ]
//!
//! From the CST, [`tomet_parser::parse_cst`], and from nothing else. A token
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
//! - Text with no tag has no range. A range is as long as it can be: the
//!   neighbours with the same tag are one.
//! - Where two kinds of node meet, the inner one decides: the `code` inside
//!   `*em*` is code.
//!
//! =[ Tags ]
//!
//! The names are plain words, so they can be used as they are for a style
//! name. See [`Tag`] for what each one covers.

use std::collections::HashSet;

use tomet_cst::{NodeOrToken, SyntaxKind as K, SyntaxNode, SyntaxToken, WalkEvent};

/// What a range of the text is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tag {
    /// The `=` run of a heading, `=[ Title ]` or `== Title`.
    Heading,
    /// An element's name with its sigil: `@name`, `::name`, `<tag>`, `$`.
    TagName,
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
    /// The brackets, colons, commas and `|` margins that draw a group.
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
            Tag::List => "list",
            Tag::String => "string",
            Tag::Number => "number",
            Tag::Atom => "atom",
            Tag::LineComment => "lineComment",
            Tag::BlockComment => "blockComment",
            Tag::PropertyName => "propertyName",
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
}

/// The ranges of `src` that have a [`Tag`], in order and without overlap.
pub fn highlight(src: &str) -> Vec<HighlightSpan> {
    let root = tomet_parser::parse_cst(src);
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
        let Some(tag) = tag_of(&token, src, &fences) else {
            continue;
        };
        match spans.last_mut() {
            Some(last) if last.tag == tag && last.to == from => last.to = to,
            _ => spans.push(HighlightSpan { from, to, tag }),
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
fn tag_of(token: &SyntaxToken, src: &str, fences: &HashSet<usize>) -> Option<Tag> {
    let kind = token.kind();
    if matches!(kind, K::WHITESPACE | K::NEWLINE) {
        return inline_span(token);
    }
    own_tag(token, src, fences).or_else(|| inline_span(token))
}

fn own_tag(token: &SyntaxToken, src: &str, fences: &HashSet<usize>) -> Option<Tag> {
    let kind = token.kind();
    let parent = token.parent()?;
    let parent_kind = parent.kind();

    match parent_kind {
        K::CODE_SPAN => return None,
        K::CODE_BLOCK => {
            let at = usize::from(token.text_range().start());
            return fences.contains(&at).then_some(Tag::ProcessingInstruction);
        }
        K::SIGIL => return Some(Tag::TagName),
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
        K::PIPE => is_pipe_margin(token, src).then_some(Tag::Punctuation),
        K::L_PAREN
        | K::R_PAREN
        | K::L_BRACE
        | K::R_BRACE
        | K::L_BRACKET
        | K::R_BRACKET
        | K::COLON
        | K::COMMA
        | K::HASH
            if is_group_punctuation(parent_kind) =>
        {
            Some(Tag::Punctuation)
        }
        _ if is_key(token) => Some(Tag::PropertyName),
        K::STRING_LITERAL | K::RAW_STRING_LITERAL if in_value(token) => Some(Tag::String),
        K::INT_NUMBER | K::FLOAT_NUMBER if in_value(token) => Some(Tag::Number),
        K::TRUE_KW | K::FALSE_KW | K::NULL_KW if in_value(token) => Some(Tag::Atom),
        _ => None,
    }
}

/// The nodes whose own brackets, colons and commas are punctuation. Those of a
/// paragraph are prose.
fn is_group_punctuation(kind: K) -> bool {
    matches!(
        kind,
        K::ARGS
            | K::VALUE_DATA
            | K::CONTENT
            | K::MAP_LITERAL
            | K::SEQ_LITERAL
            | K::ID_GROUP
            | K::CONNECT
            | K::MAP_ENTRY
            | K::INTERP_EXPR
    )
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

/// A `|` that is the margin of a run -- the one that opens it, or the one that
/// begins a later line -- and not a bar in the text.
fn is_pipe_margin(token: &SyntaxToken, src: &str) -> bool {
    let Some(parent) = token.parent() else {
        return false;
    };
    if parent.kind() == K::CONTENT && parent.first_token().as_ref() == Some(token) {
        return true;
    }
    let at = usize::from(token.text_range().start());
    let line_start = src[..at].rfind('\n').map_or(0, |i| i + 1);
    src[line_start..at]
        .chars()
        .all(|c| c == '|' || c.is_whitespace())
        && token
            .parent_ancestors()
            .any(|n| n.kind() == K::CONTENT && n.first_token().is_some_and(|f| f.kind() == K::PIPE))
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
            | K::SEQ_LITERAL
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
        // `#(id)` is an id now, and the old `#[` heading is prose.
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
    fn groups_draw_punctuation() {
        let src = "@x(a: 1)[ t ]{ k: v }\n";
        // Neighbours with one tag are one range, so ask about each character.
        for (at, c) in src.char_indices() {
            if "()[]{}:".contains(c) {
                assert!(punct_at(src, at), "{c} at {at}");
            }
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
