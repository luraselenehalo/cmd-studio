import { describe, expect, it } from "vitest";
import { en } from "./en";
import { th } from "./th";
import { fill, formatTime } from "./index";

const THAI = /[฀-๿]/;

// Names that are the same in both languages: products, the signing line, codes.
const SAME_IN_BOTH = new Set<string>([
  "app.name",
  "language.en",
  "language.th",
  "wizard.check.windows",
  "home.update.placeholder",
  "home.restore.latest",
  "editor.prompt.title",
  "editor.prompt.layout.powerline",
  "help.credits.signpath",
  "advanced.version.app",
  "advanced.version.ohMyPosh",
  "advanced.version.clink",
  "advanced.version.nerdFonts",
  "advanced.log",
]);

describe("the two dictionaries", () => {
  it("have exactly the same keys", () => {
    expect(Object.keys(th).sort()).toEqual(Object.keys(en).sort());
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

  it("start every button text with a verb, as spec §4.5 asks (spot checks)", () => {
    expect(en["apply.start"]).toBe("Apply");
    expect(en["editor.saveApply"]).toBe("Save and apply");
    expect(th["editor.saveApply"]).toBe("บันทึกและใช้");
    expect(en["home.active.openCmd"]).toBe("Open cmd");
  });
});

describe("fill and formatTime", () => {
  it("fills holes and leaves unknown ones as written", () => {
    expect(fill("Step {n} of 6", { n: 2 })).toBe("Step 2 of 6");
    expect(fill("You can use {user} and {cwd}.", {})).toBe("You can use {user} and {cwd}.");
    expect(fill("plain")).toBe("plain");
  });

  it("formats a time in the language", () => {
    const ms = Date.UTC(2026, 9, 9, 9, 47);
    expect(formatTime(ms, "en")).toMatch(/2026/);
    expect(formatTime(ms, "th")).toMatch(/2569/);
  });
});
