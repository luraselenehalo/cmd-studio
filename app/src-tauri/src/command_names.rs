// The IPC commands the window may call: spec §5.7's list, plus `preview_render`,
// `open_external` and `advanced_read` (Phase 2b; spec §5.7 records them). One list, read by
// `build.rs`, which turns each into the permissions `allow-<command>` and `deny-<command>`
// (underscores become hyphens), and by `ipc.rs`, whose test checks that every name here is
// registered with Tauri and allowed by `capabilities/default.json`, and nothing else is.
// (Plain `//` lines, not `//!`: `build.rs` `include!`s this file, and rustc refuses an inner
// doc comment in an included file.)

/// The 27 IPC command names, in the order of spec §5.7.
pub const COMMANDS: &[&str] = &[
    "system_detect",
    "themes_list",
    "theme_get",
    "theme_save",
    "theme_duplicate",
    "theme_delete",
    "theme_import_preview",
    "theme_import_confirm",
    "theme_export",
    "palette_generate",
    "contrast_check",
    "contrast_fix",
    "preview_render",
    "plan_apply",
    "apply",
    "undo_last",
    "snapshots_list",
    "snapshot_restore",
    "revert_all",
    "doctor_run",
    "doctor_fix",
    "settings_get",
    "settings_set",
    "open_cmd",
    "open_external",
    "diagnostics_copy",
    "advanced_read",
];
