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

/// Opens the window. Everything that can fail before it exists (the data folder, the window
/// itself) ends in a message box and exit code 1; after that, Tauri owns the thread.
#[cfg(windows)]
pub fn run_gui() {
    use tauri::Manager;
    // The backend over this computer, with the preview's sample identity read from the
    // environment (`machine::real_backend` is the one place that reads it).
    let backend = match machine::real_backend() {
        Ok(backend) => std::sync::Arc::new(backend),
        Err(why) => {
            console::fatal(&format!("CMD Studio could not start: {why}"));
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
            // The window is in tauri.conf.json with `create: false`, so that WebView2's data
            // folder can be set here: `<data>\webview`, a folder of CMD Studio's choosing in its
            // data folder, which `--cleanup` knows (spec §5.3). Tauri's own default would be the
            // data folder itself, where WebView2 makes an `EBWebView` folder of its own. This is
            // the only place the window is made.
            let config = app
                .config()
                .app
                .windows
                .iter()
                .find(|w| w.label == "main")
                .cloned()
                .ok_or("tauri.conf.json has no window labelled main")?;
            tauri::WebviewWindowBuilder::from_config(app.handle(), &config)?
                .data_directory(for_window.paths().webview())
                .build()?;
            Ok(())
        })
        .build(tauri::generate_context!());
    match built {
        Ok(app) => app.run(|_, _| {}),
        Err(e) => {
            console::fatal(&format!("CMD Studio could not open its window: {e}"));
            std::process::exit(1);
        }
    }
}

#[cfg(not(windows))]
pub fn run_gui() {
    console::fatal("CMD Studio runs on Windows only.");
}
