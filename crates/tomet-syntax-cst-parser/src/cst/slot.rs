//! Boxes: `<name>`, `<?matcher(args){data}>[content]` and `<$type(args)>`.
//!
//! One family: the `<` and the `>` are a box's, and what stands right after the `<` is its mark
//! ([`SlotMark`], the `tomet-cst` vocabulary tree readers tell a box by too), which says what may
//! be inside. A `<` that does not start a box that fits is text, so prose such as `a < b` or
//! `cost <$5>` is never a box. What follows it is read as it is anywhere else, which is mostly
//! text too, but not always: `$x(a)` is an interpolation, so `<$x(a) >` is a `<`, an
//! interpolation and ` >`; and `<$x(a>` is a parse error, because `$x(` is the head of an
//! interpolation whose arguments never close. See [`CstParser::type_seat_close`].

use super::CstParser;
use tomet_cst::CstSlotMark as SlotMark;
use tomet_cst::SyntaxKind as K;

impl<'a, 't> CstParser<'a, 't> {
    /// The mark of the box that starts at the `<` at `i`.
    fn slot_mark_at(&self, i: usize) -> SlotMark {
        SlotMark::of_token(self.kind_at(i + 1))
    }

    /// The box that starts at the `<` at `i`, if the tokens form one: its mark, and the index of
    /// its closing `>`. `None` leaves the `<` as text.
    ///
    /// A box is tight: no white space inside its head.
    ///
    /// - A name tag is `<`, a letter-first name and `>`.
    /// - A hole is `<?`, an optional name, an optional `(args)` and an optional `{data}`, and `>`.
    /// - A type seat is `<$`, a name and an optional `(args)`, and `>`: the name is mandatory and
    ///   the seat takes no `{data}`.
    ///
    /// The `>` is the first one outside those groups, which a string or a nested bracket cannot
    /// close.
    pub(super) fn slot_at(&self, i: usize) -> Option<(SlotMark, usize)> {
        if self.kind_at(i) != Some(K::LT) {
            return None;
        }
        let mark = self.slot_mark_at(i);
        let close = match mark {
            SlotMark::NameTag => self.name_tag_close(i + 1),
            SlotMark::Hole => self.hole_close(i + 2),
            SlotMark::TypeSeat => self.type_seat_close(i + 2),
        }?;
        Some((mark, close))
    }

    /// The `>` of the name tag whose name starts at `j`.
    fn name_tag_close(&self, j: usize) -> Option<usize> {
        let end = j + self.slot_name_tokens_at(j, SlotMark::NameTag);
        (end > j && self.kind_at(end) == Some(K::GT)).then_some(end)
    }

    /// The `>` of the hole whose head, after the `?`, starts at `j`.
    fn hole_close(&self, j: usize) -> Option<usize> {
        let mut j = j + self.slot_name_tokens_at(j, SlotMark::Hole);
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
        (self.kind_at(j) == Some(K::GT)).then_some(j)
    }

    /// The `>` of the type seat whose name, after the `$`, starts at `j`. Nothing but the name
    /// and one `(args)` may stand before it.
    ///
    /// A head that is no seat leaves the `<` as text, and the rest is read as it is anywhere else.
    /// Mostly that is text (`<$x`, `<$x >`). But `$x(args)` is an interpolation, so `<$x(a) >` is
    /// a `<`, the interpolation `$x(a)` and ` >`; and a `(` that does not close (`<$x(a>`) is the
    /// head of an interpolation whose arguments never end, which the tree holds as a form that
    /// has no `)`, and reading the document refuses, as it refuses `$x(a`.
    fn type_seat_close(&self, j: usize) -> Option<usize> {
        let mut end = j + self.slot_name_tokens_at(j, SlotMark::TypeSeat);
        if end == j {
            return None;
        }
        if self.kind_at(end) == Some(K::L_PAREN) {
            end = self.group_end(end)?;
        }
        (self.kind_at(end) == Some(K::GT)).then_some(end)
    }

    /// A box, at the `<` the cursor stands on ([`CstParser::slot_at`] said it is one): `<`, the
    /// mark, the name, the `(args)` of the head and, for a hole, its `{data}`, the `>`, and for
    /// a hole the `[content]` after it. A type seat takes no `{data}` and no `[content]`: what
    /// follows its `>` is not its.
    pub(super) fn parse_slot(&mut self) {
        self.builder.start_node(K::SLOT.into());
        let mark = self.slot_mark_at(self.pos);
        self.bump(); // '<'
        if mark != SlotMark::NameTag {
            self.bump(); // '?' or '$'
        }
        let n = self.slot_name_tokens_at(self.pos, mark);
        if n > 0 {
            self.builder.start_node(K::NAME.into());
            for _ in 0..n {
                self.bump();
            }
            self.builder.finish_node();
        }
        match mark {
            SlotMark::NameTag => {}
            SlotMark::Hole => {
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
            SlotMark::TypeSeat => {
                if self.current_kind() == Some(K::L_PAREN) {
                    self.parse_args_group();
                }
            }
        }
        if self.current_kind() == Some(K::GT) {
            self.bump();
        }
        if mark == SlotMark::Hole && self.current_kind() == Some(K::L_BRACKET) {
            self.parse_content_group();
        }
        self.builder.finish_node();
    }
}
