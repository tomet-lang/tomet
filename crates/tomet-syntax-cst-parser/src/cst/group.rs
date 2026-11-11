//! Groups: `(args)`, `{value}`, `[content]`, `#(id)` and `::connect`.
//!
//! Which of them follow a nexus, in what order, and bracket matching to find where one ends. What
//! is inside `()` and `{}` is [`super::value`]'s.

use super::CstParser;
use crate::inline::escaped_run_len;
use tomet_cst::SyntaxKind as K;

impl<'a, 't> CstParser<'a, 't> {
    /// Index just past the group opened at `i`, or `None` if it never closes.
    pub(super) fn group_end(&self, i: usize) -> Option<usize> {
        let (open, close) = match self.kind_at(i)? {
            K::L_PAREN => (K::L_PAREN, K::R_PAREN),
            K::L_BRACKET => (K::L_BRACKET, K::R_BRACKET),
            K::L_BRACE => (K::L_BRACE, K::R_BRACE),
            K::PIPE => return Some(self.scan_pipe_run(i).0),
            _ => return None,
        };
        let mut depth = 0usize;
        let mut j = i;
        while let Some(k) = self.kind_at(j) {
            if k == K::QUOTE {
                let start = self.start_of(j);
                let bytes = self.src.as_bytes();
                if start < bytes.len() {
                    let quote = bytes[start];
                    let mut cur = start + 1;
                    let mut escaped = false;
                    while cur < bytes.len() {
                        let b = bytes[cur];
                        if b == b'\n' {
                            break;
                        }
                        cur += 1;
                        if escaped {
                            escaped = false;
                        } else if b == b'\\' {
                            escaped = true;
                        } else if b == quote {
                            break;
                        }
                    }
                    j = self.token_at_offset(cur);
                    continue;
                }
            }
            if j > i
                && let Some(len) = escaped_run_len(self.src.as_bytes(), self.start_of(j))
            {
                let end = self.start_of(j) + len;
                while self.start_of(j) < end {
                    j += 1;
                }
                continue;
            }
            if k == open {
                depth += 1;
            } else if k == close {
                depth -= 1;
                if depth == 0 {
                    return Some(j + 1);
                }
            }
            j += 1;
        }
        None
    }

    /// A form's `(args)`, `{value}` and `[content]` (at most one of each,
    /// in any order), then its `#(id)` slot, then its `::name(...)` connects.
    ///
    /// No ordering is enforced between `#(id)` and the other three groups,
    /// mirroring the AST-path parser's `parse_groups` -- only "before any
    /// connect" matters, and that needs no explicit check here either: a
    /// connect's own id check (in [`parse_connect`]) claims an adjacent
    /// `#(...)` for itself first, so a `#(id)` written after a connect
    /// never reaches this loop's own `id` slot at all.
    ///
    /// `|` is `[` without the brackets: the same slot, closed by the end of its
    /// run. A block form (`cross_newline`) may open it on the line after
    /// its own, `@quote(A)` and then `| text`; any other construct opens it
    /// on its own line or not at all.
    ///
    /// With `strict`, a second group of a kind already taken, written against the last one, is
    /// an `ERROR` node (see the arm below).
    pub(super) fn parse_groups(&mut self, cross_newline: bool, strict: bool) {
        let (mut args, mut value, mut content, mut id) = (false, false, false, false);
        loop {
            let mut j = self.skip_ws_from(self.pos);
            while cross_newline && self.kind_at(j) == Some(K::COMMENT) {
                j = self.skip_ws_from(j + self.comment_len_at(j));
            }
            if cross_newline && self.kind_at(j) == Some(K::NEWLINE) {
                let k = self.skip_ws_from(j + 1);
                let free = match self.kind_at(k) {
                    Some(K::L_PAREN) => !args,
                    Some(K::L_BRACKET) => !content,
                    Some(K::L_BRACE) => !value && !self.gate_at(k),
                    Some(K::HASH) => !id && self.id_follows(k),
                    Some(K::COLON) => self.allow_connect && self.connect_follows_on_next_line(k),
                    _ => false,
                };
                if free {
                    while self.pos < j {
                        self.bump(); // white space and comments
                    }
                    self.bump(); // the newline
                    self.bump_while(K::WHITESPACE); // an outer run's markers
                    continue;
                }
            }
            match self.kind_at(j) {
                Some(K::PIPE) if !content => {
                    content = true;
                    self.bump_while(K::WHITESPACE);
                    self.parse_pipe_content();
                }
                Some(K::NEWLINE) if cross_newline && !content && self.pipe_follows_line(j) => {
                    content = true;
                    while self.pos < j {
                        self.bump(); // white space and comments
                    }
                    self.bump(); // the newline
                    self.bump_while(K::WHITESPACE); // an outer run's markers
                    self.parse_pipe_content();
                }
                Some(K::L_PAREN) if !args => {
                    args = true;
                    self.bump_while(K::WHITESPACE);
                    self.parse_args_group();
                }
                // `{$ expr }` is an expression, not a value group.
                Some(K::L_BRACE) if !value && !self.gate_at(j) => {
                    value = true;
                    self.bump_while(K::WHITESPACE);
                    self.parse_value_group();
                }
                Some(K::L_BRACKET) if !content => {
                    content = true;
                    self.bump_while(K::WHITESPACE);
                    self.parse_content_group();
                }
                Some(K::HASH) if !id && self.id_follows(j) => {
                    id = true;
                    self.bump_while(K::WHITESPACE);
                    self.parse_id_group();
                }
                Some(K::COLON) if self.allow_connect && self.connect_follows(j) => {
                    self.bump_while(K::WHITESPACE);
                    self.parse_connect(strict);
                }
                // A group of a kind the owner has already, written against the last one. Apart
                // from it (a space, a line) it is prose; against it, it is an error, kept in the
                // tree as one and read as the group it looks like. Only a form and a
                // declaration take one of each.
                Some(k @ (K::L_PAREN | K::L_BRACKET | K::L_BRACE | K::PIPE | K::HASH))
                    if strict
                        && j == self.pos
                        && !self.gate_at(j)
                        && match k {
                            K::L_PAREN => args,
                            K::L_BRACE => value,
                            K::L_BRACKET | K::PIPE => content,
                            _ => id && self.id_follows(j),
                        } =>
                {
                    self.builder.start_node(K::ERROR.into());
                    match k {
                        K::L_PAREN => self.parse_args_group(),
                        K::L_BRACE => self.parse_value_group(),
                        K::L_BRACKET => self.parse_content_group(),
                        K::PIPE => self.parse_pipe_content(),
                        _ => self.parse_id_group(),
                    }
                    self.builder.finish_node();
                }
                _ => break,
            }
        }
    }

    /// Is `i` a `#` immediately followed by `(`, so that it opens the one
    /// `#(id)` slot rather than being free text?
    pub(super) fn id_follows(&self, i: usize) -> bool {
        self.kind_at(i + 1) == Some(K::L_PAREN)
    }

    pub(super) fn parse_id_group(&mut self) {
        self.builder.start_node(K::ID_GROUP.into());
        self.bump(); // '#'
        self.bump(); // '('
        self.parse_value(K::R_PAREN);
        if self.current_kind() == Some(K::R_PAREN) {
            self.bump();
        }
        self.builder.finish_node();
    }

    /// How many `:` open the connect that starts at `colon` -- 2 for `::name(...)` and `::{...}`,
    /// 1 for the retired `:name(...)` -- if a connect starts there: a name, if there is one, and
    /// then a group.
    ///
    /// The single colon is still read as a connect, so that lowering can refuse it and say what
    /// to write; it is not text.
    pub(super) fn connect_colons(&self, colon: usize) -> Option<usize> {
        if self.kind_at(colon) != Some(K::COLON) {
            return None;
        }
        let colons = if self.kind_at(colon + 1) == Some(K::COLON) {
            2
        } else {
            1
        };
        let mut e = colon + colons;
        if self.name_at(e) {
            e += self.name_tokens_at(e);
        }
        Self::is_opener(self.kind_at(e)).then_some(colons)
    }

    pub(super) fn connect_follows(&self, colon: usize) -> bool {
        self.connect_colons(colon).is_some()
    }

    /// Whether the connect at `colon` may stand on the line after its owner: the retired
    /// `::name(...)`, and a nameless `::{...}` / `::(...)`. A `::name` at the start of a line is
    /// a declaration, and says so by its name.
    pub(super) fn connect_follows_on_next_line(&self, colon: usize) -> bool {
        match self.connect_colons(colon) {
            Some(1) => true,
            Some(_) => !self.name_at(colon + 2),
            None => false,
        }
    }

    fn parse_connect(&mut self, strict: bool) {
        self.builder.start_node(K::CONNECT.into());
        self.bump(); // ':'
        if self.current_kind() == Some(K::COLON) {
            self.bump(); // the second ':' of '::'
        }
        if self.name_at(self.pos) {
            self.parse_name_node();
        }
        // The groups of a connect are a form's: `(args)`, `{value}` and
        // `[content]` in any order, each once -- but not a sibling `::connect`,
        // which stays for the loop this was called from.
        let outer = std::mem::replace(&mut self.allow_connect, false);
        self.parse_groups(false, strict);
        self.allow_connect = outer;
        self.builder.finish_node();
    }

    pub(super) fn parse_args_group(&mut self) {
        self.builder.start_node(K::ARGS.into());
        self.bump(); // '('
        self.parse_entries(K::R_PAREN);
        self.builder.finish_node();
    }

    pub(super) fn parse_value_group(&mut self) {
        self.builder.start_node(K::VALUE_DATA.into());
        self.bump(); // '{'
        self.parse_entries(K::R_BRACE);
        self.builder.finish_node();
    }

    pub(super) fn parse_content_group(&mut self) {
        self.builder.start_node(K::CONTENT.into());
        self.bump(); // '['
        if self.too_deep() {
            self.parse_too_deep_group(K::L_BRACKET, K::R_BRACKET);
            self.builder.finish_node();
            return;
        }
        self.depth += 1;
        let outer_connect = std::mem::replace(&mut self.allow_connect, true);
        let outer_bracket = std::mem::replace(&mut self.in_bracket, true);
        self.parse_body(0);
        self.in_bracket = outer_bracket;
        self.allow_connect = outer_connect;
        self.depth -= 1;
        if self.current_kind() == Some(K::R_BRACKET) {
            self.bump();
        } else {
            self.builder.start_node(K::ERROR.into());
            self.builder.finish_node();
        }
        self.builder.finish_node();
    }
}
