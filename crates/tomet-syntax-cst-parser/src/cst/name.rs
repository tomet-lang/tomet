//! Names: the run of tokens a `@name`, `::name` or key spells.
//!
//! A name is a token that may start one and the tokens that follow it with no white space
//! between: words, digits, `_`, `-`, and a `.` with another part right after it. The tokenizer
//! cuts these finely; the parser joins them back into one `IDENT` or one `NAME` node.

use super::CstParser;
use tomet_cst::{SyntaxKind, SyntaxKind as K};

pub(super) fn is_name_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

/// Can a token of this kind be a part of a name once it is under way?
fn is_name_part(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        K::IDENT | K::INT_NUMBER | K::UNDERSCORE | K::MINUS | K::TRUE_KW | K::FALSE_KW | K::NULL_KW
    )
}

impl<'a, 't> CstParser<'a, 't> {
    /// How many tokens, from `from`, form a name; 0 if none starts there.
    ///
    /// A keyword (`true`, `false`, `null`) starts a name only when it is the head of a longer
    /// one (`true-ish`): alone it is a value.
    pub(super) fn name_tokens_at(&self, from: usize) -> usize {
        let Some(first_kind) = self.raw_kind_at(from) else {
            return 0;
        };
        let first_text = self.text_at(from);
        let valid_start = match first_kind {
            K::IDENT => first_text.chars().next().is_some_and(is_name_start),
            K::UNDERSCORE => true,
            K::TRUE_KW | K::FALSE_KW | K::NULL_KW => {
                let next = from + 1;
                self.start_of(next) == self.start_of(from) + first_text.len()
                    && matches!(
                        self.raw_kind_at(next),
                        Some(K::UNDERSCORE | K::MINUS | K::DOT | K::IDENT | K::INT_NUMBER)
                    )
            }
            _ => false,
        };
        if valid_start {
            self.name_run_len(from)
        } else {
            0
        }
    }

    /// [`CstParser::name_tokens_at`] for a box's name, whose first character is `first`'s to
    /// accept: any letter for a name tag, a name start for a marked box.
    pub(super) fn slot_name_tokens_at(&self, i: usize, first: impl Fn(char) -> bool) -> usize {
        let Some(first_kind) = self.raw_kind_at(i) else {
            return 0;
        };
        let first_text = self.text_at(i);
        let valid_start = match first_kind {
            K::IDENT | K::TRUE_KW | K::FALSE_KW | K::NULL_KW => {
                first_text.chars().next().is_some_and(first)
            }
            K::UNDERSCORE => first('_'),
            _ => false,
        };
        if valid_start { self.name_run_len(i) } else { 0 }
    }

    /// The length, in tokens, of the name that starts at `from`, whose first token is known to
    /// be a valid start: it, and every token against the end of the last that is a part of a name.
    fn name_run_len(&self, from: usize) -> usize {
        let mut count = 1;
        let mut last_end = self.start_of(from) + self.text_at(from).len();
        while let Some(kind) = self.raw_kind_at(from + count) {
            let tok_start = self.start_of(from + count);
            if tok_start != last_end {
                break;
            }
            let tok_len = self.text_at(from + count).len();
            let continues = match kind {
                // A `.` joins two parts: it needs one against its far side.
                K::DOT => {
                    let next = from + count + 1;
                    self.start_of(next) == tok_start + tok_len
                        && self.raw_kind_at(next).is_some_and(is_name_part)
                }
                k => is_name_part(k),
            };
            if !continues {
                break;
            }
            last_end = tok_start + tok_len;
            count += 1;
        }
        count
    }

    pub(super) fn name_at(&self, i: usize) -> bool {
        self.name_tokens_at(i) > 0
    }

    pub(super) fn bump_name(&mut self) {
        let count = self.name_tokens_at(self.pos);
        if count == 0 {
            return;
        }
        let start = self.start_of(self.pos);
        let end = self.start_of(self.pos + count);
        self.builder.token(K::IDENT.into(), &self.src[start..end]);
        self.pos += count;
    }

    pub(super) fn parse_name_node(&mut self) {
        let count = self.name_tokens_at(self.pos);
        if count == 0 {
            return;
        }
        self.builder.start_node(K::NAME.into());
        for _ in 0..count {
            self.bump();
        }
        self.builder.finish_node();
    }
}
