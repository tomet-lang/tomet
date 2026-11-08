# Tag forms: `::tag` (declaration & postfix connect) and `@tag`

Decided by the author on 2026-11-07 and aligned with the `::` connect unification on 2026-11-09. Not started.

Supersedes the earlier `<#tag>` notes in `.agents/tasks/nexus-syntax.md`.

## Decided

- **One `tag` concept across Tomet's grammatical seats:**
  - `::tag(...)` — document-level declaration in the preamble (`File::header`), e.g. `::tag(rust, parser)`, **and**
    postfix connect attaching tags to a host node (section `=[ Heading ]::tag(design)`, list item `- item ::tag(a, b)`,
    or element `@card(...) ::tag(a, b)`), since all connects and declarations are unified onto `::` (`Nexus::Nexus`).
  - `@tag(...)` — body or inline element placing tags directly in content, e.g. `@tag(design)` or `-@tag(a, b)`.
- **All take `()` positional scalar arguments:** one or more tag names inside parentheses (`::tag(a, b)`, `@tag(a, b)`),
  aligning with the `()` argument rule in `.agents/tasks/nexus-syntax.md` and `::profile(a, b)`.
- **Expression access:** exposed via the `::` nexus operator as `${self::tag}` / `${doc::tag}` / `${part::tag}`.
- **`<#tag>` and `#(tag, tag2)` are rejected:**
  - `#` stays strictly intra-document anchor ID (`#(id)` slot, `@link(#id)`, `${#id}`, `path#id`). Writing `<#tag>`
    makes a vault-wide tag look like an intra-file `#(id)` anchor reference.
  - `<>` stays strictly the seat of an unfilled value (`<name>` label, `<?...>` hole, `<%type>` type seat; see
    `tmtroot/dev/plans/angle-box.tmt`), not concrete metadata attached to a node.
  - `#(tag, tag2)` is not syntax (`ID_GROUP` parses a single `#(id)`). Stale prose in
    `tmtroot/spec/elements/std.tag.element.tmt`, `tmtroot/readme.tmt`, and `tmtroot/docs/spec/syntax.tmt` must be
    replaced.
- **Optional `::id(tag)` hub resolution (classification, never a broken link):**
  - Tags classify their host; they do not assert that a target document must exist. A tag with no matching `::id(tag)`
    in the vault is **never** a validation error in `tomet check`.
  - When a document declaring `::id(<tag>)` exists in the vault, workspace tooling (LSP goto-definition and hover,
    vault indexers/backlinks, exporters) may resolve the tag to that document as its hub/definition page.
  - Mandatory cross-document `::id(...)` citations that must fail validation when missing stay in `@link(...)`
    (`tomet-semantics-address`), separate from tags.

## Details to confirm before or during implementation

- **`::tag` cardinality and scope:** `Scope::Both` (allowing `.ctmt` headers to carry `::tag`, matching `::kind` and
  `::id`) vs `Scope::Tmt`; `Cardinality::Once` (like `::profile(a, b)`) vs `Cardinality::Many` (merging multiple
  `::tag(...)` lines in source order).
- **`::meta` relationship:** whether `tags` inside `::meta` is deprecated and migrated to `::tag(...)` (paralleling
  `::meta.type` -> `::kind` and `::meta.id` -> `::id` via `Problem::MetaIdMoved` and `tomet-transform`), and how
  `tomet-semantics-expr` exposes tags (`${doc.tags}`, `${part.tags}`).
- **Tag name lexical rules:** exact constraints on tag scalars inside `()` (non-empty strings; whether hierarchical
  names such as `a/b` or `a.b` work as unquoted bare arguments).

## Steps

- [ ] **1. `tomet-semantics-declaration` (`::tag`):**
  - Add `DeclKind::Tag` (`"tag"`) to `SPECS` and `DeclKind::ALL` in
    `crates/tomet-semantics-declaration/src/kind.rs`.
  - Add `document_tags(&[Declaration])` in `crates/tomet-semantics-declaration/src/read.rs` (modeled on
    `profile_names`).
  - Validate `::tag(...)` argument shape (non-empty positional scalars; reject named args, `{}`, and `[]`) in
    `crates/tomet-semantics-declaration/src/check.rs`.
- [ ] **2. `tomet-semantics` (role-separated sigil dispatch, `:tag`, unified tag extraction):**
  - Refactor `crates/tomet-semantics/src/kind.rs` so `"tag"` can be legal as **both** `::tag` (`DeclKind::Tag`) and
    `@tag` (`ElementKind::Tag`). Today `builtin_kind` checks `DeclKind::from_name` before `BODY_KINDS`,
    `classify_std` calls `refuse_declaration_as_element` (which rejects any `DeclKind` written with `@`), and
    `builtin_kinds()` chains `DeclKind::ALL` with `BODY_KINDS`. Dispatch `@` classification and `::` classification by
    syntactic role without rejecting `@tag` or duplicating `"tag"` in `builtin_kinds()`.
  - Add `ConnectMember::Tag` (`"tag"`) to `CONNECT_MEMBERS` in `crates/tomet-semantics/src/connect_member.rs`.
  - Extend `crates/tomet-semantics/src/elements/tag.rs` to extract tags across `Declaration` (`::tag`), `Connect`
    (`:tag`), `Element` (`@tag`), and `-@tag(...)` combine `ListItem`s (via `ListItem::as_element_view` /
    `for_each_node`, since `for_each_element` does not visit combine list items).
- [ ] **3. Validation (`tomet-validator-*`) and migration (`tomet-transform`):**
  - Validate `:tag(...)` connects and `@tag(...)` elements with the same non-empty scalar argument rules as
    `::tag(...)`.
  - If `::meta` `tags` is superseded, add diagnostic + `tomet-transform` promotion pass (modeled on
    `promote_meta_id_to_id`).
- [ ] **4. Vault, expressions, search/stats, and LSP (`::id(tag)` hub lookup):**
  - Expose document and host-node tags in `tomet-vault`, `tomet-load`, `tomet-semantics-expr`, `tomet-search`, and
    `tomet-stats`.
  - Wire optional `::id(<tag>)` hub lookup in `tomet-workspace-links` / `apps/lsp` (goto-definition and hover on tag
    arguments when a matching `::id(<tag>)` document exists in the vault; no broken-link diagnostic when absent).
- [ ] **5. Formatter and exporters (`tomet-format-printer`, `tomet-convert-*`):**
  - Verify `tomet-format-printer` round-trips `::tag(...)`, `:tag(...)`, and `@tag(...)` cleanly across preamble,
    heading, list-item, block, and inline positions (CST/AST and `tree-sitter-tomet` already parse `::name(...)`,
    `:name(...)`, and `@name(...)` generically).
  - Define export output for `::tag`, `:tag`, and `@tag` in `tomet-convert-markdown`, `tomet-convert-html`,
    `tomet-convert-typst`, `tomet-convert-pandoc`, and standalone export (linking to the resolved `::id(tag)` hub when
    present, plain tag rendering otherwise).
- [ ] **6. Documentation, writ rules, and cross-crate tests:**
  - Rewrite `tmtroot/spec/elements/std.tag.element.tmt` and replace retired `#(tag, tag2, ...)` examples in
    `tmtroot/readme.tmt` and `tmtroot/docs/spec/syntax.tmt`; regenerate artifacts with `just docs`.
  - Add corpus/snapshot tests under `tests/` and verify with `just test`, `just test-suite`, and `just docs-check`.

## Destination when done

- Role-based sigil dispatch (`::tag` / `:tag` / `@tag` sharing one name across declaration, connect, and element) ->
  `//!` and `///` doc-comments in `tomet-semantics` (`kind.rs`, `connect_member.rs`, `elements/tag.rs`) and
  `tomet-semantics-declaration`.
- User-facing syntax, semantics, and optional `::id(tag)` hub behavior ->
  `tmtroot/spec/elements/std.tag.element.tmt` (plus declaration/connect spec pages under `tmtroot/spec/`).
- If `::tag` supersedes `tags` in `::meta` -> `.writ.tmt` rule alongside `kind-not-meta-type`.
- Delete this file once folded.
