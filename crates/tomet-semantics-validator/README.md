# tomet-validator

Read-only `.tmt` document schema and lint validation.

## Architecture & Responsibilities

1. **Read-Only In-Memory Validation**:
   - `tomet-validator` performs pure, read-only semantic validation across an in-memory `Document`.
   - It performs zero I/O, no reference resolution (which is handled by `tomet-resolver` and `tomet-links`), and no AST mutations.
   - Depends only on `tomet-ast` and `tomet-walker`.

2. **Validation Rules**:
   - **Duplicate ID Detection (`id.rs`)**: Walks all element nodes (headings, typed elements, bare elements, inline elements) via `tomet_walker::walk_document` and checks for duplicate `id` attributes in both `{id:...}` and `(id:...)`.
   - **Diagnostic Error Mapping (`error.rs`)**: Emits `ValidationError::DuplicateId` carrying exact `Span` locations for the first definition and the offending duplicate, powering LSP diagnostic ranges in `apps/lsp`.
