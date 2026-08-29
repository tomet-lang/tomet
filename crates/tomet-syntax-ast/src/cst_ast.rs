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

impl CstRoot {
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
