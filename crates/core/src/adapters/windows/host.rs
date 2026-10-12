//! Identity, known folders, 8.3 names, the private ProgramData folder
//! (spec §5.4 `<clink-launch>` step 3) and system facts.
use super::registry::RegLocation;
use super::{io_err, last_error, wide};
use crate::adapters::{
    AdapterError, Arch, Created, DefaultTerminal, DirState, Host, RegValue, SystemInfo,
};
use std::ffi::{c_void, OsStr, OsString};
use std::io::ErrorKind;
use std::os::windows::ffi::OsStringExt;
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use windows_sys::core::{GUID, PWSTR};
use windows_sys::Win32::Foundation::{
    GetLastError, APPMODEL_ERROR_NO_PACKAGE, ERROR_ALREADY_EXISTS, HANDLE,
};
use windows_sys::Win32::Globalization::GetUserDefaultUILanguage;
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
    GetNamedSecurityInfoW, SDDL_REVISION_1, SE_FILE_OBJECT,
};
use windows_sys::Win32::Security::{
    AclSizeInformation, GetAce, GetAclInformation, GetTokenInformation, TokenUser,
    ACCESS_ALLOWED_ACE, ACE_HEADER, ACL, ACL_SIZE_INFORMATION, DACL_SECURITY_INFORMATION,
    OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID, SECURITY_ATTRIBUTES, TOKEN_QUERY,
    TOKEN_USER,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateDirectoryW, GetShortPathNameW, FILE_ATTRIBUTE_REPARSE_POINT,
};
use windows_sys::Win32::Storage::Packaging::Appx::GetCurrentPackageFullName;
use windows_sys::Win32::System::Com::CoTaskMemFree;
use windows_sys::Win32::System::SystemInformation::{
    IMAGE_FILE_MACHINE_AMD64, IMAGE_FILE_MACHINE_ARM64,
};
use windows_sys::Win32::System::SystemServices::{ACCESS_ALLOWED_ACE_TYPE, ACCESS_DENIED_ACE_TYPE};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, IsWow64Process2, OpenProcessToken};
use windows_sys::Win32::UI::Shell::{
    FOLDERID_LocalAppData, FOLDERID_ProgramData, SHGetKnownFolderPath, KF_FLAG_DEFAULT,
};

const LOCAL_SYSTEM: &str = "S-1-5-18";
const ADMINISTRATORS: &str = "S-1-5-32-544";

/// Rights that let a SID change what is in a folder or who may: write,
/// append, extended attributes, delete child, attributes, delete, change the
/// access list, take ownership, and the generic forms.
const WRITE_RIGHTS: u32 = 0x0000_0002 // FILE_WRITE_DATA / FILE_ADD_FILE
    | 0x0000_0004 // FILE_APPEND_DATA / FILE_ADD_SUBDIRECTORY
    | 0x0000_0010 // FILE_WRITE_EA
    | 0x0000_0040 // FILE_DELETE_CHILD
    | 0x0000_0100 // FILE_WRITE_ATTRIBUTES
    | 0x0001_0000 // DELETE
    | 0x0004_0000 // WRITE_DAC
    | 0x0008_0000 // WRITE_OWNER
    | 0x1000_0000 // GENERIC_ALL
    | 0x4000_0000; // GENERIC_WRITE

const WINDOWS_VERSION: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";
const CONSOLE_STARTUP: &str = r"Console\%%Startup";
const WEBVIEW2_CLIENT: &str = "{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";

/// The real machine: the current user, the known folders and the system facts.
#[derive(Clone, Copy, Debug, Default)]
pub struct WinHost;

/// Owning guards for Win32 resources. Their fields are private to this module, so the only way to
/// make one is its `unsafe fn new`, whose `# Safety` section says what the caller promises.
mod guard {
    use std::ffi::c_void;
    use windows_sys::Win32::Foundation::{CloseHandle, LocalFree, HANDLE};

    /// A block that `LocalAlloc` handed out (a security descriptor, a SID string).
    /// `LocalFree` runs when the guard goes out of scope, on every path out of a function.
    pub(super) struct LocalBlock(*mut c_void);

    impl LocalBlock {
        /// # Safety
        /// `block` must be NULL or a block that a Win32 call allocated with `LocalAlloc`, and
        /// nothing else may free it.
        pub(super) unsafe fn new(block: *mut c_void) -> Self {
            LocalBlock(block)
        }

        pub(super) fn as_ptr(&self) -> *mut c_void {
            self.0
        }
    }

    impl Drop for LocalBlock {
        fn drop(&mut self) {
            // SAFETY: the caller of `new` promised the pointer is NULL or a LocalAlloc block that
            // nothing else frees, and this guard is its only owner.
            unsafe { LocalFree(self.0) };
        }
    }

    /// A process token, closed when the guard goes out of scope.
    pub(super) struct Token(HANDLE);

    impl Token {
        /// # Safety
        /// `handle` must be an open handle that the caller owns, such as one from a successful
        /// `OpenProcessToken`, and nothing else may close it.
        pub(super) unsafe fn new(handle: HANDLE) -> Self {
            Token(handle)
        }

        pub(super) fn handle(&self) -> HANDLE {
            self.0
        }
    }

    impl Drop for Token {
        fn drop(&mut self) {
            // SAFETY: the caller of `new` promised the handle is open and owned by this guard
            // alone.
            unsafe { CloseHandle(self.0) };
        }
    }
}

use guard::{LocalBlock, Token};

/// The UTF-16 text a Win32 call returned, up to its NUL.
///
/// # Safety
/// `p` must point to a NUL-terminated UTF-16 string that stays valid during the call.
unsafe fn from_wide_ptr(p: *const u16) -> OsString {
    let mut len = 0usize;
    loop {
        // SAFETY: the caller promises a terminating NUL, and the loop stops at the first one,
        // so every unit read here lies inside the string.
        let unit = unsafe { *p.add(len) };
        if unit == 0 {
            break;
        }
        len += 1;
    }
    // SAFETY: the `len` units before the NUL were all just read, so they are valid for reads.
    OsString::from_wide(unsafe { std::slice::from_raw_parts(p, len) })
}

fn known_folder(id: &GUID, name: &str) -> Result<PathBuf, AdapterError> {
    let mut p: PWSTR = std::ptr::null_mut();
    // SAFETY: `id` is a valid GUID and `p` a valid out-pointer; the NULL token means the current
    // user. Whatever `p` holds afterwards is freed below, whether the call worked or not.
    let hr =
        unsafe { SHGetKnownFolderPath(id, KF_FLAG_DEFAULT as u32, std::ptr::null_mut(), &mut p) };
    let path = if hr >= 0 && !p.is_null() {
        // SAFETY: on success `p` is a NUL-terminated UTF-16 path, valid until it is freed below.
        let text = unsafe { from_wide_ptr(p) };
        Ok(PathBuf::from(text))
    } else {
        Err(AdapterError::new(
            format!("SHGetKnownFolderPath {name}"),
            format!("HRESULT {hr:#010x}"),
        ))
    };
    // SAFETY: `p` is NULL or the CoTaskMem string from the call above. The docs ask for it to
    // be freed even when the call failed, and nothing reads it after this.
    unsafe { CoTaskMemFree(p.cast::<c_void>()) };
    path
}

/// The SID, or an error when the pointer is NULL. A security descriptor can have no owner, and
/// `sid_string` must only ever be given a SID.
fn non_null_sid(sid: PSID, context: &str) -> Result<PSID, AdapterError> {
    if sid.is_null() {
        Err(AdapterError::new(context, "there is no SID"))
    } else {
        Ok(sid)
    }
}

/// # Safety
/// `sid` must point to a valid SID.
unsafe fn sid_string(sid: PSID) -> Result<String, AdapterError> {
    let mut s: PWSTR = std::ptr::null_mut();
    // SAFETY: the caller promises `sid` is a valid SID; `s` is a valid out-pointer.
    let converted = unsafe { ConvertSidToStringSidW(sid, &mut s) };
    if converted == 0 {
        return Err(last_error("ConvertSidToStringSidW"));
    }
    // SAFETY: success gave a string that ConvertSidToStringSidW allocated with LocalAlloc, and
    // nothing else frees it; the guard does, after the text is copied out.
    let s = unsafe { LocalBlock::new(s.cast::<c_void>()) };
    // SAFETY: success gives a NUL-terminated UTF-16 string, valid until the guard drops.
    let text = unsafe { from_wide_ptr(s.as_ptr().cast::<u16>()) };
    Ok(text.to_string_lossy().into_owned())
}

fn current_user_sid() -> Result<String, AdapterError> {
    let mut handle: HANDLE = std::ptr::null_mut();
    // SAFETY: GetCurrentProcess returns a pseudo-handle that needs no closing; `handle` is a
    // valid out-pointer.
    let opened = unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut handle) };
    if opened == 0 {
        return Err(last_error("OpenProcessToken"));
    }
    // SAFETY: `handle` came from the successful OpenProcessToken above, and nothing else closes it.
    let token = unsafe { Token::new(handle) };
    let mut len = 0u32;
    // SAFETY: a size query: no buffer with length 0 is allowed, and `len` receives the size needed.
    unsafe { GetTokenInformation(token.handle(), TokenUser, std::ptr::null_mut(), 0, &mut len) };
    if len == 0 {
        return Err(last_error("GetTokenInformation (size)"));
    }
    // Whole u64 words: at least `len` bytes, and aligned for TOKEN_USER's pointer.
    let mut buf = vec![0u64; (len as usize).div_ceil(8)];
    // SAFETY: `buf` holds at least `len` writable bytes, which is the length passed.
    let got = unsafe {
        GetTokenInformation(
            token.handle(),
            TokenUser,
            buf.as_mut_ptr().cast::<c_void>(),
            len,
            &mut len,
        )
    };
    if got == 0 {
        return Err(last_error("GetTokenInformation"));
    }
    // SAFETY: success wrote a TOKEN_USER at the start of `buf`, which is 8-byte aligned.
    let sid = unsafe { (*buf.as_ptr().cast::<TOKEN_USER>()).User.Sid };
    let sid = non_null_sid(sid, "GetTokenInformation")?;
    // SAFETY: `sid` is not NULL (just checked) and points into `buf`, which outlives the call.
    unsafe { sid_string(sid) }
}

/// Allow-entries of an access list: (SID, rights).
type AllowEntries = Vec<(String, u32)>;

/// The folder's owner SID and its allow-entries; `None` for a NULL access list.
fn owner_and_allowed(dir: &Path) -> Result<(String, Option<AllowEntries>), AdapterError> {
    let path = wide(dir.as_os_str());
    let mut owner: PSID = std::ptr::null_mut();
    let mut dacl: *mut ACL = std::ptr::null_mut();
    let mut sd: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    // SAFETY: `path` is NUL-terminated and outlives the call; the owner, DACL and descriptor
    // out-pointers are valid locals, and the group and SACL ones are NULL because those parts
    // are not asked for.
    let rc = unsafe {
        GetNamedSecurityInfoW(
            path.as_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            std::ptr::null_mut(),
            &mut dacl,
            std::ptr::null_mut(),
            &mut sd,
        )
    };
    if rc != 0 {
        return Err(AdapterError::new(
            format!("GetNamedSecurityInfoW {}", dir.display()),
            std::io::Error::from_raw_os_error(rc as i32).to_string(),
        ));
    }
    // SAFETY: GetNamedSecurityInfoW allocated the descriptor with LocalAlloc, and nothing else
    // frees it. `owner` and `dacl` point into it, so the guard keeps it until the function returns.
    let _descriptor = unsafe { LocalBlock::new(sd) };
    // `owner` is NULL when the descriptor has no owner.
    let owner = non_null_sid(owner, &format!("GetNamedSecurityInfoW {}", dir.display()))?;
    // SAFETY: `owner` is not NULL (just checked), so it is the owner SID inside the descriptor,
    // which the guard keeps alive.
    let owner = unsafe { sid_string(owner) }?;
    if dacl.is_null() {
        return Ok((owner, None));
    }
    let mut info = ACL_SIZE_INFORMATION {
        AceCount: 0,
        AclBytesInUse: 0,
        AclBytesFree: 0,
    };
    // SAFETY: `dacl` is the ACL inside the descriptor; `info` is the structure that
    // AclSizeInformation fills in, and its size is the length passed.
    let got = unsafe {
        GetAclInformation(
            dacl,
            (&raw mut info).cast::<c_void>(),
            std::mem::size_of::<ACL_SIZE_INFORMATION>() as u32,
            AclSizeInformation,
        )
    };
    if got == 0 {
        return Err(last_error("GetAclInformation"));
    }
    let mut allowed = Vec::new();
    for index in 0..info.AceCount {
        let mut ace: *mut c_void = std::ptr::null_mut();
        // SAFETY: `index` is below the ACL's entry count, and `ace` receives a pointer to that
        // entry inside `dacl`.
        let found = unsafe { GetAce(dacl, index, &mut ace) };
        if found == 0 {
            return Err(last_error("GetAce"));
        }
        // SAFETY: every ACE starts with an ACE_HEADER, and `ace` points at one.
        let ace_type = unsafe { (*ace.cast::<ACE_HEADER>()).AceType };
        match u32::from(ace_type) {
            ACCESS_ALLOWED_ACE_TYPE => {
                let entry = ace.cast::<ACCESS_ALLOWED_ACE>();
                // SAFETY: an ACE of this type is an ACCESS_ALLOWED_ACE, so its mask is readable.
                // The SID starts at `SidStart` and runs on to the end of the ACE; the pointer is
                // taken from `entry` itself, so it may reach past that one field.
                let (mask, sid) =
                    unsafe { ((*entry).Mask, (&raw mut (*entry).SidStart).cast::<c_void>()) };
                // SAFETY: that is a valid SID inside the ACL, which the guard keeps alive.
                let sid = unsafe { sid_string(sid) }?;
                allowed.push((sid, mask));
            }
            // A deny entry only takes rights away.
            ACCESS_DENIED_ACE_TYPE => {}
            other => allowed.push((format!("<ACE type {other}>"), u32::MAX)),
        }
    }
    Ok((owner, Some(allowed)))
}

/// Creates `dir` with the security descriptor `sddl` in one call.
pub(crate) fn create_dir_with_sddl(dir: &Path, sddl: &str) -> Result<Created, AdapterError> {
    let text = wide(OsStr::new(sddl));
    let mut sd: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    // SAFETY: `text` is NUL-terminated and outlives the call; `sd` is a valid out-pointer, and
    // the descriptor-size out-pointer may be NULL.
    let converted = unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            text.as_ptr(),
            SDDL_REVISION_1,
            &mut sd,
            std::ptr::null_mut(),
        )
    };
    if converted == 0 {
        return Err(last_error(
            "ConvertStringSecurityDescriptorToSecurityDescriptorW",
        ));
    }
    // SAFETY: the call above allocated the descriptor with LocalAlloc, and nothing else frees it;
    // the guard frees it on every path out of here.
    let descriptor = unsafe { LocalBlock::new(sd) };
    let attributes = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor.as_ptr(),
        bInheritHandle: 0,
    };
    let path = wide(dir.as_os_str());
    // SAFETY: `path` is NUL-terminated and `attributes` points at a live descriptor; both
    // outlive the call.
    let created = unsafe { CreateDirectoryW(path.as_ptr(), &attributes) };
    // SAFETY: GetLastError takes no arguments. It is read right after the call it belongs to,
    // before anything else can change it.
    let code = unsafe { GetLastError() };
    match (created, code) {
        (0, ERROR_ALREADY_EXISTS) => Ok(Created::AlreadyExisted),
        (0, _) => Err(AdapterError::new(
            format!("CreateDirectoryW {}", dir.display()),
            std::io::Error::from_raw_os_error(code as i32).to_string(),
        )),
        _ => Ok(Created::New),
    }
}

/// Owner in {user, SYSTEM, Administrators} and nobody else may write (spec §5.4).
pub(crate) fn judge(me: &str, owner: &str, allowed: Option<&[(String, u32)]>) -> DirState {
    let trusted = |sid: &str| sid == me || sid == LOCAL_SYSTEM || sid == ADMINISTRATORS;
    if !trusted(owner) {
        return DirState::Unsafe(format!("owned by {owner}"));
    }
    let Some(allowed) = allowed else {
        return DirState::Unsafe("it has no access list, so everyone has full access".into());
    };
    match allowed
        .iter()
        .find(|(sid, mask)| mask & WRITE_RIGHTS != 0 && !trusted(sid))
    {
        Some((sid, _)) => DirState::Unsafe(format!("{sid} may change it")),
        None => DirState::Private,
    }
}

/// How the registry facts read one value. `WinHost` passes `RegLocation::read_raw`; tests pass
/// readers that fail, answer from a table, or record what they were asked.
pub(crate) type RegRead<'a> =
    &'a dyn Fn(&RegLocation, &str) -> Result<Option<RegValue>, AdapterError>;

/// The facts that come from the registry, and the ones that could not be read.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct RegistryFacts {
    pub(crate) build: Option<u32>,
    pub(crate) default_terminal: DefaultTerminal,
    pub(crate) webview2: Option<String>,
    /// `"<fact>: <error>"`, one per fact that could not be read.
    pub(crate) unreadable: Vec<String>,
}

/// Where a key lives, for the messages: `LocalMachine\SOFTWARE\...`.
fn place(loc: &RegLocation) -> String {
    format!("{:?}\\{}", loc.hive, loc.subkey)
}

/// The text of one value. `Ok(None)` when it is absent; `Err` with the reason when the read
/// failed or the value is not text.
fn read_text(read: RegRead<'_>, loc: &RegLocation, name: &str) -> Result<Option<String>, String> {
    match read(loc, name) {
        Ok(None) => Ok(None),
        Ok(Some(value)) => value
            .text()
            .map(Some)
            .ok_or_else(|| format!("{name} in {} is not text", place(loc))),
        Err(e) => Err(e.to_string()),
    }
}

fn build_number(read: RegRead<'_>) -> Result<Option<u32>, String> {
    let loc = RegLocation::machine(WINDOWS_VERSION);
    let Some(text) = read_text(read, &loc, "CurrentBuildNumber")? else {
        return Ok(None);
    };
    text.trim().parse().map(Some).map_err(|_| {
        format!(
            "CurrentBuildNumber in {} is not a number: {text:?}",
            place(&loc)
        )
    })
}

fn default_terminal_setting(read: RegRead<'_>) -> Result<DefaultTerminal, String> {
    let loc = RegLocation::user(CONSOLE_STARTUP);
    let value = read_text(read, &loc, "DelegationConsole")?;
    Ok(terminal_from(value.as_deref()))
}

/// The WebView2 Runtime version, from the machine's key and then the user's. A version found in
/// either counts, whatever the other says or fails to say. With none found, a key that could not
/// be read leaves "not installed" unknown, which is an error naming every key that failed.
fn webview2_version(read: RegRead<'_>) -> Result<Option<String>, String> {
    let keys = [
        RegLocation::machine(&format!(
            r"SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{WEBVIEW2_CLIENT}"
        )),
        RegLocation::user(&format!(
            r"Software\Microsoft\EdgeUpdate\Clients\{WEBVIEW2_CLIENT}"
        )),
    ];
    let mut failures = Vec::new();
    for key in &keys {
        match read_text(read, key, "pv") {
            Ok(Some(version)) if is_installed_version(&version) => return Ok(Some(version)),
            Ok(_) => {}
            Err(why) => failures.push(why),
        }
    }
    if failures.is_empty() {
        Ok(None)
    } else {
        Err(failures.join("; "))
    }
}

/// The facts that come from the registry. What cannot be read is not answered: its field holds
/// the "absent" value and `unreadable` says why. A value that is simply not there is absent, and
/// not listed.
pub(crate) fn registry_facts(read: RegRead<'_>) -> RegistryFacts {
    let mut unreadable = Vec::new();
    let build = build_number(read).unwrap_or_else(|why| {
        unreadable.push(format!("build: {why}"));
        None
    });
    let default_terminal = default_terminal_setting(read).unwrap_or_else(|why| {
        unreadable.push(format!("default terminal: {why}"));
        DefaultTerminal::LetWindowsDecide
    });
    let webview2 = webview2_version(read).unwrap_or_else(|why| {
        unreadable.push(format!("WebView2: {why}"));
        None
    });
    RegistryFacts {
        build,
        default_terminal,
        webview2,
        unreadable,
    }
}

/// The terminal `DelegationConsole` names; the values are from
/// `policies/WindowsTerminal.admx` in microsoft/terminal.
fn terminal_from(delegation_console: Option<&str>) -> DefaultTerminal {
    match delegation_console.map(str::to_ascii_uppercase).as_deref() {
        None | Some("{00000000-0000-0000-0000-000000000000}") => DefaultTerminal::LetWindowsDecide,
        Some("{2EACA947-7F5F-4CFA-BA87-8F7FBEEFBE69}") => DefaultTerminal::WindowsTerminal,
        Some("{06EC847C-C0A5-46B8-92CB-7C92F6E35CD5}") => DefaultTerminal::WindowsTerminalPreview,
        Some("{B23D10C0-E52E-411E-9D5B-C09FDF709C7D}") => DefaultTerminal::Conhost,
        Some(_) => DefaultTerminal::Other,
    }
}

/// Edge Update writes `0.0.0.0` as the version when the runtime is not installed.
fn is_installed_version(pv: &str) -> bool {
    !pv.is_empty() && pv != "0.0.0.0"
}

/// The machine's architecture, not the process's: x64 code emulated on ARM64 is still on ARM64.
fn arch_of(native_machine: u16) -> Arch {
    match native_machine {
        IMAGE_FILE_MACHINE_AMD64 => Arch::X64,
        IMAGE_FILE_MACHINE_ARM64 => Arch::Arm64,
        _ => Arch::Other,
    }
}

fn arch() -> Arch {
    let (mut process, mut native) = (0u16, 0u16);
    // SAFETY: GetCurrentProcess returns a pseudo-handle; `process` and `native` are valid
    // out-pointers.
    let ok = unsafe { IsWow64Process2(GetCurrentProcess(), &mut process, &mut native) };
    if ok == 0 {
        return Arch::Other;
    }
    arch_of(native)
}

fn packaged() -> bool {
    let mut len = 0u32;
    // SAFETY: a length query: `len` is a valid in/out-pointer, and a NULL buffer with length 0
    // is the documented way to ask only whether the process has a package.
    let rc = unsafe { GetCurrentPackageFullName(&mut len, std::ptr::null_mut()) };
    rc != APPMODEL_ERROR_NO_PACKAGE
}

/// All the facts, with the registry read through `read`. `WinHost::system_info` passes the real
/// reader; tests pass their own.
pub(crate) fn system_info_with(read: RegRead<'_>) -> SystemInfo {
    let facts = registry_facts(read);
    // SAFETY: GetUserDefaultUILanguage takes no arguments and only returns a language id.
    let ui_language = unsafe { GetUserDefaultUILanguage() };
    SystemInfo {
        build: facts.build,
        arch: arch(),
        default_terminal: facts.default_terminal,
        webview2: facts.webview2,
        packaged: packaged(),
        ui_language,
        unreadable: facts.unreadable,
    }
}

impl Host for WinHost {
    fn local_app_data(&self) -> Result<PathBuf, AdapterError> {
        known_folder(&FOLDERID_LocalAppData, "LocalAppData")
    }

    fn program_data(&self) -> Result<PathBuf, AdapterError> {
        known_folder(&FOLDERID_ProgramData, "ProgramData")
    }

    fn user_sid(&self) -> Result<String, AdapterError> {
        current_user_sid()
    }

    fn short_path(&self, path: &Path) -> Result<PathBuf, AdapterError> {
        let long = wide(path.as_os_str());
        // SAFETY: `long` is NUL-terminated; a NULL buffer with size 0 asks for the size needed,
        // which includes the closing NUL.
        let need = unsafe { GetShortPathNameW(long.as_ptr(), std::ptr::null_mut(), 0) };
        if need == 0 {
            return Err(io_err(
                "GetShortPathNameW",
                path,
                &std::io::Error::last_os_error(),
            ));
        }
        let mut buf = vec![0u16; need as usize];
        // SAFETY: `long` is NUL-terminated, and `buf` holds `need` writable units, which is the
        // size passed.
        let got = unsafe { GetShortPathNameW(long.as_ptr(), buf.as_mut_ptr(), need) };
        if got == 0 {
            return Err(io_err(
                "GetShortPathNameW",
                path,
                &std::io::Error::last_os_error(),
            ));
        }
        if got >= need {
            // On success the length leaves out the NUL, so `need` or more means it did not fit.
            return Err(AdapterError::new(
                format!("GetShortPathNameW {}", path.display()),
                "the short name grew between two calls",
            ));
        }
        buf.truncate(got as usize);
        Ok(PathBuf::from(OsString::from_wide(&buf)))
    }

    fn private_dir_state(&self, dir: &Path) -> Result<DirState, AdapterError> {
        // The path is looked at twice: here, for what it is (a link, a file, a folder), and in
        // `owner_and_allowed` for who owns it and who may write to it. Another user could swap
        // the path between the two looks, and that is harmless: GetNamedSecurityInfoW reads a
        // link's own descriptor and does not follow it (the test
        // `a_link_is_read_as_itself_and_not_as_its_target` checks that for a junction). So
        // whatever is put in the folder's place, a link or a folder, is owned by the user who
        // made it, and `judge` calls it Unsafe.
        let meta = match std::fs::symlink_metadata(dir) {
            Ok(m) => m,
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(DirState::Absent),
            Err(e) => return Err(io_err("inspect", dir, &e)),
        };
        if meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Ok(DirState::Unsafe("it is a link to another place".into()));
        }
        if !meta.is_dir() {
            return Ok(DirState::Unsafe("it is not a folder".into()));
        }
        let me = current_user_sid()?;
        // The second look, of the path itself (see the note above).
        let (owner, allowed) = owner_and_allowed(dir)?;
        Ok(judge(&me, &owner, allowed.as_deref()))
    }

    fn create_private_dir(&self, dir: &Path) -> Result<Created, AdapterError> {
        let sid = current_user_sid()?;
        create_dir_with_sddl(
            dir,
            &format!("D:P(A;OICI;FA;;;{sid})(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)"),
        )
    }

    fn system_info(&self) -> SystemInfo {
        system_info_with(&|loc, name| loc.read_raw(name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::windows::registry::Hive;
    use crate::adapters::{RegValue, REG_SZ};
    use std::cell::RefCell;
    use std::os::windows::process::CommandExt;
    use windows_sys::Win32::Security::{
        GetSecurityDescriptorControl, CONTAINER_INHERIT_ACE, INHERITED_ACE, INHERIT_ONLY_ACE,
        OBJECT_INHERIT_ACE, SE_DACL_PROTECTED,
    };
    use windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;

    #[test]
    fn the_user_sid_and_known_folders_are_real() {
        let sid = WinHost.user_sid().unwrap();
        assert!(sid.starts_with("S-1-5-"), "{sid}");
        assert!(WinHost.local_app_data().unwrap().is_dir());
        assert!(WinHost.program_data().unwrap().is_dir());
    }

    #[test]
    fn short_path_names_the_same_folder() {
        // Review Focus 4: the VM-C user folder name.
        let tmp = tempfile::tempdir().unwrap();
        let long = tmp.path().join("ผู้ทดสอบ ไทย");
        std::fs::create_dir(&long).unwrap();
        let short = WinHost.short_path(&long).unwrap();
        assert_eq!(
            std::fs::canonicalize(&short).unwrap(),
            std::fs::canonicalize(&long).unwrap()
        );
        println!(
            "8.3 names on this volume: {}",
            short.to_str().is_some_and(str::is_ascii)
        );
        // The check above also passes for a short_path that hands its input back. The shell
        // makes the short form on its own: where it gets a real short name, the volume makes
        // 8.3 names and ours has to be that same name.
        let theirs = cmd_short_path(&long);
        if is_short_name(&theirs) {
            println!("short name: ours {short:?}, the shell's {theirs:?}");
            assert!(
                short
                    .to_str()
                    .is_some_and(|ours| ours.eq_ignore_ascii_case(&theirs)),
                "ours {short:?}, the shell's {theirs:?}"
            );
        } else {
            println!("the shell made no short name either: nothing to compare");
        }
        assert!(WinHost.short_path(&tmp.path().join("missing")).is_err());
    }

    #[test]
    fn the_shells_answer_is_trusted_only_when_it_is_a_real_short_name() {
        // What cmd printed here for a folder with a Thai name: a real short name.
        assert!(is_short_name(
            r"C:\Users\x\AppData\Local\Temp\TMP3UR~1\3607~1"
        ));
        // The same folder in its long form, and a path with a component that has no short name:
        // cmd could not print the Thai letters and printed '?' for each, which is still ASCII.
        assert!(!is_short_name(
            r"C:\Users\x\AppData\Local\Temp\.tmp3UrrCE\???????? ???"
        ));
        assert!(!is_short_name(
            r"C:\Users\x\AppData\Local\Temp\TMP3UR~1\????? ????????"
        ));
        // Nothing at all: a wildcard that matched nothing, or a cmd that printed nothing.
        assert!(!is_short_name(""));
        // Not ASCII: a console that did print the letters.
        assert!(!is_short_name(r"C:\Users\x\ผู้ทดสอบ ไทย"));
    }

    /// Whether the shell's answer is a short name that ours can be compared with: not empty, plain
    /// ASCII and without a '?'. Where cmd cannot print a letter (Thai, on an English console) it
    /// prints '?' for it, which is still ASCII, and '?' is never valid in a Windows file name, so
    /// one marks a name that was not made short (a volume or a folder without an 8.3 name).
    /// An empty answer is cmd printing nothing.
    fn is_short_name(answer: &str) -> bool {
        !answer.is_empty() && answer.is_ascii() && !answer.contains('?')
    }

    /// The short form of a path as cmd prints it (`%~sI` in a `for` loop): the shell's own answer.
    /// `/d` keeps AutoRun out; `raw_arg` hands cmd the text as written, quotes included.
    fn cmd_short_path(path: &Path) -> String {
        let out = std::process::Command::new("cmd")
            .raw_arg(format!(
                r#"/d /c for %I in ("{}") do @echo %~sI"#,
                path.display()
            ))
            .stdin(std::process::Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    #[test]
    fn a_private_folder_is_private_and_a_second_create_says_so() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("ai.resincore.cmdstudio-test");
        assert_eq!(WinHost.private_dir_state(&dir).unwrap(), DirState::Absent);
        assert_eq!(WinHost.create_private_dir(&dir).unwrap(), Created::New);
        assert_eq!(
            WinHost.create_private_dir(&dir).unwrap(),
            Created::AlreadyExisted
        );
        assert_eq!(WinHost.private_dir_state(&dir).unwrap(), DirState::Private);
        // Children inherit the protected list (OICI).
        let child = dir.join("clink-1.9.34");
        std::fs::create_dir(&child).unwrap();
        assert_eq!(
            WinHost.private_dir_state(&child).unwrap(),
            DirState::Private
        );
    }

    #[test]
    fn a_folder_others_can_write_is_unsafe() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("everyone");
        create_dir_with_sddl(&dir, "D:P(A;OICI;FA;;;WD)").unwrap();
        assert!(
            matches!(WinHost.private_dir_state(&dir).unwrap(), DirState::Unsafe(r) if r.contains("S-1-1-0"))
        );
        let file = tmp.path().join("file");
        std::fs::write(&file, b"x").unwrap();
        assert!(matches!(
            WinHost.private_dir_state(&file).unwrap(),
            DirState::Unsafe(_)
        ));
    }

    #[test]
    fn a_junction_is_unsafe() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("target");
        std::fs::create_dir(&target).unwrap();
        let link = tmp.path().join("link");
        let status = std::process::Command::new("cmd")
            .args(["/d", "/c", "mklink", "/J"])
            .arg(&link)
            .arg(&target)
            .stdout(std::process::Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .status()
            .unwrap();
        assert!(status.success(), "mklink /J failed: {status}");
        assert!(
            matches!(WinHost.private_dir_state(&link).unwrap(), DirState::Unsafe(r) if r.contains("link"))
        );
    }

    #[test]
    fn judge_follows_the_spec_rule() {
        let me = "S-1-5-21-1-2-3-1001";
        let full = 0x001F_01FF;
        let ok = vec![
            (me.to_string(), full),
            (LOCAL_SYSTEM.to_string(), full),
            (ADMINISTRATORS.to_string(), full),
        ];
        assert_eq!(judge(me, ADMINISTRATORS, Some(&ok)), DirState::Private);
        let mut users_read = ok.clone();
        users_read.push(("S-1-5-32-545".to_string(), 0x0012_00A9));
        assert_eq!(judge(me, me, Some(&users_read)), DirState::Private);
        let mut users_write = ok.clone();
        users_write.push(("S-1-5-32-545".to_string(), 0x0000_0006));
        assert!(matches!(
            judge(me, me, Some(&users_write)),
            DirState::Unsafe(_)
        ));
        assert!(matches!(
            judge(me, "S-1-5-21-9-9-9-1002", Some(&ok)),
            DirState::Unsafe(_)
        ));
        assert!(matches!(judge(me, me, None), DirState::Unsafe(_)));
    }

    #[test]
    fn system_info_reads_this_machine() {
        let info = WinHost.system_info();
        println!("system facts on this machine: {info:?}");
        assert!(info.build.is_some_and(|b| b >= 17763), "{info:?}");
        assert_eq!(info.arch, Arch::X64);
        assert!(!info.packaged, "a test runner is not an MSIX package");
        assert!(info.unreadable.is_empty(), "a real read failed: {info:?}");
    }

    // The tests below pin what the ones above cannot see, because a temp folder's parent
    // grants nothing to strangers and its access list is plain.

    #[test]
    fn a_private_folder_ignores_what_its_parent_grants() {
        // %ProgramData% lets BUILTIN\Users create folders, and a folder made there normally
        // inherits from it. The private folder must not pick up what its parent hands down.
        // (This pins the outcome. It does not isolate the `P` of the SDDL: on current Windows a
        // list given at creation is not merged with a parent's different entries anyway. The
        // flags test below is the one that catches a missing `P`.)
        let tmp = tempfile::tempdir().unwrap();
        let parent = tmp.path().join("open");
        create_dir_with_sddl(&parent, "D:P(A;OICI;FA;;;WD)").unwrap();
        // The control: an ordinary folder made in that parent does inherit Everyone.
        let plain = parent.join("plain");
        std::fs::create_dir(&plain).unwrap();
        assert!(
            matches!(WinHost.private_dir_state(&plain).unwrap(), DirState::Unsafe(r) if r.contains("S-1-1-0")),
            "the parent no longer hands Everyone down, so this test proves nothing"
        );
        let dir = parent.join("ai.resincore.cmdstudio-test");
        assert_eq!(WinHost.create_private_dir(&dir).unwrap(), Created::New);
        assert_eq!(WinHost.private_dir_state(&dir).unwrap(), DirState::Private);
        let child = dir.join("clink-1.9.34");
        std::fs::create_dir(&child).unwrap();
        assert_eq!(
            WinHost.private_dir_state(&child).unwrap(),
            DirState::Private
        );
    }

    #[test]
    fn the_access_list_is_read_entry_by_entry() {
        let tmp = tempfile::tempdir().unwrap();
        let me = WinHost.user_sid().unwrap();
        let ours = format!("(A;OICI;FA;;;{me})(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)");
        let state = |name: &str, sddl: &str| {
            let dir = tmp.path().join(name);
            assert_eq!(create_dir_with_sddl(&dir, sddl).unwrap(), Created::New);
            WinHost.private_dir_state(&dir).unwrap()
        };
        assert_eq!(
            state("ours", &format!("D:P{ours}")),
            DirState::Private,
            "the three entries of the real descriptor"
        );
        // Read and run for BUILTIN\Users (S-1-5-32-545): still private.
        assert_eq!(
            state("users-read", &format!("D:P{ours}(A;OICI;FRFX;;;BU)")),
            DirState::Private
        );
        // A deny entry only takes rights away, so it never makes a folder unsafe. It names
        // BUILTIN\Guests, not Users: the test user is in Users, and a folder that denies its own
        // user writing could not be cleaned up afterwards.
        assert_eq!(
            state("guests-denied", &format!("D:P(D;OICI;FW;;;BG){ours}")),
            DirState::Private
        );
        // Write for Users, as the third entry of the list: unsafe, and the reason names them.
        assert!(matches!(
            state("users-write", &format!("D:P{ours}(A;OICI;FW;;;BU)")),
            DirState::Unsafe(r) if r.contains("S-1-5-32-545")
        ));
        // Someone else's entry before ours is read as well.
        assert!(matches!(
            state("users-write-first", &format!("D:P(A;OICI;FW;;;BU){ours}")),
            DirState::Unsafe(r) if r.contains("S-1-5-32-545")
        ));
        // An entry of a kind this check cannot read (a conditional one here) counts as a writer.
        assert!(matches!(
            state("conditional", &format!("D:P{ours}(XA;OICI;FA;;;WD;(@User.Title==\"PM\"))")),
            DirState::Unsafe(r) if r.contains("<ACE type 9>")
        ));
        // No access list at all means everyone has full access.
        assert!(matches!(
            state("no-list", "D:NO_ACCESS_CONTROL"),
            DirState::Unsafe(r) if r.contains("no access list")
        ));
    }

    #[test]
    fn a_folder_that_cannot_be_made_is_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        // The folder above is missing: not created on the way, and not "already existed" either.
        let missing = tmp.path().join("no").join("parent");
        let err = WinHost.create_private_dir(&missing).unwrap_err();
        assert!(err.context.contains("CreateDirectoryW"), "{err}");
        assert!(!tmp.path().join("no").exists());
        // A descriptor Windows cannot read is refused before any folder is made.
        let bad = tmp.path().join("bad");
        let err = create_dir_with_sddl(&bad, "this is not sddl").unwrap_err();
        assert!(
            err.context
                .contains("ConvertStringSecurityDescriptorToSecurityDescriptorW"),
            "{err}"
        );
        assert!(!bad.exists());
        // A file in the way answers "already existed" as well, so the caller must check the
        // state again (it says: not a folder).
        let file = tmp.path().join("file");
        std::fs::write(&file, b"x").unwrap();
        assert_eq!(
            WinHost.create_private_dir(&file).unwrap(),
            Created::AlreadyExisted
        );
        assert!(matches!(
            WinHost.private_dir_state(&file).unwrap(),
            DirState::Unsafe(r) if r.contains("not a folder")
        ));
    }

    #[test]
    fn the_default_terminal_is_named_by_its_guid() {
        let named = |guid: &str| terminal_from(Some(guid));
        assert_eq!(terminal_from(None), DefaultTerminal::LetWindowsDecide);
        assert_eq!(
            named("{00000000-0000-0000-0000-000000000000}"),
            DefaultTerminal::LetWindowsDecide
        );
        assert_eq!(
            named("{2EACA947-7F5F-4CFA-BA87-8F7FBEEFBE69}"),
            DefaultTerminal::WindowsTerminal
        );
        assert_eq!(
            named("{06EC847C-C0A5-46B8-92CB-7C92F6E35CD5}"),
            DefaultTerminal::WindowsTerminalPreview
        );
        assert_eq!(
            named("{B23D10C0-E52E-411E-9D5B-C09FDF709C7D}"),
            DefaultTerminal::Conhost
        );
        // A value typed by hand may be in lower case.
        assert_eq!(
            named("{2eaca947-7f5f-4cfa-ba87-8f7fbeefbe69}"),
            DefaultTerminal::WindowsTerminal
        );
        // Any other terminal application.
        assert_eq!(
            named("{11111111-2222-3333-4444-555555555555}"),
            DefaultTerminal::Other
        );
    }

    /// The flag byte of every entry of a path's access list, in order.
    fn ace_flags(path: &Path) -> Vec<u32> {
        let name = wide(path.as_os_str());
        let mut dacl: *mut ACL = std::ptr::null_mut();
        let mut sd: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
        // SAFETY: `name` is NUL-terminated and outlives the call; the DACL and descriptor
        // out-pointers are valid locals, and the other out-pointers are NULL because only the
        // access list is asked for.
        let rc = unsafe {
            GetNamedSecurityInfoW(
                name.as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut dacl,
                std::ptr::null_mut(),
                &mut sd,
            )
        };
        assert_eq!(rc, 0, "{}", path.display());
        // SAFETY: GetNamedSecurityInfoW allocated the descriptor with LocalAlloc, and nothing
        // else frees it.
        let _descriptor = unsafe { LocalBlock::new(sd) };
        assert!(!dacl.is_null(), "{} has no access list", path.display());
        // SAFETY: `dacl` is not NULL (just asserted), so it is the ACL inside the descriptor,
        // which the guard keeps alive.
        let count = unsafe { (*dacl).AceCount };
        (0..u32::from(count))
            .map(|index| {
                let mut ace: *mut c_void = std::ptr::null_mut();
                // SAFETY: `index` is below the entry count; `ace` receives that entry's address.
                let found = unsafe { GetAce(dacl, index, &mut ace) };
                assert_ne!(found, 0);
                // SAFETY: every ACE starts with an ACE_HEADER, and `ace` points at one.
                u32::from(unsafe { (*ace.cast::<ACE_HEADER>()).AceFlags })
            })
            .collect()
    }

    /// Whether the path's access list has the "protected from inheritance" flag.
    fn dacl_is_protected(path: &Path) -> bool {
        let name = wide(path.as_os_str());
        let mut dacl: *mut ACL = std::ptr::null_mut();
        let mut sd: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
        // SAFETY: as in `ace_flags`: `name` is NUL-terminated, the DACL and descriptor
        // out-pointers are valid locals, and the other out-pointers are NULL.
        let rc = unsafe {
            GetNamedSecurityInfoW(
                name.as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut dacl,
                std::ptr::null_mut(),
                &mut sd,
            )
        };
        assert_eq!(rc, 0, "{}", path.display());
        // SAFETY: GetNamedSecurityInfoW allocated the descriptor with LocalAlloc, and nothing
        // else frees it.
        let _descriptor = unsafe { LocalBlock::new(sd) };
        let (mut control, mut revision) = (0u16, 0u32);
        // SAFETY: `sd` is the descriptor from the call above, kept alive by the guard;
        // `control` and `revision` are valid out-pointers.
        let got = unsafe { GetSecurityDescriptorControl(sd, &mut control, &mut revision) };
        assert_ne!(got, 0);
        control & SE_DACL_PROTECTED != 0
    }

    #[test]
    fn the_private_folder_and_what_is_made_in_it_hold_exactly_the_three_entries() {
        // Reads the entries back. "Private" alone is not enough: a fresh folder in a user's
        // own temp folder, and a child that inherited nothing and got the process's default
        // list, can hold the same user, SYSTEM and Administrators (an administrator's token
        // does), so only the entries' flags tell inherited from default.
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("ai.resincore.cmdstudio-test");
        assert_eq!(WinHost.create_private_dir(&dir).unwrap(), Created::New);
        let child_folder = dir.join("clink-1.9.34");
        std::fs::create_dir(&child_folder).unwrap();
        let child_file = dir.join("clink.exe");
        std::fs::write(&child_file, b"x").unwrap();
        let mut want = vec![
            WinHost.user_sid().unwrap(),
            LOCAL_SYSTEM.to_string(),
            ADMINISTRATORS.to_string(),
        ];
        want.sort();
        let full = windows_sys::Win32::Storage::FileSystem::FILE_ALL_ACCESS;
        for path in [&dir, &child_folder, &child_file] {
            let (_, allowed) = owner_and_allowed(path).unwrap();
            let allowed = allowed.expect("an access list, not a NULL one");
            assert!(
                allowed.iter().all(|(_, mask)| mask & full == full),
                "{}: {allowed:?}",
                path.display()
            );
            let mut sids: Vec<String> = allowed.into_iter().map(|(sid, _)| sid).collect();
            sids.sort();
            assert_eq!(sids, want, "{}", path.display());
        }
        // The folder's own entries are explicit (not inherited), they pass down to the folders
        // (CI) and files (OI) made inside, and the list is protected from its parent ...
        let pass_down = OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE;
        let own = ace_flags(&dir);
        assert_eq!(own.len(), 3, "{own:?}");
        assert!(
            own.iter()
                .all(|f| f & pass_down == pass_down && f & (INHERIT_ONLY_ACE | INHERITED_ACE) == 0),
            "{own:?}"
        );
        assert!(dacl_is_protected(&dir));
        // ... and what is made inside holds those entries as inherited ones.
        for path in [&child_folder, &child_file] {
            let flags = ace_flags(path);
            assert_eq!(flags.len(), 3, "{}: {flags:?}", path.display());
            assert!(
                flags.iter().all(|f| f & INHERITED_ACE != 0),
                "{}: {flags:?}",
                path.display()
            );
        }
    }

    #[test]
    fn the_known_folders_are_the_two_windows_names() {
        // `is_dir` alone would also pass for any other folder: the session's environment is set
        // from the same two known folders when the user signs in, so it names them too.
        let both = [
            (WinHost.local_app_data().unwrap(), "LOCALAPPDATA"),
            (WinHost.program_data().unwrap(), "ProgramData"),
        ];
        for (folder, var) in both {
            let named = std::env::var_os(var).unwrap();
            assert_eq!(
                std::fs::canonicalize(&folder).unwrap(),
                std::fs::canonicalize(&named).unwrap(),
                "{var}"
            );
        }
    }

    #[test]
    fn webview2_zero_means_not_installed() {
        assert!(is_installed_version("154.0.4258.62"));
        assert!(!is_installed_version("0.0.0.0"));
        assert!(!is_installed_version(""));
    }

    #[test]
    fn the_native_machine_decides_the_arch() {
        assert_eq!(arch_of(0x8664), Arch::X64);
        assert_eq!(arch_of(0xAA64), Arch::Arm64);
        // 32-bit x86, ARM, and "unknown".
        assert_eq!(arch_of(0x014C), Arch::Other);
        assert_eq!(arch_of(0x01C4), Arch::Other);
        assert_eq!(arch_of(0), Arch::Other);
    }

    #[test]
    fn the_write_rights_are_the_sdk_ones() {
        use windows_sys::Win32::Foundation::{
            GENERIC_ALL, GENERIC_EXECUTE, GENERIC_READ, GENERIC_WRITE,
        };
        use windows_sys::Win32::Storage::FileSystem::{
            DELETE, FILE_ALL_ACCESS, FILE_APPEND_DATA, FILE_DELETE_CHILD, FILE_EXECUTE,
            FILE_GENERIC_EXECUTE, FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_READ_ATTRIBUTES,
            FILE_READ_DATA, FILE_READ_EA, FILE_WRITE_ATTRIBUTES, FILE_WRITE_DATA, FILE_WRITE_EA,
            READ_CONTROL, SYNCHRONIZE, WRITE_DAC, WRITE_OWNER,
        };
        let writes = [
            FILE_WRITE_DATA,
            FILE_APPEND_DATA,
            FILE_WRITE_EA,
            FILE_DELETE_CHILD,
            FILE_WRITE_ATTRIBUTES,
            DELETE,
            WRITE_DAC,
            WRITE_OWNER,
            GENERIC_ALL,
            GENERIC_WRITE,
        ];
        // The hex list in the code is exactly the SDK's rights for changing a folder.
        assert_eq!(
            WRITE_RIGHTS,
            writes.iter().fold(0, |all, right| all | right)
        );
        // One stranger with only this right: is the folder still private?
        let me = "S-1-5-21-1-2-3-1001";
        let stranger = |mask: u32| judge(me, me, Some(&[("S-1-5-32-545".to_string(), mask)]));
        // Each one alone is enough, and so are the SDK's combined forms ...
        for mask in writes
            .into_iter()
            .chain([FILE_GENERIC_WRITE, FILE_ALL_ACCESS])
        {
            assert!(
                matches!(stranger(mask), DirState::Unsafe(_)),
                "{mask:#x} lets a stranger change the folder"
            );
        }
        // ... while reading and running never are.
        let reads = [
            FILE_READ_DATA,
            FILE_READ_EA,
            FILE_EXECUTE,
            FILE_READ_ATTRIBUTES,
            READ_CONTROL,
            SYNCHRONIZE,
            FILE_GENERIC_READ,
            FILE_GENERIC_EXECUTE,
            GENERIC_READ,
            GENERIC_EXECUTE,
        ];
        for mask in reads {
            assert_eq!(
                stranger(mask),
                DirState::Private,
                "{mask:#x} is only reading"
            );
        }
    }

    #[test]
    fn the_windows_folder_is_not_private() {
        // A real folder with another owner (TrustedInstaller) and a real access list, read only:
        // the owner and the entries come through the real calls, long SIDs included.
        let windows = PathBuf::from(std::env::var("SystemRoot").unwrap());
        let state = WinHost.private_dir_state(&windows).unwrap();
        println!("{} is {state:?}", windows.display());
        assert!(matches!(state, DirState::Unsafe(_)), "{state:?}");
    }

    /// `mklink /J`: a junction, the kind of link that needs no privilege.
    fn make_junction(link: &Path, target: &Path) {
        let status = std::process::Command::new("cmd")
            .args(["/d", "/c", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .stdout(std::process::Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .status()
            .unwrap();
        assert!(status.success(), "mklink /J failed: {status}");
    }

    #[test]
    fn a_link_is_read_as_itself_and_not_as_its_target() {
        // private_dir_state looks at a path twice, with a gap between the looks: is it a link,
        // then who owns it and who may write to it. The gap is harmless because the second look
        // does not follow a link, so a link put in a folder's place is judged by its own owner
        // and list. (A directory symlink needs a privilege this test user lacks, so only the
        // junction is tried.)
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("target");
        create_dir_with_sddl(&target, "D:P(A;OICI;FA;;;WD)").unwrap(); // Everyone, nobody else
        let link = tmp.path().join("link");
        make_junction(&link, &target);
        let sids = |dir: &Path| -> Vec<String> {
            let (_, allowed) = owner_and_allowed(dir).unwrap();
            allowed.unwrap().into_iter().map(|(sid, _)| sid).collect()
        };
        assert_eq!(sids(&target), ["S-1-1-0"], "the target is Everyone's alone");
        let of_link = sids(&link);
        assert!(
            !of_link.iter().any(|sid| sid == "S-1-1-0"),
            "the link was read as its target: {of_link:?}"
        );
        // Its owner is whoever made it: the test user, or Administrators when elevated.
        let (owner, _) = owner_and_allowed(&link).unwrap();
        let me = WinHost.user_sid().unwrap();
        assert!(owner == me || owner == ADMINISTRATORS, "{owner}");
    }

    #[test]
    fn a_missing_sid_is_an_error_before_anything_reads_it() {
        // A security descriptor may have no owner, and sid_string must only be given a SID.
        let err = non_null_sid(std::ptr::null_mut(), r"GetNamedSecurityInfoW C:\x").unwrap_err();
        assert_eq!(err.context, r"GetNamedSecurityInfoW C:\x");
        assert!(err.message.contains("no SID"), "{err}");
        // A pointer that is there passes through as it is; nothing here reads it.
        let some = std::ptr::dangling_mut::<c_void>();
        assert_eq!(non_null_sid(some, "x").unwrap(), some);
    }

    // Registry facts: a read that fails must not turn into an answer. These tests hand
    // `registry_facts` their own reader, so they touch no registry.

    type Reply = Result<Option<RegValue>, AdapterError>;

    fn text(s: &str) -> Reply {
        Ok(Some(RegValue::from_text(REG_SZ, s)))
    }

    fn denied(loc: &RegLocation, name: &str) -> AdapterError {
        AdapterError::new(
            format!("open {:?}\\{} ({name})", loc.hive, loc.subkey),
            "Access is denied. (os error 5)",
        )
    }

    #[test]
    fn facts_that_cannot_be_read_are_listed_and_never_answered() {
        let failing = |loc: &RegLocation, name: &str| -> Reply { Err(denied(loc, name)) };
        let facts = registry_facts(&failing);
        // The fields hold their "absent" values ...
        assert_eq!(facts.build, None);
        assert_eq!(facts.default_terminal, DefaultTerminal::LetWindowsDecide);
        assert_eq!(facts.webview2, None);
        // ... and each fact says why, once, in this order.
        let names = ["build", "default terminal", "WebView2"];
        assert_eq!(
            facts.unreadable.len(),
            names.len(),
            "{:?}",
            facts.unreadable
        );
        for (entry, fact) in facts.unreadable.iter().zip(names) {
            assert!(entry.starts_with(&format!("{fact}: ")), "{entry}");
            assert!(entry.contains("Access is denied"), "{entry}");
        }
        // WebView2 is read in two places, and its one entry names both.
        let webview2 = facts.unreadable.last().unwrap();
        assert!(
            webview2.contains("LocalMachine") && webview2.contains("CurrentUser"),
            "{webview2}"
        );
    }

    #[test]
    fn a_missing_value_is_absent_not_unreadable() {
        let absent = |_: &RegLocation, _: &str| -> Reply { Ok(None) };
        assert_eq!(
            registry_facts(&absent),
            RegistryFacts {
                build: None,
                default_terminal: DefaultTerminal::LetWindowsDecide,
                webview2: None,
                unreadable: Vec::new(),
            }
        );
    }

    #[test]
    fn webview2_from_either_key_counts_and_a_failed_key_is_listed_only_without_an_answer() {
        const VERSION: &str = "154.0.4258.62";
        // What the machine key (HKLM) and the user key (HKCU) answer; every other value is absent.
        let facts = |machine: Reply, user: Reply| {
            let reader = move |loc: &RegLocation, name: &str| -> Reply {
                match (loc.hive, name) {
                    (Hive::LocalMachine, "pv") => machine.clone(),
                    (Hive::CurrentUser, "pv") => user.clone(),
                    _ => Ok(None),
                }
            };
            registry_facts(&reader)
        };
        let fails = |hive: Hive| -> Reply {
            let loc = RegLocation {
                hive,
                subkey: "the key".to_string(),
            };
            Err(denied(&loc, "pv"))
        };
        // The user key answers although the machine key cannot be read: the answer counts.
        let f = facts(fails(Hive::LocalMachine), text(VERSION));
        assert_eq!(f.webview2.as_deref(), Some(VERSION));
        assert!(f.unreadable.is_empty(), "{:?}", f.unreadable);
        // The machine key answers: the user key is not needed, whatever it would say.
        let f = facts(text(VERSION), fails(Hive::CurrentUser));
        assert_eq!(f.webview2.as_deref(), Some(VERSION));
        assert!(f.unreadable.is_empty(), "{:?}", f.unreadable);
        // No version from the keys that could be read, and one that could not: "not installed"
        // is not known, so it is listed, and only the key that failed is named.
        let f = facts(fails(Hive::LocalMachine), Ok(None));
        assert_eq!(f.webview2, None);
        assert_eq!(f.unreadable.len(), 1, "{:?}", f.unreadable);
        let entry = f.unreadable.first().unwrap();
        assert!(entry.starts_with("WebView2: "), "{entry}");
        assert!(
            entry.contains("LocalMachine") && !entry.contains("CurrentUser"),
            "{entry}"
        );
        let f = facts(text("0.0.0.0"), fails(Hive::CurrentUser));
        assert_eq!(f.webview2, None);
        let entry = f.unreadable.first().unwrap();
        assert!(
            entry.contains("CurrentUser") && !entry.contains("LocalMachine"),
            "{entry}"
        );
        // Both keys readable, and neither holds a runtime: absent, with nothing to report.
        for (machine, user) in [(Ok(None), Ok(None)), (text("0.0.0.0"), text(""))] {
            let f = facts(machine, user);
            assert_eq!(f.webview2, None);
            assert!(f.unreadable.is_empty(), "{:?}", f.unreadable);
        }
    }

    #[test]
    fn the_registry_facts_read_exactly_these_places() {
        let seen = RefCell::new(Vec::new());
        let recording = |loc: &RegLocation, name: &str| -> Reply {
            seen.borrow_mut().push((loc.clone(), name.to_string()));
            Ok(None)
        };
        registry_facts(&recording);
        let place = |loc: RegLocation, name: &str| (loc, name.to_string());
        assert_eq!(
            seen.into_inner(),
            vec![
                place(
                    RegLocation::machine(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion"),
                    "CurrentBuildNumber"
                ),
                place(RegLocation::user(r"Console\%%Startup"), "DelegationConsole"),
                place(
                    RegLocation::machine(
                        r"SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"
                    ),
                    "pv"
                ),
                place(
                    RegLocation::user(
                        r"Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"
                    ),
                    "pv"
                ),
            ]
        );
    }

    #[test]
    fn the_values_become_the_facts() {
        let answers = |loc: &RegLocation, name: &str| -> Reply {
            match (loc.hive, name) {
                (Hive::LocalMachine, "CurrentBuildNumber") => text(" 26300 "),
                (Hive::CurrentUser, "DelegationConsole") => {
                    text("{2eaca947-7f5f-4cfa-ba87-8f7fbeefbe69}")
                }
                (Hive::CurrentUser, "pv") => text("154.0.4258.62"),
                _ => Ok(None),
            }
        };
        assert_eq!(
            registry_facts(&answers),
            RegistryFacts {
                build: Some(26300),
                default_terminal: DefaultTerminal::WindowsTerminal,
                webview2: Some("154.0.4258.62".to_string()),
                unreadable: Vec::new(),
            }
        );
    }

    #[test]
    fn a_value_that_is_not_what_it_should_be_is_unreadable_too() {
        let odd = |_: &RegLocation, name: &str| -> Reply {
            match name {
                "CurrentBuildNumber" => text("twenty-six"),
                // A REG_DWORD where text belongs.
                "DelegationConsole" => Ok(Some(RegValue {
                    kind: 4,
                    bytes: vec![1, 0, 0, 0],
                })),
                _ => Ok(None),
            }
        };
        let facts = registry_facts(&odd);
        assert_eq!(facts.build, None);
        assert_eq!(facts.default_terminal, DefaultTerminal::LetWindowsDecide);
        assert_eq!(facts.webview2, None);
        assert_eq!(facts.unreadable.len(), 2, "{:?}", facts.unreadable);
        let (build, terminal) = (&facts.unreadable[0], &facts.unreadable[1]);
        assert!(
            build.starts_with("build: ") && build.contains("not a number"),
            "{build}"
        );
        assert!(
            terminal.starts_with("default terminal: ") && terminal.contains("not text"),
            "{terminal}"
        );
    }

    // The last hop: from the registry facts into `SystemInfo`. The one test that calls the real
    // `system_info` runs on a healthy machine, where nothing is unreadable, so these two drive
    // the whole of it through a reader of their own.

    #[test]
    fn system_info_carries_what_could_not_be_read() {
        let failing = |loc: &RegLocation, name: &str| -> Reply { Err(denied(loc, name)) };
        let info = system_info_with(&failing);
        assert_eq!(info.build, None);
        assert_eq!(info.default_terminal, DefaultTerminal::LetWindowsDecide);
        assert_eq!(info.webview2, None);
        let names = ["build", "default terminal", "WebView2"];
        assert_eq!(info.unreadable.len(), names.len(), "{:?}", info.unreadable);
        for (entry, fact) in info.unreadable.iter().zip(names) {
            assert!(entry.starts_with(&format!("{fact}: ")), "{entry}");
        }
        // What does not come from the registry is still read from the machine.
        assert_ne!(info.ui_language, 0);
    }

    #[test]
    fn system_info_carries_what_was_read() {
        let answers = |loc: &RegLocation, name: &str| -> Reply {
            match (loc.hive, name) {
                (Hive::LocalMachine, "CurrentBuildNumber") => text("26300"),
                (Hive::CurrentUser, "DelegationConsole") => {
                    text("{06EC847C-C0A5-46B8-92CB-7C92F6E35CD5}")
                }
                (Hive::LocalMachine, "pv") => text("154.0.4258.62"),
                _ => Ok(None),
            }
        };
        let info = system_info_with(&answers);
        assert_eq!(info.build, Some(26300));
        assert_eq!(
            info.default_terminal,
            DefaultTerminal::WindowsTerminalPreview
        );
        assert_eq!(info.webview2.as_deref(), Some("154.0.4258.62"));
        assert!(info.unreadable.is_empty(), "{:?}", info.unreadable);
        assert_ne!(info.ui_language, 0);
    }
}
