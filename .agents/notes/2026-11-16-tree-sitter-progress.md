AGENTS.md read

# Tree-Sitter One-Container Update Progress

## Brief Summary & Constraints
- `( )` holds named and unnamed entries uniformly: `(k: v)`, `(a, b)`, `(a, k: v)`.
- `{ }` is removed from value positions; `{ ... }` at a value position is an error.
- Postfix groups (`elem { ... }`) and gates (`{$ ... }`) remain.
- Real parser (`tomet-parser`, `crates/tomet-syntax-*`) is the source of truth; tree-sitter grammars follow it.
- No commit, no `jj describe`, no `jj new`, no push.
- Environment for cargo: `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0`.
- All long-running commands run under `timeout`.

## Step 0: Survey (No Edits)
- [x] Read `grammar.js` files (`crates/tree-sitter-tomet/grammar.js`, `crates/tree-sitter-cotomet/grammar.js`)
- [x] Read `src/lib.rs` files (`crates/tree-sitter-tomet/src/lib.rs`, `crates/tree-sitter-cotomet/src/lib.rs`)
- [x] Read `queries/*.scm` in both crates
- [x] Read `scanner.c` where it mentions value or brace
- [x] Survey uses of `braced_map`, `map`, `map_entry`, `sequence`, `call` across queries, `editors/`, Rust tests, `tests/src`
- [x] Run corpus test with 4 `KNOWN_TS_ERRORS` removed locally and record first error location and construct for each:
  - `syntax/tuple.tmt`
  - `default.config.tmt`
  - `syntax/nexus-declarations.tmt`
  - `test.config.tmt`

### Survey: Node-name uses
1. `braced_map`:
   - `crates/tree-sitter-tomet/grammar.js`: `braced_map` rule, `field("value", $.braced_map)` in `map_entry`, `$.braced_map` in `_entry_value`
   - `crates/tree-sitter-tomet/queries/indents.scm`: `(braced_map "}" @end) @indent`
   - `crates/tree-sitter-tomet/src/lib.rs`: doc comments and `node-types.json` / `parser.c` bindings
   - `crates/tree-sitter-cotomet`: none
   - `editors/`: symlinks to crate queries
   - `tests/src`: none
2. `map`:
   - `crates/tree-sitter-tomet/grammar.js`: `map` rule (`seq($.map_entry, repeat(seq($._entry_sep, $.map_entry)))`), used in `value` and `braced_map`; in `conflicts: [$.map, $.children]`
   - `crates/tree-sitter-tomet/src/lib.rs`: doc comments
   - `crates/tree-sitter-cotomet/grammar.js`: `map` rule (`seq("{", repeat($._item), "}")`), used in `declaration` (`field("data", $.map)`), `_value`, `_bare_value`
   - `crates/tree-sitter-cotomet/queries/folds.scm`: `(map)`
   - `crates/tree-sitter-cotomet/queries/indents.scm`: `(map "}" @end) @indent`
   - `crates/tree-sitter-cotomet/src/lib.rs`: unit tests with `m: { ... }`
3. `map_entry`:
   - `crates/tree-sitter-tomet/grammar.js`: `map_entry` rule, used in `map`
   - `crates/tree-sitter-tomet/queries/highlights.scm`:
     `(map_entry key: (identifier) @property)`
     `(map_entry key: (string) @property)`
   - `crates/tree-sitter-tomet/src/lib.rs`: doc comments, unit test `a_bare_scalar_after_extras_is_not_swallowed_as_extras`
   - `crates/tree-sitter-tomet/src/scanner.c`: comments explaining key lookahead probe
   - `crates/tree-sitter-cotomet`: none (`cotomet` uses `pair`)
4. `sequence`:
   - `crates/tree-sitter-tomet/grammar.js`: `sequence` rule, used in `_entry_value`
   - `crates/tree-sitter-tomet/queries/highlights.scm`:
     `(sequence "(" @punctuation.bracket)`
     `(sequence ")" @punctuation.bracket)`
   - `crates/tree-sitter-tomet/queries/indents.scm`: `(sequence ")" @end) @indent`
   - `crates/tree-sitter-tomet/src/lib.rs`: doc comments and unit tests (`parses_sequence_syntax_in_values`)
   - `crates/tree-sitter-cotomet/grammar.js`: `sequence` rule, used in `_value`, `_bare_value`
   - `crates/tree-sitter-cotomet/queries/folds.scm`: `(sequence)`
   - `crates/tree-sitter-cotomet/queries/indents.scm`: `(sequence ")" @end) @indent`
   - `crates/tree-sitter-cotomet/src/lib.rs`: doc comments and unit tests (`values_match_shape`)
   - `tests/src/lib.rs`: doc comment
5. `call`:
   - `crates/tree-sitter-tomet/grammar.js`: `call` rule, used in `_entry_value`
   - `crates/tree-sitter-tomet/queries/highlights.scm`:
     `(call name: (identifier) @function)`
     `(call "(" @function)`
     `(call ")" @function)`
   - `crates/tree-sitter-tomet/queries/indents.scm`: `(call ")" @end) @indent`
   - `crates/tree-sitter-tomet/src/lib.rs`: doc comments and unit tests (`parses_call_syntax_in_values`)
   - `crates/tree-sitter-cotomet/grammar.js`: `call` rule, used in `_value`, `_bare_value`
   - `crates/tree-sitter-cotomet/queries/folds.scm`: `(call)`
   - `crates/tree-sitter-cotomet/queries/highlights.scm`:
     `(call function: (name) @function.builtin ...)`
     `(call function: (name) @function ...)`
   - `crates/tree-sitter-cotomet/queries/indents.scm`: `(call ")" @end) @indent`
   - `crates/tree-sitter-cotomet/src/lib.rs`: doc comments and unit tests

### Survey: Four KNOWN_TS_ERRORS fixtures
When removed locally from `KNOWN_TS_ERRORS`:
1. `syntax/tuple.tmt`:
   - First error: Line 2:1 (inside `::meta(...)` at `tags: (a, b, sep: ",")` on line 3).
   - Construct: `(a, b, sep: ",")` - tuple containing mixed unnamed and named entries.
   - Reason: Old `sequence` rule only accepted values separated by `,`, not `key: value` pairs (`map_entry`).
2. `default.config.tmt`:
   - First error: Line 3:9: `meta: (`.
   - Construct: `meta: (\n  always_newline: true\n)`.
   - Reason: Value-position tuple holding `key: value` entries (`always_newline: true`), not accepted by old `sequence`.
3. `syntax/nexus-declarations.tmt`:
   - First error: Line 19:30: `( type: datetime`.
   - Construct: `created: ( type: datetime, format: iso8601 )` inside `::migration(...)`.
   - Reason: Value-position tuple holding `key: value` entries, not accepted by old `sequence`.
4. `test.config.tmt`:
   - First error: Line 3:9: `meta: (`.
   - Construct: `meta: (\n  always_newline: true\n)`.
   - Reason: Value-position tuple holding `key: value` entries, not accepted by old `sequence`.

None of the four fail due to older known gaps; all four fail directly due to tuples holding `key: value` entries or mixed entries.

Proof (corpus test failure with 4 entries removed):
```
FIRST ERROR in default.config.tmt: line 3:9: kind=ERROR, text="("
FIRST ERROR in syntax/nexus-declarations.tmt: line 19:30: kind=ERROR, text="( type: datetime"
FIRST ERROR in syntax/tuple.tmt: line 2:1: kind=ERROR, text="::meta(\n  tags: (a, b, sep: \",\")\n  size: (w: 10, h: 20)\n  x: ((a, version: 3), b, c)\n  icon: glyph(triangle, pkg: lucide)\n  use: vocab(a, version: 3)\n  ordered: (\"1\": a, b)\n  status: (\"404\": x, \"500\": y)\n  numbered: (\"0\": a, \"1\": b)\n)\n\n=[ Tuple ]\n\nA tuple at a value position takes named and unnamed entries together.\n\n@item(tags: (a, b, sep: \",\"), size: (w: 10, h: 20))\n\n@item(x: ((a, version: 3), b, c))\n\nA call takes named and unnamed entries together.\n\n@item(icon: glyph(triangle, pkg: lucide), use: vocab(a, version: 3))\n\nEntries may be keyed by decimal text.\n\n@item(ordered: (\"1\": a, b), status: (\"404\": x, \"500\": y), numbered: (\"0\": a, \"1\": b))\n"
FIRST ERROR in test.config.tmt: line 3:9: kind=ERROR, text="("

thread 'tree_sitter_has_no_unexpected_errors_anywhere_in_the_corpus' (940889) panicked at tests/src/corpus.rs:188:5:
the tree-sitter grammar errors on syntax `tomet-parser` accepts.
`grammar.js` is hand-maintained and does not follow the parser on
its own -- update it, or record the case in `KNOWN_TS_ERRORS`:
  default.config.tmt: "    always_newline: true\n  )\n)\n::workspace(\n  ignore: (\"00-09 System/01 Apps/obsidian\")\n)\n::migration(\n  meta: (\n    created: (\n      type: datetime\n      format: iso8601\n    )\n    updated: (\n      type: datetime\n      format: iso8601\n      merge: modified\n    )\n  )\n  url: (\n    args: (\n      display: force\n    )\n    style: link\n  )\n)\n"
  default.config.tmt: "("
  syntax/nexus-declarations.tmt: "( type: datetime"
  syntax/tuple.tmt: "0"
  syntax/tuple.tmt: "1"
  syntax/tuple.tmt: "1\": a, b)\n  status: (\"404\": x, \"500\": y)\n  numbered: (\"0\": a, \"1\": b)\n)\n\n=[ Tuple ]\n\nA tuple at a value position takes named and unnamed entries together.\n\n@item(tags: (a, b, sep: \""
  syntax/tuple.tmt: "1\": b))\n"
  syntax/tuple.tmt: "404"
  syntax/tuple.tmt: "500"
  syntax/tuple.tmt: "::meta(\n  tags: (a, b, sep: \",\")\n  size: (w: 10, h: 20)\n  x: ((a, version: 3), b, c)\n  icon: glyph(triangle, pkg: lucide)\n  use: vocab(a, version: 3)\n  ordered: (\"1\": a, b)\n  status: (\"404\": x, \"500\": y)\n  numbered: (\"0\": a, \"1\": b)\n)\n\n=[ Tuple ]\n\nA tuple at a value position takes named and unnamed entries together.\n\n@item(tags: (a, b, sep: \",\"), size: (w: 10, h: 20))\n\n@item(x: ((a, version: 3), b, c))\n\nA call takes named and unnamed entries together.\n\n@item(icon: glyph(triangle, pkg: lucide), use: vocab(a, version: 3))\n\nEntries may be keyed by decimal text.\n\n@item(ordered: (\"1\": a, b), status: (\"404\": x, \"500\": y), numbered: (\"0\": a, \"1\": b))\n"
  test.config.tmt: "    always_newline: true\n  )\n)\n::workspace(\n  ignore: (\"00-09 System/01 Apps/obsidian\")\n)\n::migration(\n  created: (\n    type: datetime\n    format: iso8601\n  )\n  updated: (\n    type: datetime\n    format: iso8601\n    merge: modified\n  )\n)\n"
  test.config.tmt: "("
```

## Step 1: Grammar Updates
- [x] `crates/tree-sitter-tomet/grammar.js`:
  - Removed `braced_map` and the `key { ... }` form of `map_entry`
  - Replaced `sequence` with `tuple` taking named (`map_entry`) and unnamed entries
  - Unified call argument list to use `_tuple_items`
  - Resolved LR conflicts cleanly and documented rationale
- [x] `crates/tree-sitter-cotomet/grammar.js`:
  - Updated `map` and `sequence` rules to `tuple`
  - Removed `$.map` from `_value` and `_bare_value`
  - Removed `::meta{ ... }` declaration comments
- [x] Updated `src/lib.rs` module docs and "known simplifications" in both crates

### LR Conflict Rationale & Lexer Resolution
1. **Recursion & Entry Separator in Tuple Body**:
   `_tuple_items` handles both `,` and newline separators between entries (`_tuple_entry: choice($.map_entry, $._unnamed_entry)`). Because `_entry_sep` can match newlines that also delimit trailing structures, tree-sitter required explicit conflict declarations: `[$._tuple_items]` and `[$._entry_sep, $._tuple_items]`.
2. **Identifier-shaped Key vs. Unnamed Scalar**:
   `_value_scalar` in regex allows colons in scalars (`[^,()\[\]{}\n\r \t][^,()\[\]{}\n\r]*`), which would swallow `key:` or `key: value` as a scalar under longest-match rules before `map_entry` can match. By using `$.scalar` (from `scanner.c`) in `_unnamed_entry`, the external scanner checks lookahead for `:` after an identifier or string and yields to the grammar to produce `map_entry`.
3. **Nested Call vs. Scalar in Values**:
   In `scanner.c`, lookahead `(` previously only yielded `false` (allowing `call`) when `after_dollar` was set. In value positions, nested calls like `list(b, list(c))` or `glyph(triangle, ...)` were being lexed as `SCALAR` because `after_dollar` was false. Changed `scanner.c` to yield `false` on lookahead `(` for any identifier, allowing `call` to parse cleanly.
4. **Removal of Obsolete `{` Lookahead Probe**:
   `scanner.c` previously probed `{` after a word to avoid consuming keys of `key { ... }`. Since `{` is no longer valid in value positions or `map_entry`, that lookahead probe was removed.

## Step 2: Queries & Editors
- [x] Updated `queries/*.scm` in both crates:
  - `crates/tree-sitter-tomet/queries/highlights.scm`: replaced `(sequence ...)` with `(tuple ...)`
  - `crates/tree-sitter-tomet/queries/indents.scm`: removed `braced_map`, replaced `(sequence ...)` with `(tuple ...)`
  - `crates/tree-sitter-cotomet/queries/folds.scm`: replaced `(sequence)` with `(tuple)`
  - `crates/tree-sitter-cotomet/queries/indents.scm`: replaced `(sequence ...)` with `(tuple ...)`
- [x] Updated `editors/` query copies where needed:
  Verified `editors/*` are symlinks directly pointing to `crates/tree-sitter-*/queries/`.

## Step 3: Tree-Sitter Generate
- [x] Run `npx -y tree-sitter-cli@0.26.12 generate` under timeout 900 in both crates:
  - `crates/tree-sitter-tomet`: generated `src/parser.c`, `src/grammar.json`, `src/node-types.json`
  - `crates/tree-sitter-cotomet`: generated `src/parser.c`, `src/grammar.json`, `src/node-types.json`

Proof:
```
$ cd crates/tree-sitter-tomet && timeout 900 npx -y tree-sitter-cli@0.26.12 generate
Done in 2.0s
$ cd crates/tree-sitter-cotomet && timeout 900 npx -y tree-sitter-cli@0.26.12 generate
Done in 0.5s
```

## Step 4: Tests
- [x] Update unit tests in `tree-sitter-tomet` and `tree-sitter-cotomet`:
  - `crates/tree-sitter-tomet/src/lib.rs`: updated `parses_sequence_syntax_in_values` to test `tuple` and verify value-position `{}` errors; updated `parses_colon_less_braced_maps` to verify value-position `{}` errors.
  - `crates/tree-sitter-cotomet/src/lib.rs`: updated `a_file_of_entries_has_no_error` and `a_key_a_value_and_a_callee_are_told_apart_by_what_follows`, and verified value-position `{}` errors.
- [x] Run `cargo test -p tree-sitter-tomet -p tree-sitter-cotomet`:
  - `tree-sitter-cotomet`: 11 passed; 0 failed
  - `tree-sitter-tomet`: 70 passed; 0 failed
- [x] Run `cargo test -p tomet-tests --test corpus`:
  - 6 passed; 0 failed

Proof (`cargo test -p tree-sitter-tomet -p tree-sitter-cotomet`):
```
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src/lib.rs (target/debug/deps/tree_sitter_tomet-fc830e8e22972eda)
test result: ok. 70 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
```

Proof (`cargo test -p tomet-tests --test corpus`):
```
running 6 tests
test tree_sitter_image_meta_fixture_parses_cleanly ... ok
test corpus_is_not_empty ... ok
test known_ts_error_markers_are_not_vacuous ... ok
test tree_sitter_parses_every_fixture_to_a_document_root ... ok
test tree_sitter_has_no_unexpected_errors_anywhere_in_the_corpus ... ok
test every_fixture_parses_except_the_known_exceptions ... ok

test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
```

## Step 5: `KNOWN_TS_ERRORS` Cleanup & Final Verification
- [x] Remove resolved fixtures from `KNOWN_TS_ERRORS`:
  - Removed `syntax/tuple.tmt`, `default.config.tmt`, `syntax/nexus-declarations.tmt`, `test.config.tmt` and their comments.
- [x] Record count before and after:
  - Total entries before: 34 fixtures (22 drift/syntax + 12 nexus-invalid)
  - Total entries after: 30 fixtures (18 drift/syntax + 12 nexus-invalid)
- [x] Run full workspace verification:
  - `cargo fmt --all -- --check`
  - `cargo check --workspace --all-targets`
  - `cargo test -p tree-sitter-tomet -p tree-sitter-cotomet`
  - `cargo test --workspace --no-fail-fast`
- [x] Check `jj diff --stat --no-pager`

Proof (`cargo fmt --all -- --check`):
```
$ timeout 60 cargo fmt --all -- --check
Command exited with code 0.
```

Proof (`cargo check --workspace --all-targets`):
```
$ CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 timeout 600 cargo check --workspace --all-targets
    Finished `dev` profile [unoptimized] target(s) in 0.32s
Command exited with code 0.
```

Proof (`cargo test --workspace --no-fail-fast`):
```
$ CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 timeout 1500 cargo test --workspace --no-fail-fast
   Doc-tests tree_sitter_cotomet
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests tree_sitter_tomet
running 0 tests
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

Command exited with code 0.
```

Proof (`jj diff --stat --no-pager`):
```
crates/tree-sitter-cotomet/grammar.js           |     28 +-
crates/tree-sitter-cotomet/queries/folds.scm    |      2 +-
crates/tree-sitter-cotomet/queries/indents.scm  |      4 +-
crates/tree-sitter-cotomet/src/grammar.json     |     26 +-
crates/tree-sitter-cotomet/src/lib.rs           |     26 +-
crates/tree-sitter-cotomet/src/node-types.json  |    120 +-
crates/tree-sitter-cotomet/src/parser.c         |   2356 +-
crates/tree-sitter-tomet/grammar.js             |    118 +-
crates/tree-sitter-tomet/queries/highlights.scm |      4 +-
crates/tree-sitter-tomet/queries/indents.scm    |      5 +-
crates/tree-sitter-tomet/src/grammar.json       |    342 +-
crates/tree-sitter-tomet/src/lib.rs             |     77 +-
crates/tree-sitter-tomet/src/node-types.json    |    149 +-
crates/tree-sitter-tomet/src/parser.c           | 119403 ++++++++++++-----------
crates/tree-sitter-tomet/src/scanner.c          |     10 +-
tests/src/lib.rs                                |      7 -
```
