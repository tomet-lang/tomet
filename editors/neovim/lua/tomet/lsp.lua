local M = {}

--- Setup auto-attaching LSP for tomet buffers
--- @param opts table|nil Configuration options for tomet-lsp
function M.setup(opts)
  opts = opts or {}
  local cmd = opts.cmd or { "tomet-lsp" }

  vim.api.nvim_create_autocmd("FileType", {
    group = vim.api.nvim_create_augroup("tomet_lsp", { clear = true }),
    pattern = "tomet",
    callback = function(ev)
      local executable = type(cmd) == "table" and cmd[1] or cmd
      if vim.fn.executable(executable) ~= 1 then
        return
      end

      local bufname = vim.api.nvim_buf_get_name(ev.buf)
      local root_dir = vim.fs.root(ev.buf, { "tomet.config.tmt", "default.config.tmt", ".git" })
      if not root_dir and bufname ~= "" then
        root_dir = vim.fs.dirname(bufname)
      end

      local config = {
        name = "tomet-lsp",
        cmd = type(cmd) == "table" and cmd or { cmd },
        root_dir = root_dir,
        settings = opts.settings or {},
        init_options = opts.init_options or {},
      }

      if opts.on_attach then
        config.on_attach = opts.on_attach
      end

      if opts.capabilities then
        config.capabilities = opts.capabilities
      end

      vim.lsp.start(config, {
        bufnr = ev.buf,
        reuse_client = function(client, conf)
          return client.name == conf.name and client.config.root_dir == conf.root_dir
        end,
      })
    end,
  })
end

return M
