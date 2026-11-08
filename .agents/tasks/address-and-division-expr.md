# Address system (`tmt:`, `id:`) and Division expressions

Decided by the author on 2026-11-07. Not started.

Builds on `.agents/tasks/node-tree.md` (`Vault > Folder > File > Section` Tome tree) and `.agents/tasks/nexus-syntax.md`
(`()` args, `{}` data, `[]` sugar, `#(id)`). Resolves the open `ref:` / `id:` questions in
`tmtroot/dev/plans/address-followups.tmt` and `.agents/tasks/id-generation-settings.md`.

## Decided

- **Five address schemes (`ref:` is abolished):**
  - **`tmt:` — Tome structural path (`Vault > Folder > File > Section`):**
    - Uses a single `/` separator across every Division level (`Folder`, `File`, `Section`). There is no `>` separator.
    - Logical paths omit the `.tmt` extension (e.g. `tmt:/docs/syntax/addressable`), making addresses invariant when a
      Division is promoted or demoted between `Folder`, `File`, and `Section`.
    - Coexistence of `foo/` (Folder) and `foo.tmt` (File) in the same directory is forbidden; a Folder's own prose and
      declarations belong in its entry file (`foo/self.tmt`).
    - Path origin is always explicit:
      - `tmt:/...` — rooted at `Vault::root` (the vault's root `Folder`, whose entry is `./self.tmt`; `tmt:/` names that
        root `Folder` itself).
      - `tmt:./...` / `tmt:../...` — relative to the **innermost enclosing Division** (`Section` when written inside a
        section, `File` in a regular file outside sections, `Folder` in `self.tmt` outside sections, `Vault` in
        `vault.config.tmt` outside sections).
      - Bare `tmt:b` (without leading `/`, `./`, or `../`) is invalid.
    - Path segments containing `/` (e.g. a section titled `Seq / Map`) may be quoted (`tmt:/spec/overview/"Seq / Map"`).
    - Optional `#<elem_id>` suffix (`tmt:/docs/syntax/overview#fig-1`) is permitted and resolves `#(elem_id)` strictly
      within the target Division's own scope (`File`: that file; `Folder`: its `Folder::entry`, never child files;
      `Vault`: `Vault::config`; `Section`: that section's block subtree), keeping `tmt:<path>#<elem_id>` invariant under
      `Section` ⇔ `File` ⇔ `Folder` refactoring as well.
  - **`id:` — 2-level logical identity (`::id` + `#(id)`):**
    - `id:<file_id>` — resolves to the Division declaring `::id(<file_id>)` (`File`, `Folder`'s `self.tmt`, or `Vault`'s
      `vault.config.tmt`). Replaces the old intra-document-only `id:<anchor>` meaning.
    - `id:<file_id>#<elem_id>` — resolves `#(elem_id)` inside that `::id(<file_id>)` document.
    - `#<elem_id>` (shorthand for `id:#<elem_id>`) — resolves `#(elem_id)` within the current `.tmt` document
      (`self.doc`).
  - **`file:` — OS physical file path** (assets, source files, non-Tome files).
  - **`dir:` — OS physical directory path** (kept distinct from Tome `Folder`: OS directories and Tome `Folder`
    Divisions are separate concepts; `@file(...)` and `@dir(...)` remain scheme-less OS path mentions).
  - **`https:` / `url:` — External Web URL.**
  - **`ref:` is abolished** across `tomet-semantics-address`, `tomet-workspace-links` (`VaultLinkIndex::resolve_ref`),
    `tomet-semantics-expr`, validators, converters, and LSP. Fuzzy title/stem lookup belongs to editor/LSP completion
    inserting explicit `id:` or `tmt:` addresses, never to an ambiguous stored scheme.

- **Strict boundary between addresses and `${expr}`:**
  - Address syntax (`@link`, `@embed`, `addr(...)`, `str.addr()`) **identifies targets only**; it never carries property
    or value steps.
  - Property steps inside `addr(...)` (such as `addr(tmt:docs/a.tmt#intro.prop.tags)`) are removed so there is only one
    way to read properties: `${addr(...)::meta.tags}` or `${addr(...).key}` (`explicit-form-first`).

- **Address construction in `${expr}` is unified onto `addr(...)` and `.addr()`:**
  - Builtin functions `tmt("...")` and `ref("...")` in `tomet-semantics-expr` are removed.
  - Static literal addresses use `addr(...)` (`addr(tmt:/docs/a)`, `addr(id:std.config#intro)`, `addr(#intro)`); dynamic
    runtime strings use `.addr()` (`("tmt:" + path).addr()`, `raw.addr()`).

- **Three distinct access forms in `${expr}` (`::` for Nexus, `.` for Data keys, `.method()` for Methods — `.prop` is abolished):**
  - **`X::name` (Nexus $\to$ Nexus traversal):** steps from a Division or Element nexus to a connected nexus (`::id`,
    `::meta`, `::rule`, `::kind`, `::prose`) or structural Division relation (`::doc`, `::parent`, `::children`,
    `::folder`, `::files`, `::path`, `::file_path`, `::filename`, `::name`, `::title`).
    - Because `{...}` on an element is sugar for `::meta(...)` and `#(id)` is sugar for `::id(id)`, they are accessed as
      `elem::meta.key` and `elem::id` (where `::id` evaluates directly to its scalar value via `X.X == X`).
  - **`X.key` (Nexus/Map $\to$ Data key access):** reads a key directly from the nexus's `()` arguments or a `{}` map
    value (e.g. `user.id` reads `id` from `@user(id: "u123")`, and `self::meta.author` reads `author` from `::meta`).
    User argument names inside `()` (`id`, `name`, `parent`, `path`, `title`, etc.) never collide with `::` nexus
    members, eliminating `.prop` completely.
  - **`X.method(...)` (Method call):** distinguished unambiguously from `.key` by the presence of `()` (since Tomet data
    maps do not hold first-class closures), e.g. `self::meta.tags.join(", ")`, `self.link()`.

- **Division (`Vault / Folder / File / Section`) values and members in `${expr}`:**
  - `self` evaluates to the **innermost enclosing Division** (`Vault`, `Folder`, `File`, or `Section`), identical to
    `addr(tmt:.)`. It is a reference value (not implicitly coerced to output text); representations and enclosing scopes
    are projected explicitly via `self::<member>` (or methods like `self.link()`).
  - `vault` evaluates to the top-level `Vault` Division (replacing `EvalValue::Directory(DirRef(""))`).
  - Bare `#<id>` in `${#id}` resolves in `self::doc` (the current `.tmt` document), matching `@link(#id)` and
    `addr(#id)`. When `#(id)` sits on a `Section`, resolving `#id` produces that `Section` Division so path-resolved and
    ID-resolved sections expose identical members.
  - **Division nexus members (accessed via `::`):**
    - Identity & representations:
      - `::path` — Tome logical path (`"/docs/syntax/overview"`, `"/"` for root `Folder`, `Null` for `Vault`).
      - `::name` — final segment of the Tome path (`""` / `Null` for root `Folder` / `Vault`).
      - `::id` — `::id(...)` on `Vault` / `Folder` / `File`, `#(id)` (`::id`) on `Section` (`Null` when unset).
      - `::title` — heading text on `Section`, `::meta.title` on `Vault` / `Folder` / `File` (`Null` when unset; never
        guesses from filename).
      - `::file_path` / `::filename` — vault-relative physical `.tmt` path and filename (`"tmtroot/agents.tmt"`,
        `"docs/self.tmt"`; `Null` only on an entry-less `Folder`; replaces old `self.path` in
        `macro.generated_by(self::file_path)`).
      - `.link()` — method returning a link fragment to this Division.
      - `::prose` — body block fragment of this Division (or `[...]` prose on an Element).
    - Hierarchy navigation:
      - `::parent` — immediate parent Division (`Null` on `Vault`).
      - `::children` — direct child Divisions as `Seq` (in `tomet_tome::children` order).
      - `::doc` — the Division of the enclosing `.tmt` file (`File` for regular files, `Folder` for `self.tmt`, `Vault`
        for `vault.config.tmt`), allowing `${self::doc::meta.xxx}` from any section depth and staying invariant across
        `foo.tmt` ⇔ `foo/self.tmt`.
      - `::folder` — nearest enclosing `Folder` Division (`Null` on `Vault`).
      - `::files` — on `Vault` and `Folder`, all descendant `File` Divisions as `Seq` (replaces `.docs`; OS `Directory`
        from `addr(dir:...)` stays separate).
    - Declarations & connected nexuses:
      - `::meta`, `::kind`, `::rule`, `::tag` — on `Vault` / `Folder` / `File` / `Section` / `Element` (alongside direct
        `.key` access for `()` arguments on an `Element` and `.item(n)` / `.link()` methods).

## Details to confirm before or during implementation

- **Crate layering (`crate-layering` in `.writ.tmt`):** `tomet-semantics-address` and `tomet-semantics-expr` are in
  Layer 3 while `tomet-tome` is in Layer 6. Keep `tomet-semantics-expr` pure behind its `Resolver` trait (implemented in
  Layer 6/7 over `Tome` and `Vault`), or if any `tomet-tome` type needs to live in a lower layer, **propose the
  `.writ.tmt` edit to the author and stop** rather than working around the layer rule.
- **Sibling name collisions under one Division:** `Tome::build` / `tomet check` must reject same-stem `foo/` and
  `foo.tmt` in one directory; confirm whether duplicate sibling section titles (e.g. two `=[ 例 ]` under one parent) or
  an entry section colliding with a child file/folder stem are hard errors in `tomet check` or reported only when
  targeted by a `tmt:` path.
- **Unquoted `scheme:...` in `@link(...)`:** verify `recover_target_scheme_entry`
  (`crates/tomet-semantics/src/positional.rs`) handles `tmt:/...`, `tmt:./...`, `tmt:../...`, `id:<file>#<elem>`, and
  quoted segments cleanly when written bare inside `@link(...)` / `@embed(...)`.
- **Open-vocabulary unnamed positionals with `{}`:** confirm behavior of `.prop` and duplicate-key checking when an
  element in an `open: true` vocabulary carries both an unnamed bare positional `(x)` (no `ParamDecl` slot) and
  `{ k: v }`.

## Steps

- [ ] **1. `tomet-semantics-address` (address grammar & types):**
  - Replace `AddressKind` / `Address` / `LinkKind` to represent the 5 schemes (`tmt:`, `id:` 2-level, `file:`, `dir:`,
    `url:`/`https:`), removing `Ref` and `Unresolved`.
  - Parse `tmt:` paths (`/` root, `./` and `../` relative, quoted `"..."` segments, optional `#<elem_id>`, rejecting
    bare `tmt:b` and `.tmt` suffixes) and 2-level `id:` (`id:<file_id>`, `id:<file_id>#<elem_id>`, `#<elem_id>`).
- [ ] **2. `tomet-tome` & vault-wide `::id` index (`tomet-vault`, `tomet-load`):**
  - Enforce stem uniqueness between `Folder` (`foo/`) and `File` (`foo.tmt`) in `Tome::build`, and add Tome path /
    relative Division lookup + scoped `#(elem_id)` lookup over `Node`.
  - Build and validate the vault-wide `::id(...)` table across `Vault`, `Folder` entry, and `File` documents (reporting
    duplicate `::id` across files in `tomet check`).
- [ ] **3. Validation (`tomet-semantics`, `tomet-validator-*`):**
  - Check duplicate keys across normalized `()` (including `[...]` sugar) and `{}` on elements, sections, connects,
    declarations, and list items.
  - Update intra-document and workspace link validation (`tomet-workspace-links`, removing `VaultLinkIndex::resolve_ref`)
    for 2-level `id:` and Tome `tmt:` paths.
- [ ] **4. `tomet-semantics-expr` & `tomet-semantics-eval` (Division values, `self`, `vault`, `.prop`):**
  - Replace `EvalValue::Document` with Division references (`Vault`, `Folder`, `File`, `Section`), wire `self` to the
    enclosing Division (`addr(tmt:.)`) and `vault` to `Node::Vault`, and keep `EvalValue::Directory` strictly for OS
    `dir:` addresses.
  - Remove property steps inside `addr(...)` / `.addr()`, remove builtin functions `tmt(...)` and `ref(...)`, and bind
    bare `${#id}` to `self.doc`.
  - Implement all Division members (`.path`, `.name`, `.id`, `.title`, `.file_path`, `.filename`, `.parent`,
    `.children`, `.doc`, `.folder`, `.files`, `.meta`, `.kind`, `.prop`, `.connects`, `.content`, `.link()`) and
    normalize positional `()` in `prop()`.
- [ ] **5. Exporters, formatter, refactor/LSP (`tomet-convert-*`, `apps/lsp`, `apps/cli`):**
  - Update Markdown, HTML, Typst, and Pandoc link/anchor resolution and relative link rebasing (`rebase.rs`) for `tmt:`
    Division paths and 2-level `id:`.
  - Update LSP goto-definition, hover, and link completion (completing titles/stems into explicit `id:` or `tmt:`
    targets instead of `ref:`).
- [ ] **6. Repository `.tmt` migration, spec pages, and test suite:**
  - Migrate repository `.tmt` files (`default.config.tmt`'s `generated_by` macro `self.path` -> `self.file_path`, old
    `tmt:*.tmt` / `id:anchor` / `ref:...` links, `tmt("...")` / `ref("...")` expressions).
  - Rewrite `tmtroot/spec/syntax/addressable.tmt`, `tmtroot/spec/expressions/address.expr.tmt`,
    `tmtroot/spec/expressions/self.expr.tmt`, `tmtroot/spec/expressions/names.expr.tmt`, and
    `tmtroot/spec/expressions/types.expr.tmt`; clean up superseded items in `tmtroot/dev/plans/address-followups.tmt`
    and `.agents/tasks/id-generation-settings.md`.
  - Regenerate docs (`just docs`) and verify with `just test`, `just test-suite`, `just docs-check`, and
    `just writ-check`.

## Destination when done

- Address grammar, schemes, and invariants -> `//!` / `///` doc-comments in `tomet-semantics-address` and
  `tmtroot/spec/syntax/addressable.tmt`.
- Tome Division path resolution and stem-collision rule -> `//!` / `///` doc-comments in `tomet-tome` and
  `tmtroot/spec/tome.tmt` / `tmtroot/spec/division*.tmt`.
- Division values, `self` / `vault`, `addr(...)` / `.addr()`, and unified `.prop` rules -> `//!` / `///` doc-comments in
  `tomet-semantics-expr` and `tmtroot/spec/expressions/{address,self,names,types}.expr.tmt`.
- Duplicate-key prohibition across `()` and `{}` -> `//!` / `///` doc-comments in `tomet-semantics` /
  `tomet-validator-*` (and a `.writ.tmt` guard rule if desired by the author).
- Delete this file once folded.
