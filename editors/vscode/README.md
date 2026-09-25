# Tomet for VS Code

Syntax highlighting and language server support for
[Tomet](https://github.com/tomet-lang/tomet) (`.tmt`) files.

## Features

- Syntax highlighting for `.tmt` (and `.tm`) files.
- Language server features (diagnostics, formatting, folding, ...) provided by
  `tomet-lsp`.
- Format on save is enabled by default for Tomet files.
- Table columns are underlined in alternating colors; see
  `tomet.table.columnColors` below.
- Smart Enter key handling inside Tomet documents.

## Requirements

This extension does **not** bundle the language server. Install `tomet-lsp`
and make sure it is on your `$PATH`:

```sh
cargo install --git https://github.com/tomet-lang/tomet tomet-lsp
```

Alternatively, point the `tomet.serverPath` setting at the binary. Without the
server you still get syntax highlighting.

## Settings

| Setting                     | Default      | Description                                                                                                                    |
| --------------------------- | ------------ | ------------------------------------------------------------------------------------------------------------------------------ |
| `tomet.serverPath`          | `tomet-lsp`  | Path to the `tomet-lsp` binary.                                                                                                |
| `tomet.trace.server`        | `off`        | Traces the communication between VS Code and the server (`off`, `messages`, `verbose`).                                        |
| `tomet.table.columnColors`  | `[]`         | Colors for the table column underlines. When empty, the theme's `editorBracketHighlight.foreground1..3` colors are used in a cycle. |

## Commands

- **Tomet: Restart Language Server** (`tomet.restartServer`)

## License

Licensed under either of MIT or Apache-2.0, at your option.

## Development

```sh
npm install
npm run compile   # or `npm run watch`
```

Then open this directory in VS Code and press F5 to launch an Extension
Development Host.

To build an installable package: `npx @vscode/vsce package`.
