//! init.lua's start counter and the pause it reads (recovery spec §3, §10), run for real in Lua
//! 5.2 with stand-ins for Clink. Each window is a Lua state of its own, as Clink makes one for
//! each cmd window, over one disk of files kept here between windows. The files are in memory,
//! not in a temp folder: Lua's own `io.open` takes the ANSI code page on Windows, unlike Clink's,
//! so a real folder would test mlua and not Clink; Thai paths are the spike's (plan 3b-1 Task 1,
//! S2). banner.lua loads first and init.lua second, as Clink loads a folder in name order; then
//! the inject callbacks run, then the prompt filters, the lowest priority first, and then Clink
//! waits for a key and resumes the prompt coroutines (spike S6b,
//! `docs/spikes/phase3b-recovery/results.md`).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use cmdstudio_core::generate::lua::{banner_lua, init_lua, PAUSED_NOW, PAUSED_STILL};
use cmdstudio_core::model::Theme;
use cmdstudio_core::presets;
use cmdstudio_core::recovery::{PauseReason, PauseRecord, PausedBy, PAUSED_VERSION};
use mlua::{Function, Lua, Table, Value};
use std::collections::BTreeMap;
use std::path::Path;

mod common;
use common::PRELUDE;

const HEALTH: &str = r"C:\Users\tester\AppData\Local\ai.resincore.cmdstudio\health";
const OMP_EXE: &str =
    r"C:\Users\tester\AppData\Local\ai.resincore.cmdstudio\runtime\omp-31.5.0\oh-my-posh.exe";
const OMP_CONFIG: &str =
    r"C:\Users\tester\AppData\Local\ai.resincore.cmdstudio\generated\theme.omp.json";
/// `os.gethost()` in a cmd window, and in CMD Studio's own `clink set`.
const CMD: &str = r"C:\Windows\System32\cmd.exe";
const CLINK: &str =
    r"C:\Users\tester\AppData\Local\ai.resincore.cmdstudio\runtime\clink-1.9.34\clink_x64.exe";
/// Friday 9 October 2026, 16:47 UTC, in seconds.
const T: u64 = 1_791_694_709;

/// Stand-ins for what Clink gives the scripts beyond `common::PRELUDE`: the files of `__files`
/// (a path to its text; `__fail[path]` or `__fail_all` makes a file fail), `os.globfiles` over
/// them (names only, or whole paths with `__glob_paths`, the two shapes Clink may give),
/// `os.getpid`, `os.gethost`, `os.time`, `os.getenv` giving no value at all for a variable that
/// is not set (as Clink 1.9.34's does), several inject callbacks, and the prompt:
/// - `__prompt()` runs the prompt filters as Clink does, the lowest priority first, each one's
///   `filter` and then its `rightfilter`, and stops at the first that returns `false` as its
///   second value: Oh My Posh's right filter does, so no filter after it runs (spike S6).
///   It notes how many values each `filter` returned.
/// - `clink.promptcoroutine(f)` may be called only from a prompt filter. A filter gets one
///   coroutine for an input line: a later call on the same line gives what `f` returned, `nil`
///   until it has run. With `__async = false` (Clink's `prompt.async` off) it runs `f(false)` at
///   once.
/// - `__idle()` is Clink waiting for a key: it resumes the coroutines with `true`, fails when one
///   raises, and filters the prompt once more when one has completed, as Clink does.
/// - `__new_line()` is the next input line: the coroutines of the last one are dropped.
const STUBS: &str = r#"
__fail = {}
__fail_all = false
__glob_paths = false
__injects = {}
__filters = {}
__async = true
__line = {}
__queue = {}
__current = nil
__coroutine_calls = 0
__drawn = nil
__popen_cmds = {}
__logs = {}
log = { info = function(s) __logs[#__logs + 1] = s end }
print = function(...) __out[#__out + 1] = "<print>" .. table.concat({...}, " ") end
clink.oninject = function(f) __injects[#__injects + 1] = f end
clink.promptfilter = function(priority)
  local f = { priority = priority, returned = {} }
  __filters[#__filters + 1] = f
  return f
end
clink.promptcoroutine = function(func)
  __coroutine_calls = __coroutine_calls + 1
  if __current == nil then error("clink.promptcoroutine is for a prompt filter") end
  if not __async then return func(false) end
  local entry = __line[__current]
  if entry == nil then
    entry = { co = coroutine.create(func) }
    __line[__current] = entry
    __queue[#__queue + 1] = entry
  end
  return entry.result
end
local getenv = os.getenv
os.getenv = function(name)
  local value = getenv(name)
  if value == nil then return end
  return value
end
os.time = function() return __now end
os.getpid = function() return __pid end
os.gethost = function() return __host end
io.open = function(path, mode)
  if __fail_all or __fail[path] then return nil, path .. ": Permission denied" end
  if mode:sub(1, 1) == "r" then
    local text = __files[path]
    if text == nil then return nil, path .. ": No such file or directory" end
    return { read = function() return text end, close = function() end }
  end
  local parts = {}
  return {
    write = function(self, ...)
      for _, s in ipairs({...}) do parts[#parts + 1] = tostring(s) end
      return self
    end,
    close = function() __files[path] = table.concat(parts) end,
  }
end
os.remove = function(path)
  if __fail_all or __fail[path] then return nil, path .. ": Permission denied" end
  if __files[path] == nil then return nil, path .. ": No such file or directory" end
  __files[path] = nil
  return true
end
os.globfiles = function(pattern)
  if __fail_all then error("globfiles: access denied") end
  local dir, prefix = pattern:match("^(.*)\\([^\\]*)%*$")
  local found = {}
  for path in pairs(__files) do
    local d, name = path:match("^(.*)\\([^\\]*)$")
    if d == dir and name:sub(1, #prefix) == prefix then
      found[#found + 1] = __glob_paths and path or name
    end
  end
  table.sort(found)
  return found
end
io.popen = function(cmd)
  __popen_cmds[#__popen_cmds + 1] = cmd
  if __popen_result == nil then return nil end
  return { read = function() return __popen_result end, close = function() end }
end
function __inject()
  for _, f in ipairs(__injects) do f() end
end
function __prompt()
  table.sort(__filters, function(a, b) return a.priority < b.priority end)
  local prompt = "C:\\>"
  for _, f in ipairs(__filters) do
    __current = f
    local go_on = true
    if f.filter then
      local r = table.pack(f:filter(prompt))
      f.returned[#f.returned + 1] = r.n
      if r[1] ~= nil then prompt = r[1] end
      if r[2] == false then go_on = false end
    end
    if go_on and f.rightfilter then
      local _, onwards = f:rightfilter("")
      if onwards == false then go_on = false end
    end
    __current = nil
    if not go_on then break end
  end
  __drawn = prompt
  return prompt
end
function __idle()
  local queue = __queue
  __queue = {}
  local completed = false
  for _, entry in ipairs(queue) do
    local ok, result = coroutine.resume(entry.co, true)
    if not ok then error("a prompt coroutine raised: " .. tostring(result), 0) end
    if coroutine.status(entry.co) == "dead" then
      entry.result = result
      completed = true
    else
      __queue[#__queue + 1] = entry
    end
  end
  if completed then __prompt() end
end
function __new_line()
  __line = {}
  __queue = {}
end
"#;

/// What `oh-my-posh init cmd` prints, cut down: a prompt filter at
/// `_ZL_CLINK_PROMPT_PRIORITY + 1`, 1 when that is unset, as Oh My Posh 31.5.0 registers its own.
/// Its `filter` notes whether the window's tag is still there when it first runs (the first
/// prompt, before any refilter) and returns one value; its `rightfilter` returns `false` second,
/// which ends the chain.
const OMP: &str = r#"
local value = os.getenv("_ZL_CLINK_PROMPT_PRIORITY")
local p = clink.promptfilter((tonumber(value) or 0) + 1)
function p:filter(prompt)
  if __tag_seen_by_omp == nil then
    __tag_seen_by_omp = __files[__health .. "\\start-" .. __pid] ~= nil
  end
  return "omp> "
end
function p:rightfilter(prompt)
  return "", false
end
"#;

/// The files, path to text, kept from one window to the next.
type Disk = BTreeMap<String, String>;

/// How a window is opened.
#[derive(Clone, Copy)]
struct How {
    pid: u32,
    now: u64,
    /// `os.gethost()`.
    host: &'static str,
    /// Clink injects: a cmd window. CMD Studio's own `clink set` never does.
    inject: bool,
    /// The prompt filters run and return, and the prompt is drawn.
    prompt: bool,
    /// Then Clink waits for a key (no key is pressed).
    idle: bool,
}

/// A cmd window, process `pid`, started at `now`, closed (or hung) before its first prompt.
fn window(pid: u32, now: u64) -> How {
    How {
        pid,
        now,
        host: CMD,
        inject: true,
        prompt: false,
        idle: false,
    }
}

impl How {
    /// The same window, and it reaches its first prompt: the filters return, the prompt is
    /// drawn and Clink waits for a key.
    fn prompted(self) -> How {
        How {
            prompt: true,
            idle: true,
            ..self
        }
    }

    /// The same window, closed once its prompt was drawn but before Clink waited for a key.
    fn closed_at_its_prompt(self) -> How {
        How {
            prompt: true,
            idle: false,
            ..self
        }
    }
}

/// A prompt filter of the window.
struct Filter {
    priority: i64,
    /// How many values its `filter` returned, each time it was called.
    returned: Vec<u32>,
}

/// What a window did.
struct Window {
    /// What it printed: the welcome box, the line of a paused theme.
    printed: Vec<String>,
    /// It ran Oh My Posh.
    omp: bool,
    /// init.lua set CMDSTUDIO_PAUSED.
    paused: bool,
    /// The prompt as drawn last, `None` when it was not drawn or a filter failed.
    prompt: Option<String>,
    /// Oh My Posh's filter found the window's tag still there.
    tag_seen_by_omp: bool,
    /// The window's tag was there once the filters had returned, before Clink waited for a key.
    tag_before_idle: bool,
    /// The variables it set.
    env: BTreeMap<String, String>,
    /// What it wrote to Clink's log.
    logs: Vec<String>,
    /// Its prompt filters, the lowest priority first.
    filters: Vec<Filter>,
    /// How many times it called `clink.promptcoroutine`.
    coroutine_calls: u32,
}

impl Window {
    fn shows_the_box(&self) -> bool {
        self.printed.iter().any(|p| p.contains("Welcome back"))
    }
    /// What it printed besides the box.
    fn lines(&self) -> Vec<String> {
        self.printed
            .iter()
            .filter(|p| !p.contains("Welcome back"))
            .cloned()
            .collect()
    }
    /// Its log lines about the start counter.
    fn counter_logs(&self) -> Vec<&String> {
        self.logs
            .iter()
            .filter(|l| l.contains("start counter"))
            .collect()
    }
}

fn terracotta() -> Theme {
    presets::builtin()
        .unwrap()
        .into_iter()
        .find(|(key, _)| *key == "terracotta")
        .unwrap()
        .1
}

fn tag(pid: u32) -> String {
    format!(r"{HEALTH}\start-{pid}")
}

fn paused_file() -> String {
    format!(r"{HEALTH}\paused.json")
}

/// A disk with a tag for each of `pids`, written `age` seconds before [`T`].
fn tags_of_age(pids: &[u32], age: u64) -> Disk {
    pids.iter()
        .map(|pid| (tag(*pid), format!("v1 {}\n", T - age)))
        .collect()
}

fn truthy(lua: &Lua, name: &str) -> mlua::Result<bool> {
    let v: Value = lua.globals().get(name)?;
    Ok(!matches!(v, Value::Nil | Value::Boolean(false)))
}

fn strings(table: &Table) -> mlua::Result<Vec<String>> {
    table.sequence_values::<String>().collect()
}

/// Opens one window over `disk`: `before` sets the stage after the stand-ins, `after` runs once
/// the window has done what `how` says, and the files are written back to `disk` at the end.
fn open_steps(
    disk: &mut Disk,
    how: How,
    before: impl FnOnce(&Lua) -> mlua::Result<()>,
    after: impl FnOnce(&Lua) -> mlua::Result<()>,
) -> mlua::Result<Window> {
    let lua = Lua::new();
    lua.load(PRELUDE).exec()?;
    let g = lua.globals();
    let files = lua.create_table()?;
    for (path, text) in disk.iter() {
        files.set(path.as_str(), text.as_str())?;
    }
    g.set("__files", files)?;
    lua.load(STUBS).exec()?;
    g.set("__now", how.now)?;
    g.set("__pid", how.pid)?;
    g.set("__host", how.host)?;
    g.set("__health", HEALTH)?;
    g.set("__popen_result", OMP)?;
    before(&lua)?;
    lua.load(banner_lua(&terracotta()))
        .set_name("banner.lua")
        .exec()?;
    lua.load(init_lua(
        Path::new(OMP_EXE),
        Path::new(OMP_CONFIG),
        Path::new(HEALTH),
    ))
    .set_name("init.lua")
    .exec()?;
    if how.inject {
        g.get::<Function>("__inject")?.call::<()>(())?;
    }
    let mut prompt = None;
    let mut tag_before_idle = false;
    if how.prompt {
        // A filter that raises stands in for one that hangs until the window is closed: nothing
        // after it runs.
        lua.load("__prompt_ok = pcall(__prompt)").exec()?;
        if g.get::<bool>("__prompt_ok")? {
            tag_before_idle = g
                .get::<Table>("__files")?
                .contains_key(tag(how.pid).as_str())?;
            if how.idle {
                g.get::<Function>("__idle")?.call::<()>(())?;
            }
            prompt = Some(g.get::<String>("__drawn")?);
        }
    }
    after(&lua)?;
    let printed = strings(&g.get::<Table>("__out")?)?;
    let files: Table = g.get("__files")?;
    disk.clear();
    for pair in files.pairs::<String, String>() {
        let (path, text) = pair?;
        disk.insert(path, text);
    }
    let mut filters = Vec::new();
    for f in g.get::<Table>("__filters")?.sequence_values::<Table>() {
        let f = f?;
        filters.push(Filter {
            priority: f.get("priority")?,
            returned: f
                .get::<Table>("returned")?
                .sequence_values::<u32>()
                .collect::<mlua::Result<_>>()?,
        });
    }
    filters.sort_by_key(|f| f.priority);
    let env: Table = g.get("__env")?;
    Ok(Window {
        printed,
        omp: g.get::<Table>("__popen_cmds")?.raw_len() > 0,
        paused: truthy(&lua, "CMDSTUDIO_PAUSED")?,
        prompt,
        tag_seen_by_omp: truthy(&lua, "__tag_seen_by_omp")?,
        tag_before_idle,
        env: env.pairs::<String, String>().collect::<mlua::Result<_>>()?,
        logs: strings(&g.get::<Table>("__logs")?)?,
        filters,
        coroutine_calls: g.get("__coroutine_calls")?,
    })
}

fn open_with(
    disk: &mut Disk,
    how: How,
    before: impl FnOnce(&Lua) -> mlua::Result<()>,
) -> mlua::Result<Window> {
    open_steps(disk, how, before, |_| Ok(()))
}

fn open(disk: &mut Disk, how: How) -> mlua::Result<Window> {
    open_with(disk, how, |_| Ok(()))
}

#[test]
fn a_first_window_leaves_its_tag_and_loads_the_prompt_and_the_box() -> mlua::Result<()> {
    let mut disk = Disk::new();
    let w = open(&mut disk, window(100, T))?;
    assert_eq!(disk, Disk::from([(tag(100), format!("v1 {T}\n"))]));
    assert!(w.omp && !w.paused && w.shows_the_box());
    assert!(w.lines().is_empty(), "{:?}", w.printed);
    Ok(())
}

#[test]
fn the_first_prompt_takes_the_tag_away_once_clink_waits_for_a_key() -> mlua::Result<()> {
    let mut disk = Disk::new();
    let w = open(&mut disk, window(100, T).prompted())?;
    assert_eq!(w.prompt.as_deref(), Some("omp> "));
    // The tag was still there while Oh My Posh drew the prompt, and when every filter had
    // returned: only the coroutine Clink resumes after that takes it away, so a window whose Oh
    // My Posh hangs at its prompt counts (recovery spec §3.2 step 5, spike S6b).
    assert!(w.tag_seen_by_omp && w.tag_before_idle);
    // No key was pressed and no line ended: a window closed at its first prompt has started.
    assert!(disk.is_empty(), "{disk:?}");
    Ok(())
}

#[test]
fn two_stale_tags_do_not_pause() -> mlua::Result<()> {
    let mut disk = tags_of_age(&[1, 2], 61);
    let w = open(&mut disk, window(3, T))?;
    assert!(!w.paused && w.omp && w.shows_the_box());
    assert_eq!(disk.len(), 3, "{disk:?}");
    assert!(disk.contains_key(&tag(3)));
    Ok(())
}

#[test]
fn three_windows_that_never_reach_a_prompt_pause_the_fourth() -> mlua::Result<()> {
    // Criterion 1 of recovery spec §1: the fourth window opens with cmd's own prompt, by itself,
    // and says so in one line of English.
    let mut disk = Disk::new();
    for (i, pid) in [1u32, 2, 3].into_iter().enumerate() {
        let w = open(&mut disk, window(pid, T + i as u64))?;
        assert!(!w.paused, "window {pid}");
    }
    let w = open(&mut disk, window(4, T + 70).prompted())?;
    assert!(w.paused && !w.omp && !w.shows_the_box());
    assert_eq!(w.lines(), [PAUSED_NOW.replace("%d", "3")]);
    assert_eq!(w.prompt.as_deref(), Some("C:\\>"), "cmd's own prompt");
    assert_eq!(
        disk,
        Disk::from([(
            paused_file(),
            format!(
                "{{\"version\":1,\"reason\":\"cmdStarts\",\"at\":{},\"count\":3,\"by\":\"clink\"}}\n",
                (T + 70) * 1000
            )
        )])
    );
    // Every window after it: no prompt, no box, the shorter line, and no tag.
    let w = open(&mut disk, window(5, T + 80).prompted())?;
    assert!(w.paused && !w.omp && !w.shows_the_box());
    assert_eq!(w.lines(), [PAUSED_STILL]);
    assert_eq!(disk.len(), 1, "{disk:?}");
    Ok(())
}

#[test]
fn the_pause_init_lua_writes_is_the_record_the_app_reads() -> mlua::Result<()> {
    let mut disk = tags_of_age(&[1, 2, 3, 4], 600);
    open(&mut disk, window(5, T))?;
    let record: PauseRecord = serde_json::from_str(&disk[&paused_file()]).unwrap();
    assert_eq!(
        record,
        PauseRecord {
            version: PAUSED_VERSION,
            reason: PauseReason::CmdStarts,
            at: T * 1000,
            count: 4,
            by: PausedBy::Clink,
            resume: None,
        }
    );
    Ok(())
}

#[test]
fn a_fresh_tag_of_another_window_is_neither_counted_nor_taken_away() -> mlua::Result<()> {
    // Windows Terminal opens several tabs at once (recovery spec §3.2 step 5).
    let mut disk = tags_of_age(&[1, 2], 61);
    disk.insert(tag(9), format!("v1 {}\n", T - 59));
    let w = open(&mut disk, window(3, T).prompted())?;
    assert!(!w.paused && w.omp);
    // A success: the stale ones go with its own, the other window's stays.
    assert_eq!(disk.keys().cloned().collect::<Vec<_>>(), [tag(9)]);
    Ok(())
}

#[test]
fn a_file_that_cannot_be_read_or_written_never_pauses() -> mlua::Result<()> {
    let stale = tags_of_age(&[1, 2, 3], 600);
    // Nothing can be read, written or listed: the counter is skipped, the theme loads.
    let mut disk = stale.clone();
    let w = open_with(&mut disk, window(4, T).prompted(), |lua| {
        lua.load("__fail_all = true").exec()
    })?;
    assert!(!w.paused && w.omp && w.shows_the_box());
    assert_eq!(w.prompt.as_deref(), Some("omp> "));
    assert_eq!(disk, stale);
    // paused.json alone can't be written: no pause, and the window counts as any other.
    let mut disk = stale.clone();
    let w = open_with(&mut disk, window(4, T), |lua| {
        lua.globals()
            .get::<Table>("__fail")?
            .set(paused_file(), true)
    })?;
    assert!(!w.paused && w.omp);
    assert!(disk.contains_key(&tag(4)) && !disk.contains_key(&paused_file()));
    Ok(())
}

#[test]
fn a_paused_theme_loads_no_prompt_and_no_welcome_box() -> mlua::Result<()> {
    let mut disk = Disk::from([(
        paused_file(),
        r#"{"version":1,"reason":"user","at":1,"count":0,"by":"app"}"#.to_string(),
    )]);
    let before = disk.clone();
    let w = open(&mut disk, window(7, T).prompted())?;
    assert!(w.paused && !w.omp && !w.shows_the_box());
    assert_eq!(w.lines(), [PAUSED_STILL]);
    assert_eq!(w.prompt.as_deref(), Some("C:\\>"));
    assert_eq!(disk, before, "no tag while paused");
    Ok(())
}

#[test]
fn a_damaged_pause_file_still_pauses() -> mlua::Result<()> {
    // Review Focus 3: init.lua asks only whether the file is there.
    for text in ["{ not json", "", "{\"version\":9}"] {
        let mut disk = Disk::from([(paused_file(), text.to_string())]);
        let w = open(&mut disk, window(7, T))?;
        assert!(w.paused && !w.omp, "{text:?}");
    }
    Ok(())
}

#[test]
fn the_lines_of_a_paused_window_are_one_line_of_english() -> mlua::Result<()> {
    let mut disk = tags_of_age(&[1, 2, 3, 4], 600);
    let w = open(&mut disk, window(5, T))?;
    assert_eq!(w.lines(), [PAUSED_NOW.replace("%d", "4")]);
    let w2 = open(&mut disk, window(6, T + 1))?;
    for line in w.lines().iter().chain(w2.lines().iter()) {
        assert!(line.is_ascii() && !line.contains('\n'), "{line:?}");
        assert!(
            !line.contains('\u{1b}'),
            "in the window's own colors: {line:?}"
        );
    }
    Ok(())
}

#[test]
fn a_tag_whose_time_cannot_be_read_is_not_counted_nor_one_from_the_future() -> mlua::Result<()> {
    // A clock set back makes every tag "from the future": never stale, never a pause.
    let mut disk = Disk::from([
        (tag(1), "v1 abc".to_string()),
        (tag(2), "v2 123\n".to_string()),
        (tag(3), "1791694000\n".to_string()),
        (tag(4), format!("v1 {}\n", T + 1000)),
    ]);
    let w = open(&mut disk, window(5, T).prompted())?;
    assert!(!w.paused && w.omp);
    assert_eq!(
        disk.len(),
        4,
        "they never go stale, so a success leaves them: {disk:?}"
    );
    Ok(())
}

#[test]
fn a_tag_another_window_is_writing_is_left_alone() -> mlua::Result<()> {
    // Review Focus 2: io.open makes the file before its text is written, so another window can
    // see it empty. It is not counted, and a success does not take it away.
    let mut disk = tags_of_age(&[1, 2], 600);
    disk.insert(tag(9), String::new());
    let w = open(&mut disk, window(3, T).prompted())?;
    assert!(!w.paused);
    assert_eq!(disk, Disk::from([(tag(9), String::new())]));
    Ok(())
}

#[test]
fn cmd_studios_own_clink_set_never_counts() -> mlua::Result<()> {
    // Review Focus 1: the apply's check and doctor D3 run `clink --profile <ours> set clink.logo`,
    // which loads init.lua in clink_x64.exe and never shows a prompt. Three of them in a row, or
    // three Home visits, must not pause the theme, nor leave a tag (spike S4).
    let mut disk = tags_of_age(&[1, 2, 3], 600);
    let before = disk.clone();
    for pid in 10..13 {
        let how = How {
            host: CLINK,
            inject: false,
            ..window(pid, T)
        };
        let w = open(&mut disk, how)?;
        assert!(
            !w.paused && w.omp,
            "it still loads the prompt, as it did before"
        );
        assert!(w.printed.is_empty(), "{:?}", w.printed);
    }
    assert_eq!(disk, before);
    // And with the theme paused it says nothing there either.
    let mut disk = Disk::from([(paused_file(), "{}".to_string())]);
    let w = open(
        &mut disk,
        How {
            host: CLINK,
            inject: false,
            ..window(14, T)
        },
    )?;
    assert!(w.paused && w.printed.is_empty());
    Ok(())
}

#[test]
fn whole_paths_from_globfiles_count_the_same() -> mlua::Result<()> {
    let mut disk = tags_of_age(&[1, 2, 3], 600);
    let w = open_with(&mut disk, window(4, T), |lua| {
        lua.load("__glob_paths = true").exec()
    })?;
    assert!(w.paused);
    assert_eq!(disk.keys().cloned().collect::<Vec<_>>(), [paused_file()]);
    Ok(())
}

#[test]
fn a_cmd_started_from_a_paused_window_says_nothing_again() -> mlua::Result<()> {
    let mut disk = Disk::from([(paused_file(), "{}".to_string())]);
    let first = open(&mut disk, window(7, T))?;
    assert_eq!(
        first.env.get("CMDSTUDIO_PAUSE_SAID").map(String::as_str),
        Some("1")
    );
    let nested = open_with(&mut disk, window(8, T), |lua| {
        lua.globals()
            .get::<Table>("__env")?
            .set("CMDSTUDIO_PAUSE_SAID", "1")
    })?;
    assert!(nested.paused && nested.printed.is_empty());
    Ok(())
}

#[test]
fn a_window_whose_prompt_never_comes_keeps_its_tag() -> mlua::Result<()> {
    // Oh My Posh hangs at the prompt and the window is closed during the hang: Clink never
    // waits for a key, so the coroutine never runs (spike S6b-c).
    let mut disk = Disk::new();
    let w = open_with(&mut disk, window(6, T).prompted(), |lua| {
        lua.globals().set(
            "__popen_result",
            "local p = clink.promptfilter(1)\nfunction p:filter() error('closed while oh-my-posh hung') end",
        )
    })?;
    assert_eq!(w.prompt, None);
    assert_eq!(disk.keys().cloned().collect::<Vec<_>>(), [tag(6)]);
    // The filters returned, but the window was closed before Clink waited for a key.
    let mut disk = Disk::new();
    let w = open(&mut disk, window(7, T).closed_at_its_prompt())?;
    assert_eq!(w.prompt.as_deref(), Some("omp> "));
    assert_eq!(disk.keys().cloned().collect::<Vec<_>>(), [tag(7)]);
    Ok(())
}

#[test]
fn a_refilter_on_the_same_line_clears_nothing_twice() -> mlua::Result<()> {
    // Clink filters the prompt again when the coroutine completes, and on every later line. The
    // filter asks for its coroutine on the first line only: a tag of another window that goes
    // stale afterwards is left to the next window's start.
    let mut disk = Disk::new();
    let w = open_steps(
        &mut disk,
        window(100, T).prompted(),
        |_| Ok(()),
        |lua| {
            lua.globals()
                .get::<Table>("__files")?
                .set(tag(7), format!("v1 {}\n", T - 600))?;
            lua.load("__prompt() __idle() __prompt() __idle() __new_line() __prompt() __idle()")
                .exec()
        },
    )?;
    assert_eq!(disk.keys().cloned().collect::<Vec<_>>(), [tag(7)]);
    assert_eq!(w.coroutine_calls, 1);
    Ok(())
}

#[test]
fn cmd_studios_filter_runs_before_oh_my_poshs_and_returns_no_value() -> mlua::Result<()> {
    // Oh My Posh registers its filter at _ZL_CLINK_PROMPT_PRIORITY + 1, and its right filter ends
    // the chain: no filter after it runs (spike S6). CMD Studio's runs before it whatever the
    // variable says, and returns no value, so the chain goes on and the prompt is Oh My Posh's.
    for (zl, omp) in [(None, 1i64), (Some("-5"), -4), (Some("50"), 51)] {
        let mut disk = Disk::new();
        let w = open_steps(
            &mut disk,
            window(100, T).prompted(),
            |lua| {
                if let Some(v) = zl {
                    lua.globals()
                        .get::<Table>("__env")?
                        .set("_ZL_CLINK_PROMPT_PRIORITY", v)?;
                }
                // A filter after Oh My Posh's, to show that the stand-in ends the chain there.
                lua.load(format!(
                    "local late = clink.promptfilter({})\nfunction late:filter() __late_ran = true end",
                    omp + 1
                ))
                .exec()
            },
            |lua| {
                assert!(!truthy(lua, "__late_ran")?, "{zl:?}");
                Ok(())
            },
        )?;
        assert_eq!(w.prompt.as_deref(), Some("omp> "), "{zl:?}");
        assert!(disk.is_empty(), "{zl:?}: {disk:?}");
        let priorities: Vec<i64> = w.filters.iter().map(|f| f.priority).collect();
        assert_eq!(priorities.len(), 3, "{zl:?}: {priorities:?}");
        assert!(
            priorities[0] < omp && priorities[1] == omp,
            "{zl:?}: {priorities:?}"
        );
        // Called for the first prompt and again when the coroutine completed: no value either
        // time.
        assert_eq!(w.filters[0].returned, [0, 0], "{zl:?}");
    }
    Ok(())
}

#[test]
fn a_raise_in_the_coroutine_is_logged_and_never_thrown() -> mlua::Result<()> {
    // Between the prompt and the wait for a key the folder can no longer be listed: os.globfiles
    // raises inside the coroutine. Clink would print an error raised there in the window.
    let mut disk = tags_of_age(&[1], 600);
    let w = open_steps(
        &mut disk,
        window(2, T).closed_at_its_prompt(),
        |_| Ok(()),
        |lua| {
            lua.load("__fail_all = true").exec()?;
            lua.globals().get::<Function>("__idle")?.call::<()>(())
        },
    )?;
    assert_eq!(w.counter_logs().len(), 1, "{:?}", w.logs);
    assert!(
        w.counter_logs()[0].contains("globfiles: access denied"),
        "{:?}",
        w.logs
    );
    assert!(w.lines().is_empty(), "{:?}", w.printed);
    assert_eq!(disk.len(), 2, "nothing could be removed: {disk:?}");
    Ok(())
}

#[test]
fn a_clink_that_cannot_give_a_prompt_coroutine_never_leaves_a_tag_to_count() -> mlua::Result<()> {
    // Without clink.promptcoroutine the counter cannot see a first prompt: it writes no tag,
    // counts none and says so in Clink's log, once. The theme loads as before.
    let stale = tags_of_age(&[1, 2, 3], 600);
    let mut disk = stale.clone();
    let w = open_with(&mut disk, window(4, T).prompted(), |lua| {
        lua.load("clink.promptcoroutine = nil").exec()
    })?;
    assert!(!w.paused && w.omp && w.shows_the_box());
    assert_eq!(w.prompt.as_deref(), Some("omp> "));
    assert_eq!(disk, stale);
    assert_eq!(w.counter_logs().len(), 1, "{:?}", w.logs);
    // One that raises: the window's own tag goes at once, and the others stay, since no start was
    // seen to work.
    let stale = tags_of_age(&[1, 2], 61);
    let mut disk = stale.clone();
    let w = open_with(&mut disk, window(3, T).prompted(), |lua| {
        lua.load("clink.promptcoroutine = function() error('no coroutine now') end")
            .exec()
    })?;
    assert_eq!(w.prompt.as_deref(), Some("omp> "));
    assert_eq!(disk, stale);
    assert_eq!(w.counter_logs().len(), 1, "{:?}", w.logs);
    assert!(
        w.counter_logs()[0].contains("no coroutine now"),
        "{:?}",
        w.logs
    );
    Ok(())
}

#[test]
fn with_prompt_async_off_a_start_is_never_counted() -> mlua::Result<()> {
    // Clink then runs the coroutine at once, inside CMD Studio's filter and before Oh My Posh's.
    // The tag goes before the prompt is drawn: a hang is not counted, and nothing is left behind
    // to pause the theme for nothing. CMD Studio leaves prompt.async at Clink's default, on.
    let mut disk = tags_of_age(&[1, 2], 61);
    let w = open_with(&mut disk, window(3, T).prompted(), |lua| {
        lua.load("__async = false").exec()
    })?;
    assert_eq!(w.prompt.as_deref(), Some("omp> "));
    assert!(!w.tag_seen_by_omp);
    assert!(disk.is_empty(), "{disk:?}");
    Ok(())
}

#[test]
fn loading_init_lua_prints_nothing_and_sets_no_variable() -> mlua::Result<()> {
    // Only the inject callbacks print, and only in a cmd window (spec §2.2 #4).
    let mut disk = Disk::from([(paused_file(), "{}".to_string())]);
    let w = open(
        &mut disk,
        How {
            inject: false,
            ..window(7, T)
        },
    )?;
    assert!(w.paused && w.printed.is_empty() && w.env.is_empty());
    Ok(())
}
