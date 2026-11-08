# Nexus Syntax, Singular Dot-Path Declarations, `()` Seq & `::` Connect Implementation

Implementation tracking file for the unified Nexus syntax and declaration architecture (`tmtroot/dev/design/nexus-brackets-and-singular-declarations.tmt` and `.agents/tasks/nexus-syntax.md`).

- **Workflow:** **One commit per feature, each 100% green** (`just format-check`, `just test`, `just test-suite`, `just gen-treesitter-check`, `twrit check .`). Every slice includes its own parser, semantics, printer, tree-sitter, and fixture updates so no commit leaves the test suite broken.
- **Workspace:** `.agents/workspaces/nexus-syntax`

---

## Feature Commits (`Commit 1` .. `Commit 6`)

### Commit 1: Enforce Nexus Primary Group (`()` / `[]`) & Migrate `{}` Declarations to `()` **[READY TO COMMIT]**

- [x] **1.1** Enforce primary group requirement (`()` or `[]`, plus `@table` pipe-run exception) and reject bare `@name`, `@name{}`, `@name#(id)`, `::name`, `::name{}` across `tomet-syntax-cst-parser`, `tomet-syntax-lowering`, and `tomet-syntax-ast-parser`.
- [x] **1.2** Update bare `@hr` to `@hr()` across crates, converters, tests, and docs.
- [x] **1.3** Migrate the 12 `{}`-only declarations to `(...)` argument groups (`bound_args()`).
- [x] **1.4** Update `tree-sitter-tomet`, `tree-sitter-cotomet`, corpus fixtures, and reference snapshots (`just test`: 1979 passed; `just test-suite`: 54 passed; `twrit check .` & `just gen-treesitter-check`: exit 0).

---

### Commit 2: Bare `(a, b, c)` Sequence Literals in Value Position & Removal of Data-Space `list(...)`

**Scope:** `crates/tomet-syntax-cst-parser`, `crates/tomet-syntax-lowering`, `crates/tomet-syntax-ast-parser`, `crates/tomet-format-printer`, `crates/cotomet`, `crates/tree-sitter-*`, `tests/`

- [ ] **2.1** Parse `( ... )` in value positions (`key: (a, b)`, nested sequence elements) as `Value::Seq` (`()` = empty seq, `(x)` = 1-element seq, `(a, b)` = multi-element seq).
- [ ] **2.2** Remove `list(...)` call-to-seq coercion from data value space so `list(...)` stays strictly in type space (`type: list(string)`).
- [ ] **2.3** Update `tomet-format-printer` and `cotomet` printer to emit `(a, b)` instead of `list(a, b)`.
- [ ] **2.4** Update Tree-sitter grammars, corpus fixtures, and `tmtroot/` files (`vault.config.tmt`, etc.), and verify all tests pass.

---

### Commit 3: Unify Postfix Connects onto `::name(...)` / `::{...}`

**Scope:** `crates/tomet-syntax-cst-parser`, `crates/tomet-syntax-lowering`, `crates/tomet-syntax-ast-parser`, `crates/tomet-format-printer`, `crates/cotomet`, `crates/tree-sitter-*`, `tests/`

- [ ] **3.1** Change postfix connect syntax on Elements, Sections, ListItems, and Cotomet values from single-colon `:name(...)` / `:{...}` to double-colon `::name(...)` / `::{...}` (while keeping `{...}` directly attached to `@name(...)` as `::meta(...)` sugar and `#(id)` as `::id(id)` sugar), reserving single `:` strictly for `key: value`.
- [ ] **3.2** Update `tomet-format-printer`, Tree-sitter grammars, corpus fixtures, and `tmtroot/` examples, and verify all tests pass.

---

### Commit 4: Singular Dot-Path Declarations (`::field.x`), `X.X == X` Contraction & `[...]` Prose Sugar

**Scope:** `crates/tomet-syntax-*`, `crates/tomet-semantics-declaration`, `crates/tomet-semantics`, `crates/tomet-config`, `crates/tomet-load*`, `crates/tomet-validator*`, `crates/tomet-format*`, `crates/tree-sitter-*`, `tests/`

- [ ] **4.1** Support multi-segment dotted paths on `Declaration` (`::root.sub.leaf(...)`) and preserve `[...]` and `#(id)` attachments on declarations.
- [ ] **4.2** Unify plural declaration roots (`macros`, `vars`, `fields`, `elements`, `vocabularies`, `dictionaries`, `blueprints`) onto singular root names (`macro`, `var`, `field`, `element`, `vocabulary`, `dictionary`, `blueprint`), folding both dotted heads (`::field.author(...)`) and grouped parent args (`::field(author: {...})`) into the same tree.
- [ ] **4.3** Implement **`X.X == X` self-slot contraction** (`::id(a)` $\equiv$ `::id(id: a)` $\implies$ `id == a`, `::macro.copyright("...")` $\equiv$ `::macro(copyright: "...")`) and `[...]` prose sugar key resolution (default `prose`; `label` on `@link`, `about` on `::field` / `::var` / `::element`, `body` on `@callout`).
- [ ] **4.4** Migrate vocabulary files to `::vocabulary.<ns>(version: "1.0.0")` + `::element.<name>(...)[about]`, update fixtures/Tree-sitter/docs, and verify all tests pass.

---

### Commit 5: Dictionary Body Form (`::dictionary(name)` Header + `;@term(...)` in Body)

**Scope:** `crates/tomet-semantics/src/dictionary.rs`, `crates/tomet-load/src/dictionary.rs`, `crates/tomet-validator-core`, `tmtroot/dict/tomet.dict.tmt`, `tests/`

- [ ] **5.1** Make `::dictionary(name)` a header-only declaration and extract dictionary entries from `@term("word")[description]#(id)` elements in the document body.
- [ ] **5.2** Migrate `tmtroot/dict/tomet.dict.tmt` and dictionary tests, and verify all tests pass.

---

### Commit 6: Expression `::` (Nexus Traversal) vs `.` (Data Key / Method) & `.prop` Removal

**Scope:** `crates/tomet-semantics-expr`, `crates/tomet-semantics-eval`, `crates/tomet-semantics-address`, `crates/tomet-vault`, `tests/` (see `.agents/tasks/address-and-division-expr.md`)

- [ ] **6.1** Add `expr::ident` in `tomet-semantics-expr` for Nexus-to-Nexus traversal (`self::doc`, `self::parent`, `self::meta`, `self::id`, `u1::id`, `u1::meta`, `u1::prose`).
- [ ] **6.2** Make `expr.key` read directly from the nexus's `()` arguments or a `{}` map (`u1.id`, `self::meta.author`, `u1::meta.tag`), keep `expr.method(...)` for method calls, and remove `.prop`.
- [ ] **6.3** Update expression fixtures, docs, and tests, and verify all tests pass.
