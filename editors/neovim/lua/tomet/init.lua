local M = {}

--- Setup function for tomet.nvim
--- @param opts table|nil Configuration options
function M.setup(opts)
  opts = opts or {}

  -- Register tree-sitter parser if nvim-treesitter is available
  local has_ts, parsers = pcall(require, "nvim-treesitter.parsers")
  if has_ts and parsers.get_parser_configs then
    local parser_config = parsers.get_parser_configs()
    if not parser_config.tomet then
      parser_config.tomet = {
        install_info = {
          url = opts.treesitter_url or "https://github.com/tefww/tomet",
          files = { "src/parser.c", "src/scanner.c" },
          branch = "main",
          location = "crates/tree-sitter-tomet",
          generate_requires_npm = false,
          requires_generate_from_grammar = false,
        },
        filetype = "tomet",
      }
    end
  end

  -- Auto-start LSP unless explicitly disabled
  if opts.lsp ~= false then
    local lsp = require("tomet.lsp")
    lsp.setup(opts.lsp_opts or {})
  end
end

return M
