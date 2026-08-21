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
  `Element` officially means -- `url`/`file`/`ref` inference from a bare
  `@(key:...)`, and recognizing TypedMark's own built-in vocabulary
  (`@meta`, `@config`, `@links`, `em`/`strong`/`mark`, ...) via an
  `ElementKind` enum and a `classify(el: &Element) -> ElementKind`
  function. Depends only on `typedmark-ast` (not `typedmark-parser` --
  the diagram above shows the pipeline's logical ordering, not a Cargo
  dependency edge), so any consumer holding an `Element` can classify it
  without pulling in the parser. Exists so `typedmark-html` and
  `typedmark-markdown` don't each carry their own copy of this
  recognition logic, which is what "what does this kind mean" would
  otherwise silently drift into being (both used to independently
  reimplement it as a `Sigil` match returning a stringly-typed `kind:
  String`) -- the TUI's structural-search engine
  (`crates/typedmark-tui/src/engine`) uses it the same way. Only expresses
  *recognition*, not *action*: whether a given kind's output is empty
  (`@meta`/`@config`) is still each consumer's own call, since the same
  kind can mean different things for different output formats.

Everything downstream of `typedmark-ast`/`typedmark-parser` is a
*consumer* -- it reads the AST (or, for `typedmark-markdown`, produces
one) and does not get to redefine what the grammar means. `typedmark-html`
and `typedmark-markdown` live under `crates/converters/` specifically
because both convert a `Document` to/from an *external* format (HTML,
CommonMark); `typedmark-formatter` stays outside that group since it
converts `Document` back into TypedMark's own source, not another format.

- **`typedmark-html`** (`crates/converters/html`): `Document` -> HTML. Generic and data-driven,
  not a full semantic engine: most `<T>`/`@name` elements become a
  `<div>`/`<span>` carrying their `args` map as `data-*` attributes and
  `content` as inner content. A handful of kinds get special-cased rendering
  because the spec gives them fixed meaning (`@(url:..)`/`@(file:..)` as
  links, `@(ref:..)` as an anchor reference, `@meta`/`@config` as
  invisible, `@links{}` as a definition list, `codeblock`/`blockquote`/
  `hr`/`em`/`strong`/`mark` with their obvious HTML mapping).
- **`typedmark-markdown`** (`crates/converters/markdown`): bidirectional CommonMark <-> `Document`
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
- **`typedmark-formatter`**: AST-aware whitespace and raw-content preserving formatter.
  Normalizes whitespace policy (LF line endings, no trailing whitespace, one final
  newline, collapsed blank-line runs) while using AST `Span` metadata to losslessly
  preserve literal spacing and line breaks inside verbatim content (`<codeblock>[...]`
  or elements with `content:raw`).
- **`typedmark-printer`**: serializes a `Document` back to `.tm` source
  text (`document_to_tm`/`document_to_tm_with_config`), no span info
  required -- unlike `typedmark-formatter`, which re-formats *existing*
  `.tm` text losslessly using spans, this is for documents that never
  had `.tm` source to begin with (built from Markdown, or edited purely
  at the AST level). Also owns `PrinterConfig` (loaded from a
  `default.config.tm`/`typedmark.config.tm`) and everything it drives:
  meta format (yaml/json/toml), wikilink/link spacing, callout/list
  style, and `@meta` id auto-generation. Split out of `typedmark-tui`'s
  `engine::printer` module so it's usable outside the TUI.
- **`typedmark-indexer`**: directory scanning and read-only `.tm`/`.tmt`
  metadata cataloging (`collect_tm_files`, `extract_metadata`,
  `is_path_ignored`). Split out of `typedmark-tui`'s
  `engine::batch_meta` module (`is_path_ignored` had been misplaced in
  `engine::printer` -- it's a file filter, not a print concern) for the
  same reason as `typedmark-printer`: TUI-independent reuse (a future
  search/browse feature, e.g.). Depends on `typedmark-printer` for
  `PrinterConfig`/`find_config_file` (the config-auto-discovering
  `collect_tm_files` wrapper needs them); `apps/typedmark`'s `export`
  subcommand uses it directly.
- **`typedmark-tui`**: the interactive TUI workbench (ratatui/crossterm) for
  Markdown migration, batch metadata editing, and structural AST refactoring
  across a directory of `.tm` files. A standalone library crate (single
  entry point `run_tui(dir_path, config_path)`) so it's independently
  buildable/testable rather than living inside the `apps/typedmark` bin;
  `apps/typedmark`'s `tui` subcommand just calls into it. Built on
  `typedmark-printer`/`typedmark-indexer` for serialization/scanning;
  what's left in `engine::batch_meta`/`engine::structural` is the
  *editing* half (renaming tags/keys, replacing values, updating `@meta`
  keys) -- a future `typedmark-edit` extraction candidate, not done yet.
- **`typedmark-walker`**: generic recursive traversal of a `Document`'s
  tree (`Heading`/`ListItem`/`Element`, including ones nested inside an
  element's `[content]` and `ElementValue::Children`), depending on
  nothing but `typedmark-ast`. Exists because `typedmark-validator` and
  `typedmark-resolver` each independently hand-rolled the same tree-walk
  shape for unrelated reasons (duplicate-id collection vs. `${id}`
  lookup) -- the same "don't let two consumers silently reimplement the
  same thing" motivation `typedmark-semantics` was extracted for.
  Consumers implement a `Visitor<B>` (one `visit(Node) ->
  ControlFlow<B>` method) and get to either collect everything
  (`Continue` always) or stop at the first match and carry a result out
  through `Break(b)`.
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
  see the module doc in `crates/typedmark-resolver/src/lib.rs` for the
  open questions.


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

- **`apps/typedmark`** (workspace default member): the CLI. Subcommands:
  `check` (parse, report OK/error), `ast` (pretty-print the parsed AST),
  `roundtrip` (parse a data file -> `serde_typedmark` render -> reparse,
  to confirm the save/load round trip is lossless), `html`/`serve`
  (render via `typedmark-html`, `serve` re-renders fresh on every HTTP
  request), `to-md` (via `typedmark-markdown`), `format` (via
  `typedmark-formatter`, with `--write`/`--check`), `tui` (launches
  `typedmark-tui`'s workbench).
- **`apps/typedmark-lsp`**: a diagnostics, formatting, hover, document symbol, goto definition,
  and completion language server (`lsp-server`/`lsp-types` over stdio, full-document sync).
  Parses the buffer with `typedmark-parser` on open/change, validates AST rules with `typedmark-validator`,
  and delegates formatting to `typedmark-formatter`.
- **`apps/integrations/zed`**: a Zed editor extension. Doesn't link any
  `typedmark-*` crate directly -- it shells out to a `typedmark-lsp`
  binary expected on `$PATH` (e.g. via the Nix package), since
  `typedmark-lsp` has no published release binary yet.
- **`apps/integrations/vscode`** (TypeScript, not part of the Cargo
  workspace): syntax highlighting via a TextMate grammar
  (`syntaxes/typedmark.tmLanguage.json`) plus an LSP client
  (`vscode-languageclient`) that spawns `typedmark-lsp` the same way the
  Zed extension does (configurable `serverPath`, defaults to expecting it
  on `$PATH`).
- **`apps/typedmark-helix`, `typedmark-java`, `typedmark-js`,
  `typedmark-neovim`, `typedmark-python`**: workspace members reserved for
  future editor/language-binding support, currently all unimplemented
  `cargo new` stubs (just the generated `add(left, right)` function and
  its test) with no `typedmark-*` dependencies wired up yet.

## Packaging

Nix (`flake.nix`, `nix/`) is the packaging story: `nix/pkgs/typedmark.nix`
and `nix/pkgs/vscode-extension.nix` build the CLI and the VS Code
extension respectively. `nix/dev.nix` is the dev shell.
