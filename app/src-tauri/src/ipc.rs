//! The IPC commands (spec §5.7), one Tauri command each, as thin wrappers over the
//! [`Backend`] and the core: they move arguments and results across, run the work that touches
//! the machine off the window's thread, and log it. Nothing is decided here, so the tests of
//! what a command does are the backend's; the tests here hold the list of commands to
//! `command_names::COMMANDS`, to what Tauri registers and to what `capabilities/default.json`
//! allows (spec §5.10), and check which commands log.
//!
//! Every command that reads or changes the machine, the data folder or the settings (23 of
//! the 27) writes exactly one log line through [`Backend::logged`]: its name (a constant of
//! `command_names`, never text built at run time), how long it took, and `ok` or the error's
//! code. The two that ask the user for a file (`theme_import_preview`, `theme_export`) log once
//! for the whole command: the time includes the user's time in the dialog, and a cancel is
//! `ok`.
//!
//! The four pure commands, `palette_generate`, `contrast_check`, `contrast_fix` and
//! `preview_render`, do not log. They compute over the core and touch neither the machine nor
//! a file nor the settings, and the log is there to diagnose what does. They run on each
//! settled edit or slider move, and each log line rewrites the log file (up to 1 MB), which
//! would be on the window's thread: they are sync, so Tauri runs them there one at a time and
//! their answers come back in the order they were asked. Each runs inside `caught`, so a
//! panic in one is an `E_INTERNAL` answer, not the end of the app.
use crate::backend::apply::{
    ApplyOutcome, ApplyProgress, ApplyRequest, PlanOutcome, RestoreOutcome, RestorePoints,
    RevertOutcome, UndoOutcome,
};
use crate::backend::doctor::{DoctorReport, FixProposal};
use crate::backend::preview::{self, ContrastIssue, PreviewResult};
use crate::backend::system::SystemReport;
use crate::backend::{AdvancedInfo, Backend};
use crate::command_names as names;
use crate::error::AppError;
use crate::library::{ImportPreview, Library, ThemeDetail, ThemeSummary, THEME_EXT};
use crate::machine::RealMachine;
use crate::open::{self, ExternalTarget};
use crate::settings::{AppSettings, SettingsLoad};
use cmdstudio_core::model::Theme;
use cmdstudio_core::palette::{Colors, Tone};
use cmdstudio_core::preview::PreviewContext;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State as TauriState, WebviewWindow};
use uuid::Uuid;

/// The event `apply` emits for each step (spec §4.2 page 5).
pub const APPLY_PROGRESS_EVENT: &str = "apply-progress";

type Shared = Arc<Backend<RealMachine>>;

/// Runs `f` on a worker thread, so a command that runs Oh My Posh or Clink, or waits for a file
/// dialog, never blocks the window's thread (sync commands run there).
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, AppError> + Send + 'static,
) -> Result<T, AppError> {
    match tauri::async_runtime::spawn_blocking(f).await {
        Ok(result) => result,
        Err(e) => Err(AppError::internal(format!(
            "the worker that ran the command did not answer: {e}"
        ))),
    }
}

/// Runs a pure command's computation and turns a panic in it into [`AppError::internal`].
/// The four pure commands are sync, so they run on the window's thread, inside Tauri's event
/// loop: a panic unwinding out of them would take the whole app down. The commands that run on
/// a worker ([`blocking`]) need no such guard: a panic there ends the worker, and the command
/// answers `E_INTERNAL`.
fn caught<T>(compute: impl FnOnce() -> T) -> Result<T, AppError> {
    // UNWIND SAFETY: `compute` reaches only the command's own arguments, which it owns or
    // borrows from this call and which are dropped with it; it shares no state with another
    // command and writes nothing, so nothing is left half changed when it unwinds.
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(compute)) {
        Ok(value) => Ok(value),
        Err(payload) => {
            // Dropping the payload of a panic runs its code, which could panic too.
            std::mem::forget(payload);
            Err(AppError::internal(
                "the computation stopped on an internal error (a panic)",
            ))
        }
    }
}

/// Shows the native file dialog for a `.cmdtheme` file, with the window as its parent, and
/// waits for the user's answer: the path, or `None` when they cancelled. The dialog comes here
/// with its filter and parent set, and `show` adds what differs and shows it: the Open dialog
/// (`pick_file`), or the Save dialog with a file name to offer (`save_file`).
///
/// A modal dialog must be shown on the window's thread, so this asks that thread to show it
/// and blocks the calling thread until the dialog is closed. It is called only on a worker
/// ([`blocking`]), never on the window's thread itself.
fn ask_for_theme_file(
    window: &WebviewWindow,
    show: impl FnOnce(rfd::FileDialog) -> Option<PathBuf> + Send + 'static,
) -> Result<Option<PathBuf>, AppError> {
    let (tx, rx) = std::sync::mpsc::channel();
    let parent = window.clone();
    window
        .run_on_main_thread(move || {
            let dialog = rfd::FileDialog::new()
                .add_filter("CMD Studio theme", &[THEME_EXT])
                .set_parent(&parent);
            // `rx` waits for this below; were it gone, there would be nobody to tell.
            let _ = tx.send(show(dialog));
        })
        .map_err(|e| AppError::internal(format!("the file dialog could not be shown: {e}")))?;
    rx.recv()
        .map_err(|e| AppError::internal(format!("the file dialog gave no answer: {e}")))
}

#[tauri::command]
pub async fn system_detect(state: TauriState<'_, Shared>) -> Result<SystemReport, AppError> {
    let b = state.inner().clone();
    blocking(move || b.logged(names::SYSTEM_DETECT, |b| b.system_detect())).await
}

#[tauri::command]
pub async fn themes_list(state: TauriState<'_, Shared>) -> Result<Library, AppError> {
    let b = state.inner().clone();
    blocking(move || b.logged(names::THEMES_LIST, |b| b.themes_list())).await
}

#[tauri::command]
pub async fn theme_get(state: TauriState<'_, Shared>, id: Uuid) -> Result<ThemeDetail, AppError> {
    let b = state.inner().clone();
    blocking(move || b.logged(names::THEME_GET, |b| b.theme_get(id))).await
}

#[tauri::command]
pub async fn theme_save(
    state: TauriState<'_, Shared>,
    theme: Theme,
    draft: bool,
) -> Result<ThemeSummary, AppError> {
    let b = state.inner().clone();
    blocking(move || b.logged(names::THEME_SAVE, |b| b.theme_save(theme, draft))).await
}

#[tauri::command]
pub async fn theme_duplicate(
    state: TauriState<'_, Shared>,
    id: Uuid,
    name: String,
) -> Result<ThemeSummary, AppError> {
    let b = state.inner().clone();
    blocking(move || b.logged(names::THEME_DUPLICATE, |b| b.theme_duplicate(id, name))).await
}

#[tauri::command]
pub async fn theme_delete(
    state: TauriState<'_, Shared>,
    id: Uuid,
    draft_only: bool,
) -> Result<(), AppError> {
    let b = state.inner().clone();
    blocking(move || b.logged(names::THEME_DELETE, |b| b.theme_delete(id, draft_only))).await
}

/// Import, step one (spec §3.1): the Open dialog for a `.cmdtheme` file, then what the file
/// holds, kept under a token for `theme_import_confirm`. `None` when the user cancelled.
#[tauri::command]
pub async fn theme_import_preview(
    window: WebviewWindow,
    state: TauriState<'_, Shared>,
) -> Result<Option<ImportPreview>, AppError> {
    let b = state.inner().clone();
    blocking(move || {
        b.logged(names::THEME_IMPORT_PREVIEW, |b| {
            match ask_for_theme_file(&window, rfd::FileDialog::pick_file)? {
                Some(path) => b.theme_import_preview_from(&path).map(Some),
                None => Ok(None),
            }
        })
    })
    .await
}

#[tauri::command]
pub async fn theme_import_confirm(
    state: TauriState<'_, Shared>,
    token: String,
) -> Result<ThemeSummary, AppError> {
    let b = state.inner().clone();
    blocking(move || {
        b.logged(names::THEME_IMPORT_CONFIRM, |b| {
            b.theme_import_confirm(&token)
        })
    })
    .await
}

/// Exports a theme to a file the user picks in the Save dialog, which offers the theme's name:
/// the path written, or `None` when the user cancelled.
#[tauri::command]
pub async fn theme_export(
    window: WebviewWindow,
    state: TauriState<'_, Shared>,
    id: Uuid,
) -> Result<Option<String>, AppError> {
    let b = state.inner().clone();
    blocking(move || {
        b.logged(names::THEME_EXPORT, |b| {
            let file_name = b.export_file_name(id)?;
            let chosen = ask_for_theme_file(&window, move |dialog| {
                dialog.set_file_name(file_name).save_file()
            })?;
            let Some(path) = chosen else {
                return Ok(None);
            };
            b.theme_export_to(id, &path)?;
            Ok(Some(path.display().to_string()))
        })
    })
    .await
}

#[tauri::command]
pub fn palette_generate(tone: Tone, accent: String, tint: f64) -> Result<Colors, AppError> {
    caught(|| preview::palette_generate(tone, &accent, tint))?
}

#[tauri::command]
pub fn contrast_check(colors: Colors) -> Result<Vec<ContrastIssue>, AppError> {
    caught(|| preview::contrast_check(&colors))
}

#[tauri::command]
pub fn contrast_fix(colors: Colors) -> Result<Colors, AppError> {
    caught(|| preview::contrast_fix(colors))
}

#[tauri::command]
pub fn preview_render(theme: Theme, context: PreviewContext) -> Result<PreviewResult, AppError> {
    caught(|| preview::preview_render(&theme, &context))
}

#[tauri::command]
pub async fn plan_apply(
    state: TauriState<'_, Shared>,
    request: ApplyRequest,
) -> Result<PlanOutcome, AppError> {
    let b = state.inner().clone();
    blocking(move || b.logged(names::PLAN_APPLY, |b| b.plan_apply(request))).await
}

/// Applies the plan the user saw (`digest`), emitting [`APPLY_PROGRESS_EVENT`] for each step.
#[tauri::command]
pub async fn apply(
    app: AppHandle,
    state: TauriState<'_, Shared>,
    request: ApplyRequest,
    digest: String,
) -> Result<ApplyOutcome, AppError> {
    let b = state.inner().clone();
    blocking(move || {
        // The listener runs between two steps of the transaction (`Backend::apply`): it must
        // not panic, and an emit that fails (the window is gone) is ignored, since the
        // transaction runs to its end either way and its outcome is the answer.
        let mut tell = |p: ApplyProgress| {
            let _ = app.emit(APPLY_PROGRESS_EVENT, &p);
        };
        b.logged(names::APPLY, |b| b.apply(request, &digest, &mut tell))
    })
    .await
}

#[tauri::command]
pub async fn undo_last(state: TauriState<'_, Shared>) -> Result<UndoOutcome, AppError> {
    let b = state.inner().clone();
    blocking(move || b.logged(names::UNDO_LAST, |b| b.undo_last())).await
}

#[tauri::command]
pub async fn snapshots_list(state: TauriState<'_, Shared>) -> Result<RestorePoints, AppError> {
    let b = state.inner().clone();
    blocking(move || b.logged(names::SNAPSHOTS_LIST, |b| b.snapshots_list())).await
}

#[tauri::command]
pub async fn snapshot_restore(
    state: TauriState<'_, Shared>,
    id: String,
) -> Result<RestoreOutcome, AppError> {
    let b = state.inner().clone();
    blocking(move || b.logged(names::SNAPSHOT_RESTORE, |b| b.snapshot_restore(&id))).await
}

#[tauri::command]
pub async fn revert_all(state: TauriState<'_, Shared>) -> Result<RevertOutcome, AppError> {
    let b = state.inner().clone();
    blocking(move || b.logged(names::REVERT_ALL, |b| b.revert_all())).await
}

#[tauri::command]
pub async fn doctor_run(state: TauriState<'_, Shared>) -> Result<DoctorReport, AppError> {
    let b = state.inner().clone();
    blocking(move || b.logged(names::DOCTOR_RUN, |b| b.doctor_run())).await
}

#[tauri::command]
pub async fn doctor_fix(
    state: TauriState<'_, Shared>,
    check_id: String,
) -> Result<FixProposal, AppError> {
    let b = state.inner().clone();
    blocking(move || b.logged(names::DOCTOR_FIX, |b| b.doctor_fix(&check_id))).await
}

#[tauri::command]
pub async fn settings_get(state: TauriState<'_, Shared>) -> Result<SettingsLoad, AppError> {
    let b = state.inner().clone();
    blocking(move || b.logged(names::SETTINGS_GET, |b| b.settings_get())).await
}

#[tauri::command]
pub async fn settings_set(
    state: TauriState<'_, Shared>,
    settings: AppSettings,
) -> Result<AppSettings, AppError> {
    let b = state.inner().clone();
    blocking(move || b.logged(names::SETTINGS_SET, |b| b.settings_set(settings))).await
}

/// Opens a cmd window: the one `Backend::cmd_launch` decides (spec §4.3), started by
/// `open::open_cmd`.
#[tauri::command]
pub async fn open_cmd(state: TauriState<'_, Shared>) -> Result<(), AppError> {
    let b = state.inner().clone();
    blocking(move || {
        b.logged(names::OPEN_CMD, |b| {
            let launch = b.cmd_launch();
            open::open_cmd(&launch).map_err(AppError::internal)
        })
    })
    .await
}

/// Opens one of the four fixed places (`ExternalTarget`) outside the window.
#[tauri::command]
pub async fn open_external(
    state: TauriState<'_, Shared>,
    target: ExternalTarget,
) -> Result<(), AppError> {
    let b = state.inner().clone();
    blocking(move || {
        b.logged(names::OPEN_EXTERNAL, |b| {
            open::open_external(target, &b.paths().data, &b.paths().logs())
                .map_err(AppError::internal)
        })
    })
    .await
}

#[tauri::command]
pub async fn diagnostics_copy(state: TauriState<'_, Shared>) -> Result<String, AppError> {
    let b = state.inner().clone();
    blocking(move || b.logged(names::DIAGNOSTICS_COPY, |b| b.diagnostics_copy())).await
}

#[tauri::command]
pub async fn advanced_read(state: TauriState<'_, Shared>) -> Result<AdvancedInfo, AppError> {
    let b = state.inner().clone();
    blocking(move || b.logged(names::ADVANCED_READ, |b| b.advanced_read())).await
}

/// Every command, for `tauri::Builder::invoke_handler`.
pub fn handler() -> impl Fn(tauri::ipc::Invoke<tauri::Wry>) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        system_detect,
        themes_list,
        theme_get,
        theme_save,
        theme_duplicate,
        theme_delete,
        theme_import_preview,
        theme_import_confirm,
        theme_export,
        palette_generate,
        contrast_check,
        contrast_fix,
        preview_render,
        plan_apply,
        apply,
        undo_last,
        snapshots_list,
        snapshot_restore,
        revert_all,
        doctor_run,
        doctor_fix,
        settings_get,
        settings_set,
        open_cmd,
        open_external,
        diagnostics_copy,
        advanced_read,
    ]
}

#[cfg(test)]
mod tests {
    use crate::command_names::COMMANDS;
    use std::collections::BTreeSet;

    const SOURCE: &str = include_str!("ipc.rs");
    const CAPABILITY: &str = include_str!("../capabilities/default.json");

    #[test]
    fn every_named_command_is_a_tauri_command_here_and_in_the_handler_and_no_other() {
        // The handler list is the text between `generate_handler![` and `]`.
        let start = SOURCE.find("tauri::generate_handler![").unwrap();
        let rest = SOURCE.get(start..).unwrap();
        let end = rest.find(']').unwrap();
        let listed: BTreeSet<&str> = rest
            .get(..end)
            .unwrap()
            .split([',', '\n', ' ', '!', '['])
            .filter(|w| !w.is_empty() && *w != "tauri::generate_handler")
            .collect();
        let named: BTreeSet<&str> = COMMANDS.iter().copied().collect();
        assert_eq!(listed, named);
        for name in COMMANDS {
            let decl_async = format!("pub async fn {name}(");
            let decl = format!("pub fn {name}(");
            assert!(
                SOURCE.contains(&decl_async) || SOURCE.contains(&decl),
                "{name} has no #[tauri::command] function"
            );
        }
        assert_eq!(COMMANDS.len(), 27);
    }

    #[test]
    fn the_capability_allows_exactly_the_named_commands_and_the_event() {
        let capability: serde_json::Value = serde_json::from_str(CAPABILITY).unwrap();
        let permissions: BTreeSet<String> = capability["permissions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p.as_str().unwrap().to_string())
            .collect();
        let mut expected: BTreeSet<String> = COMMANDS
            .iter()
            .map(|c| format!("allow-{}", c.replace('_', "-")))
            .collect();
        expected.insert("core:event:allow-listen".to_string());
        expected.insert("core:event:allow-unlisten".to_string());
        assert_eq!(permissions, expected);
        assert_eq!(capability["windows"], serde_json::json!(["main"]));
    }

    /// The four pure commands: they compute over the core and touch neither the machine nor a
    /// file nor the settings.
    const PURE: [&str; 4] = [
        "palette_generate",
        "contrast_check",
        "contrast_fix",
        "preview_render",
    ];

    /// Ruling F11 of the pre-flight review, as revised: each of the 23 commands that read or
    /// change the machine, the data folder or the settings writes one log line through
    /// `Backend::logged`, under its own name from `command_names` (a constant, never text built
    /// at run time). The four pure commands write none, and stay sync.
    #[test]
    fn each_command_logs_once_under_its_own_name_and_the_pure_four_do_not_log() {
        // Each command is the text from its attribute, at the start of a line, to the next one;
        // the last ends where `handler()` starts.
        let attribute = "\n#[tauri::command]\n";
        let first = SOURCE.find(attribute).unwrap();
        let end = SOURCE.find("\npub fn handler()").unwrap();
        let commands: Vec<&str> = SOURCE
            .get(first..end)
            .unwrap()
            .split(attribute)
            .filter(|c| !c.trim().is_empty())
            .collect();
        assert_eq!(commands.len(), COMMANDS.len());
        for name in PURE {
            assert!(COMMANDS.contains(&name), "{name} is not a command");
        }
        let mut logging = 0;
        for name in COMMANDS {
            let own: Vec<&&str> = commands
                .iter()
                .filter(|c| c.contains(&format!(" fn {name}(")))
                .collect();
            assert_eq!(own.len(), 1, "{name} is one command");
            let body = own[0];
            if PURE.contains(name) {
                assert!(body.contains(&format!("pub fn {name}(")), "{name} is sync");
                assert!(body.contains("caught(|| "), "{name} catches a panic");
                assert!(!body.contains("logged("), "{name} writes no log line");
                assert!(!body.contains("names::"), "{name} names no command");
                continue;
            }
            logging += 1;
            assert_eq!(body.matches(".logged(").count(), 1, "{name} logs once");
            assert_eq!(
                body.matches("names::").count(),
                1,
                "{name} names one command"
            );
            let constant = format!("names::{}", name.to_uppercase());
            assert!(
                body.contains(&format!("{constant},")),
                "{name} logs under {constant}"
            );
        }
        assert_eq!(logging, 23);
    }

    /// A pure command runs on the window's thread: a panic in it must come back as an error,
    /// never unwind into Tauri's event loop.
    #[test]
    fn a_panic_in_a_pure_command_is_an_internal_error() {
        assert_eq!(super::caught(|| 7), Ok(7));
        let err = super::caught(|| -> u8 { panic!("a bug in the core") }).unwrap_err();
        assert_eq!(err.code, crate::error::INTERNAL);
        assert!(err.details.contains("panic"), "{}", err.details);
    }
}
