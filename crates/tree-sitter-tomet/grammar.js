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
  //
  // `type_seat_open` (see `type_seat`'s comment below) is external because a
  // seat is only a seat if it closes: the scanner reads ahead past `<$name`
  // and emits the token only when the `>` (or the `(args)` and then the `>`)
  // is there, which no regex can say and no token can be taken back from.
  externals: ($) => [
    $._scalar_token,
    $._list_marker_token,
    $._list_marker_gap,
    $.type_seat_open,
  ],

  // `heading`'s optional `{attrs}` can follow `]` either on the same
  // line or after exactly one newline (see the real fixture examples in
  // `fixtures/readme.tmt`) -- with only one token of lookahead, a lone
  // newline right after `]` is genuinely ambiguous between "start of the
  // attrs gap" and "the heading's own trailing newline, no attrs here",
  // so this needs GLR resolution rather than a lookahead-free CFG
  // rewrite (there isn't one: the ambiguity is in the language, not the
  // grammar's phrasing).
  conflicts: ($) => [
    [$.section],
    [$.map],
    [$.children],
    // `(a)` is a group or the start of `(a) => ...`; only the `=>` after the
    // `)` decides, which a lookahead of one token cannot see.
    [$._interp_operand, $.interp_lambda],
    [$.thematic_break, $.titled_thematic_break],
  ],

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
        $.section,
        $.titled_thematic_break,
        $.thematic_break,
        $.fenced_code_block,
        $.line_raw,
        $.list,
        $.document_element,
        $.semi_break,
        $.paragraph,
        $.line_comment,
        prec(1, $.block_comment),
      ),

    // Two backticks at the start of a line make the whole line raw; the lines
    // under it that start the same way are more of the same block. A third
    // backtick is a fence, so the token excludes it. In the middle of a line
    // a single backtick opens an inline `code_span`. Like `;`, only at the top level.
    line_raw: ($) => prec.right(repeat1(seq($._line_raw_line, $._newline))),
    _line_raw_line: (_$) => token(prec(2, /``([^`\n][^\n]*)?/)),

    // A `;` at the start of a line breaks the block, as a blank line does.
    // It is only a break where a block can begin, so a paragraph's lines may
    // not start with a `;` punctuation
    // (`_first_line_item`) and the `;` in `text` is carved out below. In the
    // middle of a line it is plain `punctuation`. Known simplification: only
    // at the top level, not inside `[content]` or a `|` run.
    semi_break: (_$) => ";",

    _newline: (_$) => /\r?\n/,
    _blank_gap: ($) => repeat1($._newline),

    // ---- sections ----------------------------------------------------
    // `=` is a sigil for sections: it takes `(args)`, `[content]` and
    // `{value}`. The bracket-less sugar (`= title`) needs whitespace after
    // the run. Heading-less sections (`=`, `==` with optional args/attrs)
    // are also supported.
    section: ($) =>
      seq(
        field("marker", $.section_marker),
        choice(
          seq(
            optional(field("args", $.args_group)),
            "[",
            field("content", repeat($._bracket_item)),
            "]",
            optional(field("close_marker", $.section_marker)),
          ),
          seq(
            optional(field("args", $.args_group)),
            field("content", $.marked_content),
          ),
          field("content", repeat1($._line_item)),
          optional(field("args", $.args_group)),
        ),
        optional(field("id", $.id_group)),
        optional(
          prec(
            1,
            seq(
              optional($._blank_gap),
              choice(
                seq(
                  optional(token(prec(1, "::"))),
                  field("attrs", $.value_group),
                ),
                repeat1($.connect),
              ),
            ),
          ),
        ),
        $._newline,
      ),
    section_marker: (_$) => token(prec(1, /=+/)),

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
    //
    // The title may also be `|` lines starting on the line right after the
    // opening run (`marked_content`, the same `[content]`-without-brackets
    // group an element takes). The closing run is required there too: it
    // is the `]---` the `|` form has no `]` to carry. `thematic_break` is
    // the same dash run followed by a newline, so the two share a prefix;
    // the conflict is resolved by the closing run, which a plain break
    // never has (and `prec.dynamic` prefers the titled break when both fit).
    titled_thematic_break: ($) =>
      prec.dynamic(
        1,
        seq(
          field("open", alias($._dash_run, $.thematic_break_marker)),
          choice(
            seq("[", field("content", repeat($._bracket_item)), "]"),
            seq($._newline, field("content", $.marked_content)),
          ),
          field("close", alias($._dash_run, $.thematic_break_marker)),
          $._newline,
        ),
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
        alias(token(prec(3, "```")), $.fence_marker),
        optional(field("lang", alias(/[^\n]+/, $.text))),
        $._newline,
        optional(field("content", alias(/(?:[^`]|`[^`]|``[^`])+/, $.text))),
        alias(token(prec(3, "```")), $.fence_marker),
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
    // `fixtures/cheatsheet.tmt`) -- so deciding whether that whitespace
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

    // A list item is an element: it takes the same `(args)`,
    // `[content]` and `{value}` groups `@name` does. The bracketed
    // form needs no gap after the marker (`- ()[ x ]`) and may span
    // lines, since `content_group` admits `_newline` through
    // `_bracket_item`. The bracket-less sugar keeps its mandatory gap
    // and stays on one line.
    ordered_list_item: ($) =>
      seq(
        "-.",
        choice(
          seq(
            field("marker", $.list_marker),
            choice($._list_marker_gap, optional(/[ \t]*/)),
          ),
          $._list_marker_gap,
        ),
        choice(
          seq(
            field("content", $.content_group),
            optional(field("attrs", $.value_group)),
            repeat($._line_item),
          ),
          field("content", $.marked_content),
          seq(repeat($._line_item), optional(field("attrs", $.value_group))),
        ),
        $._newline,
      ),
    // A list item is an element: it takes the same `(args)`,
    // `[content]` and `{value}` groups `@name` does. The bracketed
    // form needs no gap after the marker (`- ()[ x ]`) and may span
    // lines, since `content_group` admits `_newline` through
    // `_bracket_item`. The bracket-less sugar keeps its mandatory gap
    // and stays on one line.
    unordered_list_item: ($) =>
      seq(
        "-",
        choice(
          seq(
            field("marker", $.list_marker),
            choice($._list_marker_gap, optional(/[ \t]*/)),
          ),
          $._list_marker_gap,
        ),
        choice(
          seq(
            field("content", $.content_group),
            optional(field("attrs", $.value_group)),
            repeat($._line_item),
          ),
          field("content", $.marked_content),
          seq(repeat($._line_item), optional(field("attrs", $.value_group))),
        ),
        $._newline,
      ),

    // ---- paragraphs -----------------------------------------------------
    // Simplification: ends only at a blank line or EOF, not at the next
    // line merely *looking* like a new block (the real parser's lazy
    // continuation rule) -- see the module doc in `src/lib.rs`.
    paragraph: ($) =>
      prec.right(
        repeat1(seq($._first_line_item, repeat($._line_item), $._newline)),
      ),

    // ---- inline content ---------------------------------------------
    _line_item: ($) =>
      choice(
        $.escape,
        $.text,
        $.code_span,
        $.block_comment,
        $.emphasis,
        $.strong,
        $.strikeout,
        $.element,
        $.label,
        $.hole,
        $.type_seat,
        $.interpolation,
        $.punctuation,
      ),
    // What may open a line of a paragraph: anything but a `;`, which breaks the block.
    _first_line_item: ($) =>
      choice(
        $.escape,
        $.text,
        $.code_span,
        $.block_comment,
        $.emphasis,
        $.strong,
        $.strikeout,
        $.element,
        $.label,
        $.hole,
        $.type_seat,
        $.interpolation,
        alias($._punctuation_not_semi, $.punctuation),
      ),
    _bracket_item: ($) => choice($._line_item, $._newline),

    escape: (_$) =>
      token(
        prec(
          2,
          /\\(\\|!+|"+|#+|\$+|%+|\&+|'+|\(+|\)+|\*+|\++|,+|-+|\.+|\/+|:+|;+|<+|=+|>+|\?+|@+|\[+|\]+|\^+|_+|\`+|\{+|\|+|\}+|~+)/,
        ),
      ),

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
    // `=` is excluded so `section_marker` can win. `#` is excluded for
    // the same longest-match reason as the bracket characters above:
    // `id_group`'s `#(` needs to win over a `text` run that would
    // otherwise swallow it (e.g. "#(myid)" read as one word). A bare
    // `#` with nothing to open still falls back to `punctuation`, the
    // same as the other excluded characters -- it has no syntax at
    // all on its own, only `#(` does.
    // `:` is excluded for the same longest-match reason as the rest of
    // this list: `::name(...)` (`connect`) and the bare `::(`/`::{` merge
    // both start with a two-character `"::"` token, but right after
    // an element, `text`'s regex is free to keep matching straight
    // through a `:` into whatever ordinary words follow it (`::as`,
    // `::rule`, ...) -- unlike the *bracket* characters above, `:` has
    // no excluded character stopping that run early, so without this,
    // the longer `text` match always won and no `::name(...)` (or even
    // a bare `::{`/`::(` right after certain groups) was ever reachable.
    // A bare `:` with nothing to attach to falls back to `punctuation`,
    // same as the other excluded characters.
    // `<` is excluded for the same longest-match reason: a box (`label`, `hole`) opens with it
    // and must win over a `text` run that would swallow `<PATH>` whole.
    //
    // `$` and `<?` are excluded from that run, so they open an interpolation or a hole -- but a
    // name is ASCII, and `$a-é(1)` or `<?aé>` is text in `tomet-parser`. Tree-sitter cannot take
    // back an interpolation or a hole it has begun, so these two prefixes, each through the first
    // letter outside ASCII, are a `text` token of their own: longer than the `$` or the `<?name`
    // it would otherwise open with, so it wins.
    text: (_$) =>
      token(
        choice(
          /[^\n\\`*_=~<@$#:()\[{\]/|;-]+/,
          /\$[A-Za-z_][A-Za-z0-9_.\-]*[^\x00-\x7F]/,
          prec(3, /<\?[A-Za-z0-9_.\-]*[^\x00-\x7F]/),
        ),
      ),

    punctuation: ($) => choice($._punctuation_not_semi, ";"),
    _punctuation_not_semi: (_$) =>
      choice(/[()\[{/<>|~]/, "-", "$", "#", ":", "=", "\\", /`+/),
    code_span: (_$) => /`[^`\n]+`/,

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
    // GFM's spelling, and Tomet's. Paired delimiter with excluded character inside.
    strikeout: ($) => seq("~~", repeat1($._bracket_item_no_tilde), "~~"),

    _bracket_item_no_star: ($) =>
      choice(
        $.escape,
        $.code_span,
        $.block_comment,
        $.strikeout,
        $.element,
        $.label,
        $.hole,
        $.type_seat,
        $.interpolation,
        $._newline,
        $.punctuation,
        alias(/[^\n\\`*=~<@$()\[{\]/|]+/, $.text),
      ),
    _bracket_item_no_underscore: ($) =>
      choice(
        $.escape,
        $.code_span,
        $.block_comment,
        $.strikeout,
        $.element,
        $.label,
        $.hole,
        $.type_seat,
        $.interpolation,
        $._newline,
        $.punctuation,
        alias(/[^\n\\`_=~<@$()\[{\]/|]+/, $.text),
      ),
    _bracket_item_no_tilde: ($) =>
      choice(
        $.escape,
        $.code_span,
        $.block_comment,
        $.emphasis,
        $.strong,
        $.element,
        $.label,
        $.hole,
        $.type_seat,
        $.interpolation,
        $._newline,
        $.punctuation,
        alias(/[^\n\\`*_=~<@$()\[{\]/|]+/, $.text),
      ),

    // ---- `$` interpolation: `{$ expr }`, `$(ref)`, `$name(args)` -------
    // Grammar-only recognition of the three spellings `tomet-syntax-lowering`'s
    // `parse_interpolation` reads. `{$ expr }` takes the whole expression
    // language, `$(...)` one reference and `$name(...)` one call; here `$(...)`
    // reads the whole expression language too, since telling a reference from
    // another expression is the real parser's check (it refuses the others).
    // `Path` (`a.b.c`) reuses the existing `identifier` token as one
    // opaque span rather than exposing dot-separated segments -- this
    // grammar is highlighting-only and doesn't need a structured `Path`
    // AST, just to recognize the whole span and not fall into `ERROR`.
    interpolation: ($) =>
      choice(
        seq(token(prec(1, "{$")), $._interp_expr, "}"),
        seq(token(prec(1, "$(")), $._interp_expr, ")"),
        $.interp_dollar_call,
        // `$addr(text)`: raw text up to the `)`, as `addr(...)` is inside an expression.
        alias(token(prec(3, seq("$addr(", /[^)\n]*/, ")"))), $.interp_addr),
      ),
    // `$name(args)`. Its head is one token, `$name(`, so that it is longer than the
    // scalar a value position would read `$name` as, and wins.
    interp_dollar_call: ($) =>
      seq($.interp_dollar_head, optional($._interp_args), ")"),
    interp_dollar_head: (_$) =>
      token(
        prec(
          1,
          /\$[A-Za-z_][A-Za-z0-9_-]*(\.[A-Za-z_][A-Za-z0-9_-]*)*\(/,
        ),
      ),
    // The expression language, as far as highlighting needs it: operators
    // (`+` `-` `==` `!=` `<` `<=` `>` `>=` `&&` `||`), `( expr )`, lambdas
    // (`x => body`, `(a, b) => body`), `doc#id`, and `addr(...)`. See
    // `tmtroot/spec/expressions/`. Precedence is only here to keep the tree
    // unambiguous; the real parser (`tomet-parser/src/interp.rs`) is the
    // authority. `true` / `false` / `null` read as plain identifiers.
    _interp_expr: ($) =>
      choice($.interp_binary, $.interp_lambda, $._interp_operand),
    _interp_operand: ($) =>
      choice(
        $.interp_call,
        $.interp_hash_call,
        $.interp_connect_call,
        $.interp_connect,
        $.interp_member,
        $.interp_hash,
        $.interp_group,
        $.interp_addr,
        $.identifier,
        $.number,
        $.string,
      ),
    interp_binary: ($) =>
      choice(
        prec.left(1, seq($._interp_expr, "||", $._interp_expr)),
        prec.left(2, seq($._interp_expr, "&&", $._interp_expr)),
        prec.left(
          3,
          seq(
            $._interp_expr,
            choice("==", "!=", "<=", ">=", "<", ">"),
            $._interp_expr,
          ),
        ),
        // `+` and `-` need white space on both sides in the real parser (`a-b` is a name);
        // here the spaces are `extras`, and the identifier token already takes `a-b` whole.
        prec.left(4, seq($._interp_expr, choice("+", "-"), $._interp_expr)),
      ),
    interp_group: ($) =>
      seq(
        "(",
        optional(
          seq(
            $._interp_expr,
            repeat(seq(",", $._interp_expr)),
            optional(","),
          ),
        ),
        ")",
      ),
    // A call's arguments: expressions, or `name: expr`.
    _interp_args: ($) => seq($._interp_arg, repeat(seq(",", $._interp_arg))),
    _interp_arg: ($) => choice($.interp_named_arg, $._interp_expr),
    interp_named_arg: ($) =>
      seq(field("name", $.identifier), ":", $._interp_expr),
    // `.name` after something that is not a bare name: `addr(...)::docs.item(0)`,
    // `x.addr().title`, `doc#(id).title`.
    interp_member: ($) =>
      prec.left(
        seq(
          field(
            "object",
            choice(
              $.interp_call,
              $.interp_hash_call,
              $.interp_connect_call,
              $.interp_connect,
              $.interp_hash,
              $.interp_group,
              $.interp_addr,
            ),
          ),
          ".",
          field("member", $.identifier),
        ),
      ),
    // `node::name`: from a node to what hangs off it (`u1::meta`, `self::doc::meta`,
    // `addr(dir:docs)::docs`). The dot reads data inside a node; the `::` goes from
    // node to node. The member is an `identifier`, which already spans the
    // `.key.key` that may follow it (`self::meta.tags`).
    interp_connect: ($) =>
      prec.left(
        seq(
          field(
            "object",
            choice(
              $.identifier,
              $.interp_call,
              $.interp_hash_call,
              $.interp_connect_call,
              $.interp_connect,
              $.interp_hash,
              $.interp_group,
              $.interp_addr,
            ),
          ),
          "::",
          field("member", $.identifier),
        ),
      ),
    interp_lambda: ($) =>
      prec.right(
        0,
        seq(
          field(
            "params",
            choice(
              $.identifier,
              seq("(", $.identifier, repeat(seq(",", $.identifier)), ")"),
            ),
          ),
          "=>",
          $._interp_expr,
        ),
      ),
    // `#id`, `#(id)`, and `doc#id`. The document on the left is any call or
    // name; the id's own `.steps` are part of the `identifier` token.
    interp_hash: ($) =>
      seq(
        optional(field("doc", choice($.interp_call, $.identifier))),
        "#",
        choice($.identifier, seq("(", $.identifier, ")")),
      ),
    // A call whose callee is a `::` member: `self::meta.tags.join(", ")`. Kept apart
    // from `interp_call` for the same reason as `interp_hash_call`: `$name(...)` can
    // never start with a `::` path.
    // Like `interp_call`, its callee may be an earlier such call followed by `.name`:
    // `vault::docs.filter(f).map(g)`.
    interp_connect_call: ($) =>
      prec.left(
        seq(
          field("name", choice($.interp_connect, $.interp_connect_call)),
          repeat(seq(".", field("member", $.identifier))),
          "(",
          optional($._interp_args),
          ")",
        ),
      ),
    // A call whose callee is a `#` address: `#item(0)`, `doc#id.item(2)`.
    // Kept apart from `interp_call` so that `$name(...)` can never start
    // with `#`.
    interp_hash_call: ($) =>
      prec.left(
        seq(field("name", $.interp_hash), "(", optional($._interp_args), ")"),
      ),
    // `addr(` reads raw text up to the first `)`. The real parser also
    // balances nested parentheses; this approximation does not.
    interp_addr: (_$) => token(prec(3, seq("addr(", /[^)\n]*/, ")"))),
    // A call's callee may itself be a call followed by `.name`, which
    // is how `{$ ref(id(x).contents(y)) }` reads: member access binds to
    // whatever precedes it, then takes its own argument list.
    interp_call: ($) =>
      prec.left(
        seq(
          field("name", choice($.identifier, $.interp_call)),
          repeat(seq(".", field("member", $.identifier))),
          "(",
          optional($._interp_args),
          ")",
        ),
      ),
    number: (_$) => /-?[0-9]+(\.[0-9]+)?/,

    // ---- `@name` elements ---------------------------------------------
    //
    // One sigil, whatever the element's placement. `#` and `<T>` were
    // both tried here and both carried no information as a *second
    // sigil*; `#(id)` isn't one either -- it's a slot on the one `@`
    // element, same family as `_element_group`, just read as a
    // sibling choice here rather than folded into it (an id must not
    // take the `:` prefix a bare merge/connect does). Whether an
    // element stands as a block or belongs to running text is decided
    // by position (does it occupy its own line), which is a question
    // for `tomet-parser`, not for syntax highlighting -- so this
    // grammar has one element rule.
    element: ($) => $.inline_element,
    // ---- `<...>` boxes ----------------------------------------------------
    //
    // A name tag, `<PATH>`: a label for the reader that tomet gives no meaning, so the
    // real parser lowers it to text. One token, a letter first and no white space, which is
    // what keeps `a < b`, `x<=1`, `<->`, `</x>` and `<x/>` out of it.
    label: (_$) => token(prec(2, /<\p{L}[\p{L}\p{N}_.\-]*>/)),
    // A hole, `<?matcher(args){data}>[content]#(id)`: the head is inside the box, the
    // content follows its `>`. Simplification: the real parser needs the head to be tight, and
    // here white space (an `extras`) is allowed inside it; and a box in a value position
    // (`url: <?(u)>`) is left to `scalar`, so it is not a `hole` there.
    hole: ($) =>
      prec.right(
        3,
        seq(
          $.hole_open,
          optional($.args_group),
          optional($.value_group),
          $.hole_close,
          optional($.content_group),
          optional($.id_group),
        ),
      ),
    hole_open: (_$) => token(prec(2, /<\?([A-Za-z_][A-Za-z0-9_.\-]*)?/)),
    hole_close: (_$) => token(prec(1, ">")),
    // A type seat, `<$type(args)>`: a place that takes a value of that type. Unlike a hole
    // it has no data and no content, and its `>` ends it. The head, `<$` and the name, is
    // the external `type_seat_open` (see `scanner.c`): it is emitted only when the seat
    // really closes -- the `>` right after the name or after the `(args)`, so no white space
    // stands before it -- and `<$x >`, `<$5>` and `<$x{a: 1}>` stay text, here as in
    // `tomet-parser`. (A `(` that never closes, `<$x(a>`, is no seat either, but then `$x(`
    // is an interpolation call head that never closes: an `ERROR` here, a parse error there.)
    // A seat takes the place of a value as well as of running text (`::id(<$uuid>)`,
    // `key: <$string>`), which is why the scanner also tries it before `scalar`. The
    // arguments are the expression language's (`list(string, length: 6)`, `default: auto`,
    // strings), where a `>` is an operator and closes nothing.
    type_seat: ($) =>
      seq(
        $.type_seat_open,
        optional(
          seq("(", choice($._blank_gap, optional($._seat_args)), ")"),
        ),
        alias($.hole_close, $.type_seat_close),
      ),
    // The arguments may run over lines, as the real parser's do: a line break goes around
    // each argument, where the comma is.
    _seat_args: ($) => seq($._seat_arg, repeat(seq(",", $._seat_arg))),
    _seat_arg: ($) =>
      seq(optional($._blank_gap), $._interp_arg, optional($._blank_gap)),
    // ---- `::name` declarations ------------------------------------------
    //
    // A declaration about the document, not content in it. It is only
    // ever a *block*: `::` has to open a line, which this grammar
    // expresses by listing the rule among `_block`s, so a `::` in the
    // middle of a line (`std::vec`) is lexed as the two `:` it is. The
    // real parser reads what follows the name exactly as it does after
    // an `@`, and so does this: a nexus needs a primary group, so
    // `::meta(a: 1)` is a declaration and `::meta{ a: 1 }` is an error
    // (see `_nexus_groups`). `::vocabulary(ns){ ... }` keeps its `{...}`
    // body as a secondary group after its `(ns)`.
    document_element: ($) =>
      prec.right(
        3,
        seq(
          token(prec(2, "::")),
          field("name", $._element_name),
          $._nexus_groups,
        ),
      ),
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
    inline_element: ($) =>
      prec.right(
        3,
        seq(
          "@",
          optional(field("name", $._element_name)),
          optional($._nexus_groups),
        ),
      ),
    // ---- the nexus rule ---------------------------------------------
    //
    // `@name`, a `::name` declaration and a `::name` connect are nexus points: they take groups, and the
    // groups are not equal. `(args)`, `[content]` and a `|` run are the
    // *primary* groups; `{value}`, `#(id)` and a `:connect` only add to one
    // and cannot stand as the first group on their own. So `@name(...)` and
    // `@name[...]` are elements, `@name{}`, `@name#(id)`, `@name::rule(...)`
    // are errors, and a primary group may follow the secondary ones
    // (`@name{...}(...)`) as in the real parser. `tomet-parser`'s
    // `tomet-syntax-cst-parser` doc has the same rule in prose.
    //
    // Two shapes are exempt, both of the *inline* `@name` alone and neither a
    // nexus error in the real parser: no group at all is prose, and a nameless
    // `::{...}` / `::(...)` connect is a plain group. This rule is the
    // only place the grammar says "primary first"; the three element-shaped
    // rules call it instead of repeating the groups.
    //
    // Known simplification: the real parser also reports a block-level bare
    // `@name` as an error (`@name()` is the empty element), since the line is
    // an element rather than a word in a paragraph. Position is not
    // something this grammar sees (see `element`), so a bare `@name` is
    // accepted wherever it stands.
    _nexus_groups: ($) =>
      prec.right(
        3,
        seq(
          repeat(choice($.value_group, $.id_group)),
          $._primary_group,
          repeat(choice($._element_group, $.connect, $.id_group)),
        ),
      ),
    // The groups that make a nexus: `(args)`, `[content]`, and the `|` run
    // that stands for a `[content]` without its brackets (`@table` then `|`
    // rows on the next lines).
    _primary_group: ($) =>
      choice($.args_group, $.content_group, $.marked_content),
    // Two adjacent `optional($._blank_gap)` around an optional middle
    // piece is ambiguous (nothing forces how a run of blank lines splits
    // between "leading" and "trailing" when the middle is absent), so
    // the leading gap is folded into the same `optional(...)` as the
    // content it precedes instead -- when there's no content, only the
    // single trailing gap is ever reachable.
    //
    // The opening `(`/`[`/`{` are wrapped in `prec(1, ...)`: once
    // `extras` (inline whitespace) sits between an element's sigil and
    // its first group -- `@links() {` rather than `@links(){` -- that same
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
    content_group: ($) =>
      seq(token(prec(1, "[")), repeat($._bracket_item), "]"),
    // `|content` is `[content]` without the brackets: the same group,
    // closed by the end of the marked run rather than by `]`.
    //
    // Simplification: `tomet-parser` also requires every marker in one
    // run to stand in the same column, and rejects a run whose markers
    // do not line up. A column is not something this grammar can see
    // without an external scanner, and a highlighter does not need it,
    // so any indentation is accepted here -- one more entry in the list
    // of approximations in `src/lib.rs`.
    marked_content: ($) =>
      prec.right(
        repeat1(
          prec.right(
            seq(
              token(prec(1, "|")),
              repeat($._line_item),
              optional($._newline),
            ),
          ),
        ),
      ),
    // `{...}` is always data. Entries are read uniformly, each a
    // `key: value` pair, a bare `(marker)[content]` entry, or a named
    // element, in source order -- nothing here consults the enclosing
    // element's name. `@links(){ (1)[a] note: x (2)[b] }` and
    // `@element(c){ display: block @args(){ @param(id){} } }` are the two
    // shapes that needs.
    //
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
    // A `{...}` group's elements: bare `(args)[content]` entries whose
    // container supplies the type (`@links(){ (1)[...] (2)[...] }`), and
    // named ones (`@element(c){ @args(){ @param(id){} } }`), which is how
    // a vocabulary declares an element's slots.
    //
    // Pairs and elements in the *same* group are a shape the real
    // parser takes and this rule does not -- see `KNOWN_TS_ERRORS`.
    // Splicing `map_entry` into this list was tried and reverted: it
    // makes `id: review` ambiguous with `id:` followed by an entry
    // keyed `review`, and the lexer prefers `identifier` over a scalar.
    children: ($) =>
      repeat1(
        seq(optional($._blank_gap), choice($.bare_element, $.inline_element)),
      ),
    // Simplification: unlike `args_group`/`value_group`'s own internal
    // gaps, `content_group` here must immediately follow (inline whitespace
    // only, no blank-line tolerance) -- avoids an LR conflict where a
    // lone newline can't be told apart from "no content_group at all" with
    // only one token of lookahead.
    bare_element: ($) => seq($.args_group, optional($.content_group)),
    // A group opener with no leading `:` at all -- what a connect's
    // own body may take (see `connect`, below). Not the same list as
    // `_element_group`'s (just below) because a connect's own groups
    // permit neither spelling of a leading `:` (matching
    // `parse_groups(cur, &mut connect_el, false)` in the real parser:
    // `allow_colon_connect` gates the *whole* colon-anything branch,
    // bare merge included, not just the named-connect check inside
    // it).
    _plain_group: ($) =>
      choice($.args_group, $.content_group, $.marked_content, $.value_group),
    // The original, unchanged bare-merge/plain-group form -- kept
    // exactly as it was (a flat `seq`, not nested inside a further
    // `choice`) because `inline_element`'s own `prec.right(3, ...)`
    // was tuned against this exact shape; wrapping it changed which
    // LALR states got merged and silently broke the bare `::{...}`/
    // `::(...)` merge even after a preceding group (verified: it did,
    // empirically, during this feature's own development -- see
    // `.agents/tasks/rule-connect-mvp.md`). `connect` (below) is
    // therefore added as a *sibling* choice at each `repeat(...)`
    // call site instead of being folded into this rule.
    // `token(prec(1, "::"))`, not a bare `"::"`: the same reason
    // `args_group`'s opening `(` is wrapped the same way -- `:` is
    // now excluded from `text` (see that rule's comment) so it can
    // start a group/connect at all, but `punctuation` accepts a bare
    // `:` too (as the fallback for one with nothing to attach to),
    // an equal-length match this token must outrank or lose every
    // tie to it.
    _element_group: ($) =>
      seq(
        optional(token(prec(1, "::"))),
        choice($.args_group, $.content_group, $.marked_content, $.value_group),
      ),
    // `::name(...)` -- one member of a closed family attached after an
    // element (`@x(...)::as(y)::rule(...)`), e.g. `::rule(allow:(card))`.
    // Structurally an element in its own right (its own `args`/
    // `content`/`value`), just introduced by `:` instead of `@` --
    // mirrors `tomet-parser::element::parse_groups`'s named-connect
    // branch, which reuses its own recursive call for exactly this
    // reason.
    //
    // Takes `repeat(choice($._plain_group, $.id_group))`, not
    // `repeat(choice($._element_group, $.connect))`: a connect must
    // not itself swallow a *sibling* connect, nor a bare merge of its
    // own (both would need a `:` that `_plain_group` never matches),
    // so this repeat naturally stops right before the next `:`,
    // leaving it for whichever *outer* rule holds the sibling
    // `repeat(...)` to pick up. `id_group` is included alongside it
    // because a connect is structurally an element in its own right
    // (see below) and can carry its own id the same way one does: a
    // connect claims an adjacent `#(...)` for itself, as its own groups are.
    //
    // `prec.right(4, ...)` (higher than `inline_element`'s own
    // `prec.right(3, ...)`): a bare group right after `::name` --
    // `::rule(1)` -- is otherwise genuinely ambiguous between "the
    // connect's own group" and "a fresh, colon-less group the
    // *outer* element's repeat happens to also accept" (both are
    // valid parses of the same text, differing only in which node
    // owns it), the same shift/reduce shape `inline_element`'s own
    // comment already describes for a trailing value group. The real
    // parser has no such ambiguity -- `parse_groups`'s recursive call
    // for a connect greedily consumes every immediately-adjacent
    // group before returning control -- so the higher precedence
    // here reproduces that greediness: prefer extending the connect
    // over ending it early.
    connect: ($) =>
      prec.right(
        4,
        seq(
          token(prec(1, "::")),
          field("name", $._element_name),
          repeat(choice($.value_group, $.id_group)),
          $._primary_group,
          repeat(choice($._plain_group, $.id_group)),
        ),
      ),
    // `#(foobar)` -- an element's (or a connect's) own id, read after
    // its `(args)[content]{value}` but before any following connect
    // -- mirrors `tomet-parser::element::parse_hash_id`. The `#` is
    // wrapped in `token(prec(1, ...))` for the same longest-match
    // reason `connect`'s own `:` is: `punctuation` also matches a
    // bare `#` (the fallback for one with nothing to open), an
    // equal-length tie this token must win.
    id_group: ($) => seq(token(prec(1, "#")), "(", $.value, ")"),
    identifier: (_$) => /[A-Za-z_][A-Za-z0-9_.-]*/,
    // `inline_element`'s name field specifically: needs
    // higher lexical precedence than `text` (both can match e.g. "meta"
    // in `@meta(...)`, equal length since both stop at `(`, and
    // tree-sitter only consults token precedence to break length ties)
    // -- but *not* applied to `identifier` generally, since that would
    // also make it win over `map_entry`'s already-lost tie-break against
    // `scalar` in a way that breaks rather than fixes things (see
    // `_entry_value`/`scalar`'s comment). Aliased back to `identifier`
    // (referencing the rule, not a bare string) so it still shows up as
    // a normal, visible `identifier` node in the tree.
    // An element name is an ASCII identifier, optionally `.`-separated
    // into `namespace.name`. The old permissive charset existed to let
    // `<id:taskA>` smuggle a target through the name; that is
    // `#id(taskA)` now, so the name can be a real identifier.
    _element_name: ($) =>
      alias(
        token(prec(1, /[A-Za-z_][A-Za-z0-9_-]*(\.[A-Za-z_][A-Za-z0-9_-]*)*/)),
        $.identifier,
      ),

    // ---- data value grammar (mirrors `value.rs`, simplified) --------
    // Newlines aren't in `extras` (they're structurally significant at
    // the block level), so groups that tolerate embedded blank lines
    //
    // No `$.call` alternative here (see this crate's module doc for
    // why `@id(list(a, b))`/`::meta(list(a, b))` -- a bare `list(...)`
    // occupying the *whole* `(args)`/`{value}` -- still errors,
    // unlike `a: list(...)` under a named key).
    value: ($) =>
      choice($.map, $.string, $.type_seat, $.interpolation, $.scalar),
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
          choice(
            alias(token(prec(1, /[A-Za-z_][A-Za-z0-9_.-]*/)), $.identifier),
            $.string,
          ),
        ),
        choice(
          seq(":", optional(field("value", $._entry_value))),
          field("value", $.braced_map),
        ),
      ),
    // `$._element_name` is also listed bare (not just inside `$.call`):
    // once its token wins the lexer's length-tie against
    // `_value_scalar` for an identifier-shaped word (see `call`'s own
    // comment), the *only* thing that could reduce it at all -- with
    // no `(` following -- has to be a plain, paren-less value alias
    // living at this same choice, or an ordinary bare word (`variant:
    // info`) turns into a dead end instead of a scalar. Aliased to
    // `$.scalar`, the same node `_value_scalar` already produces, so
    // this is invisible in the tree either way.
    // `name(arg, arg, ...)` -- an immediately-resolved literal
    // (`string(a)`) or a type (`list(string)`), mirroring
    // `tomet_ast::Value::Call`. A sequence is not a call: see `sequence`.
    // Deliberately a *plain*, unprecedented regex -- not `_element_name`
    // (which is `token(prec(1, ...))`, aliased to `$.identifier`, this
    // grammar's declared `word` token) and not wrapped in `token(prec(...))`
    // itself. Both were tried and both broke a real, pre-existing case:
    // `url:https://example.com` -- `_value_scalar`'s colon-inclusive
    // regex matches the whole scheme URI, strictly longer than the
    // bare word `https` a precedence-boosted (or `word`-aliased)
    // callee token would match up to its `:` -- and longest match is
    // supposed to always win regardless of precedence, but empirically
    // did not once either of those was in play (suspected interaction
    // with tree-sitter's `word`/keyword-extraction fast path, not
    // fully root-caused -- this plain-token form is the confirmed
    // workaround, not a chosen design). Confirmed clean now: a bare
    // scheme URI still parses as one scalar, and `list(string)` still
    // reaches this rule -- see this crate's own tests and
    // `.agents/tasks/rule-connect-mvp.md` for what was actually tried.
    //
    // Reuses no shared name-token at all for the same reason; matches
    // `_element_name`'s *pattern* (an ASCII identifier, optionally
    // `.`-separated) without matching its *token*, since sharing the
    // token is what caused the regression above.
    call: ($) =>
      seq(
        field(
          "name",
          alias(
            /[A-Za-z_][A-Za-z0-9_-]*(\.[A-Za-z_][A-Za-z0-9_-]*)*/,
            $.identifier,
          ),
        ),
        "(",
        optional(seq($._entry_value, repeat(seq(",", $._entry_value)))),
        ")",
      ),
    // `_entry_value`'s own plain-word fallback: once `call`'s callee
    // token is a live alternative here, an argument or value that is
    // just a bare word with nothing following it (`variant: info`,
    // or `list(card)`'s own `card`) needs *something* to reduce to,
    // or it dead-ends instead of becoming a scalar -- aliased to
    // `$.scalar`, the same node `_value_scalar` already produces, so
    // this is invisible in the tree either way. Whether a bare word is
    // read as *this* or as the start of `call` is then an ordinary,
    // unambiguous one-token-lookahead decision: shift `(` to continue
    // into a call, or reduce on anything else.
    // `(a, b)` -- a bare `(` where a value goes is a sequence
    // (`tomet_ast::Value::Seq`): a value has no infix arithmetic, so no
    // parenthesis there can group anything. `()` is empty and `(x)` has
    // one element. Its elements are values, never `key: value` entries.
    sequence: ($) =>
      seq(
        "(",
        optional(seq($._entry_value, repeat(seq(",", $._entry_value)))),
        ")",
      ),
    // `[a, b]` (a `$.seq` rule) used to be a third alternative here.
    // Retired in `tomet-parser`: `[`/`]` already mean `[content]` at
    // the element level, and `(a, b)` (`$.sequence`) is the sole
    // list-value spelling.
    _entry_value: ($) =>
      choice(
        $.call,
        $.sequence,
        $.string,
        $.braced_map,
        $.type_seat,
        $.interpolation,
        $._value_scalar,
        alias(/[A-Za-z_][A-Za-z0-9_-]*(\.[A-Za-z_][A-Za-z0-9_-]*)*/, $.scalar),
      ),
    braced_map: ($) =>
      seq(
        token(prec(1, "{")),
        optional(seq(optional($._blank_gap), $.map)),
        optional($._blank_gap),
        "}",
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
    // KNOWN DRIFT from `tomet_ast_parser` (see this crate's own module
    // doc, and `docs/design/decisions/2026-08-22-link-reference-uri-schemes.md`
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
