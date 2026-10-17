//! Reads the machine facts the planner needs (spec §5.2 `detect`). Read only,
//! except that it creates the data folder, because `GetShortPathNameW` works
//! only on a path that exists (spec §5.4 step 2).
use crate::adapters::{
    program_data_dir, AdapterError, DirState, RegText, RegValue, System, SystemInfo, WtLocation,
};
use crate::plan::{Machine, Paths};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Detected {
    pub machine: Machine,
    /// Passed through as the host reported it, including `unreadable`.
    pub info: SystemInfo,
    /// Every Windows Terminal variant, installed or not, with where its settings live.
    pub wt: Vec<WtLocation>,
    /// `%ProgramData%\ai.resincore.cmdstudio-<SID>` and what detect found there.
    pub program_data: (PathBuf, DirState),
    pub user_sid: String,
}

/// Why `detect` stopped. A read never carries a write code (spec §5.9).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DetectError {
    /// Reading this computer's settings failed (short path, SID, ProgramData state, Windows
    /// Terminal, AutoRun, fonts).
    #[error("could not read this computer's settings: {0}")]
    Read(AdapterError),
    /// Creating CMD Studio's data folder failed.
    #[error("could not create CMD Studio's data folder: {0}")]
    Write(AdapterError),
}

impl DetectError {
    /// Stable code shown to users (spec §5.9).
    pub fn code(&self) -> &'static str {
        match self {
            DetectError::Read(_) => "E_MACHINE_READ",
            DetectError::Write(_) => "E_DATA_WRITE",
        }
    }
}

/// Creating the data folder is the only write; every other step reads.
pub fn detect(sys: System<'_>, paths: &Paths) -> Result<Detected, DetectError> {
    sys.fs
        .create_dir_all(&paths.data)
        .map_err(DetectError::Write)?;
    let data_short = sys
        .host
        .short_path(&paths.data)
        .map_err(DetectError::Read)?;
    let sid = sys.host.user_sid().map_err(DetectError::Read)?;
    let pd = program_data_dir(&sys.host.program_data().map_err(DetectError::Read)?, &sid);
    let pd_state = sys.host.private_dir_state(&pd).map_err(DetectError::Read)?;
    let wt = sys.wt.locations();
    let wt_settings = wt
        .iter()
        .filter(|l| sys.fs.exists(&l.settings))
        .map(|l| l.settings.clone())
        .collect();
    // An installed Windows Terminal that was never opened has no settings.json yet: the apply
    // makes one (founder 2026-10-17, spec §5.4).
    let wt_create = wt
        .iter()
        .filter(|l| l.installed && !sys.fs.exists(&l.settings))
        .map(|l| l.settings.clone())
        .collect();
    // Raw text for the planner. A value that is there but is not plain text is told apart from
    // an absent one: the planner never edits it (spec §5.4).
    let text = |v: Option<RegValue>| RegText::of(v.as_ref());
    let machine = Machine {
        wt_settings,
        wt_create,
        autorun_hkcu: text(sys.autorun.read_user().map_err(DetectError::Read)?),
        autorun_hklm: text(sys.autorun.read_machine().map_err(DetectError::Read)?),
        fonts: sys.fonts.families().map_err(DetectError::Read)?,
        data_short: Some(data_short),
        // Usable when absent (created at install) or private (spec §5.4).
        program_data_dir: matches!(pd_state, DirState::Absent | DirState::Private)
            .then(|| pd.clone()),
    };
    Ok(Detected {
        machine,
        info: sys.host.system_info(),
        wt,
        program_data: (pd, pd_state),
        user_sid: sid,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::fake::FakeMachine;
    use crate::adapters::{AutoRun, Fs, RegValue, WtVariant, REG_EXPAND_SZ, REG_SZ};
    use std::path::Path;

    const DATA: &str = r"C:\Users\ผู้ทดสอบ ไทย\AppData\Local\ai.resincore.cmdstudio";
    const SHORT: &str = r"C:\Users\3607~1\AppData\Local\AIRESI~1.CMD";
    const PD: &str = r"C:\ProgramData\ai.resincore.cmdstudio-S-1-5-21-1-2-3-1001";

    fn machine() -> FakeMachine {
        let mut m = FakeMachine::new();
        m.host
            .short
            .insert(PathBuf::from(DATA), PathBuf::from(SHORT));
        let store = PathBuf::from(
            r"C:\L\Packages\Microsoft.WindowsTerminal_8wekyb3d8bbwe\LocalState\settings.json",
        );
        let preview = PathBuf::from(
            r"C:\L\Packages\Microsoft.WindowsTerminalPreview_8wekyb3d8bbwe\LocalState\settings.json",
        );
        m.fs.put(&store, b"{}");
        m.wt.locations = vec![
            WtLocation {
                variant: WtVariant::Store,
                installed: true,
                settings: store,
            },
            WtLocation {
                variant: WtVariant::Preview,
                installed: true,
                settings: preview,
            },
        ];
        m
    }

    fn paths() -> Paths {
        Paths {
            data: PathBuf::from(DATA),
        }
    }

    #[test]
    fn fills_the_planner_facts() {
        let m = machine();
        m.autorun
            .write_user(&RegValue::from_text(REG_EXPAND_SZ, r"%USERPROFILE%\x.cmd"))
            .unwrap();
        m.autorun
            .set_machine(Some(RegValue::from_text(REG_SZ, "clink inject")));
        let d = detect(m.system(), &paths()).unwrap();
        assert!(m.fs.is_dir(Path::new(DATA)));
        assert_eq!(d.machine.data_short, Some(PathBuf::from(SHORT)));
        assert_eq!(
            d.machine.wt_settings.len(),
            1,
            "only settings files that exist"
        );
        assert_eq!(
            d.machine.autorun_hkcu,
            RegText::Text(r"%USERPROFILE%\x.cmd".into()),
            "unexpanded text"
        );
        assert_eq!(d.machine.autorun_hklm, RegText::Text("clink inject".into()));
        assert_eq!(
            d.machine.fonts,
            vec!["Arial".to_string(), "Cascadia Mono".to_string()]
        );
        let pd = PathBuf::from(PD);
        assert_eq!(d.machine.program_data_dir, Some(pd.clone()));
        assert_eq!(d.program_data, (pd, DirState::Absent));
        assert_eq!(d.user_sid, "S-1-5-21-1-2-3-1001");
        assert_eq!(d.wt, m.wt.locations, "every variant, installed or not");
        assert_eq!(d.info, m.host.info);
    }

    #[test]
    fn an_installed_terminal_without_settings_is_one_to_set_up() {
        // Founder 2026-10-17: a Windows Terminal that is installed but was never opened has no
        // settings.json yet, and an apply sets it up. One that is not installed is not.
        let mut m = machine();
        let unpackaged = PathBuf::from(r"C:\L\Microsoft\Windows Terminal\settings.json");
        m.wt.locations.push(WtLocation {
            variant: WtVariant::Unpackaged,
            installed: false,
            settings: unpackaged.clone(),
        });
        let [store, preview, _] =
            m.wt.locations
                .iter()
                .map(|l| l.settings.clone())
                .collect::<Vec<_>>()
                .try_into()
                .unwrap();
        let d = detect(m.system(), &paths()).unwrap();
        assert_eq!(d.machine.wt_settings, vec![store.clone()]);
        assert_eq!(d.machine.wt_create, vec![preview.clone()]);
        // Both lists keep the order of the variants.
        m.fs.remove_file(&store).unwrap();
        let d = detect(m.system(), &paths()).unwrap();
        assert!(d.machine.wt_settings.is_empty());
        assert_eq!(d.machine.wt_create, vec![store, preview.clone()]);
        // Once the file is there, it is one to edit, never one to make.
        m.fs.put(&preview, b"{}");
        let d = detect(m.system(), &paths()).unwrap();
        assert_eq!(d.machine.wt_settings, vec![preview]);
        assert!(!d.machine.wt_create.contains(&unpackaged));
    }

    #[test]
    fn a_program_data_folder_someone_else_controls_is_not_offered() {
        let m = machine();
        let pd = PathBuf::from(PD);
        let unsafe_state = DirState::Unsafe("S-1-5-32-545 may change it".into());
        m.host.set_dir_state(&pd, unsafe_state.clone());
        let d = detect(m.system(), &paths()).unwrap();
        assert_eq!(d.machine.program_data_dir, None);
        assert_eq!(
            d.program_data,
            (pd.clone(), unsafe_state),
            "the state is still reported, for the wizard and doctor"
        );
        m.host.set_dir_state(&pd, DirState::Private);
        let d = detect(m.system(), &paths()).unwrap();
        assert_eq!(d.machine.program_data_dir, Some(pd));
    }

    #[test]
    fn an_autorun_that_is_not_text_is_never_taken_for_an_absent_one() {
        for value in crate::adapters::fake::not_text_values() {
            let m = machine();
            m.autorun.write_user(&value).unwrap();
            let d = detect(m.system(), &paths()).unwrap();
            assert_eq!(d.machine.autorun_hkcu, RegText::NotText, "{value:?}");
            assert_eq!(d.machine.autorun_hklm, RegText::Absent, "{value:?}");
        }
    }

    #[test]
    fn a_machine_autorun_that_is_not_text_is_told_apart_too() {
        for value in crate::adapters::fake::not_text_values() {
            let m = machine();
            m.autorun.set_machine(Some(value.clone()));
            let d = detect(m.system(), &paths()).unwrap();
            assert_eq!(d.machine.autorun_hklm, RegText::NotText, "{value:?}");
            assert_eq!(d.machine.autorun_hkcu, RegText::Absent, "an absent value");
        }
    }

    #[test]
    fn a_failed_data_folder_is_a_write_failure() {
        let m = machine();
        // A file where the folder belongs: creating the folder fails.
        m.fs.put(DATA, b"not a folder");
        let e = detect(m.system(), &paths()).unwrap_err();
        assert!(matches!(e, DetectError::Write(_)), "{e:?}");
        assert_eq!(e.code(), "E_DATA_WRITE");
        assert!(
            e.to_string()
                .starts_with("could not create CMD Studio's data folder: "),
            "{e}"
        );
    }

    #[test]
    fn a_failed_read_is_a_read_failure_never_a_write_code() {
        let m = machine();
        m.fonts.fail_families();
        let e = detect(m.system(), &paths()).unwrap_err();
        assert!(matches!(e, DetectError::Read(_)), "{e:?}");
        assert_eq!(e.code(), "E_MACHINE_READ");
        let text = e.to_string();
        assert!(
            text.starts_with("could not read this computer's settings: "),
            "{text}"
        );
        assert!(text.contains("EnumFontFamiliesExW"), "{text}");
    }

    #[test]
    fn creating_the_data_folder_is_the_only_write() {
        let m = machine();
        m.autorun
            .write_user(&RegValue::from_text(REG_SZ, "echo hi"))
            .unwrap();
        let files = m.fs.snapshot().0;
        let autorun = m.autorun.snapshot();
        let fonts = m.fonts.snapshot();
        detect(m.system(), &paths()).unwrap();
        assert_eq!(m.fs.snapshot().0, files, "no file is written");
        assert!(m.fs.is_dir(Path::new(DATA)), "the data folder is made");
        assert_eq!(m.autorun.snapshot(), autorun, "AutoRun is untouched");
        assert_eq!(m.fonts.snapshot(), fonts, "fonts are untouched");
        assert_eq!(
            m.host.dir_state(Path::new(PD)),
            DirState::Absent,
            "the ProgramData folder is only looked at, not made"
        );
    }

    #[test]
    fn an_existing_data_folder_and_its_files_are_left_alone() {
        let m = machine();
        let state = Path::new(DATA).join("state.json");
        m.fs.put(&state, b"{\"version\":1}");
        detect(m.system(), &paths()).unwrap();
        detect(m.system(), &paths()).unwrap();
        assert_eq!(m.fs.file(&state), Some(b"{\"version\":1}".to_vec()));
    }

    #[test]
    fn registry_facts_that_could_not_be_read_are_passed_on() {
        let mut m = machine();
        m.host.info.build = None;
        m.host.info.unreadable = vec!["build: access denied".to_string()];
        let d = detect(m.system(), &paths()).unwrap();
        assert_eq!(d.info, m.host.info);
        assert_eq!(d.info.unreadable, vec!["build: access denied".to_string()]);
    }
}
