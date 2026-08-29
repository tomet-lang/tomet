# tomet-walker

Generic recursive AST traversal for `tomet_ast::Document`.

## Architecture & Responsibilities

1. **Traversal Engine without Parser / I/O Dependencies**:
   - `tomet-walker` provides generic recursive tree walking across a `Document`.
   - Depends strictly on `tomet-ast` alone (no `tomet-parser`, no I/O, no semantic classification dependency).
   - Any consumer (`tomet-validator`, `tomet-resolver`, `tomet-indexer`, `tomet-edit`, `tomet-links`, `apps/lsp`) can traverse documents without pulling in unnecessary dependencies.

2. **Core Capabilities**:
   - **Immutable Traversal (`walk_document`, `Visitor`, `for_each_element`)**:
     - Depth-first, document-order traversal visiting all `Element` nodes (including headings, list items, bare elements, inline elements in `[content]`, and children in `ElementValue::Children`).
     - Early termination support via `ControlFlow::Break(b)`.
     - Closure-based traversal via `for_each_element` and `impl<F, B> Visitor<B> for F`.
   - **Mutable Traversal (`walk_document_mut`, `VisitorMut`, `for_each_element_mut`)**:
     - In-place mutation of `&mut Element` nodes for AST transformations (structural editing, batch metadata).
     - Closure-based in-place traversal via `for_each_element_mut` and `impl<F, B> VisitorMut<B> for F`.
   - **Attribute Views (`element_attrs_view`, `element_attrs_mut`)**:
     - `element_attrs_view(&Element) -> Option<Value>`: Merges `(args)` and `{value}` into one unified map view (with `{value}` overriding on conflict), essential for id search across headings and elements.
     - `element_attrs_mut(&mut Element) -> Option<&mut Value>`: Accesses the mutable attribute map slot (targeting `args` primarily, falling back to `value` map for headings).
