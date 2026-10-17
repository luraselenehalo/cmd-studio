// Mounts the app once the IPC is picked (main.tsx calls this; it is a file of its own so the
// tests can hand it an IPC that fails). If the IPC cannot be picked (its module does not load, or
// `pickIpc` rejects), there is no app to mount and no boundary to catch anything: the window shows
// the E_INTERNAL notice in English and Thai with Reload, never a blank window.
import { StrictMode } from "react";
import { createRoot, type Root } from "react-dom/client";
import { App } from "./App";
import { CrashNotice, ErrorBoundary } from "./components/ErrorBoundary";
import { IpcProvider } from "./ipc/context";
import type { Ipc } from "./ipc/types";

/** Never rejects. `reload` is for tests. */
export async function start(container: HTMLElement, pickIpc: () => Promise<Ipc>, reload?: () => void): Promise<Root> {
  const root = createRoot(container);
  let ipc: Ipc;
  try {
    ipc = await pickIpc();
  } catch (error) {
    console.error("CMD Studio: the app could not start", error);
    root.render(
      <StrictMode>
        <CrashNotice error={error} reload={reload} />
      </StrictMode>,
    );
    return root;
  }
  root.render(
    <StrictMode>
      <ErrorBoundary>
        <IpcProvider ipc={ipc}>
          <App />
        </IpcProvider>
      </ErrorBoundary>
    </StrictMode>,
  );
  return root;
}
