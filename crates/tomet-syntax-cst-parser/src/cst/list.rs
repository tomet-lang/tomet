//! Lists: items of one kind in one column, and the sublists under them.

use super::CstParser;
use super::prose::InlineStop;
use tomet_cst::SyntaxKind as K;

impl<'a, 't> CstParser<'a, 't> {
    /// Does a list item start at `i`: a `-` (`-.` for an ordered one), then
    /// white space, a form's `@`, or a group opener -- the forms
    /// `list::eat_list_marker` reads. A `-` followed by anything else is prose.
    pub(super) fn is_list_start_at(&self, i: usize) -> bool {
        if self.kind_at(i) != Some(K::MINUS) {
            return false;
        }
        let mut next = i + 1;
        if self.kind_at(next) == Some(K::DOT) {
            next += 1;
        }
        match self.kind_at(next) {
            Some(K::WHITESPACE | K::AT | K::L_PAREN | K::L_BRACKET | K::L_BRACE | K::PIPE) => true,
            Some(K::HASH) => self.id_follows(next),
            _ => false,
        }
    }

    /// A list: items of one kind (`-` or `-.`) that stand in the same column or
    /// further out, and, inside an item, the list of the deeper marker that
    /// follows it, so `- a`, `  - b`, `- c`
    /// are one list of two items and `b` is the sublist of the first.
    pub(super) fn parse_list(&mut self) {
        let ordered = self.kind_at(self.pos + 1) == Some(K::DOT);
        self.parse_list_level(ordered, 0);
    }

    /// The list marker the next line opens with, after its indentation (and a
    /// `|` run's own marker): the token index, its column, and whether it is `-.`.
    fn list_marker_ahead(&self) -> Option<(usize, usize, bool)> {
        let j = self.skip_ws_from(self.pos);
        self.is_list_start_at(j).then(|| {
            (
                j,
                self.column_at(self.start_of(j)),
                self.kind_at(j + 1) == Some(K::DOT),
            )
        })
    }

    fn parse_list_level(&mut self, ordered: bool, min_col: usize) {
        self.builder.start_node(K::LIST.into());
        while let Some((marker, col, is_ordered)) = self.list_marker_ahead() {
            if col < min_col || is_ordered != ordered {
                break;
            }
            // The indentation, and in a `|` run the marker of the line, belong
            // to the list.
            while self.pos < marker {
                self.bump();
            }
            self.builder.start_node(K::LIST_ITEM.into());
            self.parse_list_item();
            if let Some((_, deeper, deeper_ordered)) = self.list_marker_ahead()
                && deeper > col
            {
                if self.too_deep() {
                    self.parse_too_deep_line();
                } else {
                    self.depth += 1;
                    self.parse_list_level(deeper_ordered, deeper);
                    self.depth -= 1;
                }
            }
            self.builder.finish_node();
        }
        self.builder.finish_node();
    }

    fn parse_list_item(&mut self) {
        self.bump(); // '-'
        if self.current_kind() == Some(K::DOT) {
            self.bump();
        }
        self.bump_while(K::WHITESPACE);
        // The item's own groups. Reading them here keeps a bracketed item,
        // which may span lines, inside one node. `allow_connect` is left
        // as whatever the caller already had (true unless we are nested
        // inside something that disabled it), so a full-form item's own
        // trailing `::name(...)`/`::(...)`/`::{...}` -- immediately after
        // its closing bracket, nothing else there to claim it -- attaches
        // to the item itself, the same as any other sigil's groups do.
        let sugar = self.list_item_is_sugar();
        self.parse_groups(false, false);
        if sugar {
            // `- text` and `- (marker) text`: the text, and the groups it ends in.
            let outer = std::mem::replace(&mut self.allow_connect, false);
            self.parse_sugar_tail();
            self.allow_connect = outer;
        } else {
            // The rest of the line -- plain trailing text for a full-form
            // item -- must not let an inner form (`@link` in
            // `- @link(ref:x) ::{ id: b }`) steal a colon connect meant for
            // the item: `connect_joins_the_form` depends on this staying
            // `false` here.
            let outer = self.allow_connect;
            // The combine notation `-@name(...)` is the one exception: that
            // form is the item, and takes its own connects.
            let combine = self.current_kind() == Some(K::AT)
                && self.pos > 0
                && self.kind_at(self.pos - 1) != Some(K::WHITESPACE);
            if combine {
                self.allow_connect = true;
                if !self.try_parse_inline_form() {
                    self.parse_bare_combine_form();
                }
            }
            self.allow_connect = false;
            if !matches!(self.current_kind(), None | Some(K::NEWLINE)) {
                self.builder.start_node(K::BODY.into());
                self.parse_inline(InlineStop::LineText);
                self.builder.finish_node();
            }
            self.parse_inline(InlineStop::Line);
            self.allow_connect = outer;
        }
        // A `::connect` on the line after the item is the item's own, with the
        // groups that follow it on that line.
        let k = self.skip_ws_from(self.pos);
        if self.allow_connect && self.kind_at(k) == Some(K::COLON) && self.connect_follows(k) {
            self.bump_while(K::WHITESPACE);
            self.parse_groups(false, false);
            self.parse_inline(InlineStop::Line);
        }
    }

    /// Whether the item whose marker was just read is the bracket-less sugar
    /// (`- text`, `- (marker) text`): not the combine notation `-@name`, and not
    /// the full form, where a group opens on the marker (`- [ content ]`,
    /// `- (marker)[ content ]`, `- {value}`, `- #(id)`).
    fn list_item_is_sugar(&self) -> bool {
        let opens_full = |i: usize| match self.kind_at(i) {
            Some(K::L_BRACKET | K::L_BRACE | K::PIPE) => true,
            Some(K::HASH) => self.id_follows(i),
            _ => false,
        };
        let at = self.pos;
        if self.kind_at(at) == Some(K::AT) && at > 0 && self.kind_at(at - 1) != Some(K::WHITESPACE)
        {
            return false;
        }
        if opens_full(at) {
            return false;
        }
        if self.kind_at(at) == Some(K::L_PAREN) {
            return match self.group_end(at) {
                Some(end) => !opens_full(self.skip_ws_from(end).min(self.tokens.len())),
                None => true,
            };
        }
        true
    }
}
