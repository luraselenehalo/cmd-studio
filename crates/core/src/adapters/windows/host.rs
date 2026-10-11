//! Identity, known folders, 8.3 names, the private ProgramData folder
//! (spec §5.4 `<clink-launch>` step 3) and system facts.
use super::registry::RegLocation;
use super::{io_err, last_error, wide};
use crate::adapters::{AdapterError, Arch, Created, DefaultTerminal, DirState, Host, SystemInfo};
use std::ffi::{c_void, OsStr, OsString};
use std::io::ErrorKind;
use std::os::windows::ffi::OsStringExt;
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use windows_sys::core::{GUID, PWSTR};
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, LocalFree, APPMODEL_ERROR_NO_PACKAGE, ERROR_ALREADY_EXISTS, HANDLE,
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

const CONSOLE_STARTUP: &str = r"Console\%%Startup";
const WEBVIEW2_CLIENT: &str = "{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";

/// The real machine: the current user, the known folders and the system facts.
#[derive(Clone, Copy, Debug, Default)]
pub struct WinHost;

/// A block that `LocalAlloc` handed out (a security descriptor, a SID string).
/// `LocalFree` runs when the guard goes out of scope, on every path out of a function.
struct LocalBlock(*mut c_void);

impl Drop for LocalBlock {
    fn drop(&mut self) {
        // SAFETY: the pointer came from a Win32 call that allocates with LocalAlloc, this guard
        // is its only owner, and this is the only place that frees it.
        unsafe { LocalFree(self.0) };
    }
}

/// The current process's token, closed when the guard goes out of scope.
struct Token(HANDLE);

impl Drop for Token {
    fn drop(&mut self) {
        // SAFETY: the handle came from a successful OpenProcessToken, this guard is its only
        // owner, and this is the only place that closes it.
        unsafe { CloseHandle(self.0) };
    }
}

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

/// # Safety
/// `sid` must point to a valid SID.
unsafe fn sid_string(sid: PSID) -> Result<String, AdapterError> {
    let mut s: PWSTR = std::ptr::null_mut();
    // SAFETY: the caller promises `sid` is a valid SID; `s` is a valid out-pointer.
    let converted = unsafe { ConvertSidToStringSidW(sid, &mut s) };
    if converted == 0 {
        return Err(last_error("ConvertSidToStringSidW"));
    }
    // The string was allocated with LocalAlloc; the guard frees it after the text is copied out.
    let s = LocalBlock(s.cast::<c_void>());
    // SAFETY: success gives a NUL-terminated UTF-16 string, valid until the guard drops.
    let text = unsafe { from_wide_ptr(s.0.cast::<u16>()) };
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
    let token = Token(handle);
    let mut len = 0u32;
    // SAFETY: a size query: no buffer with length 0 is allowed, and `len` receives the size needed.
    unsafe { GetTokenInformation(token.0, TokenUser, std::ptr::null_mut(), 0, &mut len) };
    if len == 0 {
        return Err(last_error("GetTokenInformation (size)"));
    }
    // Whole u64 words: at least `len` bytes, and aligned for TOKEN_USER's pointer.
    let mut buf = vec![0u64; (len as usize).div_ceil(8)];
    // SAFETY: `buf` holds at least `len` writable bytes, which is the length passed.
    let got = unsafe {
        GetTokenInformation(
            token.0,
            TokenUser,
            buf.as_mut_ptr().cast::<c_void>(),
            len,
            &mut len,
        )
    };
    if got == 0 {
        return Err(last_error("GetTokenInformation"));
    }
    // SAFETY: success wrote a TOKEN_USER at the start of `buf`, which is 8-byte aligned; its SID
    // points into `buf` as well, and `buf` outlives the call.
    unsafe { sid_string((*buf.as_ptr().cast::<TOKEN_USER>()).User.Sid) }
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
    // `owner` and `dacl` point into this descriptor, so it stays until the function returns.
    let _descriptor = LocalBlock(sd);
    // SAFETY: `owner` is the SID inside the descriptor, which the guard keeps alive.
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
    // Allocated with LocalAlloc by the call above; freed on every path out of here.
    let descriptor = LocalBlock(sd);
    let attributes = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor.0,
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

fn read_text(loc: &RegLocation, name: &str) -> Option<String> {
    loc.read_raw(name).ok().flatten().and_then(|v| v.text())
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

fn default_terminal() -> DefaultTerminal {
    let value = read_text(&RegLocation::user(CONSOLE_STARTUP), "DelegationConsole");
    terminal_from(value.as_deref())
}

/// Edge Update writes `0.0.0.0` as the version when the runtime is not installed.
fn is_installed_version(pv: &str) -> bool {
    !pv.is_empty() && pv != "0.0.0.0"
}

fn webview2() -> Option<String> {
    let keys = [
        RegLocation::machine(&format!(
            r"SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{WEBVIEW2_CLIENT}"
        )),
        RegLocation::user(&format!(
            r"Software\Microsoft\EdgeUpdate\Clients\{WEBVIEW2_CLIENT}"
        )),
    ];
    keys.iter()
        .filter_map(|k| read_text(k, "pv"))
        .find(|v| is_installed_version(v))
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
        let build = read_text(
            &RegLocation::machine(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion"),
            "CurrentBuildNumber",
        )
        .and_then(|b| b.trim().parse().ok());
        // SAFETY: GetUserDefaultUILanguage takes no arguments and only returns a language id.
        let ui_language = unsafe { GetUserDefaultUILanguage() };
        SystemInfo {
            build,
            arch: arch(),
            default_terminal: default_terminal(),
            webview2: webview2(),
            packaged: packaged(),
            ui_language,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        assert!(WinHost.short_path(&tmp.path().join("missing")).is_err());
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
        let _descriptor = LocalBlock(sd);
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
        let _descriptor = LocalBlock(sd);
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
}
