-- Commands for kbase neo-tree source

local cc = require("neo-tree.sources.common.commands")
local renderer = require("neo-tree.ui.renderer")
local manager = require("neo-tree.sources.manager")
local utils = require("neo-tree.utils")

local M = {}

M.name = "kbase"

local refresh = utils.wrap(manager.refresh, M.name)

-- Open file or toggle directory
M.open = function(state)
  local tree = state.tree
  if not tree then return end

  local node = tree:get_node()
  if not node then return end

  if node.type == "file" then
    local path = node.path or (node.extra and node.extra.path)
    if path then
      -- Find a non-neo-tree window
      for _, w in ipairs(vim.api.nvim_list_wins()) do
        local buf = vim.api.nvim_win_get_buf(w)
        local ft = vim.api.nvim_buf_get_option(buf, "filetype")
        if ft ~= "neo-tree" then
          vim.api.nvim_set_current_win(w)
          break
        end
      end
      vim.cmd("edit " .. vim.fn.fnameescape(path))
    end
  else
    M.toggle_node(state)
  end
end

-- Toggle directory expansion
M.toggle_node = function(state)
  local tree = state.tree
  if not tree then return end

  local node = tree:get_node()
  if not node or node.type ~= "directory" then return end

  if node:is_expanded() then
    node:collapse()
    renderer.redraw(state)
  else
    -- Load children if not loaded
    if not node.loaded then
      local source = require("neo-tree.sources.kbase")
      source.load_children(state, node, function()
        node:expand()
        renderer.redraw(state)
      end)
    else
      node:expand()
      renderer.redraw(state)
    end
  end
end

M.refresh = function(state)
  refresh()
end

-- Toggle between all-tags and primary-tags-only view
M.toggle_view_mode = function(state)
  local source = require("neo-tree.sources.kbase")
  source.toggle_view_mode(state)
end

-- Inherit all common commands
cc._add_common_commands(M, nil)

return M
