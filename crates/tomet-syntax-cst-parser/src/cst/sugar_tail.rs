//! The tail of a bracket-less body (`= text`, `- text`): the groups it ends in.
//!
//! Which groups at the end of the line are the owner's and which are prose. The judgement of a
//! chain of groups is [`super::tail_validity`]'s; this file asks it, once per place, and keeps
//! the answer.

use super::CstParser;
use super::prose::InlineStop;
use super::tail_validity::{is_valid_tail_group_list, tail_claims_something};
use crate::tail::TailScan;
use std::rc::Rc;
use tomet_cst::SyntaxKind as K;
use tomet_cst::SyntaxNode;

/// What the grammar says of the chain of groups a body's line ends in, from one place.
#[derive(Clone, Copy)]
pub(super) enum TailVerdict {
    /// They are the owner's, from this offset on: the first of the groups that are valid.
    From(usize),
    /// They read to the end of the line and none of them, counted from the end, is valid.
    Invalid,
    /// They do not read to the end of the line: it was prose that looked like a group.
    NotGroups,
}

impl<'a, 't> CstParser<'a, 't> {
    /// The rest of a bracket-less body (`= text`, `- text`): the text, and the
    /// groups it ends in -- `#(id)`, `{value}`, `::connect` -- which are the owner's,
    /// not the text's.
    ///
    /// Where they begin is the leftmost place from which the groups read to the end
    /// of the line ([`CstParser::trailing_groups_start`]) and which the text before it
    /// reaches without a form of its own in the way: a `{value}` that an
    /// `@form` at the end of the text would take is the form's.
    pub(super) fn parse_sugar_tail(&mut self) {
        self.bump_while(K::WHITESPACE);
        let from = self.start_of(self.pos);
        let mut line = TailScan::new(self.src, from);
        let mut resume = from;
        let mut in_body = false;
        loop {
            let Some(at) = self.trailing_groups_start(&mut line, resume) else {
                if !in_body && !matches!(self.current_kind(), None | Some(K::NEWLINE)) {
                    self.builder.start_node(K::BODY.into());
                    in_body = true;
                }
                if in_body {
                    self.parse_inline(InlineStop::LineText);
                    self.builder.finish_node();
                }
                self.parse_inline(InlineStop::Line);
                return;
            };
            if at > self.start_of(self.pos) && !in_body {
                self.builder.start_node(K::BODY.into());
                in_body = true;
            }
            if at > self.start_of(self.pos) {
                self.parse_inline(InlineStop::Offset(at));
            }
            if self.start_of(self.pos) > at {
                // A form in the text took the groups; look for one after it.
                resume = self.start_of(self.pos);
                continue;
            }
            if in_body {
                self.builder.finish_node();
            }
            let outer = std::mem::replace(&mut self.allow_connect, true);
            self.parse_groups(false, false);
            self.allow_connect = outer;
            self.parse_inline(InlineStop::Line);
            return;
        }
    }

    /// Where the groups that end the line begin, if the line at or after `resume` has any.
    ///
    /// The first candidate whose characters make a chain of groups to the end of the line
    /// ([`TailScan`]) is read with the grammar, once, and the answer settles the rest of the line:
    /// every later candidate is inside that chain, and a candidate inside a group cannot read to
    /// the end of the line on its own.
    fn trailing_groups_start(&self, line: &mut TailScan, resume: usize) -> Option<usize> {
        let mut scan = resume;
        while let Some(at) = line.next_candidate(scan) {
            match self.tail_verdict(line, at) {
                TailVerdict::From(start) => return Some(start),
                TailVerdict::Invalid => return None,
                TailVerdict::NotGroups => scan = at + 1,
            }
        }
        None
    }

    /// What the grammar says of the chain of groups that begins at `at`, kept: a trial parse of a
    /// line asks the same of the lines inside it, and the real parse asks it again; without the
    /// answers kept, every level of nesting doubled the work.
    fn tail_verdict(&self, line: &mut TailScan, at: usize) -> TailVerdict {
        let key = self.base + at;
        if let Some(&known) = self.tail_verdicts.borrow().get(&key) {
            return known;
        }
        let verdict = self.judge_tail(line, at);
        self.tail_verdicts.borrow_mut().insert(key, verdict);
        verdict
    }

    /// The groups from `at` to the end of the line, read on a subslice of the existing
    /// token stream (without re-tokenizing) and lowered: what the AST says of them is
    /// what counts, so a `{x}` (a value group holds `key: value`) or a `#(a: 1)` is
    /// text, as it always was, and so is a trailing `[content]` or `|` run: the owner has no slot
    /// for one.
    ///
    /// The tail is the longest run of the groups, counted from the end, that is valid and claims
    /// something: more groups only add ways to be invalid, so the first group of it is found by
    /// halving, however many groups the line ends in.
    fn judge_tail(&self, line: &mut TailScan, at: usize) -> TailVerdict {
        let Some(end) = line.chain(at) else {
            return TailVerdict::NotGroups;
        };
        let tok_start = self.token_at_offset(at);
        let tok_end = self.token_at_offset(end);
        let slice = &self.src[at..end];
        let mut scratch = CstParser::new(slice, &self.tokens[tok_start..tok_end]);
        scratch.base = self.base + at;
        scratch.depth = self.depth;
        scratch.tail_verdicts = Rc::clone(&self.tail_verdicts);
        scratch.builder.start_node(K::ROOT.into());
        scratch.parse_groups(false, false);
        scratch.bump_while(K::WHITESPACE);
        scratch.builder.finish_node();
        if !scratch.is_eof() {
            return TailVerdict::NotGroups;
        }
        let root = SyntaxNode::new_root(scratch.builder.finish());
        let groups: Vec<SyntaxNode> = root.children().collect();
        let read = |first: usize| -> bool {
            let slice = &groups[first..];
            !slice.is_empty() && is_valid_tail_group_list(slice)
        };
        let (mut low, mut high) = (0, groups.len());
        while low < high {
            let mid = low + (high - low) / 2;
            if read(mid) {
                high = mid;
            } else {
                low = mid + 1;
            }
        }
        match groups.get(low) {
            Some(first) if read(low) && tail_claims_something(&groups[low..]) => {
                TailVerdict::From(at + usize::from(first.text_range().start()))
            }
            _ => TailVerdict::Invalid,
        }
    }
}
