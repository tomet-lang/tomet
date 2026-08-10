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
#include "tree_sitter/parser.h"

enum TokenType {
  SCALAR,
};

void *tree_sitter_typedmark_external_scanner_create(void) { return NULL; }
void tree_sitter_typedmark_external_scanner_destroy(void *payload) { (void)payload; }
unsigned tree_sitter_typedmark_external_scanner_serialize(void *payload, char *buffer) {
  (void)payload;
  (void)buffer;
  return 0;
}
void tree_sitter_typedmark_external_scanner_deserialize(void *payload, const char *buffer,
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

bool tree_sitter_typedmark_external_scanner_scan(void *payload, TSLexer *lexer,
                                                   const bool *valid_symbols) {
  (void)payload;
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
