//! Opening things outside the window: a cmd window (spec §4.2 page 6, §4.3 "เปิด cmd"), Windows
//! Terminal's Store page (spec §4.2 page 2), GitHub Issues (spec §4.3), the data and log folders
//! (spec §2.1). This module only does what it is told: which cmd window to open is decided by
//! `Backend::cmd_launch`, and every external target is fixed here. Nothing from the UI or a theme
//! is ever run or opened (spec §5.10): the UI names one of four targets and nothing else.
use cmdstudio_core::generate::wt::CMD_PROFILE_GUID;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use ts_rs::TS;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ExternalTarget {
    /// Windows Terminal in the Store; the web page when the Store cannot open (G18).
    WindowsTerminalStore,
    GitHubIssues,
    DataFolder,
    LogsFolder,
}

pub const STORE_URL: &str = "ms-windows-store://pdp/?ProductId=9N0DX20HK701";
pub const TERMINAL_URL: &str = "https://aka.ms/terminal";
pub const ISSUES_URL: &str = "https://github.com/luraselenehalo/cmd-studio/issues";

/// How "Open cmd" opens a window, as `Backend::cmd_launch` decides it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CmdLaunch {
    /// `wt.exe -p <cmd profile guid>`: a Windows Terminal tab with the cmd profile CMD Studio
    /// themed. This is the window "Open cmd" prefers whenever a theme is applied and that
    /// profile is there, for both targets (ruled in the review of Task 14): it is the one
    /// window that shows the whole theme, the colors and font as well as the prompt and the
    /// welcome box. A plain console window shows no background color on Windows 10 (spec §1,
    /// the Windows 10 bullet), and a user who ticked "open Windows Terminal as cmd" in the
    /// wizard lives in Windows Terminal anyway.
    WindowsTerminalProfile,
    /// The fallback: `cmd.exe` in a new console window, as the user would start it, so their
    /// AutoRun runs and loads the theme. It opens when no theme is applied, when there is no
    /// usable Windows Terminal (none installed, or one never opened, which has no cmd
    /// profile), or when `wt.exe` fails to start. Windows 11 shows it in Windows Terminal
    /// itself when that is the default terminal.
    PlainCmd,
}

/// The program and arguments of a launch. Both are fixed: nothing in them comes from outside.
///
/// The plain `cmd.exe` is the fallback window of [`CmdLaunch::PlainCmd`]; the Windows Terminal
/// profile is preferred whenever a theme is applied and that profile is there, for both targets
/// (see [`CmdLaunch::WindowsTerminalProfile`]).
///
/// `cmd.exe` is started without `/d` on purpose: this window exists to show what AutoRun loads
/// (spec §4.2 page 6, doctor D3), and `/d` would switch AutoRun off.
pub fn command_line(launch: &CmdLaunch) -> (&'static str, Vec<String>) {
    match launch {
        CmdLaunch::WindowsTerminalProfile => (
            "wt.exe",
            vec!["-p".to_string(), CMD_PROFILE_GUID.to_string()],
        ),
        CmdLaunch::PlainCmd => ("cmd.exe", Vec::new()),
    }
}

/// One thing the shell is asked to open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Place {
    /// A link: opened by whatever handles it (the browser, the Store).
    Url(&'static str),
    /// A folder of CMD Studio's: opened in Explorer.
    Folder(PathBuf),
}

/// What `target` opens, in the order to try: the first that opens is enough. The Store page
/// comes first and the web page of Windows Terminal is the way out when the Store cannot open
/// it (spec §4.2 page 2, G18). The folders are the data folder and its log folder (spec §5.3).
pub fn external_places(target: ExternalTarget, data: &Path, logs: &Path) -> Vec<Place> {
    match target {
        ExternalTarget::WindowsTerminalStore => {
            vec![Place::Url(STORE_URL), Place::Url(TERMINAL_URL)]
        }
        ExternalTarget::GitHubIssues => vec![Place::Url(ISSUES_URL)],
        ExternalTarget::DataFolder => vec![Place::Folder(data.to_path_buf())],
        ExternalTarget::LogsFolder => vec![Place::Folder(logs.to_path_buf())],
    }
}

/// Starts the window `launch` names, in a new console, in `%USERPROFILE%`.
///
/// When the Windows Terminal profile cannot be started (`wt.exe` is missing or not on the
/// path: the Preview build's alias is `wtd.exe`, an unpackaged Terminal may not be on the
/// path), this falls back to the plain cmd window, and the fallback is silent: nothing is
/// reported and the first error is not kept. The user asked for a cmd window and gets one, which
/// is the point of the button; an error dialog would be worse than a window without the profile.
/// Only when the plain window cannot start either is there an error.
#[cfg(windows)]
pub fn open_cmd(launch: &CmdLaunch) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    use windows_sys::Win32::System::Threading::CREATE_NEW_CONSOLE;
    let (program, args) = command_line(launch);
    let mut command = std::process::Command::new(program);
    command.args(&args).creation_flags(CREATE_NEW_CONSOLE);
    if let Some(home) = std::env::var_os("USERPROFILE") {
        command.current_dir(home);
    }
    match command.spawn() {
        Ok(_) => Ok(()),
        // wt.exe missing after all: a plain window is better than nothing (silent, see above).
        Err(_) if *launch == CmdLaunch::WindowsTerminalProfile => open_cmd(&CmdLaunch::PlainCmd),
        Err(e) => Err(format!("{program} could not be started: {e}")),
    }
}

/// Opens what `target` names (see [`external_places`]). When nothing opens, the error is the
/// first failure's, with each later one's text after it (the Store's, then the web page's), so
/// none is lost: the caller puts the whole text in the error's details.
#[cfg(windows)]
pub fn open_external(target: ExternalTarget, data: &Path, logs: &Path) -> Result<(), String> {
    open_first(&external_places(target, data, logs), shell_open)
}

/// Tries `places` in order with `open`; the first that opens ends it. When none does, the error
/// is the first failure's text followed by each later one's, so neither the Store's reason nor
/// the web page's is lost. Not tied to Windows so that the order and the error can be tested
/// with an `open` of the test's own.
pub fn open_first(
    places: &[Place],
    mut open: impl FnMut(&Place) -> Result<(), String>,
) -> Result<(), String> {
    let mut failures = Vec::new();
    for place in places {
        match open(place) {
            Ok(()) => return Ok(()),
            Err(e) => failures.push(e),
        }
    }
    let mut failures = failures.into_iter();
    match failures.next() {
        Some(first) => Err(failures.fold(first, |all, next| format!("{all}; then {next}"))),
        None => Err("there is nothing to open".to_string()),
    }
}

/// COM on the calling thread, as the documentation of `ShellExecuteW` asks (apartment
/// threaded, without OLE 1 DDE): the IPC commands run on pool threads that never started it.
/// Balanced when dropped, and only if it started. Not `Send`: `CoUninitialize` must run on the
/// thread that initialized.
#[cfg(windows)]
struct ComScope {
    started: bool,
    _same_thread: std::marker::PhantomData<*const ()>,
}

#[cfg(windows)]
impl ComScope {
    fn enter() -> Self {
        use windows_sys::Win32::System::Com::{
            CoInitializeEx, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
        };
        // SAFETY: the reserved pointer is null as required; the flags are documented constants;
        // the call reads and writes none of our memory, and the result is read as a number only.
        let result = unsafe {
            CoInitializeEx(
                std::ptr::null(),
                (COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) as u32,
            )
        };
        // S_OK (0) and S_FALSE (1, already initialized in this mode) both count one more
        // initialization to balance; a failure (e.g. the thread is in another mode) counts none.
        ComScope {
            started: result >= 0,
            _same_thread: std::marker::PhantomData,
        }
    }
}

#[cfg(windows)]
impl Drop for ComScope {
    fn drop(&mut self) {
        if self.started {
            use windows_sys::Win32::System::Com::CoUninitialize;
            // SAFETY: `started` is true only when `CoInitializeEx` succeeded (S_OK or S_FALSE) in
            // `enter` on this same thread (the type is not `Send`), and this is its one balancing
            // call; it takes no arguments and touches no memory of ours.
            unsafe { CoUninitialize() };
        }
    }
}

/// `ShellExecuteW("open", place)`: a link in the browser or the Store, a folder in Explorer.
/// Windows reports success as a value above 32.
#[cfg(windows)]
fn shell_open(place: &Place) -> Result<(), String> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let wide = |s: &OsStr| -> Vec<u16> { s.encode_wide().chain(std::iter::once(0)).collect() };
    let (name, file) = match place {
        Place::Url(url) => (url.to_string(), wide(OsStr::new(url))),
        Place::Folder(path) => (path.display().to_string(), wide(path.as_os_str())),
    };
    let verb = wide(OsStr::new("open"));
    let _com = ComScope::enter();
    // SAFETY: both strings are NUL-terminated UTF-16 buffers that outlive the call; the other
    // pointers are null as the documentation allows; the call comes after the attempt to start
    // COM on this thread, which `_com` ends only when it drops, after the call; the result is
    // read as a number only.
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    if result as isize > 32 {
        Ok(())
    } else {
        Err(format!(
            "{name} could not be opened (ShellExecuteW returned {})",
            result as isize
        ))
    }
}

#[cfg(not(windows))]
pub fn open_cmd(_launch: &CmdLaunch) -> Result<(), String> {
    Err("CMD Studio runs on Windows only".to_string())
}

#[cfg(not(windows))]
pub fn open_external(_target: ExternalTarget, _data: &Path, _logs: &Path) -> Result<(), String> {
    Err("CMD Studio runs on Windows only".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The program and the arguments of each way to open a window. The literal text is the
    /// point: the profile is the one the planner writes (`CMD_PROFILE_GUID`), and the plain
    /// window is `cmd.exe` as the user would start it, so their AutoRun runs (spec §4.2 page 6).
    #[test]
    fn each_launch_is_one_program_with_fixed_arguments() {
        assert_eq!(
            command_line(&CmdLaunch::WindowsTerminalProfile),
            (
                "wt.exe",
                vec![
                    "-p".to_string(),
                    "{0caa0dad-35be-5f56-a8ff-afceeeaa6101}".to_string()
                ]
            )
        );
        assert_eq!(command_line(&CmdLaunch::PlainCmd), ("cmd.exe", Vec::new()));
    }

    /// Every target of `open_external` and what it asks the shell to open, in order. The text
    /// of the links is written out here, not taken from the constants, so a changed link fails
    /// this test (spec §4.2 page 2, §4.3, G18).
    #[test]
    fn each_external_target_opens_what_the_spec_names() {
        let data = PathBuf::from(r"C:\Users\tester\AppData\Local\ai.resincore.cmdstudio");
        let logs = data.join("logs");
        // The Store page first; the web page when the Store cannot open it.
        assert_eq!(
            external_places(ExternalTarget::WindowsTerminalStore, &data, &logs),
            [
                Place::Url("ms-windows-store://pdp/?ProductId=9N0DX20HK701"),
                Place::Url("https://aka.ms/terminal"),
            ]
        );
        assert_eq!(
            external_places(ExternalTarget::GitHubIssues, &data, &logs),
            [Place::Url(
                "https://github.com/luraselenehalo/cmd-studio/issues"
            )]
        );
        assert_eq!(
            external_places(ExternalTarget::DataFolder, &data, &logs),
            [Place::Folder(data.clone())]
        );
        assert_eq!(
            external_places(ExternalTarget::LogsFolder, &data, &logs),
            [Place::Folder(logs.clone())]
        );
        assert_ne!(data, logs, "the two folders must be told apart");
    }

    fn failing(text: &str) -> Result<(), String> {
        Err(text.to_string())
    }

    /// The places are tried in order and the first that opens ends it: the Store page opening
    /// means the web page is never asked.
    #[test]
    fn the_first_place_that_opens_ends_it() {
        let places = external_places(
            ExternalTarget::WindowsTerminalStore,
            Path::new("data"),
            Path::new("logs"),
        );
        let mut asked = Vec::new();
        let result = open_first(&places, |p| {
            asked.push(p.clone());
            Ok(())
        });
        assert_eq!(result, Ok(()));
        assert_eq!(asked, [Place::Url(STORE_URL)]);

        // The Store cannot open it: the web page is the way out, and its success is the result.
        let mut asked = Vec::new();
        let result = open_first(&places, |p| {
            asked.push(p.clone());
            match p {
                Place::Url(url) if *url == STORE_URL => failing("the Store said no"),
                _ => Ok(()),
            }
        });
        assert_eq!(result, Ok(()));
        assert_eq!(asked, [Place::Url(STORE_URL), Place::Url(TERMINAL_URL)]);
    }

    /// When nothing opens, the error is the first failure's, with the later ones after it: the
    /// reason the Store gave is not lost behind the web page's.
    #[test]
    fn when_nothing_opens_every_reason_is_kept_first_one_first() {
        let places = external_places(
            ExternalTarget::WindowsTerminalStore,
            Path::new("data"),
            Path::new("logs"),
        );
        let result = open_first(&places, |p| match p {
            Place::Url(url) if *url == STORE_URL => failing("store: no handler"),
            _ => failing("web: no browser"),
        });
        assert_eq!(
            result,
            Err("store: no handler; then web: no browser".to_string())
        );
        // One place: its own error, as it was.
        let one = [Place::Url(ISSUES_URL)];
        assert_eq!(
            open_first(&one, |_| failing("no browser")),
            Err("no browser".to_string())
        );
        assert_eq!(
            open_first(&[], |_| Ok(())),
            Err("there is nothing to open".to_string())
        );
    }

    /// The UI names a target; it never sends a link or a path (spec §5.10). The four names are
    /// the whole wire format, and anything else is refused before it reaches the shell.
    #[test]
    fn the_ui_can_only_name_a_target() {
        for (name, target) in [
            ("windowsTerminalStore", ExternalTarget::WindowsTerminalStore),
            ("gitHubIssues", ExternalTarget::GitHubIssues),
            ("dataFolder", ExternalTarget::DataFolder),
            ("logsFolder", ExternalTarget::LogsFolder),
        ] {
            let wire = serde_json::to_string(&target).unwrap();
            assert_eq!(wire, format!("\"{name}\""));
            assert_eq!(
                serde_json::from_str::<ExternalTarget>(&wire).unwrap(),
                target
            );
        }
        for free in [
            "\"https://example.com\"",
            r#""C:\\Windows\\System32\\cmd.exe""#,
            "\"windowsterminalstore\"",
            "{\"url\":\"https://example.com\"}",
            "42",
        ] {
            assert!(
                serde_json::from_str::<ExternalTarget>(free).is_err(),
                "{free}"
            );
        }
    }
}
