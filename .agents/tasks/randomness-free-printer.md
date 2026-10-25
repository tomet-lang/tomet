# randomness-free-printer: stop the printer pulling in an RNG on wasm

Status: not started (recorded only). Origin 2026-10-29 from tomet-cms.
Nothing is implemented. Do not start without the user's go-ahead.

## Problem

`tomet-cms` builds for `wasm32-unknown-unknown` with `tomet` features `parser`, `printer`, `merge`.
That build only succeeds with `getrandom = { version = "0.2", features = ["js"] }` added by the
consumer. The dependency path is:

    tomet-printer -> tomet-field-utils -> nanoid -> rand 0.8 -> getrandom 0.2

`nanoid` is used by `tomet_field_utils::generate_id_for_field`
(`crates/tomet-format-field-utils/src/lib.rs`), which only `ensure_document_id_with_config`
(`crates/tomet-format-printer/src/id.rs`) calls; the only non-test caller of that is
`apps/cli/src/commands/convert.rs`. Rendering (`document_to_tm`, `document_to_tm_with_config`)
needs no randomness. `tomet-merge` and `tomet-workspace` also depend on `tomet-printer`, so any
consumer of `merge` inherits the RNG dependency.

`bindings/js/Cargo.toml` already carries the same workaround (`getrandom 0.2` + `js`), and the
workspace `uuid` dependency enables `js`. So the current state is consistent and works in browsers.

## Why it may be worth fixing later

- The `js` feature requires `wasm-bindgen`/a browser-like host. Non-browser wasm (WASI, other
  runtimes) cannot use the crate without extra work.
- Every wasm consumer of `printer` or `merge` must know about the workaround.
- It is a randomness dependency in a crate whose main job (rendering text) is deterministic.

## Options (decide before starting)

- **A. Feature split.** Add an `id-generation` feature (default on) to `tomet-field-utils` and
  `tomet-printer`, gating `nanoid`/`uuid` and `generate_id_for_field` /
  `ensure_document_id_with_config`. Every workspace dependency on `tomet-printer`
  (including through `tomet-merge`, `tomet-workspace`, `tomet`'s `printer` feature) must then be
  `default-features = false`, with the CLI and the bindings that need ID generation enabling it
  explicitly. Touches the public feature set of `tomet`; check every consumer.
- **B. Inject the RNG.** `generate_id_for_field` takes the randomness source (or a ready-made
  id generator) from its caller. Changes a public signature; the CLI supplies the real source.
- **C. Keep the workaround.** Consumers add `getrandom/js` themselves. No change here.

Recommendation as of 2026-10-29: stay on C. Browser is the only wasm target in use
(`tomet-wiki`). Revisit when a non-browser wasm target is wanted, then take A.

## If A is chosen: steps

1. [ ] Work in a dedicated jj workspace (`.agents/workspaces/randomness-free-printer`), after
   checking `jj status` for other people's in-progress changes in the default workspace.
2. [ ] Gate `nanoid`/`uuid` and `generate_id_for_field` behind `id-generation` in
   `tomet-field-utils`; keep `is_valid_id_format` and the timestamp helpers ungated.
3. [ ] Gate `ensure_document_id_with_config` (and its use of `generate_id_for_field`) in
   `tomet-printer` behind the same feature, forwarding it to `tomet-field-utils`.
4. [ ] Make the workspace dependency on `tomet-printer` `default-features = false`; enable
   `id-generation` where it is used: `apps/cli`, and any binding that calls
   `ensure_document_id_with_config`. Audit all other consumers with
   `cargo tree -e features -i tomet-printer`.
5. [ ] Add a `tomet` crate feature that forwards `id-generation`, so a downstream crate can opt in.
6. [ ] Verify: `cargo check --target wasm32-unknown-unknown` for a crate depending on
   `tomet` with `parser`, `printer`, `merge` and **no** `getrandom` dependency;
   `cargo tree -i getrandom` shows no 0.2 in that build; the full workspace tests still pass;
   the CLI still generates IDs.
7. [ ] Tell `tomet-cms` it can drop its `getrandom` dependency (take the new `tomet` with
   `cargo update -p tomet`; do not use `[patch]`).

## Out of scope

Changing how IDs are generated, replacing `nanoid` with another algorithm, or touching
`bindings/js`'s own `getrandom` line before step 6 is verified.

## Log

- 2026-10-29: Task recorded only. Findings from reading the dependency path and `bindings/js`.
