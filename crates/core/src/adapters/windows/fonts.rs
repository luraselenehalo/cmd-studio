//! Per-user fonts (spec §5.4 `Fonts`, G12): the file lives in
//! `%LOCALAPPDATA%\Microsoft\Windows\Fonts`, the registration value in
//! `HKCU\Software\Microsoft\Windows NT\CurrentVersion\Fonts`, and
//! `AddFontResourceW` plus a `WM_FONTCHANGE` broadcast make it usable now.
//!
//! The GDI calls used here (`GetDC`, `ReleaseDC`, `EnumFontFamiliesExW`, `AddFontResourceW`,
//! `RemoveFontResourceW`) name no error code in their documentation; `AddFontResourceW` says "No
//! extended error information is available". What `GetLastError` may hold after them is a stale
//! code from some earlier call, or "The operation completed successfully". So an error reported
//! for one of them is a fixed sentence that names the call, and the file where there is one, and
//! `GetLastError` is never read here. Where Windows only says that a call failed, the sentence
//! ends with `NO_REASON`.
use super::registry::RegLocation;
use super::wide;
use crate::adapters::{AdapterError, Fonts, RegValue, REG_SZ};
use std::path::{Path, PathBuf};
use windows_sys::Win32::Foundation::LPARAM;
use windows_sys::Win32::Graphics::Gdi::{
    AddFontResourceW, EnumFontFamiliesExW, GetDC, ReleaseDC, RemoveFontResourceW, DEFAULT_CHARSET,
    HDC, LOGFONTW, TEXTMETRICW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_FONTCHANGE,
};

/// The per-user key where Windows lists the fonts it loads at sign-in.
pub const FONTS_SUBKEY: &str = r"Software\Microsoft\Windows NT\CurrentVersion\Fonts";

/// How many times `unload` asks Windows to drop one file. Windows keeps a resource count for a
/// font, and the documented way to bring it to zero is to call `RemoveFontResourceW` in a loop
/// until it returns FALSE. A font file added 32 times is far beyond anything real, so a loop that
/// gets this far has found something wrong.
const MAX_REMOVALS: u32 = 32;

/// The end of a fixed sentence for a call that failed and says nothing more.
const NO_REASON: &str = "Windows gives no further reason";

/// The user's fonts: the family list GDI shows, the per-user fonts folder, the registration
/// values and the loading into this session.
#[derive(Clone, Debug)]
pub struct WinFonts {
    dir: PathBuf,
    reg: RegLocation,
    /// When false, `load`/`unload` leave the session's font table alone.
    /// Tests use false: `AddFontResourceW` and the broadcast reach every
    /// program the developer has open.
    session: bool,
}

impl WinFonts {
    /// The user's own fonts folder and registration key, with session loading on.
    pub fn real(local_app_data: &Path) -> Self {
        WinFonts {
            dir: local_app_data
                .join("Microsoft")
                .join("Windows")
                .join("Fonts"),
            reg: RegLocation::user(FONTS_SUBKEY),
            session: true,
        }
    }

    /// Other places, for tests.
    pub fn at(dir: PathBuf, reg: RegLocation, session: bool) -> Self {
        WinFonts { dir, reg, session }
    }
}

fn no_screen_dc() -> AdapterError {
    AdapterError::new(
        "GetDC",
        format!("returned NULL, so there is no device context for the screen; {NO_REASON}"),
    )
}

fn no_families() -> AdapterError {
    AdapterError::new(
        "EnumFontFamiliesExW",
        format!("GDI listed no font families; {NO_REASON}"),
    )
}

fn add_failed(file: &Path) -> AdapterError {
    AdapterError::new(
        format!("AddFontResourceW {}", file.display()),
        format!(
            "no font was added, for example because the file is missing, is not a font Windows can load, or is damaged; {NO_REASON}"
        ),
    )
}

/// Not a failure Windows reported: every removal answered that it removed something.
fn still_loaded(file: &Path) -> AdapterError {
    AdapterError::new(
        format!("RemoveFontResourceW {}", file.display()),
        format!("the file was still loaded after {MAX_REMOVALS} removals"),
    )
}

/// The device context of the whole screen, released when the guard goes out of scope. GDI wants it
/// released on the thread that got it, and the raw handle inside keeps this type (neither `Send` nor
/// `Sync`) on that thread.
struct ScreenDc(HDC);

impl ScreenDc {
    fn acquire() -> Result<Self, AdapterError> {
        // SAFETY: no pointers are involved: a NULL window handle asks for the DC of the whole screen.
        let dc = unsafe { GetDC(std::ptr::null_mut()) };
        if dc.is_null() {
            Err(no_screen_dc())
        } else {
            Ok(ScreenDc(dc))
        }
    }
}

impl Drop for ScreenDc {
    fn drop(&mut self) {
        // SAFETY: `self.0` is the DC that `acquire` got from GetDC with a NULL window handle, this
        // guard is its only owner, and it is released here, once, on the thread that got it. The
        // result is not used: if Windows says it did not release the DC, the cost is one handle
        // held until the process ends, and the font list has been read by then.
        unsafe { ReleaseDC(std::ptr::null_mut(), self.0) };
    }
}

/// The callback `EnumFontFamiliesExW` calls once per family. It adds the family's name to the
/// `Vec<String>` whose address is in `lparam` and returns 1 to go on (0 stops the enumeration).
///
/// It must not panic: GDI calls it through an `extern "system"` boundary, which a panic cannot cross
/// (the whole process would abort). So it has no indexing, no unwrap and no arithmetic, and a NULL
/// pointer is checked instead of followed. Running out of memory aborts the process as well, and
/// does not unwind either.
///
/// # Safety
/// `lf` must be NULL or point to a valid `LOGFONTW`, and `lparam` must be 0 or the address of a live
/// `Vec<String>` that nothing else uses while this call runs.
unsafe extern "system" fn collect(
    lf: *const LOGFONTW,
    _metrics: *const TEXTMETRICW,
    _font_type: u32,
    lparam: LPARAM,
) -> i32 {
    // SAFETY: the caller promises `lparam` is 0 or the address of a live `Vec<String>` that nothing
    // else uses during this call. `as_mut` turns 0 into None, so a reference is only made for a real
    // Vec, and it is the only one to it while this call runs.
    let Some(names) = (unsafe { (lparam as *mut Vec<String>).as_mut() }) else {
        return 0;
    };
    // SAFETY: the caller promises `lf` is NULL or points to a valid LOGFONTW, which GDI keeps valid
    // until this callback returns. `as_ref` turns NULL into None, and the reference does not outlive
    // this call.
    let Some(font) = (unsafe { lf.as_ref() }) else {
        return 1;
    };
    // `lfFaceName` holds 32 units with a NUL after the name, unless the name fills it. Taking the
    // units up to the NUL, or all 32, never reads past the array.
    let name = char::decode_utf16(
        font.lfFaceName
            .iter()
            .copied()
            .take_while(|unit| *unit != 0),
    )
    .map(|unit| unit.unwrap_or(char::REPLACEMENT_CHARACTER))
    .collect::<String>();
    names.push(name);
    1
}

/// What GDI lists, as it lists it: one name per family, in character set `DEFAULT_CHARSET`.
fn enumerate() -> Result<Vec<String>, AdapterError> {
    let dc = ScreenDc::acquire()?;
    // An empty face name with DEFAULT_CHARSET asks for every uniquely named font, once.
    let filter = LOGFONTW {
        lfCharSet: DEFAULT_CHARSET,
        ..LOGFONTW::default()
    };
    let mut names: Vec<String> = Vec::new();
    // SAFETY: `dc.0` is a live screen DC, which the guard releases only after this call. `&filter`
    // points to a LOGFONTW that outlives the call and that GDI only reads. `collect` has the
    // signature GDI expects. The LPARAM is the address of `names`, a local that outlives the call and
    // that nothing else touches until the call returns: GDI runs the callback on this thread, one
    // entry at a time, so the callback holds the only access to `names` while it runs, and GDI keeps
    // no copy of the address afterwards. The result is the last value the callback returned and says
    // nothing about errors, so it is not used.
    unsafe { EnumFontFamiliesExW(dc.0, &filter, Some(collect), (&raw mut names) as LPARAM, 0) };
    Ok(names)
}

/// The family list callers see. '@' names are the vertical-writing twins of CJK fonts, and an empty
/// name is no family; what is left is sorted and has no duplicates. Names stay as GDI spelled them.
///
/// Every Windows has dozens of families, so a list with nothing left means the enumeration did not
/// work. That is an error, not an answer ("no font is installed").
fn family_list(mut names: Vec<String>) -> Result<Vec<String>, AdapterError> {
    names.retain(|n| !n.is_empty() && !n.starts_with('@'));
    names.sort();
    names.dedup();
    if names.is_empty() {
        Err(no_families())
    } else {
        Ok(names)
    }
}

/// Tells every top-level window that the font list changed. A window that does not answer within a
/// second is skipped, and a hung one at once. The wait adds up over windows that are busy without
/// being hung (Windows documents up to the timeout for each top-level window), so a caller with a
/// window of its own should not call `load` or `unload` from its UI thread.
fn broadcast() {
    let mut result = 0usize;
    // SAFETY: HWND_BROADCAST is a special window value, not a pointer; WM_FONTCHANGE carries no data
    // (both parameters are 0); `result` is a local that is valid for the one write the call may make
    // through its address, and the call has returned before `result` goes out of scope. The return
    // value is not used: for a broadcast it says nothing about single windows, and a window that
    // misses the notice sees the font when it restarts, while the font is already loaded.
    unsafe {
        SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_FONTCHANGE,
            0,
            0,
            SMTO_ABORTIFHUNG,
            1000,
            &mut result,
        )
    };
}

/// Calls `remove` until it answers false, which means the font table holds nothing more for the
/// file: `true` then. `false` when it still answered true after `MAX_REMOVALS` calls.
fn drop_all_references(mut remove: impl FnMut() -> bool) -> bool {
    (0..MAX_REMOVALS).any(|_| !remove())
}

impl Fonts for WinFonts {
    fn families(&self) -> Result<Vec<String>, AdapterError> {
        family_list(enumerate()?)
    }

    fn user_fonts_dir(&self) -> PathBuf {
        self.dir.clone()
    }

    /// Only a `REG_SZ` value with valid text counts. Any other value is an error and not "absent":
    /// the caller records what this reports, writes its own value over it, and writes the record
    /// back on undo. Reporting a value it cannot write back (a `REG_DWORD`, or a `REG_EXPAND_SZ`
    /// whose `%VAR%` would turn into plain text) as absent would make the undo delete or damage it.
    fn registration(&self, value_name: &str) -> Result<Option<String>, AdapterError> {
        let Some(value) = self.reg.read_raw(value_name)? else {
            return Ok(None);
        };
        let wrong = |why: String| {
            AdapterError::new(
                format!("read {value_name}"),
                format!("the value in {} {why}", self.reg),
            )
        };
        if value.kind != REG_SZ {
            return Err(wrong(format!(
                "has registry type {}, not REG_SZ",
                value.kind
            )));
        }
        value
            .text()
            .map(Some)
            .ok_or_else(|| wrong("is not valid UTF-16 text".to_string()))
    }

    fn register(&self, value_name: &str, file: &str) -> Result<(), AdapterError> {
        self.reg
            .write_raw(value_name, &RegValue::from_text(REG_SZ, file))
    }

    fn unregister(&self, value_name: &str) -> Result<(), AdapterError> {
        self.reg.delete_value(value_name)
    }

    fn registration_key_exists(&self) -> Result<bool, AdapterError> {
        self.reg.exists()
    }

    fn delete_registration_key_if_empty(&self) -> Result<(), AdapterError> {
        self.reg.delete_if_empty()
    }

    fn load(&self, file: &Path) -> Result<(), AdapterError> {
        if !self.session {
            return Ok(());
        }
        let path = wide(file.as_os_str());
        // SAFETY: `path` is a NUL-terminated UTF-16 string that this function owns and keeps alive
        // past the call, and AddFontResourceW only reads it during the call.
        let added = unsafe { AddFontResourceW(path.as_ptr()) };
        if added == 0 {
            return Err(add_failed(file));
        }
        broadcast();
        Ok(())
    }

    fn unload(&self, file: &Path) -> Result<(), AdapterError> {
        if !self.session {
            return Ok(());
        }
        let path = wide(file.as_os_str());
        // FALSE from the first call means the file was not loaded, which is the state wanted.
        let gone = drop_all_references(|| {
            // SAFETY: as in `load`: `path` is NUL-terminated, owned by this function, and only read
            // during the call.
            unsafe { RemoveFontResourceW(path.as_ptr()) != 0 }
        });
        broadcast();
        if gone {
            Ok(())
        } else {
            Err(still_loaded(file))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::windows::registry::TestKey;
    use crate::adapters::REG_EXPAND_SZ;
    use std::io::ErrorKind;

    #[test]
    fn families_lists_gdi_names_without_vertical_twins() {
        let t = TestKey::new();
        let f = WinFonts::at(PathBuf::from(r"C:\unused"), t.location("unused"), false);
        let names = f.families().unwrap();
        assert!(names.iter().any(|n| n == "Arial"), "{names:?}");
        assert!(names.iter().all(|n| !n.starts_with('@')));
        let mut sorted = names.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(names, sorted);
    }

    #[test]
    fn registration_values_live_in_the_given_key() {
        // Review Focus 4: a Thai user folder in the registration data. A new account
        // may not have the key; it goes again once empty.
        let t = TestKey::new();
        let f = WinFonts::at(PathBuf::from(r"C:\unused"), t.location("Fonts"), false);
        let name = "CaskaydiaCove NF (TrueType)";
        assert_eq!(f.registration(name).unwrap(), None);
        assert!(!f.registration_key_exists().unwrap());
        f.register(
            name,
            r"C:\Users\ผู้ใช้\AppData\Local\Microsoft\Windows\Fonts\a.ttf",
        )
        .unwrap();
        assert_eq!(
            f.registration(name).unwrap().as_deref(),
            Some(r"C:\Users\ผู้ใช้\AppData\Local\Microsoft\Windows\Fonts\a.ttf")
        );
        assert!(f.registration_key_exists().unwrap());
        f.delete_registration_key_if_empty().unwrap();
        assert!(
            f.registration_key_exists().unwrap(),
            "a key with values stays"
        );
        f.unregister(name).unwrap();
        f.unregister(name).unwrap();
        assert_eq!(f.registration(name).unwrap(), None);
        f.delete_registration_key_if_empty().unwrap();
        assert!(!f.registration_key_exists().unwrap());
    }

    #[test]
    fn test_mode_never_touches_the_session_font_table() {
        let t = TestKey::new();
        let f = WinFonts::at(PathBuf::from(r"C:\unused"), t.location("unused"), false);
        // The file does not exist; with the session on, AddFontResourceW would fail.
        f.load(Path::new(r"C:\no\such\font.ttf")).unwrap();
        f.unload(Path::new(r"C:\no\such\font.ttf")).unwrap();
    }

    #[test]
    fn listing_the_families_gives_the_screen_dc_back() {
        use windows_sys::Win32::System::Threading::{
            GetCurrentProcess, GetGuiResources, GR_GDIOBJECTS,
        };
        let gdi_objects = || {
            // SAFETY: GetCurrentProcess returns a pseudo-handle that needs no closing, and
            // GetGuiResources only counts the process's objects.
            unsafe { GetGuiResources(GetCurrentProcess(), GR_GDIOBJECTS) }
        };
        let t = TestKey::new();
        let f = WinFonts::at(PathBuf::from(r"C:\unused"), t.location("unused"), false);
        let before = gdi_objects();
        for _ in 0..200 {
            f.families().unwrap();
        }
        let after = gdi_objects();
        // A DC that is not given back counts once per call (200 here). Other tests that list
        // fonts at the same time hold a few for a moment.
        assert!(
            after < before + 50,
            "GDI objects grew from {before} to {after}: a screen DC was not released"
        );
    }

    #[test]
    fn real_points_at_the_users_own_places_and_turns_the_session_on() {
        // Compares places only; nothing is opened.
        let f = WinFonts::real(Path::new(r"C:\Users\x\AppData\Local"));
        assert_eq!(
            f.user_fonts_dir(),
            PathBuf::from(r"C:\Users\x\AppData\Local\Microsoft\Windows\Fonts")
        );
        assert_eq!(
            f.reg,
            RegLocation::user(r"Software\Microsoft\Windows NT\CurrentVersion\Fonts")
        );
        assert_eq!(
            FONTS_SUBKEY,
            r"Software\Microsoft\Windows NT\CurrentVersion\Fonts"
        );
        assert!(f.session);
        let other = WinFonts::at(
            PathBuf::from(r"C:\d"),
            RegLocation::user(r"Software\k"),
            false,
        );
        assert_eq!(other.user_fonts_dir(), PathBuf::from(r"C:\d"));
        assert_eq!(other.reg, RegLocation::user(r"Software\k"));
        assert!(!other.session);
    }

    #[test]
    fn a_registration_is_stored_as_reg_sz_with_the_full_path() {
        let t = TestKey::new();
        let loc = t.location("Fonts");
        let f = WinFonts::at(PathBuf::from(r"C:\unused"), loc.clone(), false);
        f.register("A (TrueType)", r"C:\f\a.ttf").unwrap();
        assert_eq!(
            loc.read_raw("A (TrueType)").unwrap(),
            Some(RegValue::from_text(REG_SZ, r"C:\f\a.ttf"))
        );
    }

    #[test]
    fn a_registration_value_that_is_not_plain_text_is_an_error_not_absent() {
        // The apply step records this value before it writes its own and writes the record back on
        // undo, as a REG_SZ. Reporting a value it cannot write back as "absent" would make the undo
        // delete it; a REG_EXPAND_SZ would come back with its %VAR% as plain text.
        let t = TestKey::new();
        let loc = t.location("Fonts");
        let f = WinFonts::at(PathBuf::from(r"C:\unused"), loc.clone(), false);
        let name = "Odd (TrueType)";
        let odd = [
            (
                RegValue {
                    kind: 4, // REG_DWORD
                    bytes: vec![1, 0, 0, 0],
                },
                "registry type 4, not REG_SZ",
            ),
            (
                RegValue::from_text(REG_EXPAND_SZ, r"%LOCALAPPDATA%\x.ttf"),
                "registry type 2, not REG_SZ",
            ),
            (
                RegValue {
                    kind: REG_SZ,
                    bytes: vec![0x00, 0xD8, 0, 0], // a lone surrogate
                },
                "not valid UTF-16 text",
            ),
        ];
        for (value, why) in odd {
            loc.write_raw(name, &value).unwrap();
            let err = f.registration(name).unwrap_err();
            assert!(err.message.contains(why), "{err}");
            assert!(err.context.contains(name), "{err}");
            // Reading never touches the value.
            assert_eq!(loc.read_raw(name).unwrap(), Some(value));
        }
        // And the plain kind is still read.
        f.register(name, r"C:\f\a.ttf").unwrap();
        assert_eq!(
            f.registration(name).unwrap().as_deref(),
            Some(r"C:\f\a.ttf")
        );
    }

    fn owned(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| (*n).to_string()).collect()
    }

    #[test]
    fn the_family_list_drops_twins_and_empty_names_sorts_and_has_no_duplicates() {
        let raw = [
            "Segoe UI",
            "@MS Gothic",
            "Arial",
            "",
            "Arial",
            "@Arial",
            "Zapf",
            "arial",
        ];
        // Names stay as GDI spelled them: "arial" is not "Arial".
        assert_eq!(
            family_list(owned(&raw)).unwrap(),
            ["Arial", "Segoe UI", "Zapf", "arial"]
        );
    }

    #[test]
    fn a_family_list_with_nothing_left_is_an_error_not_an_empty_answer() {
        // "No font is installed" would be an answer; a failed enumeration is not.
        for raw in [&[][..], &["", "@MS Gothic"][..]] {
            let err = family_list(owned(raw)).unwrap_err();
            assert_eq!(err.context, "EnumFontFamiliesExW");
            assert!(err.message.contains("no font families"), "{err}");
        }
    }

    /// A LOGFONTW whose face name starts with `face` and is otherwise zero.
    fn log_font(face: &[u16]) -> LOGFONTW {
        let mut lf = LOGFONTW::default();
        for (slot, unit) in lf.lfFaceName.iter_mut().zip(face) {
            *slot = *unit;
        }
        lf
    }

    /// Runs the enumeration callback once, the way GDI does.
    fn call_collect(lf: *const LOGFONTW, names: *mut Vec<String>) -> i32 {
        // SAFETY: each caller passes NULL or a LOGFONTW that lives through this call, and NULL or
        // the address of a Vec<String> that nothing else uses during it.
        unsafe { collect(lf, std::ptr::null(), 0, names as LPARAM) }
    }

    #[test]
    fn the_callback_copies_the_face_name_up_to_its_nul_and_never_past_the_array() {
        let utf16 = |s: &str| s.encode_utf16().collect::<Vec<u16>>();
        let mut names: Vec<String> = Vec::new();
        // "Arial", a NUL, and leftovers that are not part of the name.
        let mut with_leftovers = utf16("Arial");
        with_leftovers.push(0);
        with_leftovers.extend(utf16("junk"));
        assert_eq!(call_collect(&log_font(&with_leftovers), &mut names), 1);
        assert_eq!(call_collect(&log_font(&utf16("ฟอนต์ไทย")), &mut names), 1);
        // All 32 units used and no NUL: exactly those 32, and nothing read after the array.
        assert_eq!(
            call_collect(&log_font(&[u16::from(b'x'); 32]), &mut names),
            1
        );
        assert_eq!(call_collect(&log_font(&[]), &mut names), 1);
        assert_eq!(
            names,
            vec![
                "Arial".to_string(),
                "ฟอนต์ไทย".to_string(),
                "x".repeat(32),
                String::new()
            ]
        );
    }

    #[test]
    fn the_callback_survives_null_pointers_instead_of_crashing() {
        let mut names: Vec<String> = Vec::new();
        // No font entry: it is skipped and the enumeration goes on.
        assert_eq!(call_collect(std::ptr::null(), &mut names), 1);
        // Nowhere to put the names: the enumeration stops.
        assert_eq!(
            call_collect(&log_font(&[u16::from(b'A')]), std::ptr::null_mut()),
            0
        );
        assert!(names.is_empty());
    }

    #[test]
    fn unloading_repeats_until_windows_has_nothing_left_to_remove() {
        // Windows counts each add of a file, and the documented way to bring the count to zero
        // is to call RemoveFontResource until it returns FALSE.
        let (mut left, mut calls) = (3, 0);
        assert!(drop_all_references(|| {
            calls += 1;
            if left == 0 {
                false
            } else {
                left -= 1;
                true
            }
        }));
        assert_eq!((left, calls), (0, 4));
        // Not loaded at all: one call, and that is fine.
        calls = 0;
        assert!(drop_all_references(|| {
            calls += 1;
            false
        }));
        assert_eq!(calls, 1);
        // A font table that never lets go is reported, after a bounded number of tries.
        calls = 0;
        assert!(!drop_all_references(|| {
            calls += 1;
            true
        }));
        assert_eq!(calls, MAX_REMOVALS);
    }

    #[test]
    fn calls_without_an_error_code_report_fixed_words_never_the_threads_last_error() {
        use windows_sys::Win32::Foundation::SetLastError;
        let file = Path::new(r"C:\Users\ผู้ใช้\Fonts\a.ttf");
        // Whatever an earlier call left behind (5 is "Access is denied") must not reach the text.
        // SAFETY: SetLastError takes a plain number and only stores it for this thread.
        unsafe { SetLastError(5) };
        let errors = [
            no_screen_dc(),
            no_families(),
            add_failed(file),
            still_loaded(file),
        ];
        assert_eq!(errors[0].context, "GetDC");
        assert_eq!(errors[1].context, "EnumFontFamiliesExW");
        assert_eq!(
            errors[2].context,
            r"AddFontResourceW C:\Users\ผู้ใช้\Fonts\a.ttf"
        );
        assert_eq!(
            errors[3].context,
            r"RemoveFontResourceW C:\Users\ผู้ใช้\Fonts\a.ttf"
        );
        // A call that failed and says nothing more ends in the same words.
        for e in &errors[..3] {
            assert!(e.message.contains("no further reason"), "{e}");
            assert!(e.message.ends_with(NO_REASON), "{e}");
        }
        // The causes of an AddFontResourceW failure are examples, not what Windows said.
        assert!(errors[2].message.contains("for example"), "{}", errors[2]);
        // The removal loop gives up by its own count. Windows reported nothing wrong, so the
        // text says what happened and does not claim a reason is missing.
        assert_eq!(
            errors[3].message,
            format!("the file was still loaded after {MAX_REMOVALS} removals")
        );
        for e in &errors {
            assert!(!e.message.contains("os error"), "{e}");
            assert!(!e.message.contains("Access is denied"), "{e}");
        }
    }

    /// What a font install run changed, so that it can be put back however the run ends: a
    /// failing assert or unwrap must never leave a developer's fonts changed. Each flag is set
    /// before its step, so a step that fails half-way is undone as well. One `unload` takes every
    /// reference to the file, so the one `loaded` flag covers any number of `load` calls.
    struct Undo<'a> {
        fonts: &'a WinFonts,
        value_name: &'a str,
        dest: &'a Path,
        /// The per-user fonts folder and the registration key as the run found them: one the run
        /// made goes again (a new account may have neither).
        dir_existed: bool,
        key_existed: bool,
        copied: bool,
        registered: bool,
        loaded: bool,
    }

    impl Drop for Undo<'_> {
        fn drop(&mut self) {
            let mut left: Vec<String> = Vec::new();
            if self.loaded {
                if let Err(e) = self.fonts.unload(self.dest) {
                    left.push(e.to_string());
                }
            }
            if self.registered {
                if let Err(e) = self.fonts.unregister(self.value_name) {
                    left.push(e.to_string());
                }
            }
            if self.copied {
                // A virus scanner or the font cache may hold the fresh file for a moment (errors
                // 5 and 32), so a busy file is tried again a few times before it is reported.
                let mut tries = 0;
                loop {
                    match std::fs::remove_file(self.dest) {
                        Ok(()) => break,
                        Err(e) if e.kind() == ErrorKind::NotFound => break,
                        Err(e) if tries < 20 && matches!(e.raw_os_error(), Some(5 | 32)) => {
                            tries += 1;
                            std::thread::sleep(std::time::Duration::from_millis(50));
                        }
                        Err(e) => {
                            left.push(format!("delete {}: {e}", self.dest.display()));
                            break;
                        }
                    }
                }
            }
            if !self.dir_existed {
                // `remove_dir` refuses a folder with something in it, which then is not ours to remove.
                match std::fs::remove_dir(self.fonts.user_fonts_dir()) {
                    Ok(()) => {}
                    Err(e)
                        if matches!(
                            e.kind(),
                            ErrorKind::NotFound | ErrorKind::DirectoryNotEmpty
                        ) => {}
                    Err(e) => left.push(format!(
                        "delete {}: {e}",
                        self.fonts.user_fonts_dir().display()
                    )),
                }
            }
            if !self.key_existed {
                if let Err(e) = self.fonts.delete_registration_key_if_empty() {
                    left.push(e.to_string());
                }
            }
            for problem in left {
                eprintln!(
                    "COULD NOT PUT THE USER'S FONTS BACK: {problem}. Check the file {} and the value {}",
                    self.dest.display(),
                    self.value_name
                );
            }
        }
    }

    /// Installs `src` the way the app does (copy, register, load) in `fonts`' own places, calls
    /// `while_installed`, and puts everything back afterwards, also when a step or
    /// `while_installed` panics.
    fn run_installed(
        fonts: &WinFonts,
        src: &Path,
        value_name: &str,
        while_installed: impl FnOnce(),
    ) {
        let dest = fonts.user_fonts_dir().join(src.file_name().unwrap());
        // Nothing is changed before these checks pass: they protect what the user already has.
        assert!(!dest.exists(), "{} already exists", dest.display());
        assert_eq!(
            fonts.registration(value_name).unwrap(),
            None,
            "{value_name} is already registered"
        );
        let mut undo = Undo {
            fonts,
            value_name,
            dest: &dest,
            dir_existed: fonts.user_fonts_dir().is_dir(),
            key_existed: fonts.registration_key_exists().unwrap(),
            copied: false,
            registered: false,
            loaded: false,
        };
        std::fs::create_dir_all(fonts.user_fonts_dir()).unwrap();
        undo.copied = true;
        std::fs::copy(src, &dest).unwrap();
        undo.registered = true;
        fonts.register(value_name, dest.to_str().unwrap()).unwrap();
        undo.loaded = true;
        fonts.load(&dest).unwrap();
        while_installed();
    }

    /// A stand-in for a font file; with the session off it is never loaded.
    fn fake_font(dir: &Path) -> PathBuf {
        let src = dir.join("Probe-Regular.ttf");
        std::fs::write(&src, b"not a font").unwrap();
        src
    }

    /// Whether `run` panicked. The panic's message still goes to stderr.
    fn panicked(run: impl FnOnce()) -> bool {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(run)).is_err()
    }

    #[test]
    fn the_undo_guard_puts_back_what_a_failed_run_changed() {
        let tmp = tempfile::tempdir().unwrap();
        let src = fake_font(tmp.path());
        let fonts_dir = tmp.path().join("Fonts");
        let t = TestKey::new();
        let f = WinFonts::at(fonts_dir.clone(), t.location("Fonts"), false);
        let name = "Probe (TrueType)";
        let mut during = None;
        // The run makes the folder, the file, the key and the value; then the check fails.
        let died = panicked(|| {
            run_installed(&f, &src, name, || {
                during = Some((
                    fonts_dir.join("Probe-Regular.ttf").is_file(),
                    f.registration(name).unwrap().is_some(),
                    f.registration_key_exists().unwrap(),
                ));
                std::panic::resume_unwind(Box::new("the check failed"));
            });
        });
        assert!(died);
        assert_eq!(
            during,
            Some((true, true, true)),
            "all in place while the check ran"
        );
        assert!(!fonts_dir.exists(), "the folder the run made is gone");
        assert_eq!(f.registration(name).unwrap(), None);
        assert!(
            !f.registration_key_exists().unwrap(),
            "the key the run made is gone"
        );
    }

    #[test]
    fn the_undo_guard_leaves_alone_what_was_there_before() {
        let tmp = tempfile::tempdir().unwrap();
        let src = fake_font(tmp.path());
        let t = TestKey::new();
        // A folder and a key that were there and hold nothing: they stay, because the run did not
        // make them. (A folder or key that is empty would go if it were the run's.)
        let empty_dir = tmp.path().join("EmptyFonts");
        std::fs::create_dir(&empty_dir).unwrap();
        let empty_key = t.location("EmptyKey");
        drop(empty_key.open_or_create().unwrap());
        let f = WinFonts::at(empty_dir.clone(), empty_key, false);
        run_installed(&f, &src, "Probe (TrueType)", || {});
        assert!(empty_dir.is_dir(), "the folder was there before the run");
        assert!(
            f.registration_key_exists().unwrap(),
            "the key was there before the run"
        );
        assert!(!empty_dir.join("Probe-Regular.ttf").exists());
        assert_eq!(f.registration("Probe (TrueType)").unwrap(), None);
        // What other programs keep in the folder and in the key stays as it was.
        let fonts_dir = tmp.path().join("Fonts");
        std::fs::create_dir(&fonts_dir).unwrap();
        std::fs::write(fonts_dir.join("Other.ttf"), b"theirs").unwrap();
        let loc = t.location("Fonts");
        loc.write_raw(
            "Other (TrueType)",
            &RegValue::from_text(REG_SZ, r"C:\x\Other.ttf"),
        )
        .unwrap();
        let f = WinFonts::at(fonts_dir.clone(), loc, false);
        run_installed(&f, &src, "Probe (TrueType)", || {});
        assert!(!fonts_dir.join("Probe-Regular.ttf").exists());
        assert_eq!(f.registration("Probe (TrueType)").unwrap(), None);
        assert_eq!(
            std::fs::read(fonts_dir.join("Other.ttf")).unwrap(),
            b"theirs"
        );
        assert_eq!(
            f.registration("Other (TrueType)").unwrap().as_deref(),
            Some(r"C:\x\Other.ttf")
        );
    }

    #[test]
    fn a_run_refuses_to_start_over_a_file_or_a_value_that_is_already_there() {
        let tmp = tempfile::tempdir().unwrap();
        let src = fake_font(tmp.path());
        let fonts_dir = tmp.path().join("Fonts");
        std::fs::create_dir(&fonts_dir).unwrap();
        let t = TestKey::new();
        let loc = t.location("Fonts");
        let f = WinFonts::at(fonts_dir.clone(), loc.clone(), false);
        let start = || {
            panicked(|| {
                run_installed(&f, &src, "Probe (TrueType)", || {});
            })
        };
        // The user's own file under the same name: not overwritten, not deleted.
        std::fs::write(fonts_dir.join("Probe-Regular.ttf"), b"theirs").unwrap();
        assert!(start());
        assert_eq!(
            std::fs::read(fonts_dir.join("Probe-Regular.ttf")).unwrap(),
            b"theirs"
        );
        assert!(
            !f.registration_key_exists().unwrap(),
            "nothing was registered"
        );
        // The user's own registration under the same name: not replaced, not deleted.
        std::fs::remove_file(fonts_dir.join("Probe-Regular.ttf")).unwrap();
        f.register("Probe (TrueType)", r"C:\x\Probe-Regular.ttf")
            .unwrap();
        assert!(start());
        assert_eq!(
            f.registration("Probe (TrueType)").unwrap().as_deref(),
            Some(r"C:\x\Probe-Regular.ttf")
        );
        assert!(
            !fonts_dir.join("Probe-Regular.ttf").exists(),
            "nothing was copied"
        );
    }

    #[test]
    fn a_step_that_fails_half_way_is_undone_too() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("Missing.ttf");
        let fonts_dir = tmp.path().join("Fonts");
        let t = TestKey::new();
        let f = WinFonts::at(fonts_dir.clone(), t.location("Fonts"), false);
        // The folder is made, then the copy fails.
        let died = panicked(|| {
            run_installed(&f, &missing, "Probe (TrueType)", || {});
        });
        assert!(died);
        assert!(!fonts_dir.exists(), "the folder the run made is gone");
        assert!(!f.registration_key_exists().unwrap());
    }

    /// Real-location check (G12): installs the bundled CaskaydiaCove NF for
    /// this user, adds it to the session a second time, sees GDI list it,
    /// then removes it and sees GDI stop listing it, which shows that one
    /// `unload` takes every reference. Runs only with CMDSTUDIO_SYSTEM_TESTS=1
    /// (CI and test VMs); needs resources\ filled by scripts\fetch-resources.ps1.
    #[test]
    fn system_real_font_install_and_remove() {
        // Only the value 1 enables this: CMDSTUDIO_SYSTEM_TESTS=0 or an empty value does not.
        if std::env::var("CMDSTUDIO_SYSTEM_TESTS").as_deref() != Ok("1") {
            println!("SKIPPED system_real_font_install_and_remove: set CMDSTUDIO_SYSTEM_TESTS=1");
            return;
        }
        use crate::adapters::Host;
        let family = "CaskaydiaCove NF";
        let value_name = "CaskaydiaCove NF (TrueType)";
        let src = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(r"..\..\resources\fonts\CaskaydiaCoveNerdFont-Regular.ttf");
        assert!(
            src.is_file(),
            "{} is missing; run scripts\\fetch-resources.ps1",
            src.display()
        );
        let f = WinFonts::real(
            &crate::adapters::windows::host::WinHost
                .local_app_data()
                .unwrap(),
        );
        assert!(
            !f.families().unwrap().iter().any(|n| n == family),
            "{family} is already installed; run this on a machine without it"
        );
        let dest = f.user_fonts_dir().join("CaskaydiaCoveNerdFont-Regular.ttf");
        // Nothing of the user's may be at that path when the unload below runs.
        assert!(!dest.exists(), "{} already exists", dest.display());
        // Not loaded yet, so there is nothing to remove, and that is not an error.
        f.unload(&dest).unwrap();
        let dir_existed = f.user_fonts_dir().is_dir();
        let key_existed = f.registration_key_exists().unwrap();
        let mut seen = false;
        run_installed(&f, &src, value_name, || {
            // A second add: if Windows counts references, it now holds two to the file. The
            // removal at the end of the run has to take both, or the family stays listed.
            f.load(&dest).unwrap();
            seen = f.families().unwrap().iter().any(|n| n == family);
        });
        // `run_installed` has put everything back; check that before judging the install itself.
        assert!(!dest.exists(), "the copied font file is still there");
        assert_eq!(f.registration(value_name).unwrap(), None);
        assert_eq!(f.registration_key_exists().unwrap(), key_existed);
        assert_eq!(f.user_fonts_dir().is_dir(), dir_existed);
        assert!(
            !f.families().unwrap().iter().any(|n| n == family),
            "{family} is still listed after the removal: unload left a reference to the file"
        );
        assert!(seen, "GDI did not list {family} after the install");
    }
}
