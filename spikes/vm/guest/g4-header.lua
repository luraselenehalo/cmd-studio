-- G4 in AutoRun mode: what does the console hold when Clink is injected into a cmd that a person
-- opened (Win+R), and would the prototype's header check in claude-banner.lua match it?
clink.oninject(function()
  local l1 = console.getlinetext(1) or ""
  local l2 = console.getlinetext(2) or ""
  for i = 1, 4 do
    log.info("g4 screen line " .. i .. ": [" .. (console.getlinetext(i) or "nil") .. "]")
  end
  local match = l1:find("^Microsoft Windows %[") ~= nil and l2:find("^%(c%) Microsoft Corporation") ~= nil
  log.info("g4 prototype header match: " .. tostring(match))
  log.info("g4 os.getenv USERPROFILE: [" .. (os.getenv("USERPROFILE") or "nil") .. "]")
end)
