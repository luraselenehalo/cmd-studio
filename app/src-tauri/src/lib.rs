//! CMD Studio's desktop app (Tauri 2): a window over `cmdstudio-core`. The IPC commands of
//! spec §5.7 are thin wrappers (`ipc`) over a [`backend::Backend`], which is tested against the
//! core's in-memory adapters; the theme library and drafts (`library`), the app's settings
//! (`settings`) and its log (`logs`) live in the data folder (spec §5.3); the CLI is the same
//! exe (`main.rs`). The UI (`app/ui`) paints what the core computes: no palette, contrast or
//! prompt logic lives in TypeScript (spec §5.6).
pub mod command_names;
pub mod console;
