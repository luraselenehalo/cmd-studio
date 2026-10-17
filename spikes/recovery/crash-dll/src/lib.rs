//! A stand-in for Clink's `clink_dll_x64.dll` that crashes the process loading it, at once: an
//! illegal instruction (exception c000001d) inside this module, so that Windows records an
//! "Application Error" event 1000 for cmd.exe with this module as the faulting one (recovery
//! spec §0, §10). Test VMs only: it never goes into the installer, nor onto the dev PC's own
//! runtime.

/// `DLL_PROCESS_ATTACH`: the process is loading this module.
const DLL_PROCESS_ATTACH: u32 = 1;

/// The module's entry point, which the C runtime's own entry point calls.
#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn DllMain(
    _module: *mut core::ffi::c_void,
    reason: u32,
    _reserved: *mut core::ffi::c_void,
) -> i32 {
    if reason == DLL_PROCESS_ATTACH {
        // SAFETY: ud2 raises the illegal instruction exception on purpose; it never returns.
        unsafe { core::arch::asm!("ud2", options(noreturn)) }
    }
    1
}
