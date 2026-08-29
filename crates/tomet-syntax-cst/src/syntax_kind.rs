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
    NEWLINE,

    // --- Punctuation / Delimiters ---
    AT,          // `@`
    HASH,        // `#`
    LT,          // `<`
    GT,          // `>`
    L_PAREN,     // `(`
    R_PAREN,     // `)`
    L_BRACKET,   // `[`
    R_BRACKET,   // `]`
    L_BRACE,     // `{`
    R_BRACE,     // `}`
    COLON,       // `:`
    COMMA,       // `,`
    EQUAL,       // `=`
    MINUS,       // `-`
    PLUS,        // `+`
    STAR,        // `*`
    SLASH,       // `/`
    DOLLAR,      // `$`
    PERCENT,     // `%`
    PIPE,        // `|`
    BACKTICK,    // `
    DOT,         // `.`
    EXCLAMATION, // `!`
    QUESTION,    // `?`
    AMPERSAND,   // `&`
    TILDE,       // `~`
    CARET,       // `^`
    SEMI,        // `;`
    UNDERSCORE,  // `_`

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
    BLOCK_ELEMENT,
    INLINE_ELEMENT,
    HEADING,
    PARAGRAPH,
    LIST,
    LIST_ITEM,
    CODE_BLOCK,
    CALLOUT,
    SIGIL,
    ARGS,
    CONTENT,
    VALUE_DATA,
    VALUE_CHILDREN,
    MAP_ENTRY,
    MAP_LITERAL,
    SEQ_LITERAL,
    SEQ_ITEM,
    INTERP_EXPR,
    ERROR,

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
        assert!(kind.0 < SyntaxKind::__LAST as u16, "unknown syntax kind: {}", kind.0);
        unsafe { std::mem::transmute::<u16, SyntaxKind>(kind.0) }
    }
}

impl SyntaxKind {
    pub fn is_trivia(self) -> bool {
        matches!(self, SyntaxKind::WHITESPACE | SyntaxKind::COMMENT | SyntaxKind::NEWLINE)
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
