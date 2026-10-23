-- Buffer-local settings for motem (.mtm) buffers

-- Comment settings: motem has line comments only
vim.bo.commentstring = "// %s"
vim.bo.comments = "://"

-- Indentation settings
vim.bo.tabstop = 2
vim.bo.shiftwidth = 2
vim.bo.softtabstop = 2
vim.bo.expandtab = true

-- Start Tree-sitter highlighting if available
if vim.treesitter then
  pcall(vim.treesitter.start)
end
