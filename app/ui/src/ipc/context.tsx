import { createContext, useContext, type ReactNode } from "react";
import type { Ipc } from "./types";

const IpcContext = createContext<Ipc | null>(null);

export function IpcProvider({ ipc, children }: { ipc: Ipc; children: ReactNode }) {
  return <IpcContext.Provider value={ipc}>{children}</IpcContext.Provider>;
}

/** The IPC of the app: Tauri's in the window, the mock in tests and the visual check. */
export function useIpc(): Ipc {
  const ipc = useContext(IpcContext);
  if (!ipc) throw new Error("useIpc needs an IpcProvider");
  return ipc;
}
