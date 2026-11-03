# Phase 3b-2 spike — WebView2 ProcessFailed through Tauri 2.12.4, and the process stamp (recovery spec §0, §6.4)

- **Date / commit:** 2026-11-03 / 5b4e285 (the scratch worktree's base).
- **S1 and S3 ran on the dev PC in a scratch worktree** (`cargo check` and two unit tests; the app was never started). **S2 and S4 run on VM-A** against CI's artifact of this branch once Task 6 is in (the controller; `spikes/vm/guest/recovery-renderer.ps1`, Task 12 step 2b).

| # | What | Plan 3b-2 assumes | Result | Holds? | Fallback taken |
|---|---|---|---|---|---|
| S1 | `with_webview` → `controller().CoreWebView2()` → `add_ProcessFailed(ProcessFailedEventHandler)`, `Reload()` from the handler, with webview2-com 0.39.1 as a direct dependency | compiles | `cargo check -p cmdstudio-app --lib`: `Finished` with no error, no change to the brief's text; `cargo test -p cmdstudio-app --lib webview_watch`: `test result: ok. 1 passed`; `cargo clippy -p cmdstudio-app --all-targets -- -D warnings` and `cargo fmt --check` clean | ✅ | — |
| S2 | `Stop-Process` of the renderer fires the handler and `Reload()` brings the page back | the page is back within 5 s; `app_status` asked; Home drawn | pending (VM-A, after Task 6) | pending | pending |
| S3 | `GetProcessTimes` of this process; `OpenProcess` of a pid that is not there | a time within 60 s; `ERROR_INVALID_PARAMETER` → `Ok(None)` | `cargo test -p cmdstudio-core --lib process_stamp`: `test result: ok. 1 passed` (this process's creation time within 60 s; pid 4294967294 is `None`; the FILETIME of 1970 is 0); clippy clean for the core | ✅ | — |
| S4 | "Start normally" restarts the app: the child (`--after-pid <old pid>`) waits for the old process, then one new process with Home | one process, Home within 15 s, `run end` before `run start` | pending (VM-A, after Task 6) | pending | pending |
