-- Phase 3b-1 spike round 2, S6b: does a coroutine registered at the first edit line run only after
-- every prompt filter (Oh My Posh's included) has returned and the prompt is drawn? If so, the start
-- counter can delete its tag there: a window that hangs in a filter never idles, keeps its tag and
-- is counted, while a window opened and closed at once without typing is not.
-- Two routes are tried in the same window: clink.addcoroutine from clink.onbeginedit, and the
-- documented clink.promptcoroutine from the prompt filter below. Oh My Posh's own filter calls are
-- timed ("omp filter start/end"), so each coroutine line reads as before or after "omp filter end".
-- recovery-s6b.ps1 -Setup loads this as aa-probe.lua on clink.path (before init.lua), with
-- `local p3b2_profile = [==[<Clink profile folder>]==]` in front.
-- Each line is "p3b2 <pid> ...": in clink.log, in <profile>\p3b2.txt (opened, written and closed per
-- line, so a window that is killed loses nothing), and printed when the host is not cmd (clink set).
-- Every check goes through try(), which logs "<what> raised: <error>": an error never looks like a
-- coroutine that did not run. A sink (clink.log, p3b2.txt) that fails says so through the other one.
-- Note for Clink 1.9.34: a call that returns no value (os.getenv of an unset variable) raises inside
-- tostring(), so none is used here.
local profile = p3b2_profile
local unpack = table.unpack or unpack
local pid, host = "nopid", "nohost"
local early = {}
do
  local ok, value = pcall(os.getpid)
  if ok then pid = value else early[#early + 1] = "getpid raised: " .. tostring(value) end
  ok, value = pcall(os.gethost)
  if ok then host = value else early[#early + 1] = "gethost raised: " .. tostring(value) end
end
local in_cmd = tostring(host):lower():find("\\cmd%.exe$") ~= nil
local function fmt(seconds) return string.format("%.3f", seconds) end
local function wall() return tostring(os.time()) end
local function stamp() return "t=" .. fmt(os.clock()) .. " wall=" .. wall() end
local function to_log(line) return pcall(function() log.info(line) end) end
local function to_file(line)
  return pcall(function()
    if not profile then error("p3b2_profile is not set", 0) end
    local f, why = io.open(profile .. "\\p3b2.txt", "ab")
    if not f then error("io.open: " .. tostring(why), 0) end
    f:write(line .. "\n")
    f:close()
  end)
end
local sink_failed = {}
local function say(what)
  local line = "p3b2 " .. tostring(pid) .. " " .. what
  local ok_log, err_log = to_log(line)
  local ok_file, err_file = to_file(line)
  if not ok_log and not sink_failed.log then
    sink_failed.log = true
    to_file("p3b2 " .. tostring(pid) .. " log.info raised: " .. tostring(err_log))
  end
  if not ok_file and not sink_failed.file then
    sink_failed.file = true
    to_log("p3b2 " .. tostring(pid) .. " p3b2.txt not written: " .. tostring(err_file))
  end
  if not in_cmd then pcall(function() print(line) end) end
end
local function try(what, fn)
  local ok, err = pcall(fn)
  if not ok then say(what .. " raised: " .. tostring(err)) end
  return ok
end
for _, text in ipairs(early) do say(text) end

-- 1. What exists, and the clock at load.
try("load", function()
  say("load host=" .. tostring(host) .. " time=" .. wall())
  say("api addcoroutine=" .. type(clink.addcoroutine) .. " promptcoroutine=" .. type(clink.promptcoroutine) ..
    " onendedit=" .. type(clink.onendedit) .. " oncommand=" .. type(clink.oncommand) ..
    " onaftercommand=" .. type(clink.onaftercommand) .. " sleep=" .. type(os.sleep) ..
    " popen=" .. type(io.popen) .. " pack=" .. type(table.pack) .. " unpack=" .. type(unpack) ..
    " version_encoded=" .. tostring(clink.version_encoded) .. " clock=" .. fmt(os.clock()))
end)

-- 2. Remember every prompt filter that is registered after this file (Oh My Posh's init.lua loads
-- later), with its priority. The probe's own filter is made with the real function below, so it is
-- not among them.
local real_promptfilter = clink.promptfilter
local remembered = {}
try("wrap clink.promptfilter", function()
  if type(real_promptfilter) ~= "function" then return end
  clink.promptfilter = function(priority, ...)
    local obj = real_promptfilter(priority, ...)
    pcall(function() remembered[#remembered + 1] = { obj = obj, priority = priority } end)
    return obj
  end
end)

-- At the first onbeginedit, wrap filter, rightfilter and transientfilter of every remembered
-- object: each call logs "omp <method> start" and "omp <method> end" (priority, t=, wall=, the
-- number of results and the second one: Oh My Posh's rightfilter returns false there, which halts
-- the chain) and returns what the original returned, unchanged. A raise is logged and re-raised.
local function wrap_method(entry, name)
  local obj, priority = entry.obj, entry.priority
  local original = obj[name]
  if type(original) ~= "function" then return false end
  local calls = 0
  obj[name] = function(...)
    calls = calls + 1
    local n = calls
    say("omp " .. name .. " start prio=" .. tostring(priority) .. " n=" .. n .. " " .. stamp())
    local r = table.pack(pcall(original, ...))
    if not r[1] then
      say("omp " .. name .. " raised prio=" .. tostring(priority) .. " n=" .. n .. " " .. stamp() ..
        ": " .. tostring(r[2]))
      error(r[2], 0)
    end
    say("omp " .. name .. " end prio=" .. tostring(priority) .. " n=" .. n .. " " .. stamp() ..
      " results=" .. (r.n - 1) .. " second=" .. tostring(r[3]))
    return unpack(r, 2, r.n)
  end
  return true
end
local function wrap_filters()
  if type(table.pack) ~= "function" or type(unpack) ~= "function" then
    say("cannot wrap Oh My Posh's filters: table.pack " .. type(table.pack) .. ", unpack " .. type(unpack))
    return
  end
  local done = {}
  for _, entry in ipairs(remembered) do
    for _, name in ipairs({ "filter", "rightfilter", "transientfilter" }) do
      try("wrap " .. name, function()
        if wrap_method(entry, name) then done[#done + 1] = name .. "@" .. tostring(entry.priority) end
      end)
    end
  end
  say("wrapped " .. #done .. " methods of " .. #remembered .. " filters: " .. table.concat(done, ","))
end

-- 3. The edit lines. The first one registers a coroutine with clink.addcoroutine (the route to
-- test): Clink resumes coroutines while the edit loop waits for input, so its line must come after
-- the prompt filters' ("omp filter end", filter0). "addcoroutine ok=... status=..." says at once
-- whether the call raised and whether the coroutine had already run (status dead).
local edit_lines = 0
local began_first, began_last = nil, nil
local function add_coroutine_route()
  if type(clink.addcoroutine) ~= "function" then
    say("no addcoroutine")
    return
  end
  local ran = false
  local co = coroutine.create(function()
    try("coroutine body", function()
      if ran then return end
      ran = true
      local now = os.clock()
      say("coroutine t=" .. fmt(now) .. " after=" .. fmt(now - (began_first or now)) .. " wall=" .. wall())
    end)
  end)
  local ret
  local ok, err = pcall(function() ret = clink.addcoroutine(co) end)
  say("addcoroutine ok=" .. tostring(ok) .. " ret=" .. tostring(ret) .. " status=" .. coroutine.status(co))
  if not ok then say("addcoroutine raised: " .. tostring(err)) end
end
try("register onbeginedit", function()
  if not clink.onbeginedit then
    say("no onbeginedit")
    return
  end
  clink.onbeginedit(function()
    edit_lines = edit_lines + 1
    began_last = os.clock()
    if edit_lines > 1 then return end
    began_first = began_last
    try("onbeginedit", function()
      say("beginedit t=" .. fmt(began_first) .. " wall=" .. wall())
      try("wrap Oh My Posh's filters", wrap_filters)
      try("addcoroutine route", add_coroutine_route)
    end)
  end)
end)

-- 4. A prompt filter at priority 0, before Oh My Posh's. On its first call per edit line it also
-- registers clink.promptcoroutine (the documented route: "resumed while the input line is idle").
-- With <profile>\p3b2-hang it holds the thread for 20 s on its first call, to stand in for a slow
-- Oh My Posh; the file's text says how: "loop" (default) spins with os.clock() in a loop (os.sleep
-- is not used: a hang should hold the thread like a busy program, and a stuck clock is capped by
-- os.time()), "popen" waits on io.popen("ping -n 21 127.0.0.1 >nul"), as Oh My Posh waits on its
-- own child. A cmd /c child is not injected by Clink (S7).
local function hang_kind()
  if not profile then return nil end
  local f = io.open(profile .. "\\p3b2-hang", "rb")
  if not f then return nil end
  local text = f:read("*a") or ""
  f:close()
  if text:lower():find("popen", 1, true) then return "popen" end
  return "loop"
end
local function spin(seconds)
  local start, started = os.clock(), os.time()
  while os.clock() - start < seconds and os.time() - started < seconds + 10 do end
end
local function popen_wait()
  local handle, why = io.popen("ping -n 21 127.0.0.1 >nul")
  if not handle then
    say("filter0 popen failed: " .. tostring(why))
    return
  end
  handle:read("*a")
  handle:close()
end
local function run_promptcoroutine()
  if type(clink.promptcoroutine) ~= "function" then
    say("no promptcoroutine")
    return
  end
  local began = began_last
  local ret
  local ok, err = pcall(function()
    ret = clink.promptcoroutine(function()
      try("promptcoroutine body", function()
        local now = os.clock()
        say("promptcoroutine t=" .. fmt(now) .. " wall=" .. wall() .. " after=" .. fmt(now - (began or now)))
      end)
    end)
  end)
  say("promptcoroutine call ok=" .. tostring(ok) .. " ret=" .. tostring(ret))
  if not ok then say("promptcoroutine raised: " .. tostring(err)) end
end
try("register filter0", function()
  if type(real_promptfilter) ~= "function" then
    say("no promptfilter")
    return
  end
  local before = real_promptfilter(0)
  local calls, line_seen = 0, -1
  -- Returns nothing, so the prompt is unchanged and the next filter runs.
  function before:filter()
    calls = calls + 1
    try("filter0", function()
      if line_seen ~= edit_lines then
        line_seen = edit_lines
        try("promptcoroutine route", run_promptcoroutine)
      end
      if calls == 1 then
        local kind = hang_kind()
        if kind then
          say("filter0 hanging kind=" .. kind .. " " .. stamp())
          local started, clock = os.time(), os.clock()
          if kind == "popen" then popen_wait() else spin(20) end
          say("filter0 " .. stamp() .. " hung=" .. (os.time() - started) .. "s clock=" .. fmt(os.clock() - clock))
        else
          say("filter0 " .. stamp())
        end
      elseif calls <= 4 then
        say("filter0 again n=" .. calls .. " " .. stamp())
      end
    end)
  end
end)
