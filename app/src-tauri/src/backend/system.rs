//! `system_detect`: what the wizard's machine check (spec §4.2 page 2), the home page's status
//! bar and the editor's font list need to know about this computer, from the core's `detect`
//! and `plan::autorun_summary`. It changes nothing on the machine, but may create the data
//! folder: `detect` creates it when it is missing, because a short path can be asked of a folder
//! that exists only.
use super::{versions, Backend, Machine, Versions};
use crate::error::AppError;
use cmdstudio_core::adapters::{Arch, WtVariant};
use cmdstudio_core::detect::detect;
use cmdstudio_core::plan::{autorun_summary, NERD_FONT_FAMILY};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Windows 10 2004, the oldest Windows that Windows Terminal supports (G18).
pub const WT_MIN_BUILD: u32 = 19041;

/// Font families the editor offers when they are installed (spec §3.4: monospace fonts on the
/// machine, and the bundled Nerd Font). GDI does not tell the app which families are
/// monospace, so this is the list of common ones; the editor also takes any family name typed.
pub const KNOWN_MONOSPACE: &[&str] = &[
    "CaskaydiaCove NF",
    "Cascadia Mono",
    "Cascadia Code",
    "Consolas",
    "Courier New",
    "Lucida Console",
    "Fira Code",
    "FiraCode Nerd Font",
    "JetBrains Mono",
    "JetBrainsMono Nerd Font",
    "Source Code Pro",
    "SauceCodePro Nerd Font",
    "Hack",
    "Hack Nerd Font",
    "Iosevka",
    "IBM Plex Mono",
    "Roboto Mono",
    "Ubuntu Mono",
    "DejaVu Sans Mono",
    "Noto Sans Mono",
    "Inconsolata",
    "Victor Mono",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum WtKind {
    Store,
    Preview,
    Unpackaged,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct WtInfo {
    pub kind: WtKind,
    /// Its folder exists.
    pub installed: bool,
    /// Its settings.json exists (one that was never opened has none, D6).
    pub has_settings: bool,
}

/// What the user's AutoRun holds, from `plan::autorun_summary`. `other_commands` are the user's
/// own commands as written: the UI uses their count and `foreign_clink` (to ask before another
/// Clink is replaced), and they are written to no log.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AutoRunInfo {
    pub other_commands: Vec<String>,
    pub foreign_clink: Vec<String>,
    pub ours_present: bool,
    pub machine_wide_clink: bool,
    pub not_text: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FontInfo {
    /// The bundled Nerd Font (`CaskaydiaCove NF`) is installed for this account.
    pub nerd_font_installed: bool,
    /// Installed families from [`KNOWN_MONOSPACE`], in that order.
    pub monospace: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SystemReport {
    /// `CurrentBuildNumber`; absent when it could not be read (see `unreadable`).
    pub build: Option<u32>,
    pub arch: String,
    pub packaged: bool,
    /// The facts of the registry that could not be read, as the host reported them; each is
    /// absent from the report above, never guessed.
    pub unreadable: Vec<String>,
    pub windows_terminal: Vec<WtInfo>,
    /// Any Windows Terminal is installed.
    pub wt_installed: bool,
    /// The Store page of Windows Terminal applies: the build is known and at least 19041.
    pub store_available: bool,
    pub autorun: AutoRunInfo,
    pub fonts: FontInfo,
    pub data_dir: String,
    pub versions: Versions,
}

impl<M: Machine> Backend<M> {
    /// The machine report. Changes nothing but may create the data folder (see the module's
    /// note). A registry fact the host cannot read is listed in `unreadable`; but AutoRun and
    /// the font list are what the planner reads too, and it would fail on the same read, so when
    /// either cannot be read this fails with the code of the read (`E_MACHINE_READ`; the data
    /// folder that cannot be created is `E_DATA_WRITE`) instead of showing a made-up answer.
    pub fn system_detect(&self) -> Result<SystemReport, AppError> {
        let sys = self.sys();
        let detected = detect(sys, self.paths())?;
        let autorun = autorun_summary(&detected.machine, self.paths());
        let windows_terminal: Vec<WtInfo> = detected
            .wt
            .iter()
            .map(|l| WtInfo {
                kind: match l.variant {
                    WtVariant::Store => WtKind::Store,
                    WtVariant::Preview => WtKind::Preview,
                    WtVariant::Unpackaged => WtKind::Unpackaged,
                },
                installed: l.installed,
                has_settings: sys.fs.exists(&l.settings),
            })
            .collect();
        let families = &detected.machine.fonts;
        let has = |family: &str| families.iter().any(|f| f.eq_ignore_ascii_case(family));
        let monospace = KNOWN_MONOSPACE
            .iter()
            .filter(|f| has(f))
            .map(|f| f.to_string())
            .collect();
        Ok(SystemReport {
            build: detected.info.build,
            arch: match detected.info.arch {
                Arch::X64 => "x64",
                Arch::Arm64 => "arm64",
                Arch::Other => "other",
            }
            .to_string(),
            packaged: detected.info.packaged,
            unreadable: detected.info.unreadable.clone(),
            wt_installed: windows_terminal.iter().any(|w| w.installed),
            store_available: detected.info.build.is_some_and(|b| b >= WT_MIN_BUILD),
            windows_terminal,
            autorun: AutoRunInfo {
                other_commands: autorun.other_commands,
                foreign_clink: autorun.foreign_clink,
                ours_present: autorun.ours_present,
                machine_wide_clink: autorun.machine_wide_clink,
                not_text: autorun.not_text,
            },
            fonts: FontInfo {
                nerd_font_installed: has(NERD_FONT_FAMILY),
                monospace,
            },
            data_dir: self.paths().data.display().to_string(),
            versions: versions(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testing::{backend, DATA};
    use cmdstudio_core::adapters::fake::FakeMachine;
    use cmdstudio_core::adapters::{AutoRun, RegValue, REG_SZ};

    #[test]
    fn the_report_reads_the_machine_the_planner_reads() {
        let b = backend();
        let r = b.system_detect().unwrap();
        assert_eq!(
            (r.build, r.arch.as_str(), r.packaged),
            (Some(26100), "x64", false)
        );
        assert!(r.unreadable.is_empty());
        assert_eq!(r.windows_terminal.len(), 1);
        assert_eq!(
            r.windows_terminal[0],
            WtInfo {
                kind: WtKind::Store,
                installed: true,
                has_settings: true
            }
        );
        assert!(r.wt_installed && r.store_available);
        assert_eq!(r.autorun, AutoRunInfo::default_for_test());
        assert_eq!(r.fonts.monospace, ["Cascadia Mono", "Consolas"]);
        assert!(!r.fonts.nerd_font_installed);
        assert_eq!(r.data_dir, DATA);
        assert_eq!(r.versions.oh_my_posh, "31.5.0");
    }

    impl AutoRunInfo {
        fn default_for_test() -> Self {
            AutoRunInfo {
                other_commands: Vec::new(),
                foreign_clink: Vec::new(),
                ours_present: false,
                machine_wide_clink: false,
                not_text: false,
            }
        }
    }

    #[test]
    fn another_programs_autorun_and_another_clink_are_told_apart() {
        let b = backend();
        let value = r#"if exist "C:\conda\conda_hook.bat" "C:\conda\conda_hook.bat" & "C:\clink\clink_x64.exe" inject --autorun"#;
        let m: &FakeMachine = &b.machine;
        m.autorun.set_machine(None);
        m.autorun
            .write_user(&RegValue::from_text(REG_SZ, value))
            .unwrap();
        let r = b.system_detect().unwrap();
        assert_eq!(
            r.autorun.other_commands,
            [r#"if exist "C:\conda\conda_hook.bat" "C:\conda\conda_hook.bat""#]
        );
        assert_eq!(
            r.autorun.foreign_clink,
            [r#""C:\clink\clink_x64.exe" inject --autorun"#]
        );
        assert!(!r.autorun.ours_present && !r.autorun.machine_wide_clink);
    }

    #[test]
    fn an_old_windows_without_terminal_gets_no_store_link() {
        let mut b = backend();
        b.machine.host.info.build = Some(18363);
        b.machine.wt.locations = vec![];
        let r = b.system_detect().unwrap();
        assert!(!r.wt_installed && !r.store_available);
        b.machine.host.info.build = None;
        b.machine.host.info.unreadable = vec!["build: access denied".to_string()];
        let r = b.system_detect().unwrap();
        assert!(r.build.is_none() && !r.store_available);
        assert_eq!(r.unreadable, ["build: access denied"]);
    }

    /// The planner would fail on the same read, so the report fails too, with the code of the
    /// read (spec §5.9), rather than show an AutoRun or a font list it made up.
    #[test]
    fn an_autorun_or_a_font_list_that_cannot_be_read_fails_with_its_code() {
        let b = backend();
        b.machine.autorun.fail_reads();
        assert_eq!(b.system_detect().unwrap_err().code, "E_MACHINE_READ");
        let b = backend();
        b.machine.fonts.fail_families();
        assert_eq!(b.system_detect().unwrap_err().code, "E_MACHINE_READ");
    }

    /// The user's own AutoRun commands go to the UI, which counts them; the log of a command is
    /// its name, how long it took and its outcome, nothing the command read.
    #[test]
    fn the_users_autorun_commands_are_not_written_to_the_log() {
        let b = backend();
        let mine = r"C:\Users\tester\tools\sync.cmd --profile work";
        b.machine
            .autorun
            .write_user(&RegValue::from_text(REG_SZ, mine))
            .unwrap();
        let r = b.logged("system_detect", |b| b.system_detect()).unwrap();
        assert_eq!(r.autorun.other_commands, [mine]);
        b.machine.autorun.fail_reads();
        assert!(b.logged("system_detect", |b| b.system_detect()).is_err());
        let log = b.log_tail(10);
        let lines: Vec<&str> = log.lines().collect();
        assert_eq!(lines.len(), 2, "{log}");
        assert!(
            lines[0].contains(" cmd=system_detect ms=") && lines[0].ends_with(" ok"),
            "{log}"
        );
        assert!(
            lines[1].contains(" cmd=system_detect ms=")
                && lines[1].ends_with(" err=E_MACHINE_READ"),
            "{log}"
        );
        assert!(
            !log.contains("sync.cmd") && !log.contains("work") && !log.contains("tester"),
            "{log}"
        );
    }
}
