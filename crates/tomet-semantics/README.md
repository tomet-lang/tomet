# tomet-semantics

I/O-free semantic classification and normalization layer for Tomet AST elements.

## Architecture & Responsibilities

1. **Pure I/O-Free Semantic Layer**:
   - `tomet-semantics` is strictly a pure classification and semantic extraction layer. It performs zero I/O and depends exclusively on `tomet-ast` (not `tomet-parser`), allowing downstream consumers (`tomet-html`, `tomet-markdown`, `tomet-links`, `tomet-tui`, `tomet-lsp`) to classify elements without pulling in the parser.

2. **Core Capabilities**:
   - **Element Classification (`kind.rs`)**: Canonical recognition of Tomet's built-in vocabulary (`@meta`, `@config`, `@links`, `@link`, `<embed>`, `hr`, `em`, `strong`, `mark`, `codeblock`, `quote`, `table`, `heading`, `ol`, `ul`, `bare`, `interp`) via `ElementKind` and `classify(&Element) -> ElementKind`.
   - **Link Target Extraction & Scheme Classification (`target.rs`)**: Canonical extraction of link targets (`link_target`, `link_target_of`) and scheme prefix classification (`TargetScheme::Url`, `File`, `Tm`, `Id`, `Ref`) via `target_scheme`.
   - **Positional Argument Normalization (`positional.rs`)**: Unifies positional argument mapping (e.g. `<codeblock>(rust)` -> `{lang: "rust"}`) for built-in elements and custom `@settings`-defined element schemas (`SettingsSchema`), while recovering split scheme prefixes (`@link(tm:foo)` -> `target: "tm:foo"`).
   - **Document Configuration (`config.rs`)**: Merges `@config` and `@settings` blocks across a document into a structured `DocumentConfig` (export format types, output paths, table width adjustment, style).
   - **Metadata Extraction (`meta.rs`)**: Extracts top-level `@meta` values via `document_meta`.
   - **Heading Levels (`heading.rs`)**: Extracts clamped `1..=6` integer levels from heading elements.
   - **List Helpers (`list.rs`)**: Provides ordered list detection (`list_ordered`) and item extraction (`list_items`).
   - **Table Parsing (`table.rs`)**: Parses table rows and cells from `@table` inline content (`parse_table_rows`).
   - **Connected Value Merging (`connect.rs`)**: Merges connected values (`:{...}`, `:(...)`) with direct element values according to specification precedence rules.
