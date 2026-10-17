// The real IPC: Tauri's `invoke` for each command of spec §5.7, and `listen` for the
// `apply-progress` event while `apply` runs. Command names are app/src-tauri/src/command_names.rs;
// argument names are the Rust parameters of ipc.rs in camelCase, as Tauri expects them.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
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

/**
 * Ends the `apply-progress` listener without letting it change what `apply` answers.
 * `UnlistenFn` is typed `() => void`, but in @tauri-apps/api 2.12.4 it is async (it invokes
 * `plugin:event|unlisten`), so a failure is a rejection, which is dropped here. It would
 * otherwise go unhandled. Nothing is left to do about it: the listener goes with the window.
 */
function stopListening(unlisten: UnlistenFn): void {
  try {
    void Promise.resolve(unlisten()).catch(() => {});
  } catch {
    // A synchronous failure, which 2.12.4 does not have; dropped for the same reason.
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
    let unlisten: UnlistenFn;
    try {
      unlisten = await listen<ApplyProgress>(APPLY_PROGRESS_EVENT, (event) => {
        onProgress(event.payload);
      });
    } catch (thrown) {
      // Nothing was applied: the listener comes first.
      throw toIpcError(thrown);
    }
    try {
      return await call("apply", { request, digest });
    } finally {
      stopListening(unlisten);
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
