-- Tests for main kbase module
local assert = dofile('nvim/test/assertions.lua')

print('\nRunning init module tests...')

-- Add current directory to runtime path
vim.opt.runtimepath:prepend(vim.fn.getcwd() .. '/nvim')

-- Mock telescope modules
package.loaded['telescope.pickers'] = { new = function() return { find = function() end } end }
package.loaded['telescope.finders'] = { new_table = function() end, new_dynamic = function() end }
package.loaded['telescope.config'] = { values = { generic_sorter = function() end, buffer_previewer_maker = function() end } }
package.loaded['telescope.actions'] = { close = function() end, select_default = { replace = function() end } }
package.loaded['telescope.actions.state'] = { get_selected_entry = function() end, get_current_line = function() end }
package.loaded['telescope.previewers'] = { new_buffer_previewer = function() end }
package.loaded['telescope.sorters'] = { empty = function() end }

-- Clear cached modules
package.loaded['kbase'] = nil
package.loaded['kbase.telescope'] = nil
package.loaded['kbase.neotree'] = nil

local kbase = require('kbase')

assert.run_test('main module loads', function()
  assert.not_nil(kbase, 'module should load')
end)

assert.run_test('kbase.setup is a function', function()
  assert.equals(type(kbase.setup), 'function', 'setup should be a function')
end)

assert.run_test('kbase.telescope is available', function()
  assert.not_nil(kbase.telescope, 'telescope submodule should be available')
end)

assert.run_test('kbase.neotree is available', function()
  assert.not_nil(kbase.neotree, 'neotree submodule should be available')
end)

assert.run_test('setup creates keymaps with default prefix', function()
  -- Track created keymaps
  local keymaps = {}
  local original_set = vim.keymap.set
  vim.keymap.set = function(mode, lhs, rhs, opts)
    table.insert(keymaps, { mode = mode, lhs = lhs })
  end

  -- Mock neotree setup to avoid neo-tree dependency
  kbase.neotree.setup = function() end

  kbase.setup({})

  vim.keymap.set = original_set

  -- Should have created telescope keymaps
  assert.truthy(#keymaps >= 4, 'should create at least 4 keymaps')

  -- Check for expected prefixes
  local has_ks = false
  local has_kb = false
  local has_kn = false
  local has_kt = false

  for _, km in ipairs(keymaps) do
    if km.lhs:match('<leader>ks') then has_ks = true end
    if km.lhs:match('<leader>kb') then has_kb = true end
    if km.lhs:match('<leader>kn') then has_kn = true end
    if km.lhs:match('<leader>kt') then has_kt = true end
  end

  assert.truthy(has_ks, 'should have <leader>ks keymap')
  assert.truthy(has_kb, 'should have <leader>kb keymap')
  assert.truthy(has_kn, 'should have <leader>kn keymap')
  assert.truthy(has_kt, 'should have <leader>kt keymap')
end)

assert.run_test('setup respects telescope = false', function()
  -- Clear keymaps
  local keymaps = {}
  local original_set = vim.keymap.set
  vim.keymap.set = function(mode, lhs, rhs, opts)
    table.insert(keymaps, { mode = mode, lhs = lhs })
  end

  -- Mock neotree setup
  kbase.neotree.setup = function() end

  kbase.setup({ telescope = false })

  vim.keymap.set = original_set

  -- Should not have created telescope keymaps
  assert.equals(#keymaps, 0, 'should not create keymaps when telescope = false')
end)

return assert
