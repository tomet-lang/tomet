# TypedMark for VS Code

Syntax highlighting and language server support for TypedMark (`.tm`) files.

## Requirements

This extension does not bundle `typedmark-lsp` -- it expects the binary
on `$PATH` (e.g. installed via the repo's nix package), or at the path
set in the `typedmark.serverPath` setting.

## Development

```sh
bun install
bun run compile   # or `bun run watch`
```

Then open this directory in VS Code and press F5 to launch an Extension
Development Host.
