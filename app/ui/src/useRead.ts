// One read of a page, done again whenever `version` changes (moved out of Home so the Recovery
// page reads the same way).
import { useEffect, useState } from "react";
import { useIpc } from "./ipc/context";
import type { Ipc } from "./ipc/types";

/** One read of the page, done again whenever `version` changes: its answer or what failed. An
 * answer that comes after a newer read started is dropped. `read` must keep one identity across
 * renders (a module-level function). */
export function useRead<T>(read: (ipc: Ipc) => Promise<T>, version: number): { value: T | null; error: unknown } {
  const ipc = useIpc();
  const [state, setState] = useState<{ value: T | null; error: unknown }>({ value: null, error: null });
  useEffect(() => {
    let alive = true;
    read(ipc).then(
      (value) => {
        if (alive) setState({ value, error: null });
      },
      (error: unknown) => {
        if (alive) setState({ value: null, error });
      },
    );
    return () => {
      alive = false;
    };
  }, [ipc, read, version]);
  return state;
}
