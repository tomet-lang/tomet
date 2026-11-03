//! Defines the raw token and composite node kinds for Tomet's Concrete Syntax Tree (CST).

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[allow(non_camel_case_types)]
#[repr(u16)]
pub enum SyntaxKind {
    // --- Special / Trivia ---
    TOMBSTONE = 0,
    EOF,
    WHITESPACE,
    COMMENT,
    COMMENT_LINE_OPEN,   // `//` after whitespace or at start of source
    COMMENT_BLOCK_OPEN,  // `/*`
    COMMENT_BLOCK_CLOSE, // `*/`
    NEWLINE,

    // --- Punctuation / Delimiters ---
    AT,        // `@`
    HASH,      // `#`
    LT,        // `<`
    GT,        // `>`
    L_PAREN,   // `(`
    R_PAREN,   // `)`
    L_BRACKET, // `[`
    R_BRACKET, // `]`
    L_BRACE,   // `{`
    R_BRACE,   // `}`
    COLON,     // `:`
    COMMA,     // `,`
    EQUAL,     // `=`
    MINUS,     // `-`
    PLUS,      // `+`
    STAR,      // `*`
    SLASH,     // `/`
    DOLLAR,    // `$`
    PERCENT,   // `%`
    PIPE,      // `|`
    /// The `|` that begins a later line of a `|` run: layout that folds away with the line break
    /// before it, not content. The `|` that opens the run stays a `PIPE`.
    MARGIN,
    BACKTICK,    // `
    DOT,         // `.`
    EXCLAMATION, // `!`
    QUESTION,    // `?`
    AMPERSAND,   // `&`
    TILDE,       // `~`
    SEMI,        // `;`
    UNDERSCORE,  // `_`
    QUOTE,       // `"` | `'`

    // --- Literals ---
    INT_NUMBER,
    FLOAT_NUMBER,
    STRING_LITERAL,
    RAW_STRING_LITERAL,
    TRUE_KW,
    FALSE_KW,
    NULL_KW,

    // --- Identifiers / Text Chunks ---
    IDENT,
    TEXT_CHUNK,

    // --- Composite Nodes (SyntaxNode) ---
    ROOT,
    DOCUMENT,
    BLOCK_FORM,
    INLINE_FORM,
    /// `=`-run heading plus every block up to the next section of the same or
    /// a shallower level. Sections nest by level; the first child is the
    /// [`SyntaxKind::SECTION_HEADING`].
    SECTION,
    /// The `=[ title ]` / `== title` line of a section, without its body.
    SECTION_HEADING,
    /// `---` or `---[ title ]---`.
    THEMATIC_BREAK,
    PARAGRAPH,
    LIST,
    LIST_ITEM,
    CODE_BLOCK,
    CALLOUT,
    SIGIL,
    /// `@a.b-c`, `::name`, `:name`, `<?name>`: an identifier or dot-separated
    /// name assembled from name tokens.
    NAME,
    /// A run of fence characters (`BACKTICK` for code blocks, `MINUS` for thematic breaks).
    FENCE,
    /// The info string on the opening line of a fenced code block (e.g. ` rust`).
    INFO_STRING,
    /// A line of the body of a fenced code block or line raw.
    CODE_LINE,
    /// The running text of a bracket-less sugar body (in a list item or section heading).
    BODY,
    ARGS,
    CONTENT,
    /// `#(foobar)` -- a single id slot, read after `ARGS`/`CONTENT`/
    /// `VALUE_DATA` but before any `CONNECT`.
    ID_GROUP,
    /// `:name(...)`, `:(...)`, `:{...}` -- a group joined to the preceding form.
    CONNECT,
    VALUE_DATA,
    VALUE_CHILDREN,
    MAP_ENTRY,
    MAP_LITERAL,
    SEQ_ITEM,
    INTERP_EXPR,
    /// `*em*` or `_em_`: the delimiters are the first and last tokens, the
    /// running text between them is read as in any paragraph.
    EMPHASIS,
    /// `**strong**` or `__strong__`.
    STRONG,
    /// `~~struck~~`.
    STRIKEOUT,
    /// `` `code` ``: the opening and closing `BACKTICK` tokens and the
    /// `TEXT_CHUNK` tokens between them, never read as syntax.
    CODE_SPAN,
    /// A bare `https://`, `http://` or `mailto:` address in running text,
    /// without the punctuation that closes the sentence around it.
    AUTOLINK,
    /// A `\` escape in running text (`\\`, or `\` followed by a run of the same
    /// ASCII punctuation character, such as `\;`, `\*`, `\**`, `` \` ``, `\//`):
    /// the leading `\` is omitted and the escaped characters are read as literal text.
    ESCAPE,
    /// `<name>`, `<?matcher(args){data}>` and the `[content]` a marked one takes
    /// after its `>`: a seat for a value that is not there yet. The head is
    /// inside the angle brackets; one family in the syntax whatever its mark.
    /// The AST gives it a node only where tomet interprets it (a hole is a
    /// `Hole`); a plain name tag lowers to text.
    ///
    /// A marked box is a form in the tree, so it stands in an `INLINE_FORM` or a
    /// `BLOCK_FORM` like `@name(...)` does; a name tag is a span of running text, like an
    /// `EMPHASIS` or a `CODE_SPAN`, and stands bare among the text. That is why the two are not
    /// wrapped alike.
    SLOT,
    /// A mistake of shape the tree keeps instead of refusing: the tree never fails, lowering turns
    /// the node into an error with the same message and offset. These include: a second group of a
    /// kind a form already has, the rest of the line after a `::` declaration, an unclosed `/*`
    /// block comment, an unterminated `[` content bracket, a titled thematic break with fewer than 3
    /// dashes or with trailing content, and a misaligned `|` marker.
    ERROR,
    /// A construct nested deeper than the parser reads (`MAX_DEPTH` in `tomet-parser`): its text, flat,
    /// with no structure inside, so that nothing which walks the tree recurses further than the limit.
    /// Lowering refuses a document that has one.
    TOO_DEEP,

    // Must be the last variant
    #[doc(hidden)]
    __LAST,
}

impl From<SyntaxKind> for rowan::SyntaxKind {
    fn from(kind: SyntaxKind) -> Self {
        Self(kind as u16)
    }
}

impl From<rowan::SyntaxKind> for SyntaxKind {
    fn from(kind: rowan::SyntaxKind) -> Self {
        assert!(
            kind.0 < SyntaxKind::__LAST as u16,
            "unknown syntax kind: {}",
            kind.0
        );
        unsafe { std::mem::transmute::<u16, SyntaxKind>(kind.0) }
    }
}

impl SyntaxKind {
    pub fn is_trivia(self) -> bool {
        matches!(
            self,
            SyntaxKind::WHITESPACE | SyntaxKind::COMMENT | SyntaxKind::NEWLINE
        )
    }

    pub fn is_literal(self) -> bool {
        matches!(
            self,
            SyntaxKind::INT_NUMBER
                | SyntaxKind::FLOAT_NUMBER
                | SyntaxKind::STRING_LITERAL
                | SyntaxKind::RAW_STRING_LITERAL
                | SyntaxKind::TRUE_KW
                | SyntaxKind::FALSE_KW
                | SyntaxKind::NULL_KW
        )
    }
}
