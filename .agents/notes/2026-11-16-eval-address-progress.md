AGENTS.md read

# Task: Tuple, step 3 -- the evaluator's one tuple, and addresses by position

## Core Rules and Constraints
- Environment variables: `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0` for all cargo commands.
- Do not touch: `crates/tree-sitter-tomet`, `crates/tree-sitter-cotomet`, existing files under `tests/fixtures/`, anything under `tmtroot/`, `.writ.tmt` files, `.md` files (doc files), `packages/*`.
- No commit, no `jj describe`, no `jj new`, no push, no `git` writes.
- Never add `_ =>` to make a `match` compile, never delete a test or mark it ignored.
- Tick a step only after running the command that proves it, and paste the last lines of its real output under the step.

## Steps

- [x] Step 1: Generic `Tuple<V = Value>` and `TupleEntry<V = Value>` in `crates/tomet-syntax-ast`.
```
   Compiling tomet-ast v0.5.0 (/home/tefla/projects/develop/project-tomet/tomet/.agents/workspaces/one-container/crates/tomet-syntax-ast)
    Finished `test` profile [unoptimized] target(s) in 0.53s
     Running unittests src/lib.rs (target/debug/deps/tomet_ast-fb28cbd17a0046ec)

running 8 tests
test tests::a_type_seat_displays_as_the_source_it_reads_back_from ... ok
test tests::a_key_left_out_and_the_same_key_written_are_one_tuple ... ok
test tests::test_element_view_and_list_item_as_element_view ... ok
test tests::generic_tuple_preserves_rules_for_custom_type ... ok
test tests::the_same_keys_in_another_order_are_another_tuple ... ok
test tests::removing_an_unnamed_entry_moves_the_ones_after_it ... ok
test tests::a_key_is_held_once ... ok
test tests::test_tuple_and_value_json_serde ... ok

test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests tomet_ast

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```
- [x] Step 2: Aliases instead of rewriting: replace `bind_positional` and `Declaration::bound_args` with a view over a tuple and signature in `crates/tomet-semantics`.
```
     Running unittests src/lib.rs (target/debug/deps/tomet_semantics-9326ca4a1e7649a8)
test result: ok. 143 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

     Running unittests src/lib.rs (target/debug/deps/tomet_semantics_core-9f61d5b5fa5a5361)
test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```
- [x] Step 3: One `EvalValue::Tuple`, eliminate `EvalValue::Seq` and `EvalValue::Map`, update `EvalValue::Call(String, Tuple<EvalValue>)`.
```
     Running unittests src/lib.rs (target/debug/deps/tomet_expr-b2d84cdeda2b7dbc)
test result: ok. 150 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
     Running unittests src/lib.rs (target/debug/deps/tomet_vault-3a1062cfeac5ce3a)
test result: ok. 29 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running unittests src/lib.rs (target/debug/deps/tomet_load-0774a580c11f8af5)
test result: ok. 28 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running unittests src/lib.rs (target/debug/deps/tomet_transform-a874a585a96ada0a)
test result: ok. 75 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```
- [x] Step 4: Members: `x.key` on tuple, `x.0 == x` on scalars, fix lexing of `use.0.0` / `x.1.0` so digit after dot is a member, not float.
```
     Running unittests src/lib.rs (target/debug/deps/tomet_ast_parser-789a9f4c330df30b)
test tests::interpolation::parses_dotted_member_chain_with_digits ... ok
test result: ok. 354 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

     Running unittests src/lib.rs (target/debug/deps/tomet_expr-262bfcb52f82babe)
test tests::scalar_zero_member_and_tuple_operations ... ok
```
- [x] Step 5: Methods: update ordered ops on positional entries, `has` for key membership, `get` with position/name, search repository.
```
     Running unittests src/lib.rs (target/debug/deps/tomet_expr-262bfcb52f82babe)
test tests::scalar_zero_member_and_tuple_operations ... ok
test tests::maps_and_strings_have_their_own_contains_and_get ... ok
```
- [x] Step 6: Addresses: `#x.0` and `#x.target` through the view of step 2.
```
     Running unittests src/lib.rs (target/debug/deps/tomet_semantics-9326ca4a1e7649a8)
test positional::tests::signature_alias_and_position_with_and_without_vocabulary_and_invariance ... ok
test result: ok. 144 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running unittests src/lib.rs (target/debug/deps/tomet_expr-262bfcb52f82babe)
test tests::addresses_by_position_and_signature_alias ... ok
test result: ok. 151 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
```
- [x] Step 7: Verification and clean-up (cargo fmt, cargo check, cargo test, grep checks, git status check, etc.).
```
$ cargo fmt --all -- --check
(exited 0)
$ CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 cargo check --workspace --all-targets
(exited 0)
$ CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 cargo test --workspace
(exited 0: all tests across the workspace passed)
$ CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 cargo test -p tomet-tests
(exited 0: 73 passed in corpus, roundtrip, spelling, snapshot, vocabulary)
$ just writ-check
(exited 0)
$ git grep -E "EvalValue::(Seq|Map)\b|fn bind_positional|fn bound_args"
(0 occurrences in Rust codebase)
$ git status --porcelain -- tests/ref
(0 changes in tests/ref)
```
