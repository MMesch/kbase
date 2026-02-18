-- Tests for neo-tree integration
local assert = dofile('nvim/test/assertions.lua')

print('\nRunning neo-tree tests...')

-- Add current directory to runtime path
vim.opt.runtimepath:prepend(vim.fn.getcwd() .. '/nvim')

-- Clear cached modules
package.loaded['kbase.neotree'] = nil

local neotree = require('kbase.neotree')

-- Test that module loads correctly
assert.run_test('neotree module loads', function()
  assert.not_nil(neotree, 'module should load')
end)

assert.run_test('neotree.setup is a function', function()
  assert.equals(type(neotree.setup), 'function', 'setup should be a function')
end)

-- Test setup without neo-tree installed (should warn, not error)
assert.run_test('neotree.setup handles missing neo-tree gracefully', function()
  -- Ensure neo-tree is not loaded and will fail to load
  package.loaded['neo-tree'] = nil
  package.preload['neo-tree'] = nil

  -- Should not error (pcall the setup)
  local ok, err = pcall(neotree.setup, {})

  -- Setup should complete without throwing
  assert.truthy(ok, 'setup should not error: ' .. tostring(err))
end)

return assert
