-- Test runner for kbase.nvim
-- Usage: nvim --headless -u NONE -c 'lua dofile("nvim/test/run.lua")' -c 'qall'

print('Running kbase.nvim tests...')
print('Working directory:', vim.fn.getcwd())
print(string.rep('=', 50))

-- Track totals across all test files
local total_passed = 0
local total_failed = 0

-- Run telescope tests
local telescope_assert = dofile('nvim/test/test_telescope.lua')
total_passed = total_passed + telescope_assert.passed
total_failed = total_failed + telescope_assert.failed
telescope_assert.reset()

-- Run neotree setup tests
local neotree_assert = dofile('nvim/test/test_neotree.lua')
total_passed = total_passed + neotree_assert.passed
total_failed = total_failed + neotree_assert.failed
neotree_assert.reset()

-- Run neotree source tests
local source_assert = dofile('nvim/test/test_neotree_source.lua')
total_passed = total_passed + source_assert.passed
total_failed = total_failed + source_assert.failed
source_assert.reset()

-- Run main init tests
local init_assert = dofile('nvim/test/test_init.lua')
total_passed = total_passed + init_assert.passed
total_failed = total_failed + init_assert.failed

-- Final summary
print('')
print(string.rep('=', 50))
print('FINAL RESULTS')
print(string.rep('=', 50))

if total_failed == 0 then
  print(string.format('All tests passed: %d/%d', total_passed, total_passed + total_failed))
else
  print(string.format('Tests: %d passed, %d FAILED', total_passed, total_failed))
end

-- Exit with appropriate code for CI
if total_failed > 0 then
  vim.cmd('cquit 1')
end
