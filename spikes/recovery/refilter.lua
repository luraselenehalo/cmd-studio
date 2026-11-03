-- Phase 3b-1 Task 12, step 2f (docs\spikes\phase3b-recovery\vm-check.md): what the refilter costs.
-- Recovery spec §13: when CMD Studio's prompt coroutine ends, Clink filters the prompt of the
-- window's first line once more. Loaded first (generated\aa-refilter.lua, before init.lua), this
-- file marks each call of a prompt filter of its own at priority -1000 (Clink calls the lower
-- number first: "own n=1" is the first chain, "own n=2" the refilter), and wraps the filter and
-- rightfilter methods of every prompt filter registered after it (CMD Studio's at 0 and Oh My
-- Posh's at 1; spike S6b's wrapper) with a "start" and an "end" line each, so the refilter's end
-- is seen too: its cost runs from "own n=2" to the last "end" of the second chain, and the whole
-- from the first chain's last "end" to that. Only the first edit line is marked (the first
-- onbeginedit counts the lines). The lines go to <data>\refilter-<pid>.txt with io.open (spike
-- S2), each "<what> n=<call> t=<os.clock> wall=<os.time>". recovery-filters.ps1 -Install puts
-- `local p3b_data = [==[<data>]==]` and `local p3b_priority = <n>` in front. Nothing here raises
-- past pcall, and the probe's own filter returns nothing: the prompt stays Oh My Posh's.
-- Plan 3b-2 Task 12 (carry F3 of 3b-1: the VM check saw "own n=3", a second refilter): who asks
-- for each refilter. clink.refilterprompt is wrapped once at load, and each call on the first
-- line logs "refilterprompt from <caller>" (the caller's frame of debug.traceback: a line of
-- `[string "..."]` is Oh My Posh's init script, which init.lua runs through load(script) with no
-- chunk name; a line naming init.lua is CMD Studio's own; a generated file's name is that file).
-- An "own n=<k>" line with no "refilterprompt from" line before it is Clink's own refilter, the
-- one it runs after the prompt coroutine ends (recovery spec §13): no script asked for it.
-- clink.promptcoroutine exposes nothing of its completion; the traceback is the evidence.
local unpack = table.unpack or unpack
local lines, own_calls = 0, 0
local function mark(text)
  pcall(function()
    local f = io.open(p3b_data .. "\\refilter-" .. tostring(os.getpid()) .. ".txt", "ab")
    if f then
      f:write(text .. "\n")
      f:close()
    end
  end)
end
local function stamp(what, n)
  return string.format("%s n=%d t=%.3f wall=%.0f", what, n, os.clock(), os.time())
end
-- Every prompt filter registered after this file, with its priority.
local real_promptfilter = clink.promptfilter
local later = {}
clink.promptfilter = function(priority, ...)
  local obj = real_promptfilter(priority, ...)
  pcall(function() later[#later + 1] = { obj = obj, priority = priority } end)
  return obj
end
-- Logs the method's start and end on the first line and returns what it returned, unchanged;
-- a raise is logged and raised again.
local function wrap(entry, name)
  local original = entry.obj[name]
  if type(original) ~= "function" then return false end
  local calls = 0
  local label = name .. "@" .. tostring(entry.priority)
  entry.obj[name] = function(...)
    calls = calls + 1
    if lines <= 1 then mark(stamp(label .. " start", calls)) end
    local r = table.pack(pcall(original, ...))
    if lines <= 1 then
      if r[1] then
        mark(stamp(label .. " end", calls) .. " results=" .. (r.n - 1) .. " second=" .. tostring(r[3]))
      else
        mark(stamp(label .. " raised", calls) .. " " .. tostring(r[2]))
      end
    end
    if not r[1] then error(r[2], 0) end
    return unpack(r, 2, r.n)
  end
  return true
end
clink.onbeginedit(function()
  lines = lines + 1
  if lines > 1 then return end
  mark(stamp("beginedit", 1))
  local done = 0
  for _, entry in ipairs(later) do
    if wrap(entry, "filter") then done = done + 1 end
    if wrap(entry, "rightfilter") then done = done + 1 end
  end
  mark(string.format("wrapped %d methods of %d filters", done, #later))
end)
-- Who asks for a refilter (3b-2, carry F3): the wrapper's caller, the traceback's first frame
-- above the wrapper (level 2: level 1 is the wrapper itself; a C function would count as a
-- level too, so no pcall sits between). A Clink without debug.traceback, or one whose
-- refilterprompt is not a function, loses the line and nothing else; mark itself is under pcall.
if type(clink.refilterprompt) == "function" then
  local real_refilter = clink.refilterprompt
  clink.refilterprompt = function(...)
    if lines <= 1 then
      local who = "?"
      if type(debug) == "table" and type(debug.traceback) == "function" then
        who = tostring(debug.traceback("", 2)):match("stack traceback:\n%s*([^\n]+)") or "?"
      end
      mark("refilterprompt from " .. who .. string.format(" t=%.3f", os.clock()))
    end
    return real_refilter(...)
  end
end
local own = real_promptfilter(p3b_priority)
function own:filter()
  if lines > 1 then return end
  own_calls = own_calls + 1
  mark(stamp("own", own_calls))
end
