-- Tests for neo-tree kbase source
local assert = dofile('nvim/test/assertions.lua')

print('\nRunning neo-tree source tests...')

-- Add current directory to runtime path
vim.opt.runtimepath:prepend(vim.fn.getcwd() .. '/nvim')

-- Mock neo-tree modules
package.loaded['neo-tree.utils'] = {}
package.loaded['neo-tree.ui.renderer'] = {
  show_nodes = function() end,
  redraw = function() end,
}
package.loaded['neo-tree.sources.manager'] = {}
package.loaded['neo-tree.events'] = {}

-- Clear cached modules
package.loaded['neo-tree.sources.kbase'] = nil

local source = require('neo-tree.sources.kbase')

assert.run_test('source module loads', function()
  assert.not_nil(source, 'source should load')
end)

assert.run_test('source has correct name', function()
  assert.equals(source.name, 'kbase', 'source name should be kbase')
end)

assert.run_test('source.navigate is a function', function()
  assert.equals(type(source.navigate), 'function', 'navigate should be a function')
end)

assert.run_test('source.load_children is a function', function()
  assert.equals(type(source.load_children), 'function', 'load_children should be a function')
end)

assert.run_test('source.setup is a function', function()
  assert.equals(type(source.setup), 'function', 'setup should be a function')
end)

assert.run_test('source has default_config', function()
  assert.not_nil(source.default_config, 'should have default_config')
  assert.equals(source.default_config.enable_git_status, false, 'git status should be disabled')
  assert.equals(source.default_config.auto_expand_depth, 0, 'auto_expand_depth should be 0')
end)

assert.run_test('source has renderers config', function()
  assert.not_nil(source.default_config.renderers, 'should have renderers')
  assert.not_nil(source.default_config.renderers.directory, 'should have directory renderer')
  assert.not_nil(source.default_config.renderers.file, 'should have file renderer')
end)

return assert
