# Unify borrowed AST node views on `tomet_tree::Node` and rename `tomet_expr::Node` to `Part`

> **Prerequisite / Execution Order**:
> Execute **after** `.agents/tasks/unify-ast-file.md` lands (`Document` → `File<B>`, `Block::Declaration` removed, `file.header: Vec<Declaration>` separated from `file.body`). Both tasks touch `crates/tomet-syntax-tree/src/walk.rs`, `crates/tomet-validator-core/src/lib.rs`, and `crates/tomet-semantics-expr/src/nodes.rs`.

## Core Design Principles

1. **Long-term architectural beauty over legacy inertia**: No backward-compatibility type aliases (`pub type Node = Part;` or deprecated `Child` shims). Update every caller cleanly.
2. **Do not force two different layers into one enum**:
   Originally this task asked whether four enums (`tomet_tree::Node`, `tomet_tree::Child`, `tomet_validator_core::Written`, `tomet_expr::Node`) should become a single "any node" type. First-principles analysis shows they belong to **two distinct layers**:
   - **Layer 1 — Syntax-tree borrowed cursor (`tomet_tree::Node<'a>`)**:
     A short-lived, `Copy` borrowed reference over **every vertex of a file's AST** (`Block`, `Inline`, `Value`, `Entry`, `Connect`, `Item`, `List`, `Declaration`), including prose `Paragraph`s, `Text`, `Hole`s, and scalar `Value`s. Parallel to CST's `SyntaxNode`. Keeping the name **`Node`** in `tomet-syntax-tree` is right (macro hierarchy `Vault / Folder / File / Section` uses **`Division`**, so there is no collision).
     `Child<'a>` and `Written<'a>` are redundant borrowed subsets in this same layer and **should be eliminated** in favor of `tomet_tree::Node<'a>` (and `ElementView<'a>`).
   - **Layer 2 — Expression runtime owned value (`tomet_expr::Part`)**:
     Inside `tomet-semantics-expr`, `EvalValue` is an owned `'static` value (stored in `Env` closures and returned across `Arc<File>` loads). Its variant holds **only** structured parts of a document (`Element`, `Section`, `List`, `Connect`, `Declaration`, `Item`) that expose `.name`, `.prop`, `.id`, `.connects`, `.content`, and `.item(n)` — produced via `doc#id`, `doc.item(n)`, `x.connects`, or `EvalValue::from_ast` on `Value::Element`.
     Forcing this into `tomet_tree::Node<'a>` would mix lifetimes or pollute the evaluator with unreachable AST arms (`Paragraph`, `Int`, `Hole`). Instead, keep it as its own owned enum and rename it from the ambiguous `Node` to **`Part`** (`EvalValue::Part { doc, part }`), forming the clean triad **`Document` (whole file) / `Part` (structured part with provenance) / `Fragment` (detached output blocks)**.

---

## Part 1: Borrowed AST Views — Fold `Child` and `Written` into `tomet_tree::Node`

### 1.1 `tomet-syntax-tree` (`crates/tomet-syntax-tree/src/walk.rs`, `lib.rs`)

- Ensure `Node<'a>` covers every AST vertex after `unify-ast-file` (including `Node::Declaration(&'a Declaration)` for `file.header` declarations).
- Add structural accessors on `Node<'a>` so callers never need `Child<'a>`:
  - `pub fn element_view(self) -> Option<ElementView<'a>>`: returns `Some(el.as_view())` when `self.element()` is `Some`, or `item.as_element_view()` when `self` is `Node::Item(item)`, else `None`.
  - `pub fn span(self) -> Span`: returns the `Span` of whichever AST vertex `self` wraps (for `Value`, if `Value` has no span, keep `span` on the roles or return `Option<Span>` — check whether `Node::Value` / `Node::Entry(Entry::Pair)` has a span; if not all `Node` arms carry a `Span`, provide `element_view()` + `Node::Item(item) => item.span` or `pub fn span(self) -> Option<Span>`).
- **Remove `pub enum Child<'a>`** and change the descendant walkers (`descendants`, `for_each_descendant`, `for_each_descendant_of_item`, `for_each_descendant_in_blocks`, `for_each_item_descendant`) to pass `Node<'a>` directly (yielding the exact same filtered set of nodes: elements and list items, skipping `Place::Args | Place::PairValue | Place::Connect`).
- Remove `Child` from `crates/tomet-syntax-tree/src/lib.rs` re-exports and update `//!` docs.

### 1.2 `tomet-validator-rule` & `tomet-validator-core`

- **`crates/tomet-validator-core/src/lib.rs`**:
  - Update `child_kind_name` to accept `node: tomet_tree::Node<'_>` (or rename if appropriate, updating its doc-comment):
    - `if let Some(view) = node.element_view() { Ok(classify_in(view, bindings)?.as_str().to_string()) }`
    - `else if matches!(node, tomet_tree::Node::Item(_)) { Ok(PLAIN_ITEM_KIND_NAME.to_string()) }`
  - **Eliminate `enum Written<'a>`**:
    - After `unify-ast-file`, `Declaration`s exist only in `file.header` and `Element`s / named `ListItem`s in `file.body` (and inside declaration content/data if any).
    - Inspect what remains of `Written<'a>` in `check_singletons_and_regions` / `visit_all` / `visit_child` after `unify-ast-file`:
      - `visit_child` disappears completely (`node.element_view()` replaces it).
      - Check `file.header` declarations directly against `&Declaration`, and body elements via `ElementView<'_>` (using `node.element_view()` in `walk_node`), removing `enum Written<'a>` altogether if no shared multi-arm walker needs it. If a tiny local helper inside `check_singletons_and_regions` still checks singleton/sigil rules for both `header` declarations and `body` `ElementView`s, express it via a plain function parameter or `Node<'_>` rather than a parallel AST node enum.
- **`crates/tomet-validator-rule/src/lib.rs`**:
  - Update `check_one_connect` / `check_one_rule` signatures and closures from `Child<'_>` to `tomet_tree::Node<'_>`.

---

## Part 2: Expression Evaluator — Rename `tomet_expr::Node` to `Part`

### 2.1 `crates/tomet-semantics-expr`

- **`src/value.rs`**:
  - Rename `pub enum Node` → `pub enum Part` (keeping variants `Element`, `Section`, `List`, `Connect`, `Declaration`, `Item`).
  - Update its doc-comment to state what a `Part` actually is: a structured part of a document (`doc: DocRef`) inspectable via `.name`, `.prop`, `.id`, `.connects`, `.content`, and `.item(n)` — in contrast to `EvalValue::Document(DocRef)` (the whole document) and `EvalValue::Fragment(Vec<Block>)` (detached output blocks).
  - Rename `EvalValue::Node { doc: DocRef, node: Box<Node> }` → `EvalValue::Part { doc: DocRef, part: Box<Part> }`.
  - Keep `EvalValue::type_name()` returning `"element"` for `EvalValue::Part { .. }` (this is the user-facing type name in diagnostics and `tmtroot/spec/expressions/`).
  - Update `EvalValue::from_ast` (`Value::Element(el)` arm) and `//!` module doc-comments (`=[ References and fragments ]`).
- **`src/nodes.rs` → `src/parts.rs`**:
  - Rename module file `crates/tomet-semantics-expr/src/nodes.rs` to `src/parts.rs` (`mod parts;` in `src/lib.rs`).
  - Rename functions and locals cleanly:
    - `find_node` → `find_part`
    - `expr_node(node: tomet_tree::Node<'_>) -> Option<Part>` → `Part::from_tree_node(node: tomet_tree::Node<'_>) -> Option<Part>` (the single explicit bridge from Layer 1 `tomet_tree::Node` to Layer 2 `tomet_expr::Part`)
    - `node_by_id` → `part_by_id`
    - `node_member` → `part_member`
    - `node_method` → `part_method`
- **Other files in `crates/tomet-semantics-expr/src/`**:
  - `lib.rs`: `pub use value::{..., Part};` (remove `Node`), update `refers_to_vault` (`EvalValue::Part { .. }`).
  - `address.rs`, `fragments.rs`, `include.rs`, `methods.rs`, `ops.rs`, `outside.rs`, `tests.rs` (and submodules): update all `EvalValue::Node` / `Node::*` imports, patterns, and comments to `EvalValue::Part` / `Part::*`.
- **Workspace-wide caller check**:
  - Grep the workspace (`rg '\btomet_expr::Node\b|EvalValue::Node\b|\bChild::'`) to ensure zero stale references remain in `tomet-semantics-eval`, `tomet-transform`, `tomet-load`, tests, or bindings.

---

## Documentation & Folding

- Update `//!` doc-comments in:
  - `crates/tomet-syntax-tree/src/lib.rs` and `walk.rs`
  - `crates/tomet-semantics-expr/src/lib.rs`, `value.rs`, and `parts.rs`
- Check `tmtroot/dev/design/node-tree.tmt` (if present/relevant) so it does not conflate `Division` (`Vault / Folder / File / Section`), `tomet_tree::Node` (AST traversal vertex), and `tomet_expr::Part` (expression runtime document part).
- Run `just docs` if any `.tmt` file or public Rust API doc (`tomet api --check`) changed.

---

## Execution & Verification Checklist

- [ ] Isolate work in a jj workspace: `mkdir -p .agents/workspaces && jj workspace add .agents/workspaces/node-kinds-unify`
- [ ] Verify `unify-ast-file` has already landed; inspect post-merge `walk.rs`, `validator-core/src/lib.rs`, and `expr/src/nodes.rs`.
- [ ] Complete **Part 1**: add accessors to `tomet_tree::Node`, remove `tomet_tree::Child`, and eliminate `tomet_validator_core::Written`.
- [ ] Complete **Part 2**: rename `tomet_expr::Node` → `Part`, `EvalValue::Node` → `EvalValue::Part`, `nodes.rs` → `parts.rs`, and expose `Part::from_tree_node`.
- [ ] Update `//!` / `///` doc-comments and any affected `.tmt` design docs (`just docs` if needed).
- [ ] Run verification inside the Nix dev environment:
  - `just test` (`cargo test --workspace`)
  - `just test-suite` (`cargo test -p tomet-tests`)
  - `just writ-check` (`twrit check`)
  - `just docs-check` (`tomet check`, `format --check`, `export --check`, `api --check`, `twrit check`)
- [ ] Clean up workspace (`jj workspace forget node-kinds-unify && rm -rf .agents/workspaces/node-kinds-unify`) and delete `.agents/tasks/node-kinds-unify.md` once folded and verified.
