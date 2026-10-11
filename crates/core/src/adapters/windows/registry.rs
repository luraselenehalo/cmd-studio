//! cmd's AutoRun value (spec §5.4 AutoRun). Values are read and written raw,
//! so the type (`REG_SZ` or `REG_EXPAND_SZ`) and every byte survive a
//! restore. Keys are opened, never recreated (spec §2.2 lesson 9).
use crate::adapters::{AdapterError, AutoRun, RegValue};
use std::borrow::Cow;
use std::io::ErrorKind;
use winreg::enums::{
    RegType, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_SET_VALUE, REG_BINARY, REG_DWORD,
    REG_DWORD_BIG_ENDIAN, REG_EXPAND_SZ, REG_FULL_RESOURCE_DESCRIPTOR, REG_LINK, REG_MULTI_SZ,
    REG_NONE, REG_QWORD, REG_RESOURCE_LIST, REG_RESOURCE_REQUIREMENTS_LIST, REG_SZ,
};
use winreg::RegKey;

pub const COMMAND_PROCESSOR: &str = r"Software\Microsoft\Command Processor";
pub const AUTORUN: &str = "AutoRun";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hive {
    CurrentUser,
    LocalMachine,
}

/// A registry key: hive plus subkey path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegLocation {
    pub hive: Hive,
    pub subkey: String,
}

impl RegLocation {
    pub fn user(subkey: &str) -> Self {
        RegLocation {
            hive: Hive::CurrentUser,
            subkey: subkey.to_string(),
        }
    }
    pub fn machine(subkey: &str) -> Self {
        RegLocation {
            hive: Hive::LocalMachine,
            subkey: subkey.to_string(),
        }
    }

    fn root(&self) -> RegKey {
        RegKey::predef(match self.hive {
            Hive::CurrentUser => HKEY_CURRENT_USER,
            Hive::LocalMachine => HKEY_LOCAL_MACHINE,
        })
    }

    fn err(&self, what: &str, e: &std::io::Error) -> AdapterError {
        AdapterError::new(
            format!("{what} {:?}\\{}", self.hive, self.subkey),
            e.to_string(),
        )
    }

    /// The existing key, or `None` when it does not exist.
    pub(crate) fn open(&self, write: bool) -> Result<Option<RegKey>, AdapterError> {
        let access = if write {
            KEY_READ | KEY_SET_VALUE
        } else {
            KEY_READ
        };
        match self.root().open_subkey_with_flags(&self.subkey, access) {
            Ok(key) => Ok(Some(key)),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
            Err(e) => Err(self.err("open", &e)),
        }
    }

    /// The key for writing. An existing key is opened as it is; a missing one
    /// is created (`RegCreateKeyExW`, which never clears an existing key).
    pub(crate) fn open_or_create(&self) -> Result<RegKey, AdapterError> {
        if let Some(key) = self.open(true)? {
            return Ok(key);
        }
        self.root()
            .create_subkey_with_flags(&self.subkey, KEY_READ | KEY_SET_VALUE)
            .map(|(key, _)| key)
            .map_err(|e| self.err("create", &e))
    }

    pub(crate) fn exists(&self) -> Result<bool, AdapterError> {
        Ok(self.open(false)?.is_some())
    }

    /// Deletes the key only when it holds no values and no subkeys; `Ok` when it is absent.
    pub(crate) fn delete_if_empty(&self) -> Result<(), AdapterError> {
        let Some(key) = self.open(false)? else {
            return Ok(());
        };
        let info = key.query_info().map_err(|e| self.err("query", &e))?;
        drop(key);
        if info.values != 0 || info.sub_keys != 0 {
            return Ok(());
        }
        match self.root().delete_subkey(&self.subkey) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
            Err(e) => Err(self.err("delete", &e)),
        }
    }

    pub(crate) fn read_raw(&self, name: &str) -> Result<Option<RegValue>, AdapterError> {
        let Some(key) = self.open(false)? else {
            return Ok(None);
        };
        match key.get_raw_value(name) {
            Ok(v) => Ok(Some(RegValue {
                kind: v.vtype.clone() as u32,
                bytes: v.bytes.to_vec(),
            })),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
            Err(e) => Err(self.err("read", &e)),
        }
    }

    pub(crate) fn write_raw(&self, name: &str, value: &RegValue) -> Result<(), AdapterError> {
        let vtype = reg_type(value.kind).ok_or_else(|| {
            AdapterError::new(
                format!("write {name}"),
                format!("unknown registry type {}", value.kind),
            )
        })?;
        let raw = winreg::RegValue {
            bytes: Cow::Borrowed(value.bytes.as_slice()),
            vtype,
        };
        self.open_or_create()?
            .set_raw_value(name, &raw)
            .map_err(|e| self.err("write", &e))
    }

    pub(crate) fn delete_value(&self, name: &str) -> Result<(), AdapterError> {
        let Some(key) = self.open(true)? else {
            return Ok(());
        };
        match key.delete_value(name) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
            Err(e) => Err(self.err("delete value in", &e)),
        }
    }
}

fn reg_type(kind: u32) -> Option<RegType> {
    Some(match kind {
        0 => REG_NONE,
        1 => REG_SZ,
        2 => REG_EXPAND_SZ,
        3 => REG_BINARY,
        4 => REG_DWORD,
        5 => REG_DWORD_BIG_ENDIAN,
        6 => REG_LINK,
        7 => REG_MULTI_SZ,
        8 => REG_RESOURCE_LIST,
        9 => REG_FULL_RESOURCE_DESCRIPTOR,
        10 => REG_RESOURCE_REQUIREMENTS_LIST,
        11 => REG_QWORD,
        _ => return None,
    })
}

/// AutoRun under two `Command Processor` keys: the user's (read and write)
/// and the machine's (read only).
#[derive(Clone, Debug)]
pub struct WinAutoRun {
    user: RegLocation,
    machine: RegLocation,
}

impl WinAutoRun {
    /// `HKCU\Software\Microsoft\Command Processor` and its HKLM twin.
    pub fn real() -> Self {
        WinAutoRun {
            user: RegLocation::user(COMMAND_PROCESSOR),
            machine: RegLocation::machine(COMMAND_PROCESSOR),
        }
    }

    /// Other keys, for tests.
    pub fn at(user: RegLocation, machine: RegLocation) -> Self {
        WinAutoRun { user, machine }
    }
}

impl AutoRun for WinAutoRun {
    fn user_key_exists(&self) -> Result<bool, AdapterError> {
        self.user.exists()
    }

    fn read_user(&self) -> Result<Option<RegValue>, AdapterError> {
        self.user.read_raw(AUTORUN)
    }

    fn write_user(&self, value: &RegValue) -> Result<(), AdapterError> {
        self.user.write_raw(AUTORUN, value)
    }

    fn delete_user(&self) -> Result<(), AdapterError> {
        self.user.delete_value(AUTORUN)
    }

    fn delete_user_key_if_empty(&self) -> Result<(), AdapterError> {
        self.user.delete_if_empty()
    }

    fn read_machine(&self) -> Result<Option<RegValue>, AdapterError> {
        self.machine.read_raw(AUTORUN)
    }
}

/// The folder that holds every throwaway key.
#[cfg(test)]
const TEST_ROOT: &str = r"Software\CmdStudioTest";

/// A throwaway key `HKCU\Software\CmdStudioTest\<uuid>`, deleted on drop.
#[cfg(test)]
pub(crate) struct TestKey {
    pub(crate) subkey: String,
}

#[cfg(test)]
impl TestKey {
    pub(crate) fn new() -> Self {
        TestKey {
            subkey: format!(r"{TEST_ROOT}\{}", uuid::Uuid::new_v4().simple()),
        }
    }
    pub(crate) fn location(&self, child: &str) -> RegLocation {
        RegLocation::user(&format!(r"{}\{child}", self.subkey))
    }
}

#[cfg(test)]
impl Drop for TestKey {
    fn drop(&mut self) {
        // `delete_subkey_all` removes a whole tree, so it only ever runs on a key below our own folder.
        let ours = self
            .subkey
            .strip_prefix(TEST_ROOT)
            .is_some_and(|rest| rest.starts_with('\\'));
        if !ours {
            eprintln!(
                "TestKey: not deleting HKCU\\{}, it is not below HKCU\\{TEST_ROOT}",
                self.subkey
            );
            return;
        }
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        // Only "already gone" is fine: any other error leaves a key behind, so it is said out loud.
        match hkcu.delete_subkey_all(&self.subkey) {
            Ok(()) => {}
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            Err(e) => eprintln!("TestKey: could not delete HKCU\\{}: {e}", self.subkey),
        }
        // The folder goes too once it is empty. `RegDeleteKeyExW` refuses a key that still has
        // subkeys ("access denied"), so while another test's key lives (tests run in parallel) that
        // refusal is expected, and so is a folder another test has already removed.
        match hkcu.delete_subkey(TEST_ROOT) {
            Ok(()) => {}
            Err(e) if matches!(e.kind(), ErrorKind::NotFound | ErrorKind::PermissionDenied) => {}
            Err(e) => eprintln!("TestKey: could not delete HKCU\\{TEST_ROOT}: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::{REG_EXPAND_SZ as EXPAND, REG_SZ as SZ};

    fn adapter(t: &TestKey) -> WinAutoRun {
        WinAutoRun::at(t.location("Command Processor"), t.location("Machine"))
    }

    #[test]
    fn a_missing_key_reads_as_absent_and_is_created_on_write() {
        let t = TestKey::new();
        let a = adapter(&t);
        assert!(!a.user_key_exists().unwrap());
        assert_eq!(a.read_user().unwrap(), None);
        assert_eq!(a.read_machine().unwrap(), None);
        a.delete_user().unwrap();
        // Reading and deleting on a missing key leave it missing: doctor reads the real AutoRun,
        // and a read must never create a key.
        assert!(!a.user_key_exists().unwrap());
        assert!(!t.location("Machine").exists().unwrap());
        a.write_user(&RegValue::from_text(SZ, "echo hi")).unwrap();
        assert!(a.user_key_exists().unwrap());
        assert_eq!(
            a.read_user().unwrap().unwrap().text().as_deref(),
            Some("echo hi")
        );
        // The key now exists without the value: that reads as None, and deleting what is not
        // there is fine, also twice (every op can run again safely, spec §5.5).
        a.delete_user().unwrap();
        assert_eq!(a.read_user().unwrap(), None);
        a.delete_user().unwrap();
        assert!(a.user_key_exists().unwrap());
        // A key that was never there has nothing to delete either.
        let never = t.location("Never");
        never.delete_if_empty().unwrap();
        assert!(!never.exists().unwrap());
    }

    #[test]
    fn raw_values_come_back_byte_for_byte_with_their_type() {
        // Review Focus 4: Thai text. REG_EXPAND_SZ stays REG_EXPAND_SZ, unexpanded.
        let t = TestKey::new();
        let a = adapter(&t);
        let v = RegValue::from_text(EXPAND, "%USERPROFILE%\\x.cmd & ผู้ใช้");
        a.write_user(&v).unwrap();
        assert_eq!(a.read_user().unwrap(), Some(v.clone()));
        let read = a.read_user().unwrap().unwrap();
        a.write_user(&read).unwrap();
        assert_eq!(a.read_user().unwrap(), Some(v));
        // No type turns into another (REG_DWORD_BIG_ENDIAN into REG_LINK, say), whatever the bytes.
        let types = t.location("Types");
        for kind in 0..=11u32 {
            let marker = u8::try_from(kind).unwrap();
            let w = RegValue {
                kind,
                bytes: vec![marker, 0, 0, 0, 0xFF, 0, 0, 0],
            };
            types.write_raw("V", &w).unwrap();
            assert_eq!(
                types.read_raw("V").unwrap(),
                Some(w),
                "registry type {kind}"
            );
        }
        // 12 is no registry type: refused, and the key it was meant for is not created.
        let unknown = t.location("Unknown");
        let err = unknown
            .write_raw(
                "V",
                &RegValue {
                    kind: 12,
                    bytes: vec![0, 0],
                },
            )
            .unwrap_err();
        assert!(err.message.contains("12"), "{err}");
        assert!(!unknown.exists().unwrap());
    }

    #[test]
    fn other_values_and_subkeys_survive_write_and_delete() {
        // spec §2.2 lesson 9: never recreate the key.
        let t = TestKey::new();
        let loc = t.location("Command Processor");
        let key = loc.open_or_create().unwrap();
        key.set_value("CompletionChar", &0x40u32).unwrap();
        key.create_subkey("Child").unwrap();
        drop(key);
        let a = adapter(&t);
        a.write_user(&RegValue::from_text(SZ, "x")).unwrap();
        a.delete_user().unwrap();
        a.delete_user_key_if_empty().unwrap();
        let key = loc.open(false).unwrap().expect("key still there");
        assert_eq!(key.get_value::<u32, _>("CompletionChar").unwrap(), 0x40);
        assert!(key.open_subkey("Child").is_ok());
        // Only a key with neither goes: each half on its own is enough to keep a key.
        let only_value = t.location("OnlyValue");
        only_value
            .open_or_create()
            .unwrap()
            .set_value("CompletionChar", &0x40u32)
            .unwrap();
        let only_subkey = t.location("OnlySubkey");
        only_subkey
            .open_or_create()
            .unwrap()
            .create_subkey("Child")
            .unwrap();
        only_value.delete_if_empty().unwrap();
        only_subkey.delete_if_empty().unwrap();
        assert!(only_value.exists().unwrap());
        assert!(only_subkey.exists().unwrap());
    }

    #[test]
    fn an_empty_key_we_created_can_go_again() {
        let t = TestKey::new();
        let a = adapter(&t);
        a.write_user(&RegValue::from_text(SZ, "x")).unwrap();
        a.delete_user().unwrap();
        a.delete_user_key_if_empty().unwrap();
        assert!(!a.user_key_exists().unwrap());
    }

    #[test]
    fn the_machine_value_is_read_from_its_own_key() {
        let t = TestKey::new();
        t.location("Machine")
            .write_raw(AUTORUN, &RegValue::from_text(SZ, "clink inject"))
            .unwrap();
        let a = adapter(&t);
        assert_eq!(
            a.read_machine().unwrap().unwrap().text().as_deref(),
            Some("clink inject")
        );
        assert_eq!(a.read_user().unwrap(), None);
        // real() wires the same two keys to the real places: the user's under HKCU, the machine's
        // under HKLM, both at the one path cmd reads AutoRun from. This only compares locations;
        // it opens nothing.
        let real = WinAutoRun::real();
        assert_eq!(real.user, RegLocation::user(COMMAND_PROCESSOR));
        assert_eq!(real.machine, RegLocation::machine(COMMAND_PROCESSOR));
        assert_eq!(real.user.hive, Hive::CurrentUser);
        assert_eq!(real.machine.hive, Hive::LocalMachine);
        assert_eq!(COMMAND_PROCESSOR, r"Software\Microsoft\Command Processor");
        assert_eq!(AUTORUN, "AutoRun");
    }

    /// Real-location check: reads the user's real AutoRun and writes the same
    /// bytes back. Runs only with CMDSTUDIO_SYSTEM_TESTS=1 (CI and test VMs).
    #[test]
    fn system_real_autorun_round_trips() {
        // Puts the user's AutoRun back however the test ends, so a failed assert or unwrap can
        // never leave the probe as the real AutoRun.
        struct Restore<'a> {
            a: &'a WinAutoRun,
            existed: bool,
            before: Option<&'a RegValue>,
        }
        impl Drop for Restore<'_> {
            fn drop(&mut self) {
                let value = match self.before {
                    Some(v) => self.a.write_user(v),
                    None => self.a.delete_user(),
                };
                // A key this run created goes again; one that was there stays.
                let key = if self.existed {
                    Ok(())
                } else {
                    self.a.delete_user_key_if_empty()
                };
                for e in [value, key].into_iter().filter_map(Result::err) {
                    eprintln!(
                        "COULD NOT PUT THE USER'S AutoRun BACK: {e}. It was {:?}",
                        self.before
                    );
                }
            }
        }

        // Only the value 1 enables this: CMDSTUDIO_SYSTEM_TESTS=0 or an empty value does not.
        if std::env::var("CMDSTUDIO_SYSTEM_TESTS").as_deref() != Ok("1") {
            println!("SKIPPED system_real_autorun_round_trips: set CMDSTUDIO_SYSTEM_TESTS=1");
            return;
        }
        let a = WinAutoRun::real();
        let existed = a.user_key_exists().unwrap();
        let before = a.read_user().unwrap();
        let restore = Restore {
            a: &a,
            existed,
            before: before.as_ref(),
        };
        let probe = RegValue::from_text(SZ, "rem cmdstudio system test");
        a.write_user(&probe).unwrap();
        assert_eq!(a.read_user().unwrap(), Some(probe.clone()));
        // The probe went to HKCU: the machine value (read only) is not it.
        assert_ne!(a.read_machine().unwrap(), Some(probe));
        drop(restore);
        assert_eq!(a.read_user().unwrap(), before);
        assert_eq!(a.user_key_exists().unwrap(), existed);
    }
}
