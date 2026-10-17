// The real IPC: Tauri's `invoke` for each command of spec §5.7, and `listen` for the
// `apply-progress` event while `apply` runs. Command names are app/src-tauri/src/command_names.rs;
// argument names are the Rust parameters of ipc.rs in camelCase, as Tauri expects them.
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { ApplyProgress } from "../bindings/ApplyProgress";
import { toIpcError, type Ipc } from "./types";

export const APPLY_PROGRESS_EVENT = "apply-progress";

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (thrown) {
    throw toIpcError(thrown);
  }
}

export const tauriIpc: Ipc = {
  systemDetect: () => call("system_detect"),
  themesList: () => call("themes_list"),
  themeGet: (id) => call("theme_get", { id }),
  themeSave: (theme, draft) => call("theme_save", { theme, draft }),
  themeDuplicate: (id, name) => call("theme_duplicate", { id, name }),
  themeDelete: (id, draftOnly) => call("theme_delete", { id, draftOnly }),
  themeImportPreview: () => call("theme_import_preview"),
  themeImportConfirm: (token) => call("theme_import_confirm", { token }),
  themeExport: (id) => call("theme_export", { id }),
  paletteGenerate: (tone, accent, tint) => call("palette_generate", { tone, accent, tint }),
  contrastCheck: (colors) => call("contrast_check", { colors }),
  contrastFix: (colors) => call("contrast_fix", { colors }),
  previewRender: (theme, context) => call("preview_render", { theme, context }),
  planApply: (request) => call("plan_apply", { request }),
  apply: async (request, digest, onProgress) => {
    const unlisten = await listen<ApplyProgress>(APPLY_PROGRESS_EVENT, (event) => {
      onProgress(event.payload);
    });
    try {
      return await call("apply", { request, digest });
    } finally {
      unlisten();
    }
  },
  undoLast: () => call("undo_last"),
  snapshotsList: () => call("snapshots_list"),
  snapshotRestore: (id) => call("snapshot_restore", { id }),
  revertAll: () => call("revert_all"),
  doctorRun: () => call("doctor_run"),
  doctorFix: (checkId) => call("doctor_fix", { checkId }),
  settingsGet: () => call("settings_get"),
  settingsSet: (settings) => call("settings_set", { settings }),
  openCmd: () => call("open_cmd"),
  openExternal: (target) => call("open_external", { target }),
  diagnosticsCopy: () => call("diagnostics_copy"),
  advancedRead: () => call("advanced_read"),
};
