//! Typed AST wrappers over Rowan's [`SyntaxNode`].

use crate::{SyntaxKind, SyntaxNode, SyntaxToken, TextRange};

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
define_ast_node!(CstBlockForm, BLOCK_FORM);
define_ast_node!(CstParagraph, PARAGRAPH);
define_ast_node!(CstList, LIST);
define_ast_node!(CstListItem, LIST_ITEM);
// Note: `CstCodeBlock` / `CODE_BLOCK` refers to the concrete syntax construct
// (backtick fences), whereas the semantic element sigil is `@raw`.
define_ast_node!(CstCodeBlock, CODE_BLOCK);
define_ast_node!(CstSigil, SIGIL);
define_ast_node!(CstName, NAME);
define_ast_node!(CstFence, FENCE);
define_ast_node!(CstInfoString, INFO_STRING);
define_ast_node!(CstCodeLine, CODE_LINE);
define_ast_node!(CstSlot, SLOT);
define_ast_node!(CstArgs, ARGS);
define_ast_node!(CstContent, CONTENT);
define_ast_node!(CstValueData, VALUE_DATA);
define_ast_node!(CstSection, SECTION);
define_ast_node!(CstSectionHeading, SECTION_HEADING);
define_ast_node!(CstThematicBreak, THEMATIC_BREAK);
define_ast_node!(CstInlineForm, INLINE_FORM);
define_ast_node!(CstConnect, CONNECT);
define_ast_node!(CstIdGroup, ID_GROUP);
define_ast_node!(CstInterpExpr, INTERP_EXPR);
define_ast_node!(CstEscape, ESCAPE);
define_ast_node!(CstBody, BODY);
define_ast_node!(CstMapEntry, MAP_ENTRY);
define_ast_node!(CstSeqItem, SEQ_ITEM);
define_ast_node!(CstError, ERROR);

impl CstRoot {
    /// The top-level sections. Deeper ones are reached through [`CstSection::sections`].
    pub fn sections(&self) -> impl Iterator<Item = CstSection> {
        self.syntax().children().filter_map(CstSection::cast)
    }

    pub fn forms(&self) -> impl Iterator<Item = CstBlockForm> {
        self.syntax().children().filter_map(CstBlockForm::cast)
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

impl CstBlockForm {
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

    pub fn id(&self) -> Option<CstIdGroup> {
        self.syntax().children().find_map(CstIdGroup::cast)
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

    pub fn id(&self) -> Option<CstIdGroup> {
        self.syntax().children().find_map(CstIdGroup::cast)
    }

    pub fn body(&self) -> Option<CstBody> {
        self.syntax().children().find_map(CstBody::cast)
    }
}

impl CstInlineForm {
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

    pub fn id(&self) -> Option<CstIdGroup> {
        self.syntax().children().find_map(CstIdGroup::cast)
    }
}

impl CstConnect {
    pub fn name(&self) -> Option<CstName> {
        self.syntax().children().find_map(CstName::cast)
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

    pub fn id(&self) -> Option<CstIdGroup> {
        self.syntax().children().find_map(CstIdGroup::cast)
    }
}

impl CstListItem {
    pub fn args(&self) -> Option<CstArgs> {
        self.syntax().children().find_map(CstArgs::cast)
    }

    pub fn content(&self) -> Option<CstContent> {
        self.syntax().children().find_map(CstContent::cast)
    }

    pub fn value_data(&self) -> Option<CstValueData> {
        self.syntax().children().find_map(CstValueData::cast)
    }

    pub fn id(&self) -> Option<CstIdGroup> {
        self.syntax().children().find_map(CstIdGroup::cast)
    }

    pub fn body(&self) -> Option<CstBody> {
        self.syntax().children().find_map(CstBody::cast)
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

impl CstName {
    /// Returns the namespace (if any) and the name.
    pub fn parts(&self) -> (Option<String>, String) {
        let mut segments: Vec<String> = Vec::new();
        let mut current = String::new();
        for token in self
            .syntax()
            .children_with_tokens()
            .filter_map(|e| e.into_token())
        {
            if token.kind() == SyntaxKind::DOT {
                if current
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                {
                    segments.push(std::mem::take(&mut current));
                } else {
                    current.clear();
                    break;
                }
            } else {
                current.push_str(token.text());
            }
        }
        if current
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        {
            segments.push(current);
        }
        let name = segments.pop().unwrap_or_default();
        let namespace = if segments.is_empty() {
            None
        } else {
            Some(segments.join("."))
        };
        (namespace, name)
    }
}

impl CstSigil {
    pub fn name(&self) -> Option<CstName> {
        self.syntax().children().find_map(CstName::cast)
    }
}

/// What a box (`SLOT`) is, told by what stands right after its `<`: nothing, `?` or `$`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CstSlotMark {
    /// `<PATH>`: no mark. A label for the reader, which is running text.
    NameTag,
    /// `<?matcher(args){data}>[content]`: the `?` mark. A hole.
    Hole,
    /// `<$type(args)>`: the `$` mark. A seat for a value of the type written.
    TypeSeat,
}

impl CstSlotMark {
    /// The mark that a token of `kind` right after a `<` makes: no token of a mark is no mark.
    pub fn of_token(kind: Option<SyntaxKind>) -> Self {
        match kind {
            Some(SyntaxKind::QUESTION) => Self::Hole,
            Some(SyntaxKind::DOLLAR) => Self::TypeSeat,
            _ => Self::NameTag,
        }
    }

    /// Whether a box with this mark is a form of the tree, standing in an `INLINE_FORM` or a
    /// `BLOCK_FORM` like `@name(...)`; a name tag is a span of running text.
    pub fn is_form(self) -> bool {
        self != Self::NameTag
    }
}

impl CstSlot {
    pub fn name(&self) -> Option<CstName> {
        self.syntax().children().find_map(CstName::cast)
    }

    /// The box's mark: what follows its `<`.
    pub fn mark(&self) -> CstSlotMark {
        let after_lt = self.syntax().children_with_tokens().nth(1);
        CstSlotMark::of_token(after_lt.map(|e| e.kind()))
    }
}

impl CstFence {
    /// The number of delimiter characters in the fence.
    pub fn count(&self) -> usize {
        usize::from(self.syntax().text().len())
    }
}

impl CstInfoString {
    /// The language tag (first word) in the info string, if any.
    pub fn lang(&self) -> Option<String> {
        self.text().split_whitespace().next().map(|s| s.to_string())
    }
}

impl CstCodeLine {
    /// The mark (e.g. `` `` `` in a line raw), if present.
    pub fn fence(&self) -> Option<CstFence> {
        self.syntax().children().find_map(CstFence::cast)
    }
}

impl CstCodeBlock {
    pub fn opening_fence(&self) -> Option<CstFence> {
        self.syntax().children().find_map(CstFence::cast)
    }

    pub fn info_string(&self) -> Option<CstInfoString> {
        self.syntax().children().find_map(CstInfoString::cast)
    }

    pub fn lines(&self) -> impl Iterator<Item = CstCodeLine> {
        self.syntax().children().filter_map(CstCodeLine::cast)
    }

    pub fn closing_fence(&self) -> Option<CstFence> {
        self.syntax().children().filter_map(CstFence::cast).nth(1)
    }

    /// Whether this block is a line-raw construct (`` `` `` at start of each line).
    pub fn is_line_raw(&self) -> bool {
        self.syntax()
            .first_child()
            .is_some_and(|c| c.kind() == SyntaxKind::CODE_LINE)
    }
}

impl CstThematicBreak {
    pub fn opening_fence(&self) -> Option<CstFence> {
        self.syntax().children().find_map(CstFence::cast)
    }

    pub fn content(&self) -> Option<CstContent> {
        self.syntax().children().find_map(CstContent::cast)
    }

    pub fn closing_fence(&self) -> Option<CstFence> {
        self.syntax().children().filter_map(CstFence::cast).nth(1)
    }
}
