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
    vim.keymap.set("n", prefix .. "b", M.telescope.backlinks, { desc = "kbase: backlinks" })
    vim.keymap.set("n", prefix .. "n", M.telescope.notes, { desc = "kbase: all notes" })
    vim.keymap.set("n", prefix .. "t", M.telescope.tags, { desc = "kbase: tags" })
  end

  -- Neo-tree integration
  if opts.neotree ~= false then
    M.neotree.setup({
      keymap = opts.neotree_keymap,
    })
  end
end

return M
