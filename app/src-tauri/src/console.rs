//! The console of the CLI mode, and the one message box the app shows when it cannot start.
//! Windows only; elsewhere these do nothing.

/// Attaches to the console of the parent process (the cmd or PowerShell window the exe was
/// started from), so that a Windows-subsystem build prints its CLI output there. Does nothing
/// when there is none (started by the uninstaller with its output redirected, or by a program
/// that gave no console): the standard handles then stay as they were inherited.
#[cfg(windows)]
pub fn attach_parent() {
    use windows_sys::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
    // SAFETY: `AttachConsole` takes a process id and touches no memory of ours; a failure (no
    // parent console, or a console already attached) is reported by its return value, which
    // is deliberately ignored: either way the standard handles are usable or inert.
    unsafe {
        AttachConsole(ATTACH_PARENT_PROCESS);
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
