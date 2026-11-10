//! Boxes: `<name>` and `<?name(args){data}>[content]`.

use super::CstParser;
use super::name::is_name_start;
use tomet_cst::SyntaxKind as K;

impl<'a, 't> CstParser<'a, 't> {
    /// The box that starts at the `<` at `i`, if the tokens form one: whether it
    /// has a mark, and the index of its closing `>`. `None` leaves the `<` as text.
    ///
    /// A box is tight: no white space inside its head. An unmarked one is a name
    /// tag, `<` a letter-first name and `>`. A marked one is `<?`, an optional name,
    /// an optional `(args)` and an optional `{data}`, and `>`. The `>` is the first
    /// one outside those groups, which a string or a nested bracket cannot close.
    pub(super) fn slot_at(&self, i: usize) -> Option<(bool, usize)> {
        if self.kind_at(i) != Some(K::LT) {
            return None;
        }
        let marked = self.kind_at(i + 1) == Some(K::QUESTION);
        let mut j = i + 1 + usize::from(marked);
        if !marked {
            let n = self.slot_name_tokens_at(j, char::is_alphabetic, false);
            if n == 0 {
                return None;
            }
            j += n;
            return (self.kind_at(j) == Some(K::GT)).then_some((false, j));
        }
        let n = self.slot_name_tokens_at(j, is_name_start, true);
        if n > 0 {
            j += n;
        }
        let (mut args, mut value) = (false, false);
        loop {
            match self.kind_at(j) {
                Some(K::L_PAREN) if !args => {
                    args = true;
                    j = self.group_end(j)?;
                }
                Some(K::L_BRACE) if !value => {
                    value = true;
                    j = self.group_end(j)?;
                }
                _ => break,
            }
        }
        (self.kind_at(j) == Some(K::GT)).then_some((true, j))
    }

    /// A box, at the `<` the cursor stands on ([`CstParser::slot_at`] said it is one):
    /// `<`, the mark, the name, the `(args)` and `{data}` of the head, the `>`, and
    /// for a marked one the `[content]` after it.
    pub(super) fn parse_slot(&mut self) {
        self.builder.start_node(K::SLOT.into());
        let marked = self.kind_at(self.pos + 1) == Some(K::QUESTION);
        self.bump(); // '<'
        if marked {
            self.bump(); // '?'
        }
        let first_pred: fn(char) -> bool = if marked {
            is_name_start
        } else {
            char::is_alphabetic
        };
        let n = self.slot_name_tokens_at(self.pos, first_pred, marked);
        if n > 0 {
            self.builder.start_node(K::NAME.into());
            for _ in 0..n {
                self.bump();
            }
            self.builder.finish_node();
        }
        if marked {
            let (mut args, mut value) = (false, false);
            loop {
                match self.current_kind() {
                    Some(K::L_PAREN) if !args => {
                        args = true;
                        self.parse_args_group();
                    }
                    Some(K::L_BRACE) if !value => {
                        value = true;
                        self.parse_value_group();
                    }
                    _ => break,
                }
            }
        }
        if self.current_kind() == Some(K::GT) {
            self.bump();
        }
        if marked && self.current_kind() == Some(K::L_BRACKET) {
            self.parse_content_group();
        }
        self.builder.finish_node();
    }
}
