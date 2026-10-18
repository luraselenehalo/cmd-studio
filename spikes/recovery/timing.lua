-- Marks the start of each cmd window's first edit line for recovery-timing.ps1: the first
-- clink.onbeginedit writes <data>\first-prompt-<pid>.txt once. That is the point after every
-- script has loaded, the start counter in init.lua included, and before the prompt filters run.
-- The first version of this file marked the first prompt with a prompt filter at priority 100,
-- which never runs with Oh My Posh loaded (spike S6: its right filter ends the chain), so spike
-- S8 was measured with this marker on the VMs (docs\spikes\phase3b-recovery\results.md,
-- timing-vm-a.txt), and plan 3b-1 Task 12 measures the shipped app the same way. The host
-- script reads the file's write time against the process's start time. recovery-setup.ps1 and
-- recovery-timing.ps1 -InstallMarker put `local p3b_data = [==[<data>]==]` in front.
local marked = false
clink.onbeginedit(function()
  if marked then return end
  marked = true
  local f = io.open(p3b_data .. "\\first-prompt-" .. tostring(os.getpid()) .. ".txt", "w")
  if f then
    f:write("ok")
    f:close()
  end
end)
