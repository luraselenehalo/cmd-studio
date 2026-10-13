//! Windows implementations of the adapters (spec §5.2). Each one takes the
//! places it touches as parameters, so tests can aim it at a throwaway
//! registry key and temp folders; `real()` constructors use the user's own.
use super::AdapterError;
use std::ffi::{c_void, OsStr};
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use windows_sys::Win32::System::Diagnostics::Debug::{
    FormatMessageW, FORMAT_MESSAGE_FROM_HMODULE, FORMAT_MESSAGE_FROM_SYSTEM,
    FORMAT_MESSAGE_IGNORE_INSERTS,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;

pub mod fonts;
pub mod fs;
pub mod host;
pub mod process;
pub mod registry;
pub mod wt;

pub(crate) fn io_err(op: &str, path: &Path, e: &std::io::Error) -> AdapterError {
    AdapterError::new(format!("{op} {}", path.display()), english_io_error(e))
}

/// NUL-terminated UTF-16 for Win32 `PCWSTR` parameters.
pub(crate) fn wide(s: &OsStr) -> Vec<u16> {
    s.encode_wide().chain(std::iter::once(0)).collect()
}

/// The calling thread's last Win32 error as text, in English. Call it straight after the
/// failing call: any other Win32 call in between may replace the error.
pub(crate) fn last_error(context: impl Into<String>) -> AdapterError {
    AdapterError::new(context, english_io_error(&std::io::Error::last_os_error()))
}

/// The language id of US English (`MAKELANGID(LANG_ENGLISH, SUBLANG_ENGLISH_US)`).
const ENGLISH_US: u32 = 0x0409;

/// The most UTF-16 units of a message: the size std's own `io::Error` text uses.
const MESSAGE_UNITS: usize = 2048;

/// Windows' text for a system error code, in US English: "Access is denied." for 5.
///
/// Nothing CMD Studio writes into a cmd window may be Thai, and Windows' own error text would be
/// in the display language: `std::io::Error` formats an OS error with `FormatMessageW` and no
/// language, which on Thai Windows gives Thai. This asks for US English instead. When Windows
/// has no English text for the code (an unknown code, or a Windows without English messages,
/// `ERROR_RESOURCE_LANG_NOT_FOUND`), the text is a fixed English sentence that names the code:
/// `Windows error 5 (0x00000005)`. Never the text of another language.
pub(crate) fn english_message(code: u32) -> String {
    message_in(code, ENGLISH_US)
}

/// The facility bit of an NTSTATUS carried in an HRESULT (`FACILITY_NT_BIT`).
const FACILITY_NT_BIT: u32 = 0x1000_0000;

/// [`english_message`] for the language `lang`; tests give it a language Windows has none of.
fn message_in(code: u32, lang: u32) -> String {
    // As std does: an NTSTATUS in an HRESULT has its text in ntdll's messages, not the system's.
    let (mut source, mut flags, mut id) = (
        std::ptr::null::<c_void>(),
        FORMAT_MESSAGE_FROM_SYSTEM | FORMAT_MESSAGE_IGNORE_INSERTS,
        code,
    );
    if code & FACILITY_NT_BIT != 0 {
        let name = wide(OsStr::new("NTDLL.DLL"));
        // SAFETY: `name` is NUL-terminated and outlives the call. GetModuleHandleW only looks
        // the module up; ntdll is in every process, and the handle needs no freeing.
        let ntdll = unsafe { GetModuleHandleW(name.as_ptr()) };
        if !ntdll.is_null() {
            (source, flags, id) = (
                ntdll.cast_const(),
                flags | FORMAT_MESSAGE_FROM_HMODULE,
                code ^ FACILITY_NT_BIT,
            );
        }
    }
    let mut buf = [0u16; MESSAGE_UNITS];
    // SAFETY: the source is NULL with FROM_SYSTEM alone, or with FROM_HMODULE the handle of
    // ntdll, which stays loaded for the life of the process. IGNORE_INSERTS leaves inserts as
    // they are, so no argument list is read and it may be NULL. `buf` holds MESSAGE_UNITS
    // writable units, which is the size passed, and the call writes at most that many.
    let written = unsafe {
        FormatMessageW(
            flags,
            source,
            id,
            lang,
            buf.as_mut_ptr(),
            MESSAGE_UNITS as u32,
            std::ptr::null(),
        )
    };
    // The count leaves out the closing NUL. Like std, the CR LF at the end goes and the
    // message's own full stop stays: "Access is denied."
    let text = usize::try_from(written)
        .ok()
        .filter(|&n| n > 0)
        .and_then(|n| buf.get(..n))
        .and_then(|units| String::from_utf16(units).ok())
        .map(|text| text.trim_end().to_string())
        .filter(|text| !text.is_empty());
    text.unwrap_or_else(|| format!("Windows error {code} ({code:#010X})"))
}

/// What a `std::io::Error` says, with Windows' part in English. An OS error is
/// [`english_message`] and then the `(os error N)` that std itself adds, so on English Windows
/// the text is what `to_string()` gives, byte for byte. Any other error keeps `to_string()`:
/// that text is Rust's own, and English.
pub(crate) fn english_io_error(e: &std::io::Error) -> String {
    match e.raw_os_error() {
        Some(code) => format!("{} (os error {code})", english_message(code as u32)),
        None => e.to_string(),
    }
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
        // Windows' text in English, then std's own suffix.
        assert_eq!(e.message, "Access is denied. (os error 5)");
    }

    #[test]
    fn windows_error_text_is_english() {
        assert_eq!(english_message(5), "Access is denied.");
        assert_eq!(
            english_message(2),
            "The system cannot find the file specified."
        );
        // What std writes on English Windows, byte for byte: the text, then "(os error N)".
        for code in [2, 5, 32] {
            let e = std::io::Error::from_raw_os_error(code);
            assert_eq!(
                english_io_error(&e),
                format!("{} (os error {code})", english_message(code as u32))
            );
        }
        assert_eq!(
            english_io_error(&std::io::Error::from_raw_os_error(32)),
            "The process cannot access the file because it is being used by another process. \
             (os error 32)"
        );
    }

    #[test]
    fn an_nt_status_code_is_looked_up_in_ntdll_as_std_does() {
        // HRESULT_FROM_NT(STATUS_ACCESS_DENIED): the facility bit sends std, and this, to ntdll's
        // messages. The text is ntdll's, CR LF inside it and all, as std writes it.
        let code = 0xD000_0022_u32;
        assert_eq!(
            english_message(code),
            "{Access Denied}\r\nA process has requested access to an object, but has not been \
             granted those access rights."
        );
    }

    #[test]
    fn io_err_names_the_operation_and_says_windows_text_in_english() {
        let e = io_err(
            "read",
            Path::new(r"C:\x\state.json"),
            &std::io::Error::from_raw_os_error(32),
        );
        assert_eq!(e.context, r"read C:\x\state.json");
        assert_eq!(
            e.message,
            english_io_error(&std::io::Error::from_raw_os_error(32))
        );
        assert!(e.message.ends_with(". (os error 32)"), "{e}");
    }

    #[test]
    fn an_error_without_an_os_code_keeps_rusts_own_english_text() {
        let e = std::io::Error::other("the reader stopped");
        assert_eq!(english_io_error(&e), "the reader stopped");
        let e = std::io::Error::from(std::io::ErrorKind::UnexpectedEof);
        assert_eq!(english_io_error(&e), e.to_string());
    }

    #[test]
    fn a_code_windows_has_no_english_text_for_gets_a_fixed_english_sentence() {
        let text = english_message(0x7FFF_FFFF);
        assert_eq!(text, "Windows error 2147483647 (0x7FFFFFFF)");
        assert!(text.is_ascii(), "{text}");
        assert_eq!(
            english_io_error(&std::io::Error::from_raw_os_error(0x7FFF_FFFF)),
            "Windows error 2147483647 (0x7FFFFFFF) (os error 2147483647)"
        );
        // A language Windows has no messages in (0xFFFF is no language at all) gives the same
        // fallback: never the text of another language.
        assert_eq!(message_in(5, 0xFFFF), "Windows error 5 (0x00000005)");
        // The language asked for is US English.
        assert_eq!(ENGLISH_US, 0x0409);
        assert_eq!(english_message(5), message_in(5, ENGLISH_US));
    }

    #[test]
    fn the_text_is_english_whatever_the_threads_ui_language() {
        use windows_sys::Win32::Globalization::SetThreadUILanguage;
        // On Thai Windows FormatMessageW without a language gives Thai first. This machine may
        // have no Thai messages to give, but the text asked for in English stays English
        // either way. The test's thread is its own, and the language is put back.
        // SAFETY: SetThreadUILanguage takes a plain language id and changes this thread only.
        unsafe { SetThreadUILanguage(0x041E) };
        let thai_thread = (english_message(5), last_error_now(5));
        // SAFETY: as above; 0 sets the thread back to the user's UI language.
        unsafe { SetThreadUILanguage(0) };
        assert_eq!(
            thai_thread,
            (
                "Access is denied.".to_string(),
                "Access is denied. (os error 5)".to_string()
            )
        );
    }

    /// `last_error` right after the thread's last error was set to `code`.
    fn last_error_now(code: u32) -> String {
        // SAFETY: SetLastError takes a plain number and only stores it for this thread.
        unsafe { SetLastError(code) };
        last_error("x").message
    }
}
