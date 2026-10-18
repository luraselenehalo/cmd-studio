-- Phase 3b-1 Task 12, step 2a (docs\spikes\phase3b-recovery\vm-check.md): a prompt filter at Oh
-- My Posh's priority whose first call raises, as Oh My Posh's filter would if it raised at run
-- time. CMD Studio's filter (priority 0) has asked for its prompt coroutine by then: does the
-- coroutine still run and take the window's tag away, and does the window reach a prompt
-- (recovery spec §13)? Each call writes a line to <data>\raise-<pid>.txt first (io.open, spike
-- S2), and "raising" right before the error: a raise that never happened (two filters of one
-- priority have no fixed order, and the chain ends at Oh My Posh's right filter, spike S6) is told
-- from one that did. recovery-filters.ps1 -Install puts `local p3b_data = [==[<data>]==]` and
-- `local p3b_priority = <n>` in front and writes this as generated\zz-raise.lua.
local calls = 0
local function mark(text)
  pcall(function()
    local f = io.open(p3b_data .. "\\raise-" .. tostring(os.getpid()) .. ".txt", "ab")
    if f then
      f:write(text .. "\n")
      f:close()
    end
  end)
end
local probe = clink.promptfilter(p3b_priority)
function probe:filter()
  calls = calls + 1
  mark(string.format("filter n=%d priority=%s t=%.3f wall=%.0f", calls, tostring(p3b_priority), os.clock(), os.time()))
  if calls == 1 then
    mark("raising")
    error("raise")
  end
end
