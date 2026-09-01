# tomet.nvim

Neovim plugin providing syntax highlighting, Tree-sitter queries, filetype detection, and language server support for Tomet (`.tmt`) files.

## Features

- **Filetype Detection:** Automatically detects `.tmt` files as `tomet` filetype.
- **Tree-sitter Support:** Includes `highlights.scm`, `indents.scm`, `brackets.scm`, and `injections.scm`.
- **LSP Auto-Attach:** Automatically connects `tomet-lsp` to Tomet buffers when available on `$PATH`.
- **Buffer Settings:** Configures comments (`//`, `/* */`) and standard 2-space indentation.

## Requirements

- Neovim >= 0.8.0
- `tomet-lsp` binary on `$PATH` (e.g. built via `cargo install --path apps/lsp` or via Nix).
- `nvim-treesitter` (optional, recommended for advanced syntax highlighting).

## Installation

### Using [lazy.nvim](https://github.com/folke/lazy.nvim)

```lua
{
  "tefww/tomet",
  ft = { "tomet" },
  config = function()
    require("tomet").setup({
      -- Options (defaults shown):
      lsp = true,
      lsp_opts = {
        cmd = { "tomet-lsp" },
      },
    })
  end,
}
```

Or when developing locally:

```lua
{
  dir = "/path/to/tomet/editors/neovim",
  ft = { "tomet" },
  config = function()
    require("tomet").setup()
  end,
}
```

### Using [packer.nvim](https://github.com/wbthomason/packer.nvim)

```lua
use({
  "tefww/tomet",
  config = function()
    require("tomet").setup()
  end,
})
```

## Configuration

Pass options to `require("tomet").setup(opts)`:

```lua
require("tomet").setup({
  -- Automatically attach tomet-lsp when opening .tmt files (default: true)
  lsp = true,
  lsp_opts = {
    cmd = { "tomet-lsp" },
    on_attach = function(client, bufnr)
      -- Custom keymaps or LSP buffer setup
    end,
    capabilities = require("cmp_nvim_lsp").default_capabilities(),
  },
})
```

## Tree-sitter Installation

If using `nvim-treesitter`, `tomet.nvim` automatically registers the parser configuration. Run:

```vim
:TSInstall tomet
```
