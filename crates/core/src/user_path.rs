//! The user's `Path` (`HKCU\Environment`, spec §5.7, phase 4a): the install folder goes on it
//! so that cmd finds `cmdstudio`, and comes off it at uninstall. The text rules here are pure;
//! [`add`] and [`remove`] run them over a [`UserPath`] adapter. Every other entry stays byte for
//! byte: the value is split at `;` only, entries are compared without case and without trailing
//! `\`, and CMD Studio's entry takes exactly one separator with it, so that taking it out gives
//! the old text back (a value that ended with `;` gets `<dir>;` appended before that end, one
//! that did not gets `;<dir>`, an empty or absent one becomes `<dir>`). Any text that is not
//! plain `REG_SZ`/`REG_EXPAND_SZ` text is never edited (spec §5.4's rule for AutoRun).
use crate::adapters::{AdapterError, RegText, RegValue, UserPath, REG_EXPAND_SZ};
use std::path::Path;
/// Two entries are the same folder when they match without case and without trailing
/// backslashes; an empty entry is never a match.
fn same_entry(entry: &str, dir: &str) -> bool {
    let norm = |s: &str| s.trim_end_matches('\\').to_lowercase();
    !entry.trim().is_empty() && norm(entry) == norm(dir)
}

/// Whether `value` (absent, or its text) holds `dir`.
pub fn has_entry(value: Option<&str>, dir: &Path) -> bool {
    let dir = dir.to_string_lossy();
    value.is_some_and(|v| v.split(';').any(|e| same_entry(e, &dir)))
}

/// The value with `dir` added at its end, or `None` when it is already there. A value that ends
/// with `;` keeps ending with `;` (the entry goes before that end); one that does not gets `;`
/// and the entry; an empty or absent one becomes the entry alone.
pub fn with_entry(value: Option<&str>, dir: &Path) -> Option<String> {
    if has_entry(value, dir) {
        return None;
    }
    let dir = dir.to_string_lossy().into_owned();
    let value = value.unwrap_or_default();
    if value.is_empty() {
        return Some(dir);
    }
    let mut entries: Vec<&str> = value.split(';').collect();
    let ends_with_separator = entries.last().is_some_and(|last| last.is_empty());
    if ends_with_separator {
        entries.pop();
        entries.push(&dir);
        entries.push("");
    } else {
        entries.push(&dir);
    }
    Some(entries.join(";"))
}

/// The value with every entry that is `dir` taken out, each with one `;`, or `None` when none is
/// there. `Some("")` means nothing is left: the caller deletes the value.
pub fn without_entry(value: &str, dir: &Path) -> Option<String> {
    let dir = dir.to_string_lossy();
    let entries: Vec<&str> = value.split(';').collect();
    if !entries.iter().any(|e| same_entry(e, &dir)) {
        return None;
    }
    let kept: Vec<&str> = entries
        .into_iter()
        .filter(|e| !same_entry(e, &dir))
        .collect();
    Some(kept.join(";"))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathChange {
    Added,
    AlreadyThere,
    Removed,
    NotThere,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PathError {
    /// The value is there but is not plain text (registry type `kind`): it is left alone.
    #[error("the Path value of HKEY_CURRENT_USER\\Environment is not plain text (registry type {kind}), so CMD Studio did not change it")]
    NotText { kind: u32 },
    #[error("{0}")]
    Read(AdapterError),
    #[error("{0}")]
    Write(AdapterError),
}

impl PathError {
    /// Stable code (spec §5.9): a registry that cannot be read is `E_MACHINE_READ`; a value that
    /// cannot be edited or written is `E_PATH_WRITE`.
    pub fn code(&self) -> &'static str {
        match self {
            PathError::Read(_) => "E_MACHINE_READ",
            PathError::NotText { .. } | PathError::Write(_) => "E_PATH_WRITE",
        }
    }
}

/// The value's text, or the error for a value that is not text.
fn current(user_path: &dyn UserPath) -> Result<(Option<RegValue>, RegText), PathError> {
    let value = user_path.read().map_err(PathError::Read)?;
    let text = RegText::of(value.as_ref());
    if let (RegText::NotText, Some(v)) = (&text, &value) {
        return Err(PathError::NotText { kind: v.kind });
    }
    Ok((value, text))
}

/// Puts `dir` on the user's Path, keeping the value's kind (a missing value is made as
/// `REG_EXPAND_SZ`), and broadcasts the change when something was written.
pub fn add(user_path: &dyn UserPath, dir: &Path) -> Result<PathChange, PathError> {
    let (value, text) = current(user_path)?;
    let (kind, old) = match (&value, &text) {
        (Some(v), RegText::Text(t)) => (v.kind, Some(t.as_str())),
        _ => (REG_EXPAND_SZ, None),
    };
    let Some(new) = with_entry(old, dir) else {
        return Ok(PathChange::AlreadyThere);
    };
    user_path
        .write(&RegValue::from_text(kind, &new))
        .map_err(PathError::Write)?;
    user_path.broadcast_change();
    Ok(PathChange::Added)
}

/// Takes `dir` off the user's Path; a value left empty is deleted. Broadcasts when something
/// was written or deleted.
pub fn remove(user_path: &dyn UserPath, dir: &Path) -> Result<PathChange, PathError> {
    let (value, text) = current(user_path)?;
    let (Some(v), RegText::Text(old)) = (&value, &text) else {
        return Ok(PathChange::NotThere);
    };
    let Some(new) = without_entry(old, dir) else {
        return Ok(PathChange::NotThere);
    };
    if new.is_empty() {
        user_path.delete().map_err(PathError::Write)?;
    } else {
        user_path
            .write(&RegValue::from_text(v.kind, &new))
            .map_err(PathError::Write)?;
    }
    user_path.broadcast_change();
    Ok(PathChange::Removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::fake::FakeUserPath;
    use crate::adapters::REG_SZ;
    use std::path::PathBuf;

    const DIR: &str = r"C:\Users\tester\AppData\Local\CMD Studio";
    const APPS: &str = r"%USERPROFILE%\AppData\Local\Microsoft\WindowsApps";

    fn dir() -> PathBuf {
        PathBuf::from(DIR)
    }

    /// Review Focus 1: a fresh Windows 11 user Path ends with `;`. Adding and removing gives the
    /// old text back exactly, and so for a value without the trailing `;`, an empty one, and
    /// one where the user appended something after ours.
    #[test]
    fn a_path_that_ends_with_a_separator_comes_back_byte_equal() {
        for (before, after_add) in [
            (format!("{APPS};"), format!("{APPS};{DIR};")),
            (APPS.to_string(), format!("{APPS};{DIR}")),
            (String::new(), DIR.to_string()),
            (";".to_string(), format!(";{DIR};")),
        ] {
            assert_eq!(
                with_entry(Some(&before), &dir()).as_deref(),
                Some(after_add.as_str()),
                "{before:?}"
            );
            assert_eq!(
                without_entry(&after_add, &dir()).as_deref(),
                Some(before.as_str()),
                "{after_add:?}"
            );
        }
        assert_eq!(with_entry(None, &dir()).as_deref(), Some(DIR));
        // The user added an entry after ours: ours goes, theirs stays.
        let later = format!("{APPS};{DIR};C:\\theirs");
        assert_eq!(
            without_entry(&later, &dir()).as_deref(),
            Some(&*format!("{APPS};C:\\theirs"))
        );
    }

    /// Case and a trailing backslash do not make a different entry; nothing else is touched,
    /// not even an empty entry (`;;`) or a quoted one.
    #[test]
    fn the_entry_is_found_without_case_and_trailing_backslash_and_nothing_else_moves() {
        let value = format!("C:\\a;;\"C:\\q;r\";{};C:\\b\\", DIR.to_uppercase() + "\\");
        assert!(has_entry(Some(&value), &dir()));
        assert_eq!(
            with_entry(Some(&value), &dir()),
            None,
            "already there: no change"
        );
        assert_eq!(
            without_entry(&value, &dir()).as_deref(),
            Some("C:\\a;;\"C:\\q;r\";C:\\b\\")
        );
        assert_eq!(
            without_entry("C:\\a;C:\\b", &dir()),
            None,
            "not there: no change"
        );
        assert!(!has_entry(None, &dir()));
        assert!(!has_entry(
            Some("C:\\Users\\tester\\AppData\\Local\\CMD Studio 2"),
            &dir()
        ));
    }

    /// Review Focus 1: a Path far longer than NSIS's 1024-character strings is kept whole.
    #[test]
    fn a_long_path_is_kept_whole() {
        let long: String = (0..200)
            .map(|i| format!("C:\\tools\\entry-{i:03};"))
            .collect();
        assert!(long.len() > 3000);
        let added = with_entry(Some(&long), &dir()).unwrap();
        assert_eq!(added, format!("{long}{DIR};"));
        assert_eq!(without_entry(&added, &dir()).unwrap(), long);
    }

    /// Review Focus 2: the kind of the value is kept (`REG_SZ` stays `REG_SZ`); a missing value
    /// is made as `REG_EXPAND_SZ`; a value that empties is deleted; the broadcast follows every
    /// write and no non-write.
    #[test]
    fn the_kind_of_the_value_is_kept() {
        let p = FakeUserPath::new();
        assert_eq!(add(&p, &dir()).unwrap(), PathChange::Added);
        assert_eq!(p.value(), Some(RegValue::from_text(REG_EXPAND_SZ, DIR)));
        assert_eq!(p.broadcasts(), 1);
        assert_eq!(add(&p, &dir()).unwrap(), PathChange::AlreadyThere);
        assert_eq!(p.broadcasts(), 1, "nothing written, nothing broadcast");
        assert_eq!(remove(&p, &dir()).unwrap(), PathChange::Removed);
        assert_eq!(
            p.value(),
            None,
            "an empty Path is deleted, as it was absent before"
        );
        assert_eq!(p.broadcasts(), 2);
        assert_eq!(remove(&p, &dir()).unwrap(), PathChange::NotThere);
        assert_eq!(p.broadcasts(), 2);

        p.set(Some(RegValue::from_text(REG_SZ, "C:\\a")));
        assert_eq!(add(&p, &dir()).unwrap(), PathChange::Added);
        assert_eq!(
            p.value(),
            Some(RegValue::from_text(REG_SZ, &format!("C:\\a;{DIR}")))
        );
        assert_eq!(remove(&p, &dir()).unwrap(), PathChange::Removed);
        assert_eq!(p.value(), Some(RegValue::from_text(REG_SZ, "C:\\a")));
    }

    /// Review Focus 2: a value that is not plain text (a `REG_BINARY`, or text with bytes after
    /// its NUL) is left alone and named, never read as absent and never written over.
    #[test]
    fn a_value_that_is_not_text_is_left_and_named() {
        let p = FakeUserPath::new();
        p.set(Some(RegValue {
            kind: 3,
            bytes: vec![1, 2, 3],
        }));
        for change in [add(&p, &dir()), remove(&p, &dir())] {
            match change {
                Err(PathError::NotText { kind }) => assert_eq!(kind, 3),
                other => panic!("{other:?}"),
            }
        }
        assert_eq!(
            p.value(),
            Some(RegValue {
                kind: 3,
                bytes: vec![1, 2, 3]
            })
        );
        assert_eq!(p.broadcasts(), 0);
        assert_eq!(PathError::NotText { kind: 3 }.code(), "E_PATH_WRITE");
    }

    /// A registry that cannot be read is `E_MACHINE_READ`; one that cannot be written is
    /// `E_PATH_WRITE`, and the broadcast is not sent for a write that failed.
    #[test]
    fn read_and_write_failures_have_their_codes() {
        let p = FakeUserPath::new();
        p.fail_reads.set(true);
        assert_eq!(add(&p, &dir()).unwrap_err().code(), "E_MACHINE_READ");
        p.fail_reads.set(false);
        p.fail_writes.set(true);
        let e = add(&p, &dir()).unwrap_err();
        assert_eq!(e.code(), "E_PATH_WRITE");
        assert!(e.to_string().contains("access denied (injected)"), "{e}");
        assert_eq!(p.broadcasts(), 0);
    }
}
