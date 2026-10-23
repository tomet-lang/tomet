// motem grammar for tree-sitter -- editor syntax highlighting only.
// An approximation of `crates/motem-syntax/src/parser.rs`, not a match for it;
// see `src/lib.rs` for what it simplifies and `tests/src/motem.rs` (in the
// `tomet-tests` package) for how drift from the parser is caught.
//
// A standalone motem file (`.mtm`) is `key: value` lines. Entries end at a
// newline or a `,`; a value is a quoted string, a `{...}` map, a `name(...)`
// call (`list(...)` among them), or a bare scalar, which may hold spaces
// (`hello world`) and stops at a `,` `)` `}` or a `//` comment.
//
// A file may open with declarations about itself: `::name`, then `(args)`
// and a `{map}`, either or both, on one line (`::kind(catalog, 2)`). The
// parser wants them before the first entry; the grammar accepts one wherever
// an entry may stand.
//
// A key and the first word of a bare scalar are the same token (`word`), and
// the token after it says which it was: `:` makes it a key, `(` a callee.
// That is a one-token decision for the parser, so the grammar needs no
// external scanner. The same goes for a quoted string, which is a key when a
// `:` follows it.
//
// The words of a scalar are not typed here (`null`, `true`, a number): the
// highlight query does that from the node's text.

// A character a word may hold. `:` is left out so a key ends before its
// colon, except in `://` (`https://example.com`), as the parser does.
const WORD_CHAR = String.raw`(?:[^\s,(){}\[\]":]|:\/\/)`;
// A word does not start with `//` (a comment); a single `/` is fine (`/usr`).
const WORD_START = String.raw`(?:[^\s,(){}\[\]":\/]|\/[^\s,(){}\[\]":\/])`;

module.exports = grammar({
	name: "motem",

	// A newline ends an entry, so it is not skipped.
	extras: ($) => [/[ \t\r]+/, $.comment],

	rules: {
		source_file: ($) => repeat($._item),

		_item: ($) =>
			choice($.declaration, $.pair, $._bare_value, ",", $._newline),

		// `::name(args){data}`. The `::` is its own token: where an entry may
		// start it is read as the sigil, and after a key it is two `:` as before.
		// `prec.right`: a `{` right after a declaration is its data, not the
		// start of the next entry.
		declaration: ($) =>
			prec.right(
				seq(
					"::",
					field("name", alias($.word, $.name)),
					optional(field("arguments", $.arguments)),
					optional(field("data", $.map)),
				),
			),

		// The `(...)` of a declaration, the `(` right after the name.
		arguments: ($) =>
			seq(
				token.immediate("("),
				repeat(choice($._value, ",", $._newline)),
				")",
			),

		_newline: (_) => /\n/,

		comment: (_) => token(seq("//", /[^\n]*/)),

		// `key: value`. The value may start on the next line.
		pair: ($) =>
			seq(
				field("key", choice(alias($.word, $.key), alias($.string, $.quoted_key))),
				":",
				repeat($._newline),
				field("value", $._value),
			),

		// A value after a `:` or inside a call's parentheses: its scalar may
		// hold a `:` of its own (`note: see: this`, as the parser reads it).
		_value: ($) => choice($.string, $.map, $.call, $.scalar),

		// A value where an entry could start instead, in a file or a `{...}`.
		// There the first word and the `:` after it make a key, so a bare
		// scalar holds no `:` of its own.
		_bare_value: ($) =>
			choice($.string, $.map, $.call, alias($._bare_scalar, $.scalar)),

		map: ($) => seq("{", repeat($._item), "}"),

		// `name(arg, ...)`, the `(` right after the name. Arguments may
		// run over several lines.
		call: ($) =>
			seq(
				field("function", alias($.word, $.name)),
				token.immediate("("),
				repeat(choice($._value, ",", $._newline)),
				")",
			),

		// A bare scalar: words on a line, up to a `,` `)` `}` or a comment.
		// A `:` is part of it (`localhost:8080`, `12:30`, `see: this`).
		scalar: ($) =>
			prec.right(
				seq($.word, repeat(choice($.word, $._colon_tail, ":"))),
			),

		_bare_scalar: ($) =>
			prec.right(seq($.word, repeat(choice($.word, $._colon_tail)))),

		_colon_tail: (_) => token(new RegExp(`:${WORD_CHAR}+`)),

		word: (_) => token(new RegExp(`${WORD_START}${WORD_CHAR}*`)),

		string: ($) =>
			seq(
				'"',
				repeat(choice(token.immediate(prec(1, /[^"\\]+/)), $.escape_sequence)),
				'"',
			),

		escape_sequence: (_) => token.immediate(seq("\\", /[\s\S]/)),
	},
});
