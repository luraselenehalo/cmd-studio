//! Guards over `tauri.conf.json` (tests only). No cargo build enables `tauri/custom-protocol`, so
//! every exe is a "dev build" to tauri-codegen, which loads `build.devUrl` instead of the embedded
//! assets when the key is set: whatever served that port would then hold the IPC of all 27
//! commands. The config must not have it, must carry the CSP of the Global Constraints and
//! nothing else under `app.security`, and must not create the window at startup (Task 15 builds
//! it with a data directory).
use serde_json::Value;

const CONFIG: &str = include_str!("../tauri.conf.json");

/// The CSP of the Global Constraints (spec §5.10): no remote origin.
const CSP: &str = "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self'; \
                   font-src 'self'; connect-src ipc: http://ipc.localhost; object-src 'none'; \
                   base-uri 'self'; frame-ancestors 'none'";

#[test]
fn the_config_serves_only_the_embedded_ui_under_the_csp_and_creates_no_window() {
    let config: Value = serde_json::from_str(CONFIG).unwrap();

    // (a) No dev server: the window may not load http://localhost:<port> with the IPC.
    let build = config.get("build").and_then(Value::as_object).unwrap();
    assert!(!build.contains_key("devUrl"), "build.devUrl is set");

    // (b) The CSP, exactly, and no other security key (no devCsp, no dangerous* switch).
    let security = config
        .pointer("/app/security")
        .and_then(Value::as_object)
        .unwrap();
    assert_eq!(
        security.keys().map(String::as_str).collect::<Vec<_>>(),
        ["csp"]
    );
    assert_eq!(security.get("csp").and_then(Value::as_str), Some(CSP));

    // (c) One window, not created at startup.
    let windows = config
        .pointer("/app/windows")
        .and_then(Value::as_array)
        .unwrap();
    assert_eq!(windows.len(), 1, "one window");
    assert_eq!(windows[0].get("create"), Some(&Value::Bool(false)));
}
