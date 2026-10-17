// Mounts the app. In the Tauri window the IPC is Tauri's; `VITE_MOCK_IPC=1` (the visual check in
// a plain browser, never in the shipped app) swaps in the in-memory mock through a dynamic
// import behind a build-time constant, so the production build drops that branch and the mock
// and the preset files stay out of the bundle (checked by grepping ui/dist after `vite build`).
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { IpcProvider } from "./ipc/context";
import type { Ipc } from "./ipc/types";
import "./styles.css";

async function pickIpc(): Promise<Ipc> {
  if (import.meta.env.VITE_MOCK_IPC === "1") {
    const { createMockIpc } = await import("./ipc/mock");
    return createMockIpc();
  }
  const { tauriIpc } = await import("./ipc/tauri");
  return tauriIpc;
}

const root = document.getElementById("root");
if (root) {
  void pickIpc().then((ipc) => {
    createRoot(root).render(
      <StrictMode>
        <IpcProvider ipc={ipc}>
          <App />
        </IpcProvider>
      </StrictMode>,
    );
  });
}
