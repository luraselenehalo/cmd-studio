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
    /// themed, which shows the whole theme (colors, font, prompt and welcome box).
    WindowsTerminalProfile,
    /// `cmd.exe` in a new console window, as the user would start it: their AutoRun runs, which
    /// is what loads the theme. Windows 11 shows it in Windows Terminal itself when that is the
    /// default terminal.
    PlainCmd,
}

/// The program and arguments of a launch. Both are fixed: nothing in them comes from outside.
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
        // wt.exe missing after all: a plain window is better than nothing.
        Err(_) if *launch == CmdLaunch::WindowsTerminalProfile => open_cmd(&CmdLaunch::PlainCmd),
        Err(e) => Err(format!("{program} could not be started: {e}")),
    }
}

#[cfg(windows)]
pub fn open_external(target: ExternalTarget, data: &Path, logs: &Path) -> Result<(), String> {
    let mut failure = None;
    for place in external_places(target, data, logs) {
        match shell_open(&place) {
            Ok(()) => return Ok(()),
            Err(e) => failure = Some(e),
        }
    }
    Err(failure.unwrap_or_else(|| "there is nothing to open".to_string()))
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
    // SAFETY: both strings are NUL-terminated UTF-16 buffers that outlive the call; the other
    // pointers are null as the documentation allows; the result is read as a number only.
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
