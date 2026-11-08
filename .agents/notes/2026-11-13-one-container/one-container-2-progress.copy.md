# Task Progress: one-container stage 2 -- one container in the AST

## Plan & Steps

- [x] Step 0: Initial analysis, inventory of AST types and crate dependencies
- [x] Step 1: Layer 2 -- `tomet-syntax-ast`: Define `Container`, `ContainerEntry`, update `Value::Container`, `Data.args`, `Group` conversions, serde implementation
- [x] Step 2: Layer 2 -- `tomet-syntax-tree`: Update tree walking, traversal, remove `as_seq`/`as_map`
- [x] Step 3: Layer 2 -- `tomet-syntax-ast-parser` and `tomet-syntax-lowering`:
  - `lower_paren_value` produces `Container` without unboxing `(x)`
  - `Data.args` is `Container`
  - `resolve_call` produces one-entry container for non-primitive calls
  - Remove `POSITIONAL_ENTRY_KEY` from lowering
  - Update `connect.rs` merge logic for containers
  - Update unit tests in `tomet-syntax-ast-parser` (353 passed) and `tomet-lowering` (10 passed)
- [x] Step 4: Layer 2 -- `serde_cotomet`: `de.rs`, `print.rs`, `deserialize_enum`, `deserialize_seq`, `deserialize_map`, `deserialize_any` (21 passed)
- [x] Step 5: Layer 3 -- `cotomet`: `find_not_data` (13 passed)
- [x] Step 6: Layer 3 -- `tomet-semantics-core`:
  - `positional.rs`: `bind_positional` over `Container`
  - `parse.rs`: `parse_type_spec`
  - `admits.rs`: list/map type admission and wording ("a list", "a map", "a list with named entries")
  - Remove `POSITIONAL_ENTRY_KEY` from semantics-core (16 tests passed)
- [x] Step 7: Layer 3 -- `tomet-semantics-declaration`: Update positional argument binding and declaration reading (60 tests passed)
- [x] Step 8: Layer 3 -- `tomet-semantics`: Update positional binding callers, `value_to_json` (143 tests passed)
- [x] Step 9: Layer 3 -- `tomet-semantics-expr` & `tomet-semantics-eval`:
  - Remove `EvalValue::Call`, bridge AST `Container` to/from `EvalValue::Seq` / `EvalValue::Map` (147 tests in tomet-expr passed, 19 tests in cotomet-eval passed)
- [x] Step 10: Layer 4 -- `tomet-validator-*` crates: Update rule decoding, schema validation, blueprints (126 tests passed)
- [x] Step 11: Layer 5 -- `tomet-config*`: Update config extraction and deserialization (38 tests passed)
- [ ] Step 12: Layer 6 -- `tomet-printer`:
  - Implement container printing rules
  - Check `tomet-formatter`
- [ ] Step 13: Layer 6 -- `tomet-convert-*` crates (`html`, `markdown`, `pandoc`, `typst`, `standalone`)
- [ ] Step 14: Layer 6 -- `tomet-transform`, `tomet-search`, `tomet-merge`, `tomet-deps`, `tomet-stats`, `tomet-spec`, `tomet-vault`, `tomet-load*`
- [ ] Step 15: Layer 7 & 8 -- `tomet-workspace*`, `tomet`, `apps/*` (LSP, CLI), `bindings/*` (`js/index.d.ts`, `python`, `java`), `benches`
- [ ] Step 16: Verification & Tests:
  - `tests/src` updates
  - Done criterion 1 regex check
  - `cargo check --workspace --all-targets` and `cargo test --workspace`
  - Snapshot updates & review: `.html`, `.md`, `.typ`, `.pandoc.json`, `.printed.tmt`, `.pandoc.roundtrip.tmt`, `SYNTAX.md`
  - Verify doc comments and single source of truth rules

## Questions & Observations

- Tree-sitter comments: `crates/tree-sitter-tomet` contains comments mentioning `Value::Call` and `Value::Seq`. Rule forbids touching `crates/tree-sitter-tomet`.
- Expressions observing mixed containers: Any expression reading node data or arguments where a node carries mixed positional and named parameters (e.g. `@foo(10, k: "v")`), accessed via `node.0`, `node.1`, `node.k`, or by iterating over the map (`.keys()`, `.map()`, `.filter()`). Unnamed entries are assigned `"0"`, `"1"` string keys.
