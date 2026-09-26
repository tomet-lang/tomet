//! Resilient, lossless Concrete Syntax Tree (CST) parser for Tomet.
//!
//! Preserves 100% of source characters including trivia (whitespace, newlines, comments).
//!
//! The tree follows the current surface syntax, not the AST parser's exact
//! decisions: it never fails, and where the two could disagree it prefers
//! the reading that keeps more structure.
//!
//! ```text
//! ROOT
//!   SECTION                    `=[ T ]` and everything up to the next section
//!     SECTION_HEADING          of the same or a shallower level (they nest)
//!     PARAGRAPH | LIST | BLOCK_ELEMENT | CODE_BLOCK | THEMATIC_BREAK | SECTION ...
//! ```
//!
//! An element (`BLOCK_ELEMENT`, `INLINE_ELEMENT`) is a `SIGIL` followed by
//! its groups -- `ARGS` `( )`, `VALUE_DATA` `{ }`, `CONTENT` `[ ]`, at most
//! one of each -- then any `CONNECT` (`:name(...)`) and `FENCE` (`+++`).
//! `${ ... }` is an `INTERP_EXPR`. Inside `ARGS`/`VALUE_DATA` an entry is a
//! `MAP_ENTRY` (`key: value`) or a `SEQ_ITEM`, and nested groups are
//! `MAP_LITERAL`, `SEQ_LITERAL` or (a call such as `list(...)`) `ARGS`.
//! `CONTENT` holds inline text, which may itself contain elements.
//!
//! Emphasis (`*em*`) and `|`-marked content runs are not modelled: they stay
//! flat tokens in their paragraph.

use tomet_cst::{GreenNodeBuilder, SyntaxKind, SyntaxNode};
use tomet_lexer::tokenize;

use SyntaxKind as K;

/// Parse `src` into a lossless, resilient [`SyntaxNode`].
///
/// Guaranteed Invariant: `parse_cst(src).text().to_string() == src`.
pub fn parse_cst(src: &str) -> SyntaxNode {
    let raw_tokens = tokenize(src);
    let mut parser = CstParser::new(&raw_tokens);
    parser.parse_root();
    SyntaxNode::new_root(parser.builder.finish())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum InlineStop {
    /// Up to and including the newline.
    Line,
    /// Up to (not including) the `]` that closes the enclosing `[`.
    Bracket,
    /// Up to and including the newline that ends the paragraph.
    Paragraph,
}

struct CstParser<'a> {
    tokens: &'a [(SyntaxKind, &'a str)],
    pos: usize,
    builder: GreenNodeBuilder<'static>,
    /// Whether an element may claim a `:name(...)` group that follows it.
    /// A list item's own top-level element may not: the group is the item's.
    allow_connect: bool,
}

impl<'a> CstParser<'a> {
    fn new(tokens: &'a [(SyntaxKind, &'a str)]) -> Self {
        Self {
            tokens,
            pos: 0,
            builder: GreenNodeBuilder::new(),
            allow_connect: true,
        }
    }

    // --- token access -------------------------------------------------

    fn is_eof(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    fn kind_at(&self, i: usize) -> Option<SyntaxKind> {
        self.tokens.get(i).map(|(k, _)| *k)
    }

    fn text_at(&self, i: usize) -> &'a str {
        self.tokens.get(i).map_or("", |(_, t)| *t)
    }

    fn current_kind(&self) -> Option<SyntaxKind> {
        self.kind_at(self.pos)
    }

    fn bump(&mut self) {
        if let Some((kind, text)) = self.tokens.get(self.pos).copied() {
            self.builder.token(kind.into(), text);
            self.pos += 1;
        }
    }

    fn bump_while(&mut self, kind: SyntaxKind) {
        while self.current_kind() == Some(kind) {
            self.bump();
        }
    }

    fn skip_ws_from(&self, mut i: usize) -> usize {
        while self.kind_at(i) == Some(K::WHITESPACE) {
            i += 1;
        }
        i
    }

    fn run_len(&self, i: usize, kind: SyntaxKind) -> usize {
        let mut n = 0;
        while self.kind_at(i + n) == Some(kind) {
            n += 1;
        }
        n
    }

    fn at_line_start(&self) -> bool {
        let mut i = self.pos;
        while i > 0 && self.kind_at(i - 1) == Some(K::WHITESPACE) {
            i -= 1;
        }
        i == 0 || self.kind_at(i - 1) == Some(K::NEWLINE)
    }

    fn is_opener(kind: Option<SyntaxKind>) -> bool {
        matches!(kind, Some(K::L_PAREN | K::L_BRACKET | K::L_BRACE))
    }

    // --- lookahead ------------------------------------------------------

    /// Index just past the group opened at `i`, or `None` if it never closes.
    fn group_end(&self, i: usize) -> Option<usize> {
        let (open, close) = match self.kind_at(i)? {
            K::L_PAREN => (K::L_PAREN, K::R_PAREN),
            K::L_BRACKET => (K::L_BRACKET, K::R_BRACKET),
            K::L_BRACE => (K::L_BRACE, K::R_BRACE),
            _ => return None,
        };
        let mut depth = 0usize;
        let mut j = i;
        while let Some(k) = self.kind_at(j) {
            if k == open {
                depth += 1;
            } else if k == close {
                depth -= 1;
                if depth == 0 {
                    return Some(j + 1);
                }
            }
            j += 1;
        }
        None
    }

    /// After an element's name (index `i`): does a group, a `:` connect or a
    /// `+++` fence follow, so that it is an element and not plain text?
    fn groups_follow(&self, i: usize) -> bool {
        let i = self.skip_ws_from(i);
        match self.kind_at(i) {
            Some(K::L_PAREN | K::L_BRACKET | K::L_BRACE) => true,
            Some(K::COLON) => {
                Self::is_opener(self.kind_at(i + 1)) || self.kind_at(i + 1) == Some(K::IDENT)
            }
            Some(K::PLUS) => self.run_len(i, K::PLUS) >= 3,
            _ => false,
        }
    }

    /// Index just past everything an element starting at `i` (its sigil is
    /// `@name`) owns, using only bracket matching.
    fn element_end(&self, mut i: usize) -> usize {
        i += 2; // `@` and the name
        let mut seen = [false; 3]; // args, content, value: each at most once
        loop {
            let j = self.skip_ws_from(i);
            match self.kind_at(j) {
                Some(k @ (K::L_PAREN | K::L_BRACKET | K::L_BRACE)) => {
                    let slot =
                        &mut seen[(k == K::L_BRACKET) as usize + 2 * (k == K::L_BRACE) as usize];
                    if std::mem::replace(slot, true) {
                        return i;
                    }
                    match self.group_end(j) {
                        Some(e) => i = e,
                        None => return self.tokens.len(),
                    }
                }
                Some(K::COLON) => {
                    let mut e = j + 1;
                    if self.kind_at(e) == Some(K::IDENT) {
                        e += 1;
                    }
                    if Self::is_opener(self.kind_at(e)) {
                        match self.group_end(e) {
                            Some(e) => i = e,
                            None => return self.tokens.len(),
                        }
                    } else {
                        return i;
                    }
                }
                Some(K::PLUS) if self.run_len(j, K::PLUS) >= 3 => {
                    return j + self.run_len(j, K::PLUS);
                }
                _ => return i,
            }
        }
    }

    /// Is the `@` at `i` a block element: an element that owns the rest of
    /// its line?
    fn is_block_element_at(&self, i: usize) -> bool {
        if self.kind_at(i) != Some(K::AT) || self.kind_at(i + 1) != Some(K::IDENT) {
            return false;
        }
        let end = self.element_end(i);
        let mut j = end;
        while matches!(self.kind_at(j), Some(K::WHITESPACE | K::COMMENT)) {
            j += 1;
        }
        // A bare `@name` is an element only as a block; a grouped one is a
        // block only when nothing but a comment follows it on its line.
        self.kind_at(j).is_none_or(|k| k == K::NEWLINE)
    }

    /// `=`-run at `pos` that opens a section: its level.
    fn section_level_at(&self, i: usize) -> Option<usize> {
        let n = self.run_len(i, K::EQUAL);
        if n == 0 {
            return None;
        }
        match self.kind_at(i + n) {
            None
            | Some(
                K::NEWLINE
                | K::COMMENT
                | K::WHITESPACE
                | K::L_PAREN
                | K::L_BRACKET
                | K::L_BRACE
                | K::PIPE,
            ) => Some(n),
            _ => None,
        }
    }

    fn section_level_here(&self) -> Option<usize> {
        if self.current_kind() == Some(K::EQUAL) && self.at_line_start() {
            self.section_level_at(self.pos)
        } else {
            None
        }
    }

    fn is_thematic_break_at(&self, i: usize) -> bool {
        if self.run_len(i, K::MINUS) < 3 {
            return false;
        }
        let j = self.skip_ws_from(i + self.run_len(i, K::MINUS));
        matches!(
            self.kind_at(j),
            None | Some(K::NEWLINE | K::COMMENT | K::L_BRACKET)
        )
    }

    fn is_list_start_at(&self, i: usize) -> bool {
        if self.kind_at(i) != Some(K::MINUS) {
            return false;
        }
        let next = self.kind_at(i + 1);
        next == Some(K::WHITESPACE)
            || (next == Some(K::DOT) && self.kind_at(i + 2) == Some(K::WHITESPACE))
    }

    fn is_code_block_start_at(&self, i: usize) -> bool {
        self.run_len(i, K::BACKTICK) >= 3
    }

    /// Does a block other than a paragraph start at `i` (the first token of a line)?
    fn block_starts_at(&self, i: usize) -> bool {
        match self.kind_at(i) {
            Some(K::EQUAL) => self.section_level_at(i).is_some(),
            Some(K::MINUS) => self.is_thematic_break_at(i) || self.is_list_start_at(i),
            Some(K::BACKTICK) => self.is_code_block_start_at(i),
            Some(K::HASH | K::LT) => true,
            Some(K::AT) => self.is_block_element_at(i),
            _ => false,
        }
    }

    /// At a `NEWLINE` inside a paragraph: does the paragraph end here?
    fn paragraph_ends_after_newline(&self) -> bool {
        // A trailing `\` continues the line.
        let mut b = self.pos;
        while b > 0 && self.kind_at(b - 1) == Some(K::WHITESPACE) {
            b -= 1;
        }
        if b > 0 && self.kind_at(b - 1) == Some(K::TEXT_CHUNK) && self.text_at(b - 1) == "\\" {
            return false;
        }
        let i = self.skip_ws_from(self.pos + 1);
        matches!(self.kind_at(i), None | Some(K::NEWLINE)) || self.block_starts_at(i)
    }

    // --- blocks -----------------------------------------------------------

    fn parse_root(&mut self) {
        self.builder.start_node(K::ROOT.into());
        self.parse_body(0);
        self.builder.finish_node();
    }

    /// Blocks until EOF, or until a section of level `<= section_level` starts.
    fn parse_body(&mut self, section_level: usize) {
        while !self.is_eof() {
            if self.current_kind().is_some_and(|k| k.is_trivia()) {
                self.bump();
                continue;
            }
            if self
                .section_level_here()
                .is_some_and(|l| l <= section_level)
            {
                break;
            }
            self.parse_block();
        }
    }

    fn parse_block(&mut self) {
        let Some(kind) = self.current_kind() else {
            return;
        };
        match kind {
            K::EQUAL if self.section_level_here().is_some() => self.parse_section(),
            K::MINUS if self.at_line_start() && self.is_thematic_break_at(self.pos) => {
                self.parse_thematic_break()
            }
            K::MINUS if self.is_list_start_at(self.pos) => self.parse_list(),
            K::HASH => self.parse_heading(),
            K::BACKTICK if self.is_code_block_start_at(self.pos) => self.parse_code_block(),
            K::AT if self.is_block_element_at(self.pos) => self.parse_block_element(),
            K::LT => self.parse_block_element(),
            _ => self.parse_paragraph(),
        }
    }

    fn parse_section(&mut self) {
        let level = self.section_level_here().unwrap_or(1);
        self.builder.start_node(K::SECTION.into());
        self.builder.start_node(K::SECTION_HEADING.into());
        self.bump_while(K::EQUAL);
        self.bump_while(K::WHITESPACE);
        if Self::is_opener(self.current_kind()) {
            self.parse_groups();
            // Decorative trailing `=`, which may itself be followed by groups.
            loop {
                let j = self.skip_ws_from(self.pos);
                if self.kind_at(j) != Some(K::EQUAL) {
                    break;
                }
                self.bump_while(K::WHITESPACE);
                self.bump_while(K::EQUAL);
                self.parse_groups();
            }
        }
        self.parse_inline(InlineStop::Line);
        self.builder.finish_node();
        self.parse_body(level);
        self.builder.finish_node();
    }

    fn parse_thematic_break(&mut self) {
        self.builder.start_node(K::THEMATIC_BREAK.into());
        self.bump_while(K::MINUS);
        self.bump_while(K::WHITESPACE);
        if self.current_kind() == Some(K::L_BRACKET) {
            self.parse_content_group();
        }
        self.parse_inline(InlineStop::Line);
        self.builder.finish_node();
    }

    fn parse_heading(&mut self) {
        self.builder.start_node(K::HEADING.into());
        self.bump_while(K::HASH);
        self.bump_while(K::WHITESPACE);
        self.parse_groups();
        self.parse_inline(InlineStop::Line);
        self.builder.finish_node();
    }

    fn parse_block_element(&mut self) {
        self.builder.start_node(K::BLOCK_ELEMENT.into());
        self.parse_sigil();
        self.parse_groups();
        while matches!(self.current_kind(), Some(K::WHITESPACE | K::COMMENT)) {
            self.bump();
        }
        self.builder.finish_node();
    }

    fn parse_list(&mut self) {
        self.builder.start_node(K::LIST.into());
        while self.is_list_start_at(self.pos) {
            self.parse_list_item();
        }
        self.builder.finish_node();
    }

    fn parse_list_item(&mut self) {
        self.builder.start_node(K::LIST_ITEM.into());
        self.bump(); // '-'
        if self.current_kind() == Some(K::DOT) {
            self.bump();
        }
        self.bump_while(K::WHITESPACE);
        let outer = std::mem::replace(&mut self.allow_connect, false);
        // The item's own groups. Reading them here keeps a bracketed item,
        // which may span lines, inside one node.
        self.parse_groups();
        self.parse_inline(InlineStop::Line);
        self.allow_connect = outer;
        self.builder.finish_node();
    }

    fn parse_code_block(&mut self) {
        self.builder.start_node(K::CODE_BLOCK.into());

        let mut count = 0;
        while self.current_kind() == Some(K::BACKTICK) {
            self.bump();
            count += 1;
        }

        // Consume until matching backticks or EOF
        let mut consecutive_backticks = 0;
        while let Some(k) = self.current_kind() {
            if k == K::BACKTICK {
                consecutive_backticks += 1;
                self.bump();
                if consecutive_backticks == count {
                    break;
                }
            } else {
                consecutive_backticks = 0;
                self.bump();
            }
        }

        self.builder.finish_node();
    }

    fn parse_paragraph(&mut self) {
        self.builder.start_node(K::PARAGRAPH.into());
        self.parse_inline(InlineStop::Paragraph);
        self.builder.finish_node();
    }

    // --- inline -------------------------------------------------------------

    fn parse_inline(&mut self, stop: InlineStop) {
        let mut depth = 0usize;
        while let Some(k) = self.current_kind() {
            match stop {
                InlineStop::Line if k == K::NEWLINE => {
                    self.bump();
                    break;
                }
                InlineStop::Paragraph if k == K::NEWLINE && self.paragraph_ends_after_newline() => {
                    self.bump();
                    break;
                }
                InlineStop::Bracket => match k {
                    K::R_BRACKET if depth == 0 => break,
                    K::R_BRACKET => depth -= 1,
                    K::L_BRACKET => depth += 1,
                    _ => {}
                },
                _ => {}
            }
            if !self.try_parse_inline_element() {
                self.bump();
            }
        }
    }

    /// Parses an element at the cursor if the tokens form one.
    fn try_parse_inline_element(&mut self) -> bool {
        let p = self.pos;
        let is_element = match self.current_kind() {
            Some(K::AT) => self.kind_at(p + 1) == Some(K::IDENT) && self.groups_follow(p + 2),
            Some(K::CARET) => {
                self.kind_at(p + 1) == Some(K::L_PAREN)
                    || (self.kind_at(p + 1) == Some(K::IDENT)
                        && self.kind_at(p + 2) == Some(K::L_PAREN))
            }
            Some(K::DOLLAR) => {
                self.kind_at(p + 1) == Some(K::L_BRACE)
                    || (self.kind_at(p + 1) == Some(K::IDENT)
                        && self.kind_at(p + 2) == Some(K::L_PAREN))
            }
            _ => false,
        };
        if !is_element {
            return false;
        }
        self.builder.start_node(K::INLINE_ELEMENT.into());
        let dollar = self.current_kind() == Some(K::DOLLAR);
        self.parse_sigil();
        if dollar {
            match self.current_kind() {
                Some(K::L_BRACE) => self.parse_interp_expr(),
                Some(K::L_PAREN) => self.parse_args_group(),
                _ => {}
            }
        } else {
            self.parse_groups();
        }
        self.builder.finish_node();
        true
    }

    // --- elements ---------------------------------------------------------

    fn parse_sigil(&mut self) {
        self.builder.start_node(K::SIGIL.into());
        match self.current_kind() {
            Some(K::LT) => {
                self.bump();
                while let Some(k) = self.current_kind() {
                    if matches!(k, K::GT | K::NEWLINE) {
                        break;
                    }
                    self.bump();
                }
                if self.current_kind() == Some(K::GT) {
                    self.bump();
                }
            }
            Some(_) => {
                self.bump();
                if self.current_kind() == Some(K::IDENT) {
                    self.bump();
                }
            }
            None => {}
        }
        self.builder.finish_node();
    }

    /// An element's `(args)`, `{value}` and `[content]` (at most one of each,
    /// in any order), then its `:name(...)` connects and `+++` fence.
    fn parse_groups(&mut self) {
        let (mut args, mut value, mut content) = (false, false, false);
        loop {
            let j = self.skip_ws_from(self.pos);
            match self.kind_at(j) {
                Some(K::L_PAREN) if !args => {
                    args = true;
                    self.bump_while(K::WHITESPACE);
                    self.parse_args_group();
                }
                Some(K::L_BRACE) if !value => {
                    value = true;
                    self.bump_while(K::WHITESPACE);
                    self.parse_value_group();
                }
                Some(K::L_BRACKET) if !content => {
                    content = true;
                    self.bump_while(K::WHITESPACE);
                    self.parse_content_group();
                }
                Some(K::COLON) if self.allow_connect && self.connect_follows(j) => {
                    self.bump_while(K::WHITESPACE);
                    self.parse_connect();
                }
                Some(K::PLUS) if self.run_len(j, K::PLUS) >= 3 => {
                    self.bump_while(K::WHITESPACE);
                    self.parse_fence();
                    break;
                }
                _ => break,
            }
        }
    }

    fn connect_follows(&self, colon: usize) -> bool {
        let mut e = colon + 1;
        if self.kind_at(e) == Some(K::IDENT) {
            e += 1;
        }
        Self::is_opener(self.kind_at(e))
    }

    fn parse_connect(&mut self) {
        self.builder.start_node(K::CONNECT.into());
        self.bump(); // ':'
        if self.current_kind() == Some(K::IDENT) {
            self.bump();
        }
        match self.current_kind() {
            Some(K::L_PAREN) => self.parse_args_group(),
            Some(K::L_BRACE) => self.parse_value_group(),
            Some(K::L_BRACKET) => self.parse_content_group(),
            _ => {}
        }
        self.builder.finish_node();
    }

    fn parse_fence(&mut self) {
        self.builder.start_node(K::FENCE.into());
        // The closing line must be a run at least as long as the opening one.
        let open = self.run_len(self.pos, K::PLUS);
        self.bump_while(K::PLUS);
        loop {
            // Rest of the line, then the newline.
            while let Some(k) = self.current_kind() {
                self.bump();
                if k == K::NEWLINE {
                    break;
                }
            }
            if self.is_eof() {
                break;
            }
            let j = self.skip_ws_from(self.pos);
            if self.kind_at(j) == Some(K::PLUS) && self.run_len(j, K::PLUS) >= open {
                self.bump_while(K::WHITESPACE);
                self.bump_while(K::PLUS);
                break;
            }
        }
        self.builder.finish_node();
    }

    fn parse_interp_expr(&mut self) {
        self.builder.start_node(K::INTERP_EXPR.into());
        self.bump(); // '{'
        let mut depth = 1usize;
        while let Some(k) = self.current_kind() {
            match k {
                K::L_BRACE => depth += 1,
                K::R_BRACE => {
                    depth -= 1;
                    if depth == 0 {
                        self.bump();
                        break;
                    }
                }
                _ => {}
            }
            self.bump();
        }
        self.builder.finish_node();
    }

    // --- groups and values -----------------------------------------------------

    fn parse_args_group(&mut self) {
        self.builder.start_node(K::ARGS.into());
        self.bump(); // '('
        self.parse_entries(K::R_PAREN);
        self.builder.finish_node();
    }

    fn parse_value_group(&mut self) {
        self.builder.start_node(K::VALUE_DATA.into());
        self.bump(); // '{'
        self.parse_entries(K::R_BRACE);
        self.builder.finish_node();
    }

    fn parse_content_group(&mut self) {
        self.builder.start_node(K::CONTENT.into());
        self.bump(); // '['
        let outer = std::mem::replace(&mut self.allow_connect, true);
        self.parse_inline(InlineStop::Bracket);
        self.allow_connect = outer;
        if self.current_kind() == Some(K::R_BRACKET) {
            self.bump();
        }
        self.builder.finish_node();
    }

    fn parse_entries(&mut self, close: SyntaxKind) {
        while let Some(k) = self.current_kind() {
            if k == close {
                self.bump();
                break;
            }
            if k.is_trivia() || k == K::COMMA {
                self.bump();
            } else {
                self.parse_entry(close);
            }
        }
    }

    fn parse_entry(&mut self, close: SyntaxKind) {
        let is_map = matches!(
            self.current_kind(),
            Some(
                K::IDENT
                    | K::STRING_LITERAL
                    | K::INT_NUMBER
                    | K::TRUE_KW
                    | K::FALSE_KW
                    | K::NULL_KW
            )
        ) && self.kind_at(self.skip_ws_from(self.pos + 1)) == Some(K::COLON);
        if is_map {
            self.builder.start_node(K::MAP_ENTRY.into());
            self.bump(); // key
            self.bump_while(K::WHITESPACE);
            self.bump(); // ':'
        } else {
            self.builder.start_node(K::SEQ_ITEM.into());
        }
        self.parse_value(close);
        self.builder.finish_node();
    }

    /// One value, ending before a `,`, a newline or the enclosing `close`.
    fn parse_value(&mut self, close: SyntaxKind) {
        while let Some(k) = self.current_kind() {
            if k == close || matches!(k, K::COMMA | K::NEWLINE) {
                break;
            }
            match k {
                K::L_BRACE => {
                    self.builder.start_node(K::MAP_LITERAL.into());
                    self.bump();
                    self.parse_entries(K::R_BRACE);
                    self.builder.finish_node();
                }
                K::L_BRACKET => {
                    self.builder.start_node(K::SEQ_LITERAL.into());
                    self.bump();
                    self.parse_entries(K::R_BRACKET);
                    self.builder.finish_node();
                }
                K::L_PAREN => self.parse_args_group(),
                _ => {
                    if !self.try_parse_inline_element() {
                        self.bump();
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Kinds of `node`'s child nodes, in order.
    fn child_kinds(node: &SyntaxNode) -> Vec<SyntaxKind> {
        node.children().map(|n| n.kind()).collect()
    }

    fn find(root: &SyntaxNode, kind: SyntaxKind) -> Option<SyntaxNode> {
        root.descendants().find(|n| n.kind() == kind)
    }

    fn count(root: &SyntaxNode, kind: SyntaxKind) -> usize {
        root.descendants().filter(|n| n.kind() == kind).count()
    }

    #[test]
    fn test_cst_lossless_roundtrip_all_syntaxes() {
        let cases = [
            "#[ Heading ]\n\nParagraph text with *em* and @tag[ content ].\n",
            "@config(format:json)+++\n{\n  \"meta\": \"yaml\"\n}\n+++\n",
            "- item 1\n- item 2\n-. ordered 1\n\n```rust\nfn main() {}\n```\n",
            "// Comment line\n@card(id: 123){ priority: high }[ Note ]\n",
            "=[ A ]=\n== B ==\n=\n---\n---[ x ]---\n\ntext ^n(1) ${a.b} $f(x) @l[t](ref:y):rule(a)\n",
            // Unterminated everything: still lossless.
            "@a(b: {c: [1, 2\n=[ open\n${ x\n@f[ never closed",
            "",
            "\n\n  \n",
        ];

        for case in cases {
            let root = parse_cst(case);
            let roundtripped = root.text().to_string();
            assert_eq!(roundtripped, case);
        }
    }

    #[test]
    fn test_cst_node_hierarchy() {
        let src = "#[ Title ]\n\nParagraph\n";
        let root = parse_cst(src);
        assert_eq!(root.kind(), K::ROOT);

        let children: Vec<_> = root.children().map(|n| n.kind()).collect();
        assert!(children.contains(&K::HEADING));
        assert!(children.contains(&K::PARAGRAPH));
    }

    #[test]
    fn sections_nest_by_level() {
        let src = "=[ A ]\n\ntext\n\n==[ A1 ]\n\ninner\n\n==[ A2 ]\n\n=[ B ]\n\nlast\n";
        let root = parse_cst(src);
        assert_eq!(child_kinds(&root), [K::SECTION, K::SECTION]);
        let a = root.children().next().unwrap();
        assert_eq!(
            child_kinds(&a),
            [K::SECTION_HEADING, K::PARAGRAPH, K::SECTION, K::SECTION]
        );
        let a1 = a.children().nth(2).unwrap();
        assert_eq!(child_kinds(&a1), [K::SECTION_HEADING, K::PARAGRAPH]);
        assert_eq!(count(&root, K::PARAGRAPH), 3);
    }

    #[test]
    fn section_forms() {
        // Bracketed, sugar, decorative `=`, and heading-less.
        for src in [
            "=[ T ]\n",
            "= T\n",
            "=[ T ]=\n",
            "== T ==\n",
            "=\n",
            "== // c\n",
        ] {
            let root = parse_cst(src);
            assert_eq!(child_kinds(&root), [K::SECTION], "{src:?}");
            assert_eq!(root.text().to_string(), src);
        }
        // `==mark==` is inline emphasis, not a section.
        let root = parse_cst("==mark== text\n");
        assert_eq!(child_kinds(&root), [K::PARAGRAPH]);
        // A bracketed title is CONTENT inside the heading.
        let heading = find(&parse_cst("=[ T ]\n"), K::SECTION_HEADING).unwrap();
        assert_eq!(child_kinds(&heading), [K::CONTENT]);
    }

    #[test]
    fn thematic_breaks() {
        let root = parse_cst("---\n---[ Title ]---\n- item\n");
        assert_eq!(
            child_kinds(&root),
            [K::THEMATIC_BREAK, K::THEMATIC_BREAK, K::LIST]
        );
    }

    #[test]
    fn inline_elements_are_structured() {
        let src = "See @link[label](ref:x) and ^note(1) and ${a.b} and $sum(1, 2).\n";
        let root = parse_cst(src);
        let para = find(&root, K::PARAGRAPH).unwrap();
        let elements: Vec<_> = para
            .children()
            .filter(|n| n.kind() == K::INLINE_ELEMENT)
            .collect();
        assert_eq!(elements.len(), 4);

        assert_eq!(child_kinds(&elements[0]), [K::SIGIL, K::CONTENT, K::ARGS]);
        assert_eq!(elements[0].text().to_string(), "@link[label](ref:x)");
        assert_eq!(child_kinds(&elements[1]), [K::SIGIL, K::ARGS]);
        assert_eq!(elements[1].text().to_string(), "^note(1)");
        assert_eq!(child_kinds(&elements[2]), [K::SIGIL, K::INTERP_EXPR]);
        assert_eq!(elements[2].text().to_string(), "${a.b}");
        assert_eq!(child_kinds(&elements[3]), [K::SIGIL, K::ARGS]);

        // `ref:x` is a structured entry.
        let entry = find(&elements[0], K::MAP_ENTRY).unwrap();
        assert_eq!(entry.text().to_string(), "ref:x");
    }

    #[test]
    fn plain_at_and_caret_are_text() {
        for src in [
            "mail me @ home\n",
            "a@b.com\n",
            "x ^ y\n",
            "cost $5\n",
            "@name\n\n",
        ] {
            let root = parse_cst(src);
            assert_eq!(count(&root, K::INLINE_ELEMENT), 0, "{src:?}");
        }
    }

    #[test]
    fn inline_elements_nest_in_content() {
        let root = parse_cst("@note[ see @link[x](ref:y) ]{ k: v }\n");
        let el = find(&root, K::BLOCK_ELEMENT).unwrap();
        assert_eq!(child_kinds(&el), [K::SIGIL, K::CONTENT, K::VALUE_DATA]);
        let content = find(&el, K::CONTENT).unwrap();
        assert_eq!(count(&content, K::INLINE_ELEMENT), 1);
    }

    #[test]
    fn block_element_needs_a_bare_line() {
        // Ends its line: a block. Text follows: part of a paragraph.
        assert_eq!(
            child_kinds(&parse_cst("@card(id: a){ p: 1 }\n")),
            [K::BLOCK_ELEMENT]
        );
        assert_eq!(child_kinds(&parse_cst("@meta\n")), [K::BLOCK_ELEMENT]);
        let root = parse_cst("@link[a](ref:b) trailing\n");
        assert_eq!(child_kinds(&root), [K::PARAGRAPH]);
        assert_eq!(count(&root, K::INLINE_ELEMENT), 1);
    }

    #[test]
    fn groups_are_structured() {
        let root = parse_cst("@x(a: 1, list(p, q), \"k\": { n: [1, 2] })\n");
        let args = find(&root, K::ARGS).unwrap();
        assert_eq!(
            child_kinds(&args),
            [K::MAP_ENTRY, K::SEQ_ITEM, K::MAP_ENTRY]
        );
        // `list(p, q)` is a call: an ARGS inside the second entry.
        let item = args.children().nth(1).unwrap();
        assert_eq!(item.text().to_string(), "list(p, q)");
        assert_eq!(count(&item, K::ARGS), 1);
        assert_eq!(count(&root, K::MAP_LITERAL), 1);
        assert_eq!(count(&root, K::SEQ_LITERAL), 1);
        // `list(p, q)`, its `p` and `q`, and the two numbers.
        assert_eq!(count(&root, K::SEQ_ITEM), 5);
    }

    #[test]
    fn value_data_holds_entries() {
        let root = parse_cst("@meta{\n  _id: intro\n  tags: [a, b]\n}\n");
        let data = find(&root, K::VALUE_DATA).unwrap();
        let entries: Vec<_> = data.children().map(|n| n.text().to_string()).collect();
        assert_eq!(entries, ["_id: intro", "tags: [a, b]"]);
    }

    #[test]
    fn connect_joins_the_element() {
        let root = parse_cst("@link[x](ref:y):rule(allow: list(a))\n");
        let el = find(&root, K::BLOCK_ELEMENT).unwrap();
        assert_eq!(
            child_kinds(&el),
            [K::SIGIL, K::CONTENT, K::ARGS, K::CONNECT]
        );
        assert_eq!(
            find(&el, K::CONNECT).unwrap().text().to_string(),
            ":rule(allow: list(a))"
        );
        // A list item's own trailing group is not claimed by its element.
        let root = parse_cst("- @link(ref:x) :{ id: b }\n");
        let el = find(&root, K::INLINE_ELEMENT).unwrap();
        assert_eq!(count(&el, K::CONNECT), 0);
    }

    #[test]
    fn each_group_kind_at_most_once() {
        // The second `(...)` is prose, not more args.
        let root = parse_cst("@link[a](ref:b) (see below)\n");
        assert_eq!(child_kinds(&root), [K::PARAGRAPH]);
        let el = find(&root, K::INLINE_ELEMENT).unwrap();
        assert_eq!(count(&el, K::ARGS), 1);
    }

    #[test]
    fn fence_body() {
        let src = "@config(format:json)+++\n{ \"a\": 1 }\n+++\nafter\n";
        let root = parse_cst(src);
        assert_eq!(child_kinds(&root), [K::BLOCK_ELEMENT, K::PARAGRAPH]);
        let fence = find(&root, K::FENCE).unwrap();
        assert!(fence.text().to_string().ends_with("+++"));
    }

    #[test]
    fn paragraph_continuation_and_breaks() {
        // A trailing `\` keeps the paragraph going across a block-looking line.
        let root = parse_cst("one \\\n= two\n");
        assert_eq!(child_kinds(&root), [K::PARAGRAPH]);
        // A block start on the next line ends the paragraph without a blank.
        let root = parse_cst("text\n=[ S ]\n- a\n");
        assert_eq!(child_kinds(&root), [K::PARAGRAPH, K::SECTION]);
    }

    #[test]
    fn fence_body_is_raw_lines() {
        // A stray apostrophe and a shorter `+++` inside a longer fence.
        let src = "@memo++++\ndon't\n+++\nstill body\n++++\n=[ S ]\n";
        let root = parse_cst(src);
        assert_eq!(child_kinds(&root), [K::BLOCK_ELEMENT, K::SECTION]);
        assert!(
            find(&root, K::FENCE)
                .unwrap()
                .text()
                .to_string()
                .contains("still body")
        );
    }

    #[test]
    fn urls_and_apostrophes_do_not_break_groups() {
        // `//` after `:` is not a comment; `'` inside a word is not a quote.
        let root = parse_cst("@link[it's](url:https://example.com/a)\n=[ S ]\n");
        assert_eq!(child_kinds(&root), [K::BLOCK_ELEMENT, K::SECTION]);
    }

    #[test]
    fn typed_wrappers_walk_the_tree() {
        use tomet_ast::{AstNode, CstRoot};

        let src = "=[ A ]{ id: a }\n==[ B ]\n@x(\"k:1\": v, n: 2)\n";
        let root = CstRoot::cast(parse_cst(src)).unwrap();
        let a = root.sections().next().unwrap();
        assert_eq!(a.level(), 1);
        let b = a.sections().next().unwrap();
        assert_eq!(b.level(), 2);
        let id = a
            .heading()
            .unwrap()
            .value_data()
            .unwrap()
            .entries()
            .next()
            .unwrap();
        assert_eq!(
            (id.key().as_deref(), id.value_text().as_str()),
            (Some("id"), "a")
        );
        let el = b
            .syntax()
            .descendants()
            .find_map(tomet_ast::CstBlockElement::cast)
            .unwrap();
        let entries: Vec<_> = el
            .args()
            .unwrap()
            .entries()
            .map(|e| e.value_text())
            .collect();
        assert_eq!(entries, ["v", "2"]);
    }
}
