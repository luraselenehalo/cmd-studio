// A tiny in-house router (the brief: no routing library): the route is React state in `App`,
// screens read it and move with `useNav()`. On every change of screen the main heading takes
// focus, so a keyboard or screen-reader user lands on the new screen's title (spec §4.5); a Help
// tab is a route of the same screen and leaves focus on its button (useRouteFocus.ts).
import { createContext, useContext } from "react";

export type HelpTab = "doctor" | "restore" | "revert" | "report" | "guide" | "credits";

export type Route =
  | { name: "wizard" }
  | { name: "home" }
  | { name: "themes" }
  | { name: "editor"; id: string }
  | { name: "settings" }
  | { name: "help"; tab: HelpTab }
  | { name: "advanced" };

export interface Nav {
  route: Route;
  go: (route: Route) => void;
}

export const NavContext = createContext<Nav | null>(null);

export function useNav(): Nav {
  const nav = useContext(NavContext);
  if (!nav) throw new Error("useNav needs a NavContext");
  return nav;
}
