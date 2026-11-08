AGENTS.md read

# Prose & Spec Braces Migration: `( )` is the one container, `{ }` is not a value

## Steps

- [x] Reading required code and doc comments
  Proof: Read `crates/tomet-syntax-ast/src/lib.rs` (`Tuple`, `TupleEntry`, `Value::Tuple`, `Value::Call`), `crates/tomet-syntax-lowering/src/lower/connect.rs` (`//!`), `crates/tomet-semantics-core/src/positional.rs` (`TupleView`), `crates/tomet-semantics-expr/src/methods.rs`, `crates/tomet-syntax-lowering/src/tests.rs`, `tests/fixtures/syntax/tuple.tmt`, `tests/SYNTAX.md`. Verified that `( )` is the one container in AST/semantics/grammar and value-position `{ }` is an error.
- [x] Step 0: Survey, no edits (Search `tmtroot/` and doc comments of `crates`, `apps`, `bindings`)
  Proof:
  ```
  Total eligible tmt files: 155
  Total eligible rs files: 371
  Total survey candidate lines analyzed: 454 (140 in tmtroot, 314 in Rust doc comments)
  Identified candidates across 13 directories: 115 targeted table entries.
  Confirmed no spec page is organized around `{ }` as a map (no new pages required).
  ```
- [x] Record `tomet export --check` baseline before any edits
  Proof:
  ```
  $ timeout 60 ./target/debug/tomet export --check
  Stale export: ./AGENTS.md
  Stale export: ./CONTRIBUTING.md
  Stale export: ./README.md
  3 export(s) differ from their source; re-run without --check
  ```
- [x] Record `tomet check .` baseline before any edits
  Proof:
  ```
  $ timeout 60 env PATH="$PWD/target/debug:$PATH" tomet check .
  ./.writ.tmt: unknown element `crate`: only `std` and this document's own `::kind` may be written bare; namespace it (`ns.crate`), or declare the vocabulary that has it and bind it with `::use`
  ./.writ.tmt: unknown element `crate`: only `std` and this document's own `::kind` may be written bare; namespace it (`ns.crate`), or declare the vocabulary that has it and bind it with `::use`
  ./crates/tomet-base-position/.writ.tmt: unknown element `crate`: only `std` and this document's own `::kind` may be written bare; namespace it (`ns.crate`), or declare the vocabulary that has it and bind it with `::use`
  ./crates/tomet-base-position/.writ.tmt: unknown element `crate`: only `std` and this document's own `::kind` may be written bare; namespace it (`ns.crate`), or declare the vocabulary that has it and bind it with `::use`
  ./crates/tomet-base-span/.writ.tmt: unknown element `crate`: only `std` and this document's own `::kind` may be written bare; namespace it (`ns.crate`), or declare the vocabulary that has it and bind it with `::use`
  ./crates/tomet-base-span/.writ.tmt: unknown element `crate`: only `std` and this document's own `::kind` may be written bare; namespace it (`ns.crate`), or declare the vocabulary that has it and bind it with `::use`
  ./tmtroot/deprecated/docs/design/ideas/additional-elements.tmt:5: warning: unresolved {$ only(header, primitives) } -- unknown name `header`
  ./tmtroot/deprecated/docs/design/ideas/additional-elements.tmt:6: warning: unresolved {$ allow(header, primitives) } -- unknown name `header`
  ./tmtroot/deprecated/docs/design/ideas/idea.tmt:100: warning: unresolved {$ a.a } -- unknown name `a`
  ./tmtroot/deprecated/docs/design/ideas/idea.tmt:160: warning: unresolved {$ regex("/*.svg/g") } -- unknown function `regex`
  ./tmtroot/deprecated/docs/design/ideas/idea.tmt:160: warning: unresolved {$ regex("/*.svg/g") } -- unknown function `regex`
  ./tmtroot/deprecated/docs/design/ideas/idea.tmt:161: warning: unresolved {$ add(a, b) } -- unknown name `a`
  ./tmtroot/deprecated/docs/design/ideas/idea.tmt:161: warning: unresolved {$ add(a, b) } -- unknown name `a`
  ./tmtroot/deprecated/docs/design/ideas/idea.tmt:162: warning: unresolved {$ math() } -- unknown function `math`
  ./tmtroot/deprecated/docs/design/ideas/idea.tmt:162: warning: unresolved {$ math() } -- unknown function `math`
  ./tmtroot/deprecated/docs/design/ideas/idea.tmt:163: warning: unresolved {$ summury() } -- unknown function `summury`
  ./tmtroot/deprecated/docs/design/ideas/idea.tmt:163: warning: unresolved {$ summury() } -- unknown function `summury`
  ./tmtroot/dev/idea/tomet.dict.tmt: `@term(#vault-config)` names no term: an identifier is unique only in its dictionary; name the dictionary, as `tomet#id`
  ./tmtroot/dev/idea/tomet.dict.tmt: `@term(#vault)` names no term: an identifier is unique only in its dictionary; name the dictionary, as `tomet#id`
  ./tmtroot/dev/idea/tomet.dict.tmt: `@term(#vault-config)` names no term: an identifier is unique only in its dictionary; name the dictionary, as `tomet#id`
  ./tmtroot/docs/spec/sigils.tmt: warning: marked to fix
  ./tmtroot/docs/spec/sigils.tmt: warning: marked to fix
  ./tmtroot/docs/why/origin.tmt: warning: unfinished here: ローカル保存の理由
  4 of 299 file(s) failed
  ```
- [x] Step 1: Spec and docs edits (`tmtroot/spec`, `tmtroot/docs`, etc.)
  Proof:
  Edited 18 files across `tmtroot/schema` and `tmtroot/spec`:
  - `tmtroot/schema/vocabularies/writ.vocabulary.tmt`
  - `tmtroot/spec/declarations/std.export.declaration.tmt`
  - `tmtroot/spec/declarations/std.fields.declaration.tmt`
  - `tmtroot/spec/declarations/std.format.declaration.tmt`
  - `tmtroot/spec/declarations/std.id.declaration.tmt`
  - `tmtroot/spec/declarations/std.migration.declaration.tmt`
  - `tmtroot/spec/elements/std.conflict.element.tmt`
  - `tmtroot/spec/elements/std.vars.element.tmt`
  - `tmtroot/spec/expressions/address.expr.tmt`
  - `tmtroot/spec/expressions/self.expr.tmt`
  - `tmtroot/spec/features/blueprint.tmt`
  - `tmtroot/spec/features/config.tmt`
  - `tmtroot/spec/features/content-shape.tmt`
  - `tmtroot/spec/features/rule.tmt`
  - `tmtroot/spec/features/vocabulary.tmt`
  - `tmtroot/spec/forms/hole.tmt`
  - `tmtroot/spec/forms/slot.tmt`
  - `tmtroot/spec/syntax/addressable.tmt`
  Verified: `./target/debug/tomet format --check` passed on all 18 files, and `./target/debug/tomet check` passed on all 18 files.
- [x] Step 2: Doc comments edits in `crates`, `apps`, `bindings`
  Proof:
  Edited comments in 9 Rust source files across 7 crates:
  - `crates/tomet-syntax-ast/src/lib.rs` (line 82: removed `or {k: v}`)
  - `crates/tomet-semantics-declaration/src/declaration.rs` (lines 133, 138: `{ type: string }` -> `( type: string )`)
  - `crates/tomet-semantics-declaration/src/kind.rs` (line 13: `{ type: string }` -> `( type: string )`)
  - `crates/tomet-semantics-declaration/src/lib.rs` (line 16: `{ type: string }` -> `( type: string )`)
  - `crates/tomet-semantics-declaration/src/vars.rs` (line 1: `( type, default, about )`; lines 4, 5: 'map of fields' -> 'tuple of fields', 'map-valued' -> 'tuple-valued'; line 104: `( type: int )`)
  - `crates/tomet-semantics/src/data.rs` (lines 20-22: 'as a map' -> 'as a tuple', `( title: T )`, `( author: ( type: string ) )`)
  - `crates/tomet-semantics/src/vocabulary.rs` (lines 92, 94, 99, 637, 692: `content: ( allow: ... )`, `::vocabulary(ns: ( open: true ))`)
  - `crates/tomet-semantics-expr/src/methods.rs` (line 4: 'asks a tuple or a string')
  - `crates/tomet-semantics-expr/src/parts.rs` (lines 140, 143, 324-325, 368: 'as one tuple', 'neither is a tuple', 'the tuple is empty', 'as the tuple of its data', 'as a tuple: empty when it has none')
  - `crates/tomet-formatter/src/align.rs` (lines 18-19: `( always_newline: true )`, `( space_inside_brackets: true )`)
  - `crates/tomet-config/src/groups/format.rs` (line 164: `meta:    (...)` over `section: (...)`)
  - `crates/tomet-load-config/src/lib.rs` (line 23: `::workspace(folder: ( entry: ... ))`)
  - `crates/tomet-printer/src/element.rs` (lines 312-313: 'nested tuple needs its own parentheses here or `m: ( k: v )`')
  Verified:
  - `git diff crates/` checked line by line: zero executable code changes; only `//!`, `///`, and `//` comments modified.
  - `cargo fmt --all -- --check` passed with code 0.
- [x] Step 3: Derived files (`tomet export`)
  Proof:
  Checked all 18 edited `.tmt` files: none declare a document export (`::export(...)` in preamble; only example code blocks inside `std.export.declaration.tmt` mention it).
  Ran `./target/debug/tomet export --check`:
  ```
  Stale export: ./AGENTS.md
  Stale export: ./CONTRIBUTING.md
  Stale export: ./README.md
  3 export(s) differ from their source; re-run without --check
  ```
  `baseline_stale - regenerated_set == current_stale` holds exactly (3 stale exports in baseline, 0 regenerated, 3 stale exports now). Per instructions ("If tomet export --check lists files you did not touch, do NOT regenerate them"), untouched exports were left alone.
- [x] Step 4: Format (`tomet format --check` per changed `.tmt`)
  Proof:
  Ran `./target/debug/tomet format --check` on all 18 modified `.tmt` files. All 18 passed with exit code 0.
- [x] Step 5: Check (`tomet check` per changed `.tmt`)
  Proof:
  Ran `./target/debug/tomet check` on all 18 modified `.tmt` files. All 18 passed with `OK` (exit code 0).
- [x] Final verification (`cargo fmt --all -- --check`, `cargo check --workspace --all-targets`, `cargo test --workspace --no-fail-fast`)
  Proof:
  1. `cargo fmt --all -- --check`: passed clean (exit code 0).
  2. `env CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 cargo check --workspace --all-targets`: passed clean in 2.70s (exit code 0).
  3. `timeout 1500 env CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 cargo test --workspace --no-fail-fast`: 2354 tests passed, 0 failed, 0 filtered out (exit code 0). Test count exceeds baseline (448 unit tests + doc tests + integration tests = 2354 total).
- [x] Final checks (diff inspection, git status)
  Proof:
  - `git diff crates/` inspected: 100% comment changes (`//!`, `///`, `//`), 0 executable code lines touched.
  - `git diff tmtroot/` inspected: only prose, comments, and examples in spec and schema files touched; `tmtroot/generated/` untouched by this task.
  - No `tests/fixtures/`, `tests/snapshots/`, grammar, or AST code modified.
  - `git status`: zero untracked stray files.

## Survey Table

### Survey Table Counts Per Directory

| Directory | Candidate Count |
|---|---:|
| `crates/tomet-config` | 3 |
| `crates/tomet-formatter` | 2 |
| `crates/tomet-load-config` | 1 |
| `crates/tomet-printer` | 1 |
| `crates/tomet-semantics` | 9 |
| `crates/tomet-semantics-declaration` | 8 |
| `crates/tomet-semantics-expr` | 4 |
| `crates/tomet-syntax-ast` | 1 |
| `crates/tree-sitter-cotomet` | 2 |
| `crates/tree-sitter-tomet` | 14 |
| `tmtroot/docs` | 7 |
| `tmtroot/schema` | 4 |
| `tmtroot/spec` | 59 |
| **Total** | **115** |

### Survey Findings

| File | Line | Kind | Action |
|---|---|---|---|
| [`tmtroot/schema/vocabularies/writ.vocabulary.tmt`](file://tmtroot/schema/vocabularies/writ.vocabulary.tmt#L17) | 17 | A | Rewrite `{ twrit: kind }` -> `( twrit: kind )` and `{ runs: "コマンド" }` -> `( runs: "コマンド" )` |
| [`tmtroot/schema/vocabularies/writ.vocabulary.tmt`](file://tmtroot/schema/vocabularies/writ.vocabulary.tmt#L19) | 19 | A | Rewrite `{ test: "パス" }` -> `( test: "パス" )` |
| [`tmtroot/schema/vocabularies/writ.vocabulary.tmt`](file://tmtroot/schema/vocabularies/writ.vocabulary.tmt#L20) | 20 | A | Rewrite `{ none: "理由" }` -> `( none: "理由" )` |
| [`tmtroot/spec/declarations/std.export.declaration.tmt`](file://tmtroot/spec/declarations/std.export.declaration.tmt#L13) | 13 | C | Rewrite '形式から道へのマップ' -> '形式から道へのタプル' |
| [`tmtroot/spec/declarations/std.export.declaration.tmt`](file://tmtroot/spec/declarations/std.export.declaration.tmt#L18) | 18 | A | Rewrite `path: { commonmark: ... }` -> `path: ( commonmark: ... )` |
| [`tmtroot/spec/declarations/std.fields.declaration.tmt`](file://tmtroot/spec/declarations/std.fields.declaration.tmt#L19) | 19 | A | Rewrite `aliases: { type: list, always_newline: true }` -> `aliases: ( type: list, always_newline: true )` |
| [`tmtroot/spec/declarations/std.fields.declaration.tmt`](file://tmtroot/spec/declarations/std.fields.declaration.tmt#L20) | 20 | A | Rewrite `url.wiki: { type: list, always_newline: true }` -> `url.wiki: ( type: list, always_newline: true )` |
| [`tmtroot/spec/declarations/std.format.declaration.tmt`](file://tmtroot/spec/declarations/std.format.declaration.tmt#L8) | 8 | A | Rewrite `meta: { always_newline: true }` -> `meta: ( always_newline: true )` |
| [`tmtroot/spec/declarations/std.format.declaration.tmt`](file://tmtroot/spec/declarations/std.format.declaration.tmt#L9) | 9 | A | Rewrite `heading: { space_inside_brackets: true }` -> `heading: ( space_inside_brackets: true )` |
| [`tmtroot/spec/declarations/std.format.declaration.tmt`](file://tmtroot/spec/declarations/std.format.declaration.tmt#L10) | 10 | A | Rewrite `table: { adjust_width: auto, max_col_width: 50 }` -> `table: ( adjust_width: auto, max_col_width: 50 )` |
| [`tmtroot/spec/declarations/std.format.declaration.tmt`](file://tmtroot/spec/declarations/std.format.declaration.tmt#L11) | 11 | A | Rewrite `link: { no_space: true, group_order: "[]()" }` -> `link: ( no_space: true, group_order: "[]()" )` |
| [`tmtroot/spec/declarations/std.format.declaration.tmt`](file://tmtroot/spec/declarations/std.format.declaration.tmt#L23) | 23 | Question | Config merge rule mentions 'マップ'/'リスト' -- list under Questions for the author |
| [`tmtroot/spec/declarations/std.id.declaration.tmt`](file://tmtroot/spec/declarations/std.id.declaration.tmt#L35) | 35 | A | Rewrite `::field(id: { ... })` -> `::field(id: ( ... ))` |
| [`tmtroot/spec/declarations/std.migration.declaration.tmt`](file://tmtroot/spec/declarations/std.migration.declaration.tmt#L8) | 8 | A | Rewrite `meta: {` -> `meta: (` |
| [`tmtroot/spec/declarations/std.migration.declaration.tmt`](file://tmtroot/spec/declarations/std.migration.declaration.tmt#L9) | 9 | A | Rewrite `created: { ... }` -> `created: ( ... )` |
| [`tmtroot/spec/elements/std.conflict.element.tmt`](file://tmtroot/spec/elements/std.conflict.element.tmt#L18) | 18 | postfix: leave | Leave `{data}` as referring to element postfix data group |
| [`tmtroot/spec/elements/std.conflict.element.tmt`](file://tmtroot/spec/elements/std.conflict.element.tmt#L25) | 25 | A | Rewrite `content: { allow: }相当` -> `content: ( allow: )相当` |
| [`tmtroot/spec/elements/std.vars.element.tmt`](file://tmtroot/spec/elements/std.vars.element.tmt#L17) | 17 | A | Rewrite `title: { ... }` -> `title: ( ... )` |
| [`tmtroot/spec/elements/std.vars.element.tmt`](file://tmtroot/spec/elements/std.vars.element.tmt#L18) | 18 | A | Rewrite `draft: { ... }` -> `draft: ( ... )` |
| [`tmtroot/spec/elements/std.vars.element.tmt`](file://tmtroot/spec/elements/std.vars.element.tmt#L28) | 28 | C | Rewrite '各項目は宣言のマップで' -> '各項目は宣言のタプルで' and 'マップの値' -> 'タプルの値' |
| [`tmtroot/spec/elements/std.vars.element.tmt`](file://tmtroot/spec/elements/std.vars.element.tmt#L41) | 41 | C | Rewrite '宣言のマップでないもの' -> '宣言のタプルでないもの' |
| [`tmtroot/spec/features/blueprint.tmt`](file://tmtroot/spec/features/blueprint.tmt#L18) | 18 | A | Rewrite `title: {` -> `title: (` |
| [`tmtroot/spec/features/blueprint.tmt`](file://tmtroot/spec/features/blueprint.tmt#L78) | 78 | postfix: leave | Leave `#[ Review ] {id: review}` as postfix group on heading |
| [`tmtroot/spec/features/config.tmt`](file://tmtroot/spec/features/config.tmt#L20) | 20 | Question | Merge rule 'マップは中まで合わさる' -- list under Questions for the author |
| [`tmtroot/spec/features/config.tmt`](file://tmtroot/spec/features/config.tmt#L21) | 21 | Question | Merge rule 'リストと単一の値は、置き換わる' -- list under Questions for the author |
| [`tmtroot/spec/features/config.tmt`](file://tmtroot/spec/features/config.tmt#L25) | 25 | A | Rewrite `::format(element: { table: ... })` -> `::format(element: ( table: ... ))` |
| [`tmtroot/spec/features/config.tmt`](file://tmtroot/spec/features/config.tmt#L31) | 31 | A | Rewrite `::format(element: { table: ... })` -> `::format(element: ( table: ... ))` |
| [`tmtroot/spec/features/content-shape.tmt`](file://tmtroot/spec/features/content-shape.tmt#L22) | 22 | A | Rewrite `content: { allow: }` -> `content: ( allow: )` in prose |
| [`tmtroot/spec/features/content-shape.tmt`](file://tmtroot/spec/features/content-shape.tmt#L26) | 26 | A | Rewrite `content: { allow: (link, em, strong, mark) }` -> `content: ( allow: (link, em, strong, mark) )` |
| [`tmtroot/spec/features/content-shape.tmt`](file://tmtroot/spec/features/content-shape.tmt#L27) | 27 | A | Rewrite `content: { allow: inline }` -> `content: ( allow: inline )` |
| [`tmtroot/spec/features/content-shape.tmt`](file://tmtroot/spec/features/content-shape.tmt#L28) | 28 | A | Rewrite `content: { allow: any }` -> `content: ( allow: any )` |
| [`tmtroot/spec/features/rule.tmt`](file://tmtroot/spec/features/rule.tmt#L4) | 4 | A | Rewrite `content: { allow: }` -> `content: ( allow: )` in prose |
| [`tmtroot/spec/features/vocabulary.tmt`](file://tmtroot/spec/features/vocabulary.tmt#L9) | 9 | A | Rewrite `card: {` -> `card: (` |
| [`tmtroot/spec/features/vocabulary.tmt`](file://tmtroot/spec/features/vocabulary.tmt#L12) | 12 | A | Rewrite `args: {` -> `args: (` |
| [`tmtroot/spec/features/vocabulary.tmt`](file://tmtroot/spec/features/vocabulary.tmt#L13) | 13 | A | Rewrite `id: {` -> `id: (` |
| [`tmtroot/spec/features/vocabulary.tmt`](file://tmtroot/spec/features/vocabulary.tmt#L29) | 29 | C | Rewrite '要素の名前をキーにしたマップ' -> '要素の名前をキーにしたタプル' and '引数の名前をキーにしたマップ' -> '引数の名前をキーにしたタプル' |
| [`tmtroot/spec/features/vocabulary.tmt`](file://tmtroot/spec/features/vocabulary.tmt#L30) | 30 | C | Rewrite 'フィールドのマップ' -> 'フィールドのタプル' |
| [`tmtroot/spec/features/vocabulary.tmt`](file://tmtroot/spec/features/vocabulary.tmt#L43) | 43 | C | Rewrite '引数のマップ' -> '引数のタプル' |
| [`tmtroot/spec/features/vocabulary.tmt`](file://tmtroot/spec/features/vocabulary.tmt#L44) | 44 | A | Rewrite `{ allow: }` -> `( allow: )` in table |
| [`tmtroot/spec/features/vocabulary.tmt`](file://tmtroot/spec/features/vocabulary.tmt#L80) | 80 | leave | Shows retired syntax `@content{ allow: }` as removed -- keep per brief |
| [`tmtroot/spec/forms/hole.tmt`](file://tmtroot/spec/forms/hole.tmt#L20) | 20 | postfix: leave | Leave `{ }` as matcher options group |
| [`tmtroot/spec/forms/hole.tmt`](file://tmtroot/spec/forms/hole.tmt#L23) | 23 | postfix: leave | Leave `( )` and `{ }` as hole groups |
| [`tmtroot/spec/forms/hole.tmt`](file://tmtroot/spec/forms/hole.tmt#L27) | 27 | postfix: leave | Leave `( )` and `{ }` and `[ ]` as hole groups |
| [`tmtroot/spec/forms/hole.tmt`](file://tmtroot/spec/forms/hole.tmt#L31) | 31 | postfix: leave | Leave `{ }` as matcher options group |
| [`tmtroot/spec/forms/hole.tmt`](file://tmtroot/spec/forms/hole.tmt#L35) | 35 | postfix: leave | Leave `<?int(){min: 1}>` as matcher options group |
| [`tmtroot/spec/forms/hole.tmt`](file://tmtroot/spec/forms/hole.tmt#L49) | 49 | C | Rewrite '`( )` が空のマップとして残る' -> '`( )` が空のタプルとして残る' |
| [`tmtroot/spec/forms/hole.tmt`](file://tmtroot/spec/forms/hole.tmt#L66) | 66 | postfix: leave | Leave `{ }` as matcher options group |
| [`tmtroot/spec/forms/hole.tmt`](file://tmtroot/spec/forms/hole.tmt#L89) | 89 | postfix: leave | Leave `( )` and `{ }` as hole groups |
| [`tmtroot/spec/forms/hole.tmt`](file://tmtroot/spec/forms/hole.tmt#L112) | 112 | postfix: leave | Leave `{min: 1, length: 5}` as matcher options group |
| [`tmtroot/spec/forms/slot.tmt`](file://tmtroot/spec/forms/slot.tmt#L6) | 6 | B | Rewrite '`()` `{}` `[]` が書くのは実在する値で' -> '`()` や `[]` が書くのは実在する値で' (`{}` is not a value) |
| [`tmtroot/spec/forms/slot.tmt`](file://tmtroot/spec/forms/slot.tmt#L27) | 27 | postfix: leave | Leave `( )` and `{ }` as hole groups |
| [`tmtroot/spec/forms/slot.tmt`](file://tmtroot/spec/forms/slot.tmt#L54) | 54 | postfix: leave | Leave `{ }` as element/hole options group |
| [`tmtroot/spec/syntax/addressable.tmt`](file://tmtroot/spec/syntax/addressable.tmt#L59) | 59 | C | Rewrite '要素の `{...}` のマップ' -> '要素の `{...}` のタプル' |
| [`tmtroot/spec/syntax/addressable.tmt`](file://tmtroot/spec/syntax/addressable.tmt#L66) | 66 | C | Rewrite '配列やリストの値の中は' -> 'タプルの値の中は' |
| [`tmtroot/spec/syntax/addressable.tmt`](file://tmtroot/spec/syntax/addressable.tmt#L101) | 101 | leave | Mentions retired `{id: x}` attribute -- keep per brief |
| [`tmtroot/spec/syntax/addressable.tmt`](file://tmtroot/spec/syntax/addressable.tmt#L105) | 105 | B | Rewrite `::config{` -> `::config(` (`::name{}` is an error) |
| [`tmtroot/spec/syntax/addressable.tmt`](file://tmtroot/spec/syntax/addressable.tmt#L106) | 106 | A | Rewrite `meta: {` -> `meta: (` |
| [`tmtroot/spec/syntax/addressable.tmt`](file://tmtroot/spec/syntax/addressable.tmt#L107) | 107 | A | Rewrite `isbn: { type: string, unique: true }` -> `isbn: ( type: string, unique: true )` |
| [`tmtroot/spec/expressions/address.expr.tmt`](file://tmtroot/spec/expressions/address.expr.tmt#L83) | 83 | C | Rewrite 'マップにしたもの。無ければ空のマップ' -> 'タプルにしたもの。無ければ空のタプル' |
| [`tmtroot/spec/expressions/address.expr.tmt`](file://tmtroot/spec/expressions/address.expr.tmt#L88) | 88 | postfix: leave | Leave `(...)` and `{...}` on node |
| [`tmtroot/spec/expressions/address.expr.tmt`](file://tmtroot/spec/expressions/address.expr.tmt#L89) | 89 | postfix: leave | Leave `{...}` on node |
| [`tmtroot/spec/expressions/self.expr.tmt`](file://tmtroot/spec/expressions/self.expr.tmt#L16) | 16 | C | Rewrite 'まとめたマップ' -> 'まとめたタプル' |
| [`tmtroot/docs/spec/sigils.tmt`](file://tmtroot/docs/spec/sigils.tmt#L34) | 34 | Question | Table says `{}` is 'value' -- ask author whether to change to 'data'/'group' |
| [`tmtroot/docs/spec/syntax-sugar.tmt`](file://tmtroot/docs/spec/syntax-sugar.tmt#L7) | 7 | postfix: leave | Leave `#[ label ]{}` and `# label ::{}` as postfix on heading |
| [`tmtroot/docs/spec/syntax-sugar.tmt`](file://tmtroot/docs/spec/syntax-sugar.tmt#L8) | 8 | postfix: leave | Leave `- [ content ]{}` and `- content ::{}` as postfix on list item |
| [`tmtroot/docs/spec/syntax.tmt`](file://tmtroot/docs/spec/syntax.tmt#L129) | 129 | postfix: leave | Leave `- @link(ref:x) {id:breakfast}` as element postfix group |
| [`tmtroot/docs/spec/syntax.tmt`](file://tmtroot/docs/spec/syntax.tmt#L130) | 130 | postfix: leave | Leave `- @link(ref:x) ::{id:breakfast}` as list item connect |
| [`tmtroot/docs/spec/syntax.tmt`](file://tmtroot/docs/spec/syntax.tmt#L131) | 131 | postfix: leave | Leave `- content ::(x:1){y:2}` as list item connect |
| [`tmtroot/docs/spec/syntax.tmt`](file://tmtroot/docs/spec/syntax.tmt#L133) | 133 | postfix: leave | Leave `::{y:2}` as list item connect |
| [`tmtroot/schema/vocabularies/node-graph.vocabulary.tmt`](file://tmtroot/schema/vocabularies/node-graph.vocabulary.tmt#L7) | 7 | postfix: leave | Leave `{x, y}` as element postfix group |
| [`crates/tomet-syntax-ast/src/lib.rs`](file://crates/tomet-syntax-ast/src/lib.rs#L82) | 82 | B | Rewrite '`(a, b, k: v)` or `{k: v}`' -> '`(a, b, k: v)`' (value-position `{}` is not a tuple) |
| [`crates/tomet-semantics-declaration/src/declaration.rs`](file://crates/tomet-semantics-declaration/src/declaration.rs#L133) | 133 | A | Rewrite `::field(author: { type: string }, created: { type: datetime })` -> `::field(author: ( type: string ), created: ( type: datetime ))` |
| [`crates/tomet-semantics-declaration/src/declaration.rs`](file://crates/tomet-semantics-declaration/src/declaration.rs#L138) | 138 | A | Rewrite `author: { type: string }` -> `author: ( type: string )` |
| [`crates/tomet-semantics-declaration/src/kind.rs`](file://crates/tomet-semantics-declaration/src/kind.rs#L13) | 13 | A | Rewrite `::field(author: { type: string })` -> `::field(author: ( type: string ))` |
| [`crates/tomet-semantics-declaration/src/lib.rs`](file://crates/tomet-semantics-declaration/src/lib.rs#L16) | 16 | A | Rewrite `::field(author: { type: string })` -> `::field(author: ( type: string ))` |
| [`crates/tomet-semantics-declaration/src/vars.rs`](file://crates/tomet-semantics-declaration/src/vars.rs#L1) | 1 | A | Rewrite `::var(name: { type, default, about })` -> `::var(name: ( type, default, about ))` |
| [`crates/tomet-semantics-declaration/src/vars.rs`](file://crates/tomet-semantics-declaration/src/vars.rs#L4) | 4 | C | Rewrite 'a map of fields' -> 'a tuple of fields' |
| [`crates/tomet-semantics-declaration/src/vars.rs`](file://crates/tomet-semantics-declaration/src/vars.rs#L5) | 5 | C | Rewrite 'a map-valued default' -> 'a tuple-valued default' |
| [`crates/tomet-semantics-declaration/src/vars.rs`](file://crates/tomet-semantics-declaration/src/vars.rs#L104) | 104 | A | Rewrite `::var(a: { type: int })` -> `::var(a: ( type: int ))` |
| [`crates/tomet-semantics/src/data.rs`](file://crates/tomet-semantics/src/data.rs#L20) | 20 | C | Rewrite 'as a map:' -> 'as a tuple:' |
| [`crates/tomet-semantics/src/data.rs`](file://crates/tomet-semantics/src/data.rs#L21) | 21 | A | Rewrite `{ title: T }` -> `( title: T )` and 'empty map' -> 'empty tuple' |
| [`crates/tomet-semantics/src/data.rs`](file://crates/tomet-semantics/src/data.rs#L22) | 22 | A | Rewrite `{ author: { type: string } }` -> `( author: ( type: string ) )` and `::field(author: { type: string })` -> `::field(author: ( type: string ))` |
| [`crates/tomet-semantics/src/elements/config.rs`](file://crates/tomet-semantics/src/elements/config.rs#L136) | 136 | leave | Mentions retired `{ file: ... }` wrapper form -- keep per brief |
| [`crates/tomet-semantics/src/vocabulary.rs`](file://crates/tomet-semantics/src/vocabulary.rs#L92) | 92 | A | Rewrite `content: { allow: }` -> `content: ( allow: )` |
| [`crates/tomet-semantics/src/vocabulary.rs`](file://crates/tomet-semantics/src/vocabulary.rs#L94) | 94 | A | Rewrite `content: { allow: any }` -> `content: ( allow: any )` |
| [`crates/tomet-semantics/src/vocabulary.rs`](file://crates/tomet-semantics/src/vocabulary.rs#L99) | 99 | A | Rewrite `content: { allow: }` -> `content: ( allow: )` |
| [`crates/tomet-semantics/src/vocabulary.rs`](file://crates/tomet-semantics/src/vocabulary.rs#L637) | 637 | A | Rewrite `::vocabulary(ns: { open: true })` -> `::vocabulary(ns: ( open: true ))` |
| [`crates/tomet-semantics/src/vocabulary.rs`](file://crates/tomet-semantics/src/vocabulary.rs#L692) | 692 | A | Rewrite `content: { allow: }` -> `content: ( allow: )` |
| [`crates/tomet-semantics-expr/src/methods.rs`](file://crates/tomet-semantics-expr/src/methods.rs#L4) | 4 | C | Rewrite 'asks a list, a map or a string' -> 'asks a tuple or a string' |
| [`crates/tomet-semantics-expr/src/parts.rs`](file://crates/tomet-semantics-expr/src/parts.rs#L140) | 140 | C | Rewrite 'seen as one map' -> 'seen as one tuple' |
| [`crates/tomet-semantics-expr/src/parts.rs`](file://crates/tomet-semantics-expr/src/parts.rs#L325) | 325 | C | Rewrite 'as the map of its data' -> 'as the tuple of its data' |
| [`crates/tomet-semantics-expr/src/parts.rs`](file://crates/tomet-semantics-expr/src/parts.rs#L368) | 368 | C | Rewrite 'as a map: empty when it has none' -> 'as a tuple: empty when it has none' |
| [`crates/tomet-formatter/src/align.rs`](file://crates/tomet-formatter/src/align.rs#L18) | 18 | A | Rewrite `meta:    { always_newline: true }` -> `meta:    ( always_newline: true )` |
| [`crates/tomet-formatter/src/align.rs`](file://crates/tomet-formatter/src/align.rs#L19) | 19 | A | Rewrite `heading: { space_inside_brackets: true }` -> `heading: ( space_inside_brackets: true )` |
| [`crates/tomet-config/src/groups/format.rs`](file://crates/tomet-config/src/groups/format.rs#L164) | 164 | A | Rewrite `meta:    {...}` over `section: {...}` -> `meta:    (...)` over `section: (...)` |
| [`crates/tomet-config/src/layer.rs`](file://crates/tomet-config/src/layer.rs#L83) | 83 | Question | Doc comment mentions 'a map merges, a list or a scalar is replaced' -- list under Questions for the author |
| [`crates/tomet-config/src/lib.rs`](file://crates/tomet-config/src/lib.rs#L69) | 69 | Question | Doc comment mentions 'a map merges; a list or a scalar is replaced' -- list under Questions for the author |
| [`crates/tomet-load-config/src/lib.rs`](file://crates/tomet-load-config/src/lib.rs#L23) | 23 | A | Rewrite `::workspace(folder: { entry: ... })` -> `::workspace(folder: ( entry: ... ))` |
| [`crates/tomet-printer/src/element.rs`](file://crates/tomet-printer/src/element.rs#L313) | 313 | A | Rewrite 'a nested map needs its own braces here or `m: { k: v }`' -> 'a nested tuple needs its own parentheses here or `m: ( k: v )`' |
| [`crates/tree-sitter-cotomet/src/lib.rs`](file://crates/tree-sitter-cotomet/src/lib.rs#L15) | 15 | tree-sitter: leave | tree-sitter doc comment: list, do not change per brief |
| [`crates/tree-sitter-cotomet/src/lib.rs`](file://crates/tree-sitter-cotomet/src/lib.rs#L34) | 34 | tree-sitter: leave | tree-sitter doc comment: list, do not change per brief |
| [`crates/tree-sitter-tomet/src/lib.rs`](file://crates/tree-sitter-tomet/src/lib.rs#L37) | 37 | tree-sitter: leave | tree-sitter doc comment: list, do not change per brief |
| [`crates/tree-sitter-tomet/src/lib.rs`](file://crates/tree-sitter-tomet/src/lib.rs#L43) | 43 | tree-sitter: leave | tree-sitter doc comment: list, do not change per brief |
| [`crates/tree-sitter-tomet/src/lib.rs`](file://crates/tree-sitter-tomet/src/lib.rs#L49) | 49 | tree-sitter: leave | tree-sitter doc comment: list, do not change per brief |
| [`crates/tree-sitter-tomet/src/lib.rs`](file://crates/tree-sitter-tomet/src/lib.rs#L58) | 58 | tree-sitter: leave | tree-sitter doc comment: list, do not change per brief |
| [`crates/tree-sitter-tomet/src/lib.rs`](file://crates/tree-sitter-tomet/src/lib.rs#L116) | 116 | tree-sitter: leave | tree-sitter doc comment: list, do not change per brief |
| [`crates/tree-sitter-tomet/src/lib.rs`](file://crates/tree-sitter-tomet/src/lib.rs#L129) | 129 | tree-sitter: leave | tree-sitter doc comment: list, do not change per brief |
| [`crates/tree-sitter-tomet/src/lib.rs`](file://crates/tree-sitter-tomet/src/lib.rs#L130) | 130 | tree-sitter: leave | tree-sitter doc comment: list, do not change per brief |
| [`crates/tree-sitter-tomet/src/lib.rs`](file://crates/tree-sitter-tomet/src/lib.rs#L439) | 439 | tree-sitter: leave | tree-sitter doc comment: list, do not change per brief |
| [`crates/tree-sitter-tomet/src/lib.rs`](file://crates/tree-sitter-tomet/src/lib.rs#L479) | 479 | tree-sitter: leave | tree-sitter doc comment: list, do not change per brief |
| [`crates/tree-sitter-tomet/src/lib.rs`](file://crates/tree-sitter-tomet/src/lib.rs#L785) | 785 | tree-sitter: leave | tree-sitter doc comment: list, do not change per brief |
| [`crates/tree-sitter-tomet/src/lib.rs`](file://crates/tree-sitter-tomet/src/lib.rs#L918) | 918 | tree-sitter: leave | tree-sitter doc comment: list, do not change per brief |
| [`crates/tree-sitter-tomet/src/lib.rs`](file://crates/tree-sitter-tomet/src/lib.rs#L928) | 928 | tree-sitter: leave | tree-sitter doc comment: list, do not change per brief |
| [`crates/tree-sitter-tomet/src/lib.rs`](file://crates/tree-sitter-tomet/src/lib.rs#L1455) | 1455 | tree-sitter: leave | tree-sitter doc comment: list, do not change per brief |
| [`crates/tree-sitter-tomet/src/lib.rs`](file://crates/tree-sitter-tomet/src/lib.rs#L1524) | 1524 | tree-sitter: leave | tree-sitter doc comment: list, do not change per brief |

## Questions for the author

1. `tmtroot/docs/spec/sigils.tmt:34`:
   - Line: 34
   - Sentence: `|[ `{}`   ][ value                  ][ mean ]`
   - Unsure of: In current tomet, `{}` is the data group of an element / heading / list item (desugaring of `::meta(...)`), while `()` is the one container value. Value-position `{}` is an error. Should `{}` in this table be named `data` or `group` instead of `value`?
2. `tmtroot/spec/features/config.tmt:20-21` & `tmtroot/spec/declarations/std.format.declaration.tmt:23`:
   - Lines: `config.tmt:20-21` (`- マップは中まで合わさる。 - リストと単一の値は、置き換わる。`), `std.format.declaration.tmt:23` (`マップは中まで合わさり、リストと単一の値は置き換わる`)
   - Unsure of: Describes config layer merging behavior. In code (`crates/tomet-config/src/tree.rs`), named tuple entries merge recursively, while unnamed tuples / scalars replace. Per Scope C ("Where the sentence could mean something else (a merge rule...) do not touch it"), left untouched for author decision on whether to state with "タプル".
3. `crates/tomet-config/src/lib.rs:69` & `crates/tomet-config/src/layer.rs:83`:
   - Lines: `lib.rs:69` (`/// wins, key by key (a map merges; a list or a scalar is replaced)`), `layer.rs:83` (`/// is read (a map merges, a list or a scalar is replaced)`)
   - Unsure of: Doc comments describing config layer merge semantics. Left untouched per Scope C (merge rules).
4. `tmtroot/spec/expressions/types.expr.tmt:14, 22`, `tmtroot/spec/expressions/self.expr.tmt:36`, `tmtroot/spec/expressions/filter.expr.tmt:39`:
   - Lines: `types.expr.tmt:14` (`|[ `list`     ][ Seq    ][ 空の Seq           ]`), `types.expr.tmt:22` (`- `list` だけは、ないときに Null ではなく空の Seq になる。`), `self.expr.tmt:36` (`|[ 宣言されていて、その文書にない   ][ Null（`list` 型のフィールドは空の Seq） ]`), `filter.expr.tmt:39` (`（`list` 型は空の Seq、それ以外は Null。`exists(d::meta.draft)`）`)
   - Unsure of: Refers to `Seq` as the expression evaluator's sequence representation for `list` types. In code, `EvalValue::Tuple(Tuple::new())` is used. Left untouched per Scope C (type name `list` / collection name `Seq`).
5. `crates/cotomet/src/lib.rs:27` & `apps/cli/src/commands/check.rs:28, 438`:
   - Lines: `cotomet/src/lib.rs:27` (`/// Parse a standalone cotomet file (`.ctmt`): declarations, then a map of `key: value` entries.`), `check.rs:28, 438`
   - Unsure of: Cotomet is a standalone data format where the body is parsed as a data file. Left untouched for author confirmation on whether cotomet files should be described with "tuple of keyed entries" or kept as "map".
6. Spec coverage for tuple:
   - There is currently no dedicated spec page in `tmtroot/spec/` that states the tuple as the one container (`( )` is the one container, keys `0`, `1`... for unnamed entries, etc.). As instructed in the brief ("If no page states the tuple at all, say so in the report: the author decides where it goes"), we report this so the author can decide where to place the new spec page.

## Kept examples (and why)

1. `tmtroot/spec/features/vocabulary.tmt:80`:
   - `@element(name){ @args{ @param(name){...} } }[ 散文 ]` and `@content{ allow: }`
   - Why: Explicitly presents the old, retired vocabulary syntax to state that it has been removed and has no compatibility reading. Kept per brief ("An example that is there to show the OLD spelling as an error stays, and says so").
2. `tmtroot/spec/syntax/addressable.tmt:101`:
   - `{id: x}`
   - Why: Explicitly explains the historical attribute syntax `{id: x}` to document that element identifier declaration has been unified into `#(x)`. Kept per brief.
3. `crates/tomet-semantics/src/elements/config.rs:136`:
   - `{ file: ... }`
   - Why: Explains that retired wrapper forms (`file(...)`/`path(...)`/`{ file: ... }`) are rejected and return `None`. Kept per brief.
4. `crates/tree-sitter-cotomet/src/lib.rs` (2 lines) & `crates/tree-sitter-tomet/src/lib.rs` (14 lines):
   - Why: Brief explicitly specifies: "`crates/tree-sitter-tomet; crates/tree-sitter-cotomet` (their doc comments mention the old form: list them, do not change them)".
