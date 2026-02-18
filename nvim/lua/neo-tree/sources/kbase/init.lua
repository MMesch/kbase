-- Neo-tree source for kbase tags
-- Shows hierarchical tags with notes, like Obsidian's tag tree

local vim = vim
local utils = require("neo-tree.utils")
local renderer = require("neo-tree.ui.renderer")
local manager = require("neo-tree.sources.manager")
local events = require("neo-tree.events")

local M = { name = "kbase" }

-- Helper to execute LSP command via coc or native
local function lsp_execute(command, args, callback)
  if vim.fn.exists("*CocAction") == 1 then
    vim.schedule(function()
      local ok, result = pcall(function()
        return vim.fn.CocAction("runCommand", command, unpack(args or {}))
      end)
      if ok and result and result ~= vim.NIL then
        callback(result)
      else
        callback(nil)
      end
    end)
  else
    local params = { command = command, arguments = args or {} }
    vim.lsp.buf_request(0, "workspace/executeCommand", params, function(err, result)
      callback(result)
    end)
  end
end

-- Build a single tag node with proper structure
local function create_tag_node(tag_name, full_tag)
  return {
    id = "tag:" .. full_tag,
    name = tag_name,
    type = "directory",
    loaded = false,
    children = {},
    extra = { full_tag = full_tag },
    expanded = false,
    skip_node = false,
  }
end

-- Build flat list of root tags only (subtags load lazily when expanded)
local function build_tag_tree(flat_tags)
  local root = {
    id = "root",
    name = "Tags",
    type = "directory",
    loaded = true,
    children = {},
  }

  local seen_roots = {}
  table.sort(flat_tags)

  for _, tag in ipairs(flat_tags) do
    -- Get the root part (before first /)
    local root_tag = tag:match("^([^/]+)")
    if root_tag and not seen_roots[root_tag] then
      seen_roots[root_tag] = true
      table.insert(root.children, create_tag_node(root_tag, root_tag))
    end
  end

  return root
end

-- Load notes for a tag - called when expanding
local function load_notes_for_tag(state, node, callback)
  -- Get full_tag from extra, or extract from node id as fallback
  local full_tag = node.extra and node.extra.full_tag
  if not full_tag then
    local id = node.id or (node.get_id and node:get_id())
    if id and id:match("^tag:") then
      full_tag = id:sub(5)  -- Remove "tag:" prefix
    end
  end

  if not full_tag then
    vim.notify("kbase: no tag found for node", vim.log.levels.WARN)
    if callback then callback() end
    return
  end

  local parent_id = node:get_id()

  -- First get subtags by querying tags that start with this tag's path
  lsp_execute("kbase.tags", {}, function(tags_result)
    local subtags = {}
    if tags_result and tags_result.tags then
      local prefix = full_tag .. "/"
      for _, tag in ipairs(tags_result.tags) do
        -- Check if this tag is a direct child (one level deeper)
        if tag:sub(1, #prefix) == prefix then
          local rest = tag:sub(#prefix + 1)
          -- Only include direct children (no further slashes)
          if not rest:find("/") then
            table.insert(subtags, {
              id = "tag:" .. tag,
              name = rest,
              type = "directory",
              loaded = false,
              children = {},
              extra = { full_tag = tag },
            })
          end
        end
      end
    end

    -- Now get notes for this tag (direct_only=true to avoid duplicates in tree)
    lsp_execute("kbase.notes", { full_tag, true }, function(result)
      local children = {}

      -- Add subtags first
      for _, subtag in ipairs(subtags) do
        table.insert(children, subtag)
      end

      -- Add notes from LSP (use parent-scoped IDs to avoid conflicts)
      if result and #result > 0 then
        for _, note in ipairs(result) do
          table.insert(children, {
            id = full_tag .. ":note:" .. note.path,
            name = note.title,
            type = "file",
            path = note.path,
            loaded = true,
            children = {},
            extra = { path = note.path },
          })
        end
      end

      -- Use show_nodes with parent_id for dynamic children
      renderer.show_nodes(children, state, parent_id)
      node.loaded = true

      if callback then callback() end
    end)
  end)
end

-- Navigate/refresh the tree
M.navigate = function(state, path, path_to_reveal, callback, async)
  state.loading = true

  lsp_execute("kbase.tags", {}, function(result)
    state.loading = false

    if not result or not result.tags or #result.tags == 0 then
      renderer.show_nodes({
        {
          id = "empty",
          name = "No tags found (is kbase LSP running?)",
          type = "file",
        }
      }, state)
      if callback then callback() end
      return
    end

    local root = build_tag_tree(result.tags)
    renderer.show_nodes(root.children, state)

    -- Ensure all nodes start collapsed
    vim.schedule(function()
      if state.tree then
        for _, node in ipairs(state.tree:get_nodes()) do
          if node and node.collapse then
            node:collapse()
          end
        end
        renderer.redraw(state)
      end
    end)

    if callback then callback() end
  end)
end

-- Called to load children when expanding a node
M.load_children = function(state, node, callback)
  load_notes_for_tag(state, node, function()
    renderer.redraw(state)
    if callback then callback() end
  end)
end

M.default_config = {
  enable_git_status = false,
  enable_diagnostics = false,
  auto_expand_depth = 0,  -- Don't auto-expand nodes
  renderers = {
    directory = {
      { "indent" },
      { "icon" },
      { "name", use_git_status_colors = false },
    },
    file = {
      { "indent" },
      { "icon" },
      { "name", use_git_status_colors = false },
    },
  },
}

M.setup = function(config, global_config)
  M.config = vim.tbl_deep_extend("force", M.default_config, config or {})
end

return M
