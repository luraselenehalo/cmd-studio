//! `doctor_run` and `doctor_fix` (spec §4.3 "ตรวจสุขภาพพร้อมปุ่มแก้", §5.8): the core's checks
//! with, for each, what the app can offer to do about it, offered only where it works. A fix that
//! changes the machine is an apply, shown as a plan first (spec §2.1); the others open something,
//! or are words for the user to follow.
//!
//! What a finding offers ([`fix_for`]):
//! - a generated file edited by hand (D9), a cmd add-on that can't load the theme's scripts
//!   (D3), CMD Studio's AutoRun entry gone or not first (D4), cmd's profile in Windows Terminal
//!   changed (D7), the theme's font gone (D8): apply the theme again;
//! - another Clink in the user's AutoRun after an apply (D5): apply again with it turned off;
//! - no Windows Terminal, on a Windows that can have it (D6): its Store page; installed but never
//!   opened (D6): open it once (words);
//! - no cmd window has loaded the theme yet (D3): open a new one;
//! - D1's three causes of `E_RUNTIME_MISSING`, told apart from the machine as D1 tells them
//!   ([`RuntimeFiles`]): the files that come with CMD Studio fail their manifest (or a gone
//!   folder's bundled files are missing or changed), reinstall CMD Studio (words); an add-on
//!   folder is gone, apply again (the apply installs it); files missing or changed inside a
//!   folder that is there, nothing but D1's own hand step;
//! - nothing for a check that could not read (check again), for what the doctor says to fix by
//!   hand (a settings.json CMD Studio can't work with, an AutoRun value that is not text), for D2
//!   and for D3 when Clink does not run (both say to do what D1 says first);
//! - the theme's font gone (D8) when the reapply would install the Nerd Font from a bundled file
//!   that is damaged: reinstall CMD Studio (words);
//! - and no apply at all, from any check, while an add-on folder holds other files or bundled
//!   files the apply needs are damaged (the apply would roll back, or not start), while another
//!   finding stops every apply before it starts (`blocks_every_apply`), or when the applied theme
//!   is no longer in the library (an apply of it can't be planned, and no copy of it is kept).
use super::apply::{ApplyRequest, PlanOutcome};
use super::system::WT_MIN_BUILD;
use super::{Backend, Machine};
use crate::error::AppError;
use cmdstudio_core::actions::Ctx;
use cmdstudio_core::adapters::{AdapterError, RegText, System, WtLocation};
use cmdstudio_core::doctor::{doctor, Check, Status};
use cmdstudio_core::model::Theme;
use cmdstudio_core::plan::{
    autorun_summary, AutoRunSummary, Component, Machine as PlanMachine, NERD_FONT_FAMILY,
};
use cmdstudio_core::resources::{check_bundle, ResourceError};
use cmdstudio_core::state::{Applied, State};
use serde::{Deserialize, Serialize};
use std::path::Path;
use ts_rs::TS;

/// What the app offers for a finding. `reapply`, `replaceOtherClink`, `installWindowsTerminal`
/// and `openNewCmdWindow` are buttons; `openWindowsTerminalOnce` and `reinstall` are steps only
/// the user can take, shown as words.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum FixKind {
    /// Apply the applied theme again, shown as a plan first: D3, D4, D7, D8, D9, and D1 when an
    /// add-on folder is gone (the apply installs it again). Offered only while each add-on folder
    /// is intact or gone (an apply never overwrites one that holds other files, spec §5.4), the
    /// applied theme is still in the library, and nothing stops every apply before it starts.
    Reapply,
    /// Apply again with the other Clink in the user's AutoRun turned off (D5); the plan names the
    /// commands that go, for the user's yes (spec §5.4).
    ReplaceOtherClink,
    /// Open Windows Terminal's Store page (D6: no Windows Terminal, on build 19041 or later).
    InstallWindowsTerminal,
    /// Windows Terminal is installed but was never opened, so it has no settings.json yet (D6):
    /// the user opens it once, then applies again. Words, not a button.
    OpenWindowsTerminalOnce,
    /// No cmd window has loaded the theme yet (D3): open a new one.
    OpenNewCmdWindow,
    /// The files that come with CMD Studio fail their manifest, or files an apply would install
    /// from are missing or changed among them (D1; D8 for the Nerd Font): only a new install of
    /// CMD Studio brings them back. Words, not a button.
    Reinstall,
}

/// One check as the Help page shows it: the core's result, and the fix offered for it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CheckReport {
    pub id: String,
    pub status: Status,
    pub code: Option<String>,
    pub message_th: String,
    pub message_en: String,
    pub details: String,
    pub details_th: Option<String>,
    pub fix: Option<FixKind>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DoctorReport {
    pub checks: Vec<CheckReport>,
    #[ts(type = "number")]
    pub checked_at_ms: u64,
}

/// What `doctor_fix` offers: the fix, and for an apply the request and its plan.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FixProposal {
    pub kind: Option<FixKind>,
    pub request: Option<ApplyRequest>,
    pub plan: Option<PlanOutcome>,
}

/// What doctor D1 finds of the applied theme's add-on files, read from the machine as D1 reads
/// them: the manifest of the files that come with CMD Studio, then each runtime folder. D1 stops
/// at the first problem; this reads every folder, because an apply installs both and works only
/// when each is intact or gone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeFiles {
    /// The manifest is good, and every runtime folder is there with its files unchanged.
    Intact,
    /// The bundled files fail their manifest, so no apply starts (`prepare_apply` reads the
    /// manifest first); or bundled files the apply would install from are missing or changed,
    /// so it would roll back: a gone runtime folder's, or the Nerd Font's when the apply would
    /// install it.
    BundleDamaged,
    /// A runtime folder is gone, its bundled files are whole, and each runtime folder that is
    /// there is intact: an apply installs the gone one.
    FolderGone,
    /// Files are missing or changed inside a runtime folder that is there. An apply never
    /// overwrites such a folder (spec §5.4, the core's `install_runtime`), so it would roll back;
    /// D1 says to delete the folder by hand first.
    FilesChanged,
    /// A read failed, so nothing can be told (D1 says `E_MACHINE_READ`).
    Unreadable,
}

/// [`RuntimeFiles`] for the applied theme `a`: the same reads as D1, in the same order. A gone
/// folder's bundled files are read too, as the apply's install reads them (every file against
/// its pinned hash), and so is the bundled Nerd Font when `installs_font` (see
/// [`installs_font`]).
fn runtime_files(sys: System<'_>, ctx: &Ctx, a: &Applied, installs_font: bool) -> RuntimeFiles {
    let m = match ctx.resources.manifest(sys.fs) {
        Ok(m) => m,
        Err(ResourceError::Io(_)) => return RuntimeFiles::Unreadable,
        Err(_) => return RuntimeFiles::BundleDamaged,
    };
    let launch = a.launch.to_clink();
    let mut gone = false;
    for (component, dir) in [
        (Component::Omp, ctx.paths.omp_dir()),
        (Component::Clink, launch.runtime_dir(&ctx.paths)),
    ] {
        if !sys.fs.exists(&dir) {
            match check_bundle(sys.fs, &ctx.resources.dir(component), m.bundle(component)) {
                Ok(()) => gone = true,
                Err(ResourceError::Io(_)) => return RuntimeFiles::Unreadable,
                Err(_) => return RuntimeFiles::BundleDamaged,
            }
            continue;
        }
        match check_bundle(sys.fs, &dir, m.bundle(component)) {
            Ok(()) => {}
            Err(ResourceError::Io(_)) => return RuntimeFiles::Unreadable,
            Err(_) => return RuntimeFiles::FilesChanged,
        }
    }
    // The font's install checks the bundled file against its pin first (`install_font`).
    let font_dir = ctx
        .resources
        .font_file(NERD_FONT_FAMILY)
        .and_then(|f| f.parent().map(Path::to_path_buf));
    if let (true, Some(dir)) = (installs_font, font_dir) {
        match check_bundle(sys.fs, &dir, &m.fonts) {
            Ok(()) => {}
            Err(ResourceError::Io(_)) => return RuntimeFiles::Unreadable,
            Err(_) => return RuntimeFiles::BundleDamaged,
        }
    }
    if gone {
        RuntimeFiles::FolderGone
    } else {
        RuntimeFiles::Intact
    }
}

/// Whether an apply of `theme` installs the bundled Nerd Font, as the planner decides it: when
/// Windows Terminal has a settings file and neither the theme's font nor the Nerd Font is
/// installed (the theme's font falls back to the Nerd Font, which is installed). A font list
/// that can't be read counts as yes, so the font file is checked rather than taken as unused.
fn installs_font(sys: System<'_>, theme: &Theme) -> bool {
    let wt = sys
        .wt
        .locations()
        .iter()
        .any(|l| sys.fs.exists(&l.settings));
    let Ok(families) = sys.fonts.families() else {
        return wt;
    };
    let installed = |family: &str| {
        families
            .iter()
            .any(|f| f.trim().eq_ignore_ascii_case(family.trim()))
    };
    wt && !installed(NERD_FONT_FAMILY) && !installed(&theme.terminal.font)
}

/// A finding that makes every apply fail before it starts, so that while it stands no check
/// offers one:
/// - a Windows Terminal settings.json CMD Studio can't work with (D6, D7 `E_WT_PARSE`):
///   `prepare_apply` reads and edits it;
/// - an AutoRun value that is there but is not text (D4 `E_AUTORUN_WRITE`): the planner refuses
///   it for every cmd window;
/// - a newest restore point that can't be read now (R1 `E_SNAPSHOT_READ`, `fail`): every change
///   is refused until it can be (the core's `begin`). The `warn` of R1 is a damaged point, which
///   the next change sets aside and goes on, so it blocks nothing;
/// - restore points that can't be listed (R2 `fail`): `begin` lists them first and refuses
///   with `E_SNAPSHOT_READ`. The `warn` of R2 (points that can't be read among those listed)
///   blocks nothing.
fn blocks_every_apply(checks: &[Check], autorun: Option<&AutoRunSummary>) -> bool {
    checks.iter().any(|c| match (c.id, c.code) {
        ("D6" | "D7", Some("E_WT_PARSE")) => true,
        ("D4", Some("E_AUTORUN_WRITE")) => autorun.is_none_or(|a| a.not_text),
        ("R1", Some("E_SNAPSHOT_READ")) | ("R2", _) => c.status == Status::Fail,
        _ => false,
    })
}

/// The fix for `check`, from what it found and from the machine as `doctor_run` read it.
/// `runtime` is [`RuntimeFiles`] of the applied theme, `None` when no theme is applied or
/// state.json can't be read (there is nothing to apply again then); `apply_blocked` says that no
/// apply of the applied theme can start: another finding stops every apply (see
/// `blocks_every_apply`), or the theme is no longer in the library; `autorun` is
/// the user's AutoRun as the planner reads it, `None` when it could not be read; `wt` the
/// Windows Terminal locations and `build` the Windows build. A fix is offered only where it
/// works (see the module's list).
pub fn fix_for(
    check: &Check,
    runtime: Option<RuntimeFiles>,
    apply_blocked: bool,
    autorun: Option<&AutoRunSummary>,
    wt: &[WtLocation],
    build: Option<u32>,
) -> Option<FixKind> {
    if matches!(check.status, Status::Ok | Status::Skipped) {
        return None;
    }
    let code = check.code?;
    // A check that could not read found nothing to fix: check again instead. A settings.json CMD
    // Studio can't work with is fixed by hand.
    if matches!(code, "E_MACHINE_READ" | "E_WT_PARSE") {
        return None;
    }
    // An apply installs both add-on folders: it works when each is intact or gone, and nothing
    // else stops it before it starts.
    let can_apply = !apply_blocked
        && matches!(
            runtime,
            Some(RuntimeFiles::Intact | RuntimeFiles::FolderGone)
        );
    match (check.id, code) {
        ("D1", _) => match runtime? {
            RuntimeFiles::BundleDamaged => Some(FixKind::Reinstall),
            RuntimeFiles::FolderGone => can_apply.then_some(FixKind::Reapply),
            RuntimeFiles::Intact | RuntimeFiles::FilesChanged | RuntimeFiles::Unreadable => None,
        },
        // A bundled program does not run: both say to do what D1 says first.
        ("D2", _) | ("D3", "E_RUNTIME_MISSING") => None,
        ("D3", "E_THEME_NOT_LOADED") => Some(FixKind::OpenNewCmdWindow),
        // The reapply would install the font from bundled files that are damaged.
        ("D8", "E_FONT_INSTALL") if runtime == Some(RuntimeFiles::BundleDamaged) => {
            Some(FixKind::Reinstall)
        }
        ("D3" | "D4" | "D7" | "D8" | "D9", _) => can_apply.then_some(FixKind::Reapply),
        ("D5", _) => (can_apply && autorun.is_some_and(|a| !a.foreign_clink.is_empty()))
            .then_some(FixKind::ReplaceOtherClink),
        ("D6", "E_WT_NOT_FOUND") => {
            if wt.iter().any(|l| l.installed) {
                Some(FixKind::OpenWindowsTerminalOnce)
            } else {
                build
                    .is_some_and(|b| b >= WT_MIN_BUILD)
                    .then_some(FixKind::InstallWindowsTerminal)
            }
        }
        _ => None,
    }
}

impl<M: Machine> Backend<M> {
    /// Runs the core's checks and offers a fix for each where there is one. It changes nothing:
    /// what it reads besides the checks (state.json, the add-on files, AutoRun, the Windows
    /// Terminal locations, the build) it reads as the checks do, and not through the core's
    /// `detect`, which may create the data folder (see `system_detect`).
    pub fn doctor_run(&self) -> Result<DoctorReport, AppError> {
        let sys = self.sys();
        let ctx = self.ctx();
        let checks = doctor(sys, &ctx);
        // An apply of the applied theme needs the theme: one deleted from the library can't be
        // planned, and nothing keeps a copy of it to apply from.
        let (runtime, theme_found) = match State::load(sys.fs, &ctx.paths.state_file()) {
            Ok(State {
                applied: Some(a), ..
            }) => {
                let theme = self.find_theme(a.theme_id).ok().map(|(t, _, _)| t);
                let font = theme.as_ref().is_some_and(|t| installs_font(sys, t));
                (Some(runtime_files(sys, &ctx, &a, font)), theme.is_some())
            }
            _ => (None, false),
        };
        let autorun = self.autorun_now().ok();
        let apply_blocked = !theme_found || blocks_every_apply(&checks, autorun.as_ref());
        let wt = sys.wt.locations();
        let build = sys.host.system_info().build;
        let checks = checks
            .iter()
            .map(|c| CheckReport {
                id: c.id.to_string(),
                status: c.status,
                code: c.code.map(str::to_string),
                message_th: c.message_th.clone(),
                message_en: c.message_en.clone(),
                details: c.details.clone(),
                details_th: c.details_th.as_deref().cloned(),
                fix: fix_for(c, runtime, apply_blocked, autorun.as_ref(), &wt, build),
            })
            .collect();
        Ok(DoctorReport {
            checks,
            checked_at_ms: ctx.now_ms,
        })
    }

    /// What "Fix" does for `check_id`: for a fix that applies, the request from state.json
    /// (the applied theme, its target, its default-profile choice) and its plan, which the UI
    /// shows before calling `apply`; for the others the fix alone. An id the doctor does not
    /// have is `E_INTERNAL`: the UI only sends the ids it was given.
    pub fn doctor_fix(&self, check_id: &str) -> Result<FixProposal, AppError> {
        let report = self.doctor_run()?;
        let check = report
            .checks
            .iter()
            .find(|c| c.id == check_id)
            .ok_or_else(|| AppError::internal(format!("there is no check {check_id}")))?;
        let kind = check.fix;
        let applies = matches!(kind, Some(FixKind::Reapply | FixKind::ReplaceOtherClink));
        if !applies {
            return Ok(FixProposal {
                kind,
                request: None,
                plan: None,
            });
        }
        let state = State::load(self.sys().fs, &self.paths().state_file())?;
        let Some(applied) = state.applied else {
            return Ok(FixProposal {
                kind: None,
                request: None,
                plan: None,
            });
        };
        let request = ApplyRequest {
            theme_id: applied.theme_id,
            target: applied.target,
            set_wt_default_profile: applied.set_default,
            replace_other_clink: kind == Some(FixKind::ReplaceOtherClink),
        };
        let plan = self.plan_apply(request.clone())?;
        Ok(FixProposal {
            kind,
            request: Some(request),
            plan: Some(plan),
        })
    }

    /// The user's AutoRun as the planner reads it (`plan::autorun_summary`), from the two values
    /// as they are now, read as doctor D5 reads them.
    pub(crate) fn autorun_now(&self) -> Result<AutoRunSummary, AdapterError> {
        let sys = self.sys();
        let values = PlanMachine {
            autorun_hkcu: RegText::of(sys.autorun.read_user()?.as_ref()),
            autorun_hklm: RegText::of(sys.autorun.read_machine()?.as_ref()),
            ..PlanMachine::default()
        };
        Ok(autorun_summary(&values, self.paths()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::apply::ApplyStatus;
    use crate::backend::testing::{
        apply_terracotta, backend, fake_fs, plan_and_apply, terracotta_id, terracotta_request, WT,
    };
    use cmdstudio_core::adapters::fake::{FakeFonts, FakeMachine};
    use cmdstudio_core::adapters::{AutoRun, RegValue, WtVariant, REG_SZ};
    use cmdstudio_core::plan::NERD_FONT_FAMILY;
    use cmdstudio_core::resources::{MANIFEST, NERD_FONT_FILE};
    use std::path::{Path, PathBuf};

    /// The user fonts folder of the test machine (`testing::backend`).
    const FONTS: &str = r"C:\Users\tester\AppData\Local\Microsoft\Windows\Fonts";

    /// Terracotta, applied as the wizard applies it, and committed.
    fn applied(b: &Backend<FakeMachine>) {
        assert_eq!(apply_terracotta(b).0.status, ApplyStatus::Committed);
    }

    fn by_id<'a>(report: &'a DoctorReport, id: &str) -> &'a CheckReport {
        report.checks.iter().find(|c| c.id == id).unwrap()
    }

    /// One of the generated files, edited by hand: D9 warns, and on its own offers a reapply.
    fn edit_by_hand(b: &Backend<FakeMachine>) {
        let banner = b.paths().generated().join("banner.lua");
        b.sys()
            .fs
            .write_atomic(&banner, b"-- edited by hand")
            .unwrap();
    }

    /// What `doctor_fix` answers when there is nothing to act on.
    fn nothing() -> FixProposal {
        FixProposal {
            kind: None,
            request: None,
            plan: None,
        }
    }

    /// No check of `report` offers an apply.
    fn offers_no_apply(report: &DoctorReport) -> bool {
        report
            .checks
            .iter()
            .all(|c| !matches!(c.fix, Some(FixKind::Reapply | FixKind::ReplaceOtherClink)))
    }

    #[test]
    fn a_clean_machine_after_an_apply_needs_no_fix() {
        let b = backend();
        applied(&b);
        let report = b.doctor_run().unwrap();
        assert_eq!(report.checks.len(), 11);
        assert!(report.checked_at_ms >= 1_791_694_709_000);
        for c in &report.checks {
            assert!(!c.message_th.is_empty() && !c.message_en.is_empty());
            if c.id != "D3" {
                assert!(c.fix.is_none(), "{}: {:?} {:?}", c.id, c.status, c.fix);
            }
        }
        // D3 warns that no window has loaded the theme yet: the fix is to open one.
        let d3 = by_id(&report, "D3");
        assert_eq!(
            (d3.status, d3.code.as_deref()),
            (Status::Warn, Some("E_THEME_NOT_LOADED"))
        );
        assert_eq!(d3.fix, Some(FixKind::OpenNewCmdWindow));
    }

    #[test]
    fn a_hand_edited_file_offers_to_apply_again_with_a_plan() {
        let b = backend();
        applied(&b);
        edit_by_hand(&b);
        let report = b.doctor_run().unwrap();
        let d9 = by_id(&report, "D9");
        assert_eq!(
            (d9.code.as_deref(), d9.fix),
            (Some("E_GENERATED_EDITED"), Some(FixKind::Reapply))
        );
        let proposal = b.doctor_fix("D9").unwrap();
        assert_eq!(proposal.kind, Some(FixKind::Reapply));
        let request = proposal.request.unwrap();
        assert_eq!(
            (request.theme_id, request.replace_other_clink),
            (terracotta_id(), false)
        );
        assert!(matches!(proposal.plan, Some(PlanOutcome::Ready { .. })));
    }

    #[test]
    fn another_clink_offers_to_replace_it_and_a_missing_terminal_the_store() {
        let b = backend();
        applied(&b);
        let other = r#""C:\clink\clink_x64.exe" inject --autorun"#;
        let ours = b
            .machine
            .autorun
            .read_user()
            .unwrap()
            .unwrap()
            .text()
            .unwrap();
        b.machine
            .autorun
            .write_user(&RegValue::from_text(REG_SZ, &format!("{ours} & {other}")))
            .unwrap();
        let report = b.doctor_run().unwrap();
        assert_eq!(by_id(&report, "D5").fix, Some(FixKind::ReplaceOtherClink));
        let proposal = b.doctor_fix("D5").unwrap();
        assert!(proposal.request.unwrap().replace_other_clink);
        assert!(matches!(proposal.plan, Some(PlanOutcome::Ready { .. })));
        // No Windows Terminal at all, on a build that can have it.
        let mut b = backend();
        b.machine.wt.locations = vec![];
        let report = b.doctor_run().unwrap();
        assert_eq!(
            by_id(&report, "D6").fix,
            Some(FixKind::InstallWindowsTerminal)
        );
        let proposal = b.doctor_fix("D6").unwrap();
        assert_eq!(
            (proposal.kind, proposal.request, proposal.plan),
            (Some(FixKind::InstallWindowsTerminal), None, None)
        );
        // Too old for Windows Terminal: nothing to offer.
        b.machine.host.info.build = Some(18363);
        assert_eq!(by_id(&b.doctor_run().unwrap(), "D6").fix, None);
        // Installed but never opened: open it once.
        b.machine.wt.locations = vec![WtLocation {
            variant: WtVariant::Store,
            installed: true,
            settings: PathBuf::from(r"C:\nowhere\settings.json"),
        }];
        assert_eq!(
            by_id(&b.doctor_run().unwrap(), "D6").fix,
            Some(FixKind::OpenWindowsTerminalOnce)
        );
    }

    #[test]
    fn a_check_that_could_not_read_offers_nothing_and_an_unknown_id_is_refused() {
        let b = backend();
        applied(&b);
        b.machine.autorun.fail_reads();
        let report = b.doctor_run().unwrap();
        let d4 = by_id(&report, "D4");
        assert_eq!(
            (d4.status, d4.code.as_deref(), d4.fix),
            (Status::Fail, Some("E_MACHINE_READ"), None)
        );
        assert_eq!(b.doctor_fix("D99").unwrap_err().code, "E_INTERNAL");
    }

    /// D1, first of its three causes of `E_RUNTIME_MISSING`: the files that come with CMD Studio
    /// fail their manifest. Only a new install brings them back, and no apply starts without
    /// them, so no other check offers one either.
    #[test]
    fn bundled_files_that_fail_their_manifest_say_reinstall_and_offer_no_apply() {
        let b = backend();
        applied(&b);
        b.sys()
            .fs
            .write_atomic(&b.resources.root.join(MANIFEST), b"{ damaged")
            .unwrap();
        edit_by_hand(&b);
        let report = b.doctor_run().unwrap();
        let d1 = by_id(&report, "D1");
        assert_eq!(
            (d1.status, d1.code.as_deref(), d1.fix),
            (
                Status::Fail,
                Some("E_RUNTIME_MISSING"),
                Some(FixKind::Reinstall)
            )
        );
        // An instruction: there is nothing to plan.
        assert_eq!(
            b.doctor_fix("D1").unwrap(),
            FixProposal {
                kind: Some(FixKind::Reinstall),
                request: None,
                plan: None,
            }
        );
        // The hand-edited file alone would be applied again; not without the bundled files.
        assert_eq!(
            by_id(&report, "D9").code.as_deref(),
            Some("E_GENERATED_EDITED")
        );
        assert!(offers_no_apply(&report), "{:?}", report.checks);
        assert_eq!(b.doctor_fix("D9").unwrap(), nothing());
        assert_eq!(
            b.plan_apply(terracotta_request()).unwrap_err().code,
            "E_RUNTIME_MISSING"
        );
    }

    /// D1, second cause: an add-on folder is gone. An apply installs it again, so the fix is a
    /// reapply with its plan, and applying that plan brings the folder back.
    #[test]
    fn a_runtime_folder_that_is_gone_is_applied_again_and_comes_back() {
        let b = backend();
        applied(&b);
        b.sys().fs.remove_dir_all(&b.paths().omp_dir()).unwrap();
        let report = b.doctor_run().unwrap();
        let d1 = by_id(&report, "D1");
        assert_eq!(
            (d1.status, d1.code.as_deref(), d1.fix),
            (
                Status::Fail,
                Some("E_RUNTIME_MISSING"),
                Some(FixKind::Reapply)
            )
        );
        let proposal = b.doctor_fix("D1").unwrap();
        assert_eq!(proposal.kind, Some(FixKind::Reapply));
        let request = proposal.request.unwrap();
        assert_eq!(request, terracotta_request());
        let plan = match proposal.plan {
            Some(PlanOutcome::Ready { plan }) => plan,
            other => panic!("{other:?}"),
        };
        let done = b.apply(request, &plan.digest, &mut |_| {}).unwrap();
        assert_eq!(done.status, ApplyStatus::Committed);
        assert!(b.sys().fs.exists(&b.paths().omp_exe()));
        assert_eq!(by_id(&b.doctor_run().unwrap(), "D1").status, Status::Ok);
    }

    /// D1, third cause: files are missing or changed inside an add-on folder that is there. An
    /// apply never overwrites such a folder (spec §5.4) and rolls back, so no check offers one:
    /// D1's own words give the hand step. D2, and D3 when Clink does not run, say to see D1 and
    /// offer nothing themselves.
    #[test]
    fn changed_files_in_a_runtime_folder_offer_no_apply_only_the_hand_step() {
        let b = backend();
        applied(&b);
        b.sys().fs.remove_file(&b.paths().omp_exe()).unwrap();
        edit_by_hand(&b);
        b.machine
            .process
            .answer("oh-my-posh.exe", "version", 0, "30.0.0");
        let report = b.doctor_run().unwrap();
        let d1 = by_id(&report, "D1");
        assert_eq!(
            (d1.status, d1.code.as_deref(), d1.fix),
            (Status::Fail, Some("E_RUNTIME_MISSING"), None)
        );
        assert!(
            d1.message_en.contains("delete the folder"),
            "{}",
            d1.message_en
        );
        let d2 = by_id(&report, "D2");
        assert_eq!(
            (d2.status, d2.code.as_deref(), d2.fix),
            (Status::Fail, Some("E_RUNTIME_MISSING"), None)
        );
        assert_eq!(
            by_id(&report, "D9").code.as_deref(),
            Some("E_GENERATED_EDITED")
        );
        assert!(offers_no_apply(&report), "{:?}", report.checks);
        for id in ["D1", "D2", "D9"] {
            assert_eq!(b.doctor_fix(id).unwrap(), nothing(), "{id}");
        }
        // The fake always runs Clink, so D3's own "does not run" is put to `fix_for` directly.
        let d3 = Check {
            id: "D3",
            status: Status::Fail,
            code: Some("E_RUNTIME_MISSING"),
            message_th: String::new(),
            message_en: String::new(),
            details: String::new(),
            details_th: None,
        };
        assert_eq!(
            fix_for(
                &d3,
                Some(RuntimeFiles::Intact),
                false,
                None,
                &[],
                Some(26100)
            ),
            None
        );
        // Why: the apply finds the folder there with other files, and rolls back.
        let (done, _) = apply_terracotta(&b);
        assert_eq!(done.status, ApplyStatus::RolledBack);
        assert_eq!(done.error.unwrap().code, "E_RUNTIME_MISSING");
    }

    /// What the doctor says to fix by hand stops every apply before it starts, so while it
    /// stands no check offers one, not even a hand-edited file that is otherwise applied again:
    /// a Windows Terminal settings.json CMD Studio can't work with, an AutoRun value that is not
    /// plain text, a newest restore point that can't be read now.
    #[test]
    fn a_problem_to_fix_by_hand_offers_no_apply() {
        let b = backend();
        applied(&b);
        edit_by_hand(&b);
        let d9 = |b: &Backend<FakeMachine>| by_id(&b.doctor_run().unwrap(), "D9").fix;
        assert_eq!(d9(&b), Some(FixKind::Reapply));
        // Windows Terminal's settings.json.
        let wt = b.sys().fs.read(Path::new(WT)).unwrap().unwrap();
        b.sys()
            .fs
            .write_atomic(Path::new(WT), b"{ not json")
            .unwrap();
        let report = b.doctor_run().unwrap();
        for id in ["D6", "D7"] {
            let c = by_id(&report, id);
            assert_eq!(
                (c.code.as_deref(), c.fix),
                (Some("E_WT_PARSE"), None),
                "{id}"
            );
        }
        assert!(offers_no_apply(&report), "{:?}", report.checks);
        assert_eq!(
            b.plan_apply(terracotta_request()).unwrap_err().code,
            "E_WT_PARSE"
        );
        b.sys().fs.write_atomic(Path::new(WT), &wt).unwrap();
        assert_eq!(d9(&b), Some(FixKind::Reapply));
        // A REG_DWORD: there, but not text.
        let ours = b.machine.autorun.read_user().unwrap().unwrap();
        b.machine
            .autorun
            .write_user(&RegValue {
                kind: 4,
                bytes: vec![1, 0, 0, 0],
            })
            .unwrap();
        let report = b.doctor_run().unwrap();
        let d4 = by_id(&report, "D4");
        assert_eq!(
            (d4.code.as_deref(), d4.fix),
            (Some("E_AUTORUN_WRITE"), None)
        );
        assert!(offers_no_apply(&report), "{:?}", report.checks);
        assert_eq!(
            b.plan_apply(terracotta_request()).unwrap_err().code,
            "E_AUTORUN_WRITE"
        );
        b.machine.autorun.write_user(&ours).unwrap();
        assert_eq!(d9(&b), Some(FixKind::Reapply));
        // The newest restore point's journal can't be read now: every change is refused until
        // it can be.
        let newest = b.snapshots_list().unwrap().points[0].id.clone();
        let journal = b.paths().snapshots().join(newest).join("journal.json");
        fake_fs(&b).fail_reads_of(&journal);
        let report = b.doctor_run().unwrap();
        let r1 = by_id(&report, "R1");
        assert_eq!(
            (r1.status, r1.code.as_deref(), r1.fix),
            (Status::Fail, Some("E_SNAPSHOT_READ"), None)
        );
        assert!(offers_no_apply(&report), "{:?}", report.checks);
        assert!(b.plan_apply(terracotta_request()).is_err());
        fake_fs(&b).stop_failing(&journal);
        assert_eq!(d9(&b), Some(FixKind::Reapply));
    }

    /// Restore points that can't be listed (R2 `fail`) stop every change before it starts (the
    /// core's `begin` lists them first), so they block every apply; R2's `warn`, points that
    /// can't be read among those listed, does not. The fake's folder listing never fails, so
    /// the rule is put to `blocks_every_apply` directly.
    #[test]
    fn restore_points_that_cannot_be_listed_block_every_apply() {
        let check = |id: &'static str, status: Status, code: &'static str| Check {
            id,
            status,
            code: Some(code),
            message_th: String::new(),
            message_en: String::new(),
            details: String::new(),
            details_th: None,
        };
        assert!(blocks_every_apply(
            &[check("R2", Status::Fail, "E_SNAPSHOT_READ")],
            None
        ));
        assert!(!blocks_every_apply(
            &[check("R2", Status::Warn, "E_SNAPSHOT_READ")],
            None
        ));
    }

    /// The applied theme was deleted from the library (allowed: the machine keeps its files
    /// until the next apply). An apply of it can't be planned, and there is no saved copy to
    /// apply from, so no check offers one; the checks' own words still stand.
    #[test]
    fn a_deleted_applied_theme_offers_no_apply() {
        let b = backend();
        let mine = b
            .theme_duplicate(terracotta_id(), "Mine".to_string())
            .unwrap();
        let req = ApplyRequest {
            theme_id: mine.id,
            ..terracotta_request()
        };
        assert_eq!(
            plan_and_apply(&b, req.clone()).0.status,
            ApplyStatus::Committed
        );
        edit_by_hand(&b);
        assert_eq!(
            by_id(&b.doctor_run().unwrap(), "D9").fix,
            Some(FixKind::Reapply)
        );
        b.theme_delete(mine.id, false).unwrap();
        let report = b.doctor_run().unwrap();
        let d9 = by_id(&report, "D9");
        assert_eq!(
            (d9.code.as_deref(), d9.fix),
            (Some("E_GENERATED_EDITED"), None)
        );
        assert!(offers_no_apply(&report), "{:?}", report.checks);
        assert_eq!(b.doctor_fix("D9").unwrap(), nothing());
        assert_eq!(b.plan_apply(req).unwrap_err().code, "E_THEME_INVALID");
    }

    /// When the apply would install the bundled Nerd Font (the theme's font and the Nerd Font
    /// are not installed), the font file must be whole: the install checks it against its pin
    /// and the apply rolls back. Then the fix is a new install, and no check offers an apply.
    /// While the Nerd Font is installed, the apply does not read the file, and it stops nothing.
    #[test]
    fn a_damaged_bundled_font_says_reinstall_when_the_apply_would_install_it() {
        let without_the_fonts = || {
            let fonts = FakeFonts::new(FONTS, &["Arial", "Consolas"]);
            fonts.knows_file(NERD_FONT_FILE, NERD_FONT_FAMILY);
            fonts
        };
        let mut b = backend();
        // Terracotta's Cascadia Mono is not installed: the apply installs the Nerd Font.
        b.machine.fonts = without_the_fonts();
        applied(&b);
        edit_by_hand(&b);
        let font = b.resources.font_file(NERD_FONT_FAMILY).unwrap();
        b.sys().fs.write_atomic(&font, b"not the font").unwrap();
        assert_eq!(
            by_id(&b.doctor_run().unwrap(), "D9").fix,
            Some(FixKind::Reapply)
        );
        // The Nerd Font is gone again: a reapply would install it from the damaged file.
        b.machine.fonts = without_the_fonts();
        let report = b.doctor_run().unwrap();
        let d8 = by_id(&report, "D8");
        assert_eq!(
            (d8.code.as_deref(), d8.fix),
            (Some("E_FONT_INSTALL"), Some(FixKind::Reinstall))
        );
        assert_eq!(by_id(&report, "D9").fix, None);
        assert!(offers_no_apply(&report), "{:?}", report.checks);
        assert_eq!(
            b.doctor_fix("D8").unwrap(),
            FixProposal {
                kind: Some(FixKind::Reinstall),
                request: None,
                plan: None,
            }
        );
        let (done, _) = apply_terracotta(&b);
        assert_eq!(done.status, ApplyStatus::RolledBack);
    }

    /// A gone runtime folder is installed again from the files that come with CMD Studio: when
    /// those fail their manifest, the apply would roll back, so the fix is a new install. A
    /// bundled file that can't be read now (an antivirus holds it, say) offers nothing.
    #[test]
    fn a_gone_runtime_folder_whose_bundled_files_are_damaged_says_reinstall() {
        let b = backend();
        applied(&b);
        let bundled = b.resources.dir(Component::Omp).join("oh-my-posh.exe");
        let bytes = b.sys().fs.read(&bundled).unwrap().unwrap();
        b.sys().fs.remove_file(&bundled).unwrap();
        b.sys().fs.remove_dir_all(&b.paths().omp_dir()).unwrap();
        let report = b.doctor_run().unwrap();
        let d1 = by_id(&report, "D1");
        assert_eq!(
            (d1.code.as_deref(), d1.fix),
            (Some("E_RUNTIME_MISSING"), Some(FixKind::Reinstall))
        );
        assert_eq!(
            b.doctor_fix("D1").unwrap(),
            FixProposal {
                kind: Some(FixKind::Reinstall),
                request: None,
                plan: None,
            }
        );
        assert!(offers_no_apply(&report), "{:?}", report.checks);
        // Why: the apply installs from those files, and rolls back.
        let (done, _) = apply_terracotta(&b);
        assert_eq!(done.status, ApplyStatus::RolledBack);
        // Back, but unreadable now: nothing to offer until it can be read.
        b.sys().fs.write_atomic(&bundled, &bytes).unwrap();
        fake_fs(&b).fail_reads_of(&bundled);
        let report = b.doctor_run().unwrap();
        assert_eq!(by_id(&report, "D1").fix, None);
        assert!(offers_no_apply(&report), "{:?}", report.checks);
    }
}
