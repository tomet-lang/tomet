# tomet-ast

Shared AST and `Value` data models for the Tomet toolchain (`Document`, `Block`, `Inline`, `Element`, `Value`, `Position`, `Span`).

## Design Policy

1. **Pure Data Models & Structural Accessors Only**:
   - `tomet-ast` is the foundational dependency-free crate referenced across the workspace.
   - It contains only struct/enum definitions and pure inherent methods for construction (`new`, `with_*`) and structural inspection (`as_str()`, `get()`, `is_null()`, `Span::union()`).

2. **Clear Crate Boundaries**:
   - **Semantics**: Classification of element meanings (e.g., links, `@meta`, `@config`) lives in `tomet-semantics`.
   - **Traversal & Manipulation**: Generic tree traversal algorithms and mutation helpers live in `tomet-tree`.
   - **Formatting & Printing**: Serialization to `.tmt` text lives in `tomet-style` and `tomet-printer`.
   - **Validation & Field Utils**: Field formatting and validation helpers live in `tomet-validator` and `tomet-field-utils`.

3. **Span Equality Contract**:
   - `Span` implements a custom `PartialEq` that always returns `true` so that structural equality comparisons (e.g., `assert_eq!(doc1, doc2)`) ignore source positions.
   - Use `Span::exact_eq()` when exact source position comparisons are required.
