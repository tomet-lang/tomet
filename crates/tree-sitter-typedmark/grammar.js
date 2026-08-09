// TypedMark grammar for tree-sitter -- editor syntax highlighting only.
// This is a reasonable approximation of the real grammar in
// `crates/typedmark-parser/src/{document,value}.rs`, not a byte-for-byte
// match: things a context-free grammar can't cheaply express (flanking-
// delimiter whitespace rules, lazy paragraph continuation, "each of
// (input)/[area]/{value} at most once in any order") are simplified.
// See `src/lib.rs` for the full list of known simplifications.

module.exports = grammar({
	name: "typedmark",

	extras: ($) => [/[ \t]/],

	word: ($) => $.identifier,

	// `heading`'s optional `{attrs}` can follow `]` either on the same
	// line or after exactly one newline (see the real fixture examples in
	// `docs/tmt/typedmark.tm`) -- with only one token of lookahead, a lone
	// newline right after `]` is genuinely ambiguous between "start of the
	// attrs gap" and "the heading's own trailing newline, no attrs here",
	// so this needs GLR resolution rather than a lookahead-free CFG
	// rewrite (there isn't one: the ambiguity is in the language, not the
	// grammar's phrasing).
	conflicts: ($) => [[$.heading]],

	rules: {
		document: ($) => repeat(choice($._block, $._newline)),

		// No separate "block element" rule: the real parser treats a
		// paragraph-shaped line that turns out to be exactly one element as
		// `Block::Element` instead of `Block::Paragraph` (see
		// `document.rs::parse_paragraph`), but that's a post-parse
		// reclassification, not a distinct grammar production -- so here a
		// standalone `<caution>[...]` line is just a one-child `paragraph`.
		// A `block_comment` reachable from both here and `_line_item` (so it
		// also works inline) makes a comment-only line genuinely ambiguous:
		// it could reduce as this rule's own `block_comment` alternative, or
		// as a one-item `paragraph` wrapping a `block_comment` `_line_item`
		// (the same "no separate block element rule" situation as `<T>[...]`
		// below, but this one needs an explicit tie-break since the two
		// interpretations don't even produce the same tree shape). The
		// `prec(1, ...)` prefers the bare top-level reading, matching
		// `document.rs`: comments are fully discarded, never paragraph
		// content.
		_block: ($) =>
			choice(
				$.heading,
				$.thematic_break,
				$.list,
				$.paragraph,
				$.line_comment,
				prec(1, $.block_comment),
			),

		_newline: (_$) => /\r?\n/,
		_blank_gap: ($) => repeat1($._newline),

		// ---- headings ----------------------------------------------------
		heading: ($) =>
			seq(
				field("marker", $.heading_marker),
				"[",
				field("content", repeat($._bracket_item)),
				"]",
				optional(seq(optional($._blank_gap), field("attrs", $.value_group))),
				$._newline,
			),
		heading_marker: (_$) => /#+/,

		// ---- thematic break -----------------------------------------------
		thematic_break: (_$) => /-{3,}[ \t]*/,

		// ---- comments -------------------------------------------------------
		// Mirrors `document.rs::skip_line_comment`/`skip_block_comment`. `//`
		// is block-level only: inline it would risk tree-sitter's
		// longest-match lexer swallowing a bare URL's `//` (or the rest of a
		// paragraph after it) as one giant comment token, so it's reachable
		// only from `_block`, never from `_line_item`. `/* ... */` has an
		// explicit closer, so it's safe in both positions.
		line_comment: (_$) => /\/\/[^\n]*/,
		block_comment: (_$) => /\/\*(?:[^*]|\*+[^*/])*\*+\//,

		// ---- lists ----------------------------------------------------------
		list: ($) => choice($.ordered_list, $.unordered_list),
		ordered_list: ($) => prec.right(repeat1($.ordered_list_item)),
		unordered_list: ($) => prec.right(repeat1($.unordered_list_item)),
		ordered_list_item: ($) =>
			seq("-.", /[ \t]+/, repeat($._line_item), $._newline),
		unordered_list_item: ($) =>
			// A single `-` not immediately followed by another `-` or `.`
			// (those are `thematic_break`/`ordered_list_item` instead).
			seq(token(seq("-", /[ \t]/)), repeat($._line_item), $._newline),

		// ---- paragraphs -----------------------------------------------------
		// Simplification: ends only at a blank line or EOF, not at the next
		// line merely *looking* like a new block (the real parser's lazy
		// continuation rule) -- see the module doc in `src/lib.rs`.
		paragraph: ($) =>
			prec.right(repeat1(seq(repeat1($._line_item), $._newline))),

		// ---- inline content ---------------------------------------------
		_line_item: ($) =>
			choice(
				$.text,
				$.code_span,
				$.block_comment,
				$.emphasis,
				$.strong,
				$.mark,
				$.element,
				$.punctuation,
			),
		_bracket_item: ($) => choice($._line_item, $._newline),

		// `(`/`[`/`{` are excluded from `text` (and from the `_bracket_item_
		// no_*` variants below) even though they have no special meaning in
		// plain prose, because `text`'s regex is a much longer match than
		// the single-character group-opening tokens used right after an
		// element (`<T>(`/`<T>[`/`<T>{`) -- tree-sitter's lexer prefers the
		// longest match regardless of grammar-level precedence when the two
		// candidate tokens are reachable from the same parse state, so
		// without this exclusion a `[`/`(`/`{` immediately following an
		// element gets swallowed into `text` instead of starting its group.
		// `punctuation` is the low-priority fallback for when one of these
		// characters shows up in prose with nothing to open. `-` is excluded
		// for the same longest-match reason: `- ` (list marker) and `-{3,}`
		// (thematic break) are both *shorter* than a `text` run starting
		// with `-` and continuing across the rest of the line (e.g. "- one"
		// as a whole beats the 2-character marker token), so without this,
		// list items and thematic breaks with plain-text content are never
		// recognized as such at all -- they just silently become paragraphs.
		// `/` is excluded for the same reason again: `block_comment`'s
		// `/* ... */` needs to win over a `text` run that would otherwise
		// swallow it whole, so a bare `/` (not opening a comment) falls
		// back to `punctuation` like the others.
		text: (_$) => /[^\n`*_=<@()\[{\]/-]+/,
		punctuation: (_$) => /[()\[{/-]/,
		code_span: (_$) => /`[^`\n]*`/,

		emphasis: ($) =>
			choice(
				seq("*", repeat1($._bracket_item_no_star), "*"),
				seq("_", repeat1($._bracket_item_no_underscore), "_"),
			),
		strong: ($) =>
			choice(
				seq("**", repeat1($._bracket_item_no_star), "**"),
				seq("__", repeat1($._bracket_item_no_underscore), "__"),
			),
		mark: ($) => seq("==", repeat1($._bracket_item_no_equals), "=="),

		// No nested `emphasis`/`strong` here (unlike `_bracket_item_no_equals`
		// below) -- `*`/`**`/`_`/`__` all share a delimiter character, and
		// letting them nest inside each other is exactly the classic
		// Markdown emphasis/strong ambiguity that needs real flanking-rule
		// lookahead (or an external scanner) to resolve; simplified away for
		// v1, see the module doc in `src/lib.rs`.
		_bracket_item_no_star: ($) =>
			choice(
				$.code_span,
				$.block_comment,
				$.mark,
				$.element,
				$._newline,
				$.punctuation,
				alias(/[^\n`*=<@()\[{\]/]+/, $.text),
			),
		_bracket_item_no_underscore: ($) =>
			choice(
				$.code_span,
				$.block_comment,
				$.mark,
				$.element,
				$._newline,
				$.punctuation,
				alias(/[^\n`_=<@()\[{\]/]+/, $.text),
			),
		_bracket_item_no_equals: ($) =>
			choice(
				$.code_span,
				$.block_comment,
				$.emphasis,
				$.strong,
				$.element,
				$._newline,
				$.punctuation,
				alias(/[^\n`*_=<@()\[{\]/]+/, $.text),
			),

		// ---- `<T>`/`@name` elements ---------------------------------------
		element: ($) => choice($.type_element, $.at_element),
		type_element: ($) =>
			seq(
				"<",
				field("name", $._element_name),
				">",
				prec.right(2, repeat($._element_group)),
			),
		at_element: ($) =>
			seq(
				"@",
				optional(field("name", $._element_name)),
				prec.right(2, repeat($._element_group)),
			),
		_element_group: ($) => choice($.input_group, $.area_group, $.value_group),

		// Two adjacent `optional($._blank_gap)` around an optional middle
		// piece is ambiguous (nothing forces how a run of blank lines splits
		// between "leading" and "trailing" when the middle is absent), so
		// the leading gap is folded into the same `optional(...)` as the
		// content it precedes instead -- when there's no content, only the
		// single trailing gap is ever reachable.
		//
		// The opening `(`/`[`/`{` are wrapped in `prec(1, ...)`: once
		// `extras` (inline whitespace) sits between an element's sigil and
		// its first group -- `@links {` rather than `@links{` -- that same
		// character is *also* reachable as a fresh `_line_item` via
		// `punctuation` (an equal-length, 1-character match), and precedence
		// is what breaks an equal-length lexer tie in `punctuation`'s favor
		// by default (it has none set, so it otherwise wins ties against an
		// unmarked literal). Without this, any element with a space before
		// its group silently gets zero groups and the group's own text
		// becomes stray paragraph content instead.
		input_group: ($) =>
			seq(
				token(prec(1, "(")),
				optional(seq(optional($._blank_gap), $.value)),
				optional($._blank_gap),
				")",
			),
		area_group: ($) => seq(token(prec(1, "[")), repeat($._bracket_item), "]"),
		value_group: ($) =>
			seq(
				token(prec(1, "{")),
				// `$.children` isn't wrapped in a leading `_blank_gap` here like
				// `$.value` is -- its own `repeat1` already consumes a leading
				// gap before each item (including the first), so adding another
				// one here would stack two adjacent optional gaps again.
				optional(choice(seq(optional($._blank_gap), $.value), $.children)),
				optional($._blank_gap),
				"}",
			),
		// `@links { (1)[...] (id2)[...] }`-style bare children: a container
		// whose `{value}` holds a list of `(input)[area]` entries with no
		// sigil of their own (the container already supplies the type).
		children: ($) =>
			prec.right(repeat1(seq(optional($._blank_gap), $.bare_element))),
		// Simplification: unlike `input_group`/`value_group`'s own internal
		// gaps, `area_group` here must immediately follow (inline whitespace
		// only, no blank-line tolerance) -- avoids an LR conflict where a
		// lone newline can't be told apart from "no area_group at all" with
		// only one token of lookahead.
		bare_element: ($) => seq($.input_group, optional($.area_group)),

		identifier: (_$) => /[A-Za-z_][A-Za-z0-9_.-]*/,
		// `type_element`/`at_element`'s name field specifically: needs
		// higher lexical precedence than `text` (both can match e.g. "meta"
		// in `@meta(...)`, equal length since both stop at `(`, and
		// tree-sitter only consults token precedence to break length ties)
		// -- but *not* applied to `identifier` generally, since that would
		// also make it win over `map_entry`'s already-lost tie-break against
		// `scalar` in a way that breaks rather than fixes things (see
		// `_entry_value`/`scalar`'s comment). Aliased back to `identifier`
		// (referencing the rule, not a bare string) so it still shows up as
		// a normal, visible `identifier` node in the tree.
		_element_name: ($) =>
			alias(token(prec(1, /[A-Za-z_][A-Za-z0-9_.-]*/)), $.identifier),

		// ---- data value grammar (mirrors `value.rs`, simplified) --------
		// Newlines aren't in `extras` (they're structurally significant at
		// the block level), so groups that tolerate embedded blank lines
		// have to skip them explicitly via `_gap` instead.

		value: ($) => choice($.map, $.seq, $.string, $.scalar),
		// Entries are separated by a comma, one-or-more newlines, or both --
		// but never consumed by `map_entry` itself (unlike a trailing comma
		// after the *last* entry, which belongs to whatever encloses the
		// map and is genuinely ambiguous to attribute here with only one
		// token of lookahead).
		map: ($) =>
			prec.right(seq($.map_entry, repeat(seq($._entry_sep, $.map_entry)))),
		_entry_sep: ($) => choice($._blank_gap, seq(",", optional($._blank_gap))),
		// `key:` needs a scoped, higher-precedence identifier token here --
		// `scalar` (colon-inclusive, since values routinely contain one,
		// e.g. URLs) would otherwise always win the lexer's longest-match
		// comparison for anything shaped like `word:more text`, and a plain
		// `identifier` (which naturally stops at `:` already) still loses
		// outright to that longer match with no way for precedence to
		// override a *strictly* longer competing token (precedence only
		// breaks ties between equal-length matches). See `scalar`'s own
		// comment for the matching half of this fix.
		map_entry: ($) =>
			seq(
				field(
					"key",
					alias(token(prec(1, /[A-Za-z_][A-Za-z0-9_.-]*/)), $.identifier),
				),
				":",
				field("value", $._entry_value),
			),
		_entry_value: ($) => choice($.seq, $.string, $.braced_map, $._value_scalar),
		braced_map: ($) =>
			seq(
				"{",
				optional(seq(optional($._blank_gap), $.map)),
				optional($._blank_gap),
				"}",
			),
		seq: ($) =>
			seq(
				"[",
				optional(
					seq(
						optional($._blank_gap),
						$._entry_value,
						repeat(seq(",", optional($._blank_gap), $._entry_value)),
					),
				),
				optional($._blank_gap),
				"]",
			),
		string: (_$) => /"([^"\\]|\\.)*"/,
		// The literal `[` in `_value_scalar`/`scalar`'s classes below (and
		// in `text`/`punctuation`/the `_bracket_item_no_*` aliases above)
		// has to stay escaped as `\[` even though it's unambiguous either
		// way in real regex semantics -- `npx tree-sitter-cli@0.26.12
		// generate`'s own regex parser mis-reads an *unescaped* `[` inside
		// a character class as attempting to open a nested class, and
		// fails the whole rule with "unclosed character class". Unrelated
		// to the comment rules added alongside this; caught because
		// regenerating for `line_comment`/`block_comment` below required
		// running that command against the pre-existing rules too.
		// Colon-inclusive raw scalar text (numbers, bools, urls, paths, ...)
		// -- used for actual *value* positions (`_entry_value`, sequence
		// items), where colons are common and never mean "this starts a
		// nested key". Aliased to the same visible `scalar` node type as
		// the top-level one below.
		_value_scalar: ($) => alias(/[^,()\[\]{}\n\r]+/, $.scalar),
		// `value`'s own top-level bare-scalar fallback (no `key:` found) --
		// deliberately colon-*excluded* so it ties in length with
		// `map_entry`'s key token above instead of always outmatching it
		// (see that comment); the trade-off is that a bare, keyless scalar
		// at this position that itself contains a colon (e.g. a URL with no
		// `key:` prefix at all) won't parse as one token. Not observed in
		// any real `.tm` content so far -- every real example gives URLs an
		// explicit key (`url:https://...`).
		scalar: (_$) => /[^,():\[\]{}\n\r]+/,
	},
});
