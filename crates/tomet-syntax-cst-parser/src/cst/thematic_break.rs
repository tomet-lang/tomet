//! Thematic breaks: `---`, and the titled one, a pair of fences around a title.

use super::CstParser;
use super::prose::InlineStop;
use tomet_cst::SyntaxKind as K;

impl<'a, 't> CstParser<'a, 't> {
    pub(super) fn is_thematic_break_at(&self, i: usize) -> bool {
        if self.run_len(i, K::MINUS) < 3 {
            return false;
        }
        let j = self.skip_ws_from(i + self.run_len(i, K::MINUS));
        matches!(
            self.kind_at(j),
            None | Some(K::NEWLINE | K::COMMENT | K::L_BRACKET)
        )
    }

    /// A thematic break: `---`, or a titled one, which is a pair of fences around a title.
    ///
    /// The title is `[content]` on the opening line (`---[ Title ]---`), or its `|` spelling, `|`
    /// lines starting on the line right after the opening `---` (a blank line in between leaves a
    /// plain break and a paragraph):
    ///
    /// ```text
    /// ---
    /// | Title
    /// | more title
    /// ---
    /// ```
    ///
    /// Both are the same node, `FENCE CONTENT FENCE`, and the closing fence is required in both:
    /// it is the `]---` of the bracketed form, which a `|` run has no `]` to carry. Without it
    /// the break gets an empty `ERROR`, and lowering reports it. A closing fence is 3 or more
    /// dashes and does not have to match the opening one. A `---` that closes one break never
    /// opens the next, so `---⏎| a⏎---⏎| b⏎---` is a titled break, a paragraph `| b`, and a plain
    /// break.
    pub(super) fn parse_thematic_break(&mut self) {
        self.builder.start_node(K::THEMATIC_BREAK.into());
        self.builder.start_node(K::FENCE.into());
        self.bump_while(K::MINUS);
        self.builder.finish_node();
        self.bump_while(K::WHITESPACE);
        if self.current_kind() == Some(K::L_BRACKET) {
            self.parse_content_group();
            let has_minus = {
                let j = self.skip_ws_from(self.pos);
                self.kind_at(j) == Some(K::MINUS)
            };
            let dash_count = if has_minus {
                let j = self.skip_ws_from(self.pos);
                self.run_len(j, K::MINUS)
            } else {
                0
            };
            if dash_count < 3 {
                if !has_minus {
                    let j = self.skip_ws_from(self.pos);
                    let non_ws = !matches!(self.kind_at(j), None | Some(K::NEWLINE));
                    if !non_ws {
                        self.builder.start_node(K::ERROR.into());
                        self.builder.finish_node();
                        self.bump_while(K::WHITESPACE);
                    } else {
                        self.bump_while(K::WHITESPACE);
                        self.builder.start_node(K::ERROR.into());
                        self.builder.finish_node();
                    }
                } else {
                    self.bump_while(K::WHITESPACE);
                    self.builder.start_node(K::FENCE.into());
                    self.bump_while(K::MINUS);
                    self.builder.finish_node();
                    self.builder.start_node(K::ERROR.into());
                    self.builder.finish_node();
                }
            } else {
                self.bump_while(K::WHITESPACE);
                self.builder.start_node(K::FENCE.into());
                self.bump_while(K::MINUS);
                self.builder.finish_node();
            }
            self.bump_while(K::WHITESPACE);
            if !matches!(self.current_kind(), None | Some(K::NEWLINE)) {
                self.builder.start_node(K::ERROR.into());
                self.parse_inline(InlineStop::Line);
                self.builder.finish_node();
            } else if self.current_kind() == Some(K::NEWLINE) {
                self.bump();
            }
        } else if self.current_kind() == Some(K::NEWLINE) && self.pipe_follows_line(self.pos) {
            self.bump(); // the newline
            self.bump_while(K::WHITESPACE); // an outer run's markers
            self.parse_pipe_content();
            self.parse_closing_fence_line();
        } else {
            self.parse_inline(InlineStop::Line);
        }
        self.builder.finish_node();
    }

    /// The closing `---` of a titled thematic break written with `|` lines, on the line right
    /// after the last of them. Like the `]---` of the bracketed form it is required: without it
    /// the break gets an empty `ERROR`, which lowering reports, and the line that is there is left
    /// alone, because it is not part of the break.
    fn parse_closing_fence_line(&mut self) {
        // The run has ended at the newline of its last line; the closing line is the next one.
        let at_newline = self.current_kind() == Some(K::NEWLINE);
        let from = if at_newline { self.pos + 1 } else { self.pos };
        let j = self.skip_ws_from(from);
        if !at_newline || self.run_len(j, K::MINUS) < 3 {
            self.builder.start_node(K::ERROR.into());
            self.builder.finish_node();
            return;
        }
        self.bump(); // the newline
        self.bump_while(K::WHITESPACE); // an outer run's markers
        self.builder.start_node(K::FENCE.into());
        self.bump_while(K::MINUS);
        self.builder.finish_node();
        self.bump_while(K::WHITESPACE);
        if !matches!(self.current_kind(), None | Some(K::NEWLINE)) {
            self.builder.start_node(K::ERROR.into());
            self.parse_inline(InlineStop::Line);
            self.builder.finish_node();
        } else if self.current_kind() == Some(K::NEWLINE) {
            self.bump();
        }
    }
}
