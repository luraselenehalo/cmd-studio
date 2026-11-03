//! A program named cmd.exe on the test VM that loads the crash stand-in named
//! clink_dll_x64.dll, for spike S11's fallback: Windows then records an event 1000 with
//! AppName cmd.exe and ModuleName clink_dll_x64.dll. Test VMs only.
use std::os::windows::ffi::OsStrExt;

#[link(name = "kernel32")]
extern "system" {
    fn LoadLibraryW(name: *const u16) -> *mut core::ffi::c_void;
}

fn main() {
    let path: Vec<u16> = std::env::args_os()
        .nth(1)
        .map(|a| a.encode_wide().chain(Some(0)).collect())
        .unwrap_or_else(|| vec![0]);
    // SAFETY: a NUL-terminated UTF-16 path; the module it names crashes this process on
    // purpose when it loads.
    unsafe { LoadLibraryW(path.as_ptr()) };
}
