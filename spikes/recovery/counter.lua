-- The start counter (recovery spec §3): each cmd window leaves a tag health\start-<pid> that
-- holds "v1 <unix seconds>" until its first prompt. PAUSE_AFTER tags older than STALE_AFTER
-- seconds are that many windows in a row that never got there, and the theme is paused in cmd:
-- health\paused.json. Only a cmd window counts: CMD Studio's own `clink set` (the apply's check,
-- doctor D3) loads this file too and never shows a prompt. A file that cannot be read or written
-- skips the counter: the theme is never paused for that.
local paused_file = health .. "\\paused.json"
local own_tag = nil
local paused_count = 0
local function in_cmd()
  local host = os.gethost and os.gethost() or ""
  return host:lower():find("\\cmd%.exe$") ~= nil
end
local host_ok, is_cmd = pcall(in_cmd)
is_cmd = host_ok and is_cmd
local function tag_time(path)
  local f = io.open(path, "rb")
  if not f then return nil end
  local text = f:read("*a") or ""
  f:close()
  local t = text:match("^v1 (%d+)%s*$")
  return t and tonumber(t)
end
local function tags()
  local found = {}
  for _, name in ipairs(os.globfiles(health .. "\\start-*") or {}) do
    local file = name:match("([^\\/]+)$")
    if file and file:match("^start%-%d+$") then found[#found + 1] = health .. "\\" .. file end
  end
  return found
end
local function stale(now)
  local old = {}
  for _, path in ipairs(tags()) do
    local t = tag_time(path)
    if t and now - t > STALE_AFTER then old[#old + 1] = path end
  end
  return old
end
local function count_start()
  local f = io.open(paused_file, "rb")
  if f then
    f:close()
    return true
  end
  if not is_cmd then return false end
  local now = os.time()
  local old = stale(now)
  if #old >= PAUSE_AFTER then
    local p = io.open(paused_file, "wb")
    if p then
      p:write(string.format('{"version":1,"reason":"cmdStarts","at":%.0f,"count":%d,"by":"clink"}\n', now * 1000, #old))
      p:close()
      for _, path in ipairs(tags()) do os.remove(path) end
      paused_count = #old
      return true
    end
  end
  local tag = health .. "\\start-" .. os.getpid()
  local t = io.open(tag, "wb")
  if t then
    t:write("v1 " .. now .. "\n")
    t:close()
    own_tag = tag
  end
  return false
end
local counted, paused = pcall(count_start)
if counted and paused then
  -- No prompt and no welcome box (banner.lua reads CMDSTUDIO_PAUSED), and one English line in a
  -- cmd window, once for it and the cmd windows started from it.
  CMDSTUDIO_PAUSED = true
  if is_cmd and clink.oninject then
    clink.oninject(function()
      if os.getenv("CMDSTUDIO_PAUSE_SAID") then return end
      os.setenv("CMDSTUDIO_PAUSE_SAID", "1")
      if paused_count > 0 then
        clink.print(string.format(PAUSED_NOW, paused_count))
      else
        clink.print(PAUSED_STILL)
      end
    end)
  end
  return
end
if counted and is_cmd and clink.promptfilter then
  -- Oh My Posh registers its prompt filter at _ZL_CLINK_PROMPT_PRIORITY + 1 (1 when that is
  -- unset), and Clink calls the lower number first: this one runs once Oh My Posh's has returned.
  local first = clink.promptfilter((tonumber(os.getenv("_ZL_CLINK_PROMPT_PRIORITY") or "") or 0) + 2)
  local reached = false
  function first:filter()
    if reached then return end
    reached = true
    pcall(function()
      if own_tag then os.remove(own_tag) end
      for _, path in ipairs(stale(os.time())) do os.remove(path) end
    end)
  end
end
