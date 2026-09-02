// External scanner that owns `scalar` tokenization entirely (see
// `grammar.js`'s `scalar` rule). Originally `scalar` was a plain internal
// regex token; that couldn't disambiguate a bare, colon-less,
// identifier-shaped scalar directly inside `(...)`/`{...}` (e.g.
// `@meta(yaml)`'s `yaml`, or `{required}`'s `required`) from `map_entry`'s
// key token -- both are equal-length matches at that position, and
// tree-sitter's lexer only uses declared precedence to break *equal-length*
// ties, so it always picked `map_entry`'s key (which then fails to find the
// `:` a key must be followed by) regardless of whether a `:` actually
// followed.
//
// Giving `scalar` an *additional* external alternative next to the old
// internal regex (rather than replacing it) was tried first and rejected:
// with both present, tree-sitter's generated lexer state for that position
// stops being able to prove the internal regex's match is always at least
// as long as `map_entry`'s key token, so it falls back to always preferring
// the key token there -- reintroducing the same bug for *any* multi-word
// bare scalar (`{hello world}`), not just the identifier-shaped-tie case
// this scanner exists to fix. Replacing the regex outright avoids that: this
// scanner is the *only* way `scalar` ever matches, so there is no competing
// internal alternative left for tree-sitter to under-resolve.
// `LIST_MARKER_TOKEN`/`LIST_MARKER_GAP` (see `grammar.js`'s `list_marker`
// comment) exist for a related but distinct reason: `document.rs::eat_list_marker`
// skips inline whitespace *before* checking for a `(` marker, so
// deciding whether the whitespace right after a list's `-`/`-.` belongs to
// an optional value marker or is just the item's own mandatory content gap
// needs to look *past* that whitespace before committing to either
// reading. Tree-sitter's internal lexer builds one shared DFA across all
// tokens valid at a given parser state and can't backtrack out of it: a
// real space character matches the mandatory-gap token outright and wins
// immediately, before ever getting a chance to look further ahead for a
// bracket. This scanner instead gets one call per position with *all* the
// externals valid there (`valid_symbols`) and picks whichever one actually
// fits after looking as far ahead as it needs to.
//
// Not a byte-for-byte port of `eat_list_marker`: that function operates on
// a *copy* of the cursor and only commits (`cur.set_pos`) on success, so a
// marker that starts looking like `(...)` but turns out not to
// close-and-then-have-trailing-whitespace costs it nothing -- the whole
// line falls back to being reparsed as plain content from the original
// `-`. `TSLexer` has no such rewind (advancing is one-directional even on
// a `false` return), so the same malformed input here just characters
// already stepped over while probing for a closing bracket are lost to
// any other candidate token, typically surfacing as an `ERROR` node
// instead of gracefully degrading to `punctuation`/`text`. Judged an
// acceptable gap for this approximation grammar: well-formed value
// markers (`- (T)`, `- ("?")`, ...) are the real-world case that matters,
// per `docs/cheatsheet.tmt`.
#include "tree_sitter/parser.h"

enum TokenType {
  SCALAR,
  LIST_MARKER_TOKEN,
  LIST_MARKER_GAP,
  RAW_FENCE,
};

void *tree_sitter_tomet_external_scanner_create(void) { return NULL; }
void tree_sitter_tomet_external_scanner_destroy(void *payload) { (void)payload; }
unsigned tree_sitter_tomet_external_scanner_serialize(void *payload, char *buffer) {
  (void)payload;
  (void)buffer;
  return 0;
}
void tree_sitter_tomet_external_scanner_deserialize(void *payload, const char *buffer,
                                                          unsigned length) {
  (void)payload;
  (void)buffer;
  (void)length;
}

static bool is_ident_start(int32_t c) {
  return c == '_' || (c >= 'A' && c <= 'Z') || (c >= 'a' && c <= 'z');
}

static bool is_ident_continue(int32_t c) {
  return is_ident_start(c) || (c >= '0' && c <= '9') || c == '.' || c == '-';
}

// The same 9 characters the old regex (`/[^,():\[\]{}\n\r]+/`) excluded.
static bool is_scalar_terminator(int32_t c) {
  switch (c) {
    case ',':
    case '(':
    case ')':
    case ':':
    case '[':
    case ']':
    case '{':
    case '}':
    case '\n':
    case '\r':
      return true;
    default:
      return false;
  }
}

// A whole `+++` fence -- opening run, the rest of its line, the body, and
// the closing run -- as one token.
//
// Taking the entire fence in a single token is what lets this be
// stateless: the opening run's length only has to survive until the
// matching close is found, which is within this one call, so the scanner
// needs no serialize/deserialize. That is also why the backtick fence's
// variable length was left as a known gap in `grammar.js` -- it is split
// across an opener, a body and a closer, and would need real state.
//
// Mirrors `crates/tomet-syntax-parser/src/fence.rs`: a closing line is one
// whose leading `+` run is at least as long as the opening run and which
// holds nothing else but inline whitespace; an unterminated fence runs to
// EOF rather than failing.
static bool scan_raw_fence(TSLexer *lexer) {
  // External scanners run before tree-sitter's automatic `extras`
  // skipping, so inline whitespace between the element head and its
  // fence (`#meta(format:yaml) +++`) has to be consumed here.
  while (lexer->lookahead == ' ' || lexer->lookahead == '\t') {
    lexer->advance(lexer, true);
  }

  unsigned open_run = 0;
  while (lexer->lookahead == '+') {
    lexer->advance(lexer, false);
    open_run++;
  }
  if (open_run < 3) {
    return false;
  }

  // Rest of the opening line.
  while (!lexer->eof(lexer) && lexer->lookahead != '\n' && lexer->lookahead != '\r') {
    lexer->advance(lexer, false);
  }

  while (!lexer->eof(lexer)) {
    // Consume the line terminator.
    if (lexer->lookahead == '\r') {
      lexer->advance(lexer, false);
    }
    if (lexer->lookahead == '\n') {
      lexer->advance(lexer, false);
    }

    unsigned run = 0;
    while (lexer->lookahead == '+') {
      lexer->advance(lexer, false);
      run++;
    }
    if (run >= open_run) {
      while (lexer->lookahead == ' ' || lexer->lookahead == '\t') {
        lexer->advance(lexer, false);
      }
      if (lexer->eof(lexer) || lexer->lookahead == '\n' || lexer->lookahead == '\r') {
        lexer->mark_end(lexer);
        lexer->result_symbol = RAW_FENCE;
        return true;
      }
    }
    // Not a closing line -- skip the rest of it.
    while (!lexer->eof(lexer) && lexer->lookahead != '\n' && lexer->lookahead != '\r') {
      lexer->advance(lexer, false);
    }
  }

  lexer->mark_end(lexer);
  lexer->result_symbol = RAW_FENCE;
  return true;
}

// Handles both `LIST_MARKER_TOKEN` and `LIST_MARKER_GAP` in one pass --
// see this file's module doc for why they can't be two independent
// lookahead-free tokens.
static bool scan_list_marker_gap(TSLexer *lexer, const bool *valid_symbols) {
  bool want_marker = valid_symbols[LIST_MARKER_TOKEN];
  bool want_gap = valid_symbols[LIST_MARKER_GAP];
  if (!want_marker && !want_gap) {
    return false;
  }

  bool consumed_ws = false;
  while (lexer->lookahead == ' ' || lexer->lookahead == '\t') {
    lexer->advance(lexer, true);
    consumed_ws = true;
  }

  if (want_marker && lexer->lookahead == '(') {
    lexer->advance(lexer, false);
    while (!lexer->eof(lexer) && lexer->lookahead != ')' &&
           lexer->lookahead != '\n' && lexer->lookahead != '\r') {
      lexer->advance(lexer, false);
    }
    if (lexer->lookahead == ')') {
      lexer->advance(lexer, false);
      // A marker only counts as one if followed by whitespace, mirroring
      // `document.rs::eat_list_marker` -- otherwise it's just ordinary
      // line content (e.g. `-(x)text`, no `list_marker` node).
      if (lexer->lookahead == ' ' || lexer->lookahead == '\t') {
        lexer->mark_end(lexer);
        lexer->result_symbol = LIST_MARKER_TOKEN;
        return true;
      }
    }
    // Not a valid marker after all -- see the "not a byte-for-byte
    // port" note above; falling through to the plain-gap check below
    // only helps if no bracket-probing characters were consumed yet,
    // which is no longer the case once execution reaches here.
  }

  if (want_gap && consumed_ws) {
    lexer->mark_end(lexer);
    lexer->result_symbol = LIST_MARKER_GAP;
    return true;
  }

  return false;
}

bool tree_sitter_tomet_external_scanner_scan(void *payload, TSLexer *lexer,
                                                   const bool *valid_symbols) {
  (void)payload;

  if (valid_symbols[RAW_FENCE] &&
      (lexer->lookahead == '+' || lexer->lookahead == ' ' || lexer->lookahead == '\t')) {
    return scan_raw_fence(lexer);
  }

  if (valid_symbols[LIST_MARKER_TOKEN] || valid_symbols[LIST_MARKER_GAP]) {
    return scan_list_marker_gap(lexer, valid_symbols);
  }

  if (!valid_symbols[SCALAR]) {
    return false;
  }

  // External scanners run *before* tree-sitter's automatic `extras`
  // skipping (`extras: ($) => [/[ \t]/]` in `grammar.js`), so leading
  // inline whitespace has to be consumed here too -- e.g. `{ id:header1 }`
  // needs this scanner to see past the leading space to notice "id:" is a
  // map key and decline, not swallow " id" itself as a bogus scalar before
  // ever finding the `:`. `advance(lexer, true)` marks the skipped
  // characters as whitespace/extra, same as the automatic skip would have.
  // Newlines are *not* included: unlike inline whitespace, they're
  // structurally significant (`_blank_gap`), not part of `extras`.
  while (lexer->lookahead == ' ' || lexer->lookahead == '\t') {
    lexer->advance(lexer, true);
  }

  if (lexer->eof(lexer) || is_scalar_terminator(lexer->lookahead)) {
    // The old regex required at least one character (`+`), and never
    // matched starting on a terminator (or `:`, also excluded); an empty
    // match here would let this token fire where it never used to.
    return false;
  }

  // If this starts with an identifier-shaped run, check what follows it
  // *before* committing to a scalar: a `:` right after means this is
  // actually a `map_entry` key, not a bare scalar, no matter what the rest
  // of the line looks like -- decline outright so the internal `identifier`
  // token (and `map_entry`) wins instead, exactly as it already does today
  // whenever the value position is unambiguous (e.g. `key: value`, handled
  // by `_value_scalar`, is untouched by this scanner).
  if (is_ident_start(lexer->lookahead)) {
    while (is_ident_continue(lexer->lookahead)) {
      lexer->advance(lexer, false);
    }
    if (lexer->lookahead == ':') {
      return false;
    }
    while (lexer->lookahead == ' ' || lexer->lookahead == '\t') {
      lexer->advance(lexer, false);
    }
    if (lexer->lookahead == '{') {
      return false;
    }
  }

  // Not a `key:` after all (or never looked like one to begin with) --
  // keep consuming exactly like the old regex did: everything up to the
  // next terminator/EOF, so multi-word bare scalars (`{hello world}`) and
  // non-identifier-shaped ones (`{42}`, `{-3.5}`) still produce one whole
  // `scalar` token, not just an identifier-shaped prefix of it.
  while (!lexer->eof(lexer) && !is_scalar_terminator(lexer->lookahead)) {
    lexer->advance(lexer, false);
  }

  lexer->mark_end(lexer);
  lexer->result_symbol = SCALAR;
  return true;
}
