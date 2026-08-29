# Tomet for VS Code

Syntax highlighting and language server support for Tomet (`.tmt`) files.

## Requirements

This extension does not bundle `tomet-lsp` -- it expects the binary
on `$PATH` (e.g. installed via the repo's nix package), or at the path
set in the `tomet.serverPath` setting.

## Development

```sh
npm install
npm run compile   # or `npm run watch`
```

Then open this directory in VS Code and press F5 to launch an Extension
Development Host.
