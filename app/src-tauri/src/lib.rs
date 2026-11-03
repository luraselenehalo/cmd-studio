//! CMD Studio's desktop app (Tauri 2): a window over `cmdstudio-core`. The IPC commands of
//! spec §5.7 are thin wrappers (`ipc`) over a [`backend::Backend`], which is tested against the
//! core's in-memory adapters; the theme library and drafts (`library`), the app's settings
//! (`settings`) and its log (`logs`) live in the data folder (spec §5.3); the CLI is the same
//! exe (`main.rs`). The UI (`app/ui`) paints what the core computes: no palette, contrast or
//! prompt logic lives in TypeScript (spec §5.6).
pub mod backend;
pub mod command_names;
pub mod console;
pub mod crash;
pub mod error;
#[cfg(windows)]
pub mod ipc;
pub mod library;
pub mod logs;
#[cfg(windows)]
pub mod machine;
pub mod open;
pub mod settings;

/// The one argument that opens the window instead of the CLI (recovery spec §4.4, §8).
pub const RECOVERY_ARG: &str = "--recovery";

#[cfg(test)]
mod config_guard;

/// Opens the window. Everything that can fail before the window is up ends in a message box
/// (English: the UI is not there yet) and exit code 1: the data folder (`real_backend`), Tauri's
/// own build, and the window itself, which is made in `setup`. Tauri runs `setup` inside
/// `App::run` and answers an error returned from it with a panic of its own ("Failed to setup
/// app"), which a release build ends without a word, so a window that cannot be built (no
/// WebView2, a `<data>\webview` folder that cannot be made) is handled inside `setup` and never
/// returned to Tauri. After that, Tauri owns the thread.
#[cfg(windows)]
pub fn run_gui(how: backend::run::GuiStart) {
    use backend::run::GuiStart;
    use tauri::Manager;
    let backend = match machine::real_backend() {
        Ok(backend) => std::sync::Arc::new(backend),
        Err(why) => {
            console::fatal(&start_failed(&why));
            std::process::exit(1);
        }
    };
    // The run's record, before the window (recovery spec §5.1); `--recovery`'s own steps run on
    // a thread once the window is up, so a slow rollback never delays the window.
    let decided = backend.start_run(how);
    let for_window = backend.clone();
    let for_instance = backend.clone();
    let for_exit = backend.clone();
    let built = tauri::Builder::default()
        // A second start focuses this window instead of opening another (spec §5.7); one with
        // `--recovery` makes this instance do the shortcut's work and reload into recovery mode
        // (recovery spec §4.4). Registered first, as the plugin asks.
        .plugin(tauri_plugin_single_instance::init(
            move |app, args, _cwd| {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.unminimize();
                    let _ = window.set_focus();
                    if args.iter().any(|a| a == RECOVERY_ARG) {
                        recover_in_place(&for_instance, window);
                    }
                }
            },
        ))
        .manage(backend)
        .invoke_handler(ipc::handler())
        .setup(move |app| {
            // Said and ended here, not returned: Tauri would turn the error into a panic.
            if let Err(why) = open_main_window(app, for_window.paths().webview()) {
                console::fatal(&window_failed(&why));
                std::process::exit(1);
            }
            if how == GuiStart::Recovery {
                start_recovery_entry(&for_window);
            } else if decided.safe_mode.is_none() {
                // Recovery mode starts no crash check (recovery spec §4.3).
                start_crash_check(&for_window);
            }
            Ok(())
        })
        .build(tauri::generate_context!());
    match built {
        Ok(app) => app.run(move |_, event| {
            if let tauri::RunEvent::Exit = event {
                for_exit.end_run();
            }
        }),
        Err(e) => {
            console::fatal(&start_failed(&e.to_string()));
            std::process::exit(1);
        }
    }
}

/// `--recovery`'s steps 1–2 on a thread of their own (recovery spec §4.4), then `after`. The
/// entry is `busy` under its own name while it runs, so the UI's start waits for it (§6.3); it
/// is marked before the thread exists, so a UI that asks in between does not find it idle. A
/// thread that cannot be started clears the mark and leaves the reason as the entry's outcome
/// for the Recovery page; nothing waits on the window's thread.
#[cfg(windows)]
fn spawn_recovery_entry(
    backend: &std::sync::Arc<backend::Backend<machine::RealMachine>>,
    after: impl FnOnce() + Send + 'static,
) {
    let b = backend.clone();
    backend.busy_mark(backend::run::RECOVERY_ENTRY_BUSY);
    let spawned = std::thread::Builder::new()
        .name("recovery-entry".to_string())
        .spawn(move || {
            let _ = b.recovery_entry();
            b.busy_clear();
            after();
        });
    if let Err(e) = spawned {
        backend.busy_clear();
        backend.recovery_entry_not_run(&e.to_string());
    }
}

/// The start with `--recovery`: the entry once the window is up; the window's first
/// `app_status` finds it busy and asks again until it has ended.
#[cfg(windows)]
fn start_recovery_entry(backend: &std::sync::Arc<backend::Backend<machine::RealMachine>>) {
    spawn_recovery_entry(backend, || {});
}

/// A second start with `--recovery` while this one runs (recovery spec §4.4): the entry, then
/// the page is loaded again; the reloaded UI asks `app_status` first and finds safe mode on.
#[cfg(windows)]
fn recover_in_place(
    backend: &std::sync::Arc<backend::Backend<machine::RealMachine>>,
    window: tauri::WebviewWindow,
) {
    backend.enter_safe_mode(backend::run::SafeModeReason::Recovery);
    spawn_recovery_entry(backend, move || {
        let _ = window.eval("window.location.reload()");
    });
}

/// Builds the window, the only place it is made. It is in tauri.conf.json with
/// `create: false`, so that WebView2's data folder can be set here: `webview`, a folder of CMD
/// Studio's choosing in its data folder, which `--cleanup` knows (spec §5.3). Tauri's own
/// default would be the data folder itself, where WebView2 makes an `EBWebView` folder of its
/// own. The window's size is the config's, 1100x760 and no less than 900x620, made smaller to fit
/// the primary monitor's work area when the screen is smaller than that (`preventOverflow` in
/// tauri.conf.json, pinned by `config_guard`): Tauri subtracts the title bar and the frame, works in
/// physical pixels with the monitor's scale factor, and keeps the config's size when no monitor
/// can be read. A 1024x768 screen's window used to run under the taskbar.
#[cfg(windows)]
fn open_main_window(app: &tauri::App, webview: std::path::PathBuf) -> Result<(), String> {
    let config = main_window_config(&app.config().app.windows)?;
    tauri::WebviewWindowBuilder::from_config(app.handle(), &config)
        .and_then(|window| window.data_directory(webview).build())
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Windows' crash reports of cmd, read once the window is up, on a thread of its own (recovery
/// spec §4.3): the window never waits for them, and `recovery_status` waits for the answer. A
/// thread that cannot be started is said as a check that could not run, so nothing waits for
/// it. Phase 3b-2's recovery mode starts none.
#[cfg(windows)]
fn start_crash_check(backend: &std::sync::Arc<backend::Backend<machine::RealMachine>>) {
    let checker = backend.clone();
    start_crash_check_with(backend, move || {
        std::thread::Builder::new()
            .name("crash-check".to_string())
            .spawn(move || {
                let _ = checker.crash_check();
            })
            .map(|_| ())
    });
}

/// The check is marked as on its way before `spawn` starts its thread, so that a
/// `recovery_status` that comes in between waits for it instead of finding none; a `spawn` that
/// fails is said as a check that could not run, and nothing waits for it. Split from
/// [`start_crash_check`] so that the order and the failure are tested without a real thread.
#[cfg(windows)]
fn start_crash_check_with<M: backend::Machine>(
    backend: &backend::Backend<M>,
    spawn: impl FnOnce() -> std::io::Result<()>,
) {
    backend.crash_check_started();
    if let Err(e) = spawn() {
        backend.crash_check_not_run(&format!("the check could not start: {e}"));
    }
}

/// The window labelled `main` in tauri.conf.json.
#[cfg(windows)]
fn main_window_config(
    windows: &[tauri::utils::config::WindowConfig],
) -> Result<tauri::utils::config::WindowConfig, String> {
    windows
        .iter()
        .find(|w| w.label == "main")
        .cloned()
        .ok_or_else(|| "tauri.conf.json has no window labelled main".to_string())
}

/// The message box of a start that failed before Tauri's build was done.
#[cfg(windows)]
fn start_failed(why: &str) -> String {
    format!("CMD Studio could not start: {why}")
}

/// The message box of a window that could not be built.
#[cfg(windows)]
fn window_failed(why: &str) -> String {
    format!("CMD Studio could not open its window: {why}")
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    /// The window `setup` builds is the config's `main` entry; a config without it, and a
    /// window that fails, end in the app's own message box, in English (the UI is not up).
    /// Showing the box and exiting need Tauri's event loop and end the process, so they are
    /// not run here.
    #[test]
    fn the_window_is_the_main_entry_and_a_failure_is_said_in_english() {
        let config: tauri::utils::config::Config =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let main = main_window_config(&config.app.windows).unwrap();
        assert_eq!(main.label, "main");
        assert!(!main.create, "setup makes the window, Tauri must not");

        let none = main_window_config(&[]).unwrap_err();
        assert_eq!(none, "tauri.conf.json has no window labelled main");
        assert_eq!(
            window_failed(&none),
            "CMD Studio could not open its window: tauri.conf.json has no window labelled main"
        );
        assert_eq!(
            start_failed("no LOCALAPPDATA"),
            "CMD Studio could not start: no LOCALAPPDATA"
        );
        for text in [window_failed("x"), start_failed("x")] {
            assert!(text.is_ascii(), "{text}");
        }
    }

    const SOURCE: &str = include_str!("lib.rs");

    /// This file without its tests: the needles below also stand in the tests themselves.
    fn production() -> &'static str {
        match SOURCE.split_once("\n#[cfg(all(test, windows))]") {
            Some((text, _)) => text,
            None => "",
        }
    }

    /// The text of the function that follows `head`, up to its closing brace at the margin; empty
    /// when there is none, which the tests' `find`s then fail on.
    fn body_of(head: &str) -> &'static str {
        let after = production().split_once(head).map_or("", |(_, after)| after);
        after.split_once("\n}\n").map_or("", |(body, _)| body)
    }

    /// A check that is on its way is waited for, and its answer is the status. The spawner here
    /// runs it at once, on this thread: the fake machine is not `Send`.
    #[test]
    fn a_check_that_started_is_waited_for_and_its_answer_is_the_status() {
        let b = crate::backend::testing::backend();
        start_crash_check_with(&b, || {
            let _ = b.crash_check();
            Ok(())
        });
        assert_eq!(
            b.recovery_status().unwrap().crashes,
            Some(crate::backend::recovery::CrashCheck::NoTheme)
        );
    }

    /// Spec §8: a thread that cannot be started is a check that could not run, and
    /// `recovery_status` answers at once instead of waiting 30 s for it.
    #[test]
    fn a_thread_that_cannot_start_is_a_check_that_could_not_run_and_nothing_waits_for_it() {
        use crate::backend::recovery::CrashCheck;
        let b = crate::backend::testing::backend();
        start_crash_check_with(&b, || Err(std::io::Error::other("out of threads")));
        let asked = std::time::Instant::now();
        let status = b.recovery_status().unwrap();
        assert!(asked.elapsed() < std::time::Duration::from_secs(5));
        match status.crashes {
            Some(CrashCheck::Unreadable { why, pause: None }) => {
                assert!(why.contains("could not start"), "{why}");
                assert!(why.contains("out of threads"), "{why}");
            }
            other => panic!("not said as a check that could not run: {other:?}"),
        }
    }

    /// The wiring that the tests above cannot see (the real thread needs the real machine):
    /// the check is marked as on its way before its thread is spawned (so that a
    /// `recovery_status` that comes in between waits for it), a failed spawn is said, the
    /// thread is named and runs the check once, and `setup` starts it once, after the window.
    #[test]
    fn the_check_is_marked_before_its_thread_starts_and_started_once_after_the_window() {
        let with = body_of("fn start_crash_check_with<");
        let marked = with.find(".crash_check_started()").unwrap();
        let spawned = with.find("if let Err(e) = spawn()").unwrap();
        let not_run = with.find(".crash_check_not_run(").unwrap();
        assert!(marked < spawned, "marked first");
        assert!(spawned < not_run, "said only when the spawn failed");

        let real = body_of("fn start_crash_check(");
        assert!(real.contains("start_crash_check_with("));
        assert!(real.contains(".name(\"crash-check\".to_string())"));
        assert_eq!(
            real.matches(".crash_check()").count(),
            1,
            "the check runs once"
        );

        let text = production();
        assert_eq!(text.matches("start_crash_check(&for_window)").count(), 1);
        let window = text.find("open_main_window(app, ").unwrap();
        let check = text.find("start_crash_check(&for_window)").unwrap();
        assert!(window < check, "the check starts once the window is up");
        let record = text.find("backend.start_run(how)").unwrap();
        assert!(record < window, "the run's record comes before the window");
        assert!(
            text.contains("tauri::RunEvent::Exit"),
            "a normal exit ends the run"
        );
        assert!(production().contains("for_exit.end_run()"));
    }

    /// `--recovery` alone opens the window in recovery mode; every other argument is the CLI's
    /// (recovery spec §4.4, §8). `main.rs` is read as text: the binary is never started here.
    #[test]
    fn only_the_recovery_argument_opens_the_window() {
        let main = include_str!("main.rs");
        assert_eq!(RECOVERY_ARG, "--recovery");
        assert!(
            main.contains("[one] if one == cmdstudio_app::RECOVERY_ARG"),
            "{main}"
        );
        assert!(main.contains("GuiStart::Recovery"));
        let gui = main.find("run_gui(GuiStart::Normal)").unwrap();
        let cli = main.find("cmdstudio_cli::run(").unwrap();
        assert!(gui < cli, "the window's cases come before the CLI");
        assert!(
            main.find("crash::install_hook").unwrap() < main.find("let args").unwrap(),
            "the hook comes first"
        );
    }

    /// Carry 3: the entry is marked busy before its thread exists (a UI that asks in between
    /// does not find it idle), and a thread that cannot be started clears the mark and leaves
    /// the reason for the Recovery page.
    #[test]
    fn the_entry_is_marked_before_its_thread_and_a_failed_spawn_clears_it() {
        let body = body_of("fn spawn_recovery_entry(");
        let marked = body.find(".busy_mark(").unwrap();
        let spawned = body.find(".spawn(").unwrap();
        let failed = body.find("if let Err(e) = spawned").unwrap();
        let cleared = body.rfind(".busy_clear()").unwrap();
        let said = body.find(".recovery_entry_not_run(").unwrap();
        assert!(marked < spawned, "marked first");
        assert!(
            spawned < failed && failed < said,
            "said only after a failed spawn"
        );
        assert!(
            cleared > failed,
            "the mark is cleared after the failed spawn is seen"
        );
        assert!(
            body.matches(".busy_clear()").count() == 2,
            "cleared when the thread ends and when it could not start"
        );
        assert!(body.contains(".recovery_entry()"));
    }
}

#[cfg(not(windows))]
pub fn run_gui(_how: backend::run::GuiStart) {
    console::fatal("CMD Studio runs on Windows only.");
}
