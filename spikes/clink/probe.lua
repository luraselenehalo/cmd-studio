clink.oninject(function()
  clink.print("CMDSTUDIO-PROBE-A")
  clink.print("CMDSTUDIO-", NONL)
  clink.print("PROBE-B")
  local found = {}
  for i = 1, 30 do
    local t = console.getlinetext(i) or ""
    if t:find("CMDSTUDIO%-") then found[#found + 1] = i .. "=" .. t end
  end
  log.info("probe getlinetext: " .. table.concat(found, " | "))
  local dir = os.getenv("CMDSTUDIO_PROBE_DIR") or ""
  local cmd = '"' .. '"' .. dir .. '\\echo args.cmd" "one two" "สาม"' .. '"'
  local h = io.popen(cmd)
  local got = h and h:read("*a") or "nil"
  if h then h:close() end
  log.info("probe popen: [" .. got .. "]")
  -- Separate input from output encoding: these print ASCII only.
  local h2 = io.popen('"' .. '"' .. dir .. '\\check.cmd" "สาม"' .. '"')
  log.info("probe popen-input (Thai file name reaches cmd?): [" .. (h2 and h2:read("*a") or "nil") .. "]")
  if h2 then h2:close() end
  local h3 = io.popen('"' .. '"' .. dir .. '\\who ami.exe"' .. '"')
  log.info("probe popen-exe (exe under a Thai path with spaces): [" .. (h3 and h3:read("*a") or "nil") .. "]")
  if h3 then h3:close() end
  for i = 1, 5 do
    log.info("probe screen line " .. i .. ": [" .. (console.getlinetext(i) or "nil") .. "]")
  end
end)
