import { createElement, type ReactNode } from "react";
import { renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { en } from "./en";
import { th } from "./th";
import { fill, formatTime, I18nProvider, useT } from "./index";
import type { Dictionary, Key } from "./types";

const THAI = /[฀-๿]/;

// Names that are the same in both languages: products, the signing line, codes.
const SAME_IN_BOTH = new Set<string>([
  "app.name",
  "language.en",
  "language.th",
  "home.update.placeholder",
  "home.restore.latest",
  "editor.prompt.layout.powerline",
  "help.credits.signpath",
  "advanced.version.app",
  "advanced.version.ohMyPosh",
  "advanced.version.clink",
  "advanced.version.nerdFonts",
]);

describe("the two dictionaries", () => {
  it("have exactly the same keys", () => {
    expect(Object.keys(th).sort()).toEqual(Object.keys(en).sort());
  });

  it("list as the same in both only keys that exist", () => {
    for (const key of SAME_IN_BOTH) {
      expect(Object.hasOwn(en, key), `${key} is not a key of the English dictionary`).toBe(true);
    }
  });

  it("have no empty text except the reserved update notice", () => {
    for (const [key, text] of Object.entries(en)) {
      if (key === "home.update.placeholder") continue;
      expect(text.trim(), key).not.toBe("");
      expect(th[key as keyof typeof th].trim(), key).not.toBe("");
    }
  });

  it("are English on one side and Thai on the other", () => {
    for (const [key, text] of Object.entries(en)) {
      expect(THAI.test(text), `${key} holds Thai in the English dictionary`).toBe(
        key === "language.th",
      );
      const thai = th[key as keyof typeof th];
      if (SAME_IN_BOTH.has(key)) {
        expect(thai, key).toBe(text);
      } else {
        expect(THAI.test(thai), `${key} has no Thai in the Thai dictionary`).toBe(true);
      }
    }
  });

  it("use the same placeholders in both languages", () => {
    const holes = (text: string) => (text.match(/\{\w+\}/g) ?? []).sort();
    for (const [key, text] of Object.entries(en)) {
      expect(holes(th[key as keyof typeof th]), key).toEqual(holes(text));
    }
  });

  it("never say สำรอง: the spec's word for a backup is จุดย้อนกลับ (§4.5)", () => {
    const offenders = Object.entries(th)
      .filter(([, text]) => text.includes("สำรอง"))
      .map(([key]) => key);
    expect(offenders).toEqual([]);
  });

  // Spot checks of buttons, not a proof for every text: these buttons start with a verb.
  it("have these buttons start with a verb, as spec §4.5 asks", () => {
    expect(en["apply.start"]).toBe("Apply");
    expect(en["editor.saveApply"]).toBe("Save and apply");
    expect(th["editor.saveApply"]).toBe("บันทึกและใช้ธีม");
    expect(en["home.active.openCmd"]).toBe("Open cmd");
    expect(en["themes.new"]).toBe("Create a theme");
    expect(en["common.details"]).toBe("Show details");
    expect(en["home.restore.all"]).toBe("See all restore points");
    expect(en["wizard.where.setDefault"]).toBe("Make Windows Terminal open cmd by default");
  });
});

describe("fill and formatTime", () => {
  it("fills holes and leaves unknown ones as written", () => {
    expect(fill("Step {n} of 6", { n: 2 })).toBe("Step 2 of 6");
    expect(fill("You can use {user} and {cwd}.", {})).toBe("You can use {user} and {cwd}.");
    expect(fill("plain")).toBe("plain");
  });

  it("does not fill a hole from a property every object inherits", () => {
    expect(fill("{constructor} {toString}", {})).toBe("{constructor} {toString}");
  });

  it("formats a time in the language", () => {
    const ms = Date.UTC(2026, 9, 9, 9, 47);
    expect(formatTime(ms, "en")).toMatch(/2026/);
    expect(formatTime(ms, "th")).toMatch(/2569/);
  });
});

describe("t", () => {
  const wrapper = ({ children }: { children: ReactNode }) =>
    createElement(I18nProvider, { language: "th", setLanguage: () => {}, children });

  it("falls back to English, then to the key, and never throws", () => {
    const { result } = renderHook(() => useT(), { wrapper });
    expect(result.current("editor.save")).toBe("บันทึก");

    // A key missing in the Thai dictionary at run time (a stale build): the English text.
    const loose = th as Partial<Dictionary>;
    const saved = th["editor.save"];
    delete loose["editor.save"];
    try {
      expect(result.current("editor.save")).toBe(en["editor.save"]);
    } finally {
      loose["editor.save"] = saved;
    }

    // A key in neither dictionary: the key itself.
    expect(result.current("no.such.key" as Key)).toBe("no.such.key");
  });
});
