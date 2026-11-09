//! Code blocks: a fence of three backticks or more, and the two-backtick line raw.

use super::CstParser;
use tomet_cst::SyntaxKind as K;

impl<'a, 't> CstParser<'a, 't> {
    /// A fence (three backticks or more), or, at the start of a line, a line raw
    /// (exactly two). A single backtick is an inline code span.
    pub(super) fn is_code_block_start_at(&self, i: usize) -> bool {
        match self.run_len(i, K::BACKTICK) {
            0 | 1 => false,
            2 => self.line_start_at(i),
            _ => true,
        }
    }

    /// A fenced code block: it closes on a line whose leading run of backticks
    /// is at least as long as the opening one and holds nothing else but
    /// white space. A longer opening run
    /// is how a body holds a closing-looking line; a run in the middle of a
    /// line closes nothing.
    pub(super) fn parse_code_block(&mut self) {
        self.builder.start_node(K::CODE_BLOCK.into());
        let count = self.run_len(self.pos, K::BACKTICK);
        if count == 2 {
            self.parse_line_raw();
            self.builder.finish_node();
            return;
        }
        self.builder.start_node(K::FENCE.into());
        for _ in 0..count {
            self.bump();
        }
        self.builder.finish_node();
        let prev_raw = self.in_raw_block;
        self.in_raw_block = true;
        // The rest of the opening line is the info string.
        if !matches!(self.current_kind(), None | Some(K::NEWLINE)) {
            self.builder.start_node(K::INFO_STRING.into());
            while !matches!(self.current_kind(), None | Some(K::NEWLINE)) {
                self.bump();
            }
            self.builder.finish_node();
        }
        if self.current_kind() == Some(K::NEWLINE) {
            self.bump();
        }
        while !self.is_eof() && !self.at_run_end() {
            // In a `|` run the line's own marker has been read already.
            let i = if self.pipe_cols.is_empty() {
                self.pos
            } else {
                self.skip_ws_from(self.pos)
            };
            let run = self.run_len(i, K::BACKTICK);
            if run >= count {
                let j = self.skip_ws_from(i + run);
                if matches!(self.kind_at(j), None | Some(K::NEWLINE)) {
                    while self.pos < i {
                        self.bump();
                    }
                    self.builder.start_node(K::FENCE.into());
                    while self.pos < i + run {
                        self.bump();
                    }
                    self.builder.finish_node();
                    while self.pos < j {
                        self.bump();
                    }
                    break;
                }
            }
            self.builder.start_node(K::CODE_LINE.into());
            self.bump_line();
            self.builder.finish_node();
        }
        self.in_raw_block = prev_raw;
        self.builder.finish_node();
    }

    /// A line raw: the rest of the line after two backticks is the text, and the
    /// lines under it that open with two backticks of their own are more of the
    /// same block. A blank line, or any other line, ends it.
    fn parse_line_raw(&mut self) {
        let prev_raw = self.in_raw_block;
        self.in_raw_block = true;
        let mut first = true;
        loop {
            self.builder.start_node(K::CODE_LINE.into());
            if !first {
                // The indentation and a run's markers before it.
                let next = self.skip_ws_from(self.pos);
                while self.pos < next {
                    self.bump();
                }
            }
            first = false;
            self.builder.start_node(K::FENCE.into());
            self.bump();
            self.bump();
            self.builder.finish_node();
            self.bump_line();
            self.builder.finish_node();
            if self.is_eof() || self.at_run_end() {
                break;
            }
            let next = self.skip_ws_from(self.pos);
            if self.run_len(next, K::BACKTICK) != 2 {
                break;
            }
        }
        self.in_raw_block = prev_raw;
    }
}
