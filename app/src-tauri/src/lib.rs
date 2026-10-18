//! CMD Studio's desktop app (Tauri 2): a window over `cmdstudio-core`. The IPC commands of
//! spec §5.7 are thin wrappers (`ipc`) over a [`backend::Backend`], which is tested against the
//! core's in-memory adapters; the theme library and drafts (`library`), the app's settings
//! (`settings`) and its log (`logs`) live in the data folder (spec §5.3); the CLI is the same
//! exe (`main.rs`). The UI (`app/ui`) paints what the core computes: no palette, contrast or
//! prompt logic lives in TypeScript (spec §5.6).
pub mod backend;
pub mod command_names;
pub mod console;
pub mod error;
#[cfg(windows)]
pub mod ipc;
pub mod library;
pub mod logs;
#[cfg(windows)]
pub mod machine;
pub mod open;
pub mod settings;

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
pub fn run_gui() {
    use tauri::Manager;
    // The backend over this computer, with the preview's sample identity read from the
    // environment (`machine::real_backend` is the one place that reads it).
    let backend = match machine::real_backend() {
        Ok(backend) => std::sync::Arc::new(backend),
        Err(why) => {
            console::fatal(&start_failed(&why));
            std::process::exit(1);
        }
    };
    let for_window = backend.clone();
    let built = tauri::Builder::default()
        // A second start focuses this window instead of opening another (spec §5.7). The plugin
        // has no JS commands, so the capability file does not change. Registered first, as the
        // plugin asks: a second start hands its arguments over and exits before the window.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .manage(backend)
        .invoke_handler(ipc::handler())
        .setup(move |app| {
            // Said and ended here, not returned: Tauri would turn the error into a panic.
            if let Err(why) = open_main_window(app, for_window.paths().webview()) {
                console::fatal(&window_failed(&why));
                std::process::exit(1);
            }
            Ok(())
        })
        .build(tauri::generate_context!());
    match built {
        Ok(app) => app.run(|_, _| {}),
        Err(e) => {
            console::fatal(&start_failed(&e.to_string()));
            std::process::exit(1);
        }
    }
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
}

#[cfg(not(windows))]
pub fn run_gui() {
    console::fatal("CMD Studio runs on Windows only.");
}
