// The preview's sample (ruling R1): who and where come from this computer (`system_detect`'s
// `previewSample`), the folder is under its home without a doubled separator, and the rest is
// the core's fixed sample. No production module reaches the mock or the preset files, so they
// stay out of the shipped bundle (the build's own check greps dist/ for them as well).
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";
import { describe, expect, it } from "vitest";
import { sampleContext, sampleFolder } from "./sample";

describe("the preview's sample", () => {
  it("takes the user, the computer and home from this computer and works under home", () => {
    expect(sampleContext({ user: "tester", host: "TESTPC", home: "C:\\Users\\tester" })).toEqual({
      user: "tester",
      host: "TESTPC",
      home: "C:\\Users\\tester",
      cwd: "C:\\Users\\tester\\projects\\cmd-studio",
      clock: { year: 2026, month: 10, day: 9, hour: 16, minute: 47 },
      git: { branch: "main", dirty: true },
      exitCode: 1,
      durationMs: 3500,
      width: 80,
      windowsTerminal: true,
      command: "npm test",
    });
  });

  it("does not double the separator of a home at the root of a drive", () => {
    expect(sampleFolder("H:\\")).toBe("H:\\projects\\cmd-studio");
    expect(sampleFolder("d:/")).toBe("d:/projects\\cmd-studio");
    expect(sampleFolder("H:")).toBe("H:\\projects\\cmd-studio");
    expect(sampleContext({ user: "tester", host: "TESTPC", home: "H:\\" }).cwd).toBe("H:\\projects\\cmd-studio");
  });
});

describe("the production modules", () => {
  it("import neither the mock nor the preset files", () => {
    // This file's folder, `ui/src` (jsdom's own `URL` is not one `fileURLToPath` takes).
    const src = import.meta.dirname;
    const files: string[] = [];
    const walk = (dir: string) => {
      for (const name of readdirSync(dir)) {
        const path = join(dir, name);
        if (statSync(path).isDirectory()) walk(path);
        else if (/\.tsx?$/.test(name)) files.push(path);
      }
    };
    walk(src);
    const production = files.filter((path) => {
      const rel = relative(src, path).replace(/\\/g, "/");
      return !/\.test\.tsx?$/.test(rel) && !rel.startsWith("test/") && rel !== "ipc/mock.ts" && rel !== "ipc/presets.ts";
    });
    expect(production.length).toBeGreaterThan(10);
    // A static import of either (main.tsx reaches the mock only through a dynamic import behind
    // VITE_MOCK_IPC, which the build drops).
    const offenders = production.filter((path) =>
      /^\s*import[^;]*from\s+["'][^"']*\bipc\/(mock|presets)["']/m.test(readFileSync(path, "utf8")) ||
      /^\s*import[^;]*from\s+["']\.\/(mock|presets)["']/m.test(readFileSync(path, "utf8")),
    );
    expect(offenders.map((path) => relative(src, path))).toEqual([]);
  });
});
