// Mounts the app once the IPC is picked (main.tsx calls this; it is a file of its own so the
// tests can hand it an IPC that fails). The IPC goes through `withWork` (recovery spec §6: the
// "still working" bar, the read-only deadline, the restart offer), and the window's own errors
// (`error`, `unhandledrejection`, §6.5) show the same E_INTERNAL notice the boundary shows and go
// to the log through `log_ui_error`, twenty per load. If the IPC cannot be picked (its module does
// not load, or `pickIpc` rejects), there is no app to mount and no boundary to catch anything: the
// window shows the E_INTERNAL notice in English and Thai with Reload, never a blank window.
import { StrictMode, useEffect, useState, type ReactNode } from "react";
import { createRoot, type Root } from "react-dom/client";
import { App } from "./App";
import { CrashNotice, ErrorBoundary } from "./components/ErrorBoundary";
import { WorkContext } from "./components/WorkingBar";
import { IpcProvider } from "./ipc/context";
import type { Ipc } from "./ipc/types";
import { withWork } from "./ipc/work";

/** How many of the window's errors are sent to the log per load (the backend keeps its own
 * count per run, recovery spec §6.5). */
export const UI_ERRORS_SENT_PER_LOAD = 20;

/** What was thrown, as one line for the log: its name and message, or its text. */
function describe(error: unknown): string {
  if (error instanceof Error) return `${error.name}: ${error.message}`;
  if (typeof error === "string") return error;
  try {
    return JSON.stringify(error) ?? String(error);
  } catch {
    return String(error);
  }
}

/** Listens for the window's errors and promises nobody handled; `onError` gets each, and the
 * first twenty go to the log. Returns the function that stops listening. */
export function watchWindowErrors(ipc: Ipc, onError: (error: unknown) => void): () => void {
  let sent = 0;
  const report = (error: unknown, logOnly: boolean) => {
    if (!logOnly) onError(error);
    if (sent < UI_ERRORS_SENT_PER_LOAD) {
      sent += 1;
      ipc.logUiError(describe(error), "window").catch(() => {});
    }
  };
  // A window `error` with no Error object (a resource that did not load, the browser's
  // "ResizeObserver loop" notices) is logged but never blanks the app: nothing of ours threw.
  const onErrorEvent = (event: ErrorEvent) => {
    const benign = event.error == null || /ResizeObserver loop/.test(event.message);
    report(event.error ?? event.message, benign);
  };
  const onRejection = (event: Event) => report((event as Event & { reason?: unknown }).reason, false);
  window.addEventListener("error", onErrorEvent);
  window.addEventListener("unhandledrejection", onRejection);
  return () => {
    window.removeEventListener("error", onErrorEvent);
    window.removeEventListener("unhandledrejection", onRejection);
  };
}

/** The catch-all around the app: the first error the window reports replaces the app with the
 * notice (a render that throws is the boundary's, below this). */
function Catch({ ipc, reload, children }: { ipc: Ipc; reload?: () => void; children: ReactNode }) {
  const [crashed, setCrashed] = useState<{ error: unknown } | null>(null);
  useEffect(() => watchWindowErrors(ipc, (error) => setCrashed((c) => c ?? { error })), [ipc]);
  if (crashed) return <CrashNotice error={crashed.error} reload={reload} />;
  return children;
}

/** Resolves with the root once the app, or the notice of why it could not start, is given to it
 * to render. A failure to pick the IPC is shown, never thrown; `createRoot` is outside the try
 * because without a root there is nowhere to show anything. `reload` is for tests. */
export async function start(container: HTMLElement, pickIpc: () => Promise<Ipc>, reload?: () => void): Promise<Root> {
  const root = createRoot(container);
  let picked: Ipc;
  try {
    picked = await pickIpc();
  } catch (error) {
    console.error("CMD Studio: the app could not start", error);
    root.render(
      <StrictMode>
        <CrashNotice error={error} reload={reload} />
      </StrictMode>,
    );
    return root;
  }
  const { ipc, work } = withWork(picked);
  root.render(
    <StrictMode>
      <ErrorBoundary reload={reload}>
        <IpcProvider ipc={ipc}>
          <WorkContext.Provider value={work}>
            <Catch ipc={ipc} reload={reload}>
              <App />
            </Catch>
          </WorkContext.Provider>
        </IpcProvider>
      </ErrorBoundary>
    </StrictMode>,
  );
  return root;
}
