// The IPC commands the window may call: spec §5.7's list, plus `preview_render`,
// `open_external` and `advanced_read` (Phase 2b; spec §5.7 records them). One list, read by
// `build.rs`, which turns each into the permissions `allow-<command>` and `deny-<command>`
// (underscores become hyphens), and by `ipc.rs`, whose test checks that every name here is
// registered with Tauri and allowed by `capabilities/default.json`, and nothing else is.
// Each name is also a constant of its own, which `ipc.rs` hands to `Backend::logged`: the log
// line of a command names it with this text and no other (a test in `ipc.rs` checks that each
// command logs under its own constant).
// (Plain `//` lines, not `//!`: `build.rs` `include!`s this file, and rustc refuses an inner
// doc comment in an included file.)

pub const SYSTEM_DETECT: &str = "system_detect";
pub const THEMES_LIST: &str = "themes_list";
pub const THEME_GET: &str = "theme_get";
pub const THEME_SAVE: &str = "theme_save";
pub const THEME_DUPLICATE: &str = "theme_duplicate";
pub const THEME_DELETE: &str = "theme_delete";
pub const THEME_IMPORT_PREVIEW: &str = "theme_import_preview";
pub const THEME_IMPORT_CONFIRM: &str = "theme_import_confirm";
pub const THEME_EXPORT: &str = "theme_export";
pub const PALETTE_GENERATE: &str = "palette_generate";
pub const CONTRAST_CHECK: &str = "contrast_check";
pub const CONTRAST_FIX: &str = "contrast_fix";
pub const PREVIEW_RENDER: &str = "preview_render";
pub const PLAN_APPLY: &str = "plan_apply";
pub const APPLY: &str = "apply";
pub const UNDO_LAST: &str = "undo_last";
pub const SNAPSHOTS_LIST: &str = "snapshots_list";
pub const SNAPSHOT_RESTORE: &str = "snapshot_restore";
pub const REVERT_ALL: &str = "revert_all";
pub const DOCTOR_RUN: &str = "doctor_run";
pub const DOCTOR_FIX: &str = "doctor_fix";
pub const SETTINGS_GET: &str = "settings_get";
pub const SETTINGS_SET: &str = "settings_set";
pub const OPEN_CMD: &str = "open_cmd";
pub const OPEN_EXTERNAL: &str = "open_external";
pub const DIAGNOSTICS_COPY: &str = "diagnostics_copy";
pub const ADVANCED_READ: &str = "advanced_read";

/// The 27 IPC command names, in the order of spec §5.7.
pub const COMMANDS: &[&str] = &[
    SYSTEM_DETECT,
    THEMES_LIST,
    THEME_GET,
    THEME_SAVE,
    THEME_DUPLICATE,
    THEME_DELETE,
    THEME_IMPORT_PREVIEW,
    THEME_IMPORT_CONFIRM,
    THEME_EXPORT,
    PALETTE_GENERATE,
    CONTRAST_CHECK,
    CONTRAST_FIX,
    PREVIEW_RENDER,
    PLAN_APPLY,
    APPLY,
    UNDO_LAST,
    SNAPSHOTS_LIST,
    SNAPSHOT_RESTORE,
    REVERT_ALL,
    DOCTOR_RUN,
    DOCTOR_FIX,
    SETTINGS_GET,
    SETTINGS_SET,
    OPEN_CMD,
    OPEN_EXTERNAL,
    DIAGNOSTICS_COPY,
    ADVANCED_READ,
];
