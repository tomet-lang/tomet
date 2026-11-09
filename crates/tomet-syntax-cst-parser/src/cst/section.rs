//! Sections: `=[ T ]` and everything up to the next section of the same or a shallower level.

use super::CstParser;
use super::prose::InlineStop;
use tomet_cst::SyntaxKind as K;

impl<'a, 't> CstParser<'a, 't> {
    /// `=`-run at `pos` that opens a section: its level.
    pub(super) fn section_level_at(&self, i: usize) -> Option<usize> {
        let n = self.run_len(i, K::EQUAL);
        if n == 0 {
            return None;
        }
        match self.kind_at(i + n) {
            // `==#(id)`: a section whose heading is its id alone.
            Some(K::HASH) if self.id_follows(i + n) => Some(n),
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

    pub(super) fn section_level_here(&self) -> Option<usize> {
        if self.current_kind() == Some(K::EQUAL) && self.at_line_start() {
            self.section_level_at(self.pos)
        } else {
            None
        }
    }

    pub(super) fn parse_section(&mut self) {
        let level = self.section_level_here().unwrap_or(1);
        self.builder.start_node(K::SECTION.into());
        self.builder.start_node(K::SECTION_HEADING.into());
        self.bump_while(K::EQUAL);
        self.bump_while(K::WHITESPACE);
        let full_form = Self::is_opener(self.current_kind())
            || (self.current_kind() == Some(K::HASH) && self.id_follows(self.pos));
        // A body with a title in it: text after white space, not a group or a comment.
        let sugar = !full_form
            && self
                .src
                .as_bytes()
                .get(self.start_of(self.pos).wrapping_sub(1))
                .is_some_and(|b| *b == b' ' || *b == b'\t')
            && !matches!(self.current_kind(), None | Some(K::NEWLINE | K::COMMENT));
        if full_form {
            self.parse_groups(true, false);
            // Decorative trailing `=`, which may itself be followed by groups.
            loop {
                let j = self.skip_ws_from(self.pos);
                if self.kind_at(j) != Some(K::EQUAL) {
                    break;
                }
                self.bump_while(K::WHITESPACE);
                self.bump_while(K::EQUAL);
                self.parse_groups(false, false);
            }
        }
        if sugar {
            self.parse_sugar_tail();
            // A `::connect` on the line after the heading is the heading's own.
            let k = self.skip_ws_from(self.pos);
            if self.kind_at(k) == Some(K::COLON) && self.connect_follows(k) {
                self.bump_while(K::WHITESPACE);
                self.parse_groups(false, false);
                self.parse_inline(InlineStop::Line);
            }
        } else {
            self.parse_inline(InlineStop::Line);
        }
        self.builder.finish_node();
        if self.too_deep() {
            self.parse_too_deep_line();
        } else {
            self.depth += 1;
            self.parse_body(level);
            self.depth -= 1;
        }
        self.builder.finish_node();
    }
}
