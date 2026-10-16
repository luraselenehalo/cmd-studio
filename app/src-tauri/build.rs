//! Tauri's build step, plus two things of CMD Studio's own. The app's commands become ACL
//! permissions (`allow-<command>`), so that `capabilities/default.json` is the one list of what
//! the window may call (spec §5.10): a command registered in `ipc.rs` but missing here, or the
//! other way round, fails the build or a test. And a placeholder page stands in for the UI when
//! it is not built yet: `frontendDist` is embedded into every build (`tauri.conf.json` has no
//! `devUrl`) and `tauri::generate_context!` refuses a missing directory, so the page guards every
//! cargo build; `cargo test --workspace` on a fresh clone, and CI's Rust jobs, need no Node.
include!("src/command_names.rs");

fn main() {
    let dist = std::path::Path::new("../ui/dist");
    if !dist.join("index.html").exists() {
        let made = std::fs::create_dir_all(dist).and_then(|()| {
            std::fs::write(
                dist.join("index.html"),
                "<!doctype html><meta charset=\"utf-8\"><title>CMD Studio</title>\
                 <p>The UI is not built. Run <code>npm run build</code> in <code>app/</code>.",
            )
        });
        if let Err(e) = made {
            println!("cargo:warning=could not write the placeholder UI page: {e}");
        }
    }
    let attributes = tauri_build::Attributes::new()
        .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS));
    if let Err(e) = tauri_build::try_build(attributes) {
        println!("cargo:warning=tauri-build failed: {e:#}");
        std::process::exit(1);
    }
}
