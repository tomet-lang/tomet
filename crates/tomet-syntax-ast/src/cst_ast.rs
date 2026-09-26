//! Typed AST wrappers over Rowan's [`SyntaxNode`].

use tomet_cst::{SyntaxKind, SyntaxNode, SyntaxToken, TextRange};

pub trait AstNode: Clone {
    fn can_cast(kind: SyntaxKind) -> bool;
    fn cast(syntax: SyntaxNode) -> Option<Self>;
    fn syntax(&self) -> &SyntaxNode;

    fn text_range(&self) -> TextRange {
        self.syntax().text_range()
    }

    fn text(&self) -> String {
        self.syntax().text().to_string()
    }
}

pub trait AstToken: Clone {
    fn can_cast(kind: SyntaxKind) -> bool;
    fn cast(syntax: SyntaxToken) -> Option<Self>;
    fn syntax(&self) -> &SyntaxToken;

    fn text_range(&self) -> TextRange {
        self.syntax().text_range()
    }

    fn text(&self) -> &str {
        self.syntax().text()
    }
}

macro_rules! define_ast_node {
    ($name:ident, $kind:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub struct $name {
            syntax: SyntaxNode,
        }

        impl AstNode for $name {
            fn can_cast(kind: SyntaxKind) -> bool {
                kind == SyntaxKind::$kind
            }

            fn cast(syntax: SyntaxNode) -> Option<Self> {
                if Self::can_cast(syntax.kind()) {
                    Some(Self { syntax })
                } else {
                    None
                }
            }

            fn syntax(&self) -> &SyntaxNode {
                &self.syntax
            }
        }
    };
}

define_ast_node!(CstRoot, ROOT);
define_ast_node!(CstHeading, HEADING);
define_ast_node!(CstBlockElement, BLOCK_ELEMENT);
define_ast_node!(CstParagraph, PARAGRAPH);
define_ast_node!(CstList, LIST);
define_ast_node!(CstListItem, LIST_ITEM);
define_ast_node!(CstCodeBlock, CODE_BLOCK);
define_ast_node!(CstSigil, SIGIL);
define_ast_node!(CstArgs, ARGS);
define_ast_node!(CstContent, CONTENT);
define_ast_node!(CstValueData, VALUE_DATA);
define_ast_node!(CstSection, SECTION);
define_ast_node!(CstSectionHeading, SECTION_HEADING);
define_ast_node!(CstThematicBreak, THEMATIC_BREAK);
define_ast_node!(CstInlineElement, INLINE_ELEMENT);
define_ast_node!(CstConnect, CONNECT);
define_ast_node!(CstFence, FENCE);
define_ast_node!(CstInterpExpr, INTERP_EXPR);
define_ast_node!(CstMapEntry, MAP_ENTRY);
define_ast_node!(CstSeqItem, SEQ_ITEM);

impl CstRoot {
    /// The top-level sections. Deeper ones are reached through [`CstSection::sections`].
    pub fn sections(&self) -> impl Iterator<Item = CstSection> {
        self.syntax().children().filter_map(CstSection::cast)
    }

    /// Legacy `#` headings.
    pub fn headings(&self) -> impl Iterator<Item = CstHeading> {
        self.syntax().children().filter_map(CstHeading::cast)
    }

    pub fn elements(&self) -> impl Iterator<Item = CstBlockElement> {
        self.syntax().children().filter_map(CstBlockElement::cast)
    }

    pub fn paragraphs(&self) -> impl Iterator<Item = CstParagraph> {
        self.syntax().children().filter_map(CstParagraph::cast)
    }

    pub fn lists(&self) -> impl Iterator<Item = CstList> {
        self.syntax().children().filter_map(CstList::cast)
    }

    pub fn code_blocks(&self) -> impl Iterator<Item = CstCodeBlock> {
        self.syntax().children().filter_map(CstCodeBlock::cast)
    }
}

impl CstBlockElement {
    pub fn sigil(&self) -> Option<CstSigil> {
        self.syntax().children().find_map(CstSigil::cast)
    }

    pub fn args(&self) -> Option<CstArgs> {
        self.syntax().children().find_map(CstArgs::cast)
    }

    pub fn content(&self) -> Option<CstContent> {
        self.syntax().children().find_map(CstContent::cast)
    }

    pub fn value_data(&self) -> Option<CstValueData> {
        self.syntax().children().find_map(CstValueData::cast)
    }
}

impl CstSection {
    pub fn heading(&self) -> Option<CstSectionHeading> {
        self.syntax().children().find_map(CstSectionHeading::cast)
    }

    pub fn level(&self) -> usize {
        self.heading().map_or(0, |h| h.level())
    }

    /// Directly nested sections, i.e. the ones one or more levels deeper.
    pub fn sections(&self) -> impl Iterator<Item = CstSection> {
        self.syntax().children().filter_map(CstSection::cast)
    }
}

impl CstSectionHeading {
    /// The number of leading `=`.
    pub fn level(&self) -> usize {
        self.syntax()
            .children_with_tokens()
            .filter_map(|e| e.into_token())
            .take_while(|t| t.kind() == SyntaxKind::EQUAL)
            .count()
    }

    pub fn content(&self) -> Option<CstContent> {
        self.syntax().children().find_map(CstContent::cast)
    }

    pub fn args(&self) -> Option<CstArgs> {
        self.syntax().children().find_map(CstArgs::cast)
    }

    pub fn value_data(&self) -> Option<CstValueData> {
        self.syntax().children().find_map(CstValueData::cast)
    }
}

impl CstInlineElement {
    pub fn sigil(&self) -> Option<CstSigil> {
        self.syntax().children().find_map(CstSigil::cast)
    }

    pub fn args(&self) -> Option<CstArgs> {
        self.syntax().children().find_map(CstArgs::cast)
    }

    pub fn content(&self) -> Option<CstContent> {
        self.syntax().children().find_map(CstContent::cast)
    }

    pub fn value_data(&self) -> Option<CstValueData> {
        self.syntax().children().find_map(CstValueData::cast)
    }

    pub fn connects(&self) -> impl Iterator<Item = CstConnect> {
        self.syntax().children().filter_map(CstConnect::cast)
    }
}

impl CstBlockElement {
    pub fn connects(&self) -> impl Iterator<Item = CstConnect> {
        self.syntax().children().filter_map(CstConnect::cast)
    }

    pub fn fence(&self) -> Option<CstFence> {
        self.syntax().children().find_map(CstFence::cast)
    }
}

impl CstArgs {
    pub fn entries(&self) -> impl Iterator<Item = CstMapEntry> {
        self.syntax().children().filter_map(CstMapEntry::cast)
    }
}

impl CstValueData {
    pub fn entries(&self) -> impl Iterator<Item = CstMapEntry> {
        self.syntax().children().filter_map(CstMapEntry::cast)
    }
}

impl CstMapEntry {
    /// The key token's text, quotes included when it is a string.
    pub fn key(&self) -> Option<String> {
        self.syntax()
            .children_with_tokens()
            .find_map(|e| e.into_token())
            .map(|t| t.text().to_string())
    }

    /// Everything after the first `:` token, trimmed.
    pub fn value_text(&self) -> String {
        let start = self.syntax().text_range().start();
        let colon = self
            .syntax()
            .children_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|t| t.kind() == SyntaxKind::COLON);
        match colon {
            Some(colon) => {
                let text = self.text();
                let offset = usize::from(colon.text_range().end() - start);
                text[offset..].trim().to_string()
            }
            None => String::new(),
        }
    }
}
