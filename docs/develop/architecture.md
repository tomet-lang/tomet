# Architecture

Tomet is a small markup language (`.tmt` files) plus a Rust toolchain
around it: a parser, an AST, consumers that turn that AST into other
things (HTML, CommonMark, formatted source, serialized data), and a set of
apps/editor integrations built on top. This doc is a map of how those
pieces fit together and depend on each other -- not a spec of the
language itself. For the grammar, `docs/spec/*.tmt` are the
spec docs (constructs marked `// 未実装` aren't implemented yet);
`docs/guide/cheatsheet.tmt` is a live example file, and `tests/fixtures/` holds
the frozen copies of it that the tests actually read. The
core grammar they document is frozen as of `docs/develop/
grammar-freeze.md` -- breaking changes to already-decided syntax go
through that doc's process, not a silent parser diff.

## The pipeline

```
tomet-cst     (Rowan-based lossless Green/Red tree: SyntaxNode / SyntaxToken)
      ^
      |  uses
tomet-lexer   (byte-position cursor & lossless tokenizer over &str)
      |
      v
tomet-parser  (recursive-descent CST & AST parser: &str -> SyntaxNode / Document / Value)
      |
      v
tomet-ast     (Pure Data AST Schema: Document / Block / Inline / Element / Value / Span)
      |
      v
tomet-tree    (Extension traits, AST builders, and generic tree visitors/walkers)
      |
      v
tomet-semantics (I/O-free classification of what an Element means)
```

- **`tomet-cst`** (`crates/tomet-syntax-cst`): Concrete Syntax Tree (CST) foundation powered by `rowan`.
  Maintains 100% of characters (including trivia, comments, and whitespace)
  losslessly (`syntax_node.text().to_string() == original_source`). Provides
  exact `TextRange` byte spans for LSP diagnostics, autocompletion, and lossless refactorings.
- **`tomet-lexer`** (`crates/tomet-syntax-lexer`): byte-position cursor and lossless tokenizer over `&str`.
  Emits `(SyntaxKind, &str)` tokens covering all trivia and structural elements.
- **`tomet-ast`** (`crates/tomet-syntax-ast`): pure data AST schema shared by every other crate and language bindings.
  Contains zero business logic or AST manipulation methods.
  `Value` is the pure-data subset (maps 1:1 onto serde's data model: maps,
  sequences, scalars) -- it's what fills an element's `(args)` or
  `{value}` group, and it's the entire result of parsing a data-only `.tmt`
  file. `Document`/`Block`/`Inline`/`Element` represent markup AST nodes
  in source order.
- **`tomet-tree`** (`crates/tomet-syntax-tree`, package `tomet-tree`): AST manipulation,
  extension traits (`ValueExt`, `ElementExt`, `DocumentExt`), builder helpers
  (`element_new`, `element_list`, `element_list_item`), and generic recursive visitor
  traversals (`walk_document`, `walk_document_mut`, `for_each_element`, `transform_elements`).
  Consolidates all AST query/mutation mechanics outside `tomet-ast` to keep the core data model pure.
- **`tomet-parser`** (`crates/tomet-syntax-parser`): the actual grammar implementation, and the one
  place that gets to decide what `.tmt` source means. Recursive-descent,
  built directly on `tomet-lexer`'s cursor (see `document.rs`'s module
  doc for the grammar it covers: `<T>(args)[content]{value}`, `@name...`,
  bare `@(key:...)`, and the bare-element children of containers like
  `@links{}`). `value.rs` handles the `Value`-only grammar shared by
  `(args)`/`{value}` bodies and by whole data-only documents.
  `embedded_format.rs` is the escape hatch that lets a `{value}` body be
  real JSON/YAML/TOML source instead of Tomet's own lightweight
  grammar, driven by a `format` key (locally, or document-wide via
  `@config(format:...)`).

  **Deterministic Static Parser Boundary**:
  `tomet-parser` is strictly a pure, side-effect-free, deterministic static
  parser. It performs zero I/O, external file resolution, or dynamic code execution.
  Given identical input text, it produces identical AST output with guaranteed linear/predictable time complexity.
- **`tomet-semantics`** (`crates/tomet-semantics`): I/O-free classification of what a parsed
  `Element` officially means -- `url`/`file`/`tm`/`id`/`ref` inference
  from a bare `@(key:...)` (`infer_at_kind`), plus shape-based inference
  for the two forms that carry no `key:` at all, `@(/some/path)` and
  `@(https://example.com)` (`infer_at_kind_from_scalar`; the grammar
  work that makes these parse at all lives in `tomet-parser`'s
  `value.rs` -- see `docs/design/decisions/2026-08-22-link-reference-uri-schemes.md`
  section 5's "Group B"), and recognizing Tomet's own built-in
  vocabulary (`@version`, `@kind`, `@meta`, `@config`, `@links`, `em`/`strong`/`mark`, ...) via
  an `ElementKind` enum and a `classify(el: &Element) -> ElementKind`
  function. Depends only on `tomet-ast` (not `tomet-parser` --
  the diagram above shows the pipeline's logical ordering, not a Cargo
  dependency edge), so any consumer holding an `Element` can classify it
  without pulling in the parser. Exists so `tomet-html` and
  `tomet-markdown` don't each carry their own copy of this
  recognition logic, which is what "what does this kind mean" would
  otherwise silently drift into being (both used to independently
  reimplement it as a `Sigil` match returning a stringly-typed `kind:
  String`) -- the TUI's structural-search engine
  (`apps/tui/src/engine`) uses it the same way. Only expresses
  *recognition*, not *action*: whether a given kind's output is empty
  (`@meta`/`@config`) is still each consumer's own call, since the same
  kind can mean different things for different output formats. Also owns
  `target::link_target`/`link_target_of`: the single canonical extraction
  of a link-shaped element's raw target string (`Url`/`File`/`Embed`/
  `Tm`/`Id`/`Ref`, per each kind's own fallback-key list), which
  `tomet-html`, `tomet-markdown`, and `tomet-links` all
  route through instead of each reading `el.args` (and disagreeing on
  fallback order) independently -- see
  `docs/design/decisions/2026-08-22-link-reference-uri-schemes.md` for the design
  history behind unifying this.
- **`tomet-validator`** (`crates/tomet-semantics-validator`): `.tmt` schema/lint validation. Currently one
  rule -- duplicate `{id:...}`/`(id:...)` detection across a `Document`,
  built on `tomet-tree`. Read-only: no I/O, no reference resolution
  (that's `tomet-resolver`), no computation (`tomet-compute`).
- **`tomet-compute`** (`crates/tomet-semantics-compute`): in-memory expression evaluation
  engine for `${...}` interpolation expressions (`InterpExpr`) and built-in math
  functions (`add`, `sub`, `mul`, `div`, `mod`). Evaluates dynamic expressions into `Value`s
  (does zero AST modification, delegating identifier lookups to `tomet-resolver`).
- **`tomet-resolver`** (`crates/tomet-semantics-resolver`): not a pipeline stage in the same sense as the
  above -- an independent "preprocessor/linker" layer (the closest
  analogy is C's `#include`) for Tomet's file-referencing
  constructs, such as `@config(import: ...)` (and legacy `@settings(file: ...)`):
  given a configuration reference element and a project root, it reads
  and parses the referenced file (recursively invoking
  `tomet_parser::parse_document`) and returns the `Value` held by
  that file's top-level `@config{ ... }` / `@settings{ ... }` block. Also resolves
  same-document `${id}` interpolation lookups and `@references` remote attribute connections.
  This is I/O `tomet-parser` is constitutionally barred from doing (see
  the Deterministic Static Parser Boundary above), so it lives
  outside it; depends on `tomet-ast` and `tomet-parser`.

Everything downstream of `tomet-ast`/`tomet-parser` is a
*consumer* -- it reads the AST (or, for `tomet-markdown`, produces
one) and does not get to redefine what the grammar means. `tomet-html`
and `tomet-markdown` live under crate directories sharing a `tomet-convert-` prefix specifically
because both convert a `Document` to/from an *external* document format (HTML,
CommonMark); `tomet-formatter` and `tomet-printer` live under `tomet-format-` since they
produce and format Tomet's own source text.

- **`tomet-html`** (`crates/tomet-convert-html`): `Document` -> HTML. Generic and data-driven,
  not a full semantic engine: most `<T>`/`@name` elements become a
  `<div>`/`<span>` carrying their `args` map as `data-*` attributes and
  `content` as inner content. A handful of kinds get special-cased rendering
  because the spec gives them fixed meaning (`@(url:..)`/`@(file:..)`/
  `@(tm:..)` as links, `@(id:..)` as a same-document anchor reference,
  `@version`/`@kind`/`@meta`/`@config` as invisible, `@links{}` as a definition list,
  `codeblock`/`blockquote`/`hr`/`em`/`strong`/`mark` with their obvious
  HTML mapping. `@(ref:..)` -- project-wide search by filename/title --
  has no dedicated HTML rendering yet, falling to the generic fallback).
- **`tomet-markdown`** (`crates/tomet-convert-markdown`): bidirectional CommonMark <-> `Document`
  conversion (`import.rs`/`export.rs`), lossy in both directions for
  constructs with no equivalent on the other side -- see
  `docs/design/decisions/2026-08-09-commonmark-support.md` for the mapping and its known-lossy
  cases. Notably depends only on `tomet-ast`, not `tomet-parser`:
  it builds/consumes `Document` values directly and uses `pulldown-cmark`
  for the actual CommonMark side, rather than round-tripping through
  Tomet source text.
- **`tomet-typst`** (`crates/tomet-convert-typst`): one-directional
  `Document -> Typst` markup-source export (no importer). Lossy where
  Typst has no clean equivalent -- see the crate's own module doc for
  the full mapping and its documented lossy cases (dropped heading
  `id`/`cssclass`, `${...}` kept inert rather than evaluated, unrecognized
  `<T>`/`@name` elements reduced to their inner content with a leading
  `// tomet:{kind}` comment, since Typst's normal compile mode has no
  raw-passthrough escape hatch the way CommonMark does). Structurally
  closer to `tomet-html` than to `tomet-markdown`: a single `lib.rs`
  since there's no import side to split out.
- **`serde_tomet`**: `serde` support for the `Value` subset only
  (plain `key: value`, nested `{ }`/`[ ]`, scalars) -- the part of the
  grammar with a direct struct mapping, the same role `serde_json`/
  `serde_yaml` play for their formats. Headings/prose/links have no serde
  equivalent and aren't handled here.
- **`tomet-formatter`** (`crates/tomet-formatter`): AST-aware whitespace and raw-content preserving formatter,
  with layout adjustment for tables.
  1. **Config-driven table layout** (`format_tables_with_config` / `format_source_with_config`):
     formats `@table[...]` column widths and cell alignments according to `PrinterConfig` (e.g.
     `table.adjust_width`, `table.max_col_width`, `table.align`). Never mutates metadata (`@meta`)
     or injects structural elements into the document.
  2. **Whitespace-hygiene pass** (`format_source`, unconditional, always
     runs last): normalizes whitespace policy (LF line endings, no
     trailing whitespace, one final newline, collapsed blank-line runs)
     while using AST `Span` metadata to losslessly preserve literal
     spacing and line breaks inside verbatim content (`<codeblock>[...]`
     or elements with `content:raw`). Guaranteed `does_not_change_the_parsed_document` --
     asserted across the whole shared corpus by the `tomet-tests` package's `roundtrip` target.
- **`tomet-config`** (`crates/tomet-workspace-config`): owns `PrinterConfig`/`FieldConfig` (loaded
  from a `default.config.tmt`/`tomet.config.tmt`, or an
  `@settings`/`@config` element in a document) and everything they
  drive: meta format (yaml/json/toml), link-key spacing (`url`/`link`/
  `ref`), callout/list style, and per-field `@meta` rules. Also owns discovery
  (`find_config_file`, `load_config_from_file`, `load_config_from_str`).
  Split out of `tomet-printer` because finding/loading this config
  is a concern shared by every consumer that needs it
  (`tomet-tui`/`tomet-workspace`/`tomet-indexer` all call
  `find_config_file` directly), not something specific to serializing a
  `Document` back to `.tmt` text -- those crates depend on
  `tomet-config` directly rather than going through
  `tomet-printer` re-exports.
- **`tomet-field-utils`** (`crates/tomet-format-field-utils`): small pure helpers for `@meta` field
  values, driven by `tomet-config`'s `FieldConfig`: id
  generation/validation (`generate_id_for_field`/`is_valid_id_format`)
  and ISO8601/RFC3339 timestamp conversion
  (`is_iso8601`/`format_rfc3339`). None of these touch `Document`/AST --
  a deliberately plain "utils" name rather than an invented abstraction
  like "rules", since it's a grab bag of generate/convert helpers with
  no shared engine behind them (contrast `tomet-style` below, which
  *is* one cohesive family). Split out of `tomet-printer` so
  `tomet-formatter` could reuse the same generate/validate/convert
  logic without depending on printer's whole-document-rebuild model.
- **`tomet-style`** (`crates/tomet-format-style`): applies `tomet-config`'s `PrinterConfig`
  style rules (meta format, link-key spacing, per-field `@meta`
  formatting) to a single `tomet_ast::Value` or `Element`, returning
  `.tmt`-syntax text -- `render_value`/`render_value_inner_with_config`/
  `render_args_with_config` (take only `&Value` + config) and
  `render_meta_element` (takes one `&Element`, renders a whole
  `@meta(...){...}` -- format-arg detection, multi-line vs single-line
  switching, per-field formatting via `tomet-field-utils`).
  Deliberately scoped to single nodes with no `Document`/tree-position
  context and no recursion into other element kinds (that general
  element-tree recursion -- `render_block`/`render_inlines`/the rest of
  `render_element` -- stays in `tomet-printer`, since it's genuinely
  shaped by "rebuild a whole document"). This scoping is what lets both
  `tomet-printer` *and* `tomet-formatter` depend on it without a
  cycle (`tomet-printer` already depends on `tomet-formatter`,
  calling `format_source` at the end of `document_to_tm_with_config`).
- **`tomet-printer`** (`crates/tomet-format-printer`): serializes a `Document` back to `.tmt` source
  text (`document_to_tm`/`document_to_tm_with_config`), no span info
  required -- unlike `tomet-formatter`, which re-formats *existing*
  `.tmt` text losslessly using spans, this is for documents that never
  had `.tmt` source to begin with (built from Markdown, or edited purely
  at the AST level). Uses `tomet-style` for `@meta` rendering and
  all `Value` rendering; keeps the element-tree-recursive rendering
  (`render_block`/`render_heading`/`render_list*`/`render_inlines`/the
  rest of `render_element`) that's specific to rebuilding a whole
  document. Also owns `@meta` id auto-generation
  (`ensure_document_id_with_config`), which mutates the AST using
  `tomet-config`'s `PrinterConfig`/`FieldConfig` rules and
  `tomet-field-utils`'s pure generate/validate functions.
- **`tomet-indexer`** (`crates/tomet-workspace-indexer`): directory scanning and read-only `.tmt`/`.tmt`
  metadata cataloging (`collect_tm_files`, `extract_metadata`,
  `is_path_ignored`). Split out of `tomet-tui`'s
  `engine::batch_meta` module for TUI-independent reuse. Depends on `tomet-config` for
  `PrinterConfig`/`find_config_file`; `apps/cli`'s `export` subcommand uses it directly. Also hosts
  `workspace_scan` (`WorkspaceScan`/`scan_workspace_with_config`, plus the
  `FileTreeNode`/`MigrationItem` types it produces): a single-pass
  directory walk that builds `tomet-tui`'s Explorer tree, Migration
  candidate list, and BatchMeta path list together from one `WalkBuilder`
  traversal instead of three.
- **`tomet-transform`** (`crates/tomet-transform`): pure in-memory AST
  transformations and refactoring rules (I/O-free). Owns macro reverse matching
  (`MacroPattern`/`MacroSet`), directive promotion (`@meta.type` -> `@kind`),
  Value DSL normalization, and structural AST transformations (rename tag,
  rename key, replace value). Depends only on `tomet-ast`, `tomet-semantics`,
  and `tomet-tree`.
- **`tomet-workspace`** (`crates/tomet-workspace`): multi-file batch operations
  and I/O execution across a workspace. Owns the [`FileDiff`] model, workspace-level
  batch refactoring (`refactor_workspace`), structural grep search & replace execution
  (`StructuralEngine`), and batch metadata updates (`BatchMetaEngine`). Discovers files
  via `tomet-indexer`, parses via `tomet-parser`, applies transforms via `tomet-transform`,
  renders via `tomet-printer`/`tomet-formatter`, and handles safe disk saving.
- **`tomet-tui`**: the interactive TUI workbench (ratatui/crossterm) for
  Markdown migration, batch metadata editing, and structural AST refactoring
  across a directory of `.tmt` files. A standalone library crate (single
  entry point `run_tui(dir_path, config_path)`) so it's independently
  buildable/testable rather than living inside the `apps/cli` bin;
  `apps/cli`'s `tui` subcommand just calls into it. All the AST-level
  work (serialization, scanning, transforming, workspace I/O) lives in
  `tomet-printer`/`tomet-indexer`/`tomet-transform`/`tomet-workspace`.
- **`tomet-links`** (`crates/tomet-workspace-links`): broken-link
  checking for a vault of `.tmt`/`.tmt` files. Extracts every `File`/
  `Embed`/`Tm`/`Ref` link (via `tomet-semantics`'s `link_target`),
  caches the extraction per source file keyed by mtime (`LinkCache`,
  SQLite-backed, so re-checking a large vault doesn't re-parse every
  unchanged file), and resolves every target against what actually
  exists on disk.


Source `Span` tracking (line, column, byte offset) is fully integrated across
all AST nodes (`Document`, `Block`, `Inline`, `Element`). This enables precise
source-location queries for tooling such as LSP diagnostics, hover ranges, and
lossless verbatim content formatting.


## The grammar has two independent implementations

`tomet-parser` is the source of truth. `crates/tree-sitter-tomet`
(`grammar.js` at the crate root, generated into `src/parser.c` etc. via
`npx tree-sitter-cli@0.26.12 generate`, compiled by `build.rs`) is a
**second, hand-maintained approximation** used only for editor syntax
highlighting (Zed and other tree-sitter-based editors) -- not a byte-for-
byte match, and known to simplify away things a context-free grammar
can't cheaply express (e.g. no flanking-delimiter whitespace rules for
`*em*`/`**strong**`, no lazy paragraph continuation). See that crate's
module doc for the current list of known gaps. Concretely: **a grammar
change in `tomet-parser` does not automatically show up in editor
syntax highlighting** -- `grammar.js` needs a matching update, and the
crate's own test (`*_fixture_has_only_known_error_cases` tests against
`docs/*.tmt` files) is the way to notice when the two have drifted apart.

## Apps

- **`apps/cli`** (package `tomet`, workspace default member): the CLI. Subcommands:
  `check` (parse, report OK/error), `ast` (pretty-print the parsed AST),
  `roundtrip` (parse a data file -> `serde_tomet` render -> reparse,
  to confirm the save/load round trip is lossless), `html`/`serve`
  (render via `tomet-html`, `serve` re-renders fresh on every HTTP
  request), `to-md` (via `tomet-markdown`), `to-typst` (via `tomet-typst`),
  `format` (via `tomet-formatter`, with `--write`/`--check`), `tui`
  (launches `tomet-tui`'s workbench).
- **`apps/lsp`** (package `tomet-lsp`): a diagnostics, formatting, hover, document symbol, goto definition,
  and completion language server (`lsp-server`/`lsp-types` over stdio, full-document sync).
  Parses the buffer with `tomet-parser` on open/change, validates AST rules with `tomet-validator`,
  and delegates formatting to `tomet-formatter`.
- **`editors/zed`**: a Zed editor extension. Doesn't link any
  `tomet-*` crate directly -- it shells out to a `tomet-lsp`
  binary expected on `$PATH` (e.g. via the Nix package), since
  `tomet-lsp` has no published release binary yet.
- **`editors/vscode`** (TypeScript, not part of the Cargo
  workspace): syntax highlighting via a TextMate grammar
  (`syntaxes/tomet.tmLanguage.json`) plus an LSP client
  (`vscode-languageclient`) that spawns `tomet-lsp` the same way the
  Zed extension does (configurable `serverPath`, defaults to expecting it
  on `$PATH`).
- **`bindings/js`** (package `tomet-js`): JavaScript and WebAssembly (WASM)
  bindings using `wasm-bindgen` and `serde-wasm-bindgen`. Exports `parseDocument`,
  `parseValue`, `toHtml`, `toMarkdown`, `toTypst`, `fromMarkdown`, `printDocument`, `formatSource`,
  and `validate` directly into native JS Objects and typed TypeScript models (`index.d.ts`).
- **`bindings/python`** (package `tomet-python`, module `tomet`): Python bindings
  powered by `pyo3` and `pythonize`. Exports `loads`, `parse_document`, `to_html`,
  `to_markdown`, `to_typst`, `from_markdown`, `print_document`, `format`, and `validate` directly
  into native Python `dict`/`list` objects with comprehensive `.pyi` type stubs.
- **`bindings/java`** (package `tomet-java`, artifact `org.tomet:tomet`): Java and JVM
  bindings powered by JNI and `serde_json`. Exports `Tomet.parseDocumentJson`,
  `Tomet.parseValueJson`, `Tomet.toHtml`, `Tomet.toMarkdown`, `Tomet.toTypst`, `Tomet.fromMarkdownJson`,
  `Tomet.printDocumentJson`, `Tomet.format`, and `Tomet.validateJson` for Java, Kotlin, Android, and Spring applications.
- **`packages/react`** (package `@tomet/react`): React components for Tomet markup and
  AST rendering with custom element overrides (`<Tomet ast={doc} components={{ ... }} />`).
- **`editors/helix`**: Helix editor configuration (`languages.toml`) and Tree-sitter query definitions (`queries/tomet/`), connecting to `tomet-lsp` on `$PATH`.
- **`editors/neovim`**: a Neovim plugin (`tomet.nvim`) providing filetype detection (`.tmt`), Tree-sitter parser registration & queries, buffer settings, and automatic `tomet-lsp` attachment.

## Tests

Layer-local tests live in the crate they test. Tests that span crates, or
that need the shared `.tmt` corpus, live in the `tests/` package
(`tomet-tests`) -- a workspace member that owns `tests/fixtures/` (the
frozen corpus) and `tests/ref/` (committed reference output). It holds
three targets: `corpus` (every fixture parses; the tree-sitter grammar
produces only its documented error cases, which is how that grammar is
noticed drifting from `tomet-parser`), `roundtrip` (parser + formatter
and parser + printer invariants), and `snapshot` (`.tmt` rendered to
HTML/CommonMark/Typst/printed `.tmt`, compared against `ref/`).

This split exists because those tests belong to no single crate: before
it, four crates reached outside their own directory for the corpus. See
`tests/README.md` for the exception lists that record where current
behavior falls short, and for why the corpus is deliberately not synced
with `docs/`.

## Packaging

Nix (`flake.nix`, `nix/`) is the packaging story: `nix/pkgs/tomet.nix`
and `nix/pkgs/vscode-extension.nix` build the CLI and the VS Code
extension respectively. `nix/dev.nix` is the dev shell.
