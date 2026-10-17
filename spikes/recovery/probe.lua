-- Phase 3b-1 spike probes (recovery spec §0; plan 3b-1 Task 1). recovery-setup.ps1 loads this
-- first on clink.path as aa-probe.lua, with `local p3b_data = [==[<data folder>]==]` in front.
-- Each line is "p3b <pid> ...": in clink.log, in <data>\probe.txt (when io.open works there), and
-- printed when the host is not cmd (clink set), so recovery-setup.ps1's own `clink set` shows it.
local pid = os.getpid and os.getpid() or "nopid"
local host = os.gethost and os.gethost() or "nohost"
local in_cmd = tostring(host):lower():find("\\cmd%.exe$") ~= nil
local function say(what)
  local line = "p3b " .. tostring(pid) .. " " .. what
  log.info(line)
  local f = io.open(p3b_data .. "\\probe.txt", "ab")
  if f then
    f:write(line .. "\n")
    f:close()
  end
  if not in_cmd then print(line) end
end
local seq = 0
local function step(name)
  seq = seq + 1
  say("order " .. seq .. " " .. name)
end
-- S1, S4, S5: the process id, the host program, the clock, Oh My Posh's priority variable.
say("load host=" .. tostring(host) .. " time=" .. tostring(os.time()) ..
  " zl=" .. tostring(os.getenv("_ZL_CLINK_PROMPT_PRIORITY")))
say("api getpid=" .. type(os.getpid) .. " gethost=" .. type(os.gethost) .. " globfiles=" ..
  type(os.globfiles) .. " find_files=" .. type(clink.find_files) .. " promptfilter=" ..
  type(clink.promptfilter) .. " onbeginedit=" .. type(clink.onbeginedit) .. " onendedit=" ..
  type(clink.onendedit) .. " oninject=" .. type(clink.oninject) .. " clock=" ..
  string.format("%.6f", os.clock()))
-- S2, S3: write, read, list and delete a tag in health\, whose path is Thai on VM-C.
local health = p3b_data .. "\\health"
local ok, err = pcall(function()
  local tag = health .. "\\start-" .. tostring(pid)
  local f, why = io.open(tag, "wb")
  say("io.open write ok=" .. tostring(f ~= nil) .. " why=" .. tostring(why))
  if f then
    f:write("v1 " .. os.time() .. "\n")
    f:close()
  end
  local r, why2 = io.open(tag, "rb")
  local text = r and r:read("*a")
  if r then r:close() end
  say("io.open read text=" .. tostring(text and text:gsub("\n", "\\n")) .. " why=" .. tostring(why2))
  local list = os.globfiles(health .. "\\start-*")
  say("globfiles count=" .. tostring(list and #list) .. " first=" .. tostring(list and list[1]))
  local removed, why3 = os.remove(tag)
  say("os.remove ok=" .. tostring(removed) .. " why=" .. tostring(why3))
  local again = io.open(tag, "rb")
  say("gone after remove=" .. tostring(again == nil))
  if again then again:close() end
end)
say("files pcall ok=" .. tostring(ok) .. " err=" .. tostring(err))
-- S6: when each hook runs, against Oh My Posh's prompt filter (priority 1, loaded by init.lua).
if clink.oninject then clink.oninject(function() step("oninject") end) end
if clink.onbeginedit then
  local first = true
  clink.onbeginedit(function()
    if first then
      first = false
      step("onbeginedit")
    end
  end)
end
if clink.promptfilter then
  local before, after = clink.promptfilter(0), clink.promptfilter(2)
  local seen0, seen2 = false, false
  function before:filter(prompt)
    if seen0 then return end
    seen0 = true
    step("filter-0 bytes=" .. #prompt .. " esc=" .. tostring(prompt:find("\27") ~= nil))
  end
  function after:filter(prompt)
    if seen2 then return end
    seen2 = true
    step("filter-2 bytes=" .. #prompt .. " esc=" .. tostring(prompt:find("\27") ~= nil))
  end
end
