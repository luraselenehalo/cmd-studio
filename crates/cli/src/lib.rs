//! The `cmdstudio` command line (spec §5.7): `apply`, `doctor`, `undo`, `--cleanup` and
//! `set-aside`. A thin layer over the core: it reads the command line, says what the core found
//! and did, and turns it into an exit code. The Phase 2 app links this library and calls `run`
//! when it is started with arguments, so the app and the CLI are one exe.
//!
//! Everything it says is English, whatever the language of Windows: nothing CMD Studio writes
//! into a cmd window is Thai (the founder's rule of 2026-10-13). Where the core has a text in
//! both languages, the CLI says the English one; the Thai ones are for the app's own UI. Names
//! and paths of the user's are shown as they are written.
//!
//! Only the commands that change the machine (apply, undo, `--cleanup`, set-aside) start the
//! way the core's commands do (`recover_at_start`): under CMD Studio's lock, a transaction a
//! crash cut off is rolled back first. `doctor` and `--help` only read: they take no lock and
//! run no recovery, so another run never blocks them.
pub mod args;

use args::{parse, Args, Command, USAGE};
use cmdstudio_core::actions::{
    pending_incomplete, prepare_apply, recover_at_start, run_apply, undo, ActionError, Ctx,
    Handled, StartReport, UndoReport, UndoResult, UnreadablePoint,
};
use cmdstudio_core::adapters::{DirEntry, System};
use cmdstudio_core::cleanup::{cleanup, Report};
use cmdstudio_core::doctor::{doctor, Check, Status};
use cmdstudio_core::exec::{op_label, CLEANUP_FIX_EN, CLEANUP_FIX_TH, COMMAND_PROCESSOR_KEY};
use cmdstudio_core::model::{parse_theme, Target};
use cmdstudio_core::plan::{
    Op, Paths, PlanError, Request, Warning, AUTORUN_NOT_TEXT, NERD_FONT_FAMILY,
};
use cmdstudio_core::recovery::clear_pause;
use cmdstudio_core::resources::Resources;
use cmdstudio_core::snapshot::{Incomplete, SetAside, SnapshotStore, Unrestored};
use cmdstudio_core::txn::{Failure, Outcome};
use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Done.
pub const EXIT_OK: i32 = 0;
/// Failed, and everything was rolled back, or refused before the user's settings were changed
/// (another run holds the lock, a restore point that must be read first, an AutoRun that needs a
/// hand edit). Before it plans, a command may already have deleted font files CMD Studio left to
/// delete, so the CLI never says that nothing at all was changed.
pub const EXIT_FAILED: i32 = 1;
/// The command line or the theme file is not valid, or another Clink is in AutoRun and
/// `--replace-other-clink` was not given.
pub const EXIT_INVALID: i32 = 2;
/// A rollback could not restore everything: the user must fix what it lists by hand.
pub const EXIT_INCOMPLETE: i32 = 3;

/// The data folder's name under `%LOCALAPPDATA%` (spec §5.3, G10): what Tauri's
/// `app_local_data_dir()` gives for the identifier `ai.resincore.cmdstudio`, spelled out so the
/// CLI and the app agree without Tauri.
pub const DATA_DIR_NAME: &str = "ai.resincore.cmdstudio";

pub struct Io<'a> {
    pub out: &'a mut dyn Write,
    pub err: &'a mut dyn Write,
}

/// Where things are when no `--data-dir` / `--resources` is given, and the clock.
#[derive(Clone, Debug)]
pub struct Defaults {
    pub data_dir: PathBuf,
    pub resources: PathBuf,
    pub now_ms: u64,
}

/// Runs one command on the machine `sys` and returns the exit code.
pub fn run_with(args: &[OsString], io: &mut Io<'_>, sys: System<'_>, defaults: &Defaults) -> i32 {
    let mut say = Say::new(io);
    let Args {
        command,
        data_dir,
        resources,
    } = match parse(args) {
        Ok(args) => args,
        Err(e) => {
            let _ = write!(say.io.err, "cmdstudio: {e}\n\n{USAGE}");
            return EXIT_INVALID;
        }
    };
    let data_dir_given = data_dir.is_some();
    let ctx = Ctx {
        paths: Paths {
            data: data_dir.unwrap_or_else(|| defaults.data_dir.clone()),
        },
        resources: Resources {
            root: resources.unwrap_or_else(|| defaults.resources.clone()),
        },
        now_ms: defaults.now_ms,
    };
    match command {
        Command::Help => {
            let _ = write!(say.io.out, "{USAGE}");
            EXIT_OK
        }
        Command::Doctor { json } => say.doctor(sys, &ctx, json),
        Command::Apply {
            theme,
            target,
            replace_other_clink,
        } => {
            let req = Request {
                target,
                // Like the wizard's checkbox, which is ticked by default (spec §4.2 page 4).
                set_wt_default_profile: true,
                replace_foreign_clink: replace_other_clink,
            };
            say.apply(sys, &ctx, &theme, &req)
        }
        Command::Undo => say.undo(sys, &ctx),
        Command::Cleanup { remove_themes } => say.cleanup(sys, &ctx, remove_themes, data_dir_given),
        Command::SetAside { id } => say.set_aside(sys, &ctx, &id),
    }
}

/// What `--cleanup` says when steps failed. The uninstaller shows the exit code; a font file
/// still in use or the record of a rollback that did not finish is often done by a second run.
const CLEANUP_AGAIN_EN: &str = "Not everything could be done: see each FAILED step above. Some \
     finish when you run `cmdstudio --cleanup` again, for example once every cmd window and \
     Windows Terminal are closed; the others say how to fix them by hand.";

/// What the user reads, and what this run said already: the starts of the CLI and of the core's
/// command both report what they found, and each thing is said once.
struct Say<'s, 'a> {
    io: &'s mut Io<'a>,
    said: Vec<String>,
    /// The restore points whose record of a rollback that did not finish was listed.
    records: Vec<String>,
}

impl<'s, 'a> Say<'s, 'a> {
    fn new(io: &'s mut Io<'a>) -> Self {
        Say {
            io,
            said: Vec::new(),
            records: Vec::new(),
        }
    }

    fn out(&mut self, text: impl std::fmt::Display) {
        let _ = writeln!(self.io.out, "{text}");
    }

    fn err(&mut self, text: impl std::fmt::Display) {
        let _ = writeln!(self.io.err, "{text}");
    }

    /// Says `text` on stderr, unless this run said it already.
    fn once(&mut self, text: String) {
        if !self.said.contains(&text) {
            self.err(&text);
            self.said.push(text);
        }
    }

    /// `code: text`, where `en` is the core's English text.
    fn coded(&self, code: &str, en: &str) -> String {
        format!("{code}: {en}")
    }

    fn note(&self, text: &str) -> String {
        format!("Note: {text}")
    }

    /// The core's words for `e`, in English.
    fn error_text(&self, e: &ActionError) -> String {
        self.coded(e.code(), &e.to_string())
    }

    /// What every command that changes the machine does first (`recover_at_start`), told.
    /// `Err` with the exit code when the command must not go on: another run holds the lock,
    /// or (`strict`) the rollback of a transaction a crash cut off could not put everything
    /// back just now, so the user fixes that first. `--cleanup` and set-aside go on: they are
    /// how it is fixed.
    fn begin(&mut self, sys: System<'_>, ctx: &Ctx, strict: bool) -> Result<(), i32> {
        match recover_at_start(sys, ctx) {
            Ok(start) => {
                if self.start(&start) && strict {
                    Err(EXIT_INCOMPLETE)
                } else {
                    Ok(())
                }
            }
            Err(e) => Err(self.refused(sys, ctx, &e)),
        }
    }

    /// Tells what a start found and did, each thing once: a crash rolled back, restore points
    /// that cannot be read, what the start could not do, and the record of a rollback that did
    /// not finish, which is said before every command until a newer transaction runs. Whether
    /// the rollback of a transaction a crash cut off could not put everything back just now.
    fn start(&mut self, start: &StartReport) -> bool {
        let mut not_back = false;
        match &start.recovered {
            Some(Outcome::RolledBack { code, reason }) => {
                self.once(format!(
                    "{code}: {reason}; the machine is as it was before it."
                ));
            }
            // Its journal cannot be read now, so nothing was rolled back: the command that
            // changes the machine refuses, and says what to do (`refused`).
            Some(Outcome::RollbackIncomplete { .. }) if start.recovery_pending => {}
            Some(Outcome::RollbackIncomplete {
                code,
                reason,
                failed,
            }) => {
                let text = self.coded(code, &format!("{reason}."));
                self.once(text);
                self.unrestored(start.incomplete.as_ref(), failed, &[]);
                not_back = true;
            }
            Some(Outcome::Committed) | None => {}
        }
        for point in &start.unreadable {
            let text = self.unreadable(point);
            self.once(text);
        }
        for problem in &start.problems {
            let text = self.note(&self.coded(problem.code, &problem.message));
            self.once(text);
        }
        if let Some(record) = &start.incomplete {
            if !self.records.contains(&record.id) {
                self.err(format!(
                    "Note: the rollback of restore point {} did not finish.",
                    record.id
                ));
                self.unrestored(Some(record), &[], &[]);
            }
        }
        not_back
    }

    fn unreadable(&self, u: &UnreadablePoint) -> String {
        let (id, code, why) = (&u.point.id, u.point.failure.code, &u.point.failure.message);
        match &u.handled {
            Handled::SetAside => format!(
                "Note: {code}: restore point {id} could not be used ({why}). It was set \
                 aside, its files kept for a look, and an undo no longer goes back to it."
            ),
            Handled::LeftForLater => format!(
                "Note: {code}: restore point {id} can't be read right now ({why}). It was \
                 left as it is, and an undo does not go past it until it can be read."
            ),
            Handled::SetAsideFailed(f) => format!(
                "Note: {code}: restore point {id} could not be used ({why}), and setting it \
                 aside failed ({}: {}). An undo does not go past it.",
                f.code, f.message
            ),
        }
    }

    /// What a rollback could not put back, and how to fix each by hand (spec §5.5), from the
    /// core's record; without one, the targets come from `failed` and `ops`, and the fix is a
    /// clean-up.
    fn unrestored(&mut self, record: Option<&Incomplete>, failed: &[(usize, Failure)], ops: &[Op]) {
        self.err("E_ROLLBACK_INCOMPLETE: not everything could be put back. Fix these by hand:");
        if let Some(r) = record {
            self.records.push(r.id.clone());
        }
        match record {
            Some(r) if !r.steps.is_empty() => {
                for s in &r.steps {
                    self.err(format!(
                        "  could not put back: {} ({}: {})",
                        s.target, s.code, s.message
                    ));
                    self.err(format!("    fix: {}", s.fix));
                }
            }
            _ => {
                if failed.is_empty() {
                    self.err(
                        "  could not put back: what that change did (its journal could not be \
                         read)",
                    );
                }
                for (i, f) in failed {
                    let what = ops.get(*i).map_or_else(|| format!("step {i}"), op_label);
                    self.err(format!(
                        "  could not put back: {what} ({}: {})",
                        f.code, f.message
                    ));
                }
                self.err(format!("    fix: {CLEANUP_FIX_EN}"));
            }
        }
    }

    /// Says why the core refused or stopped a command, and returns the exit code: 2 for another
    /// Clink without `--replace-other-clink`, which the user must agree to first; else 1, since
    /// the user's settings were not changed. (A font file CMD Studio left to delete may already
    /// be gone: apply and undo try those first, so "nothing was changed" is never said.)
    fn refused(&mut self, sys: System<'_>, ctx: &Ctx, e: &ActionError) -> i32 {
        let code = e.code();
        match e {
            ActionError::Plan(PlanError::ForeignClink { existing }) => {
                self.err(format!(
                    "{code}: another Clink starts with cmd, from AutoRun. Turning it off takes out \
                     these commands:"
                ));
                for command in existing {
                    self.err(format!("  {command}"));
                }
                self.err(
                    "Your settings were not changed. To turn it off and use CMD Studio's Clink \
                     instead, run the same command again with --replace-other-clink (undo turns \
                     it back on).",
                );
                EXIT_INVALID
            }
            // The value is not text, so there is no command to show: what is wrong is said
            // instead.
            ActionError::Plan(PlanError::AutoRunNeedsHandEdit { part })
                if part == AUTORUN_NOT_TEXT =>
            {
                let key = COMMAND_PROCESSOR_KEY;
                self.err(format!(
                    "{code}: CMD Studio can't safely change AutoRun: {AUTORUN_NOT_TEXT}."
                ));
                self.err(format!(
                    "Your settings were not changed. Fix it by hand: in Registry Editor, \
                     open {key} and make AutoRun a String Value (REG_SZ) that holds the \
                     commands you want, or delete it if you do not need it. Then run this \
                     again."
                ));
                EXIT_FAILED
            }
            ActionError::Plan(PlanError::AutoRunNeedsHandEdit { part }) => {
                self.err(format!(
                    "{code}: CMD Studio can't safely change AutoRun because of this command:"
                ));
                self.err(format!("  {part}"));
                let key = COMMAND_PROCESSOR_KEY;
                self.err(format!(
                    "Your settings were not changed. Fix it by hand: in Registry Editor, \
                     open {key} and correct or take out that command in the AutoRun value. \
                     Then run this again."
                ));
                EXIT_FAILED
            }
            // What the user can do is reinstall; what exactly is missing is for whoever looks
            // closer (the core's text names a developer script).
            ActionError::Resources(why) => {
                self.err(format!(
                    "{code}: The files that come with CMD Studio are missing or damaged. \
                     Reinstall CMD Studio."
                ));
                self.err(format!("  details: {why}"));
                EXIT_FAILED
            }
            ActionError::RecoveryPending { .. } => {
                self.err(self.error_text(e));
                self.set_aside_way_out(sys, ctx);
                EXIT_FAILED
            }
            ActionError::PendingIncomplete(record) => {
                self.err(self.error_text(e));
                if !self.records.contains(&record.id) {
                    self.unrestored(Some(record), &[], &[]);
                }
                EXIT_FAILED
            }
            // A Thai user name is where these two happen, so they are worded here, with the
            // path shown as it is written.
            ActionError::Plan(PlanError::UnsupportedPath(path)) => {
                let shown = path.display();
                self.err(format!(
                    "{code}: cmd or Clink can't take this path: it holds %, a double quote or \
                     ;, or text that is not Unicode: \"{shown}\""
                ));
                // Every path the planner checks is in the data folder, which by default is
                // under the Windows user folder: the user name is what holds the character.
                self.err(
                    "Your settings were not changed. This path is in CMD Studio's data folder, \
                     which is under your Windows user folder unless --data-dir names another. \
                     Use a Windows account whose user folder has none of them, or give \
                     --data-dir a folder whose full path has none.",
                );
                EXIT_FAILED
            }
            ActionError::Plan(PlanError::NoClinkPath(path)) => {
                let shown = path.display();
                self.err(format!(
                    "{code}: Clink can't start from \"{shown}\", and no 8.3 or ProgramData \
                     path is available instead"
                ));
                self.next_step(code);
                EXIT_FAILED
            }
            // Its text says what to do already.
            ActionError::Busy => {
                self.err(self.error_text(e));
                EXIT_FAILED
            }
            _ => {
                self.err(self.error_text(e));
                self.next_step(code);
                EXIT_FAILED
            }
        }
    }

    /// What to do after a refusal whose text the core words: what the code means is in that
    /// text; this says that the user's settings were not changed, and what to do next.
    fn next_step(&mut self, code: &str) {
        self.err(match code {
            "E_WT_PARSE" => {
                "Your settings were not changed. Fix that settings.json by hand (Windows \
                 Terminal says where it is wrong when it starts), then try again."
            }
            "E_MACHINE_READ" | "E_SNAPSHOT_READ" => {
                "Your settings were not changed. Try again in a moment; `cmdstudio doctor` shows \
                 what can't be read."
            }
            "E_STATE_INVALID" => {
                "Your settings were not changed. Run `cmdstudio doctor`: its D9 says what to do."
            }
            "E_DATA_WRITE" | "E_SNAPSHOT_WRITE" => {
                "Your settings were not changed. Check that CMD Studio's data folder can be \
                 written (free disk space, permissions), then try again."
            }
            // `NoClinkPath`; `UnsupportedPath` has its own (`refused`).
            "E_CLINK_PATH" => {
                "Your settings were not changed. CMD Studio found no path on this computer that \
                 cmd can start Clink from safely, so it can't set up the prompt here."
            }
            "E_THEME_INVALID" => {
                "Your settings were not changed. Fix the theme file, then try again."
            }
            _ => "Your settings were not changed.",
        });
    }

    /// While the journal of a transaction a crash may have cut off can't be read, every change
    /// is refused; if it never becomes readable, `cmdstudio set-aside` is the way out, at the
    /// price of checking by hand (doctor's R1 says the same).
    fn set_aside_way_out(&mut self, sys: System<'_>, ctx: &Ctx) {
        let root = ctx.paths.snapshots();
        // Recovery looks at the newest restore point, so that is the one.
        let newest = SnapshotStore::new(sys.fs, &root)
            .ids()
            .ok()
            .and_then(|mut ids| ids.pop());
        let command = match &newest {
            Some(id) => format!("`cmdstudio set-aside {id}`"),
            None => "`cmdstudio set-aside <id>` (`cmdstudio doctor` names the id)".to_string(),
        };
        self.err(format!(
            "If it never becomes readable, run {command} to set that restore point aside. \
             What that change did is then not rolled back by itself, and must be checked by \
             hand: {CLEANUP_FIX_EN}"
        ));
    }

    /// Tells how a transaction ended and returns its exit code. `ops` are its ops, when known,
    /// for a rollback whose record could not be read.
    fn finish(
        &mut self,
        sys: System<'_>,
        ctx: &Ctx,
        outcome: &Outcome,
        ops: &[Op],
        done: String,
    ) -> i32 {
        match outcome {
            Outcome::Committed => {
                self.out(done);
                EXIT_OK
            }
            Outcome::RolledBack { code, reason } => {
                self.err(self.coded(code, reason));
                self.err("Everything was rolled back; your settings are as they were.");
                EXIT_FAILED
            }
            Outcome::RollbackIncomplete {
                code,
                reason,
                failed,
            } => {
                // When the list of what to fix could not be saved, the core's reason says so.
                self.err(self.coded(code, reason));
                // The record the core just wrote for this transaction (or, when it could not be
                // saved, rebuilt from its journal): the newest one.
                let record = match pending_incomplete(sys, ctx) {
                    Ok(record) => record,
                    Err(e) => {
                        let text = self.note(&self.coded(e.code(), &e.to_string()));
                        self.err(text);
                        None
                    }
                };
                self.unrestored(record.as_ref(), failed, ops);
                EXIT_INCOMPLETE
            }
        }
    }

    fn warning(&self, w: &Warning) -> String {
        let text = match w {
            Warning::NoWindowsTerminal => {
                "Windows Terminal is not installed: only the prompt and the welcome box change."
                    .to_string()
            }
            Warning::MachineWideClink => {
                "Clink is also set up for the whole machine; the theme's prompt may not work."
                    .to_string()
            }
            Warning::FontFallback { wanted } => format!(
                "The font \"{wanted}\" is not installed; Windows Terminal gets \
                 {NERD_FONT_FAMILY} instead."
            ),
        };
        format!("Warning: {text}")
    }

    fn apply(&mut self, sys: System<'_>, ctx: &Ctx, path: &Path, req: &Request) -> i32 {
        if let Err(code) = self.begin(sys, ctx, true) {
            return code;
        }
        let shown = path.display();
        let bytes = match sys.fs.read(path) {
            Ok(Some(bytes)) => bytes,
            Ok(None) => {
                self.err(format!("E_THEME_INVALID: {shown} does not exist"));
                return EXIT_INVALID;
            }
            Err(e) => {
                self.err(format!("E_THEME_INVALID: {shown} could not be read: {e}"));
                return EXIT_INVALID;
            }
        };
        let theme = match parse_theme(&bytes) {
            Ok(theme) => theme,
            Err(e) => {
                self.err(self.coded(e.code(), &e.to_string()));
                return EXIT_INVALID;
            }
        };
        let prepared = match prepare_apply(sys, ctx, &theme, req) {
            Ok(prepared) => prepared,
            Err(e) => return self.refused(sys, ctx, &e),
        };
        // Its own start found what the CLI's did, so only something new is said; one that left
        // a rollback unfinished just now stops it (and lets the lock go).
        if self.start(&prepared.start) {
            return EXIT_INCOMPLETE;
        }
        // Names are shown as they are, in quotes: `{:?}` would escape Thai vowels and tone marks.
        let name = &theme.name;
        self.out(format!("Plan for \"{name}\":"));
        for line in &prepared.plan.summary_en {
            self.out(format!("  - {line}"));
        }
        for w in &prepared.plan.warnings {
            self.out(self.warning(w));
        }
        let ops = prepared.ops.clone();
        let outcome = run_apply(sys, ctx, prepared);
        // A theme applied is the theme on again (recovery spec §4.1): its pause goes, and with
        // it every start tag of the theme before. This comes before anything is said, so that the
        // hint to open a new window is not said when the lift failed. The apply stands either way.
        let lift = if outcome == Outcome::Committed {
            clear_pause(sys.fs, &ctx.paths).err()
        } else {
            None
        };
        let done = match (&lift, req.target) {
            // The line on stderr says what to do instead.
            (Some(_), _) => format!("Applied \"{name}\"."),
            (None, Target::All) => format!(
                "Applied \"{name}\". Open a new cmd window to see the prompt and the welcome \
                 box."
            ),
            (None, Target::Wt) => format!(
                "Applied \"{name}\". Open a new cmd tab in Windows Terminal to see the prompt \
                 and the welcome box."
            ),
        };
        let code = self.finish(sys, ctx, &outcome, &ops, done);
        if let Some(e) = lift {
            // `clear_pause` fails at a tag it cannot delete (the pause stays on purpose, or three
            // stale tags would pause the next window at once), at the listing, at paused.json or
            // at making the folder; there may be no pause at all. `{e}` names the file, so the
            // advice is for the folder and does not name a pause.
            self.err(format!(
                "E_DATA_WRITE: the theme was applied, but CMD Studio's start records in {} could \
                 not be cleared ({e}); if cmd windows open without the theme, delete the files \
                 in that folder, or turn the theme back on in CMD Studio",
                ctx.paths.health().display()
            ));
        }
        code
    }

    fn undo(&mut self, sys: System<'_>, ctx: &Ctx) -> i32 {
        if let Err(code) = self.begin(sys, ctx, true) {
            return code;
        }
        let UndoReport {
            start,
            result,
            problems,
            fonts_left,
        } = match undo(sys, ctx) {
            Ok(report) => report,
            Err(e) => return self.refused(sys, ctx, &e),
        };
        if self.start(&start) {
            return EXIT_INCOMPLETE;
        }
        let code = match result {
            UndoResult::NothingToUndo => {
                self.out("Nothing to undo.");
                EXIT_OK
            }
            UndoResult::Done { undid, outcome } => {
                let done = format!(
                    "Undid the theme applied in restore point {undid}: what it changed is \
                     back as it was. Open a new cmd window to see it."
                );
                self.finish(sys, ctx, &outcome, &[], done)
            }
        };
        // What the undo could not do besides its transaction; the result stands.
        for problem in &problems {
            self.err(self.coded(problem.code, &problem.message));
        }
        // A font the user installed since stays installed.
        for left in &fonts_left {
            self.err(format!("E_FONT_INSTALL: {}", left.en()));
        }
        code
    }

    /// `--cleanup` with `--data-dir` deletes every entry of that folder but `themes\` (core
    /// `remove_data`), so a mistyped path must never reach it. The folder is taken for CMD
    /// Studio's only when it is absent or empty, or when every entry in it is one that CMD
    /// Studio itself puts there, of the right kind (see [`put_there_by_cmd_studio`]), and one of
    /// them is a file of CMD Studio's own (see [`cmd_studios_own_file`]): folders that are only
    /// named like CMD Studio's could be anybody's. One that holds only `themes\` is taken too,
    /// since a `--cleanup` that kept the themes leaves that, but not with `--remove-themes`: such
    /// a folder could be anybody's, and that form would delete it. Anything else is refused
    /// before anything is created or deleted, naming the entries that are not CMD Studio's, or
    /// the files of its own that are missing.
    fn check_data_dir(
        &mut self,
        sys: System<'_>,
        ctx: &Ctx,
        remove_themes: bool,
    ) -> Result<(), i32> {
        let data = &ctx.paths.data;
        let entries = match sys.fs.list_dir(data) {
            Ok(entries) => entries,
            Err(e) => {
                self.err(self.coded(
                    "E_MACHINE_READ",
                    &format!(
                        "what {} holds could not be listed ({e}), so nothing was deleted",
                        data.display()
                    ),
                ));
                return Err(EXIT_FAILED);
            }
        };
        let foreign: Vec<String> = entries
            .iter()
            .filter(|entry| !put_there_by_cmd_studio(&ctx.paths, entry))
            .map(|entry| {
                let name = entry
                    .path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                // A folder is shown with a backslash, so that its kind shows.
                if entry.is_dir {
                    format!("\"{name}\\\"")
                } else {
                    format!("\"{name}\"")
                }
            })
            .collect();
        if foreign.is_empty() {
            let themes = ctx.paths.themes();
            let only_themes = !entries.is_empty()
                && entries
                    .iter()
                    .all(|entry| entry.is_dir && same_name(&entry.path, &themes));
            // Anything more than themes\ must hold one of the files CMD Studio itself writes
            // there: folders that are only named like its own could be anybody's.
            let own_file = entries
                .iter()
                .any(|entry| !entry.is_dir && cmd_studios_own_file(&ctx.paths, entry));
            if entries.is_empty() || (only_themes && !remove_themes) || (!only_themes && own_file) {
                return Ok(());
            }
            if !only_themes {
                self.err(format!(
                    "cmdstudio: {} holds none of CMD Studio's own files (cmdstudio.lock, \
                     state.json, settings.json or a .damaged-<number> copy of them), so it may not be CMD \
                     Studio's data folder, and --cleanup would delete what is in it. Nothing \
                     was deleted; check the path after --data-dir.",
                    data.display()
                ));
                return Err(EXIT_INVALID);
            }
            self.err(format!(
                "cmdstudio: {} holds only a themes folder and nothing else of CMD Studio's, \
                 so it may not be CMD Studio's data folder, and --remove-themes would delete \
                 that folder. Nothing was deleted. Check the path after --data-dir; without \
                 --remove-themes, --cleanup keeps the themes folder.",
                data.display()
            ));
            return Err(EXIT_INVALID);
        }
        const SHOWN: usize = 3;
        let named: Vec<&str> = foreign.iter().take(SHOWN).map(String::as_str).collect();
        let more = foreign.len().saturating_sub(SHOWN);
        let (named, data) = (named.join(", "), data.display());
        let more = if more > 0 {
            format!(" and {more} more")
        } else {
            String::new()
        };
        self.err(format!(
            "cmdstudio: {data} holds what CMD Studio does not put in its data folder: \
             {named}{more}. --cleanup deletes everything in that folder but themes\\, so \
             nothing was deleted. Move them out of the folder, or check the path after \
             --data-dir."
        ));
        Err(EXIT_INVALID)
    }

    fn cleanup(
        &mut self,
        sys: System<'_>,
        ctx: &Ctx,
        remove_themes: bool,
        data_dir_given: bool,
    ) -> i32 {
        if data_dir_given {
            if let Err(code) = self.check_data_dir(sys, ctx, remove_themes) {
                return code;
            }
        }
        // A rollback a crash left unfinished does not stop it: the clean-up checks what is left.
        if let Err(code) = self.begin(sys, ctx, false) {
            return code;
        }
        let report = match cleanup(sys, ctx, remove_themes) {
            Ok(report) => report,
            Err(e) => return self.refused(sys, ctx, &e),
        };
        self.start(&report.start);
        // The AutoRun and settings.json transaction has no step of its own; it committed only
        // after reading back what it wrote.
        if report.outcome == Some(Outcome::Committed) {
            self.out(
                "done   AutoRun and Windows Terminal's settings: CMD Studio's part is taken out, \
                 and read back to check",
            );
        }
        self.steps(&report);
        match &report.outcome {
            Some(Outcome::RolledBack { code, reason }) => {
                self.err(self.coded(code, reason));
                self.err("The revert was rolled back, so CMD Studio's data was not deleted.");
                EXIT_FAILED
            }
            Some(outcome @ Outcome::RollbackIncomplete { .. }) => {
                self.finish(sys, ctx, outcome, &[], String::new())
            }
            _ if !report.ok() => {
                self.err(CLEANUP_AGAIN_EN);
                EXIT_INCOMPLETE
            }
            _ => {
                let themes = ctx.paths.themes();
                let kept = !remove_themes && sys.fs.is_dir(&themes);
                let shown = themes.display();
                self.out(if kept {
                    format!(
                        "CMD Studio's changes are taken out, and its data is deleted; your \
                         themes are kept in {shown}."
                    )
                } else {
                    "CMD Studio's changes are taken out, and its data is deleted.".to_string()
                });
                EXIT_OK
            }
        }
    }

    /// One line per step of a clean-up: what went well on stdout, what failed on stderr with
    /// its code (the step's detail starts with it) and how to finish it by hand. Step texts are
    /// the core's English.
    fn steps(&mut self, report: &Report) {
        for step in &report.steps {
            if step.ok {
                self.out(format!("done   {}: {}", step.what, step.detail));
                continue;
            }
            // The detail starts with the code.
            let detail = match step.code {
                Some(code) => {
                    let message = step
                        .detail
                        .strip_prefix(code)
                        .and_then(|rest| rest.strip_prefix(": "))
                        .unwrap_or(&step.detail);
                    self.coded(code, message)
                }
                None => step.detail.clone(),
            };
            self.err(format!("FAILED {}: {detail}", step.what));
            if let Some(how) = &step.fix {
                self.err(format!("    fix: {}", how.en));
            }
        }
    }

    /// `cmdstudio set-aside <id>`: the way out when a restore point's journal can never be read
    /// again, so every change is refused (doctor's R1). Under the lock it sets aside only a
    /// restore point the listing has and that cannot be read; one that reads is refused, since
    /// CMD Studio can still roll it back or undo it.
    fn set_aside(&mut self, sys: System<'_>, ctx: &Ctx, id: &str) -> i32 {
        if let Err(code) = self.begin(sys, ctx, false) {
            return code;
        }
        let _lock = match sys.fs.try_lock(&ctx.paths.lock_file()) {
            Ok(Some(lock)) => lock,
            Ok(None) => return self.refused(sys, ctx, &ActionError::Busy),
            Err(e) => return self.refused(sys, ctx, &ActionError::Lock(e)),
        };
        let root = ctx.paths.snapshots();
        let store = SnapshotStore::new(sys.fs, &root);
        let ids = match store.ids() {
            Ok(ids) => ids,
            Err(f) => {
                self.err(self.coded(f.code, &f.message));
                return EXIT_FAILED;
            }
        };
        // Only a name the listing gives: the id typed is never made into a path of its own.
        if !ids.iter().any(|known| known == id) {
            self.err(format!(
                "cmdstudio: there is no restore point {id} in {}. `cmdstudio doctor` names \
                 the one to set aside.",
                root.display()
            ));
            return EXIT_INVALID;
        }
        if store.is_set_aside(id) {
            self.out(format!("Restore point {id} is already set aside."));
            return EXIT_OK;
        }
        let why = match store.manifest(id).and_then(|_| store.journal(id)) {
            Ok(_) => {
                self.err(format!(
                    "Restore point {id} can be read, so it is not set aside: CMD Studio can \
                     still roll back or undo what it changed. It was left as it is."
                ));
                return EXIT_FAILED;
            }
            Err(why) => why,
        };
        if let Err(f) = store.set_aside(id) {
            self.err(self.coded(f.code, &f.message));
            return EXIT_FAILED;
        }
        // The newest one is the transaction recovery would have rolled back. Like the start of
        // a command does for one it sets aside itself, its record keeps saying what to check by
        // hand, on every start and in doctor, until a newer transaction runs.
        let newest = ids.last().is_some_and(|last| last == id);
        if newest && store.set_aside_as(id) == Some(SetAside::WithJournal) {
            if let Err(f) = record_set_aside(&store, id, &why) {
                let text = self.note(&self.coded(f.code, &f.message));
                self.err(text);
            }
        }
        self.out(format!(
            "Restore point {id} is set aside; its files stay for a look. What that change did \
             is not rolled back by itself any more, and an undo does not go back to it. Check \
             by hand: {CLEANUP_FIX_EN}"
        ));
        EXIT_OK
    }

    fn doctor(&mut self, sys: System<'_>, ctx: &Ctx, json: bool) -> i32 {
        let checks = doctor(sys, ctx);
        if json {
            match checks_json(&checks).and_then(|v| serde_json::to_string_pretty(&v)) {
                Ok(text) => self.out(text),
                Err(e) => {
                    self.err(format!("cmdstudio: {e}"));
                    return EXIT_FAILED;
                }
            }
        } else {
            for c in &checks {
                let status = match c.status {
                    Status::Ok => "ok",
                    Status::Warn => "warn",
                    Status::Fail => "FAIL",
                    Status::Skipped => "skip",
                };
                let message = &c.message_en;
                let code = c.code.map(|c| format!(" [{c}]")).unwrap_or_default();
                self.out(format!("{:<4} {status:<5} {message}{code}", c.id));
                for line in c.details.lines() {
                    self.out(format!("{:11}{line}", ""));
                }
            }
        }
        // An R entry that fails counts like a D check.
        if checks.iter().any(|c| c.status == Status::Fail) {
            EXIT_FAILED
        } else {
            EXIT_OK
        }
    }
}

/// What `doctor --json` prints: `{"checks": [...]}`, each check with its `id`, `status`, `code`,
/// `messageEn` and `details`. The core's [`Check`] also holds its Thai texts (`message_th`,
/// `details_th`), which are for the app's own UI: nothing CMD Studio prints in cmd is Thai.
fn checks_json(checks: &[Check]) -> Result<serde_json::Value, serde_json::Error> {
    use serde_json::{Map, Value};
    let list = checks
        .iter()
        .map(|c| {
            let mut check = Map::new();
            check.insert("id".to_string(), Value::from(c.id));
            check.insert("status".to_string(), serde_json::to_value(c.status)?);
            check.insert("code".to_string(), c.code.map_or(Value::Null, Value::from));
            check.insert("messageEn".to_string(), Value::from(c.message_en.as_str()));
            check.insert("details".to_string(), Value::from(c.details.as_str()));
            Ok(Value::Object(check))
        })
        .collect::<Result<Vec<Value>, serde_json::Error>>()?;
    let mut all = Map::new();
    all.insert("checks".to_string(), Value::Array(list));
    Ok(Value::Object(all))
}

/// Whether two paths end in the same name, without regard to ASCII letter case, as Windows
/// compares names.
fn same_name(a: &Path, b: &Path) -> bool {
    match (
        a.file_name().and_then(|n| n.to_str()),
        b.file_name().and_then(|n| n.to_str()),
    ) {
        (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
        _ => false,
    }
}

/// Whether an entry directly in the data folder is one CMD Studio itself puts there, of the
/// kind it writes, as the core's `Paths` and its writers make them:
/// - the folders `generated\`, `clink-profile\`, `runtime\`, `snapshots\`, `themes\`, and the
///   app's `drafts\` (autosaved drafts), `logs\` (its log), `webview\` (WebView2's own data,
///   put there by the app on purpose; spec §5.3), and `health\` (init.lua's start tags and the
///   theme's pause, recovery spec §7);
/// - the files `state.json`, `settings.json` (the app's settings) and `cmdstudio.lock` (made by
///   `Fs::try_lock`);
/// - `state.json.damaged-<ms>` and `settings.json.damaged-<ms>`: a damaged file that Revert
///   everything, `--cleanup` or the app set aside (`cleanup::load_state`, the app's settings);
/// - `.state.json.<32 hex digits>.cmdstudio-tmp`, and the same for `settings.json`: the temp
///   file `WinFs::write_atomic` writes beside the file, left behind when a run is cut off in
///   that write. state.json and settings.json are the only files CMD Studio writes directly in
///   the data folder: the generated files, Clink's profile, the runtimes, the restore points, the
///   drafts, the log and the pause, with their own temp files, are in the folders above.
///
/// Names compare without regard to ASCII letter case, as Windows does.
fn put_there_by_cmd_studio(paths: &Paths, entry: &DirEntry) -> bool {
    let Some(name) = entry.path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let named = |ours: PathBuf| same_name(&ours, &entry.path);
    if entry.is_dir {
        return [
            paths.generated(),
            paths.clink_profile(),
            paths.runtime_root(),
            paths.snapshots(),
            paths.themes(),
            paths.drafts(),
            paths.logs(),
            paths.webview(),
            paths.health(),
        ]
        .into_iter()
        .any(named);
    }
    if cmd_studios_own_file(paths, entry) {
        return true;
    }
    let name = name.to_ascii_lowercase();
    [paths.state_file(), paths.settings_file()]
        .iter()
        .filter_map(|file| file.file_name().and_then(|n| n.to_str()))
        .any(|file| {
            name.strip_prefix(&format!(".{}.", file.to_ascii_lowercase()))
                .and_then(|rest| rest.strip_suffix(".cmdstudio-tmp"))
                .is_some_and(|hex| hex.len() == 32 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
        })
}

/// Whether an entry directly in the data folder is one of the files that only CMD Studio
/// itself writes there, and that every data folder it used holds until a `--cleanup` empties
/// it: `cmdstudio.lock` (made by the first command that takes the lock, before any other write,
/// and left when the lock is let go), `state.json`, `settings.json` (the app writes it the first
/// time a setting changes), or a damaged copy of the last two set aside as `<name>.damaged-<ms>`.
/// Names compare without regard to ASCII letter case. The kind is the caller's to check.
fn cmd_studios_own_file(paths: &Paths, entry: &DirEntry) -> bool {
    let own = [paths.state_file(), paths.settings_file(), paths.lock_file()];
    if own.iter().any(|ours| same_name(ours, &entry.path)) {
        return true;
    }
    let Some(name) = entry.path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let name = name.to_ascii_lowercase();
    [paths.state_file(), paths.settings_file()]
        .iter()
        .filter_map(|file| file.file_name().and_then(|n| n.to_str()))
        .any(|file| {
            name.strip_prefix(&format!("{}.damaged-", file.to_ascii_lowercase()))
                .is_some_and(|ms| !ms.is_empty() && ms.bytes().all(|b| b.is_ascii_digit()))
        })
}

/// The record of a restore point set aside by hand while its journal could not be read: one
/// step for the whole change, with the shared clean-up advice, as the core records one it sets
/// aside itself. A record that is already there, or cannot be read, is left as it is.
fn record_set_aside(store: &SnapshotStore<'_>, id: &str, why: &Failure) -> Result<(), Failure> {
    if !matches!(store.incomplete(id), Ok(None)) {
        return Ok(());
    }
    store.write_incomplete(&Incomplete {
        id: id.to_string(),
        code: why.code.to_string(),
        reason: format!(
            "restore point {id} was set aside by hand while its journal could not be read ({}), \
             so its change was not checked or rolled back",
            why.message
        ),
        steps: vec![Unrestored {
            op_index: 0,
            target: format!("every change of restore point {id}"),
            code: why.code.to_string(),
            message: why.message.clone(),
            fix: CLEANUP_FIX_EN.to_string(),
            fix_th: CLEANUP_FIX_TH.to_string(),
            restore_point: None,
        }],
    })
}

/// Milliseconds since 1970, for restore point ids.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

/// Runs one command on this machine (spec §5.7).
#[cfg(windows)]
pub fn run(args: &[OsString], io: &mut Io<'_>) -> i32 {
    use cmdstudio_core::adapters::windows::{
        fonts::WinFonts, fs::WinFs, host::WinHost, process::WinProcess, registry::WinAutoRun,
        wt::WinWt,
    };
    use cmdstudio_core::adapters::{program_data_dir, Host};
    let host = WinHost;
    let local = match host.local_app_data() {
        Ok(path) => path,
        Err(e) => {
            let mut say = Say::new(io);
            let text = say.coded(
                "E_MACHINE_READ",
                &format!("the user's local app data folder could not be found: {e}"),
            );
            say.err(text);
            return EXIT_FAILED;
        }
    };
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(PathBuf::from))
        .unwrap_or_default();
    let defaults = Defaults {
        data_dir: local.join(DATA_DIR_NAME),
        resources: exe_dir.join("resources"),
        now_ms: now_ms(),
    };
    // Only programs from the runtime folders may run (spec §5.10), each for 60 seconds at most.
    let data = parse(args)
        .ok()
        .and_then(|a| a.data_dir)
        .unwrap_or_else(|| defaults.data_dir.clone());
    let mut allowed = vec![data.join("runtime")];
    if let (Ok(pd), Ok(sid)) = (host.program_data(), host.user_sid()) {
        allowed.push(program_data_dir(&pd, &sid));
    }
    let (fs, autorun, fonts, wt) = (
        WinFs,
        WinAutoRun::real(),
        WinFonts::real(&local),
        WinWt::new(local.clone()),
    );
    let process = WinProcess::new(allowed, std::time::Duration::from_secs(60));
    let sys = System {
        fs: &fs,
        autorun: &autorun,
        fonts: &fonts,
        wt: &wt,
        process: &process,
        host: &host,
    };
    run_with(args, io, sys, &defaults)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cmdstudio_core::actions::RECORD_NOT_SAVED_EN;
    use cmdstudio_core::adapters::fake::FakeMachine;
    use cmdstudio_core::adapters::{AutoRun, Fs, RegValue, WtLocation, WtVariant, REG_SZ};
    use std::path::Path;

    const WT: &str = r"C:\Users\tester\AppData\Local\Packages\Microsoft.WindowsTerminal_8wekyb3d8bbwe\LocalState\settings.json";
    const THEME: &str = r"C:\themes\terracotta.cmdtheme";

    fn machine() -> (FakeMachine, Defaults) {
        let mut m = FakeMachine::new();
        m.fs.create_dir_all(Path::new(
            r"C:\Users\tester\AppData\Local\Microsoft\Windows",
        ))
        .unwrap();
        m.fs.put(WT, "{\n    \"profiles\": { \"list\": [] }\n}\n");
        m.fs.put(
            THEME,
            include_bytes!("../../../presets/terracotta.cmdtheme"),
        );
        m.wt.locations = vec![WtLocation {
            variant: WtVariant::Store,
            installed: true,
            settings: PathBuf::from(WT),
        }];
        m.process.answer("oh-my-posh.exe", "print", 0, "> ");
        m.process.answer("oh-my-posh.exe", "version", 0, "31.5.0");
        let res = PathBuf::from(r"C:\Program Files\CMD Studio\resources");
        cmdstudio_core::resources::fake_resources(&m.fs, &res);
        let d = Defaults {
            data_dir: PathBuf::from(r"C:\Users\tester\AppData\Local\ai.resincore.cmdstudio"),
            resources: res,
            now_ms: 1_791_694_709_000,
        };
        (m, d)
    }

    fn cli(m: &FakeMachine, d: &Defaults, args: &[&str]) -> (i32, String, String) {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let args: Vec<OsString> = args.iter().map(OsString::from).collect();
        let code = run_with(
            &args,
            &mut Io {
                out: &mut out,
                err: &mut err,
            },
            m.system(),
            d,
        );
        (
            code,
            String::from_utf8_lossy(&out).into_owned(),
            String::from_utf8_lossy(&err).into_owned(),
        )
    }

    fn later(d: &Defaults, ms: u64) -> Defaults {
        Defaults {
            now_ms: d.now_ms + ms,
            ..d.clone()
        }
    }

    /// `GetUserDefaultUILanguage` of Thai Windows.
    const THAI_WINDOWS: u16 = 0x041E;

    /// The Thai characters in `text`, in order.
    fn thai(text: &str) -> String {
        text.chars()
            .filter(|c| ('\u{0E00}'..='\u{0E7F}').contains(c))
            .collect()
    }

    /// `machine()` with Windows in Thai. Its user name and every path are ASCII, so a Thai
    /// character in what the CLI says could only be text of CMD Studio's own.
    fn thai_windows() -> (FakeMachine, Defaults) {
        let (mut m, d) = machine();
        m.host.info.ui_language = THAI_WINDOWS;
        (m, d)
    }

    /// `machine()` on English Windows, then on Thai Windows: the CLI says the same on both.
    fn english_and_thai_windows() -> [(FakeMachine, Defaults); 2] {
        [machine(), thai_windows()]
    }

    #[test]
    fn apply_doctor_undo_round_trip() {
        let (m, d) = machine();
        let before = (m.fs.file(WT), m.autorun.snapshot());
        let (code, out, err) = cli(&m, &d, &["apply", THEME]);
        assert_eq!(code, EXIT_OK, "{out}{err}");
        assert!(out.contains("Plan for \"Terracotta\":\n  - "), "{out}");
        let (code, out, _) = cli(&m, &later(&d, 1), &["doctor", "--json"]);
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["checks"].as_array().unwrap().len(), 11);
        // English only: the Thai text of a check is for the app's own UI.
        assert!(v["checks"][0]["messageEn"].is_string(), "{out}");
        assert!(v["checks"][0].get("messageTh").is_none(), "{out}");
        assert_eq!(code, EXIT_OK, "{out}");
        let (code, out, err) = cli(&m, &later(&d, 2), &["undo"]);
        assert_eq!(code, EXIT_OK, "{out}{err}");
        assert_eq!((m.fs.file(WT), m.autorun.snapshot()), before);
        assert_eq!(cli(&m, &later(&d, 3), &["undo"]).1, "Nothing to undo.\n");
    }

    #[test]
    fn apply_sets_up_a_terminal_that_was_never_opened_and_undo_and_cleanup_take_it_out() {
        // Founder 2026-10-17: Windows Terminal is installed but has no settings.json yet. The
        // apply makes it and keeps its exit codes; undo and --cleanup delete it again.
        let (m, d) = machine();
        m.fs.remove_file(Path::new(WT)).unwrap();
        let (code, out, err) = cli(&m, &d, &["apply", THEME]);
        assert_eq!(code, EXIT_OK, "{out}{err}");
        assert!(
            out.contains("\n  - Set up Windows Terminal's settings so cmd gets this theme\n"),
            "{out}"
        );
        assert!(
            !out.contains("Warning: Windows Terminal is not installed"),
            "{out}"
        );
        assert!(m.fs.exists(Path::new(WT)));
        let (code, out, err) = cli(&m, &later(&d, 1), &["undo"]);
        assert_eq!(code, EXIT_OK, "{out}{err}");
        assert!(!m.fs.exists(Path::new(WT)));
        assert_eq!(cli(&m, &later(&d, 2), &["apply", THEME]).0, EXIT_OK);
        assert!(m.fs.exists(Path::new(WT)));
        let (code, out, err) = cli(&m, &later(&d, 3), &["--cleanup"]);
        assert_eq!(code, EXIT_OK, "{out}{err}");
        assert!(!m.fs.exists(Path::new(WT)));
        // Without any Windows Terminal installed, the warning it always was.
        let (mut m, d) = machine();
        m.wt.locations.clear();
        let (code, out, _) = cli(&m, &d, &["apply", THEME]);
        assert_eq!(code, EXIT_OK);
        assert!(
            out.contains("Warning: Windows Terminal is not installed"),
            "{out}"
        );
        assert!(!out.contains("Set up Windows Terminal"), "{out}");
    }

    #[test]
    fn invalid_input_exits_2() {
        let (m, d) = machine();
        assert_eq!(cli(&m, &d, &["apply"]).0, EXIT_INVALID);
        assert_eq!(cli(&m, &d, &["apply", r"C:\nope.cmdtheme"]).0, EXIT_INVALID);
        m.fs.put(THEME, b"not json");
        let (code, _, err) = cli(&m, &d, &["apply", THEME]);
        assert_eq!(code, EXIT_INVALID);
        assert!(err.starts_with("E_THEME_INVALID"), "{err}");
    }

    #[test]
    fn a_foreign_clink_needs_the_flag() {
        let (m, d) = machine();
        let other = r#""C:\clink\clink.bat" inject --autorun"#;
        m.autorun
            .write_user(&RegValue::from_text(REG_SZ, other))
            .unwrap();
        let before = m.autorun.snapshot();
        let (code, _, err) = cli(&m, &d, &["apply", THEME]);
        assert_eq!(code, EXIT_INVALID);
        assert!(err.starts_with("E_AUTORUN_FOREIGN_CLINK: "), "{err}");
        assert!(err.contains(&format!("\n  {other}\n")), "{err}");
        assert!(err.contains("--replace-other-clink"), "{err}");
        assert!(
            err.contains("\nYour settings were not changed. To turn it off"),
            "{err}"
        );
        assert_eq!(m.autorun.snapshot(), before);
        assert_eq!(
            cli(&m, &d, &["apply", THEME, "--replace-other-clink"]).0,
            EXIT_OK
        );
    }

    #[test]
    fn an_autorun_cmd_rejects_needs_a_hand_edit_and_nothing_changes() {
        let (m, d) = machine();
        m.autorun
            .write_user(&RegValue::from_text(REG_SZ, "echo A |"))
            .unwrap();
        let before = (m.fs.file(WT), m.autorun.snapshot());
        let (code, _, err) = cli(&m, &d, &["apply", THEME]);
        assert_eq!(code, EXIT_FAILED, "{err}");
        assert!(err.starts_with("E_AUTORUN_WRITE: "), "{err}");
        assert!(err.contains("\n  echo A |\n"), "{err}");
        assert!(err.contains("Registry Editor"), "{err}");
        assert_eq!((m.fs.file(WT), m.autorun.snapshot()), before);
    }

    #[test]
    fn an_autorun_value_that_is_not_text_needs_a_hand_edit_and_nothing_changes() {
        let dword = RegValue {
            kind: 4,
            bytes: vec![1, 0, 0, 0],
        };
        for (m, d) in english_and_thai_windows() {
            m.autorun.write_user(&dword).unwrap();
            let before = (m.fs.file(WT), m.autorun.snapshot());
            let (code, out, err) = cli(&m, &d, &["apply", THEME]);
            assert_eq!(code, EXIT_FAILED, "{out}{err}");
            assert!(err.starts_with("E_AUTORUN_WRITE: "), "{err}");
            assert!(err.contains(COMMAND_PROCESSOR_KEY), "{err}");
            assert!(err.contains("not plain text"), "{err}");
            assert!(err.contains("Your settings were not changed."), "{err}");
            assert_eq!(thai(&err), "", "{err}");
            assert_eq!((m.fs.file(WT), m.autorun.snapshot()), before);
        }
    }

    /// The fake disk, but no `incomplete.json` can be written.
    struct NoRecords<'a>(&'a cmdstudio_core::adapters::fake::FakeFs);

    impl Fs for NoRecords<'_> {
        fn read(
            &self,
            path: &Path,
        ) -> Result<Option<Vec<u8>>, cmdstudio_core::adapters::AdapterError> {
            self.0.read(path)
        }
        fn write_atomic(
            &self,
            path: &Path,
            bytes: &[u8],
        ) -> Result<(), cmdstudio_core::adapters::AdapterError> {
            if path.file_name() == Some(std::ffi::OsStr::new("incomplete.json")) {
                return Err(cmdstudio_core::adapters::AdapterError::new(
                    path.display().to_string(),
                    "denied (test)",
                ));
            }
            self.0.write_atomic(path, bytes)
        }
        fn remove_file(&self, path: &Path) -> Result<(), cmdstudio_core::adapters::AdapterError> {
            self.0.remove_file(path)
        }
        fn exists(&self, path: &Path) -> bool {
            self.0.exists(path)
        }
        fn is_dir(&self, path: &Path) -> bool {
            self.0.is_dir(path)
        }
        fn create_dir_all(
            &self,
            path: &Path,
        ) -> Result<(), cmdstudio_core::adapters::AdapterError> {
            self.0.create_dir_all(path)
        }
        fn remove_dir(&self, path: &Path) -> Result<(), cmdstudio_core::adapters::AdapterError> {
            self.0.remove_dir(path)
        }
        fn remove_dir_all(
            &self,
            path: &Path,
        ) -> Result<(), cmdstudio_core::adapters::AdapterError> {
            self.0.remove_dir_all(path)
        }
        fn rename(
            &self,
            from: &Path,
            to: &Path,
        ) -> Result<(), cmdstudio_core::adapters::AdapterError> {
            self.0.rename(from, to)
        }
        fn list_dir(
            &self,
            path: &Path,
        ) -> Result<Vec<DirEntry>, cmdstudio_core::adapters::AdapterError> {
            self.0.list_dir(path)
        }
        fn try_lock(
            &self,
            path: &Path,
        ) -> Result<Option<cmdstudio_core::adapters::FsLock>, cmdstudio_core::adapters::AdapterError>
        {
            self.0.try_lock(path)
        }
    }

    #[test]
    fn a_fix_list_that_could_not_be_saved_is_said_in_english_on_every_windows() {
        for (m, d) in english_and_thai_windows() {
            let init = d.data_dir.join(r"generated\init.lua");
            m.fs.put(&init, b"old");
            m.fs.fail_writes_to(&init);
            let fs = NoRecords(&m.fs);
            let sys = System {
                fs: &fs,
                ..m.system()
            };
            let (mut out, mut err) = (Vec::new(), Vec::new());
            let args: Vec<OsString> = ["apply", THEME].iter().map(OsString::from).collect();
            let code = run_with(
                &args,
                &mut Io {
                    out: &mut out,
                    err: &mut err,
                },
                sys,
                &d,
            );
            let err = String::from_utf8_lossy(&err).into_owned();
            assert_eq!(code, EXIT_INCOMPLETE, "{err}");
            assert!(err.contains(RECORD_NOT_SAVED_EN), "{err}");
            assert_eq!(thai(&err), "", "{err}");
            // What to fix is said all the same, from the journal.
            assert!(err.contains(&format!("file {}", init.display())), "{err}");
        }
    }

    #[test]
    fn an_undo_says_it_left_a_font_the_user_installed_over_ours() {
        use cmdstudio_core::adapters::fake::FakeFonts;
        use cmdstudio_core::plan::NERD_FONT_FAMILY;
        use cmdstudio_core::resources::NERD_FONT_FILE;
        for (mut m, d) in english_and_thai_windows() {
            let fonts = r"C:\Users\tester\AppData\Local\Microsoft\Windows\Fonts";
            // No Cascadia Mono, so the apply installs the bundled font.
            m.fonts = FakeFonts::new(fonts, &["Arial"]);
            m.fonts.knows_file(NERD_FONT_FILE, NERD_FONT_FAMILY);
            assert_eq!(cli(&m, &d, &["apply", THEME]).0, EXIT_OK);
            let file = Path::new(fonts).join(NERD_FONT_FILE);
            assert!(m.fs.exists(&file));
            m.fs.put(&file, b"the user's own build");
            let (code, out, err) = cli(&m, &later(&d, 1), &["undo"]);
            assert_eq!(code, EXIT_OK, "{out}{err}");
            let shown = file.display().to_string();
            let line = err
                .lines()
                .find(|l| l.starts_with("E_FONT_INSTALL: "))
                .unwrap_or_else(|| panic!("{err}"));
            assert!(line.contains(&shown), "{line}");
            assert!(line.contains("was left installed"), "{line}");
            assert_eq!(thai(&err), "", "{err}");
            assert_eq!(m.fs.file(&file), Some(b"the user's own build".to_vec()));
        }
    }

    #[test]
    fn a_failed_verify_exits_1_and_a_failed_rollback_exits_3() {
        let (m, d) = machine();
        m.process
            .answer("oh-my-posh.exe", "print", 0, " CONFIG PARSE ERROR ");
        let (code, _, err) = cli(&m, &d, &["apply", THEME]);
        assert_eq!(code, EXIT_FAILED);
        assert!(err.starts_with("E_VERIFY_OMP"), "{err}");

        let (m, d) = machine();
        let init = d.data_dir.join(r"generated\init.lua");
        m.fs.put(&init, b"old");
        m.fs.fail_writes_to(&init);
        let (code, _, err) = cli(&m, &d, &["apply", THEME]);
        assert_eq!(code, EXIT_INCOMPLETE, "{err}");
        let target = format!(
            "could not put back: file {} (E_DATA_WRITE: ",
            init.display()
        );
        assert!(err.contains(&target), "{err}");
        let fix = format!(r"    fix: Copy {}\snapshots\", d.data_dir.display());
        assert!(err.contains(&fix), "{err}");
        assert!(
            err.contains(&format!(" over {}.\n", init.display())),
            "{err}"
        );
        // The next run, even in a new process, still says what to fix. Doctor says it in R3,
        // without the lock or a recovery, and writes nothing...
        let files = m.fs.snapshot();
        let (code, out, err) = cli(&m, &later(&d, 1), &["doctor"]);
        assert_eq!((code, err.as_str()), (EXIT_FAILED, ""), "{out}");
        assert!(out.contains("\nR3   FAIL  The last rollback"), "{out}");
        assert!(out.contains(&init.display().to_string()), "{out}");
        assert_eq!(m.fs.snapshot(), files);
        // ...and a command that changes the machine says it first, once, then refuses to undo
        // and says how to get out.
        let (code, out, err) = cli(&m, &later(&d, 2), &["undo"]);
        assert_eq!((code, out.as_str()), (EXIT_FAILED, ""), "{err}");
        assert!(
            err.starts_with("Note: the rollback of restore point "),
            "{err}"
        );
        assert_eq!(err.matches(&target).count(), 1, "{err}");
        assert!(
            err.contains("\nE_ROLLBACK_INCOMPLETE: the rollback of restore point "),
            "{err}"
        );
        assert!(err.contains("run `cmdstudio --cleanup`"), "{err}");
    }

    /// A restore point's manifest, for a transaction a test writes by hand.
    fn manifest(id: &str) -> String {
        format!(
            r#"{{"id": "{id}", "kind": "apply", "createdMs": 1, "theme": null, "target": "all",
                "summary": [], "undoes": null, "undoneBy": null}}"#
        )
    }

    #[test]
    fn every_command_first_rolls_back_a_run_a_crash_cut_off() {
        let (m, d) = machine();
        // What a crash leaves right after an apply started its journal.
        let point = d.data_dir.join(r"snapshots\0000000000001-aaaaaaaa");
        m.fs.put(
            point.join("manifest.json"),
            manifest("0000000000001-aaaaaaaa"),
        );
        m.fs.put(
            point.join("journal.json"),
            r#"{"id": "0000000000001-aaaaaaaa", "ops": [], "snapshots": [], "stage": {"state": "planned"}}"#,
        );
        // Doctor only reads: it reports the cut-off change (R1) and leaves it for the next
        // command that changes the machine.
        let files = m.fs.snapshot();
        let (code, out, err) = cli(&m, &d, &["doctor"]);
        assert_eq!((code, err.as_str()), (EXIT_OK, ""), "{out}");
        assert!(
            out.contains("\nR1   warn  An earlier change was cut off"),
            "{out}"
        );
        assert_eq!(m.fs.snapshot(), files);
        let (code, out, err) = cli(&m, &d, &["undo"]);
        assert_eq!(
            (code, out.as_str()),
            (EXIT_OK, "Nothing to undo.\n"),
            "{err}"
        );
        assert_eq!(
            err,
            "E_TXN_INTERRUPTED: the previous apply was interrupted before it changed anything; \
             the machine is as it was before it.\n"
        );
        // Once only.
        assert_eq!(cli(&m, &later(&d, 1), &["undo"]).2, "");
        assert!(!cli(&m, &later(&d, 2), &["doctor"]).1.contains("R1"));
    }

    #[test]
    fn another_run_holding_the_lock_stops_every_change_but_not_doctor() {
        let (m, d) = machine();
        assert_eq!(cli(&m, &d, &["apply", THEME]).0, EXIT_OK);
        let lock =
            m.fs.try_lock(&d.data_dir.join("cmdstudio.lock"))
                .unwrap()
                .unwrap();
        let before = (m.fs.snapshot(), m.autorun.snapshot());
        for args in [
            vec!["apply", THEME],
            vec!["undo"],
            vec!["--cleanup"],
            vec!["set-aside", "1791694709000-abcdef12"],
        ] {
            let (code, out, err) = cli(&m, &later(&d, 1), &args);
            assert_eq!(code, EXIT_FAILED, "{args:?}: {out}{err}");
            assert_eq!(
                err,
                format!("E_BUSY: {}\n", cmdstudio_core::actions::BUSY_EN),
                "{args:?}"
            );
            assert_eq!((m.fs.snapshot(), m.autorun.snapshot()), before, "{args:?}");
        }
        let (code, out, err) = cli(&m, &later(&d, 1), &["doctor"]);
        assert_eq!((code, err.as_str()), (EXIT_OK, ""), "{out}");
        assert!(out.contains("\nR1   warn  Another CMD Studio"), "{out}");
        drop(lock);
        assert_eq!(cli(&m, &later(&d, 2), &["undo"]).0, EXIT_OK);
    }

    #[test]
    fn what_to_fix_by_hand_is_said_in_english_on_thai_windows() {
        let (m, d) = thai_windows();
        let init = d.data_dir.join(r"generated\init.lua");
        m.fs.put(&init, b"old");
        m.fs.fail_writes_to(&init);
        let (code, _, err) = cli(&m, &d, &["apply", THEME]);
        assert_eq!(code, EXIT_INCOMPLETE, "{err}");
        assert!(
            err.contains(
                "\nE_ROLLBACK_INCOMPLETE: not everything could be put back. Fix these by hand:\n"
            ),
            "{err}"
        );
        let fix = format!(r"    fix: Copy {}\snapshots\", d.data_dir.display());
        assert!(err.contains(&fix), "{err}");
        // The code, then the core's English text: no meaning of the code in another language.
        assert!(err.starts_with("E_DATA_WRITE: "), "{err}");
        assert_eq!(thai(&err), "", "{err}");
        // doctor's R3 says how to fix each step in English too, and only in English.
        let (code, out, err) = cli(&m, &later(&d, 1), &["doctor"]);
        assert_eq!((code, err.as_str()), (EXIT_FAILED, ""), "{out}");
        assert!(out.contains("\nR3   FAIL  The last rollback"), "{out}");
        assert!(out.contains(&format!("\n{:11}  fix: Copy ", "")), "{out}");
        assert_eq!(thai(&out), "", "{out}");
        let (_, out, _) = cli(&m, &later(&d, 1), &["doctor", "--json"]);
        assert_eq!(thai(&out), "", "{out}");
    }

    #[test]
    fn a_relative_folder_is_refused_before_anything_is_touched() {
        let (m, d) = machine();
        let before = m.fs.snapshot();
        for args in [
            vec!["doctor", "--data-dir", "data"],
            vec!["--cleanup", "--data-dir", r"..\x"],
            vec!["apply", THEME, "--resources", "res"],
        ] {
            let (code, out, err) = cli(&m, &d, &args);
            assert_eq!(code, EXIT_INVALID, "{args:?}: {out}");
            assert!(err.contains("needs a full path"), "{args:?}: {err}");
        }
        assert_eq!(m.fs.snapshot(), before);
    }

    #[test]
    fn missing_bundled_files_say_reinstall_and_keep_the_developer_hint_in_the_details() {
        let (m, d) = machine();
        let (code, _, err) = cli(&m, &d, &["apply", THEME, "--resources", r"C:\nowhere"]);
        assert_eq!(code, EXIT_FAILED, "{err}");
        let first = err.lines().next().unwrap();
        assert!(
            first.starts_with("E_RUNTIME_MISSING: ") && first.contains("Reinstall CMD Studio"),
            "{err}"
        );
        assert!(!first.contains("fetch-resources"), "{err}");
        assert!(
            err.lines()
                .any(|l| l.starts_with("  details: ") && l.contains("fetch-resources")),
            "{err}"
        );
    }

    #[test]
    fn cleanup_says_each_failed_step_with_its_code_and_a_second_run_finishes() {
        let (m, d) = machine();
        assert_eq!(cli(&m, &d, &["apply", THEME]).0, EXIT_OK);
        let generated = d.data_dir.join("generated");
        m.fs.fail_removes_of(&generated);
        let (code, out, err) = cli(&m, &later(&d, 1), &["--cleanup"]);
        assert_eq!(code, EXIT_INCOMPLETE, "{out}{err}");
        let failed = format!("FAILED Delete {}: E_DATA_WRITE: ", generated.display());
        assert!(err.contains(&failed), "{err}");
        assert!(err.contains("\n    fix: "), "{err}");
        assert!(err.contains("`cmdstudio --cleanup` again"), "{err}");
        assert!(
            out.starts_with("done   AutoRun and Windows Terminal's settings: ")
                && !out.contains("FAILED"),
            "{out}"
        );
        assert_eq!(m.autorun.read_user().unwrap(), None);
        m.fs.stop_failing(&generated);
        let (code, out, err) = cli(&m, &later(&d, 2), &["--cleanup"]);
        assert_eq!(code, EXIT_OK, "{out}{err}");
        assert!(!m.fs.exists(&d.data_dir));
    }

    #[test]
    fn a_cleanup_whose_revert_cannot_be_put_back_exits_3_and_deletes_nothing() {
        let (m, d) = machine();
        assert_eq!(cli(&m, &d, &["apply", THEME]).0, EXIT_OK);
        m.fs.fail_writes_to(WT);
        let (code, out, err) = cli(&m, &later(&d, 1), &["--cleanup"]);
        assert_eq!(code, EXIT_INCOMPLETE, "{out}{err}");
        assert!(
            err.contains(
                "\nE_ROLLBACK_INCOMPLETE: not everything could be put back. Fix these by hand:\n"
            ),
            "{err}"
        );
        assert!(
            err.contains(&format!("  could not put back: file {WT} (")),
            "{err}"
        );
        assert!(err.contains("\n    fix: "), "{err}");
        assert!(!out.contains("is deleted"), "{out}");
        assert!(m.fs.exists(&d.data_dir.join("state.json")));
        assert!(m.fs.exists(&d.data_dir.join("snapshots")));
    }

    #[test]
    fn cleanup_refuses_a_data_dir_that_is_not_cmd_studios() {
        let (m, d) = machine();
        let docs = r"C:\Users\tester\Documents";
        m.fs.put(format!(r"{docs}\notes.txt"), b"mine");
        let before = m.fs.snapshot();
        let (code, out, err) = cli(&m, &d, &["--cleanup", "--data-dir", docs]);
        assert_eq!(code, EXIT_INVALID, "{out}{err}");
        assert!(
            err.contains("holds what CMD Studio does not put in its data folder: \"notes.txt\"."),
            "{err}"
        );
        assert_eq!(m.fs.snapshot(), before);
        // A folder that is absent, empty, or holds CMD Studio's files is cleaned up.
        let gone = r"C:\Users\tester\gone";
        assert_eq!(cli(&m, &d, &["--cleanup", "--data-dir", gone]).0, EXIT_OK);
        let other = r"C:\Users\tester\other";
        assert_eq!(
            cli(&m, &d, &["apply", THEME, "--data-dir", other]).0,
            EXIT_OK
        );
        let (code, out, err) = cli(&m, &later(&d, 1), &["--cleanup", "--data-dir", other]);
        assert_eq!(code, EXIT_OK, "{out}{err}");
        assert!(m.fs.file(format!(r"{docs}\notes.txt")).is_some());
    }

    /// The data-folder refusal of `--cleanup --data-dir`, by its wording.
    const NOT_OURS: &str = "holds what CMD Studio does not put in its data folder";

    #[test]
    fn cleanup_refuses_a_folder_that_holds_anything_cmd_studio_does_not_put_there() {
        // A project that happens to hold a state.json, even one CMD Studio wrote: --cleanup
        // would delete everything in it but themes\.
        let (m, d) = machine();
        assert_eq!(cli(&m, &d, &["apply", THEME]).0, EXIT_OK);
        let state = m.fs.file(d.data_dir.join("state.json")).unwrap();
        let project = r"D:\proj";
        m.fs.put(format!(r"{project}\state.json"), &state);
        m.fs.put(format!(r"{project}\notes.txt"), b"mine");
        m.fs.put(format!(r"{project}\src\main.rs"), b"fn main() {}");
        let before = m.fs.snapshot();
        for args in [
            vec!["--cleanup", "--data-dir", project],
            vec!["--cleanup", "--remove-themes", "--data-dir", project],
        ] {
            let (code, out, err) = cli(&m, &later(&d, 1), &args);
            assert_eq!(code, EXIT_INVALID, "{args:?}: {out}{err}");
            assert!(
                err.contains(&format!(
                    "{project} {NOT_OURS}: \"notes.txt\", \"src\\\". --cleanup deletes"
                )),
                "{err}"
            );
            assert!(
                err.contains("Move them out of the folder, or check the path after --data-dir."),
                "{err}"
            );
            assert_eq!(m.fs.snapshot(), before, "{args:?}");
        }
        // On Thai Windows, in English, with the user's names as they were written: a Thai file
        // name is the user's, not text of CMD Studio's.
        m.fs.put(format!(r"{project}\บันทึก.txt"), b"mine");
        let before = m.fs.snapshot();
        let mut m = m;
        m.host.info.ui_language = THAI_WINDOWS;
        let (code, _, err) = cli(&m, &later(&d, 1), &["--cleanup", "--data-dir", project]);
        assert_eq!(code, EXIT_INVALID, "{err}");
        assert!(
            err.contains(&format!(
                "{project} {NOT_OURS}: \"notes.txt\", \"src\\\", \"บันทึก.txt\". --cleanup deletes"
            )),
            "{err}"
        );
        assert!(
            err.contains("Move them out of the folder, or check the path after --data-dir."),
            "{err}"
        );
        assert_eq!(thai(&err), thai("บันทึก.txt"), "{err}");
        assert_eq!(m.fs.snapshot(), before);
    }

    #[test]
    fn a_name_of_cmd_studios_with_the_wrong_kind_is_not_taken_for_it() {
        let (m, d) = machine();
        // Each folder holds one entry, with the name the refusal shows for it (a folder with its
        // backslash).
        for (folder, put, shown) in [
            (
                r"C:\x\a",
                r"C:\x\a\state.json\inside.txt",
                "\"state.json\\\"",
            ),
            (r"C:\x\b", r"C:\x\b\runtime", "\"runtime\""),
            // The app's names too: a file called drafts, a folder called settings.json, and a
            // file called health (the folder of the start tags and the pause).
            (r"C:\x\d", r"C:\x\d\drafts", "\"drafts\""),
            (r"C:\x\f", r"C:\x\f\health", "\"health\""),
            (
                r"C:\x\e",
                r"C:\x\e\settings.json\inside.txt",
                "\"settings.json\\\"",
            ),
            // A lookalike of a name of CMD Studio's is anybody's.
            (r"C:\x\g", r"C:\x\g\healthy\inside.txt", "\"healthy\\\""),
        ] {
            m.fs.put(put, b"x");
            let before = m.fs.snapshot();
            let (code, out, err) = cli(&m, &d, &["--cleanup", "--data-dir", folder]);
            assert_eq!(code, EXIT_INVALID, "{folder}: {out}{err}");
            assert!(err.contains(NOT_OURS), "{err}");
            assert!(
                err.contains(&format!("{folder} {NOT_OURS}: {shown}. --cleanup deletes")),
                "{shown}: {err}"
            );
            assert_eq!(m.fs.snapshot(), before, "{folder}");
        }
        // The first few are named, then how many more.
        for n in 1..=5 {
            m.fs.put(format!(r"C:\x\c\f{n}.txt"), b"x");
        }
        let (_, _, err) = cli(&m, &d, &["--cleanup", "--data-dir", r"C:\x\c"]);
        assert!(
            err.contains(": \"f1.txt\", \"f2.txt\", \"f3.txt\" and 2 more. --cleanup"),
            "{err}"
        );
    }

    #[test]
    fn a_folder_with_only_cmd_studio_named_folders_but_none_of_its_files_is_refused() {
        // Folders named like CMD Studio's are not enough: one of its own files must be there
        // (cmdstudio.lock, state.json, settings.json or a .damaged-<number> copy of them).
        for (m, d) in english_and_thai_windows() {
            let folder = r"D:\work";
            m.fs.put(format!(r"{folder}\runtime\tool.exe"), b"mine");
            m.fs.put(format!(r"{folder}\snapshots\2026\notes.txt"), b"mine");
            // health\ is CMD Studio's name, and by itself no more proof than the others.
            m.fs.put(format!(r"{folder}\health\start-1"), b"mine");
            let before = (m.fs.snapshot(), m.autorun.snapshot());
            for args in [
                vec!["--cleanup", "--data-dir", folder],
                vec!["--cleanup", "--remove-themes", "--data-dir", folder],
            ] {
                let (code, out, err) = cli(&m, &d, &args);
                assert_eq!(code, EXIT_INVALID, "{args:?}: {out}{err}");
                for name in [
                    "cmdstudio.lock",
                    "state.json",
                    "settings.json",
                    ".damaged-<number>",
                ] {
                    assert!(err.contains(name), "{name}: {err}");
                }
                assert!(err.contains("none of CMD Studio's own files"), "{err}");
                assert!(err.contains("check the path after --data-dir"), "{err}");
                assert_eq!(thai(&err), "", "{err}");
                assert_eq!((m.fs.snapshot(), m.autorun.snapshot()), before, "{args:?}");
            }
        }
    }

    #[test]
    fn a_folder_cmd_studio_made_is_still_taken_for_its_own() {
        let other = r"C:\Users\tester\other";
        // After an apply, and after its undo.
        for undo in [false, true] {
            let (m, d) = machine();
            assert_eq!(
                cli(&m, &d, &["apply", THEME, "--data-dir", other]).0,
                EXIT_OK
            );
            if undo {
                assert_eq!(
                    cli(&m, &later(&d, 1), &["undo", "--data-dir", other]).0,
                    EXIT_OK
                );
            }
            let (code, out, err) = cli(&m, &later(&d, 2), &["--cleanup", "--data-dir", other]);
            assert_eq!(code, EXIT_OK, "undo {undo}: {out}{err}");
        }
        // After a run a crash cut off: a journal it left, the temp file of a write it did not
        // finish, and a damaged state.json a revert set aside, in any letter case.
        let (m, d) = machine();
        assert_eq!(
            cli(&m, &d, &["apply", THEME, "--data-dir", other]).0,
            EXIT_OK
        );
        let point = format!(r"{other}\snapshots\1791694709500-bbbbbbbb");
        m.fs.put(
            format!(r"{point}\manifest.json"),
            manifest("1791694709500-bbbbbbbb"),
        );
        m.fs.put(
            format!(r"{point}\journal.json"),
            r#"{"id": "1791694709500-bbbbbbbb", "ops": [], "snapshots": [], "stage": {"state": "planned"}}"#,
        );
        m.fs.put(
            format!(r"{other}\.state.json.0123456789abcdef0123456789ABCDEF.cmdstudio-tmp"),
            b"{",
        );
        m.fs.put(format!(r"{other}\State.json.damaged-1791694709000"), b"{");
        let (code, out, err) = cli(&m, &later(&d, 3), &["--cleanup", "--data-dir", other]);
        assert!(!err.contains(NOT_OURS), "{err}");
        assert_ne!(code, EXIT_INVALID, "{out}{err}");
        // Names that only look like those are not.
        for stray in [
            r".state.json.0123.cmdstudio-tmp",
            r".notes.txt.0123456789abcdef0123456789abcdef.cmdstudio-tmp",
            r"state.json.damaged-",
            r"state.json.damaged-12ab",
            // The same for settings.json, the app's file.
            r"settings.json.bak",
            r"settings.json.damaged-",
            r"settings.json.damaged-12ab",
            r".settings.json.0123.cmdstudio-tmp",
            r".settings.json.0123456789abcdef0123456789abcdef0.cmdstudio-tmp",
            r".settings.json.0123456789abcdef0123456789abcdeg.cmdstudio-tmp",
        ] {
            let (m, d) = machine();
            let folder = r"C:\Users\tester\stray";
            m.fs.put(format!(r"{folder}\state.json"), b"{}");
            m.fs.put(format!(r"{folder}\{stray}"), b"x");
            let (code, _, err) = cli(&m, &d, &["--cleanup", "--data-dir", folder]);
            assert_eq!(code, EXIT_INVALID, "{stray}: {err}");
            assert!(err.contains(&format!("\"{stray}\"")), "{err}");
        }
    }

    #[test]
    fn a_restore_point_that_cannot_be_read_is_reported_once_per_command() {
        let (m, d) = machine();
        assert_eq!(cli(&m, &d, &["apply", THEME]).0, EXIT_OK);
        assert_eq!(cli(&m, &later(&d, 1), &["apply", THEME]).0, EXIT_OK);
        let snapshots = d.data_dir.join("snapshots");
        let first = m.fs.list_dir(&snapshots).unwrap()[0].path.clone();
        let id = first.file_name().unwrap().to_str().unwrap().to_string();
        m.fs.fail_reads_of(first.join("journal.json"));
        // Both the start of the CLI and the start of the apply find it; it is said once.
        let (code, out, err) = cli(&m, &later(&d, 2), &["apply", THEME]);
        assert_eq!(code, EXIT_OK, "{out}{err}");
        assert_eq!(err.lines().filter(|l| l.contains(&id)).count(), 1, "{err}");
        assert!(
            err.starts_with(&format!(
                "Note: E_SNAPSHOT_READ: restore point {id} can't be read right now ("
            )),
            "{err}"
        );
        // A damaged one is set aside, and said once too.
        m.fs.stop_failing(first.join("journal.json"));
        m.fs.put(first.join("manifest.json"), b"{ damaged");
        let (code, out, err) = cli(&m, &later(&d, 3), &["undo"]);
        assert_eq!(code, EXIT_OK, "{out}{err}");
        assert_eq!(err.lines().filter(|l| l.contains(&id)).count(), 1, "{err}");
        assert!(err.contains("It was set aside"), "{err}");
    }

    #[test]
    fn a_record_that_cannot_be_read_is_said_once_and_the_apply_goes_on() {
        let (m, d) = machine();
        let init = d.data_dir.join(r"generated\init.lua");
        m.fs.put(&init, b"old");
        m.fs.fail_writes_to(&init);
        assert_eq!(cli(&m, &d, &["apply", THEME]).0, EXIT_INCOMPLETE);
        m.fs.stop_failing(&init);
        let point = m.fs.list_dir(&d.data_dir.join("snapshots")).unwrap()[0]
            .path
            .clone();
        m.fs.put(point.join("incomplete.json"), b"{ damaged");
        let (code, out, err) = cli(&m, &later(&d, 1), &["apply", THEME]);
        assert_eq!(code, EXIT_OK, "{out}{err}");
        assert!(err.starts_with("Note: E_SNAPSHOT_READ: "), "{err}");
        assert_eq!(
            err.matches("incomplete.json is damaged").count(),
            1,
            "{err}"
        );
    }

    #[test]
    fn cleanup_waits_while_the_record_of_an_unfinished_rollback_cannot_be_read() {
        for (m, d) in english_and_thai_windows() {
            let init = d.data_dir.join(r"generated\init.lua");
            m.fs.put(&init, b"old");
            m.fs.fail_writes_to(&init);
            assert_eq!(cli(&m, &d, &["apply", THEME]).0, EXIT_INCOMPLETE);
            m.fs.stop_failing(&init);
            let point = m.fs.list_dir(&d.data_dir.join("snapshots")).unwrap()[0]
                .path
                .clone();
            m.fs.fail_reads_of(point.join("incomplete.json"));
            let before = (m.fs.snapshot().0.len(), m.autorun.snapshot(), m.fs.file(WT));
            let (code, out, err) = cli(&m, &later(&d, 1), &["--cleanup"]);
            assert_eq!(code, EXIT_FAILED, "{out}{err}");
            assert!(err.contains("E_SNAPSHOT_READ: "), "{err}");
            assert!(
                err.contains("Your settings were not changed. Try again in a moment"),
                "{err}"
            );
            assert_eq!(thai(&err), "", "{err}");
            assert_eq!(
                (m.fs.snapshot().0.len(), m.autorun.snapshot(), m.fs.file(WT)),
                before
            );
            assert!(m.fs.exists(&point.join("journal.json")), "nothing deleted");
        }
    }

    /// A transaction a crash cut off whose journal can't be read now, newer than any restore
    /// point the tests' commands make before it. Returns its id.
    fn crashed_and_unreadable(m: &FakeMachine, d: &Defaults) -> String {
        let id = "1791694709500-bbbbbbbb";
        let dir = d.data_dir.join("snapshots").join(id);
        m.fs.put(dir.join("manifest.json"), manifest(id));
        m.fs.put(
            dir.join("journal.json"),
            format!(
                r#"{{"id": "{id}", "ops": [], "snapshots": [], "stage": {{"state": "planned"}}}}"#
            ),
        );
        m.fs.fail_reads_of(dir.join("journal.json"));
        id.to_string()
    }

    #[test]
    fn a_journal_that_cannot_be_read_stops_every_change_and_set_aside_is_the_way_out() {
        let (m, d) = machine();
        assert_eq!(cli(&m, &d, &["apply", THEME]).0, EXIT_OK);
        let snapshots = d.data_dir.join("snapshots");
        let applied = m.fs.list_dir(&snapshots).unwrap()[0].path.clone();
        let applied = applied.file_name().unwrap().to_str().unwrap().to_string();
        let id = crashed_and_unreadable(&m, &d);
        let way_out = format!("`cmdstudio set-aside {id}`");
        let before = (m.fs.snapshot(), m.autorun.snapshot());
        // Doctor is not blocked: it reports it (R1), takes no lock and changes nothing.
        let (code, out, err) = cli(&m, &later(&d, 1), &["doctor"]);
        assert_eq!((code, err.as_str()), (EXIT_FAILED, ""), "{out}");
        assert!(
            out.contains("\nR1   FAIL  ") && out.contains(&way_out),
            "{out}"
        );
        let (code, out, _) = cli(&m, &later(&d, 1), &["doctor", "--json"]);
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        let checks = v["checks"].as_array().unwrap();
        assert_eq!((code, checks.len()), (EXIT_FAILED, 12), "{out}");
        assert_eq!(checks[11]["id"], "R1");
        assert_eq!(checks[11]["code"], "E_SNAPSHOT_READ");
        assert!(checks[11]["messageEn"].as_str().unwrap().contains(&way_out));
        // Every command that changes the machine refuses, changes nothing and names the way out.
        for args in [vec!["apply", THEME], vec!["undo"], vec!["--cleanup"]] {
            let (code, out, err) = cli(&m, &later(&d, 1), &args);
            assert_eq!(code, EXIT_FAILED, "{args:?}: {out}{err}");
            assert!(
                err.contains("\nE_SNAPSHOT_READ: An earlier change could not be checked yet"),
                "{args:?}: {err}"
            );
            assert!(err.contains(&way_out), "{args:?}: {err}");
            assert_eq!((m.fs.snapshot(), m.autorun.snapshot()), before, "{args:?}");
        }
        // set-aside refuses a restore point that reads, and one that is not there.
        let (code, out, err) = cli(&m, &later(&d, 1), &["set-aside", &applied]);
        assert_eq!(code, EXIT_FAILED, "{out}{err}");
        assert!(err.contains("can be read, so it is not set aside"), "{err}");
        for missing in ["1791694709999-nothere0", r"..\..\x"] {
            let (code, _, err) = cli(&m, &later(&d, 1), &["set-aside", missing]);
            assert_eq!(code, EXIT_INVALID, "{missing}: {err}");
            assert!(err.contains("there is no restore point"), "{err}");
        }
        assert_eq!((m.fs.snapshot(), m.autorun.snapshot()), before);
        // It sets aside the one that can't be read, and says what that means.
        let (code, out, err) = cli(&m, &later(&d, 2), &["set-aside", &id]);
        assert_eq!(code, EXIT_OK, "{out}{err}");
        assert!(out.contains("is not rolled back by itself"), "{out}");
        assert!(out.contains("`cmdstudio --cleanup`"), "{out}");
        assert!(m
            .fs
            .exists(&snapshots.join(&id).join("journal.unreadable.json")));
        let (code, out, _) = cli(&m, &later(&d, 3), &["set-aside", &id]);
        assert_eq!(code, EXIT_OK);
        assert!(out.contains("already set aside"), "{out}");
        // From now on, what to check by hand is said: by doctor, and before every command.
        let (code, out, _) = cli(&m, &later(&d, 4), &["doctor"]);
        assert_eq!(code, EXIT_FAILED, "{out}");
        assert!(
            !out.contains("\nR1 ") && out.contains("\nR3   FAIL "),
            "{out}"
        );
        let (code, out, err) = cli(&m, &later(&d, 5), &["apply", THEME]);
        assert_eq!(code, EXIT_OK, "{out}{err}");
        assert!(
            err.starts_with(&format!(
                "Note: the rollback of restore point {id} did not finish."
            )),
            "{err}"
        );
        assert!(err.contains("cmdstudio --cleanup"), "{err}");
    }

    #[test]
    fn thai_names_are_shown_as_they_are_on_every_windows() {
        // Thai vowels and tone marks are characters Rust's `{:?}` would escape. The theme's name
        // and font are the user's, so they are shown as written, in the English text.
        for (m, d) in english_and_thai_windows() {
            let theme = String::from_utf8(m.fs.file(THEME).unwrap())
                .unwrap()
                .replace("\"name\": \"Terracotta\"", "\"name\": \"Terracotta ของฉัน\"")
                .replace("\"font\": \"Cascadia Mono\"", "\"font\": \"ฟอนต์ไทย\"");
            m.fs.put(THEME, theme);
            let (code, out, err) = cli(&m, &d, &["apply", THEME]);
            assert_eq!(code, EXIT_OK, "{out}{err}");
            assert!(out.starts_with("Plan for \"Terracotta ของฉัน\":\n"), "{out}");
            assert!(out.contains("\nApplied \"Terracotta ของฉัน\". "), "{out}");
            assert!(
                out.contains("\nWarning: The font \"ฟอนต์ไทย\" is not installed"),
                "{out}"
            );
            assert!(!out.contains("\\u{"), "{out}");
        }
    }

    #[test]
    fn a_path_cmd_cannot_take_is_shown_as_it_is() {
        let (m, d) = machine();
        let data = r"C:\Users\ผู้ใช้%\cmdstudio";
        let (code, _, err) = cli(&m, &d, &["apply", THEME, "--data-dir", data]);
        assert_eq!(code, EXIT_FAILED, "{err}");
        assert!(err.starts_with("E_CLINK_PATH: "), "{err}");
        assert!(err.contains(data), "{err}");
        assert!(!err.contains("\\u{") && !err.contains(r"\\"), "{err}");
        // A leftover font file of CMD Studio's may be gone already, so it is the user's
        // settings that were not changed; and this path error says what the user can do.
        assert!(
            err.contains(
                "\nYour settings were not changed. This path is in CMD Studio's data folder"
            ),
            "{err}"
        );
        assert!(
            err.contains("Use a Windows account whose user folder has none of them"),
            "{err}"
        );
        // Thai Windows gets the same English text, the path as it is written.
        let (mut m, d) = (m, d);
        m.host.info.ui_language = THAI_WINDOWS;
        let (code, _, thai_err) = cli(&m, &d, &["apply", THEME, "--data-dir", data]);
        assert_eq!(code, EXIT_FAILED, "{thai_err}");
        assert_eq!(thai_err, err);
        assert!(!err.contains("Nothing was changed"), "{err}");
    }

    #[test]
    fn a_color_written_in_thai_is_shown_as_it_was_written() {
        let (m, d) = machine();
        let theme = String::from_utf8(m.fs.file(THEME).unwrap()).unwrap();
        let first = theme
            .split("\"#")
            .nth(1)
            .unwrap()
            .get(..6)
            .unwrap()
            .to_string();
        m.fs.put(
            THEME,
            theme.replacen(&format!("\"#{first}\""), "\"#สีส้ม\"", 1),
        );
        let (code, _, err) = cli(&m, &d, &["apply", THEME]);
        assert_eq!(code, EXIT_INVALID, "{err}");
        assert!(err.starts_with("E_THEME_INVALID: "), "{err}");
        assert!(err.contains("not a #rrggbb color: \"#สีส้ม\""), "{err}");
        assert!(!err.contains("\\u{"), "{err}");
    }

    #[test]
    fn a_refusal_worded_by_the_core_says_what_to_do_next() {
        for (m, d) in english_and_thai_windows() {
            m.fs.put(WT, "{ not json");
            let before = (m.fs.file(WT), m.autorun.snapshot());
            let (code, _, err) = cli(&m, &d, &["apply", THEME]);
            assert_eq!(code, EXIT_FAILED, "{err}");
            assert!(err.starts_with("E_WT_PARSE: "), "{err}");
            assert!(
                err.contains("\nYour settings were not changed. Fix that settings.json by hand"),
                "{err}"
            );
            assert_eq!(thai(&err), "", "{err}");
            assert_eq!((m.fs.file(WT), m.autorun.snapshot()), before);
        }
    }

    #[test]
    fn a_failed_cleanup_step_is_said_in_english_on_thai_windows() {
        let (m, d) = thai_windows();
        assert_eq!(cli(&m, &d, &["apply", THEME]).0, EXIT_OK);
        let generated = d.data_dir.join("generated");
        m.fs.fail_removes_of(&generated);
        let (code, out, err) = cli(&m, &later(&d, 1), &["--cleanup"]);
        assert_eq!(code, EXIT_INCOMPLETE, "{out}{err}");
        // The code, then the core's English detail and its English way to fix it by hand.
        let step = format!("FAILED Delete {}: E_DATA_WRITE: ", generated.display());
        assert!(err.contains(&step), "{err}");
        let fix = format!(
            "\n    fix: Close every cmd window and Windows Terminal, then delete {}.",
            generated.display()
        );
        assert!(err.contains(&fix), "{err}");
        assert_eq!(thai(&format!("{out}{err}")), "", "{out}{err}");
    }

    #[test]
    fn an_older_record_without_thai_fixes_still_shows_its_fix() {
        // A record written before `fixTh` existed still reads, on Thai Windows as well.
        let (mut m, d) = machine();
        let init = d.data_dir.join(r"generated\init.lua");
        m.fs.put(&init, b"old");
        m.fs.fail_writes_to(&init);
        assert_eq!(cli(&m, &d, &["apply", THEME]).0, EXIT_INCOMPLETE);
        let point = m.fs.list_dir(&d.data_dir.join("snapshots")).unwrap()[0]
            .path
            .clone();
        let file = point.join("incomplete.json");
        let mut record: serde_json::Value =
            serde_json::from_slice(&m.fs.file(&file).unwrap()).unwrap();
        for step in record["steps"].as_array_mut().unwrap() {
            step.as_object_mut().unwrap().remove("fixTh");
        }
        m.fs.put(&file, serde_json::to_vec(&record).unwrap());
        m.host.info.ui_language = THAI_WINDOWS;
        let (code, _, err) = cli(&m, &later(&d, 1), &["undo"]);
        assert_eq!(code, EXIT_FAILED, "{err}");
        assert!(err.contains("\n    fix: Copy "), "{err}");
        assert_eq!(thai(&err), "", "{err}");
    }

    #[test]
    fn cleanup_with_a_data_dir_runs_again_and_keeps_or_removes_the_themes() {
        let (m, d) = machine();
        let other = r"C:\Users\tester\other";
        let mine = format!(r"{other}\themes\mine.cmdtheme");
        m.fs.put(&mine, b"{}");
        assert_eq!(
            cli(&m, &d, &["apply", THEME, "--data-dir", other]).0,
            EXIT_OK
        );
        // A folder that holds only themes\ is what --cleanup left: running it again is fine.
        let kept = format!("; your themes are kept in {other}\\themes.\n");
        for n in 1..=2 {
            let (code, out, err) = cli(&m, &later(&d, n), &["--cleanup", "--data-dir", other]);
            assert_eq!(code, EXIT_OK, "{n}: {out}{err}");
            assert!(out.ends_with(&kept), "{n}: {out}");
            assert!(m.fs.file(&mine).is_some(), "{n}");
            assert_eq!(m.fs.list_dir(Path::new(other)).unwrap().len(), 1, "{n}");
        }
        // With --remove-themes, though, a folder that holds only themes\ could be anybody's
        // (a mistyped --data-dir): it is refused and nothing is deleted.
        let remove = ["--cleanup", "--remove-themes", "--data-dir", other];
        let before = m.fs.snapshot();
        let (code, out, err) = cli(&m, &later(&d, 3), &remove);
        assert_eq!(code, EXIT_INVALID, "{out}{err}");
        assert!(err.contains("holds only a themes folder"), "{err}");
        assert_eq!(m.fs.snapshot(), before);
        // A folder with CMD Studio's files goes, themes and all.
        assert_eq!(
            cli(&m, &later(&d, 4), &["apply", THEME, "--data-dir", other]).0,
            EXIT_OK
        );
        let (code, out, err) = cli(&m, &later(&d, 5), &remove);
        assert_eq!(code, EXIT_OK, "{out}{err}");
        assert!(out.ends_with(", and its data is deleted.\n"), "{out}");
        assert!(!m.fs.exists(Path::new(other)));
        // themes\ next to anything else is not taken for CMD Studio's.
        let docs = r"C:\Users\tester\Documents";
        m.fs.put(format!(r"{docs}\themes\a.txt"), b"x");
        m.fs.put(format!(r"{docs}\notes.txt"), b"mine");
        let before = m.fs.snapshot();
        assert_eq!(
            cli(&m, &d, &["--cleanup", "--data-dir", docs]).0,
            EXIT_INVALID
        );
        assert_eq!(m.fs.snapshot(), before);
    }

    #[test]
    fn cleanup_with_a_damaged_state_file_does_what_doctor_says() {
        let (m, d) = machine();
        assert_eq!(cli(&m, &d, &["apply", THEME]).0, EXIT_OK);
        m.fs.put(d.data_dir.join("state.json"), b"{ damaged");
        let (_, out, _) = cli(&m, &later(&d, 1), &["doctor"]);
        assert!(out.contains("`cmdstudio --cleanup`"), "{out}");
        // It reverts, keeps the records, and ends with a failed step that says what to do.
        for n in 2..=3 {
            let (code, out, err) = cli(&m, &later(&d, n), &["--cleanup"]);
            assert_eq!(code, EXIT_INCOMPLETE, "{n}: {out}{err}");
            assert!(err.contains(": E_STATE_INVALID: "), "{n}: {err}");
            assert!(err.contains("\n    fix: "), "{n}: {err}");
            assert_eq!(m.autorun.read_user().unwrap(), None, "{n}");
            assert!(m.fs.exists(&d.data_dir.join("snapshots")), "{n}");
            let damaged =
                m.fs.list_dir(&d.data_dir)
                    .unwrap()
                    .iter()
                    .any(|e| e.path.to_string_lossy().contains("state.json.damaged-"));
            assert!(damaged, "{n}");
        }
    }

    #[test]
    fn nothing_the_cli_says_on_thai_windows_is_thai() {
        // The founder's rule (2026-10-13): English is CMD Studio's main language, and nothing it
        // writes into a cmd window is Thai, whatever the language of Windows.
        let (m, d) = thai_windows();
        let mut runs: Vec<(String, i32, (i32, String, String))> = Vec::new();
        let mut run = |m: &FakeMachine, d: &Defaults, args: &[&str], want: i32| {
            runs.push((args.join(" "), want, cli(m, d, args)));
        };
        run(&m, &d, &["--help"], EXIT_OK);
        run(&m, &d, &["frobnicate"], EXIT_INVALID);
        run(&m, &d, &["apply", THEME], EXIT_OK);
        run(&m, &later(&d, 1), &["doctor"], EXIT_OK);
        run(&m, &later(&d, 1), &["doctor", "--json"], EXIT_OK);
        run(&m, &later(&d, 2), &["undo"], EXIT_OK);
        run(&m, &later(&d, 3), &["--cleanup"], EXIT_OK);
        // A restore point whose journal can't be read, set aside by hand.
        let id = crashed_and_unreadable(&m, &d);
        run(&m, &later(&d, 4), &["set-aside", &id], EXIT_OK);
        // An apply refused: another Clink starts from AutoRun.
        let (other, od) = thai_windows();
        other
            .autorun
            .write_user(&RegValue::from_text(
                REG_SZ,
                r#""C:\clink\clink.bat" inject --autorun"#,
            ))
            .unwrap();
        run(&other, &od, &["apply", THEME], EXIT_INVALID);

        let mut wrong = Vec::new();
        for (args, want, (code, out, err)) in &runs {
            if code != want {
                wrong.push(format!("{args}: exit {code}, not {want}"));
            }
            for (stream, text) in [("stdout", out), ("stderr", err)] {
                let letters = thai(text);
                if !letters.is_empty() {
                    wrong.push(format!("{args}: {stream} holds \"{letters}\""));
                }
            }
        }
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
        // What is said is the English text, as on English Windows.
        let said = |i: usize| &runs[i].2;
        assert!(said(0).1.starts_with("Usage:\n"), "{}", said(0).1);
        assert!(
            said(1)
                .2
                .starts_with("cmdstudio: unknown command \"frobnicate\"\n\nUsage:\n"),
            "{}",
            said(1).2
        );
        assert!(
            said(2).1.starts_with("Plan for \"Terracotta\":\n  - ")
                && said(2)
                    .1
                    .contains("\nApplied \"Terracotta\". Open a new cmd window"),
            "{}",
            said(2).1
        );
        assert!(
            said(3)
                .1
                .contains("\nD10  ok    The app runs as a normal program"),
            "{}",
            said(3).1
        );
        let json: serde_json::Value = serde_json::from_str(&said(4).1).unwrap();
        for check in json["checks"].as_array().unwrap() {
            assert!(check["messageEn"].is_string(), "{check}");
        }
        assert!(
            said(5)
                .1
                .starts_with("Undid the theme applied in restore point "),
            "{}",
            said(5).1
        );
        assert_eq!(
            said(6).1.lines().last(),
            Some("CMD Studio's changes are taken out, and its data is deleted."),
            "{}",
            said(6).1
        );
        assert!(
            said(7)
                .1
                .starts_with(&format!("Restore point {id} is set aside; its files stay")),
            "{}",
            said(7).1
        );
        assert!(
            said(8)
                .2
                .starts_with("E_AUTORUN_FOREIGN_CLINK: another Clink starts with cmd"),
            "{}",
            said(8).2
        );
    }

    #[test]
    fn the_lang_option_is_gone_so_it_is_a_usage_error() {
        let (m, d) = thai_windows();
        let before = (m.fs.snapshot(), m.autorun.snapshot());
        for (args, says) in [
            (
                vec!["apply", THEME, "--lang", "th"],
                "apply needs exactly one theme file",
            ),
            (
                vec!["apply", THEME, "--lang", "en"],
                "apply needs exactly one theme file",
            ),
            (vec!["apply", THEME, "--lang"], "unknown option --lang"),
        ] {
            let (code, out, err) = cli(&m, &d, &args);
            assert_eq!((code, out.as_str()), (EXIT_INVALID, ""), "{args:?}: {err}");
            assert_eq!(err, format!("cmdstudio: {says}\n\n{USAGE}"), "{args:?}");
        }
        assert_eq!((m.fs.snapshot(), m.autorun.snapshot()), before);
        assert!(!USAGE.contains("--lang"), "{USAGE}");
    }

    #[test]
    fn the_clis_own_code_holds_no_thai_text() {
        // Every message the CLI says is written in its code, so the code outside the tests may
        // hold no Thai character: that covers the refusals no test here reaches. The tests do
        // hold Thai, as a user's names, paths and typed words.
        for (file, source) in [
            ("lib.rs", include_str!("lib.rs")),
            ("args.rs", include_str!("args.rs")),
        ] {
            let source = source.replace("\r\n", "\n");
            let code = source.split("\n#[cfg(test)]\nmod tests {").next().unwrap();
            assert_eq!(thai(code), "", "{file}");
        }
    }

    #[test]
    fn cleanup_and_usage() {
        let (m, d) = machine();
        assert_eq!(cli(&m, &d, &["apply", THEME]).0, EXIT_OK);
        let (code, out, _) = cli(&m, &later(&d, 1), &["--cleanup"]);
        assert_eq!(code, EXIT_OK, "{out}");
        assert_eq!(m.autorun.read_user().unwrap(), None);
        let (code, out, _) = cli(&m, &d, &["--help"]);
        assert_eq!(code, EXIT_OK);
        assert!(out.starts_with("Usage:"));
        assert_eq!(cli(&m, &d, &["frobnicate"]).0, EXIT_INVALID);
    }

    #[test]
    fn cleanup_with_a_data_dir_takes_the_apps_own_entries_for_cmd_studios() {
        // The app writes drafts\, logs\, webview\ (WebView2's data) and settings.json into the
        // data folder (spec §5.3). --cleanup --data-dir must know them, or a real data folder of
        // the app is refused as not CMD Studio's.
        let (m, d) = machine();
        let other = r"C:\Users\tester\other";
        m.fs.put(format!(r"{other}\themes\mine.cmdtheme"), b"{}");
        assert_eq!(
            cli(&m, &d, &["apply", THEME, "--data-dir", other]).0,
            EXIT_OK
        );
        for file in [
            r"drafts\1e9c0c7e-0e0b-4c71-9a4b-6a0c5d7f1a22.cmdtheme",
            r"logs\cmdstudio.log",
            r"logs\cmdstudio.log.1",
            r"webview\EBWebView\Default\Cache\data_0",
            r"health\start-4242",
            r"health\paused.json",
            "settings.json",
            "settings.json.damaged-1791694709000",
            ".settings.json.0123456789abcdef0123456789abcdef.cmdstudio-tmp",
            ".state.json.0123456789abcdef0123456789abcdef.cmdstudio-tmp",
        ] {
            m.fs.put(format!(r"{other}\{file}"), b"x");
        }
        let (code, out, err) = cli(&m, &later(&d, 1), &["--cleanup", "--data-dir", other]);
        assert_eq!(code, EXIT_OK, "{out}{err}");
        let left = m.fs.list_dir(Path::new(other)).unwrap();
        assert_eq!(left.len(), 1, "only the themes folder stays: {left:?}");
        assert!(left[0].is_dir, "{left:?}");
        assert_eq!(
            left[0].path.file_name().and_then(|n| n.to_str()),
            Some("themes"),
            "{left:?}"
        );
        assert!(m
            .fs
            .exists(Path::new(&format!(r"{other}\themes\mine.cmdtheme"))));
        // A folder with only settings.json and the app's folders is CMD Studio's too: the app
        // writes them before any theme is applied.
        let fresh = r"C:\Users\tester\fresh";
        m.fs.put(format!(r"{fresh}\settings.json"), b"{}");
        m.fs.put(format!(r"{fresh}\logs\cmdstudio.log"), b"");
        let (code, out, err) = cli(&m, &later(&d, 2), &["--cleanup", "--data-dir", fresh]);
        assert_eq!(code, EXIT_OK, "{out}{err}");
        assert!(!m.fs.exists(Path::new(fresh)));
        // Windows compares names without regard to case: HEALTH\ is health\.
        let upper = r"C:\Users\tester\upper";
        m.fs.put(format!(r"{upper}\settings.json"), b"{}");
        m.fs.put(format!(r"{upper}\HEALTH\paused.json"), b"{}");
        let (code, out, err) = cli(&m, &later(&d, 3), &["--cleanup", "--data-dir", upper]);
        assert_eq!(code, EXIT_OK, "{out}{err}");
        assert!(!m.fs.exists(Path::new(upper)));
        // A file CMD Studio never writes there is still refused, nothing deleted.
        m.fs.put(format!(r"{fresh}\settings.json"), b"{}");
        m.fs.put(format!(r"{fresh}\webview.txt"), b"x");
        let before = m.fs.snapshot();
        let (code, _, err) = cli(&m, &later(&d, 4), &["--cleanup", "--data-dir", fresh]);
        assert_eq!(code, EXIT_INVALID, "{err}");
        assert!(err.contains("\"webview.txt\""), "{err}");
        assert_eq!(m.fs.snapshot(), before);
    }

    /// The theme paused in cmd, as init.lua leaves it: `paused.json` and two start tags.
    fn paused_in_cmd(m: &FakeMachine, d: &Defaults) -> Paths {
        let paths = Paths {
            data: d.data_dir.clone(),
        };
        m.fs.put(
            paths.paused_file(),
            br#"{"version":1,"reason":"cmdStarts","at":1791694709000,"count":3,"by":"clink"}"#,
        );
        for pid in [11, 12] {
            m.fs.put(
                paths.health().join(format!("start-{pid}")),
                b"v1 1791694709\n",
            );
        }
        paths
    }

    /// How the apply's failure to clear the start records begins, whichever step failed.
    const NOT_CLEARED: &str =
        "E_DATA_WRITE: the theme was applied, but CMD Studio's start records in ";

    /// What a run said when the start records could not be cleared: the apply stands (exit 0, the
    /// "Applied" line, no hint to open a new window, which the line on stderr contradicts), and
    /// stderr says it once, in CMD Studio's own English words, and without claiming a pause.
    fn assert_applied_but_not_cleared(code: i32, out: &str, err: &str, paths: &Paths) {
        assert_eq!(code, EXIT_OK, "{out}{err}");
        assert!(out.contains("Applied \"Terracotta\"."), "{out}");
        assert!(!out.contains("Open a new"), "no hint to a window: {out}");
        assert_eq!(err.lines().count(), 1, "said once: {err}");
        let health = paths.health();
        assert!(
            err.starts_with(&format!(
                "{NOT_CLEARED}{} could not be cleared (",
                health.display()
            )),
            "{err}"
        );
        assert!(
            err.contains(
                "); if cmd windows open without the theme, delete the files in that folder, or \
                 turn the theme back on in CMD Studio"
            ),
            "{err}"
        );
        assert!(err.is_ascii(), "English only in cmd: {err}");
        assert!(!err.contains("until"), "no second path or deadline: {err}");
    }

    #[test]
    fn an_apply_lifts_the_pause_and_every_tag() {
        // Review Focus 5: a theme applied from the command line is the theme on again; a pause
        // left behind would keep every cmd window plain under the new theme.
        let (m, d) = machine();
        let paths = paused_in_cmd(&m, &d);
        let (code, out, err) = cli(&m, &d, &["apply", THEME]);
        assert_eq!((code, err.as_str()), (EXIT_OK, ""), "{out}");
        assert!(
            out.contains("Open a new cmd window to see the prompt and the welcome box."),
            "{out}"
        );
        assert!(!m.fs.exists(&paths.paused_file()));
        assert!(m.fs.list_dir(&paths.health()).unwrap().is_empty());
        // An apply that rolls back (here its check: Oh My Posh draws nothing) leaves the pause
        // and every tag.
        let paths = paused_in_cmd(&m, &d);
        m.process.answer("oh-my-posh.exe", "print", 1, "");
        let (code, _, err) = cli(&m, &later(&d, 1), &["apply", THEME]);
        assert_eq!(code, EXIT_FAILED, "{err}");
        assert!(m.fs.exists(&paths.paused_file()));
        for pid in [11, 12] {
            assert!(
                m.fs.exists(&paths.health().join(format!("start-{pid}"))),
                "start-{pid} stays with the pause: {err}"
            );
        }
    }

    #[test]
    fn an_apply_with_no_pause_still_makes_the_folder_init_lua_writes_its_tags_in() {
        // A fresh machine has no health\ at all; the first window's tag needs it (a fake's `put`
        // makes parent folders, so this is the one way to see `clear_pause` make it).
        let (m, d) = machine();
        let health = Paths {
            data: d.data_dir.clone(),
        }
        .health();
        assert!(
            !m.fs.exists(&health),
            "a fresh machine has no health folder"
        );
        let (code, out, err) = cli(&m, &d, &["apply", THEME]);
        assert_eq!((code, err.as_str()), (EXIT_OK, ""), "{out}");
        assert!(m.fs.is_dir(&health), "init.lua writes its tags there");
        assert!(m.fs.list_dir(&health).unwrap().is_empty());
    }

    #[test]
    fn an_apply_to_windows_terminal_only_lifts_the_pause_too() {
        // Review Focus 5: every committed apply lifts it, whatever the target.
        let (m, d) = machine();
        let paths = paused_in_cmd(&m, &d);
        let (code, out, err) = cli(&m, &d, &["apply", THEME, "--target", "wt"]);
        assert_eq!((code, err.as_str()), (EXIT_OK, ""), "{out}");
        assert!(
            out.contains("Open a new cmd tab in Windows Terminal"),
            "{out}"
        );
        assert!(!m.fs.exists(&paths.paused_file()));
        assert!(m.fs.list_dir(&paths.health()).unwrap().is_empty());
    }

    #[test]
    fn a_pause_that_cannot_be_lifted_is_said_and_the_apply_stands() {
        // Whatever the target, the hint to open a new window is not said: the line on stderr says
        // what to do instead.
        for target in ["all", "wt"] {
            let (m, d) = machine();
            let paths = paused_in_cmd(&m, &d);
            m.fs.fail_removes_of(paths.paused_file());
            let (code, out, err) = cli(&m, &d, &["apply", THEME, "--target", target]);
            assert_applied_but_not_cleared(code, &out, &err, &paths);
            assert!(m.fs.exists(&paths.paused_file()), "{target}");
        }
    }

    #[test]
    fn a_tag_that_cannot_be_deleted_is_said_without_telling_to_delete_the_pause_alone() {
        // A virus scanner holds a tag: `clear_pause` stops before the pause, which stays on
        // purpose, so the next window does not pause the theme again at once on three stale
        // tags. Deleting paused.json alone would do just that, so the line must not say it.
        let (m, d) = machine();
        let paths = paused_in_cmd(&m, &d);
        m.fs.fail_removes_of(paths.health().join("start-12"));
        let (code, out, err) = cli(&m, &d, &["apply", THEME]);
        assert_applied_but_not_cleared(code, &out, &err, &paths);
        assert!(!err.contains("paused.json"), "{err}");
        assert!(err.contains("start-12"), "the file held is named: {err}");
        assert!(m.fs.exists(&paths.paused_file()));
        assert!(m.fs.exists(&paths.health().join("start-12")));
    }

    #[test]
    fn records_that_cannot_be_cleared_with_no_pause_never_claim_a_pause() {
        // Windows that never reached their prompt leave tags; a held one fails the clean-up of
        // an apply with no pause at all. The line must not talk about a pause that is not there.
        let (m, d) = machine();
        let paths = Paths {
            data: d.data_dir.clone(),
        };
        for pid in [11, 12] {
            m.fs.put(
                paths.health().join(format!("start-{pid}")),
                b"v1 1791694709\n",
            );
        }
        m.fs.fail_removes_of(paths.health().join("start-12"));
        let (code, out, err) = cli(&m, &d, &["apply", THEME]);
        assert_applied_but_not_cleared(code, &out, &err, &paths);
        assert!(!err.contains("pause"), "{err}");
        assert!(!m.fs.exists(&paths.paused_file()));
    }

    #[test]
    fn the_failure_to_clear_shows_a_thai_data_folder_as_written_in_english_words() {
        let (m, d) = machine();
        let d = Defaults {
            data_dir: PathBuf::from(r"C:\Users\tester\AppData\Local\ข้อมูล"),
            ..d
        };
        let paths = paused_in_cmd(&m, &d);
        m.fs.fail_removes_of(paths.paused_file());
        let (code, out, err) = cli(&m, &d, &["apply", THEME]);
        assert_eq!(code, EXIT_OK, "{out}{err}");
        assert!(err.starts_with(NOT_CLEARED), "{err}");
        let data = d.data_dir.display().to_string();
        assert!(err.contains(&data), "{err}");
        assert!(!err.contains("\\u{"), "{err}");
        // The folder's name is the only Thai there is, each time the path is shown (the folder
        // and the file the error names); the words around it are CMD Studio's, and ASCII.
        assert_eq!(
            thai(&err),
            thai(&data).repeat(err.matches(&data).count()),
            "{err}"
        );
        assert!(err.replace(&data, "").is_ascii(), "{err}");
    }
}
