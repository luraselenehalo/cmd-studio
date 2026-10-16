//! `system_detect`: what the wizard's machine check (spec §4.2 page 2), the home page's status
//! bar and the editor's font list need to know about this computer, from the core's `detect`
//! and `plan::autorun_summary`. It changes nothing on the machine, but may create the data
//! folder: `detect` creates it when it is missing, because a short path can be asked of a folder
//! that exists only.
use super::preview::SampleIdentity;
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
/// machine, and the bundled Nerd Font). The core's font list (`Fonts::families`) carries the
/// family names only, no pitch, so the app cannot tell which of them are monospace; this is the
/// list of common ones. The editor also takes any family name typed.
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
    /// The registry facts the host could not read, one entry each as it reported them
    /// (`build`, `default terminal`, `WebView2`). `build` is a field of this report and is
    /// absent when it is listed here, never guessed; the default terminal and WebView2 are not
    /// fields of this report, so their entries only say that the fact is unknown.
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
    /// The user, computer and home folder the preview's sample starts from (spec §5.6): the UI
    /// cannot read the environment, so it sends these back as the `PreviewContext`'s `user`,
    /// `host` and `home`. The user name is the user's own and stays between the backend and the
    /// UI: it is never logged, and the diagnostics redact it.
    pub preview_sample: SampleIdentity,
}

impl<M: Machine> Backend<M> {
    /// The machine report. Changes nothing but may create the data folder (see the module's
    /// note). A registry fact the host cannot read is listed in `unreadable`; but `detect` reads
    /// more than that, and the planner reads the same and would fail on the same read. So when
    /// `detect` cannot read the data folder's short path, the user's SID, the state of the
    /// private `%ProgramData%` folder, AutoRun or the font list, this fails with
    /// `E_MACHINE_READ` instead of showing a made-up answer. A data folder that cannot be
    /// created is `E_DATA_WRITE`.
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
        // Both names trimmed, as the planner's `installed_font` does, so the report and the plan
        // agree on which family is installed.
        let has = |family: &str| {
            let family = family.trim();
            families
                .iter()
                .any(|f| f.trim().eq_ignore_ascii_case(family))
        };
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
            preview_sample: self.sample.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testing::{backend, DATA};
    use cmdstudio_core::adapters::fake::{FakeFonts, FakeMachine};
    use cmdstudio_core::adapters::{AutoRun, RegValue, WtLocation, REG_SZ};
    use cmdstudio_core::plan::{autorun_entry, ClinkLaunch};
    use std::path::PathBuf;

    #[test]
    fn the_report_carries_the_sample_identity_the_backend_was_built_with() {
        // The UI cannot read the environment, so the preview's user, host and home reach it
        // here (spec §5.6); the backend's own, not the core's sample.
        let report = backend().system_detect().unwrap();
        assert_eq!(
            report.preview_sample,
            crate::backend::testing::sample_identity()
        );
        assert_ne!(
            report.preview_sample,
            SampleIdentity::from_env(|_| None),
            "the test's identity must differ from the core's sample to prove the source"
        );
        let json = serde_json::to_value(&report).unwrap();
        assert_eq!(json["previewSample"]["user"], "tester");
        assert_eq!(json["previewSample"]["host"], "TESTPC");
        assert_eq!(json["previewSample"]["home"], r"C:\Users\tester");
    }

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

    /// The Store page applies from Windows 10 2004 (build 19041) and only when the build is
    /// known; whether a Windows Terminal is installed is a separate answer.
    #[test]
    fn the_store_link_starts_at_build_19041() {
        // (build, a Windows Terminal is installed, store_available, wt_installed)
        let table: [(Option<u32>, bool, bool, bool); 8] = [
            (Some(18363), false, false, false),
            (Some(19040), false, false, false),
            (Some(19040), true, false, true),
            (Some(19041), true, true, true),
            (Some(19041), false, true, false),
            (Some(26100), false, true, false),
            (None, true, false, true),
            (None, false, false, false),
        ];
        for (build, installed, store_available, wt_installed) in table {
            let mut b = backend();
            b.machine.host.info.build = build;
            if !installed {
                b.machine.wt.locations = vec![];
            }
            let r = b.system_detect().unwrap();
            assert_eq!(
                (r.build, r.store_available, r.wt_installed),
                (build, store_available, wt_installed),
                "build {build:?}, Windows Terminal installed: {installed}"
            );
        }
        // A Windows Terminal folder that is there without being installed is not "installed".
        let mut b = backend();
        b.machine.host.info.build = Some(22631);
        b.machine.wt.locations = vec![WtLocation {
            variant: WtVariant::Preview,
            installed: false,
            settings: PathBuf::from(r"C:\Users\tester\no\settings.json"),
        }];
        let r = b.system_detect().unwrap();
        assert!(!r.wt_installed && r.store_available);
        assert_eq!(
            r.windows_terminal,
            [WtInfo {
                kind: WtKind::Preview,
                installed: false,
                has_settings: false
            }]
        );
    }

    /// Our own entry is not "another program's command" and not "another Clink", though it starts
    /// a Clink; the machine-wide value only says whether a Clink starts from it; a value that is
    /// not plain text is told, and nothing in it is guessed.
    #[test]
    fn our_own_entry_is_told_from_the_others_and_a_value_that_is_not_text_is_told() {
        let b = backend();
        let launch = ClinkLaunch::Direct(b.paths().clink_dir().join("clink_x64.exe"));
        let ours = autorun_entry(&launch, b.paths()).unwrap();
        let other = r#""C:\Tools\sync.cmd" --quiet"#;
        let foreign = r#""C:\clink\clink_x64.exe" inject --autorun"#;
        let machine_wide = r#""C:\Program Files\clink\clink_x64.exe" inject --autorun --quiet"#;
        b.machine
            .autorun
            .set_machine(Some(RegValue::from_text(REG_SZ, machine_wide)));
        b.machine
            .autorun
            .write_user(&RegValue::from_text(
                REG_SZ,
                &format!("{ours} & {other} & {foreign}"),
            ))
            .unwrap();
        let r = b.system_detect().unwrap();
        assert!(r.autorun.ours_present);
        assert!(r.autorun.machine_wide_clink);
        assert!(!r.autorun.not_text);
        assert_eq!(r.autorun.other_commands, [other]);
        assert_eq!(r.autorun.foreign_clink, [foreign]);
        assert!(!r.autorun.other_commands.contains(&ours));
        assert!(!r.autorun.foreign_clink.contains(&ours));
        // The same entry on its own: told as ours, in neither list.
        b.machine
            .autorun
            .write_user(&RegValue::from_text(REG_SZ, &ours))
            .unwrap();
        let r = b.system_detect().unwrap();
        assert!(r.autorun.ours_present);
        assert!(r.autorun.other_commands.is_empty() && r.autorun.foreign_clink.is_empty());
        // A REG_DWORD where text should be: told as not text, and read as nothing else.
        b.machine
            .autorun
            .write_user(&RegValue {
                kind: 4,
                bytes: vec![1, 0, 0, 0],
            })
            .unwrap();
        let r = b.system_detect().unwrap();
        assert_eq!(
            r.autorun,
            AutoRunInfo {
                other_commands: Vec::new(),
                foreign_clink: Vec::new(),
                ours_present: false,
                machine_wide_clink: true,
                not_text: true,
            }
        );
    }

    /// The planner takes a family name with spaces around it as the family (`installed_font`
    /// trims both names), so the report does too, or the editor would offer a different list
    /// than the plan uses.
    #[test]
    fn a_family_name_with_spaces_around_it_is_still_installed() {
        let mut b = backend();
        b.machine.fonts = FakeFonts::new(
            r"C:\Users\tester\AppData\Local\Microsoft\Windows\Fonts",
            &[" Consolas ", "CaskaydiaCove NF  ", "\tCourier New"],
        );
        let r = b.system_detect().unwrap();
        assert!(r.fonts.nerd_font_installed);
        assert_eq!(
            r.fonts.monospace,
            ["CaskaydiaCove NF", "Consolas", "Courier New"]
        );
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
