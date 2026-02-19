-- kbase.nvim - Neovim integration for kbase knowledge base
--
-- Usage:
--   require("kbase").setup({
--     telescope = true,      -- Enable telescope pickers (default: true)
--     neotree = true,        -- Enable neo-tree source (default: true)
--     neotree_keymap = nil,  -- Optional keymap for :KbaseTags
--     telescope_prefix = "<leader>k",  -- Prefix for telescope keymaps
--   })

local M = {}

M.telescope = require("kbase.telescope")
M.neotree = require("kbase.neotree")

function M.setup(opts)
  opts = opts or {}

  -- Telescope integration
  if opts.telescope ~= false then
    local prefix = opts.telescope_prefix or "<leader>k"
    vim.keymap.set("n", prefix .. "s", M.telescope.search, { desc = "kbase: semantic search" })
    vim.keymap.set("n", prefix .. "n", M.telescope.notes, { desc = "kbase: all notes" })
    -- Backlinks: support both native LSP and coc.nvim
    vim.keymap.set("n", prefix .. "b", function()
      if vim.fn.exists('*CocAction') == 1 then
        vim.fn.CocAction('jumpReferences')
      else
        vim.lsp.buf.references()
      end
    end, { desc = "kbase: backlinks" })
  end

  -- Neo-tree integration
  if opts.neotree ~= false then
    local prefix = opts.telescope_prefix or "<leader>k"
    M.neotree.setup({
      keymap = opts.neotree_keymap or (prefix .. "t"),
    })
  end
end

return M
