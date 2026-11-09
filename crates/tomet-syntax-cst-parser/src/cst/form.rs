//! Forms: `@name` and `::name` with the groups they take, and `${ ... }`.
//!
//! How a line that opens with one is told to be a block or the start of a paragraph, where a form
//! ends, and what makes a bare `@name` text. The groups themselves are [`super::group`]'s.

use super::CstParser;
use super::prose::InlineStop;
use tomet_cst::SyntaxKind as K;

impl<'a, 't> CstParser<'a, 't> {
    /// The number of tokens in the `@name` or `::name` sigil at `i`, if one starts there.
    ///
    /// `::` is two `COLON` tokens; it is a sigil only with a name right after.
    fn sigil_len_at(&self, i: usize) -> Option<usize> {
        match self.kind_at(i) {
            Some(K::AT) if self.name_at(i + 1) => Some(1 + self.name_tokens_at(i + 1)),
            Some(K::COLON) if self.is_double_colon_sigil_at(i) => {
                Some(2 + self.name_tokens_at(i + 2))
            }
            _ => None,
        }
    }

    pub(super) fn is_double_colon_sigil_at(&self, i: usize) -> bool {
        self.kind_at(i) == Some(K::COLON)
            && self.kind_at(i + 1) == Some(K::COLON)
            && self.name_at(i + 2)
    }

    /// Index just past everything a form starting at `i` (its sigil is
    /// `@name`) owns, using only bracket matching.
    fn form_end(&self, mut i: usize) -> usize {
        i += self.sigil_len_at(i).unwrap_or(2); // `@name`, or `::name`
        let mut seen = [false; 3]; // args, content, value: each at most once
        let mut seen_id = false;
        loop {
            let mut j = self.skip_ws_from(i);
            // A comment after the form does not end it either.
            while self.kind_at(j) == Some(K::COMMENT) {
                j = self.skip_ws_from(j + self.comment_len_at(j));
            }
            // A group may stand on the line after the form's own, with no
            // blank line between: `@file(a)` and then `[ text ]`.
            if self.kind_at(j) == Some(K::NEWLINE) {
                let k = self.skip_ws_from(j + 1);
                let free = match self.kind_at(k) {
                    Some(K::L_PAREN) => !seen[0],
                    Some(K::L_BRACKET) => !seen[1],
                    Some(K::L_BRACE) => !seen[2],
                    Some(K::HASH) => !seen_id && self.id_follows(k),
                    Some(K::COLON) => self.connect_follows_on_next_line(k),
                    _ => false,
                };
                if free {
                    j = k;
                }
            }
            match self.kind_at(j) {
                // `|` is `[` without the brackets: the same slot, closed by the
                // end of its marked run.
                Some(k @ (K::L_PAREN | K::L_BRACKET | K::L_BRACE | K::PIPE)) => {
                    let slot = &mut seen[(k == K::L_BRACKET || k == K::PIPE) as usize
                        + 2 * (k == K::L_BRACE) as usize];
                    if std::mem::replace(slot, true) {
                        return i;
                    }
                    match self.group_end(j) {
                        Some(e) => i = e,
                        None => return self.tokens.len(),
                    }
                }
                Some(K::HASH) if !seen_id && self.id_follows(j) => {
                    seen_id = true;
                    // `#` `(` ... `)`: skip past the id the same way a
                    // bracketed group is skipped, by matching the paren.
                    match self.group_end(j + 1) {
                        Some(e) => i = e,
                        None => return self.tokens.len(),
                    }
                }
                Some(K::COLON) => {
                    let mut e = j + self.connect_colons(j).unwrap_or(1);
                    if self.name_at(e) {
                        e += self.name_tokens_at(e);
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
                _ => return i,
            }
        }
    }

    /// Is the `::` at `i` a declaration that owns the rest of its line? (An `@` form or a box
    /// is told from a paragraph after it is read: see [`CstParser::parse_form_line`].)
    pub(super) fn is_block_form_at(&self, i: usize) -> bool {
        if !self.is_double_colon_sigil_at(i) {
            return false;
        }
        let mut j = self.form_end(i);
        while let Some(k) = self.kind_at(j) {
            if k == K::WHITESPACE {
                j += 1;
            } else if k == K::COMMENT {
                j += self.comment_len_at(j);
            } else {
                break;
            }
        }
        self.kind_at(j).is_none_or(|k| k == K::NEWLINE)
    }

    /// After a line that holds nothing but a form, at its `NEWLINE`: does the next line fold
    /// into it, as running text goes on across lines? Not if that line is blank, ends the `[content]`
    /// or `|` run being read, or starts a block (`;`, a list marker, a section, ...).
    fn next_line_folds(&self, newline: usize) -> bool {
        if self.kind_at(newline) != Some(K::NEWLINE) {
            return false;
        }
        let k = self.skip_ws_from(newline + 1);
        let past_run = self
            .run_ends
            .last()
            .is_some_and(|&end| self.kind_at(k).is_some() && self.start_of(k) >= end);
        let closes = self.in_bracket && self.kind_at(k) == Some(K::R_BRACKET);
        !(past_run
            || closes
            || self.kind_at(k) == Some(K::COMMENT)
            || matches!(self.kind_at(k), None | Some(K::NEWLINE))
            || self.block_starts_at(k))
    }

    /// A line that opens with `@name` and nothing else (`after_name` is the index after the
    /// name): is it a block of its own rather than the first words of a paragraph?
    ///
    /// It is when it is the whole of its line and the next line neither opens a group that the
    /// name takes (see [`CstParser::parse_groups`]) nor folds into it
    /// ([`CstParser::next_line_folds`]).
    fn bare_name_stands_alone(&self, after_name: usize) -> bool {
        let end = self.past_blanks(after_name);
        match self.kind_at(end) {
            None => true,
            Some(K::NEWLINE) => self.group_opens_after(end) || !self.next_line_folds(end),
            Some(_) => false,
        }
    }

    /// After the `NEWLINE` at `newline`: does the next line open a group that the form above
    /// it takes across the line break? The ones [`CstParser::parse_groups`] takes there.
    fn group_opens_after(&self, newline: usize) -> bool {
        let k = self.skip_ws_from(newline + 1);
        match self.kind_at(k) {
            Some(K::L_PAREN | K::L_BRACKET | K::L_BRACE) => true,
            Some(K::HASH) => self.id_follows(k),
            Some(K::COLON) => self.allow_connect && self.connect_follows_on_next_line(k),
            Some(K::PIPE) => self.pipe_follows_line(newline),
            _ => false,
        }
    }

    /// After a form's name (index `i`): does a group, a `:` connect, or
    /// a `#(id)` follow, so that it is a form and not plain text?
    ///
    /// Only what [`CstParser::parse_groups`] would take counts: a `::name` with no group
    /// after it is not a connect (`@alice:bob`), and a connect where none is allowed is
    /// text. A bare `@name` is prose, and a form that takes nothing would not be one.
    pub(super) fn groups_follow(&self, i: usize) -> bool {
        let i = self.skip_ws_from(i);
        match self.kind_at(i) {
            Some(K::L_PAREN | K::L_BRACKET | K::L_BRACE | K::PIPE) => true,
            Some(K::HASH) => self.id_follows(i),
            Some(K::COLON) => self.allow_connect && self.connect_follows(i),
            _ => false,
        }
    }

    /// The head of a form that stands as a block: its sigil and groups, or a box.
    fn parse_block_head(&mut self) {
        if self.current_kind() == Some(K::LT) {
            self.parse_slot();
            return;
        }
        self.parse_sigil();
        self.parse_groups(true, true);
    }

    /// A `::name(...)` that opens a block and has more on its line: the declaration, read as it
    /// would be alone, and the rest of the line in an `ERROR` node (a declaration stands alone).
    pub(super) fn parse_declaration_with_stray_text(&mut self) {
        self.parse_block_form();
        if !matches!(self.current_kind(), None | Some(K::NEWLINE)) {
            self.builder.start_node(K::ERROR.into());
            self.parse_inline(InlineStop::Line);
            self.builder.finish_node();
        }
    }

    pub(super) fn parse_block_form(&mut self) {
        self.builder.start_node(K::BLOCK_FORM.into());
        self.parse_block_head();
        while matches!(self.current_kind(), Some(K::WHITESPACE | K::COMMENT)) {
            self.bump();
        }
        self.builder.finish_node();
    }

    /// A line that opens with an `@` form or a box. It is read first and told what it is
    /// after: a form that is the whole of its line is a block, unless the next line folds
    /// into it (then the line is running text); a form with more after it is the start of a
    /// paragraph. Reading it first is what keeps the cost proportional to the text: deciding
    /// before, by scanning to the form's end, scans everything inside it again at every level
    /// of nesting.
    pub(super) fn parse_form_line(&mut self) {
        let p = self.pos;
        let form = match self.current_kind() {
            // With a group `@name` is a form anywhere. Bare, it is a form only alone on its
            // line, and then only as a block (which lowering refuses: a nexus needs a primary
            // group): a bare `@name` that the next line folds into is running text.
            Some(K::AT) => {
                let n = self.name_tokens_at(p + 1);
                n > 0 && (self.groups_follow(p + 1 + n) || self.bare_name_stands_alone(p + 1 + n))
            }
            Some(K::LT) => matches!(self.slot_at(p), Some((true, _))),
            _ => false,
        };
        if !form {
            self.parse_paragraph();
            return;
        }
        let start = self.builder.checkpoint();
        self.parse_block_head();
        let after = self.past_blanks(self.pos);
        let whole_line = self.kind_at(after).is_none_or(|k| k == K::NEWLINE);
        if whole_line && !self.next_line_folds(after) {
            self.builder.start_node_at(start, K::BLOCK_FORM.into());
            self.bump_blanks();
            self.builder.finish_node();
            return;
        }
        // The form is the first inline of a paragraph. When it is the whole of its line, the
        // blanks after it are the form's, as they are for a block.
        self.builder.start_node_at(start, K::PARAGRAPH.into());
        self.builder.start_node_at(start, K::INLINE_FORM.into());
        if whole_line {
            self.bump_blanks();
        }
        self.builder.finish_node();
        self.parse_inline(InlineStop::Paragraph);
        self.builder.finish_node();
    }

    /// Parses a form at the cursor if the tokens form one.
    pub(super) fn try_parse_inline_form(&mut self) -> bool {
        let p = self.pos;
        if self.current_kind() == Some(K::LT) {
            let Some((marked, _)) = self.slot_at(p) else {
                return false;
            };
            // A marked box is a form of the tree; a name tag stays running text.
            if marked {
                self.builder.start_node(K::INLINE_FORM.into());
                self.parse_slot();
                self.builder.finish_node();
            } else {
                self.parse_slot();
            }
            return true;
        }
        let is_form = match self.current_kind() {
            Some(K::AT) => {
                let n = self.name_tokens_at(p + 1);
                n > 0 && self.groups_follow(p + 1 + n)
            }
            Some(K::DOLLAR) => {
                self.kind_at(p + 1) == Some(K::L_BRACE)
                    || (self.name_at(p + 1)
                        && self.kind_at(p + 1 + self.name_tokens_at(p + 1)) == Some(K::L_PAREN))
            }
            _ => false,
        };
        if !is_form {
            return false;
        }
        self.builder.start_node(K::INLINE_FORM.into());
        let dollar = self.current_kind() == Some(K::DOLLAR);
        self.parse_sigil();
        if dollar {
            match self.current_kind() {
                Some(K::L_BRACE) => self.parse_interp_expr(),
                Some(K::L_PAREN) => self.parse_args_group(),
                _ => {}
            }
        } else {
            // Like a block form's, an inline form's groups may begin on the next line.
            self.parse_groups(true, true);
        }
        self.builder.finish_node();
        true
    }

    /// The `@name` of a combine item (`-@name`) that has no group after it. Anywhere else a bare
    /// `@name` is running text; here the `-` directly against the `@` says it was meant as the
    /// item's element, so the tree keeps it as a form -- a sigil and no groups -- and lowering
    /// refuses it for want of a primary group.
    pub(super) fn parse_bare_combine_form(&mut self) {
        if self.current_kind() == Some(K::AT) && self.name_tokens_at(self.pos + 1) > 0 {
            self.builder.start_node(K::INLINE_FORM.into());
            self.parse_sigil();
            self.builder.finish_node();
        }
    }

    fn parse_sigil(&mut self) {
        self.builder.start_node(K::SIGIL.into());
        match self.current_kind() {
            // `::name`
            Some(K::COLON) if self.is_double_colon_sigil_at(self.pos) => {
                self.bump();
                self.bump();
                self.parse_name_node();
            }
            Some(_) => {
                self.bump();
                if self.name_at(self.pos) {
                    self.parse_name_node();
                }
            }
            None => {}
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
                K::QUOTE => {
                    self.bump_string_literal();
                    continue;
                }
                _ => {}
            }
            self.bump();
        }
        self.builder.finish_node();
    }
}
