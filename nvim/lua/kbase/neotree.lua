-- kbase neo-tree integration
-- Adds a "kbase" source showing hierarchical tags with notes

local M = {}

M.setup = function(opts)
  opts = opts or {}

  local ok, neo_tree = pcall(require, "neo-tree")
  if not ok then
    vim.notify("kbase: neo-tree.nvim not found", vim.log.levels.WARN)
    return
  end

  -- Get existing config or defaults
  local existing = neo_tree.config or {}
  local sources = existing.sources or { "filesystem", "buffers", "git_status" }

  -- Add kbase source if not present
  local has_kbase = vim.tbl_contains(sources, "kbase")
  if not has_kbase then
    table.insert(sources, "kbase")
  end

  -- Merge in kbase config
  local kbase_config = vim.tbl_deep_extend("force", {
    auto_expand_depth = 0,
    expand_all = false,
    group_empty_dirs = false,
    follow_current_file = { enabled = false },
    use_libuv_file_watcher = false,
    bind_to_cwd = false,
    hide_root_node = true,
    window = {
      mappings = {
        ["<cr>"] = "open",
        ["o"] = "open",
        ["s"] = "open_split",
        ["v"] = "open_vsplit",
        ["r"] = "refresh",
      },
    },
  }, existing.kbase or {})

  -- Re-setup neo-tree with kbase source
  neo_tree.setup(vim.tbl_deep_extend("force", existing, {
    sources = sources,
    kbase = kbase_config,
  }))

  -- Setup command
  vim.api.nvim_create_user_command("KbaseTags", function()
    vim.cmd("Neotree source=kbase reveal=false toggle")
  end, { desc = "Toggle kbase tag tree" })

  -- Optional keymap
  if opts.keymap then
    vim.keymap.set("n", opts.keymap, "<cmd>KbaseTags<cr>", { desc = "kbase: tag tree" })
  end
end

return M
