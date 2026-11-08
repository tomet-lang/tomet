//! The entries and values inside `(...)`, `{...}` and `#(...)`.

use super::CstParser;
use tomet_cst::SyntaxKind;
use tomet_cst::SyntaxKind as K;

#[derive(Clone, Copy, PartialEq, Eq)]
enum MapKey {
    String,
    Name,
    Simple,
}

impl<'a, 't> CstParser<'a, 't> {
    fn map_key_at(&self, from: usize) -> Option<MapKey> {
        match self.raw_kind_at(from)? {
            K::QUOTE => {
                let start = self.start_of(from);
                let bytes = self.src.as_bytes();
                if start >= bytes.len() {
                    return None;
                }
                let quote = bytes[start];
                let mut cur = start + 1;
                let mut escaped = false;
                while cur < bytes.len() {
                    let b = bytes[cur];
                    cur += 1;
                    if escaped {
                        escaped = false;
                    } else if b == b'\\' {
                        escaped = true;
                    } else if b == quote {
                        break;
                    }
                }
                let after_str_tok = self.token_at_offset(cur);
                let colon_tok = self.skip_ws_from(after_str_tok);
                if self.kind_at(colon_tok) == Some(K::COLON) {
                    Some(MapKey::String)
                } else {
                    None
                }
            }
            K::INT_NUMBER | K::FLOAT_NUMBER => {
                let colon_tok = self.skip_ws_from(from + 1);
                if self.kind_at(colon_tok) == Some(K::COLON) {
                    Some(MapKey::Simple)
                } else {
                    None
                }
            }
            K::TRUE_KW | K::FALSE_KW | K::NULL_KW => {
                let n = self.name_tokens_at(from);
                if n > 1 {
                    let colon_tok = self.skip_ws_from(from + n);
                    if self.kind_at(colon_tok) == Some(K::COLON) {
                        Some(MapKey::Name)
                    } else {
                        None
                    }
                } else {
                    let colon_tok = self.skip_ws_from(from + 1);
                    if self.kind_at(colon_tok) == Some(K::COLON) {
                        Some(MapKey::Simple)
                    } else {
                        None
                    }
                }
            }
            _ => {
                let n = self.name_tokens_at(from);
                if n > 0 {
                    let colon_tok = self.skip_ws_from(from + n);
                    if self.kind_at(colon_tok) == Some(K::COLON) {
                        Some(MapKey::Name)
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
        }
    }

    pub(super) fn parse_entries(&mut self, close: SyntaxKind) {
        if self.too_deep() {
            let open = if close == K::R_PAREN {
                K::L_PAREN
            } else {
                K::L_BRACE
            };
            self.parse_too_deep_group(open, close);
            return;
        }
        self.depth += 1;
        self.parse_entries_inside(close);
        self.depth -= 1;
    }

    fn parse_entries_inside(&mut self, close: SyntaxKind) {
        while let Some(k) = self.current_kind() {
            if k == close {
                self.bump();
                break;
            }
            if k.is_trivia() || k == K::COMMA {
                self.bump();
            } else {
                self.parse_entry(close);
            }
        }
    }

    fn parse_entry(&mut self, close: SyntaxKind) {
        let key = self.map_key_at(self.pos);
        if let Some(k) = key {
            self.builder.start_node(K::MAP_ENTRY.into());
            match k {
                MapKey::String => self.bump_string_literal(),
                MapKey::Name => self.bump_name(),
                MapKey::Simple => self.bump(),
            }
            self.bump_while(K::WHITESPACE);
            self.bump(); // ':'

            let mut probe = self.pos;
            while probe < self.tokens.len() && self.tokens[probe].0 == K::WHITESPACE {
                probe += 1;
            }
            if probe < self.tokens.len() && self.tokens[probe].0 == K::NEWLINE {
                let mut next_line_probe = probe + 1;
                let mut has_indent = false;
                if next_line_probe < self.tokens.len()
                    && self.tokens[next_line_probe].0 == K::WHITESPACE
                {
                    has_indent = true;
                    next_line_probe += 1;
                }
                if has_indent && next_line_probe < self.tokens.len() {
                    let k = self.tokens[next_line_probe].0;
                    if k != close && k != K::NEWLINE && self.map_key_at(next_line_probe).is_none() {
                        self.bump_while(K::WHITESPACE);
                        self.bump(); // newline
                        self.bump_while(K::WHITESPACE); // indentation
                    }
                }
            }
        } else {
            self.builder.start_node(K::SEQ_ITEM.into());
        }
        self.parse_value(close);
        self.builder.finish_node();
    }

    /// One value, ending before a `,`, a newline or the enclosing `close`.
    pub(super) fn parse_value(&mut self, close: SyntaxKind) {
        // `key: | prose`: a `|` run as the value. It ends where its markers
        // stop, which is what keeps a `,` or a `}` in the prose from closing
        // the entry.
        self.bump_while(K::WHITESPACE);
        if self.current_kind() == Some(K::PIPE) {
            self.parse_pipe_content();
        }
        while let Some(k) = self.current_kind() {
            if k == close || matches!(k, K::COMMA | K::NEWLINE) {
                break;
            }
            match k {
                K::L_BRACE => {
                    self.builder.start_node(K::MAP_LITERAL.into());
                    self.bump();
                    self.parse_entries(K::R_BRACE);
                    self.builder.finish_node();
                }
                // `[ ... ]` as a value is blocks, the same `[content]` a
                // form takes, and not a list: read as a list it would close at
                // the first `]`, even one inside `` `code` ``.
                K::L_BRACKET => self.parse_content_group(),
                K::L_PAREN => self.parse_args_group(),
                // The `#(id)` of a bare entry, `(1)[text]#(a)`.
                K::HASH if self.id_follows(self.pos) => self.parse_id_group(),
                K::QUOTE => self.bump_string_literal(),
                _ => {
                    if !self.try_parse_inline_form() {
                        if self.name_at(self.pos) {
                            self.bump_name();
                        } else {
                            self.bump();
                        }
                    }
                }
            }
        }
    }
}
