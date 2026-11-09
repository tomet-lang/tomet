//! `|` runs: `[content]` spelled without brackets.
//!
//! Where a run ends, which `|` on a later line is its margin, and the `CONTENT` node it becomes.

use super::CstParser;
use tomet_cst::SyntaxKind as K;

/// What the line after a run's last read line does to it.
enum Continuation {
    /// No marker in the right column, or a blank line: the run is over.
    End,
    /// Markers and nothing else: the run goes on, with a new paragraph.
    Empty,
    /// Markers and content: the run goes on.
    Content,
}

impl<'a, 't> CstParser<'a, 't> {
    /// Does the line after the `NEWLINE` at `newline` carry the markers of a
    /// run whose columns are `cols`, one for each run it is inside of?
    /// Returns the verdict and the index of the innermost marker.
    ///
    /// A marker in the wrong column ends the run here instead of being an
    /// error: the CST never fails. Lowering refuses it
    /// (`reject_misaligned_marker`).
    fn continuation(&self, newline: usize, cols: &[usize]) -> (Continuation, Option<usize>) {
        let mut i = self.skip_raw_ws(newline + 1);
        if matches!(self.raw_kind_at(i), None | Some(K::NEWLINE)) {
            return (Continuation::End, None);
        }
        let mut innermost = None;
        for &col in cols {
            if self.raw_kind_at(i) != Some(K::PIPE) || self.column_at(self.start_of(i)) != col {
                return (Continuation::End, None);
            }
            innermost = Some(i);
            i = self.skip_raw_ws(i + 1);
        }
        if matches!(self.raw_kind_at(i), None | Some(K::NEWLINE)) {
            (Continuation::Empty, innermost)
        } else {
            (Continuation::Content, innermost)
        }
    }

    /// The run opened by the `|` at token `opener`: the index of the `NEWLINE`
    /// after its last line (the number of tokens, if it runs to the end of the
    /// text), the index of the marker on each line after the first, and whether
    /// the line after the run carries a misaligned marker.
    pub(super) fn scan_pipe_run(&self, opener: usize) -> (usize, Vec<usize>, bool) {
        let mut cols = self.pipe_cols.clone();
        cols.push(self.column_at(self.start_of(opener)));
        let mut markers = Vec::new();
        let mut i = opener;
        loop {
            while !matches!(self.raw_kind_at(i), None | Some(K::NEWLINE)) {
                i += 1;
            }
            if self.raw_kind_at(i).is_none() {
                return (self.tokens.len(), markers, false);
            }
            match self.continuation(i, &cols) {
                (Continuation::End, _) | (_, None) => {
                    let mut j = self.skip_raw_ws(i + 1);
                    let mut is_misaligned = false;
                    if !matches!(self.raw_kind_at(j), None | Some(K::NEWLINE)) {
                        for &col in &cols {
                            if self.raw_kind_at(j) != Some(K::PIPE) {
                                break;
                            }
                            if self.column_at(self.start_of(j)) != col {
                                is_misaligned = true;
                                break;
                            }
                            j = self.skip_raw_ws(j + 1);
                        }
                    }
                    return (i, markers, is_misaligned);
                }
                (_, Some(marker)) => {
                    markers.push(marker);
                    i = marker + 1;
                }
            }
        }
    }

    /// Has the run being read ended before the cursor?
    pub(super) fn at_run_end(&self) -> bool {
        self.run_ends
            .last()
            .is_some_and(|&end| self.start_of(self.pos) >= end)
    }

    /// After the `NEWLINE` at `newline`: does the next line open a `|` run
    /// of its own (once the markers of the run it stands in are passed)?
    pub(super) fn pipe_follows_line(&self, newline: usize) -> bool {
        let inside = self
            .run_ends
            .last()
            .is_none_or(|&end| self.start_of(newline) < end);
        inside && self.kind_at(self.skip_ws_from(newline + 1)) == Some(K::PIPE)
    }

    /// A `|` run, read as the `[content]` of whatever owns it: the `|` that
    /// opens it, then blocks until the run ends. The `|` that begins each
    /// later line is the run's margin, not content; the grammar does not see
    /// it (see [`CstParser::markers`]) and the tree keeps it as a `MARGIN`.
    pub(super) fn parse_pipe_content(&mut self) {
        let (end, markers, is_misaligned) = self.scan_pipe_run(self.pos);
        let column = self.column_at(self.start_of(self.pos));
        let marker_starts: Vec<usize> = markers.iter().map(|&m| self.start_of(m)).collect();
        let end_byte = self.start_of(end);

        self.builder.start_node(K::CONTENT.into());
        self.bump(); // the opening '|'
        self.markers.extend(marker_starts);
        self.pipe_cols.push(column);
        self.run_ends.push(end_byte);
        let outer_connect = std::mem::replace(&mut self.allow_connect, true);
        let outer_bracket = std::mem::replace(&mut self.in_bracket, false);
        self.bump_while(K::WHITESPACE);
        if self.too_deep() {
            self.parse_too_deep_line();
        } else {
            self.depth += 1;
            self.parse_body(0);
            self.depth -= 1;
        }
        self.in_bracket = outer_bracket;
        self.allow_connect = outer_connect;
        self.run_ends.pop();
        self.pipe_cols.pop();
        if is_misaligned {
            self.builder.start_node(K::ERROR.into());
            self.builder.finish_node();
        }
        self.builder.finish_node();
    }
}
