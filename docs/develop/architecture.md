# Architecture

TypedMark is a small markup language (`.tm` files) plus a Rust toolchain
around it: a parser, an AST, consumers that turn that AST into other
things (HTML, CommonMark, formatted source, serialized data), and a set of
apps/editor integrations built on top. This doc is a map of how those
pieces fit together and depend on each other -- not a spec of the
language itself. For the grammar, `docs/ja/specifications/*.tm` are the
spec docs (constructs marked `// 未実装` aren't implemented yet);
`docs/ja/cheatsheet.tm` is a live example file exercised by tests. The
core grammar they document is frozen as of `docs/develop/
grammar-freeze.md` -- breaking changes to already-decided syntax go
through that doc's process, not a silent parser diff.

## The pipeline

```
typedmark-lexar  (byte-position cursor over &str)
      |
      v
typedmark-ast    (Value / Document / Block / Inline / Element types)
      ^
      |  builds
typedmark-parser (recursive-descent parser: &str -> Document/Value)
      |
      v
typedmark-semantics (I/O-free classification of what an Element means)
```

- **`typedmark-lexar`**: a minimal cursor over `&str`, nothing more. Exists
  because the grammar is context-sensitive -- `<`, `@`, `(`, `[`, `{` are
  only structural right after specific lookaheads (an element trigger),
  plain prose otherwise -- so a hand-rolled cursor is a better fit than
  pre-tokenizing into a context-free stream.
- **`typedmark-ast`**: the shared types every other crate speaks.
  `Value` is the pure-data subset (maps 1:1 onto serde's data model: maps,
  sequences, scalars) -- it's what fills an element's `(args)` or
  `{value}` group, and it's the entire result of parsing a data-only `.tm`
  file. `Document`/`Block`/`Inline`/`Element` are the full markup AST
  (headings, paragraphs, lists, typed elements), in source order. Every AST
  node carries a source `Span` (line, column, byte offset), allowing downstream
  consumers like `typedmark-formatter` and `typedmark-lsp` to perform AST-aware
  formatting and exact diagnostic range mapping.
- **`typedmark-parser`**: the actual grammar implementation, and the one
  place that gets to decide what `.tm` source means. Recursive-descent,
  built directly on `typedmark-lexar`'s cursor (see `document.rs`'s module
  doc for the grammar it covers: `<T>(args)[content]{value}`, `@name...`,
  bare `@(key:...)`, and the bare-element children of containers like
  `@links{}`). `value.rs` handles the `Value`-only grammar shared by
  `(args)`/`{value}` bodies and by whole data-only documents.
  `embedded_format.rs` is the escape hatch that lets a `{value}` body be
  real JSON/YAML/TOML source instead of TypedMark's own lightweight
  grammar, driven by a `format` key (locally, or document-wide via
  `@config(format:...)`).

  **Deterministic Static Parser Boundary**:
  `typedmark-parser` is strictly a pure, side-effect-free, deterministic static
  parser. It performs zero I/O, external file resolution, or dynamic code execution.
  Given identical input text, it produces identical AST output with guaranteed linear/predictable time complexity.
- **`typedmark-semantics`**: I/O-free classification of what a parsed
  `Element` officially means -- `url`/`file`/`tm`/`id`/`ref` inference
  from a bare `@(key:...)` (`infer_at_kind`), plus shape-based inference
  for the two forms that carry no `key:` at all, `@(/some/path)` and
  `@(https://example.com)` (`infer_at_kind_from_scalar`; the grammar
  work that makes these parse at all lives in `typedmark-parser`'s
  `value.rs` -- see `docs/reviews/2026-08-22-link-reference-uri-schemes.md`
  section 5's "Group B"), and recognizing TypedMark's own built-in
  vocabulary (`@meta`, `@config`, `@links`, `em`/`strong`/`mark`, ...) via
  an `ElementKind` enum and a `classify(el: &Element) -> ElementKind`
  function. Depends only on `typedmark-ast` (not `typedmark-parser` --
  the diagram above shows the pipeline's logical ordering, not a Cargo
  dependency edge), so any consumer holding an `Element` can classify it
  without pulling in the parser. Exists so `typedmark-html` and
  `typedmark-markdown` don't each carry their own copy of this
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
  `typedmark-html`, `typedmark-markdown`, and `typedmark-links` all
  route through instead of each reading `el.args` (and disagreeing on
  fallback order) independently -- see
  `docs/reviews/2026-08-22-link-reference-uri-schemes.md` for the design
  history behind unifying this.

Everything downstream of `typedmark-ast`/`typedmark-parser` is a
*consumer* -- it reads the AST (or, for `typedmark-markdown`, produces
one) and does not get to redefine what the grammar means. `typedmark-html`
and `typedmark-markdown` live under crate names sharing a `typedmark-codegen-` prefix specifically
because both convert a `Document` to/from an *external* format (HTML,
CommonMark); `typedmark-formatter` stays outside that group since it
converts `Document` back into TypedMark's own source, not another format.

- **`typedmark-html`** (`crates/typedmark-codegen-html`): `Document` -> HTML. Generic and data-driven,
  not a full semantic engine: most `<T>`/`@name` elements become a
  `<div>`/`<span>` carrying their `args` map as `data-*` attributes and
  `content` as inner content. A handful of kinds get special-cased rendering
  because the spec gives them fixed meaning (`@(url:..)`/`@(file:..)`/
  `@(tm:..)` as links, `@(id:..)` as a same-document anchor reference,
  `@meta`/`@config` as invisible, `@links{}` as a definition list,
  `codeblock`/`blockquote`/`hr`/`em`/`strong`/`mark` with their obvious
  HTML mapping. `@(ref:..)` -- project-wide search by filename/title --
  has no dedicated HTML rendering yet, falling to the generic fallback).
- **`typedmark-markdown`** (`crates/typedmark-codegen-markdown`): bidirectional CommonMark <-> `Document`
  conversion (`import.rs`/`export.rs`), lossy in both directions for
  constructs with no equivalent on the other side -- see
  `docs/feature/commonmark-support.md` for the mapping and its known-lossy
  cases. Notably depends only on `typedmark-ast`, not `typedmark-parser`:
  it builds/consumes `Document` values directly and uses `pulldown-cmark`
  for the actual CommonMark side, rather than round-tripping through
  TypedMark source text.
- **`serde_typedmark`**: `serde` support for the `Value` subset only
  (plain `key: value`, nested `{ }`/`[ ]`, scalars) -- the part of the
  grammar with a direct struct mapping, the same role `serde_json`/
  `serde_yaml` play for their formats. Headings/prose/links have no serde
  equivalent and aren't handled here.
- **`typedmark-formatter`**: AST-aware whitespace and raw-content preserving formatter,
  plus (as of `format_source_with_config`) an opt-in, config-gated pass that makes
  small structural additions. Two-stage, config-gated pipeline:
  1. **Config-driven patch pass** (only runs if the relevant
     `typedmark-config` rule is set on the document -- e.g.
     `meta_fields.id`; a document with no such config is untouched by
     this stage, so `format_source_with_config` degrades to exactly
     `format_source`'s behavior). Walks the parsed `Document` to find
     the target `@meta` element (or decide none exists), builds the new
     `id` state (insert if missing and `force`/`overwrite`; replace an
     invalid one only if `overwrite`; leave a valid one alone -- the
     same per-case gating as `typedmark-printer`'s
     `ensure_document_id_with_config`), renders that one element via
     `typedmark-style`'s `render_meta_element`, then splices the result
     into the *original* source at that element's `Span` (or prepends a
     freshly rendered block if there was no `@meta` element at all) --
     never rebuilds the whole document from the AST (that's
     `typedmark-printer`'s job, and doing it here would defeat the
     reason this crate exists: not touching content it wasn't told to
     change). Note: when this pass *is* triggered, the touched element
     is rendered exactly as `typedmark-printer` would render it -- e.g.
     a hand-written single-line `@meta{id: ...}` can come out multi-line
     as `@meta(format:yaml){...}` if `config.meta_format` says so. This
     is intentional: the losslessness guarantee is about *not touching*
     elements/documents with no matching config rule, not about
     preserving a touched element's original shape once config *does*
     apply to it.
  2. **Whitespace-hygiene pass** (`format_source`, unconditional, always
     runs last): normalizes whitespace policy (LF line endings, no
     trailing whitespace, one final newline, collapsed blank-line runs)
     while using AST `Span` metadata to losslessly preserve literal
     spacing and line breaks inside verbatim content (`<codeblock>[...]`
     or elements with `content:raw`). This stage alone still guarantees
     `does_not_change_the_parsed_document` (its own invariant test) --
     that invariant does not extend to stage 1, which exists precisely
     to make deliberate, config-authorized changes.
- **`typedmark-config`**: owns `PrinterConfig`/`FieldConfig` (loaded
  from a `default.config.tm`/`typedmark.config.tm`, or an
  `@settings`/`@config` element in a document) and everything they
  drive: meta format (yaml/json/toml), link-key spacing (`url`/`link`/
  `ref`), callout/list style, and per-field `@meta` rules. Also owns discovery
  (`find_config_file`, `load_config_from_file`, `load_config_from_str`).
  Split out of `typedmark-printer` because finding/loading this config
  is a concern shared by every consumer that needs it
  (`typedmark-tui`/`typedmark-edit`/`typedmark-indexer` all call
  `find_config_file` directly), not something specific to serializing a
  `Document` back to `.tm` text -- those crates depend on
  `typedmark-config` directly rather than going through
  `typedmark-printer` re-exports. `@meta` id auto-generation stayed
  behind in `typedmark-printer` despite being config-driven, since it
  mutates the AST directly rather than being a config-loading concern.
- **`typedmark-field-utils`**: small pure helpers for `@meta` field
  values, driven by `typedmark-config`'s `FieldConfig`: id
  generation/validation (`generate_id_for_field`/`is_valid_id_format`)
  and ISO8601/RFC3339 timestamp conversion
  (`is_iso8601`/`format_rfc3339`). None of these touch `Document`/AST --
  a deliberately plain "utils" name rather than an invented abstraction
  like "rules", since it's a grab bag of generate/convert helpers with
  no shared engine behind them (contrast `typedmark-style` below, which
  *is* one cohesive family). Split out of `typedmark-printer` so
  `typedmark-formatter` could reuse the same generate/validate/convert
  logic without depending on printer's whole-document-rebuild model.
- **`typedmark-style`**: applies `typedmark-config`'s `PrinterConfig`
  style rules (meta format, link-key spacing, per-field `@meta`
  formatting) to a single `typedmark_ast::Value` or `Element`, returning
  `.tm`-syntax text -- `render_value`/`render_value_inner_with_config`/
  `render_args_with_config` (take only `&Value` + config) and
  `render_meta_element` (takes one `&Element`, renders a whole
  `@meta(...){...}` -- format-arg detection, multi-line vs single-line
  switching, per-field formatting via `typedmark-field-utils`).
  Deliberately scoped to single nodes with no `Document`/tree-position
  context and no recursion into other element kinds (that general
  element-tree recursion -- `render_block`/`render_inlines`/the rest of
  `render_element` -- stays in `typedmark-printer`, since it's genuinely
  shaped by "rebuild a whole document"). This scoping is what lets both
  `typedmark-printer` *and* `typedmark-formatter` depend on it without a
  cycle (`typedmark-printer` already depends on `typedmark-formatter`,
  calling `format_source` at the end of `document_to_tm_with_config`).
- **`typedmark-printer`**: serializes a `Document` back to `.tm` source
  text (`document_to_tm`/`document_to_tm_with_config`), no span info
  required -- unlike `typedmark-formatter`, which re-formats *existing*
  `.tm` text losslessly using spans, this is for documents that never
  had `.tm` source to begin with (built from Markdown, or edited purely
  at the AST level). Uses `typedmark-style` for `@meta` rendering and
  all `Value` rendering; keeps the element-tree-recursive rendering
  (`render_block`/`render_heading`/`render_list*`/`render_inlines`/the
  rest of `render_element`) that's specific to rebuilding a whole
  document. Also owns `@meta` id auto-generation
  (`ensure_document_id_with_config`), which mutates the AST using
  `typedmark-config`'s `PrinterConfig`/`FieldConfig` rules and
  `typedmark-field-utils`'s pure generate/validate functions. Split out
  of `typedmark-tui`'s `engine::printer` module so it's usable outside
  the TUI.
- **`typedmark-indexer`**: directory scanning and read-only `.tm`/`.tmt`
  metadata cataloging (`collect_tm_files`, `extract_metadata`,
  `is_path_ignored`). Split out of `typedmark-tui`'s
  `engine::batch_meta` module (`is_path_ignored` had been misplaced in
  `engine::printer` -- it's a file filter, not a print concern) for the
  same reason as `typedmark-printer`: TUI-independent reuse (a future
  search/browse feature, e.g.). Depends on `typedmark-config` for
  `PrinterConfig`/`find_config_file` (the config-auto-discovering
  `collect_tm_files` wrapper needs them), not on `typedmark-printer`
  itself; `apps/cli`'s `export` subcommand uses it directly. Also hosts
  `workspace_scan` (`WorkspaceScan`/`scan_workspace_with_config`, plus the
  `FileTreeNode`/`MigrationItem` types it produces): a single-pass
  directory walk that builds `typedmark-tui`'s Explorer tree, Migration
  candidate list, and BatchMeta path list together from one `WalkBuilder`
  traversal instead of three. Kept content-free (`MigrationItem.markdown_src`/
  `typedmark_src` start empty, loaded lazily) so this module needs no
  `typedmark-markdown`/`typedmark-printer` dependency.
- **`typedmark-edit`**: editing operations over a directory of `.tm`
  files -- batch `@meta`/`@config` key updates (`batch_meta`) and
  AST-aware structural search & replace: rename tag, rename key, replace
  value (`structural`). Both follow the same shape: scan (via
  `typedmark-indexer`, config via `typedmark-config`) -> mutate the
  parsed `Document` in place (via `typedmark-walker`'s mutable walk) ->
  re-serialize (via `typedmark-printer`) -> `save()` writes changed
  files to disk. Split out of `typedmark-tui`'s
  `engine::batch_meta`/`engine::structural` modules, same
  TUI-independent-reuse motivation as `typedmark-printer`/
  `typedmark-indexer`.
- **`typedmark-tui`**: the interactive TUI workbench (ratatui/crossterm) for
  Markdown migration, batch metadata editing, and structural AST refactoring
  across a directory of `.tm` files. A standalone library crate (single
  entry point `run_tui(dir_path, config_path)`) so it's independently
  buildable/testable rather than living inside the `apps/cli` bin;
  `apps/cli`'s `tui` subcommand just calls into it. All the AST-level
  work (serialization, scanning, editing) now lives in `typedmark-printer`/
  `typedmark-indexer`/`typedmark-edit` -- including the workspace-wide
  directory scan itself, which lives in `typedmark-indexer::workspace_scan`.
  What's left in this crate's own `engine::migration` module is only the
  Markdown->`.tm` conversion step (`MigrationEngine::execute`/
  `execute_with_config`, plus `ensure_item_loaded`/`ensure_item_loaded_with_config`
  since `MigrationItem` is now a foreign type and can't gain new inherent
  methods here) -- it needs `typedmark-markdown`/`typedmark-printer`,
  dependencies the scan itself doesn't, which is why conversion stays
  app-side while scanning moved out. The ratatui UI layer (`app`/`ui`)
  ties it all together.
- **`typedmark-walker`**: generic recursive traversal of a `Document`'s
  tree of `Element`s -- a heading is an `Element` classified `"heading"`
  by `typedmark-semantics::classify`, and a list item is an
  `Element{ sigil: Sigil::Bare, .. }` nested inside its list's
  `ElementValue::Children` (see `typedmark_ast::Element::list`/
  `Element::list_item`), neither a dedicated node kind, including ones
  nested inside an element's `[content]`, `children`, and
  `ElementValue::Children`), depending on nothing but `typedmark-ast`.
  Exists because `typedmark-validator` and `typedmark-resolver` each
  independently hand-rolled the same tree-walk shape for unrelated reasons
  (duplicate-id collection vs. `${id}` lookup) -- the same "don't let two
  consumers silently reimplement the same thing" motivation
  `typedmark-semantics` was extracted for. Consumers implement a
  `Visitor<B>` (one `visit(&Element) -> ControlFlow<B>` method) and get to
  either collect everything (`Continue` always) or stop at the first match
  and carry a result out through `Break(b)`.
- **`typedmark-validator`**: `.tm` schema/lint validation. Currently one
  rule -- duplicate `{id:...}`/`(id:...)` detection across a `Document`,
  built on `typedmark-walker`. Read-only: no I/O, no reference resolution
  (that's `typedmark-resolver`), no computation (`typedmark-compute`).
- **`typedmark-resolver`**: not a pipeline stage in the same sense as the
  above -- an independent "preprocessor/linker" layer (the closest
  analogy is C's `#include`) for TypedMark's own file-referencing
  constructs, today just `@settings(file:...)`: given a `@settings(file:
  "path/to/other.tm")` reference element and a project root, it reads
  and parses the referenced file (recursively invoking
  `typedmark_parser::parse_document`) and returns the `Value` held by
  that file's own top-level `@settings{ ... }` block. This is exactly
  the I/O `typedmark-parser` is constitutionally barred from doing (see
  the Deterministic Static Parser Boundary above), so it has to live
  outside it; depends on `typedmark-ast` and `typedmark-parser`, but
  deliberately not `typedmark-semantics` (recognizing a `@settings(
  file:...)` reference only needs a direct `Sigil` match, not full
  classification). `@import` is anticipated but not yet designed --
  see the module doc in `crates/typedmark-doc-resolver/src/lib.rs` for the
  open questions.
- **`typedmark-links`** (`crates/typedmark-doc-links`): broken-link
  checking for a vault of `.tm`/`.tmt` files. Extracts every `File`/
  `Embed`/`Tm`/`Ref` link (via `typedmark-semantics`'s `link_target`,
  see below; `Url`/`Id` are out of scope -- external URLs need network
  requests, and `id:` is a same-document lookup with no cross-file
  aspect, better suited to `typedmark-validator` as a future lint),
  caches the extraction per source file keyed by mtime (`LinkCache`,
  SQLite-backed, so re-checking a large vault doesn't re-parse every
  unchanged file), and resolves every target against what actually
  exists on disk. Like `typedmark-resolver`, this is I/O the parser is
  constitutionally barred from doing, so it lives in its own crate.


Source `Span` tracking (line, column, byte offset) is fully integrated across
all AST nodes (`Document`, `Block`, `Inline`, `Element`). This enables precise
source-location queries for tooling such as LSP diagnostics, hover ranges, and
lossless verbatim content formatting.


## The grammar has two independent implementations

`typedmark-parser` is the source of truth. `crates/tree-sitter-typedmark`
(`grammar.js` at the crate root, generated into `src/parser.c` etc. via
`npx tree-sitter-cli@0.26.12 generate`, compiled by `build.rs`) is a
**second, hand-maintained approximation** used only for editor syntax
highlighting (Zed and other tree-sitter-based editors) -- not a byte-for-
byte match, and known to simplify away things a context-free grammar
can't cheaply express (e.g. no flanking-delimiter whitespace rules for
`*em*`/`**strong**`, no lazy paragraph continuation). See that crate's
module doc for the current list of known gaps. Concretely: **a grammar
change in `typedmark-parser` does not automatically show up in editor
syntax highlighting** -- `grammar.js` needs a matching update, and the
crate's own test (`*_fixture_has_only_known_error_cases` tests against
`docs/*.tm` files) is the way to notice when the two have drifted apart.

## Apps

- **`apps/cli`** (package `typedmark`, workspace default member): the CLI. Subcommands:
  `check` (parse, report OK/error), `ast` (pretty-print the parsed AST),
  `roundtrip` (parse a data file -> `serde_typedmark` render -> reparse,
  to confirm the save/load round trip is lossless), `html`/`serve`
  (render via `typedmark-html`, `serve` re-renders fresh on every HTTP
  request), `to-md` (via `typedmark-markdown`), `format` (via
  `typedmark-formatter`, with `--write`/`--check`), `tui` (launches
  `typedmark-tui`'s workbench).
- **`apps/lsp`** (package `typedmark-lsp`): a diagnostics, formatting, hover, document symbol, goto definition,
  and completion language server (`lsp-server`/`lsp-types` over stdio, full-document sync).
  Parses the buffer with `typedmark-parser` on open/change, validates AST rules with `typedmark-validator`,
  and delegates formatting to `typedmark-formatter`.
- **`editors/zed`**: a Zed editor extension. Doesn't link any
  `typedmark-*` crate directly -- it shells out to a `typedmark-lsp`
  binary expected on `$PATH` (e.g. via the Nix package), since
  `typedmark-lsp` has no published release binary yet.
- **`editors/vscode`** (TypeScript, not part of the Cargo
  workspace): syntax highlighting via a TextMate grammar
  (`syntaxes/typedmark.tmLanguage.json`) plus an LSP client
  (`vscode-languageclient`) that spawns `typedmark-lsp` the same way the
  Zed extension does (configurable `serverPath`, defaults to expecting it
  on `$PATH`).
- **`editors/helix`, `editors/neovim`, `bindings/java`, `bindings/js`,
  `bindings/python`**: workspace members reserved for future
  editor/language-binding support, currently all unimplemented
  `cargo new` stubs (just the generated `add(left, right)` function and
  its test) with no `typedmark-*` dependencies wired up yet.

## Packaging

Nix (`flake.nix`, `nix/`) is the packaging story: `nix/pkgs/typedmark.nix`
and `nix/pkgs/vscode-extension.nix` build the CLI and the VS Code
extension respectively. `nix/dev.nix` is the dev shell.
