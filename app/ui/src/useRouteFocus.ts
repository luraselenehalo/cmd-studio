// A route change puts focus on the new screen's heading (spec §4.5), so a keyboard or
// screen-reader user lands on its title. Every shell uses this hook: this plan's interim one and
// phase 3's final one, which imports it rather than copying it.
import { useEffect, useRef, type RefObject } from "react";
import type { Route } from "./router";

/**
 * Focuses the first `h1` inside `main` each time `route` changes. A screen that is still loading
 * gets it once its heading appears; an open dialog keeps the focus it took. The first screen
 * leaves focus at the top of the page.
 */
export function useRouteFocus(route: Route, main: RefObject<HTMLElement | null>): void {
  const started = useRef(false);
  useEffect(() => {
    if (!started.current) {
      started.current = true;
      return;
    }
    const box = main.current;
    if (!box) return;
    const focusHeading = (): boolean => {
      if (box.querySelector('[role="dialog"]')) return true;
      const heading = box.querySelector<HTMLElement>("h1");
      if (!heading) return false;
      heading.focus();
      return true;
    };
    if (focusHeading()) return;
    const observer = new MutationObserver(() => {
      if (focusHeading()) observer.disconnect();
    });
    observer.observe(box, { childList: true, subtree: true });
    return () => observer.disconnect();
  }, [route, main]);
}
