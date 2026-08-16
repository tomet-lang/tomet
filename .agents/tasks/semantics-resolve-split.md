# Task: Split TypedMark's own semantics into `typedmark-semantics` + `typedmark-resolve`

## Goal

Right now, "what does this `@xxx`/`<T>` element officially mean" is decided
independently (and partly duplicated) inside `typedmark-renderer` and
`typedmark-markdown`, with one piece of it (`infer_at_kind`) living in
`typedmark-ast` even though it's logic, not a type. Split this into two
new crates so `typedmark-ast` goes back to being pure structural types,
and TypedMark's own built-in vocabulary (`@meta`, `@config`, `@links`,
url/file/ref inference, and the not-yet-implemented `@settings(file:...)`
/ future `@import`) has one canonical home instead of N per-consumer
reimplementations.

## Context / decisions made in conversation

- **Two different jobs were hiding under one idea**, matching two
  distinct compiler-pipeline phases:
  - *semantic analysis* (pure function over an already-parsed
    `Document`, no I/O): classifying what an element means —
    `url`/`file`/`ref` inference, recognizing `@meta`/`@config`/`@links`
    etc. → new crate **`typedmark-semantics`**.
  - *preprocessor/linker*-like (I/O, file resolution, recursively
    invokes the parser): C's `#include` is the closest analogy —
    `@settings(file:...)`, future `@import` → new crate
    **`typedmark-resolve`**.
- **Existing duplication to remove**: `typedmark-renderer/src/lib.rs`'s
  `element_kind()` (~line 185) and `typedmark-markdown/src/export.rs`'s
  `element_kind()` (~line 85) are near-identical: both switch on `Sigil`
  and both call `typedmark_ast::infer_at_kind` for the `Sigil::At(None)`
  case. Both also separately hardcode what `"meta"`/`"config"`/`"links"`/
  `"embed"`/`"hr"`/`"em"`/`"strong"`/`"mark"`/`"codeblock"`/
  `"blockquote"` mean (`render_element`'s `match kind.as_str()` in
  renderer, the equivalent match in markdown export). This is real,
  already-existing duplication, not hypothetical future risk.
- **`typedmark-ast::INFERRED_AT_KEYS`/`infer_at_kind`** (currently
  `crates/typedmark-ast/src/lib.rs:253-282`, plus its 3 tests) is the
  piece to delete from `typedmark-ast` and move into `typedmark-semantics`
  wholesale (doc comments and tests move with it).
- **`@config(format:...)` is the one exception that stays in
  `typedmark-parser`**, not either new crate. `document.rs`'s
  `is_config`/`config_format_update` (~line 921) checks the literal name
  `"config"` because it changes how subsequent `{value}` bodies get
  parsed (embedded-format switching) — this has to happen mid-parse, it
  can't be deferred to a post-parse layer. Don't move this.
- **`@settings(file:...)` is net-new, not a refactor.** It already
  appears as syntax across several docs (`docs/ja/name.tm`,
  `docs/ja/specifications/*.tm`, `docs/docs.settings.tm` itself is the
  referenced-schema shape) but `grep -rn "settings"` across
  `typedmark-parser`/`typedmark-renderer`/`typedmark-validator` returns
  nothing — nobody reads it today. `typedmark-validator` is still the
  bare `cargo new` stub (`add(left, right)` + its test).
- **`@import` has no spec yet** — syntax/semantics haven't been designed
  in this conversation. Don't guess at it; leave a documented seam/TODO
  in `typedmark-resolve` instead of implementing something invented.
- **Why `typedmark-parser` can't own file resolution**:
  `docs/develop/architecture.md`'s documented "Deterministic Static
  Parser Boundary" — `typedmark-parser` is explicitly required to do
  zero I/O / external file resolution. This isn't a style preference;
  it's an existing constraint already written down, and it's the reason
  `@settings(file:...)`/`@import` need a separate crate rather than
  living in `parser`.
- **Layering**:
  `typedmark-ast` (pure types) → `typedmark-parser` (text → AST, only
  parse-time-relevant rules) → `typedmark-semantics` (I/O-free meaning
  classification, depends only on `typedmark-ast`) → consumers
  (`typedmark-renderer`, `typedmark-markdown`, `typedmark-validator`,
  CLI, LSP). `typedmark-resolve` depends on `typedmark-parser` (to
  recursively parse referenced/included files) and probably
  `typedmark-semantics` (to recognize which elements are `@settings`/
  `@import` in the first place) — confirm the exact dependency in Step 1,
  don't assume.
- **Overlaps with `.agents/tasks/rename-input-area.md`**: that task
  (`input`→`args`, `area`→`content`) touches the same `Element` struct in
  `typedmark-ast` and the same match arms in `typedmark-renderer`/
  `typedmark-markdown/export.rs` that this task is moving. If both are
  in flight, do the rename first (less churn — this task's moved code
  then gets written once with final field names) or explicitly rebase
  whichever lands second. Don't let the two files silently drift apart.

## Steps

- [ ] **Step 1**: Design pass, no code yet. Nail down before writing
  anything:
  - `typedmark-semantics`'s public API — likely an `ElementKind` enum
    (replacing today's stringly-typed `kind: String` from each crate's
    local `element_kind()`) plus a `classify(el: &Element) -> ElementKind`
    function. Decide whether it also owns "does this kind mean
    invisible/no-output" (`meta`/`config`) as a property of the enum, or
    whether that's still each consumer's call — see the note below on
    `@meta` in Step 4.
  - `typedmark-resolve`'s public API for `@settings(file:...)` — what it
    returns (parsed `@settings{}` schema `Value`? something typed?), how
    errors (missing file, parse failure in the referenced file) surface
    to callers.
  - Confirm `typedmark-resolve`'s actual dependency on
    `typedmark-semantics` (needed only if resolving requires classifying
    elements by kind first — plausible it can just pattern-match
    `Sigil::At(Some("settings"))` directly without going through full
    classification).
- [ ] **Step 2**: Create `typedmark-semantics` crate
  (`cargo new --lib crates/typedmark-semantics`), add it to root
  `Cargo.toml`'s `members` (under `#[ Library ]`) and
  `[workspace.dependencies]`. Move `INFERRED_AT_KEYS`/`infer_at_kind`
  and their doc comments/tests out of `crates/typedmark-ast/src/lib.rs`
  into it verbatim first (pure move, no behavior change), confirm
  `cargo test -p typedmark-ast -p typedmark-semantics` passes.
- [ ] **Step 3**: Add the `ElementKind` classification designed in Step 1
  to `typedmark-semantics`, folding in what's currently duplicated in
  `typedmark-renderer`'s and `typedmark-markdown/export.rs`'s local
  `element_kind()` functions (the `Sigil::Type`/`Sigil::At(Some)`/
  `Sigil::At(None)`/`Sigil::Bare` switch, plus the `INFERRED_AT_KEYS`
  call for the last case).
- [ ] **Step 4**: Wire `typedmark-renderer` and
  `typedmark-markdown/src/export.rs` to depend on `typedmark-semantics`
  instead of each carrying its own `element_kind()`. This step should be
  a **pure refactor** — existing tests in both crates must pass
  unchanged, since output behavior isn't supposed to change yet. Decide
  during this step (per Step 1's open question) whether `"meta"`'s
  "produce nothing" behavior becomes something `typedmark-semantics`
  expresses (e.g. `ElementKind::Meta` that consumers know to skip) or
  stays as each consumer's own `match` arm — likely the *recognition*
  (`this is @meta`) is shared, but the *action* (`""` in HTML vs `""` in
  CommonMark) can legitimately stay consumer-specific since they're
  different outputs for different reasons.
- [ ] **Step 5**: Create `typedmark-resolve` crate
  (`cargo new --lib crates/typedmark-resolve`), add it to root
  `Cargo.toml` the same way as Step 2. Implement `@settings(file:...)`
  resolution: read the referenced file, parse it with
  `typedmark_parser::parse_document`, extract its `@settings{}` block,
  expose it via the API designed in Step 1. Write tests against fixture
  `.tm` files (model one on `docs/docs.settings.tm`'s shape). This is
  genuinely new functionality — there's no existing behavior to preserve
  here, unlike Steps 2-4.
- [ ] **Step 6**: Leave `@import` as a documented design-seam/TODO in
  `typedmark-resolve` (a module doc comment describing the open
  questions) rather than implementing invented syntax — its grammar
  hasn't been specified in this conversation or anywhere in `docs/`.
- [ ] **Step 7**: Update `docs/develop/architecture.md`'s pipeline
  section and crate list to describe `typedmark-semantics` and
  `typedmark-resolve` and where they sit relative to
  `parser`/`renderer`/`markdown`. AGENTS.md requires this doc to be read
  before non-trivial changes, so it needs to stay accurate for whoever
  picks up work here next.
- [ ] **Step 8**: Full verification — `cargo build --workspace`,
  `cargo test --workspace`. Grep for any remaining local
  `element_kind`-style duplication that should have moved to
  `typedmark-semantics` but didn't.
- [ ] **Step 9**: Reconcile with `.agents/tasks/rename-input-area.md` if
  it's still open (see the overlap note above) — check whether it's been
  completed/deleted, and if not, coordinate field-name churn instead of
  redoing the same files twice.
- [ ] **Step 10**: Clean up this task file on completion.
