# tomet-edit

AST-aware programmatic editing pipelines, batch metadata updates, and structural search & replace.

## Architecture & Responsibilities

1. **AST-Aware Mutation Pipeline**:
   - `tomet-edit` provides high-level document editing operations across single files or workspaces.
   - Follows a consistent transformation flow: Scan (`tomet-indexer`) -> Parse (`tomet-parser`) -> In-place AST Mutation (`tomet-walker`) -> Format & Re-serialize (`tomet-printer`) -> Persist (`save`).
   - Cleanly decoupled from UI rendering so that TUI, CLI, and LSP refactoring tools share the exact same editing engine.

2. **Core Capabilities**:
   - **Batch Metadata Engine (`batch_meta`)**:
     - `BatchMetaEngine`: Updates `@meta` (or `@config`) keys across a collection of files in batch.
     - `set_meta_in_doc`: Modifies existing metadata entries in-place or prepends a new `@meta` element if missing.
   - **Structural Search & Replace (`structural`)**:
     - `StructuralEngine`: Searches for elements matching criteria (`StructuralQuery`: `tag`, `key`, `value_contains`).
     - Refactoring transformations (`StructuralAction`):
       - `RenameTag`: Renames element tags (e.g. `<note>` -> `<caution>`).
       - `RenameKey`: Renames attribute/metadata keys (e.g. `author` -> `creator`).
       - `ReplaceValue`: Replaces map values matching target keys.
