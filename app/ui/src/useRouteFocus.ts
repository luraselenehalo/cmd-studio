// A route change puts focus on the new screen's heading (spec §4.5), so a keyboard or
// screen-reader user lands on its title. Every shell uses this hook: this plan's interim one and
// phase 3's final one, which imports it rather than copying it. A change that stays on the same
// screen moves nothing: Help's tabs are routes of one screen, and a tab chosen keeps focus on its
// button (ruling Q4), as the editor's mode buttons do.
import { useEffect, useRef, type RefObject } from "react";
import type { Route } from "./router";

/** The screen a route shows: Help is one screen whichever tab it shows; each theme's editor is
 * a screen of its own. */
export function screenOf(route: Route): string {
  return route.name === "editor" ? `editor:${route.id}` : route.name;
}

/**
 * Focuses the first `h1` inside `main` each time the screen changes. A screen that is still
 * loading gets it once its heading appears; an open dialog keeps the focus it took. The first
 * screen leaves focus at the top of the page.
 */
export function useRouteFocus(route: Route, main: RefObject<HTMLElement | null>): void {
  const started = useRef(false);
  const screen = screenOf(route);
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
  }, [screen, main]);
}
