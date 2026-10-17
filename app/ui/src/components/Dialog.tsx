// A modal that keeps focus inside, closes on Escape, and names itself for screen readers.
// `onClose` is read through a ref, so the focus effect runs once, when the dialog opens: a
// parent that re-renders on each key (with a new inline `onClose`) must not move focus back
// to the dialog box, which would drop every key after the first.
import { useEffect, useId, useLayoutEffect, useRef, type ReactNode } from "react";
import { useT } from "../i18n";

// `summary` too: the plan's "Steps" and every "Show details" toggle are reached by Tab.
const FOCUSABLE =
  'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), summary, [tabindex]:not([tabindex="-1"])';

export function Dialog({
  title,
  onClose,
  children,
}: {
  title: string;
  /** Escape and the Close button call it; without it the dialog cannot be closed. */
  onClose?: () => void;
  children: ReactNode;
}) {
  const t = useT();
  const titleId = useId();
  const box = useRef<HTMLDivElement>(null);
  const close = useRef(onClose);
  useLayoutEffect(() => {
    close.current = onClose;
  });

  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null;
    box.current?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && close.current) close.current();
      if (e.key === "Tab" && box.current) {
        const focusable = box.current.querySelectorAll<HTMLElement>(FOCUSABLE);
        const first = focusable[0];
        const last = focusable[focusable.length - 1];
        if (!first || !last) {
          e.preventDefault();
          return;
        }
        const active = document.activeElement;
        // Focus outside the box goes back in; on the box itself (as the dialog opens) Tab
        // goes on to the first control by itself, and Shift+Tab wraps to the last.
        const outside = !box.current.contains(active);
        if (e.shiftKey && (outside || active === box.current || active === first)) {
          e.preventDefault();
          last.focus();
        } else if (!e.shiftKey && (outside || active === last)) {
          e.preventDefault();
          first.focus();
        }
      }
    };
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("keydown", onKey);
      previous?.focus?.();
    };
  }, []);

  return (
    <div className="dialog-backdrop">
      <div className="dialog" role="dialog" aria-modal="true" aria-labelledby={titleId} ref={box} tabIndex={-1}>
        <h2 id={titleId} style={{ marginTop: 0 }}>
          {title}
        </h2>
        {children}
        {onClose && (
          <div className="row-actions">
            <button type="button" onClick={onClose}>
              {t("a11y.closeDialog")}
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
