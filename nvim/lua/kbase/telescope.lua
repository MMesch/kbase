-- Telescope pickers for kbase
-- Requires: telescope.nvim, an LSP client with kbase attached

local pickers = require("telescope.pickers")
local finders = require("telescope.finders")
local conf = require("telescope.config").values
local actions = require("telescope.actions")
local action_state = require("telescope.actions.state")
local previewers = require("telescope.previewers")

local M = {}

-- Helper: Execute LSP command and get result
-- Supports both coc.nvim and native LSP
local function lsp_execute(command, args, callback)
  -- Try coc.nvim first
  if vim.fn.exists('*CocAction') == 1 then
    vim.schedule(function()
      local ok, result = pcall(function()
        return vim.fn.CocAction('runCommand', command, unpack(args or {}))
      end)
      if ok and result and result ~= vim.NIL then
        callback(result)
      else
        vim.notify("kbase: no results from coc (command: " .. command .. ")", vim.log.levels.WARN)
      end
    end)
    return
  end

  -- Fall back to native LSP
  local params = {
    command = command,
    arguments = args or {},
  }
  vim.lsp.buf_request(0, "workspace/executeCommand", params, function(err, result)
    if err then
      vim.notify("kbase: " .. tostring(err), vim.log.levels.ERROR)
      return
    end
    if result then
      callback(result)
    else
      vim.notify("kbase: no results", vim.log.levels.WARN)
    end
  end)
end

-- File previewer that jumps to the correct line
local note_previewer = previewers.new_buffer_previewer({
  title = "Note Preview",
  define_preview = function(self, entry)
    local path = entry.path or (entry.value and entry.value.path)
    if not path or vim.fn.filereadable(path) ~= 1 then
      return
    end

    conf.buffer_previewer_maker(path, self.state.bufnr, {
      bufname = self.state.bufname,
      winid = self.state.winid,
      callback = function(bufnr)
        -- Jump to the line after content is loaded
        local lnum = entry.lnum or (entry.value and entry.value.line and entry.value.line + 1) or 1
        local line_count = vim.api.nvim_buf_line_count(bufnr)
        -- Clamp line number to valid range
        lnum = math.max(1, math.min(lnum, line_count))
        if lnum > 0 and line_count > 0 then
          pcall(vim.api.nvim_win_set_cursor, self.state.winid, { lnum, 0 })
          -- Center the line in the preview window
          pcall(vim.api.nvim_win_call, self.state.winid, function()
            vim.cmd("normal! zz")
          end)
          -- Highlight the line
          pcall(vim.api.nvim_buf_add_highlight, bufnr, -1, "Visual", lnum - 1, 0, -1)
        end
      end,
    })
  end,
})

-- Helper to make search result entries
local function make_search_entry(entry)
  -- Format: [score] filename > section
  local filename = vim.fn.fnamemodify(entry.path or "", ":t:r")
  -- Convert kebab-case to Title Case for nicer display
  local title = filename:gsub("-", " "):gsub("(%a)([%w]*)", function(a, b)
    return a:upper() .. b
  end)
  local display
  if entry.section and entry.section ~= "" then
    display = string.format("[%.2f] %s > %s", entry.score or 0, title, entry.section)
  else
    display = string.format("[%.2f] %s", entry.score or 0, title)
  end
  return {
    value = entry,
    display = display,
    ordinal = title .. " " .. (entry.section or "") .. " " .. (entry.preview or ""),
    path = entry.path,
    lnum = (entry.line or 0) + 1, -- Convert 0-indexed to 1-indexed
  }
end

-- Semantic search picker with live search
function M.search(opts)
  opts = opts or {}

  local last_query = ""
  local search_timer = nil
  local current_results = {}

  -- Create a dynamic finder that re-queries on input change
  local finder = finders.new_dynamic({
    fn = function(prompt)
      if not prompt or prompt == "" then
        return {}
      end
      return current_results
    end,
    entry_maker = make_search_entry,
  })

  local picker = pickers.new(opts, {
    prompt_title = "kbase semantic search",
    finder = finder,
    previewer = note_previewer,
    sorter = require("telescope.sorters").empty(), -- Don't re-sort, keep by score
    attach_mappings = function(prompt_bufnr, map)
      -- Trigger semantic search on <CR> in insert mode or debounced
      local function do_search()
        local prompt = action_state.get_current_line()
        if prompt and prompt ~= "" and prompt ~= last_query then
          last_query = prompt
          lsp_execute("kbase.search", { prompt }, function(results)
            current_results = results or {}
            -- Refresh the picker
            local current_picker = action_state.get_current_picker(prompt_bufnr)
            if current_picker then
              current_picker:refresh(finders.new_table({
                results = current_results,
                entry_maker = make_search_entry,
              }), { reset_prompt = false })
            end
          end)
        end
      end

      -- Search on <Tab> or <C-s>
      map("i", "<Tab>", function()
        do_search()
      end)
      map("i", "<C-s>", function()
        do_search()
      end)

      -- Also debounce search on typing (300ms delay)
      local on_input = function()
        if search_timer then
          vim.fn.timer_stop(search_timer)
        end
        search_timer = vim.fn.timer_start(500, function()
          vim.schedule(do_search)
        end)
      end

      -- Hook into input changes
      vim.api.nvim_create_autocmd("TextChangedI", {
        buffer = prompt_bufnr,
        callback = on_input,
      })

      actions.select_default:replace(function()
        actions.close(prompt_bufnr)
        local selection = action_state.get_selected_entry()
        if selection and selection.path then
          vim.cmd("edit +" .. (selection.lnum or 1) .. " " .. vim.fn.fnameescape(selection.path))
        end
      end)
      return true
    end,
  })

  picker:find()
end

-- Notes list picker
function M.notes(opts)
  opts = opts or {}
  local tag_filter = opts.tag or nil

  lsp_execute("kbase.notes", { tag_filter }, function(results)
    pickers.new(opts, {
      prompt_title = tag_filter and ("Notes [" .. tag_filter .. "]") or "All Notes",
      finder = finders.new_table({
        results = results,
        entry_maker = function(entry)
          return {
            value = entry,
            display = entry.title,
            ordinal = entry.title .. " " .. entry.path,
            path = entry.path,
          }
        end,
      }),
      sorter = conf.generic_sorter(opts),
      previewer = note_previewer,
      attach_mappings = function(prompt_bufnr, map)
        actions.select_default:replace(function()
          actions.close(prompt_bufnr)
          local selection = action_state.get_selected_entry()
          if selection and selection.path then
            vim.cmd("edit " .. vim.fn.fnameescape(selection.path))
          end
        end)
        return true
      end,
    }):find()
  end)
end

return M
