//! What the integration tests share. Each file under `tests/` is a crate of its own; a folder
//! under it is not, so `mod common;` brings this into the files that use it.

/// Stand-ins for the parts of Clink and Windows that `banner.lua` and `init.lua` use, so that
/// they run in plain Lua 5.2: `clink.print` collects what is printed in `__out` (marking a
/// print without a newline with `<NONL>`), `clink.oninject` keeps its callback in
/// `__oninject`, `console` counts cells and reads the screen lines from `__lines`,
/// `os.getenv` reads `__env` first and then gives a Thai user name and a computer name,
/// `os.getcwd` gives a Thai folder, and `log.info` does nothing.
pub const PRELUDE: &str = r#"
__out = {}
__env = {}
__lines = {}
NONL = {}
clink = {
  print = function(s, flag)
    if flag == NONL then s = "<NONL>" .. s end
    __out[#__out + 1] = s
  end,
  oninject = function(f) __oninject = f end,
}
console = {
  cellcount = function(s)
    local plain = s:gsub("\27%[[%d;]*m", "")
    local _, n = plain:gsub("[^\128-\191]", "")
    return n
  end,
  getlinetext = function(n) return __lines[n] end,
}
os.getenv = function(k)
  if __env[k] ~= nil then return __env[k] end
  if k == "USERNAME" then return "ทดสอบ" end
  if k == "COMPUTERNAME" then return "PC-01" end
  return nil
end
os.setenv = function(k, v) __env[k] = v end
os.getcwd = function() return "C:\\Users\\ทดสอบ" end
log = { info = function() end }
"#;
