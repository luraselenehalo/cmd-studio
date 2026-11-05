//! The console of the CLI mode, and the one message box the app shows when it cannot start.
//! Windows only; elsewhere `attach_parent` does nothing and `fatal` prints to stderr.

/// What the standard output the CLI was started with is, as [`attach_parent`] sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stdout {
    /// None was given: the handle is null or `INVALID_HANDLE_VALUE`, as for a Windows-subsystem
    /// exe typed at a prompt with no redirection.
    Missing,
    /// A handle whose kind Windows cannot tell (`FILE_TYPE_UNKNOWN`), or one that is not open.
    Unknown,
    /// A file, a pipe or a console: `cmdstudio doctor > out.txt`, `| more`, or the
    /// uninstaller's own redirection. The output already goes somewhere.
    Usable,
}

impl Stdout {
    /// The kind of `handle`; `file_type` (`GetFileType`) is asked only about a handle that is
    /// there.
    #[cfg(windows)]
    pub fn of(
        handle: windows_sys::Win32::Foundation::HANDLE,
        file_type: impl FnOnce(windows_sys::Win32::Foundation::HANDLE) -> u32,
    ) -> Stdout {
        use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
        use windows_sys::Win32::Storage::FileSystem::FILE_TYPE_UNKNOWN;
        if handle.is_null() || handle == INVALID_HANDLE_VALUE {
            Stdout::Missing
        } else if file_type(handle) == FILE_TYPE_UNKNOWN {
            Stdout::Unknown
        } else {
            Stdout::Usable
        }
    }

    /// Whether the CLI attaches to its parent's console: only when its output goes nowhere
    /// yet. A redirection the CLI was started with is kept.
    pub fn needs_parent_console(self) -> bool {
        self != Stdout::Usable
    }
}

/// Attaches to the console of the parent process (the cmd or PowerShell window the exe was
/// started from), so that a Windows-subsystem build prints its CLI output there, but only when
/// the standard output goes nowhere yet ([`Stdout::needs_parent_console`]): with
/// `cmdstudio doctor > out.txt` or `| more` the output stays where it was sent. Then nothing is
/// attached for stderr either: it goes wherever the process was given, which may be nowhere,
/// so `2>&1` (or `2> err.txt`) is the way to keep the errors too. Does nothing when there is no
/// parent console (a program that gave none): the standard handles then stay as they were
/// inherited.
#[cfg(windows)]
pub fn attach_parent() {
    use windows_sys::Win32::Storage::FileSystem::GetFileType;
    use windows_sys::Win32::System::Console::{
        AttachConsole, GetStdHandle, ATTACH_PARENT_PROCESS, STD_OUTPUT_HANDLE,
    };
    // SAFETY: `GetStdHandle` takes a documented constant and returns the handle value the
    // process holds (or null, or `INVALID_HANDLE_VALUE`); it touches no memory of ours, and the
    // value is only compared and passed to `GetFileType` below, never dereferenced.
    let handle = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
    let stdout = Stdout::of(handle, |handle| {
        // SAFETY: `GetFileType` only asks the kernel about `handle`, which validates it: a
        // value that is not an open handle of this process makes it answer
        // `FILE_TYPE_UNKNOWN`, never read or write our memory. `Stdout::of` passes only a
        // handle that is neither null nor `INVALID_HANDLE_VALUE`.
        unsafe { GetFileType(handle) }
    });
    if stdout.needs_parent_console() {
        // SAFETY: `AttachConsole` takes a process id and touches no memory of ours; a failure
        // (no parent console, or a console already attached) is reported by its return value,
        // which is deliberately ignored: either way the standard handles are usable or inert.
        unsafe {
            AttachConsole(ATTACH_PARENT_PROCESS);
        }
    }
}

#[cfg(not(windows))]
pub fn attach_parent() {}

/// Shows `message` in a message box titled "CMD Studio" with `flags` (`MB_*`), and answers what
/// was pressed (`IDOK`, `IDYES`, `IDNO`). English, like every text outside the UI.
#[cfg(windows)]
fn message_box(message: &str, flags: u32) -> i32 {
    use windows_sys::Win32::UI::WindowsAndMessaging::MessageBoxW;
    let wide = |s: &str| -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() };
    let (text, title) = (wide(message), wide("CMD Studio"));
    // SAFETY: both strings are NUL-terminated UTF-16 buffers that outlive the call, the owner
    // window is null (no parent), and the flags are the caller's documented constants.
    unsafe { MessageBoxW(std::ptr::null_mut(), text.as_ptr(), title.as_ptr(), flags) }
}

/// Shows `message` and returns when it is closed. For a start that fails before the window
/// exists; English, like every text outside the UI.
#[cfg(windows)]
pub fn fatal(message: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK};
    message_box(message, MB_OK | MB_ICONERROR);
}

#[cfg(not(windows))]
pub fn fatal(message: &str) {
    eprintln!("CMD Studio: {message}");
}

/// Shows `message` with Yes and No, and answers whether Yes was pressed. For a window that
/// cannot be opened (recovery spec §5.4).
#[cfg(windows)]
pub fn ask_yes_no(message: &str) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{IDYES, MB_ICONQUESTION, MB_YESNO};
    message_box(message, MB_YESNO | MB_ICONQUESTION) == IDYES
}

#[cfg(not(windows))]
pub fn ask_yes_no(message: &str) -> bool {
    eprintln!("CMD Studio: {message}");
    false
}

/// The app's exe, which the Start-menu shortcuts and the uninstaller name.
pub const APP_EXE: &str = "cmdstudio.exe";
/// The console exe beside it (spec §5.7): the same CLI as a console-subsystem program, which cmd
/// waits for and gets an exit code from. `cmdstudio` typed in cmd or PowerShell is this one, as
/// PATHEXT tries `.COM` before `.EXE`.
pub const CONSOLE_EXE: &str = "cmdstudio.com";

/// What `cmdstudio.com` does with its arguments.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConsoleStart {
    /// `--recovery` alone: the window in recovery mode is the app's job (recovery spec §4.4);
    /// the console exe starts `cmdstudio.exe --recovery` beside itself and ends.
    OpenRecovery,
    /// Everything else is `cmdstudio_cli::run`'s. `--after-pid <pid>` included: it is
    /// `app_restart`'s own and never typed, and the CLI refuses it as it refuses any unknown
    /// option (exit code 2).
    Cli,
}

pub fn console_start(args: &[std::ffi::OsString]) -> ConsoleStart {
    match args {
        [one] if one == crate::RECOVERY_ARG => ConsoleStart::OpenRecovery,
        _ => ConsoleStart::Cli,
    }
}

/// Starts [`APP_EXE`] from this program's own folder with `args`, without waiting: the window
/// outlives the console program. `Err` says what failed, in English.
#[cfg(windows)]
pub fn start_app_beside(args: &[std::ffi::OsString]) -> Result<(), String> {
    let me = std::env::current_exe()
        .map_err(|e| format!("this program's own path could not be read: {e}"))?;
    let folder = me
        .parent()
        .ok_or_else(|| "this program has no folder".to_string())?;
    let app = folder.join(APP_EXE);
    std::process::Command::new(&app)
        .args(args)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("{} could not be started: {e}", app.display()))
}

#[cfg(all(test, windows))]
mod tests {
    use super::Stdout;
    use windows_sys::Win32::Foundation::{HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_TYPE_CHAR, FILE_TYPE_DISK, FILE_TYPE_PIPE, FILE_TYPE_UNKNOWN,
    };

    /// `cmdstudio doctor > out.txt` and `| more` keep their redirection: the CLI attaches to
    /// the parent's console only when its standard output goes nowhere.
    #[test]
    fn the_parent_console_is_attached_only_when_the_output_goes_nowhere() {
        let not_asked = |_: HANDLE| -> u32 { panic!("a missing handle has no type to ask about") };
        assert_eq!(Stdout::of(std::ptr::null_mut(), not_asked), Stdout::Missing);
        assert_eq!(Stdout::of(INVALID_HANDLE_VALUE, not_asked), Stdout::Missing);

        // A handle value is only passed on, never used here.
        let some: HANDLE = std::ptr::without_provenance_mut(0x44);
        assert_eq!(Stdout::of(some, |_| FILE_TYPE_UNKNOWN), Stdout::Unknown);
        // A file (`> out.txt`), a pipe (`| more`) or a console.
        for kind in [FILE_TYPE_DISK, FILE_TYPE_PIPE, FILE_TYPE_CHAR] {
            let asked = |h: HANDLE| {
                assert_eq!(h, some, "the type of the handle given is asked");
                kind
            };
            assert_eq!(Stdout::of(some, asked), Stdout::Usable);
        }

        assert!(Stdout::Missing.needs_parent_console());
        assert!(Stdout::Unknown.needs_parent_console());
        assert!(!Stdout::Usable.needs_parent_console());
    }
}

#[cfg(test)]
mod start_tests {
    use super::{console_start, ConsoleStart, APP_EXE, CONSOLE_EXE};
    use std::ffi::OsString;

    fn args(list: &[&str]) -> Vec<OsString> {
        list.iter().map(OsString::from).collect()
    }

    /// `cmdstudio.com --recovery` opens the window (the app's job): it is handed to
    /// `cmdstudio.exe`. Everything else, `--after-pid` and a `--recovery` with company
    /// included, is the CLI's, which refuses what it does not know with exit code 2.
    #[test]
    fn only_a_lone_recovery_flag_is_handed_to_the_app() {
        assert_eq!(
            console_start(&args(&["--recovery"])),
            ConsoleStart::OpenRecovery
        );
        for other in [
            vec![],
            vec!["doctor"],
            vec!["--recovery", "--data-dir", r"C:\d"],
            vec!["--after-pid", "1234"],
            vec!["--cleanup"],
        ] {
            assert_eq!(console_start(&args(&other)), ConsoleStart::Cli, "{other:?}");
        }
    }

    /// The two names, in one place each; the hooks file's `Rename` spells them too (Task 3's
    /// `config_guard` pins that side).
    #[test]
    fn the_two_programs_have_their_names() {
        assert_eq!(APP_EXE, "cmdstudio.exe");
        assert_eq!(CONSOLE_EXE, "cmdstudio.com");
    }
}
