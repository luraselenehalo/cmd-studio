-- Phase 3b-1 spike round 2, S6b: does a coroutine added in clink.onbeginedit run only after every
-- prompt filter (Oh My Posh's included) has returned and the prompt is drawn? If so, the start
-- counter can delete its tag there: a window that hangs in a filter never idles, keeps its tag and
-- is counted, while a window opened and closed at once without typing is not.
-- recovery-s6b.ps1 -Setup loads this as aa-probe.lua on clink.path, with
-- `local p3b2_profile = [==[<Clink profile folder>]==]` in front.
-- Each line is "p3b2 <pid> ...": in clink.log, in <profile>\p3b2.txt (opened, written and closed per
-- line, so a window that is killed loses nothing), and printed when the host is not cmd (clink set).
-- Every check is inside pcall: this file never raises. Note for Clink 1.9.34: a call that returns no
-- value (os.getenv of an unset variable) raises inside tostring(), so none is used here.
local profile = p3b2_profile
local pid = "nopid"
pcall(function() pid = os.getpid() end)
local host = "nohost"
pcall(function() host = os.gethost() end)
local in_cmd = tostring(host):lower():find("\\cmd%.exe$") ~= nil
local function fmt(seconds) return string.format("%.3f", seconds) end
local function say(what)
  local line = "p3b2 " .. tostring(pid) .. " " .. what
  pcall(function() log.info(line) end)
  pcall(function()
    if not profile then return end
    local f = io.open(profile .. "\\p3b2.txt", "ab")
    if f then
      f:write(line .. "\n")
      f:close()
    end
  end)
  if not in_cmd then pcall(function() print(line) end) end
end
-- 1. What exists, and the clock at load.
pcall(function()
  say("load host=" .. tostring(host) .. " time=" .. tostring(os.time()))
  say("api addcoroutine=" .. type(clink.addcoroutine) .. " onendedit=" .. type(clink.onendedit) ..
    " oncommand=" .. type(clink.oncommand) .. " onaftercommand=" .. type(clink.onaftercommand) ..
    " sleep=" .. type(os.sleep) .. " version_encoded=" .. tostring(clink.version_encoded) ..
    " clock=" .. fmt(os.clock()))
end)
-- 2. The first edit line: register a coroutine. Clink resumes coroutines while the edit loop waits
-- for input, so its line must come after the prompt filters' (the filter below, Oh My Posh's).
pcall(function()
  if not clink.onbeginedit then
    say("no onbeginedit")
    return
  end
  local first = true
  clink.onbeginedit(function()
    if not first then return end
    first = false
    pcall(function()
      local began = os.clock()
      say("beginedit t=" .. fmt(began) .. " wall=" .. tostring(os.time()))
      if type(clink.addcoroutine) ~= "function" then
        say("no addcoroutine")
        return
      end
      local ran = false
      clink.addcoroutine(coroutine.create(function()
        if ran then return end
        ran = true
        pcall(function()
          local now = os.clock()
          say("coroutine t=" .. fmt(now) .. " after=" .. fmt(now - began) .. " wall=" .. tostring(os.time()))
        end)
      end))
    end)
  end)
end)
-- 3. A prompt filter at priority 0, before Oh My Posh's (priority 1). With <profile>\p3b2-hang it
-- spins for 20 s first, with os.clock() in a loop, as a slow Oh My Posh would hold the edit loop
-- (os.sleep is not used: this should hold the thread like a busy program, not like a sleeper). The
-- cap on os.time() only keeps a stuck clock from hanging the window for good.
local function hang_requested()
  if not profile then return false end
  local f = io.open(profile .. "\\p3b2-hang", "rb")
  if f then
    f:close()
    return true
  end
  return false
end
local function spin(seconds)
  local start, wall = os.clock(), os.time()
  while os.clock() - start < seconds and os.time() - wall < seconds + 10 do end
end
pcall(function()
  if not clink.promptfilter then
    say("no promptfilter")
    return
  end
  local before = clink.promptfilter(0)
  local calls = 0
  -- Returns nothing, so the prompt is unchanged and the next filter runs.
  function before:filter()
    calls = calls + 1
    pcall(function()
      if calls == 1 then
        if hang_requested() then
          say("filter0 hanging t=" .. fmt(os.clock()) .. " wall=" .. tostring(os.time()))
          spin(20)
        end
        say("filter0 t=" .. fmt(os.clock()) .. " wall=" .. tostring(os.time()))
      elseif calls <= 4 then
        say("filter0 again n=" .. calls .. " t=" .. fmt(os.clock()) .. " wall=" .. tostring(os.time()))
      end
    end)
  end
end)
