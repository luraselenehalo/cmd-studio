-- cmd /c drops the outermost quotes, so the command is wrapped once more (spec gate G17).
-- 2>nul sits inside them: io.popen pipes stdout only, so without it the error text of cmd
-- (a missing exe) would be printed in every window.
local handle = io.popen('"' .. '"' .. omp .. '" init cmd --config "' .. config .. '" 2>nul' .. '"')
local script = handle and handle:read("*a") or ""
if handle then handle:close() end
if script == "" then
  log.info("cmdstudio: oh-my-posh init returned nothing")
  return
end
local chunk, err = load(script)
if not chunk then
  log.info("cmdstudio: cannot load oh-my-posh init: " .. tostring(err))
  return
end
-- A script that parses but raises must not print an error in every window either.
local ok, failure = pcall(chunk)
if not ok then
  log.info("cmdstudio: oh-my-posh init failed: " .. tostring(failure))
  return
end
log.info("cmdstudio: prompt loaded")
