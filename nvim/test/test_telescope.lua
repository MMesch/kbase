-- Tests for telescope integration
local assert = dofile('nvim/test/assertions.lua')

print('\nRunning telescope tests...')

-- Add current directory to runtime path
vim.opt.runtimepath:prepend(vim.fn.getcwd() .. '/nvim')

-- Mock telescope modules for testing
package.loaded['telescope.pickers'] = { new = function() return { find = function() end } end }
package.loaded['telescope.finders'] = {
  new_table = function(opts) return opts end,
  new_dynamic = function(opts) return opts end,
}
package.loaded['telescope.config'] = { values = { generic_sorter = function() end, buffer_previewer_maker = function() end } }
package.loaded['telescope.actions'] = { close = function() end, select_default = { replace = function() end } }
package.loaded['telescope.actions.state'] = { get_selected_entry = function() end, get_current_line = function() end }
package.loaded['telescope.previewers'] = { new_buffer_previewer = function(opts) return opts end }
package.loaded['telescope.sorters'] = { empty = function() end }

-- Clear cached modules
package.loaded['kbase.telescope'] = nil

local telescope = require('kbase.telescope')

-- Test that module loads correctly
assert.run_test('telescope module loads', function()
  assert.not_nil(telescope, 'module should load')
end)

assert.run_test('telescope.search is a function', function()
  assert.equals(type(telescope.search), 'function', 'search should be a function')
end)

assert.run_test('telescope.backlinks is a function', function()
  assert.equals(type(telescope.backlinks), 'function', 'backlinks should be a function')
end)

assert.run_test('telescope.notes is a function', function()
  assert.equals(type(telescope.notes), 'function', 'notes should be a function')
end)

assert.run_test('telescope.tags is a function', function()
  assert.equals(type(telescope.tags), 'function', 'tags should be a function')
end)

return assert
