//! What the parser does past [`MAX_DEPTH`]: keep the text flat in a `TOO_DEEP` node.

use super::CstParser;
use super::MAX_DEPTH;
use tomet_cst::SyntaxKind;
use tomet_cst::SyntaxKind as K;

impl<'a, 't> CstParser<'a, 't> {
    /// Whether one more level of nesting is past [`MAX_DEPTH`].
    pub(super) fn too_deep(&self) -> bool {
        self.depth >= MAX_DEPTH
    }

    /// The rest of the current line, flat, in a `TOO_DEEP` node.
    pub(super) fn parse_too_deep_line(&mut self) {
        self.builder.start_node(K::TOO_DEEP.into());
        while !matches!(self.current_kind(), None | Some(K::NEWLINE)) {
            self.bump();
        }
        self.builder.finish_node();
    }

    /// From just after an opening bracket to just after its closer, flat, in a `TOO_DEEP` node. Strings
    /// are one token, so a bracket in one does not count.
    pub(super) fn parse_too_deep_group(&mut self, open: SyntaxKind, close: SyntaxKind) {
        self.builder.start_node(K::TOO_DEEP.into());
        let mut depth = 1usize;
        while let Some(k) = self.current_kind() {
            if k == K::QUOTE {
                self.bump_string_literal();
                continue;
            }
            if k == open {
                depth += 1;
            } else if k == close {
                depth -= 1;
            }
            self.bump();
            if depth == 0 {
                break;
            }
        }
        self.builder.finish_node();
    }
}
