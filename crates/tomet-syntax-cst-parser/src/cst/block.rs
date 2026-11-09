//! Blocks: the loop over a body, and the choice of which block starts at the cursor.

use super::CstParser;
use tomet_cst::SyntaxKind as K;

impl<'a, 't> CstParser<'a, 't> {
    pub(super) fn parse_root(&mut self) {
        self.builder.start_node(K::ROOT.into());
        self.parse_body(0);
        self.builder.finish_node();
    }

    /// Blocks until EOF, until a section of level `<= section_level`
    /// starts, until the `|` run being read ends, or -- when
    /// [`CstParser::in_bracket`] -- until the enclosing `[content]` group's
    /// own closing `]`.
    pub(super) fn parse_body(&mut self, section_level: usize) {
        while !self.is_eof() {
            if self.at_run_end() {
                break;
            }
            if self.current_kind().is_some_and(|k| k.is_trivia()) {
                self.bump();
                continue;
            }
            if self.in_bracket && self.current_kind() == Some(K::R_BRACKET) {
                break;
            }
            // A `;` at the start of a line only breaks the block before it: it
            // stays in the tree as a token of the body, like trivia.
            if self.current_kind() == Some(K::SEMI) && self.at_line_start() {
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
            K::BACKTICK if self.is_code_block_start_at(self.pos) => self.parse_code_block(),
            K::AT | K::LT => self.parse_form_line(),
            K::COLON if self.is_block_form_at(self.pos) => self.parse_block_form(),
            // A declaration that does not stand alone: still read as one, with the text after it
            // marked as the mistake.
            K::COLON
                if self.pipe_cols.is_empty()
                    && self.is_double_colon_sigil_at(self.pos)
                    && self.groups_follow(self.pos + 2 + self.name_tokens_at(self.pos + 2)) =>
            {
                self.parse_declaration_with_stray_text()
            }
            _ => self.parse_paragraph(),
        }
    }

    /// Does a block other than a paragraph start at `i` (the first token of a line)?
    pub(super) fn block_starts_at(&self, i: usize) -> bool {
        match self.kind_at(i) {
            Some(K::EQUAL) => self.section_level_at(i).is_some(),
            Some(K::MINUS) => self.is_thematic_break_at(i) || self.is_list_start_at(i),
            Some(K::BACKTICK) => self.is_code_block_start_at(i),
            // A line-start `;` breaks the block; it is no block of its own.
            Some(K::SEMI) => true,
            // So does a comment on a line of its own.
            Some(K::COMMENT) => true,
            // An `@` form is not here: a line that starts with one is
            // running text and folds into the paragraph. A `::` declaration
            // is only ever a block.
            Some(K::COLON) => self.is_block_form_at(i),
            _ => false,
        }
    }
}
