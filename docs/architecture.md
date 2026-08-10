# Architecture

TypedMark is a small markup language (`.tm` files) plus a Rust toolchain
around it: a parser, an AST, consumers that turn that AST into other
things (HTML, CommonMark, formatted source, serialized data), and a set of
apps/editor integrations built on top. This doc is a map of how those
pieces fit together and depend on each other -- not a spec of the
language itself. For the grammar, `docs/tmt/typedmark.tm` is the running
design-notes doc and closest thing to a source of truth; `docs/cheatsheet.tm`
is a live example file exercised by tests.

## The pipeline

```
typedmark-lexar  (byte-position cursor over &str)
      |
      v
typedmark-ast    (Value / Document / Block / Inline / Element types)
      ^
      |  builds
typedmark-parser (recursive-descent parser: &str -> Document/Value)
```

- **`typedmark-lexar`**: a minimal cursor over `&str`, nothing more. Exists
  because the grammar is context-sensitive -- `<`, `@`, `(`, `[`, `{` are
  only structural right after specific lookaheads (an element trigger),
  plain prose otherwise -- so a hand-rolled cursor is a better fit than
  pre-tokenizing into a context-free stream.
- **`typedmark-ast`**: the shared types every other crate speaks.
  `Value` is the pure-data subset (maps 1:1 onto serde's data model: maps,
  sequences, scalars) -- it's what fills an element's `(input)` or
  `{value}` group, and it's the entire result of parsing a data-only `.tm`
  file. `Document`/`Block`/`Inline`/`Element` are the full markup AST
  (headings, paragraphs, lists, typed elements), in source order. No
  span/position info is carried on these types today -- that's why
  `typedmark-formatter` can't do a lossless AST reprint (see below) and
  has to fall back to a text-level pass instead.
- **`typedmark-parser`**: the actual grammar implementation, and the one
  place that gets to decide what `.tm` source means. Recursive-descent,
  built directly on `typedmark-lexar`'s cursor (see `document.rs`'s module
  doc for the grammar it covers: `<T>(input)[area]{value}`, `@name...`,
  bare `@(key:...)`, and the bare-element children of containers like
  `@links{}`). `value.rs` handles the `Value`-only grammar shared by
  `(input)`/`{value}` bodies and by whole data-only documents.
  `embedded_format.rs` is the escape hatch that lets a `{value}` body be
  real JSON/YAML/TOML source instead of TypedMark's own lightweight
  grammar, driven by a `format` key (locally, or document-wide via
  `@config(format:...)`).

Everything downstream of `typedmark-ast`/`typedmark-parser` is a
*consumer* -- it reads the AST (or, for `typedmark-markdown`, produces
one) and does not get to redefine what the grammar means:

- **`typedmark-renderer`**: `Document` -> HTML. Generic and data-driven,
  not a full semantic engine: most `<T>`/`@name` elements become a
  `<div>`/`<span>` carrying their `input` map as `data-*` attributes and
  `area` as inner content. A handful of kinds get special-cased rendering
  because the spec gives them fixed meaning (`@(url:..)`/`@(file:..)` as
  links, `@(ref:..)` as an anchor reference, `@meta`/`@config` as
  invisible, `@links{}` as a definition list, `codeblock`/`blockquote`/
  `hr`/`em`/`strong`/`mark` with their obvious HTML mapping).
- **`typedmark-markdown`**: bidirectional CommonMark <-> `Document`
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
- **`typedmark-formatter`**: *not* AST-based. `typedmark_ast::Document`
  carries no span info, so a lossless gofmt/prettier-style reprint isn't
  possible today, and hand-duplicating the parser's grammar walk just to
  preserve incidental spacing wasn't judged worth the maintenance cost.
  Instead it's a blind text-level pass that normalizes whitespace policy
  (LF line endings, no trailing whitespace, one final newline, collapsed
  blank-line runs) and claims to be a no-op as far as `parse_document` is
  concerned. That claim does **not** hold inside a raw/verbatim area
  (`<codeblock>[...]`, or any element using the `area:raw` opt-in --
  see `docs/tmt/typedmark.tm`'s `[area]` content-fidelity entry) where
  blank lines and trailing whitespace are literal, preserved content, not
  formatting noise -- a known, currently-unaddressed gap.
- **`typedmark-validator`**: not implemented yet -- currently just the
  `cargo new` boilerplate (`add(left, right)` + its test). Reserved in the
  workspace for future `.tm` schema/lint validation.

`typedmark_ast::Document` carrying no position/span info is a deliberate,
recurring constraint worth remembering: it's why the formatter can't
reprint losslessly, and it's why any future tool that needs to map an AST
node back to a source range (e.g. LSP go-to-definition, precise
diagnostics ranges beyond `typedmark_parser::Error`'s line/column) would
need to either add span tracking to the AST or re-derive positions by
re-scanning source text.

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
  (render via `typedmark-renderer`, `serve` re-renders fresh on every HTTP
  request), `to-md` (via `typedmark-markdown`), `format` (via
  `typedmark-formatter`, with `--write`/`--check`).
- **`apps/typedmark-lsp`**: a diagnostics-and-formatting-only language
  server (`lsp-server`/`lsp-types` over stdio, full-document sync). Parses
  the buffer with `typedmark-parser` on every open/change and republishes
  whatever parse error comes back (or clears diagnostics on a clean
  parse); `textDocument/formatting` delegates to `typedmark-formatter`.
  No hover/completion/goto-definition yet.
- **`apps/zed-extension`**: a Zed editor extension. Doesn't link any
  `typedmark-*` crate directly -- it shells out to a `typedmark-lsp`
  binary expected on `$PATH` (e.g. via the Nix package), since
  `typedmark-lsp` has no published release binary yet.
- **`apps/vscode-extension`** (TypeScript, not part of the Cargo
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
