//! Windows implementations of the adapters (spec §5.2). Each one takes the
//! places it touches as parameters, so tests can aim it at a throwaway
//! registry key and temp folders; `real()` constructors use the user's own.
use super::AdapterError;
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

pub mod fonts;
pub mod fs;
pub mod host;
pub mod process;
pub mod registry;
pub mod wt;

pub(crate) fn io_err(op: &str, path: &Path, e: &std::io::Error) -> AdapterError {
    AdapterError::new(format!("{op} {}", path.display()), e.to_string())
}

/// NUL-terminated UTF-16 for Win32 `PCWSTR` parameters.
pub(crate) fn wide(s: &OsStr) -> Vec<u16> {
    s.encode_wide().chain(std::iter::once(0)).collect()
}

/// The calling thread's last Win32 error as text. Call it straight after the
/// failing call: any other Win32 call in between may replace the error.
pub(crate) fn last_error(context: impl Into<String>) -> AdapterError {
    AdapterError::new(context, std::io::Error::last_os_error().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::Foundation::SetLastError;

    #[test]
    fn wide_is_utf16_with_one_closing_nul() {
        assert_eq!(wide(OsStr::new("")), vec![0]);
        assert_eq!(wide(OsStr::new("a\\b")), vec![0x61, 0x5C, 0x62, 0]);
        // Thai (one unit each) and an emoji (a surrogate pair).
        assert_eq!(
            wide(OsStr::new("ไท😀")),
            vec![0x0E44, 0x0E17, 0xD83D, 0xDE00, 0]
        );
    }

    #[test]
    fn last_error_keeps_its_context_and_the_threads_error_code() {
        // SAFETY: SetLastError takes a plain number and only stores it for this thread.
        unsafe { SetLastError(5) };
        let e = last_error("OpenSomething");
        assert_eq!(e.context, "OpenSomething");
        // "(os error 5)" is std's own suffix, so the check holds in any display language.
        assert!(e.message.contains("(os error 5)"), "{e}");
    }
}
