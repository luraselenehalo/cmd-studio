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

/// Shows `message` in a message box titled "CMD Studio" and returns when it is closed. For a
/// start that fails before the window exists; English, like every text outside the UI.
#[cfg(windows)]
pub fn fatal(message: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};
    let wide = |s: &str| -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() };
    let (text, title) = (wide(message), wide("CMD Studio"));
    // SAFETY: both strings are NUL-terminated UTF-16 buffers that outlive the call, the owner
    // window is null (no parent), and the flags are documented constants.
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            title.as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}

#[cfg(not(windows))]
pub fn fatal(message: &str) {
    eprintln!("CMD Studio: {message}");
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
