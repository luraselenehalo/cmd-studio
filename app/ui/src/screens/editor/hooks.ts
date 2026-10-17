// Small hooks of the editor screen.
import { useEffect, useState } from "react";

/** `value`, once it has stayed the same for `ms`: the preview and the draft follow a settled edit. */
export function useDebounced<T>(value: T, ms: number): T {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    const handle = setTimeout(() => setDebounced(value), ms);
    return () => clearTimeout(handle);
  }, [value, ms]);
  return debounced;
}
