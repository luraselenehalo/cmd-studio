// jsdom has no `structuredClone` in some versions, no `matchMedia`, and a clipboard the tests
// replace; the components also use `scrollIntoView`.
import { afterEach } from "vitest";
import { cleanup } from "@testing-library/react";

afterEach(() => {
  cleanup();
});

if (typeof globalThis.structuredClone !== "function") {
  globalThis.structuredClone = <T,>(value: T): T => JSON.parse(JSON.stringify(value)) as T;
}

if (typeof window !== "undefined") {
  if (!window.matchMedia) {
    window.matchMedia = (query: string) =>
      ({
        matches: false,
        media: query,
        onchange: null,
        addEventListener: () => {},
        removeEventListener: () => {},
        addListener: () => {},
        removeListener: () => {},
        dispatchEvent: () => false,
      }) as MediaQueryList;
  }
  if (!Element.prototype.scrollIntoView) {
    Element.prototype.scrollIntoView = () => {};
  }
}
