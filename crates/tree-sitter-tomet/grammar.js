// Tomet grammar for tree-sitter -- editor syntax highlighting only.
// This is a reasonable approximation of the real grammar in
// `crates/tomet-parser/src/{document,value}.rs`, not a byte-for-byte
// match: things a context-free grammar can't cheaply express (flanking-
// delimiter whitespace rules, lazy paragraph continuation, "each of
// (args)/[content]/{value} at most once in any order") are simplified.
// See `src/lib.rs` for the full list of known simplifications.

module.exports = grammar({
	name: "tomet",

	extras: ($) => [/[ \t]/],

	word: ($) => $.identifier,

	// `scalar` (see its own comment below) is entirely produced by
	// `scanner.c` rather than an internal regex -- see that file's module
	// doc for why a plain regex (with or without an extra external
	// alternative next to it) can't resolve `@meta(yaml)`'s `yaml`/
	// `{required}`'s `required` against `map_entry`'s key token.
	//
	// `_list_marker_token`/`_list_marker_gap` (see `list_marker`'s own
	// comment below) are external for the same class of reason: deciding
	// whether the whitespace right after a list marker belongs to an
	// optional value marker or is just the item's own mandatory gap needs
	// unbounded lookahead past that whitespace, which a `token()` regex
	// can't backtrack out of once `extras`/precedence has already
	// committed to one interpretation.
	externals: ($) => [$._scalar_token, $._list_marker_token, $._list_marker_gap],

	// `heading`'s optional `{attrs}` can follow `]` either on the same
	// line or after exactly one newline (see the real fixture examples in
	// `docs/tmt/tomet.tmt`) -- with only one token of lookahead, a lone
	// newline right after `]` is genuinely ambiguous between "start of the
	// attrs gap" and "the heading's own trailing newline, no attrs here",
	// so this needs GLR resolution rather than a lookahead-free CFG
	// rewrite (there isn't one: the ambiguity is in the language, not the
	// grammar's phrasing).
	conflicts: ($) => [[$.heading], [$.map], [$.children]],

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
				$.titled_thematic_break,
				$.thematic_break,
				$.fenced_code_block,
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
				optional(
					seq(
						optional($._blank_gap),
						optional(":"),
						field("attrs", $.value_group),
					),
				),
				$._newline,
			),
		heading_marker: (_$) => /#+/,

		// ---- thematic break -----------------------------------------------
		// `_dash_run` is shared with `titled_thematic_break` below so the
		// lexer only ever has one candidate token for a run of 3+ `-` at a
		// block boundary -- each rule's own next-token lookahead (`[` vs.
		// anything else) then decides which one it belongs to, rather than
		// fighting a same-length regex tie between two separate tokens.
		// Trailing inline whitespace isn't baked into the token itself
		// (unlike an earlier revision of this rule) since `extras` already
		// skips it between any two tokens.
		_dash_run: (_$) => /-{3,}/,
		thematic_break: ($) => $._dash_run,

		// `---[ Title ]---` -- a thematic break with an inline title,
		// mirroring `heading`'s `marker [content] newline` shape but with a
		// dash run playing the marker's role on *both* sides (the two runs
		// don't have to match in length, same as `tomet-parser`'s real
		// grammar -- see `document.rs::parse_titled_thematic_break`).
		// Unlike the bare `thematic_break` above, this rule consumes its
		// own trailing newline, same as `heading` does, since it's a
		// full-line construct with content of its own rather than a bare
		// marker.
		titled_thematic_break: ($) =>
			seq(
				field("open", alias($._dash_run, $.thematic_break_marker)),
				"[",
				field("content", repeat($._bracket_item)),
				"]",
				field("close", alias($._dash_run, $.thematic_break_marker)),
				$._newline,
			),

		// ---- fenced code blocks ----------------------------------------------
		// CommonMark-style ``` fence, sugar for `<codeblock>(lang:xxx)[code]`
		// (see `document.rs::parse_fenced_code_block`). Unlike the real
		// parser (which accepts 3-or-more backticks, and requires the
		// closing fence to have at least as many as the opening one -- see
		// that function's doc comment), this grammar only recognizes a
		// fixed 3-backtick fence, the same "known-narrow" simplification
		// `thematic_break` above makes for dash runs (always exactly 3,
		// never fewer/more consumed specially). Matching a real variable-
		// length, opening-tracks-closing fence would need a stateful
		// external scanner (remembering the opening run's length across the
		// body) -- left as a known gap rather than implemented here.
		// `content`'s regex borrows `block_comment`'s "exclude the closer"
		// trick just above/below: it matches any run of text that can't
		// contain 3 consecutive backticks, so the lexer can't swallow past
		// a real closing fence.
		fenced_code_block: ($) =>
			seq(
				alias("```", $.fence_marker),
				optional(field("lang", alias(/[^\n]+/, $.text))),
				$._newline,
				optional(field("content", alias(/(?:[^`]|`[^`]|``[^`])+/, $.text))),
				alias("```", $.fence_marker),
			),

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
		// Named wrapper around the external `_list_marker_token` so it
		// shows up as its own node for `highlights.scm` to target -- e.g.
		// `- (T)`/`- (?)` outliner status markers need their own color,
		// distinct from the list item's other inline content.
		//
		// Both this and `_list_marker_gap` (used directly below, no visible
		// node needed for plain whitespace) are external rather than plain
		// regexes: `document.rs::eat_list_marker` skips inline whitespace
		// *before* checking for `(` -- real fixtures write `- (T)`,
		// with a space between the dash and the marker (see
		// `docs/cheatsheet.tmt`) -- so deciding whether that whitespace
		// belongs to an optional marker or is just the item's own
		// mandatory gap needs to look *past* the whitespace before
		// committing to either interpretation. Tree-sitter's internal
		// lexer can't do that: once it's built a combined DFA for
		// `optional($.list_marker)` next to a mandatory `/[ \t]+/`, a
		// real space character matches the mandatory-gap token immediately
		// and wins outright, before ever getting a chance to look further
		// ahead for a `(`. `scanner.c` (see its module doc) instead
		// gets one external-scanner call per position with *all* the
		// externals that are valid there, and picks whichever one actually
		// fits after looking as far ahead as it needs to -- no premature
		// commitment, no backtracking required.
		list_marker: ($) => $._list_marker_token,

		ordered_list_item: ($) =>
			seq(
				"-.",
				choice(
					seq(field("marker", $.list_marker), $._list_marker_gap),
					$._list_marker_gap,
				),
				repeat($._line_item),
				optional(field("attrs", $.value_group)),
				$._newline,
			),
		unordered_list_item: ($) =>
			seq(
				"-",
				choice(
					seq(field("marker", $.list_marker), $._list_marker_gap),
					$._list_marker_gap,
				),
				repeat($._line_item),
				optional(field("attrs", $.value_group)),
				$._newline,
			),


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
				$.interpolation,
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
		text: (_$) => /[^\n`*_=<@$()\[{\]/-]+/,
		// `-` is a bare string literal alternative here, not folded into
		// the character class like the others, so it's the *same* grammar
		// symbol as the literal `"-"` used to start `unordered_list_item`
		// (tree-sitter interns identical string literals as one shared
		// token everywhere they appear, same trick `_dash_run` already
		// uses to stay a single token between `thematic_break`/
		// `titled_thematic_break`). Without this, `-` existed as two
		// *different* token definitions that happened to match the same
		// text, and choosing between them was a lexer-level tie tree-
		// sitter resolves deterministically with no runtime choice point
		// -- see `unordered_list_item`'s own comment for why that
		// mattered. Sharing the token instead resolves it via ordinary
		// reduce logic once the parser tries to continue past `-` and
		// finds no valid `list_marker`/gap: it backs out to this
		// `punctuation` reading instead of the dead end an unshared
		// marker token forced it into, with no `conflicts`/GLR needed
		// (confirmed by `npx tree-sitter-cli generate` itself flagging a
		// `[$.list, $.paragraph]` conflicts entry as unnecessary).
		// `$` is included here (not folded into `text`'s character class)
		// for the same reason `-`/`/` are: a bare `$` not immediately
		// followed by `{` (so `interpolation`'s higher-precedence `${`
		// token doesn't win) has nothing else to reduce to and would
		// otherwise dead-end into `ERROR` once excluded from `text`.
		punctuation: (_$) => choice(/[()\[{/]/, "-", "$"),
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
				$.interpolation,
				$._newline,
				$.punctuation,
				alias(/[^\n`*=<@$()\[{\]/]+/, $.text),
			),
		_bracket_item_no_underscore: ($) =>
			choice(
				$.code_span,
				$.block_comment,
				$.mark,
				$.element,
				$.interpolation,
				$._newline,
				$.punctuation,
				alias(/[^\n`_=<@$()\[{\]/]+/, $.text),
			),
		_bracket_item_no_equals: ($) =>
			choice(
				$.code_span,
				$.block_comment,
				$.emphasis,
				$.strong,
				$.element,
				$.interpolation,
				$._newline,
				$.punctuation,
				alias(/[^\n`*_=<@$()\[{\]/]+/, $.text),
			),

		// ---- `${...}` interpolation ---------------------------------------
		// Grammar-only recognition of `tomet-parser::document.rs`'s
		// `parse_interp` -- see `docs/reviews/2026-08-17-interpolation-syntax.md`
		// for the decided v1 grammar (Path/Call/literal, no infix operators).
		// `Path` (`a.b.c`) reuses the existing `identifier` token as one
		// opaque span rather than exposing dot-separated segments -- this
		// grammar is highlighting-only and doesn't need a structured `Path`
		// AST, just to recognize the whole span and not fall into `ERROR`.
		interpolation: ($) =>
			choice(
				seq(token(prec(1, "${")), $._interp_expr, "}"),
				seq("$", $.interp_call),
			),
		_interp_expr: ($) => choice($.interp_call, $.identifier, $.number, $.string),
		interp_call: ($) =>
			seq(
				field("name", $.identifier),
				"(",
				optional(seq($._interp_expr, repeat(seq(",", $._interp_expr)))),
				")",
			),
		number: (_$) => /-?[0-9]+(\.[0-9]+)?/,

		// ---- `<T>`/`@name` elements ---------------------------------------
		element: ($) => choice($.type_element, $.at_element),
		// `prec.right(3, ...)` wraps the *whole* rule (not just the trailing
		// `repeat($._element_group)`, unlike an earlier revision) --
		// `ordered_list_item`/`unordered_list_item`'s own trailing `{attrs}`
		// (added alongside `list_marker` above) and an element's own
		// trailing value group are genuinely ambiguous with one token of
		// lookahead: both can start with a bare `{` right after the line's
		// last element. The real parser (`document.rs::parse_element`)
		// always lets an open element claim an adjacent `{` as its own
		// value before ever considering the list item's own attrs, so
		// this rule-level precedence (higher than `value_group`'s own
		// `token(prec(1, "{"))`) statically resolves the shift/reduce
		// conflict in that same direction, without needing GLR.
		type_element: ($) =>
			prec.right(
				3,
				seq(
					"<",
					field("name", $._element_name),
					">",
					repeat($._element_group),
				),
			),
		at_element: ($) =>
			prec.right(
				3,
				seq(
					"@",
					optional(field("name", $._element_name)),
					repeat($._element_group),
				),
			),
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
		args_group: ($) =>
			seq(
				token(prec(1, "(")),
				optional(seq(optional($._blank_gap), $.value)),
				optional($._blank_gap),
				")",
			),
		content_group: ($) => seq(token(prec(1, "[")), repeat($._bracket_item), "]"),
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
		// whose `{value}` holds a list of `(args)[content]` entries with no
		// sigil of their own (the container already supplies the type).
		children: ($) => repeat1(seq(optional($._blank_gap), $.bare_element)),
		// Simplification: unlike `args_group`/`value_group`'s own internal
		// gaps, `content_group` here must immediately follow (inline whitespace
		// only, no blank-line tolerance) -- avoids an LR conflict where a
		// lone newline can't be told apart from "no content_group at all" with
		// only one token of lookahead.
		bare_element: ($) => seq($.args_group, optional($.content_group)),
		_element_group: ($) =>
			seq(
				optional(":"),
				choice($.args_group, $.content_group, $.value_group),
			),

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
			alias(token(prec(1, /[A-Za-z_][A-Za-z0-9_.:\[\], -]*/)), $.identifier),

		// ---- data value grammar (mirrors `value.rs`, simplified) --------
		// Newlines aren't in `extras` (they're structurally significant at
		// the block level), so groups that tolerate embedded blank lines
		value: ($) => choice($.map, $.seq, $.string, $.scalar),
		// Entries are separated by a comma, one-or-more newlines, or both --
		// but never consumed by `map_entry` itself (unlike a trailing comma
		// after the *last* entry, which belongs to whatever encloses the
		// map and is genuinely ambiguous to attribute here with only one
		// token of lookahead).
		map: ($) => seq($.map_entry, repeat(seq($._entry_sep, $.map_entry))),
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
		//
		// The value after `:` is optional -- an embedded YAML body (see
		// `embedded_format.rs`) routinely has bare `key:` entries (YAML's
		// null shorthand, e.g. `flags:`/`rating:` in real frontmatter
		// like `@meta(format:yaml){...}`), and with a *mandatory* value
		// this rule flatly fails to match at all there -- unlike every
		// other "known gap" in this grammar's module doc, that one
		// derails into an `ERROR` that swallows the rest of the
		// enclosing block (and beyond: without a closing `value_group`,
		// later `{`/`}`/`@name` tokens keep getting reinterpreted from a
		// completely wrong parser state), not just a narrow shape
		// mismatch local to the one entry. The real native
		// (non-`format:`) map grammar (`value.rs::parse_entry_value`)
		// does *not* accept an empty value the same way -- this is a
		// deliberate over-acceptance for a highlighting-only grammar:
		// `apps/lsp` (backed by the real parser) is what surfaces actual
		// `.tmt` mistakes, so silently tolerating a shape only valid in
		// embedded YAML costs nothing here, whereas erroring on valid
		// YAML costs the entire surrounding block's highlighting.
		map_entry: ($) =>
			seq(
				field(
					"key",
					alias(token(prec(1, /[A-Za-z_][A-Za-z0-9_.-]*/)), $.identifier),
				),
				choice(
					seq(":", optional(field("value", $._entry_value))),
					field("value", $.braced_map),
				),
			),
		_entry_value: ($) => choice($.seq, $.string, $.braced_map, $._value_scalar),
		braced_map: ($) =>
			seq(
				token(prec(1, "{")),
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
		string: (_$) => choice(/"([^"\\]|\\.)*"/, /'[^'\n]*'/),
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
		//
		// The *first* character additionally excludes space/tab (the tail
		// doesn't, so multi-word values like `{hello world}` keep working)
		// -- otherwise this regex happily matches a lone leading space as a
		// complete one-character token in its own right (nothing here stops
		// it, since inline whitespace was never excluded from the class),
		// and tree-sitter's lexer takes that real, complete token match
		// over skipping the space as `extras` first. That's exactly the
		// class of bug `scanner.c`'s `SCALAR`/`LIST_MARKER_GAP` externals
		// already exist to route around (see that file's module doc) --
		// `_value_scalar` was never migrated and still had the plain-regex
		// version of the same flaw. Concretely, `key: [a, b]` (a colon,
		// then a space, then a value that starts with `[`) let this regex
		// swallow just the space as a bogus scalar `key`'s value, leaving
		// the real `[a, b]` to be reparsed from scratch as an unrelated,
		// orphaned top-level `value`/`seq` -- see this crate's module doc
		// for the `ERROR` shape that produced.
		_value_scalar: ($) =>
			alias(/[^,()\[\]{}\n\r \t][^,()\[\]{}\n\r]*/, $.scalar),
		// `value`'s own top-level bare-scalar fallback (no `key:` found) --
		// deliberately colon-*excluded* (matching the old regex this
		// replaced), so a bare, keyless scalar at this position that itself
		// contains a colon (e.g. a URL with no `key:` prefix at all) won't
		// parse as one token.
		//
		// KNOWN DRIFT from `tomet_parser` (see this crate's own module
		// doc, and `docs/reviews/2026-08-22-link-reference-uri-schemes.md`
		// section 5's "Group B"): the real parser's `value.rs` now
		// special-cases `identifier://...` to read as one bare scalar
		// (`@(https://example.com)` is valid `.tmt` today), but this
		// grammar still has no equivalent disambiguation -- `map_entry`'s
		// `identifier` key token happily matches `https` + `:` here before
		// `scalar` ever gets a chance, so this still (mis)highlights as
		// `map_entry(key: "https", value: "//example.com")`. Not yet fixed
		// here because it would need the same kind of lookahead-driven
		// exception `map_entry`'s key token can't express in plain regex
		// (a job for the external scanner, like `_scalar_token` already
		// is) -- left as a follow-up since this only affects editor syntax
		// highlighting, not `.tmt` semantics.
		//
		// Produced entirely by `scanner.c` (see its module doc): the old
		// plain-regex version of this rule had an exact length-tie against
		// `map_entry`'s key token whenever the whole bare scalar happened
		// to be identifier-shaped end to end (e.g. `@meta(yaml)`'s `yaml`,
		// `{required}`'s `required`), which `token(prec(1, ...))` on that
		// key token would always win regardless of whether a `:` actually
		// followed -- one token of lookahead precedence alone can't
		// provide. The external scanner replicates the regex's own
		// greedy-match behavior for every other case (multi-word bare
		// scalars like `{hello world}`, non-identifier-shaped ones like
		// `{42}`), so this is a drop-in replacement, not just an
		// alternative alongside the regex -- see the scanner's module doc
		// for why keeping the regex as a second alternative here instead
		// doesn't work.
		scalar: ($) => $._scalar_token,
	},
});
