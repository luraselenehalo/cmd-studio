-- Marks the first prompt of each cmd window for recovery-timing.ps1: a prompt filter after every
-- other one (priority 100) writes <data>\first-prompt-<pid>.txt once.
local marked = false
local mark = clink.promptfilter(100)
function mark:filter()
  if marked then return end
  marked = true
  local f = io.open(p3b_data .. "\\first-prompt-" .. tostring(os.getpid()) .. ".txt", "w")
  if f then
    f:write("ok")
    f:close()
  end
end
