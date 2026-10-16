# Phase 2b (2 of 3) — The Themes UI: toolchain, dictionaries, IPC, shared components, the library and the editor

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. This is the second of the three sub-plans indexed by `2026-10-16-phase2b-app.md`; it needs the shell plan done (the bindings in `app/ui/src/bindings`, the app crate, the Rust CI job).

**Goal:** Give the app its UI foundation and the themes screens of spec §3 — the pinned Node toolchain, the English and Thai dictionaries with their completeness test, the `Ipc` interface with Tauri's and the mock's implementations, the shared components (the preview painter, notices, dialog, theme card, the plan→apply→progress→outcome flow), the editor's reducer, the Themes and Editor screens behind an interim shell — tested against the mock in jsdom, looked at in the browser pane, and built by CI's full app job with `tauri build --no-bundle`.

**Architecture:** React 19 function components, TypeScript strict, Vite 8; no router, state or i18n library. `ipc/types.ts` is the one contract (one method per IPC command, camelCase arguments as Tauri expects them, errors as `IpcError` from the Rust `AppError`); `ipc/tauri.ts` is `invoke` + `listen("apply-progress")`; `ipc/mock.ts` is an in-memory stand-in whose palette and preview answers are fixed data from the preset files (spec §5.6: no color or prompt logic in TypeScript), used by every test and by the visual check (`VITE_MOCK_IPC=1`, a dynamic import so it never reaches the production bundle). `i18n/` holds two typed dictionaries (`Key = keyof typeof en`) and a provider; the app opens in English and switches when the saved language is Thai. `components/ApplyFlow.tsx` is the only way a theme reaches the machine from the UI: plan → "What will change" → Apply → progress → outcome, with the other-Clink question and the `planChanged` notice. `screens/editor/reducer.ts` holds the editor's state (100 undo steps, simple/advanced mode, the rule that a hand-edited color makes `simple` null). The interim `App.tsx` of this plan shows Themes and the editor behind a sidebar; the phase 3 plan replaces it with the final shell.

**Tech Stack:** Node 24.21.0 (`.nvmrc`, `engines >=24 <25`); `@tauri-apps/api 2.12.4`, `@tauri-apps/cli 2.12.3`, `react`/`react-dom 19.3.1`, `@types/react`/`@types/react-dom 19.3.0`, `@types/node 24.19.2`, `typescript 5.9.3`, `vite 8.3.4`, `@vitejs/plugin-react 6.1.2`, `vitest 5.0.5`, `jsdom 30.1.3`, `@testing-library/react 16.3.3`, `@testing-library/user-event 14.6.8` — exact in `package.json`, locked by the `package-lock.json` of appendix A (written verbatim, so `npm ci` installs what was tested).

**Spec:** `docs/superpowers/specs/2026-10-10-cmd-studio-design.md` §2.1, §3.1–§3.6, §4.4, §4.5, §5.6, §5.7, §5.9, §5.10; the index plan's Decisions 3, 7, 11, 12, 15.

**Prerequisite:** the shell plan merged into the branch: `app/ui/src/bindings/*.ts` (71 files), `app/src-tauri` complete, `.gitignore` with the UI entries, the CI `app` job's Rust part. The code of every task below was compiled (`tsc --noEmit`) and run (vitest) in the scratch copy: `Themes.test.tsx` 6 passed against this plan's interim `App.tsx`, and the whole set 32 passed against the final one.

## Global Constraints

Every task's requirements include these. Values are copied from the spec, the founder's decisions and the earlier plans' Global Constraints (`docs/superpowers/plans/2026-10-10-phase1a-core.md`, `2026-10-11-phase1b-system.md`, `2026-10-13-phase2a-preview.md`) where they bind here.

- Repository `C:\Users\<you>\cmd-studio` (written `<repo>` below), branch `phase2b-app`, cut from main `061668a`. Rust `1.99.0` (`rust-toolchain.toml`), edition 2021, license MIT. Pinned runtimes: Oh My Posh `31.5.0`, Clink `1.9.34`, Nerd Fonts `3.5.1` (`CaskaydiaCove NF`).
- **Language (founder 2026-10-13 and 2026-10-16; spec §1, §4.5, appendix B).** Nothing CMD Studio writes into a cmd window is Thai: the app writes nothing new into cmd, and `crates/core/tests/english_in_cmd.rs` must keep passing. The app's UI **always opens in English**; Thai is a choice on the wizard's first page and in Settings, and the app remembers it in `settings.json` (`language`, default `en`). Every UI string lives in both dictionaries (`app/ui/src/i18n/en.ts`, `th.ts`); `i18n.test.ts` fails on a missing key, an empty text, Thai in the English dictionary or English-only text in the Thai one.
- **One exe (spec §5.7).** `cmdstudio.exe` is the app; started with any argument it runs `cmdstudio_cli::run` and exits with its code. `crates/cli` is a library only.
- **IPC (spec §5.7, §5.9, §5.10).** The commands are exactly the §5.7 list plus `preview_render`, `open_external` and `advanced_read` (27 names, `app/src-tauri/src/command_names.rs`): thin wrappers over `Backend`, no logic of their own. Every failure crosses the IPC as `{code, messageTh, messageEn, details}` (`AppError`); the codes are the rows of spec §5.9, no other. `capabilities/default.json` allows exactly these commands and the two event permissions; the CSP is `default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self'; font-src 'self'; connect-src ipc: http://ipc.localhost; object-src 'none'; base-uri 'self'; frame-ancestors 'none'` (no remote origin); the app is single-instance (a second start focuses the window).
- **No palette, contrast or prompt logic in TypeScript (spec §5.6).** The UI paints the spans the core returns and shows the colors `palette_generate` / `contrast_fix` return. The test mock (`app/ui/src/ipc/mock.ts`) answers with fixed data taken from the presets, never with a computation of its own.
- **`--cleanup` stays true.** Every entry the app writes under the data folder (`drafts\`, `logs\`, `webview\`, `settings.json`, the set-aside `settings.json.damaged-<ms>` and the temp file `.settings.json.<hex>.cmdstudio-tmp`) is known to `cmdstudio_cli`'s `put_there_by_cmd_studio`, so a folder the app used is still "every entry is CMD Studio's".
- **Production code** (everything outside `#[cfg(test)]` and `tests/`): no `unwrap`, `expect`, `panic!`, unchecked indexing or slicing, string slicing, or `serde_json::Value` IndexMut; the workspace denies clippy `unwrap_used`, `expect_used`, `panic`, `indexing_slicing`, `string_slice` and `undocumented_unsafe_blocks` (ruling R1); the app crate inherits `[lints] workspace = true`. Errors are typed. Every `unsafe` block (the Win32 calls in `console.rs` and `open.rs`) carries a `// SAFETY:` comment.
- **Test safety.** No test touches this computer's AutoRun, fonts, Windows Terminal settings, `%LOCALAPPDATA%\ai.resincore.cmdstudio` or `%ProgramData%`: backend tests run over the core's `FakeMachine` (feature `fake`), UI tests over the in-memory mock in jsdom. The visual check is the Vite dev server with `VITE_MOCK_IPC=1` in the built-in browser pane. Never run the built app or `cmdstudio.exe` on the dev PC; real-app runs happen only in the VMs, by the controller. Never set `CMDSTUDIO_SYSTEM_TESTS`, `CMDSTUDIO_OMP_TESTS` or `CMDSTUDIO_ALLOW_PARITY_SKIP` on the dev PC; `cmd.exe` only with `/d`.
- **Windows Application Control.** This PC intermittently blocks a freshly built unsigned binary (a build script, a test exe) with `os error 4551`. Rerun the identical command once. If it is blocked again, do not work around it (no signing, no policy change, no other toolchain): mark the step "not compiled locally; CI is the gate" in the commit message and go on; CI runs every step of this plan.
- **Pins.** Every direct dependency is exact: `= "x.y.z"` in Cargo manifests, `"x.y.z"` in `package.json`; `Cargo.lock` and `app/package-lock.json` are committed; Node is pinned by `app/.nvmrc` (`24.21.0`) and `engines`. In the repository only `npm ci`, never `npm install`; experiments run in a scratch copy with `npm_config_cache` under the scratchpad.
- **JSON** CMD Studio writes or sends over the IPC uses camelCase (ruling P21): `#[serde(rename_all = "camelCase")]` on every DTO, and ts-rs follows serde. The bindings under `app/ui/src/bindings` are generated by `scripts/export-bindings.ps1` and committed; CI fails when they are stale.
- **Each Rust task ends clean:** `cargo fmt --all`, then `cargo clippy --workspace --all-targets -- -D warnings`, `cargo clippy -p cmdstudio-core --all-targets -- -D warnings`, `cargo clippy -p cmdstudio-core --all-targets --features fake -- -D warnings` and `cargo clippy -p cmdstudio-core --all-targets --features ts -- -D warnings`. Exception, stated where it applies: while the app crate is being built up (shell plan Tasks 8–13) its clippy reports dead code for items whose users come in a later task of that plan — exactly the items each task's Expected line names, nothing else; from Task 14 on the app crate is clean too. **Each UI task ends clean:** `npm run typecheck` and `npm test` in `app/`.
- **The spec** (`docs/superpowers/specs/2026-10-10-cmd-studio-design.md`) changes only in the tasks that name the change, and the working copy on the branch already holds the coordinator's uncommitted edits of 2026-10-16 (§2.1, §4.3, §4.5, §6.1, §6.2, §6.4, §8, appendix B): edit the file as it is, never from `061668a`'s version.
- **Commits:** write the English message to `.git/CMDSTUDIO_COMMIT_MSG` with the Write tool, then `git commit -F .git/CMDSTUDIO_COMMIT_MSG`. Stage only the task's files with `git add -- <files>` and check `git diff --cached --name-only`. Never add `Co-Authored-By`, "Generated with" or any Claude/claude.ai line; never `--no-verify`; never push. Follow the `auroraz-pr-workflow` skill.
- Scratch copies and the check crate use their own `CARGO_TARGET_DIR`, never the repository's `target/`. No user name of this PC appears in a committed file or a plan: tests use `C:\Users\tester\...`, docs use `<you>` or `%USERPROFILE%`.

## Review Focus

The five of the index plan that this plan's code owns, each with the test that pins it here.

1. **Another cmd add-on (Clink) is already set up.** The dialog says so, lists the commands, and goes on only with the user's "Use CMD Studio's instead". Task 6: `asks about another cmd add-on and goes on with the user's yes`.
2. **A failed apply.** "The theme could not be applied. Everything was put back as it was." with the code and "Copy details for a problem report". Task 6: `says a failed apply was put back, with a way to copy the details`.
3. **An edit left unsaved.** The draft is saved a second after the last edit and listed as a draft; the editor offers it when the theme opens again. Task 6: `makes a new theme in simple mode and keeps a draft`; Task 3: `refuses to save or delete a preset and keeps drafts apart` (the mock's part).
4. **A hand-edited color and simple mode.** Editing any color by hand makes the recipe null; going back to simple mode keeps the edits with a notice until the tone, main color or tint regenerates them. Task 5: `makes simple null as soon as one color is edited by hand`, `keeps the hand edits when going back to simple mode, with a notice, until a regenerate`.
5. **A string missing in one language, or Thai where English must be.** Both dictionaries have the same keys, no empty text, Thai only in the Thai one; the mock answers from fixed data. Task 2: `have exactly the same keys`, `are English on one side and Thai on the other`; Task 3: `lists the twelve presets and applies, undoes and restores like the backend`.

## File map

```
app/package.json, app/.nvmrc, app/package-lock.json (appendix A)            (Task 1)
app/ui/index.html, tsconfig.json, vite.config.ts, vitest.config.ts          (Task 1)
app/ui/src/vite-env.d.ts, styles.css, test/setup.ts                         (Task 1)
app/ui/src/i18n/types.ts, en.ts, th.ts, index.tsx, i18n.test.ts             (Task 2)
app/ui/src/ipc/types.ts, tauri.ts, context.tsx, presets.ts, mock.ts, mock.test.ts   (Task 3)
app/ui/src/router.tsx, state.tsx                                            (Task 4)
app/ui/src/components/TerminalPreview.tsx, Notices.tsx, Dialog.tsx, ThemeCard.tsx, ApplyFlow.tsx   (Task 4)
app/ui/src/screens/editor/reducer.ts, reducer.test.ts                       (Task 5)
app/ui/src/screens/Themes.tsx, Editor.tsx, app/ui/src/App.tsx (interim), main.tsx, Themes.test.tsx   (Task 6)
.github/workflows/ci.yml                                                    job app, full (Task 7)
```

Commands run in `<repo>\app` unless said otherwise. `npm test` is `vitest run`; its totals are cumulative.

---

### Task 1: The Node toolchain, pinned

`package.json` with exact versions, `.nvmrc`, the lockfile written verbatim from appendix A, Vite and vitest configs rooted at `ui/`, a strict `tsconfig`, the page, the stylesheet (Windows 11 look, light and dark from the system, rem sizes, visible focus — spec §4.5), and the test setup. Nothing of the app's own yet; the gate is that `npm ci` installs exactly the locked tree and `tsc` is clean.

**Files:**
- Create: `app/package.json`, `app/.nvmrc`, `app/package-lock.json`, `app/ui/index.html`, `app/ui/tsconfig.json`, `app/ui/vite.config.ts`, `app/ui/vitest.config.ts`, `app/ui/src/vite-env.d.ts`, `app/ui/src/styles.css`, `app/ui/src/test/setup.ts`
- Test: `npm ci`, `npm run typecheck`

**Interfaces:**
- Consumes: Node 24.21.0 installed on the machine (`node --version`); the `.gitignore` entries of the shell plan's Task 1.
- Produces: scripts `dev` (Vite on port 1420, `strictPort`), `build` (`tsc --noEmit` then `vite build` into `ui/dist`), `typecheck`, `test` (vitest, jsdom, `src/**/*.test.{ts,tsx}`, setup file), `tauri`; `import.meta.env.VITE_MOCK_IPC`; `*.cmdtheme?raw` module typing; the CSS classes the components use (`shell`, `sidebar`, `card`, `theme-card`, `swatches`, `terminal`, `dialog`, `dialog-backdrop`, `notice`, `bar`, `tabs`, `row-actions`, `check-list`, `plain-list`, `color-grid`, `editor`, `preview-column`, `wizard`, `wizard-nav`, `badge`, `status-ok|warn|fail`, `visually-hidden`, `skip-link`, `log`).

- [ ] **Step 1: Write `app/package.json`**

```json
{
  "name": "cmd-studio-app",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "engines": {
    "node": ">=24 <25"
  },
  "scripts": {
    "dev": "vite --config ui/vite.config.ts",
    "build": "tsc -p ui/tsconfig.json --noEmit && vite build --config ui/vite.config.ts",
    "typecheck": "tsc -p ui/tsconfig.json --noEmit",
    "test": "vitest run --config ui/vitest.config.ts",
    "tauri": "tauri"
  },
  "dependencies": {
    "@tauri-apps/api": "2.12.4",
    "react": "19.3.1",
    "react-dom": "19.3.1"
  },
  "devDependencies": {
    "@tauri-apps/cli": "2.12.3",
    "@testing-library/react": "16.3.3",
    "@testing-library/user-event": "14.6.8",
    "@types/node": "24.19.2",
    "@types/react": "19.3.0",
    "@types/react-dom": "19.3.0",
    "@vitejs/plugin-react": "6.1.2",
    "jsdom": "30.1.3",
    "typescript": "5.9.3",
    "vite": "8.3.4",
    "vitest": "5.0.5"
  }
}
```

- [ ] **Step 2: Write `app/.nvmrc`**

```text
24.21.0
```

- [ ] **Step 3: Write `app/package-lock.json`** — the whole of appendix A, verbatim (2200 lines; `lockfileVersion: 3`).

- [ ] **Step 4: Write `app/ui/index.html`**

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>CMD Studio</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
```

- [ ] **Step 5: Write `app/ui/tsconfig.json`**

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "lib": ["ES2022", "DOM", "DOM.Iterable"],
    "module": "ESNext",
    "moduleResolution": "bundler",
    "jsx": "react-jsx",
    "strict": true,
    "noUncheckedIndexedAccess": true,
    "noImplicitOverride": true,
    "noFallthroughCasesInSwitch": true,
    "exactOptionalPropertyTypes": false,
    "useUnknownInCatchVariables": true,
    "forceConsistentCasingInFileNames": true,
    "isolatedModules": true,
    "resolveJsonModule": true,
    "allowImportingTsExtensions": true,
    "noEmit": true,
    "skipLibCheck": true,
    "types": ["vite/client", "node"]
  },
  "include": ["src", "vite.config.ts", "vitest.config.ts"]
}
```

- [ ] **Step 6: Write `app/ui/vite.config.ts`**

```ts
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { fileURLToPath } from "node:url";

// The UI is a static page Tauri embeds (frontendDist = app/ui/dist). In development the Tauri
// CLI starts this server (devUrl = http://localhost:1420) and the window loads it; the same
// server, with VITE_MOCK_IPC=1, is the visual check in a browser without Tauri (the IPC is then
// the in-memory mock, never the real machine).
export default defineConfig({
  root: fileURLToPath(new URL(".", import.meta.url)),
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    fs: {
      // The mock reads the preset files of the repository (`presets/*.cmdtheme`).
      allow: [fileURLToPath(new URL("../..", import.meta.url))],
    },
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
    target: "es2022",
    sourcemap: false,
  },
});
```

- [ ] **Step 7: Write `app/ui/vitest.config.ts`**

```ts
import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import { fileURLToPath } from "node:url";

// UI tests run in jsdom against the in-memory IPC mock (`src/ipc/mock.ts`); nothing here ever
// reaches Tauri or this computer's settings.
export default defineConfig({
  root: fileURLToPath(new URL(".", import.meta.url)),
  plugins: [react()],
  test: {
    environment: "jsdom",
    globals: false,
    include: ["src/**/*.test.{ts,tsx}"],
    setupFiles: ["src/test/setup.ts"],
    server: {
      deps: {
        inline: ["@tauri-apps/api"],
      },
    },
  },
  server: {
    fs: {
      // The mock reads the preset files of the repository.
      allow: [fileURLToPath(new URL("../..", import.meta.url))],
    },
  },
});
```

- [ ] **Step 8: Write `app/ui/src/vite-env.d.ts`**

```ts
/// <reference types="vite/client" />

declare module "*.cmdtheme?raw" {
  const text: string;
  export default text;
}
```

- [ ] **Step 9: Write `app/ui/src/test/setup.ts`**

```ts
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
```

- [ ] **Step 10: Write `app/ui/src/styles.css`**

```css
/* CMD Studio's look: Windows 11 (Segoe UI Variable, Leelawadee UI for Thai), light and dark
   from the system (spec §4.5), sizes in rem so Windows' text size carries through, visible
   focus everywhere. No framework: tokens on :root, redefined for dark. */
:root {
  color-scheme: light dark;
  --bg: #f3f3f3;
  --surface: #ffffff;
  --surface-2: #f9f9f9;
  --text: #1b1b1b;
  --text-dim: #5f5f5f;
  --border: #d9d9d9;
  --accent: #b5542f;
  --accent-text: #ffffff;
  --focus: #005fb8;
  --ok: #107c10;
  --warn: #9d5d00;
  --bad: #c42b1c;
  --radius: 0.5rem;
  --gap: 1rem;
  font-family: "Segoe UI Variable Text", "Segoe UI", "Leelawadee UI", "Noto Sans Thai", system-ui, sans-serif;
  font-size: 100%;
  line-height: 1.45;
}

@media (prefers-color-scheme: dark) {
  :root {
    --bg: #202020;
    --surface: #2b2b2b;
    --surface-2: #333333;
    --text: #f3f3f3;
    --text-dim: #b3b3b3;
    --border: #454545;
    --accent: #e08a66;
    --accent-text: #1b1b1b;
    --focus: #60cdff;
    --ok: #6ccb5f;
    --warn: #fce100;
    --bad: #ff99a4;
  }
}

* {
  box-sizing: border-box;
}

html,
body,
#root {
  margin: 0;
  min-height: 100%;
  background: var(--bg);
  color: var(--text);
}

:lang(th) {
  font-family: "Leelawadee UI", "Segoe UI Variable Text", "Segoe UI", "Noto Sans Thai", system-ui, sans-serif;
}

:focus-visible {
  outline: 0.1875rem solid var(--focus);
  outline-offset: 0.125rem;
}

.skip-link {
  position: absolute;
  left: -200vw;
  top: 0.5rem;
  background: var(--surface);
  padding: 0.5rem 0.75rem;
  border-radius: var(--radius);
}
.skip-link:focus {
  left: 0.5rem;
  z-index: 10;
}

.shell {
  display: grid;
  grid-template-columns: 13rem 1fr;
  min-height: 100vh;
}

.sidebar {
  background: var(--surface);
  border-right: 1px solid var(--border);
  padding: 1rem 0.75rem;
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}
.sidebar .brand {
  font-weight: 600;
  font-size: 1.1rem;
  padding: 0.5rem 0.75rem 1rem;
}
.sidebar button {
  text-align: left;
  width: 100%;
}
.sidebar button[aria-current="page"] {
  background: var(--surface-2);
  border-color: var(--accent);
}

main {
  padding: 1.5rem 2rem 2rem;
  max-width: 72rem;
}

h1 {
  font-size: 1.6rem;
  margin: 0 0 1rem;
}
h1:focus {
  outline: none;
}
h2 {
  font-size: 1.15rem;
  margin: 1.25rem 0 0.5rem;
}
h3 {
  font-size: 1rem;
  margin: 1rem 0 0.25rem;
}
p {
  margin: 0.25rem 0 0.75rem;
}

button,
.button {
  font: inherit;
  color: var(--text);
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 0.45rem 0.9rem;
  cursor: pointer;
  min-height: 2.25rem;
}
button:hover {
  background: var(--surface-2);
}
button:disabled {
  opacity: 0.55;
  cursor: default;
}
button.primary {
  background: var(--accent);
  color: var(--accent-text);
  border-color: var(--accent);
}
button.danger {
  border-color: var(--bad);
  color: var(--bad);
}
button.link {
  background: none;
  border: none;
  color: var(--focus);
  padding: 0.25rem 0.25rem;
  text-decoration: underline;
  min-height: 0;
}

input,
select,
textarea {
  font: inherit;
  color: var(--text);
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 0.4rem 0.6rem;
  min-height: 2.25rem;
}
input[type="color"] {
  padding: 0.1rem;
  width: 3rem;
  height: 2.25rem;
}
input[type="range"] {
  min-height: 0;
  padding: 0;
  width: 12rem;
  vertical-align: middle;
}
label {
  display: inline-flex;
  flex-direction: column;
  gap: 0.25rem;
  margin: 0.25rem 1rem 0.5rem 0;
}
label.row {
  flex-direction: row;
  align-items: center;
  gap: 0.5rem;
}
fieldset {
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 0.75rem 1rem;
  margin: 0.75rem 0;
}
legend {
  padding: 0 0.4rem;
  color: var(--text-dim);
}

.card {
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 1rem 1.25rem;
  margin: 0.75rem 0;
}
.row-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 0.5rem;
  margin-top: 0.75rem;
}
.grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(16rem, 1fr));
  gap: var(--gap);
}
.theme-card {
  background: var(--surface);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 0.75rem;
}
.theme-card.selected {
  border-color: var(--accent);
  box-shadow: 0 0 0 0.125rem var(--accent);
}
.theme-card .swatches {
  display: flex;
  gap: 0.2rem;
  margin: 0.5rem 0;
}
.theme-card .swatches span {
  flex: 1;
  height: 1rem;
  border-radius: 0.2rem;
}
.badge {
  display: inline-block;
  font-size: 0.8rem;
  border-radius: 1rem;
  padding: 0.1rem 0.6rem;
  border: 1px solid var(--border);
  color: var(--text-dim);
  margin-left: 0.4rem;
}
.badge.in-use {
  color: var(--ok);
  border-color: var(--ok);
}

.terminal {
  margin: 0;
  padding: 0.75rem 1rem;
  border-radius: var(--radius);
  border: 1px solid var(--border);
  overflow-x: auto;
  white-space: pre;
  font-size: 0.85rem;
  line-height: 1.35;
  min-height: 6rem;
}

.notice {
  border-left: 0.25rem solid var(--focus);
  background: var(--surface);
  padding: 0.6rem 0.9rem;
  border-radius: var(--radius);
  margin: 0.75rem 0;
}
.notice.error {
  border-left-color: var(--bad);
}
.notice.ok {
  border-left-color: var(--ok);
}
.notice.warn {
  border-left-color: var(--warn);
}
.notice details {
  margin-top: 0.4rem;
}
.notice pre {
  white-space: pre-wrap;
  font-size: 0.85rem;
  margin: 0.4rem 0 0;
}

.bar {
  position: sticky;
  top: 0;
  z-index: 5;
  background: var(--surface);
  border-bottom: 1px solid var(--border);
  padding: 0.6rem 2rem;
  display: flex;
  align-items: center;
  gap: 1rem;
}

.status-ok {
  color: var(--ok);
}
.status-warn {
  color: var(--warn);
}
.status-fail {
  color: var(--bad);
}
.status-skipped {
  color: var(--text-dim);
}

.dialog-backdrop {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.35);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 20;
}
.dialog {
  background: var(--surface);
  border-radius: var(--radius);
  padding: 1.25rem 1.5rem;
  max-width: 40rem;
  width: calc(100% - 2rem);
  max-height: calc(100vh - 2rem);
  overflow: auto;
}

.wizard {
  max-width: 48rem;
  margin: 0 auto;
}
.wizard-nav {
  display: flex;
  justify-content: space-between;
  gap: 0.5rem;
  margin-top: 1.5rem;
}

.check-list,
.plain-list {
  list-style: none;
  padding: 0;
  margin: 0.5rem 0;
}
.check-list li,
.plain-list li {
  padding: 0.35rem 0;
  border-bottom: 1px solid var(--border);
}

.editor {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(22rem, 28rem);
  gap: 1.5rem;
  align-items: start;
}
.editor .preview-column {
  position: sticky;
  top: 4rem;
}
.color-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(11rem, 1fr));
  gap: 0.4rem 1rem;
}
.color-grid label {
  flex-direction: row;
  align-items: center;
  margin: 0;
}
.segments li {
  display: flex;
  align-items: center;
  gap: 0.5rem;
}
.tabs {
  display: flex;
  flex-wrap: wrap;
  gap: 0.25rem;
  border-bottom: 1px solid var(--border);
  margin-bottom: 1rem;
}
.tabs button[aria-selected="true"] {
  border-bottom: 0.1875rem solid var(--accent);
  border-radius: var(--radius) var(--radius) 0 0;
}
.log {
  max-height: 20rem;
  overflow: auto;
  background: var(--surface-2);
  padding: 0.75rem;
  font-size: 0.85rem;
  white-space: pre-wrap;
}
.visually-hidden {
  position: absolute;
  width: 1px;
  height: 1px;
  overflow: hidden;
  clip: rect(0 0 0 0);
  white-space: nowrap;
}
progress {
  width: 100%;
}
```

- [ ] **Step 11: Install and type-check**

Run (in `app/`): `node --version` → `v24.21.0` (else `nvm use` / install the pinned Node first). `npm ci`
Expected: installs from the lockfile with no change to it (`git status` shows `package-lock.json` as you wrote it); `npm ls --depth=0` lists the 3 dependencies and 11 devDependencies at the versions of `package.json`.
Run: `npm run typecheck`
Expected: clean (no `.ts` of the app yet beyond the setup and the declaration file).

- [ ] **Step 12: Commit**

Stage `app/package.json app/.nvmrc app/package-lock.json app/ui/index.html app/ui/tsconfig.json app/ui/vite.config.ts app/ui/vitest.config.ts app/ui/src/vite-env.d.ts app/ui/src/styles.css app/ui/src/test/setup.ts`. Message: `ui: the Node toolchain, pinned (Node 24.21.0, Vite 8.3.4, React 19.3.1, vitest 5.0.5)`.

---

### Task 2: The two dictionaries and their completeness test

Spec §4.5 and the founder's rules: English is the default, Thai the choice, every string in both. `en.ts` is the source of the keys (`Key = keyof typeof en`), `th.ts` is typed `Dictionary` so a missing key is a type error; the test adds what types cannot: no empty text, the same `{holes}` in both, Thai characters only in the Thai dictionary (except `language.th`, the word "ไทย" shown in English), and English-only text in the Thai dictionary only for the names that are the same in both (products, codes, the signing line). `index.tsx` gives `I18nProvider`, `useI18n`, `useT`, `fill`, `formatTime` (Thai shows the Buddhist year) and `pick` (a bilingual value from Rust).

**Files:**
- Create: `app/ui/src/i18n/types.ts`, `app/ui/src/i18n/en.ts`, `app/ui/src/i18n/th.ts`, `app/ui/src/i18n/index.tsx`, `app/ui/src/i18n/i18n.test.ts`
- Test: `i18n.test.ts` (7 tests)

**Interfaces:**
- Produces:
  ```ts
  export type Language = "en" | "th";  export type Key = keyof typeof en;  export type Dictionary = Record<Key, string>;
  export const en: { … 319 keys … };  export const th: Dictionary;
  export const DICTIONARIES: Record<Language, Dictionary>;
  export function fill(text: string, vars?: Record<string, string | number>): string;   // {name} holes; unknown holes stay
  export type Translate = (key: Key, vars?: Record<string, string | number>) => string;
  export function I18nProvider(props: { language: Language; setLanguage: (l: Language) => void; children: ReactNode }): JSX.Element;
  export function useI18n(): { language: Language; setLanguage: (l: Language) => void; t: Translate };
  export function useT(): Translate;
  export function formatTime(ms: number, language: Language, withDate?: boolean): string;  // en-GB / th-TH Intl
  export function pick(language: Language, th: string, en: string): string;   // Thai when chosen and non-empty, else English
  ```
  Key families: `app.*`, `common.*`, `nav.*`, `language.*`, `a11y.*`, `error.*`, `apply.*` (plan, steps, phases, warnings, outcomes), `wizard.*`, `home.*`, `themes.*`, `editor.*`, `settings.*`, `help.*` (tabs, doctor, restore, revert, report, guide, credits), `advanced.*`. `home.update.placeholder` is the one allowed empty text: Phase 4's "Updated to vX" line.

- [ ] **Step 1: Write the failing test** — `app/ui/src/i18n/i18n.test.ts`:

```ts
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
```

Run: `npm test`
Expected: FAIL — `Failed to resolve import "./en"` (the dictionaries do not exist).

- [ ] **Step 2: Write `app/ui/src/i18n/types.ts`**

```ts
import type { en } from "./en";

/** Every key of the English dictionary; the Thai one must have exactly these. */
export type Key = keyof typeof en;
export type Dictionary = Record<Key, string>;
export type Language = "en" | "th";
```

- [ ] **Step 3: Write `app/ui/src/i18n/en.ts`**

```ts
// Every string the UI shows, in English: the app's main language (founder 2026-10-13), the
// language the app opens in (founder 2026-10-16). `th.ts` has the same keys in Thai; the
// type `Dictionary` and `i18n.test.ts` keep the two complete. No technical term on the main
// screens (spec §4.5): "restore point", "add-on", "shorten typed commands", "readable".
export const en = {
  "app.name": "CMD Studio",
  "a11y.skipToContent": "Skip to content",
  "a11y.mainMenu": "Main menu",
  "a11y.closeDialog": "Close",
  "a11y.terminalPreview": "Preview of the cmd window",

  "nav.home": "Home",
  "nav.themes": "Themes",
  "nav.settings": "Settings",
  "nav.help": "Help",
  "nav.advanced": "Advanced",

  "common.next": "Next",
  "common.back": "Back",
  "common.cancel": "Cancel",
  "common.close": "Close",
  "common.loading": "Loading…",
  "common.details": "Details",
  "common.copy": "Copy",
  "common.copied": "Copied",
  "common.retry": "Try again",
  "common.save": "Save",
  "common.delete": "Delete",
  "common.rename": "Rename",
  "common.duplicate": "Duplicate",
  "common.export": "Export",
  "common.import": "Import",
  "common.customize": "Customize",
  "common.edit": "Edit",
  "common.dismiss": "Dismiss",
  "common.refresh": "Check again",
  "common.name": "Name",

  "language.label": "Language",
  "language.en": "English",
  "language.th": "ไทย",

  "error.title": "Something went wrong",
  "error.code": "Code {code}",

  "wizard.step": "Step {n} of 6",
  "wizard.skip": "Skip, set up myself",
  "wizard.welcome.title": "Make your cmd window beautiful and easy to read",
  "wizard.welcome.text": "Pick a theme, see it applied, and undo it at any time. Nothing needs an administrator.",
  "wizard.check.title": "Checking this computer",
  "wizard.check.checking": "Looking at Windows, Windows Terminal and your cmd settings…",
  "wizard.check.windows": "Windows build {build}",
  "wizard.check.windowsUnknown": "Windows version could not be read; the check goes on without it",
  "wizard.check.wtPresent": "Windows Terminal is installed: your cmd windows get colors",
  "wizard.check.wtMissing": "Windows Terminal is not installed: the prompt and welcome box work, but the window keeps its colors",
  "wizard.check.wtInstall": "Install Windows Terminal",
  "wizard.check.wtSkipHint": "You can skip this and install it later.",
  "wizard.check.wtTooOld": "Windows Terminal needs Windows 10 version 2004 or later, so this computer cannot have it",
  "wizard.check.autorunNone": "No other program sets up cmd",
  "wizard.check.autorunKept": "{n} other program(s) already set up cmd; they stay as they are",
  "wizard.check.otherClink": "Another cmd add-on is already set up; we will ask before replacing it",
  "wizard.check.machineWideClink": "A cmd add-on is set up for the whole computer; the prompt may not show, colors still do",
  "wizard.check.noAdmin": "No administrator rights are needed",
  "wizard.check.arm64": "This computer is not x64 Windows; the add-ons that come with CMD Studio run on x64 only",
  "wizard.choose.title": "Choose a theme",
  "wizard.choose.canChangeLater": "You can always change the colors later.",
  "wizard.choose.seeAll": "See all 12 themes",
  "wizard.choose.seeFewer": "Show the recommended 6",
  "wizard.where.title": "Where should the theme go?",
  "wizard.where.all": "Every cmd window",
  "wizard.where.allHint": "Recommended. Windows run from the Start menu, Win+R or Windows Terminal all get it.",
  "wizard.where.wt": "Only in Windows Terminal",
  "wizard.where.wtHint": "cmd windows outside Windows Terminal stay as they are.",
  "wizard.where.setDefault": "Open Windows Terminal with cmd",
  "wizard.confirm.title": "Ready to apply",
  "wizard.confirm.backup": "We save a restore point first. You can undo at any time.",
  "wizard.done.title": "Done",
  "wizard.done.text": "Your theme is applied. Open a new cmd window to see the welcome box and the prompt.",
  "wizard.done.tryCmd": "Try cmd",
  "wizard.done.goHome": "Go to the home page",

  "apply.planTitle": "What will change",
  "apply.steps": "Steps",
  "apply.warnings": "Good to know",
  "apply.start": "Apply",
  "apply.working": "Applying…",
  "apply.phase.backingUp": "Saving a restore point ({step} of {total})",
  "apply.phase.applying": "Applying ({step} of {total})",
  "apply.phase.verifying": "Checking the result",
  "apply.phase.rollingBack": "Putting things back ({step} of {total})",
  "apply.step.installOhMyPosh": "Copy the prompt add-on into CMD Studio's folder",
  "apply.step.installClink": "Copy the cmd add-on into CMD Studio's folder",
  "apply.step.writeGenerated": "Write {detail}",
  "apply.step.windowsTerminal": "Update Windows Terminal's settings",
  "apply.step.autoRun": "Let every cmd window use this theme",
  "apply.step.installFont": "Install the font {detail} for this account",
  "apply.step.saveState": "Remember what was applied",
  "apply.warning.noWindowsTerminal": "Windows Terminal is not installed: only the prompt and the welcome box change.",
  "apply.warning.machineWideClink": "A cmd add-on is set up for the whole computer; the theme's prompt may not show.",
  "apply.warning.fontFallback": 'The font "{wanted}" is not installed; Windows Terminal gets CaskaydiaCove NF instead.',
  "apply.otherClink.title": "Another cmd add-on is already set up",
  "apply.otherClink.text": "Turning it off takes these commands out of cmd's start-up. You can undo this.",
  "apply.otherClink.replace": "Use CMD Studio's instead",
  "apply.done": 'Theme "{name}" applied · open a new cmd window to see the welcome box and the prompt',
  "apply.doneWt": 'Theme "{name}" applied · open a new cmd tab in Windows Terminal to see the welcome box and the prompt',
  "apply.undo": "Undo",
  "apply.undone": "Undone: everything is as it was before.",
  "apply.undoNothing": "Nothing to undo.",
  "apply.failed": "The theme could not be applied. Everything was put back as it was.",
  "apply.failedIncomplete": "The theme could not be applied, and not everything could be put back. Fix these by hand:",
  "apply.planChanged": "This computer changed while you were looking. Here is the new plan; check it again.",
  "apply.copyDiagnostics": "Copy details for a problem report",
  "apply.notesTitle": "Found when starting",
  "apply.unrestoredTitle": "To fix by hand",

  "home.title": "Home",
  "home.status.title": "Status",
  "home.status.checking": "Checking…",
  "home.status.ok": "Everything looks fine",
  "home.status.warn": "{n} thing(s) to look at",
  "home.status.fail": "{n} problem(s) found",
  "home.status.checkedAt": "Checked at {time}",
  "home.status.open": "See the checks",
  "home.update.placeholder": "",
  "home.active.title": "Theme in use",
  "home.active.none": "No theme is applied yet.",
  "home.active.pick": "Choose a theme",
  "home.active.customize": "Customize",
  "home.active.change": "Change theme",
  "home.active.openCmd": "Open cmd",
  "home.quick.title": "Switch quickly",
  "home.quick.none": "Themes you used recently appear here.",
  "home.restore.title": "Latest restore point",
  "home.restore.none": "None yet. One is saved every time a theme is applied.",
  "home.restore.latest": "{name} · {time}",
  "home.restore.all": "All restore points",

  "themes.title": "Themes",
  "themes.filter.label": "Show",
  "themes.filter.all": "All",
  "themes.filter.dark": "Dark",
  "themes.filter.light": "Light",
  "themes.inUse": "In use",
  "themes.new": "New theme",
  "themes.import": "Import a theme file",
  "themes.presets": "Built-in themes",
  "themes.mine": "My themes",
  "themes.drafts": "Unsaved drafts",
  "themes.empty": "No theme matches this filter.",
  "themes.preset": "Built in",
  "themes.draft": "Draft",
  "themes.discardDraft": "Discard draft",
  "themes.deleteConfirm": 'Delete "{name}"? This cannot be undone.',
  "themes.deleteInUse": "This theme is in use. Deleting it keeps your cmd as it is until you apply another theme.",
  "themes.renamePrompt": "New name",
  "themes.copyName": "My {name}",
  "themes.newName": "New theme",
  "themes.problems": "{n} file(s) in the themes folder could not be read as themes.",
  "themes.exported": "Saved to {path}",
  "themes.import.title": "Import this theme?",
  "themes.import.idTaken": "A theme with the same id already exists; the import gets a new one.",
  "themes.import.nameTaken": "A theme with this name already exists.",
  "themes.import.confirm": "Add to my themes",
  "themes.import.cancelled": "No file was chosen.",
  "themes.apply": "Apply",

  "editor.title": "Edit theme",
  "editor.name": "Theme name",
  "editor.author": "Author",
  "editor.mode.simple": "Simple",
  "editor.mode.advanced": "Advanced",
  "editor.simple.tone": "Tone",
  "editor.simple.dark": "Dark",
  "editor.simple.light": "Light",
  "editor.simple.accent": "Main color",
  "editor.simple.tint": "Background tint",
  "editor.simple.keptNotice": "Colors you changed by hand are kept until you change the tone, the main color or the tint.",
  "editor.colors.title": "Colors",
  "editor.colors.background": "Background",
  "editor.colors.foreground": "Text",
  "editor.colors.cursor": "Cursor",
  "editor.colors.selection": "Selection",
  "editor.colors.accent": "Main color",
  "editor.colors.muted": "Dim text",
  "editor.colors.frame": "Frame",
  "editor.colors.ansi": "The 16 terminal colors",
  "editor.colors.ansi0": "Black",
  "editor.colors.ansi1": "Red",
  "editor.colors.ansi2": "Green",
  "editor.colors.ansi3": "Yellow",
  "editor.colors.ansi4": "Blue",
  "editor.colors.ansi5": "Magenta",
  "editor.colors.ansi6": "Cyan",
  "editor.colors.ansi7": "White",
  "editor.colors.ansi8": "Bright black",
  "editor.colors.ansi9": "Bright red",
  "editor.colors.ansi10": "Bright green",
  "editor.colors.ansi11": "Bright yellow",
  "editor.colors.ansi12": "Bright blue",
  "editor.colors.ansi13": "Bright magenta",
  "editor.colors.ansi14": "Bright cyan",
  "editor.colors.ansi15": "Bright white",
  "editor.readability.title": "Readability",
  "editor.readability.ok": "Every color is easy to read.",
  "editor.readability.issue": "{slot} is hard to read ({ratio}:1, needs {required}:1)",
  "editor.readability.fix": "Fix automatically",
  "editor.font.title": "Font and cursor",
  "editor.font.family": "Font",
  "editor.font.custom": "Another font name",
  "editor.font.size": "Size",
  "editor.font.cursor": "Cursor",
  "editor.font.cursor.filledBox": "Block",
  "editor.font.cursor.bar": "Bar",
  "editor.font.cursor.underscore": "Underline",
  "editor.font.opacity": "Window opacity",
  "editor.font.acrylic": "Frosted background",
  "editor.font.paddingX": "Side padding",
  "editor.font.paddingY": "Top and bottom padding",
  "editor.font.powerlineNotice": "The powerline look needs special characters, so the font was switched to CaskaydiaCove NF.",
  "editor.prompt.title": "Prompt",
  "editor.prompt.layout": "Look",
  "editor.prompt.layout.box": "Box",
  "editor.prompt.layout.line": "One line",
  "editor.prompt.layout.powerline": "Powerline",
  "editor.prompt.segments": "What the prompt shows",
  "editor.prompt.segment.path": "Folder",
  "editor.prompt.segment.git": "Git branch",
  "editor.prompt.segment.time": "Time",
  "editor.prompt.segment.user": "User name",
  "editor.prompt.segment.host": "Computer name",
  "editor.prompt.segment.status": "Last command failed",
  "editor.prompt.segment.duration": "How long the last command took",
  "editor.prompt.moveUp": "Move {segment} up",
  "editor.prompt.moveDown": "Move {segment} down",
  "editor.prompt.transient": "Shorten commands you already typed",
  "editor.banner.title": "Welcome box",
  "editor.banner.enabled": "Show a welcome box when cmd opens",
  "editor.banner.titleField": "Title",
  "editor.banner.lines": "Lines (up to 4)",
  "editor.banner.line": "Line {n}",
  "editor.banner.tips": "Show the tips line",
  "editor.banner.clearLogo": "Hide the Windows version text at the top",
  "editor.banner.variables": "You can use {user}, {host}, {cwd}, {date} and {time}.",
  "editor.preview.title": "Preview",
  "editor.preview.user": "Sample user name",
  "editor.preview.folder": "Sample folder",
  "editor.preview.width": "Window width",
  "editor.undo": "Undo",
  "editor.redo": "Redo",
  "editor.save": "Save",
  "editor.saveApply": "Save and apply",
  "editor.saved": "Saved.",
  "editor.draftSaved": "Draft saved",
  "editor.draftFound": "You have unsaved changes from before.",
  "editor.useDraft": "Continue them",
  "editor.discardDraft": "Discard them",
  "editor.back": "Back to themes",

  "settings.title": "Settings",
  "settings.target.title": "Where themes go",
  "settings.target.applyAgain": "Apply the current theme again with this choice",
  "settings.language.title": "Language",
  "settings.updates.title": "Updates",
  "settings.updates.auto": "Automatic (recommended)",
  "settings.updates.notify": "Tell me first",
  "settings.updates.off": "Off",
  "settings.updates.note": "Updates come with new versions of CMD Studio, checked each time it starts. This choice is saved now and takes effect when updating ships.",
  "settings.saved": "Saved.",
  "settings.reset": "Your settings could not be read and were reset. The old file is kept as {file}.",

  "help.title": "Help",
  "help.tabs.doctor": "Health check",
  "help.tabs.restore": "Restore points",
  "help.tabs.revert": "Undo everything",
  "help.tabs.report": "Report a problem",
  "help.tabs.guide": "Short guide",
  "help.tabs.credits": "Credits and licenses",
  "help.doctor.run": "Check now",
  "help.doctor.checkedAt": "Checked at {time}",
  "help.doctor.status.ok": "OK",
  "help.doctor.status.warn": "Look",
  "help.doctor.status.fail": "Problem",
  "help.doctor.status.skipped": "Skipped",
  "help.doctor.fix.reapply": "Apply the theme again",
  "help.doctor.fix.replaceOtherClink": "Replace the other add-on",
  "help.doctor.fix.installWindowsTerminal": "Install Windows Terminal",
  "help.doctor.fix.openWindowsTerminalOnce": "Open Windows Terminal once, then apply the theme again",
  "help.doctor.fix.openNewCmdWindow": "Open a new cmd window",
  "help.doctor.fix.reinstall": "Reinstall CMD Studio",
  "help.restore.title": "Restore points",
  "help.restore.none": "No restore points yet.",
  "help.restore.restore": "Go back to this point",
  "help.restore.confirm": "Go back to this point? Every theme applied after it is undone too.",
  "help.restore.kind.apply": "Theme applied",
  "help.restore.kind.undo": "Undo",
  "help.restore.kind.revertAll": "Everything undone",
  "help.restore.undone": "Undone",
  "help.restore.done": "Done: everything is back to that point.",
  "help.restore.stopped": "Stopped part-way; see the message.",
  "help.restore.notRestorable": "This point cannot be restored.",
  "help.restore.problems": "{n} restore point(s) could not be read.",
  "help.revert.title": "Undo everything",
  "help.revert.text": "Takes every change CMD Studio made out of this computer: cmd's start-up, Windows Terminal's settings and the font. Your themes stay. You can apply a theme again afterwards.",
  "help.revert.button": "Undo everything",
  "help.revert.confirm": "Undo everything CMD Studio changed?",
  "help.revert.done": "Done: everything CMD Studio changed is back as it was.",
  "help.revert.notOk": "Not everything could be undone. See the steps below.",
  "help.report.title": "Report a problem",
  "help.report.text": "Copy the details below (your user name is replaced by %USERNAME%) and paste them into a new issue on GitHub.",
  "help.report.copy": "Copy details",
  "help.report.copied": "Copied. Now open GitHub Issues and paste them.",
  "help.report.issues": "Open GitHub Issues",
  "help.guide.title": "Short guide",
  "help.guide.p1": "Pick a theme on the Themes page and press Apply. CMD Studio saves a restore point first, so you can undo at any time.",
  "help.guide.p2": "Open a new cmd window to see the welcome box and the prompt. Windows Terminal windows that are already open change their colors by themselves.",
  "help.guide.p3": "To make a theme your own, press Customize: you get a copy in My themes. Simple mode picks every color from one main color; Advanced mode lets you set all 23.",
  "help.guide.p4": "The Health check on this page tells you if something is off and offers a fix. Restore points let you go back; Undo everything takes CMD Studio's changes out completely.",
  "help.guide.p5": "Uninstalling CMD Studio puts everything back as well. Your themes are kept unless you tick the box to remove them.",
  "help.credits.title": "Credits and licenses",
  "help.credits.intro": "CMD Studio is open source under the MIT license. It bundles these programs, this font and these color schemes, each under its own license:",
  "help.credits.bundled": "Bundled programs and font",
  "help.credits.schemes": "Color schemes used by the built-in themes",
  "help.credits.signpath": "Free code signing provided by SignPath.io, certificate by SignPath Foundation",

  "advanced.title": "Advanced",
  "advanced.versions": "Add-on versions",
  "advanced.version.app": "CMD Studio",
  "advanced.version.ohMyPosh": "Oh My Posh",
  "advanced.version.clink": "Clink",
  "advanced.version.nerdFonts": "CaskaydiaCove NF (Nerd Fonts)",
  "advanced.openData": "Open the data folder",
  "advanced.openLogs": "Open the log folder",
  "advanced.dataDir": "Data folder",
  "advanced.log": "Log",
  "advanced.generated": "Generated files (read only)",
  "advanced.none": "Nothing yet: no theme has been applied.",
} as const satisfies Record<string, string>;
```

- [ ] **Step 4: Write `app/ui/src/i18n/th.ts`**

```ts
// ทุกข้อความของหน้าโปรแกรมเป็นภาษาไทย คีย์เหมือน `en.ts` ทุกตัว (type `Dictionary` บังคับ และ
// `i18n.test.ts` ตรวจซ้ำ) ใช้คำในตาราง §4.5 ของสเปก: จุดย้อนกลับ, ส่วนเสริม, ย่อคำสั่งที่พิมพ์ไปแล้ว,
// อ่านง่าย/อ่านยาก ชื่อโปรแกรมที่เป็นชื่อเฉพาะ (Windows Terminal, GitHub, cmd) คงเป็นอังกฤษ
import type { Dictionary } from "./types";

export const th: Dictionary = {
  "app.name": "CMD Studio",
  "a11y.skipToContent": "ข้ามไปที่เนื้อหา",
  "a11y.mainMenu": "เมนูหลัก",
  "a11y.closeDialog": "ปิด",
  "a11y.terminalPreview": "พรีวิวหน้าต่าง cmd",

  "nav.home": "หน้าแรก",
  "nav.themes": "ธีม",
  "nav.settings": "ตั้งค่า",
  "nav.help": "ช่วยเหลือ",
  "nav.advanced": "ขั้นสูง",

  "common.next": "ถัดไป",
  "common.back": "ย้อนกลับ",
  "common.cancel": "ยกเลิก",
  "common.close": "ปิด",
  "common.loading": "กำลังโหลด…",
  "common.details": "รายละเอียด",
  "common.copy": "คัดลอก",
  "common.copied": "คัดลอกแล้ว",
  "common.retry": "ลองใหม่",
  "common.save": "บันทึก",
  "common.delete": "ลบ",
  "common.rename": "เปลี่ยนชื่อ",
  "common.duplicate": "ทำสำเนา",
  "common.export": "ส่งออก",
  "common.import": "นำเข้า",
  "common.customize": "ปรับแต่ง",
  "common.edit": "แก้ไข",
  "common.dismiss": "ปิดข้อความ",
  "common.refresh": "ตรวจอีกครั้ง",
  "common.name": "ชื่อ",

  "language.label": "ภาษา",
  "language.en": "English",
  "language.th": "ไทย",

  "error.title": "มีบางอย่างผิดพลาด",
  "error.code": "รหัส {code}",

  "wizard.step": "ขั้นที่ {n} จาก 6",
  "wizard.skip": "ข้าม ตั้งค่าเอง",
  "wizard.welcome.title": "แต่งหน้าต่าง cmd ให้สวยและอ่านง่าย",
  "wizard.welcome.text": "เลือกธีม ดูผลทันที และย้อนกลับได้ทุกเมื่อ ไม่ต้องใช้สิทธิ์ผู้ดูแลระบบ",
  "wizard.check.title": "กำลังตรวจเครื่องนี้",
  "wizard.check.checking": "กำลังดู Windows, Windows Terminal และการตั้งค่า cmd ของคุณ…",
  "wizard.check.windows": "Windows build {build}",
  "wizard.check.windowsUnknown": "อ่านรุ่นของ Windows ไม่ได้ ตรวจข้ออื่นต่อไป",
  "wizard.check.wtPresent": "มี Windows Terminal แล้ว หน้าต่าง cmd ของคุณจะได้สีของธีม",
  "wizard.check.wtMissing": "ไม่มี Windows Terminal: prompt กับกล่องต้อนรับใช้ได้ แต่สีพื้นของหน้าต่างจะเป็นแบบเดิม",
  "wizard.check.wtInstall": "ติดตั้ง Windows Terminal",
  "wizard.check.wtSkipHint": "ข้ามไปก่อนแล้วค่อยติดตั้งทีหลังก็ได้",
  "wizard.check.wtTooOld": "Windows Terminal ต้องใช้ Windows 10 รุ่น 2004 ขึ้นไป เครื่องนี้จึงใช้ไม่ได้",
  "wizard.check.autorunNone": "ไม่มีโปรแกรมอื่นตั้งค่า cmd ไว้",
  "wizard.check.autorunKept": "มีโปรแกรมอื่น {n} รายการตั้งค่า cmd ไว้แล้ว เราจะเก็บไว้ตามเดิม",
  "wizard.check.otherClink": "มีส่วนเสริมของ cmd ตัวอื่นติดตั้งอยู่แล้ว เราจะถามก่อนแทนที่",
  "wizard.check.machineWideClink": "มีส่วนเสริมของ cmd ติดตั้งไว้ให้ทั้งเครื่อง prompt อาจไม่ขึ้น แต่สียังใช้ได้",
  "wizard.check.noAdmin": "ไม่ต้องใช้สิทธิ์ผู้ดูแลระบบ",
  "wizard.check.arm64": "เครื่องนี้ไม่ใช่ Windows แบบ x64 ส่วนเสริมที่มากับ CMD Studio ใช้ได้กับ x64 เท่านั้น",
  "wizard.choose.title": "เลือกธีม",
  "wizard.choose.canChangeLater": "ปรับสีเองได้ทีหลังเสมอ",
  "wizard.choose.seeAll": "ดูทั้งหมด 12 แบบ",
  "wizard.choose.seeFewer": "แสดง 6 แบบที่แนะนำ",
  "wizard.where.title": "ใช้ธีมที่ไหน",
  "wizard.where.all": "cmd ทุกหน้าต่าง",
  "wizard.where.allHint": "แนะนำ หน้าต่างที่เปิดจากเมนู Start, Win+R หรือ Windows Terminal ได้ธีมทั้งหมด",
  "wizard.where.wt": "เฉพาะใน Windows Terminal",
  "wizard.where.wtHint": "หน้าต่าง cmd นอก Windows Terminal เป็นแบบเดิม",
  "wizard.where.setDefault": "เปิด Windows Terminal มาเป็น cmd",
  "wizard.confirm.title": "พร้อมใช้ธีมแล้ว",
  "wizard.confirm.backup": "สำรองค่าเดิมไว้ก่อน ย้อนกลับได้ทุกเมื่อ",
  "wizard.done.title": "เสร็จแล้ว",
  "wizard.done.text": "ใช้ธีมแล้ว เปิด cmd หน้าต่างใหม่เพื่อดูกล่องต้อนรับกับ prompt",
  "wizard.done.tryCmd": "ลองเปิด cmd",
  "wizard.done.goHome": "ไปหน้าแรก",

  "apply.planTitle": "สิ่งที่จะเปลี่ยน",
  "apply.steps": "ขั้นตอน",
  "apply.warnings": "ควรรู้",
  "apply.start": "ใช้ธีม",
  "apply.working": "กำลังใช้ธีม…",
  "apply.phase.backingUp": "กำลังสำรองค่าเดิม ({step} จาก {total})",
  "apply.phase.applying": "กำลังใช้ธีม ({step} จาก {total})",
  "apply.phase.verifying": "กำลังตรวจผล",
  "apply.phase.rollingBack": "กำลังคืนค่าเดิม ({step} จาก {total})",
  "apply.step.installOhMyPosh": "คัดลอกส่วนเสริมของ prompt ไปไว้ในโฟลเดอร์ของ CMD Studio",
  "apply.step.installClink": "คัดลอกส่วนเสริมของ cmd ไปไว้ในโฟลเดอร์ของ CMD Studio",
  "apply.step.writeGenerated": "เขียนไฟล์ {detail}",
  "apply.step.windowsTerminal": "ปรับการตั้งค่าของ Windows Terminal",
  "apply.step.autoRun": "ให้ cmd ทุกหน้าต่างใช้ธีมนี้",
  "apply.step.installFont": "ติดตั้งฟอนต์ {detail} สำหรับบัญชีนี้",
  "apply.step.saveState": "จดไว้ว่าใช้ธีมอะไรอยู่",
  "apply.warning.noWindowsTerminal": "ไม่มี Windows Terminal: เปลี่ยนได้แค่ prompt กับกล่องต้อนรับ",
  "apply.warning.machineWideClink": "มีส่วนเสริมของ cmd ติดตั้งไว้ให้ทั้งเครื่อง prompt ของธีมอาจไม่ขึ้น",
  "apply.warning.fontFallback": 'เครื่องนี้ไม่มีฟอนต์ "{wanted}" Windows Terminal จะใช้ CaskaydiaCove NF แทน',
  "apply.otherClink.title": "มีส่วนเสริมของ cmd ตัวอื่นติดตั้งอยู่แล้ว",
  "apply.otherClink.text": "ถ้าปิด คำสั่งเหล่านี้จะถูกเอาออกจากการเริ่มต้นของ cmd ย้อนกลับได้",
  "apply.otherClink.replace": "ใช้ของ CMD Studio แทน",
  "apply.done": 'ใช้ธีม "{name}" แล้ว · เปิด cmd หน้าต่างใหม่เพื่อดูกล่องต้อนรับกับ prompt',
  "apply.doneWt": 'ใช้ธีม "{name}" แล้ว · เปิดแท็บ cmd ใหม่ใน Windows Terminal เพื่อดูกล่องต้อนรับกับ prompt',
  "apply.undo": "ย้อนกลับ",
  "apply.undone": "ย้อนกลับแล้ว ทุกอย่างเป็นเหมือนเดิม",
  "apply.undoNothing": "ไม่มีอะไรให้ย้อนกลับ",
  "apply.failed": "ใช้ธีมไม่สำเร็จ คืนทุกอย่างเป็นเหมือนเดิมแล้ว",
  "apply.failedIncomplete": "ใช้ธีมไม่สำเร็จ และคืนค่าเดิมได้ไม่ครบ ให้แก้รายการนี้ด้วยมือ:",
  "apply.planChanged": "เครื่องนี้มีการเปลี่ยนแปลงระหว่างที่คุณดูอยู่ นี่คือแผนใหม่ ตรวจดูอีกครั้ง",
  "apply.copyDiagnostics": "คัดลอกข้อมูลสำหรับแจ้งปัญหา",
  "apply.notesTitle": "สิ่งที่พบตอนเริ่ม",
  "apply.unrestoredTitle": "ต้องแก้ด้วยมือ",

  "home.title": "หน้าแรก",
  "home.status.title": "สถานะ",
  "home.status.checking": "กำลังตรวจ…",
  "home.status.ok": "ทุกอย่างปกติดี",
  "home.status.warn": "มี {n} เรื่องที่ควรดู",
  "home.status.fail": "พบปัญหา {n} เรื่อง",
  "home.status.checkedAt": "ตรวจล่าสุดเมื่อ {time}",
  "home.status.open": "ดูผลตรวจ",
  "home.update.placeholder": "",
  "home.active.title": "ธีมที่ใช้อยู่",
  "home.active.none": "ยังไม่ได้ใช้ธีม",
  "home.active.pick": "เลือกธีม",
  "home.active.customize": "ปรับแต่ง",
  "home.active.change": "เปลี่ยนธีม",
  "home.active.openCmd": "เปิด cmd",
  "home.quick.title": "สลับธีมเร็ว",
  "home.quick.none": "ธีมที่เพิ่งใช้จะมาอยู่ตรงนี้",
  "home.restore.title": "จุดย้อนกลับล่าสุด",
  "home.restore.none": "ยังไม่มี โปรแกรมจะสร้างให้ทุกครั้งที่ใช้ธีม",
  "home.restore.latest": "{name} · {time}",
  "home.restore.all": "จุดย้อนกลับทั้งหมด",

  "themes.title": "ธีม",
  "themes.filter.label": "แสดง",
  "themes.filter.all": "ทั้งหมด",
  "themes.filter.dark": "มืด",
  "themes.filter.light": "สว่าง",
  "themes.inUse": "ใช้อยู่",
  "themes.new": "สร้างธีมใหม่",
  "themes.import": "นำเข้าไฟล์ธีม",
  "themes.presets": "ธีมสำเร็จรูป",
  "themes.mine": "ธีมของฉัน",
  "themes.drafts": "ร่างที่ยังไม่ได้บันทึก",
  "themes.empty": "ไม่มีธีมตรงกับตัวกรองนี้",
  "themes.preset": "สำเร็จรูป",
  "themes.draft": "ร่าง",
  "themes.discardDraft": "ทิ้งร่าง",
  "themes.deleteConfirm": 'ลบ "{name}" ไหม การลบนี้ย้อนกลับไม่ได้',
  "themes.deleteInUse": "ธีมนี้ใช้อยู่ ลบแล้ว cmd ยังเป็นแบบเดิมจนกว่าจะใช้ธีมอื่น",
  "themes.renamePrompt": "ชื่อใหม่",
  "themes.copyName": "{name} ของฉัน",
  "themes.newName": "ธีมใหม่",
  "themes.problems": "มี {n} ไฟล์ในโฟลเดอร์ธีมที่อ่านเป็นธีมไม่ได้",
  "themes.exported": "บันทึกไว้ที่ {path}",
  "themes.import.title": "นำเข้าธีมนี้ไหม",
  "themes.import.idTaken": "มีธีมที่มีรหัสเดียวกันอยู่แล้ว ธีมที่นำเข้าจะได้รหัสใหม่",
  "themes.import.nameTaken": "มีธีมชื่อนี้อยู่แล้ว",
  "themes.import.confirm": "เพิ่มเข้าธีมของฉัน",
  "themes.import.cancelled": "ยังไม่ได้เลือกไฟล์",
  "themes.apply": "ใช้ธีม",

  "editor.title": "แก้ธีม",
  "editor.name": "ชื่อธีม",
  "editor.author": "ผู้สร้าง",
  "editor.mode.simple": "โหมดง่าย",
  "editor.mode.advanced": "โหมดละเอียด",
  "editor.simple.tone": "โทน",
  "editor.simple.dark": "มืด",
  "editor.simple.light": "สว่าง",
  "editor.simple.accent": "สีหลัก",
  "editor.simple.tint": "ย้อมพื้นหลัง",
  "editor.simple.keptNotice": "สีที่คุณแก้เองยังอยู่ จนกว่าจะเปลี่ยนโทน สีหลัก หรือค่าย้อมพื้นหลัง",
  "editor.colors.title": "สี",
  "editor.colors.background": "พื้นหลัง",
  "editor.colors.foreground": "ตัวอักษร",
  "editor.colors.cursor": "เคอร์เซอร์",
  "editor.colors.selection": "ส่วนที่เลือก",
  "editor.colors.accent": "สีหลัก",
  "editor.colors.muted": "ตัวอักษรจาง",
  "editor.colors.frame": "กรอบ",
  "editor.colors.ansi": "สี 16 สีของเทอร์มินัล",
  "editor.colors.ansi0": "ดำ",
  "editor.colors.ansi1": "แดง",
  "editor.colors.ansi2": "เขียว",
  "editor.colors.ansi3": "เหลือง",
  "editor.colors.ansi4": "น้ำเงิน",
  "editor.colors.ansi5": "ม่วง",
  "editor.colors.ansi6": "ฟ้า",
  "editor.colors.ansi7": "ขาว",
  "editor.colors.ansi8": "ดำสว่าง",
  "editor.colors.ansi9": "แดงสว่าง",
  "editor.colors.ansi10": "เขียวสว่าง",
  "editor.colors.ansi11": "เหลืองสว่าง",
  "editor.colors.ansi12": "น้ำเงินสว่าง",
  "editor.colors.ansi13": "ม่วงสว่าง",
  "editor.colors.ansi14": "ฟ้าสว่าง",
  "editor.colors.ansi15": "ขาวสว่าง",
  "editor.readability.title": "ความอ่านง่าย",
  "editor.readability.ok": "ทุกสีอ่านง่าย",
  "editor.readability.issue": "{slot} อ่านยาก ({ratio}:1 ต้องได้ {required}:1)",
  "editor.readability.fix": "แก้ให้อัตโนมัติ",
  "editor.font.title": "ฟอนต์และเคอร์เซอร์",
  "editor.font.family": "ฟอนต์",
  "editor.font.custom": "ชื่อฟอนต์อื่น",
  "editor.font.size": "ขนาด",
  "editor.font.cursor": "เคอร์เซอร์",
  "editor.font.cursor.filledBox": "บล็อก",
  "editor.font.cursor.bar": "ขีดตั้ง",
  "editor.font.cursor.underscore": "ขีดล่าง",
  "editor.font.opacity": "ความทึบของหน้าต่าง",
  "editor.font.acrylic": "พื้นหลังแบบฝ้า",
  "editor.font.paddingX": "ระยะขอบซ้ายขวา",
  "editor.font.paddingY": "ระยะขอบบนล่าง",
  "editor.font.powerlineNotice": "แบบ powerline ต้องใช้อักษรพิเศษ จึงสลับฟอนต์เป็น CaskaydiaCove NF ให้แล้ว",
  "editor.prompt.title": "Prompt",
  "editor.prompt.layout": "รูปแบบ",
  "editor.prompt.layout.box": "กล่อง",
  "editor.prompt.layout.line": "บรรทัดเดียว",
  "editor.prompt.layout.powerline": "Powerline",
  "editor.prompt.segments": "สิ่งที่ prompt แสดง",
  "editor.prompt.segment.path": "โฟลเดอร์",
  "editor.prompt.segment.git": "branch ของ Git",
  "editor.prompt.segment.time": "เวลา",
  "editor.prompt.segment.user": "ชื่อผู้ใช้",
  "editor.prompt.segment.host": "ชื่อเครื่อง",
  "editor.prompt.segment.status": "คำสั่งก่อนหน้าล้มเหลว",
  "editor.prompt.segment.duration": "คำสั่งก่อนหน้าใช้เวลาเท่าไร",
  "editor.prompt.moveUp": "เลื่อน {segment} ขึ้น",
  "editor.prompt.moveDown": "เลื่อน {segment} ลง",
  "editor.prompt.transient": "ย่อคำสั่งที่พิมพ์ไปแล้ว",
  "editor.banner.title": "กล่องต้อนรับ",
  "editor.banner.enabled": "แสดงกล่องต้อนรับเมื่อเปิด cmd",
  "editor.banner.titleField": "หัวข้อ",
  "editor.banner.lines": "บรรทัด (ไม่เกิน 4)",
  "editor.banner.line": "บรรทัดที่ {n}",
  "editor.banner.tips": "แสดงบรรทัด tips",
  "editor.banner.clearLogo": "ซ่อนข้อความรุ่นของ Windows ด้านบน",
  "editor.banner.variables": "ใช้ {user}, {host}, {cwd}, {date} และ {time} ได้",
  "editor.preview.title": "พรีวิว",
  "editor.preview.user": "ชื่อผู้ใช้ตัวอย่าง",
  "editor.preview.folder": "โฟลเดอร์ตัวอย่าง",
  "editor.preview.width": "ความกว้างหน้าต่าง",
  "editor.undo": "เลิกทำ",
  "editor.redo": "ทำซ้ำ",
  "editor.save": "บันทึก",
  "editor.saveApply": "บันทึกและใช้",
  "editor.saved": "บันทึกแล้ว",
  "editor.draftSaved": "บันทึกร่างแล้ว",
  "editor.draftFound": "คุณมีสิ่งที่แก้ค้างไว้จากคราวก่อน",
  "editor.useDraft": "แก้ต่อ",
  "editor.discardDraft": "ทิ้งไป",
  "editor.back": "กลับไปหน้าธีม",

  "settings.title": "ตั้งค่า",
  "settings.target.title": "ให้ธีมไปอยู่ตรงไหน",
  "settings.target.applyAgain": "ใช้ธีมปัจจุบันซ้ำตามตัวเลือกนี้",
  "settings.language.title": "ภาษา",
  "settings.updates.title": "การอัปเดต",
  "settings.updates.auto": "อัตโนมัติ (แนะนำ)",
  "settings.updates.notify": "แจ้งก่อน",
  "settings.updates.off": "ปิด",
  "settings.updates.note": "อัปเดตมาพร้อม CMD Studio รุ่นใหม่ โดยเช็กทุกครั้งที่เปิดโปรแกรม ตัวเลือกนี้บันทึกไว้แล้ว และจะมีผลเมื่อระบบอัปเดตมาถึง",
  "settings.saved": "บันทึกแล้ว",
  "settings.reset": "อ่านการตั้งค่าไม่ได้ จึงกลับเป็นค่าเริ่มต้น ไฟล์เดิมเก็บไว้ที่ {file}",

  "help.title": "ช่วยเหลือ",
  "help.tabs.doctor": "ตรวจสุขภาพ",
  "help.tabs.restore": "จุดย้อนกลับ",
  "help.tabs.revert": "ย้อนทุกอย่าง",
  "help.tabs.report": "แจ้งปัญหา",
  "help.tabs.guide": "คู่มือสั้น",
  "help.tabs.credits": "เครดิตและ license",
  "help.doctor.run": "ตรวจตอนนี้",
  "help.doctor.checkedAt": "ตรวจล่าสุดเมื่อ {time}",
  "help.doctor.status.ok": "ปกติ",
  "help.doctor.status.warn": "ควรดู",
  "help.doctor.status.fail": "มีปัญหา",
  "help.doctor.status.skipped": "ข้าม",
  "help.doctor.fix.reapply": "ใช้ธีมซ้ำ",
  "help.doctor.fix.replaceOtherClink": "แทนที่ส่วนเสริมตัวอื่น",
  "help.doctor.fix.installWindowsTerminal": "ติดตั้ง Windows Terminal",
  "help.doctor.fix.openWindowsTerminalOnce": "เปิด Windows Terminal หนึ่งครั้ง แล้วใช้ธีมซ้ำ",
  "help.doctor.fix.openNewCmdWindow": "เปิด cmd หน้าต่างใหม่",
  "help.doctor.fix.reinstall": "ติดตั้ง CMD Studio ใหม่",
  "help.restore.title": "จุดย้อนกลับ",
  "help.restore.none": "ยังไม่มีจุดย้อนกลับ",
  "help.restore.restore": "กลับไปที่จุดนี้",
  "help.restore.confirm": "กลับไปที่จุดนี้ไหม ธีมที่ใช้หลังจากจุดนี้จะถูกย้อนกลับด้วย",
  "help.restore.kind.apply": "ใช้ธีม",
  "help.restore.kind.undo": "ย้อนกลับ",
  "help.restore.kind.revertAll": "ย้อนทุกอย่าง",
  "help.restore.undone": "ย้อนกลับแล้ว",
  "help.restore.done": "เสร็จแล้ว ทุกอย่างกลับไปที่จุดนั้น",
  "help.restore.stopped": "หยุดกลางทาง ดูข้อความ",
  "help.restore.notRestorable": "จุดนี้กู้คืนไม่ได้",
  "help.restore.problems": "มีจุดย้อนกลับ {n} จุดที่อ่านไม่ได้",
  "help.revert.title": "ย้อนทุกอย่าง",
  "help.revert.text": "เอาทุกอย่างที่ CMD Studio เปลี่ยนไว้ออกจากเครื่องนี้: การเริ่มต้นของ cmd, การตั้งค่าของ Windows Terminal และฟอนต์ ธีมของคุณยังอยู่ และใช้ธีมใหม่ได้ทีหลัง",
  "help.revert.button": "ย้อนทุกอย่าง",
  "help.revert.confirm": "ย้อนทุกอย่างที่ CMD Studio เปลี่ยนไว้ไหม",
  "help.revert.done": "เสร็จแล้ว ทุกอย่างที่ CMD Studio เปลี่ยนกลับเป็นเหมือนเดิม",
  "help.revert.notOk": "ย้อนได้ไม่ครบ ดูขั้นตอนด้านล่าง",
  "help.report.title": "แจ้งปัญหา",
  "help.report.text": "คัดลอกข้อมูลด้านล่าง (ชื่อผู้ใช้ของคุณถูกแทนด้วย %USERNAME% แล้ว) แล้ววางใน issue ใหม่บน GitHub",
  "help.report.copy": "คัดลอกข้อมูล",
  "help.report.copied": "คัดลอกแล้ว เปิด GitHub Issues แล้ววางได้เลย",
  "help.report.issues": "เปิด GitHub Issues",
  "help.guide.title": "คู่มือสั้น",
  "help.guide.p1": "เลือกธีมในหน้าธีมแล้วกดใช้ธีม CMD Studio จะสร้างจุดย้อนกลับไว้ก่อน จึงย้อนกลับได้ทุกเมื่อ",
  "help.guide.p2": "เปิด cmd หน้าต่างใหม่เพื่อดูกล่องต้อนรับกับ prompt หน้าต่าง Windows Terminal ที่เปิดค้างอยู่จะเปลี่ยนสีเอง",
  "help.guide.p3": "อยากปรับเอง กดปรับแต่ง จะได้สำเนาในธีมของฉัน โหมดง่ายเลือกทุกสีจากสีหลักสีเดียว โหมดละเอียดตั้งได้ครบทั้ง 23 สี",
  "help.guide.p4": "ตรวจสุขภาพในหน้านี้บอกว่ามีอะไรผิดปกติและเสนอวิธีแก้ จุดย้อนกลับพากลับไปจุดก่อนหน้าได้ ส่วนย้อนทุกอย่างเอาสิ่งที่ CMD Studio เปลี่ยนออกทั้งหมด",
  "help.guide.p5": "ถอนโปรแกรมแล้วทุกอย่างกลับเป็นเหมือนเดิมเช่นกัน ธีมของคุณยังอยู่ เว้นแต่คุณติ๊กให้ลบ",
  "help.credits.title": "เครดิตและ license",
  "help.credits.intro": "CMD Studio เป็นโอเพนซอร์สภายใต้ license MIT และแนบโปรแกรม ฟอนต์ และชุดสีเหล่านี้มาด้วย แต่ละอย่างมี license ของตัวเอง:",
  "help.credits.bundled": "โปรแกรมและฟอนต์ที่แนบมา",
  "help.credits.schemes": "ชุดสีที่ธีมสำเร็จรูปใช้",
  "help.credits.signpath": "Free code signing provided by SignPath.io, certificate by SignPath Foundation",

  "advanced.title": "ขั้นสูง",
  "advanced.versions": "เวอร์ชันของส่วนเสริม",
  "advanced.version.app": "CMD Studio",
  "advanced.version.ohMyPosh": "Oh My Posh",
  "advanced.version.clink": "Clink",
  "advanced.version.nerdFonts": "CaskaydiaCove NF (Nerd Fonts)",
  "advanced.openData": "เปิดโฟลเดอร์ข้อมูล",
  "advanced.openLogs": "เปิดโฟลเดอร์ log",
  "advanced.dataDir": "โฟลเดอร์ข้อมูล",
  "advanced.log": "Log",
  "advanced.generated": "ไฟล์ที่โปรแกรมสร้าง (อ่านได้อย่างเดียว)",
  "advanced.none": "ยังไม่มี เพราะยังไม่ได้ใช้ธีม",
};
```

- [ ] **Step 5: Write `app/ui/src/i18n/index.tsx`**

```tsx
// The UI's two languages (spec §4.5). The app opens in English; Thai is a choice on the
// wizard's first page and in Settings, which the app remembers (founder 2026-10-16). The
// language is React state here, so a switch re-renders every screen at once.
import { createContext, useCallback, useContext, useMemo, type ReactNode } from "react";
import { en } from "./en";
import { th } from "./th";
import type { Dictionary, Key, Language } from "./types";

export type { Dictionary, Key, Language };

export const DICTIONARIES: Record<Language, Dictionary> = { en, th };

/** Fills `{name}` holes; a hole with no value stays as written (the welcome box's own
 * variables, such as `{user}` in a help text, are meant to show). */
export function fill(text: string, vars?: Record<string, string | number>): string {
  if (!vars) return text;
  return text.replace(/\{(\w+)\}/g, (whole, name: string) => {
    const value = vars[name];
    return value === undefined ? whole : String(value);
  });
}

export type Translate = (key: Key, vars?: Record<string, string | number>) => string;

interface I18nValue {
  language: Language;
  setLanguage: (language: Language) => void;
  t: Translate;
}

const I18nContext = createContext<I18nValue | null>(null);

export function I18nProvider({
  language,
  setLanguage,
  children,
}: {
  language: Language;
  setLanguage: (language: Language) => void;
  children: ReactNode;
}) {
  const t = useCallback<Translate>(
    (key, vars) => fill(DICTIONARIES[language][key], vars),
    [language],
  );
  const value = useMemo(() => ({ language, setLanguage, t }), [language, setLanguage, t]);
  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

export function useI18n(): I18nValue {
  const value = useContext(I18nContext);
  if (!value) throw new Error("useI18n needs an I18nProvider");
  return value;
}

export function useT(): Translate {
  return useI18n().t;
}

/** A clock time or a date and time in the UI's language (Thai shows the Buddhist year). */
export function formatTime(ms: number, language: Language, withDate = true): string {
  const locale = language === "th" ? "th-TH" : "en-GB";
  const options: Intl.DateTimeFormatOptions = withDate
    ? { dateStyle: "medium", timeStyle: "short" }
    : { timeStyle: "short" };
  return new Intl.DateTimeFormat(locale, options).format(new Date(ms));
}

/** The language's own text of a bilingual value from the Rust side. */
export function pick(language: Language, th: string, en: string): string {
  return language === "th" && th.length > 0 ? th : en;
}
```

- [ ] **Step 6: Run the tests**

Run: `npm test`
Expected: `Test Files 1 passed (1)`, `Tests 7 passed (7)`. `npm run typecheck` clean.

- [ ] **Step 7: See that the completeness test bites**

Delete the line `"editor.saveApply": "บันทึกและใช้",` from `th.ts`; `npm run typecheck` FAILS (`Property 'editor.saveApply' is missing`) and `npm test` FAILS `have exactly the same keys`. Put it back; both clean.

- [ ] **Step 8: Commit**

Stage `app/ui/src/i18n`. Message: `ui: English and Thai dictionaries (319 keys each) with the completeness test; English is the default`.

---

### Task 3: The `Ipc` contract, Tauri's implementation, and the mock

One interface, two implementations (index Decision 11). `types.ts` names every command with the Rust parameter names in camelCase; `IpcError` carries the Rust `AppError` and `toIpcError` turns anything else into `E_INTERNAL`. `tauri.ts` is `invoke`, plus `listen` on `apply-progress` for the duration of `apply`. `mock.ts` keeps themes, drafts, the applied theme, restore points, settings and a list of calls in memory, answers `planApply` with the real plan shape (and `otherClink` when told to), `apply` with progress events and a `planChanged` for a stale digest, `doctorRun` with the eleven checks; its palette is the preset's colors with the accent put in, its preview a frame and text in the theme's colors — fixed data, not a rendering (spec §5.6). `presets.ts` imports the twelve `.cmdtheme` files as raw text (Vite's `?raw`), which is why the configs allow the repository root.

**Files:**
- Create: `app/ui/src/ipc/types.ts`, `app/ui/src/ipc/tauri.ts`, `app/ui/src/ipc/context.tsx`, `app/ui/src/ipc/presets.ts`, `app/ui/src/ipc/mock.ts`, `app/ui/src/ipc/mock.test.ts`
- Test: `mock.test.ts` (2 tests)

**Interfaces:**
- Consumes: the bindings `app/ui/src/bindings/*.ts` (shell plan Task 15); `@tauri-apps/api/core` `invoke`, `@tauri-apps/api/event` `listen`; `presets/*.cmdtheme`.
- Produces:
  ```ts
  export interface Ipc {
    systemDetect(): Promise<SystemReport>;
    themesList(): Promise<Library>;  themeGet(id: string): Promise<ThemeDetail>;
    themeSave(theme: Theme, draft: boolean): Promise<ThemeSummary>;  themeDuplicate(id: string, name: string): Promise<ThemeSummary>;
    themeDelete(id: string, draftOnly: boolean): Promise<void>;
    themeImportPreview(): Promise<ImportPreview | null>;  themeImportConfirm(token: string): Promise<ThemeSummary>;  themeExport(id: string): Promise<string | null>;
    paletteGenerate(tone: Tone, accent: string, tint: number): Promise<Colors>;  contrastCheck(colors: Colors): Promise<ContrastIssue[]>;  contrastFix(colors: Colors): Promise<Colors>;
    previewRender(theme: Theme, context: PreviewContext): Promise<PreviewResult>;
    planApply(request: ApplyRequest): Promise<PlanOutcome>;
    apply(request: ApplyRequest, digest: string, onProgress: (p: ApplyProgress) => void): Promise<ApplyOutcome>;
    undoLast(): Promise<UndoOutcome>;  snapshotsList(): Promise<RestorePoints>;  snapshotRestore(id: string): Promise<RestoreOutcome>;  revertAll(): Promise<RevertOutcome>;
    doctorRun(): Promise<DoctorReport>;  doctorFix(checkId: string): Promise<FixProposal>;
    settingsGet(): Promise<SettingsLoad>;  settingsSet(settings: AppSettings): Promise<AppSettings>;
    openCmd(): Promise<void>;  openExternal(target: ExternalTarget): Promise<void>;
    diagnosticsCopy(): Promise<string>;  advancedRead(): Promise<AdvancedInfo>;
  }
  export class IpcError extends Error { code; messageTh; messageEn; details; toAppError(): AppError }
  export function toIpcError(thrown: unknown): IpcError;
  export const APPLY_PROGRESS_EVENT = "apply-progress";  export const tauriIpc: Ipc;
  export function IpcProvider(props: { ipc: Ipc; children: ReactNode }): JSX.Element;  export function useIpc(): Ipc;
  export const PRESETS: Theme[];   // the twelve, Terracotta first
  export const SAMPLE_CONTEXT: PreviewContext;   // user "you", host "PC", cwd C:\Users\you\projects\cmd-studio, 2026-10-09 16:47, branch main dirty, exit 1, 3500 ms, width 80
  export interface MockOptions { otherClink?: string[]; failApplyWith?: string; noWindowsTerminal?: boolean; settings?: Partial<AppSettings> }
  export interface MockState { mine: Theme[]; drafts: Theme[]; applied: { theme; request; snapshotId } | null; points: RestorePoint[]; settings: AppSettings; calls: string[]; clipboardSuggestion: string }
  export function createMockIpc(options?: MockOptions): Ipc & { state: MockState };
  ```

- [ ] **Step 1: Write the failing test** — `app/ui/src/ipc/mock.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { createMockIpc } from "./mock";

describe("the mock IPC", () => {
  it("lists the twelve presets and applies, undoes and restores like the backend", async () => {
    const ipc = createMockIpc();
    const lib = await ipc.themesList();
    expect(lib.themes).toHaveLength(12);
    expect(lib.themes[0]?.name).toBe("Terracotta");
    const request = { themeId: lib.themes[0]!.id, target: "all" as const, setWtDefaultProfile: true, replaceOtherClink: false };
    const plan = await ipc.planApply(request);
    expect(plan.kind).toBe("ready");
    if (plan.kind !== "ready") throw new Error("not ready");
    const progress: string[] = [];
    const done = await ipc.apply(request, plan.plan.digest, (p) => progress.push(p.phase));
    expect(done.status).toBe("committed");
    expect(progress[0]).toBe("backingUp");
    expect(progress[progress.length - 1]).toBe("verifying");
    expect((await ipc.themesList()).themes[0]?.inUse).toBe(true);
    const stale = await ipc.apply(request, "stale", () => {});
    expect(stale.status).toBe("planChanged");
    const points = await ipc.snapshotsList();
    expect(points.points).toHaveLength(1);
    expect(points.points[0]?.restorable).toBe(true);
    const restored = await ipc.snapshotRestore(points.points[0]!.id);
    expect(restored.status).toBe("done");
    expect((await ipc.undoLast()).status).toBe("nothingToUndo");
  });

  it("refuses to save or delete a preset and keeps drafts apart", async () => {
    const ipc = createMockIpc();
    const [preset] = (await ipc.themesList()).themes;
    const detail = await ipc.themeGet(preset!.id);
    await expect(ipc.themeSave(detail.theme, false)).rejects.toMatchObject({ code: "E_THEME_INVALID" });
    await expect(ipc.themeDelete(preset!.id, false)).rejects.toMatchObject({ code: "E_THEME_INVALID" });
    const copy = await ipc.themeDuplicate(preset!.id, "Mine");
    await ipc.themeSave({ ...detail.theme, id: copy.id, name: "Mine, drafted" }, true);
    expect((await ipc.themeGet(copy.id)).draft?.name).toBe("Mine, drafted");
    expect((await ipc.themesList()).themes.find((t) => t.id === copy.id)?.hasDraft).toBe(true);
    await ipc.themeDelete(copy.id, true);
    expect((await ipc.themeGet(copy.id)).draft).toBeNull();
  });
});
```

Run: `npm test`
Expected: FAIL — `Failed to resolve import "./mock"`.

- [ ] **Step 2: Write `app/ui/src/ipc/types.ts`**

```ts
// The IPC surface the screens use (spec §5.7, with preview_render, open_external and
// advanced_read). One interface, two implementations: `tauri.ts` over Tauri's invoke, and
// `mock.ts` in memory for tests and the visual check. The types are the ts-rs bindings the
// Rust side exports (app/ui/src/bindings, committed; CI fails when they are stale).
import type { AdvancedInfo } from "../bindings/AdvancedInfo";
import type { AppError } from "../bindings/AppError";
import type { AppSettings } from "../bindings/AppSettings";
import type { ApplyOutcome } from "../bindings/ApplyOutcome";
import type { ApplyProgress } from "../bindings/ApplyProgress";
import type { ApplyRequest } from "../bindings/ApplyRequest";
import type { Colors } from "../bindings/Colors";
import type { ContrastIssue } from "../bindings/ContrastIssue";
import type { DoctorReport } from "../bindings/DoctorReport";
import type { ExternalTarget } from "../bindings/ExternalTarget";
import type { FixProposal } from "../bindings/FixProposal";
import type { ImportPreview } from "../bindings/ImportPreview";
import type { Library } from "../bindings/Library";
import type { PlanOutcome } from "../bindings/PlanOutcome";
import type { PreviewContext } from "../bindings/PreviewContext";
import type { PreviewResult } from "../bindings/PreviewResult";
import type { RestoreOutcome } from "../bindings/RestoreOutcome";
import type { RestorePoints } from "../bindings/RestorePoints";
import type { RevertOutcome } from "../bindings/RevertOutcome";
import type { SettingsLoad } from "../bindings/SettingsLoad";
import type { SystemReport } from "../bindings/SystemReport";
import type { Theme } from "../bindings/Theme";
import type { ThemeDetail } from "../bindings/ThemeDetail";
import type { ThemeSummary } from "../bindings/ThemeSummary";
import type { Tone } from "../bindings/Tone";
import type { UndoOutcome } from "../bindings/UndoOutcome";

export interface Ipc {
  systemDetect(): Promise<SystemReport>;
  themesList(): Promise<Library>;
  themeGet(id: string): Promise<ThemeDetail>;
  themeSave(theme: Theme, draft: boolean): Promise<ThemeSummary>;
  themeDuplicate(id: string, name: string): Promise<ThemeSummary>;
  themeDelete(id: string, draftOnly: boolean): Promise<void>;
  /** Opens the native Open dialog; `null` when the user cancelled. */
  themeImportPreview(): Promise<ImportPreview | null>;
  themeImportConfirm(token: string): Promise<ThemeSummary>;
  /** Opens the native Save dialog; the path written, or `null` when cancelled. */
  themeExport(id: string): Promise<string | null>;
  paletteGenerate(tone: Tone, accent: string, tint: number): Promise<Colors>;
  contrastCheck(colors: Colors): Promise<ContrastIssue[]>;
  contrastFix(colors: Colors): Promise<Colors>;
  previewRender(theme: Theme, context: PreviewContext): Promise<PreviewResult>;
  planApply(request: ApplyRequest): Promise<PlanOutcome>;
  /** Applies, calling `onProgress` for each `apply-progress` event while it runs. */
  apply(
    request: ApplyRequest,
    digest: string,
    onProgress: (progress: ApplyProgress) => void,
  ): Promise<ApplyOutcome>;
  undoLast(): Promise<UndoOutcome>;
  snapshotsList(): Promise<RestorePoints>;
  snapshotRestore(id: string): Promise<RestoreOutcome>;
  revertAll(): Promise<RevertOutcome>;
  doctorRun(): Promise<DoctorReport>;
  doctorFix(checkId: string): Promise<FixProposal>;
  settingsGet(): Promise<SettingsLoad>;
  settingsSet(settings: AppSettings): Promise<AppSettings>;
  openCmd(): Promise<void>;
  openExternal(target: ExternalTarget): Promise<void>;
  diagnosticsCopy(): Promise<string>;
  advancedRead(): Promise<AdvancedInfo>;
}

/** A command failed: the Rust side's `AppError` (spec §5.9), as a JavaScript error. */
export class IpcError extends Error {
  readonly code: string;
  readonly messageTh: string;
  readonly messageEn: string;
  readonly details: string;

  constructor(error: AppError) {
    super(error.messageEn);
    this.name = "IpcError";
    this.code = error.code;
    this.messageTh = error.messageTh;
    this.messageEn = error.messageEn;
    this.details = error.details;
  }

  toAppError(): AppError {
    return {
      code: this.code,
      messageTh: this.messageTh,
      messageEn: this.messageEn,
      details: this.details,
    };
  }
}

function isAppError(value: unknown): value is AppError {
  if (typeof value !== "object" || value === null) return false;
  const v = value as Record<string, unknown>;
  return (
    typeof v.code === "string" &&
    typeof v.messageTh === "string" &&
    typeof v.messageEn === "string" &&
    typeof v.details === "string"
  );
}

/**
 * Whatever a command threw, as an `IpcError`. Tauri rejects with the serialized `AppError`;
 * anything else (a deserialization message from Tauri, a plain Error) becomes `E_INTERNAL`
 * with its text as the details.
 */
export function toIpcError(thrown: unknown): IpcError {
  if (thrown instanceof IpcError) return thrown;
  if (isAppError(thrown)) return new IpcError(thrown);
  const details = thrown instanceof Error ? thrown.message : String(thrown);
  return new IpcError({
    code: "E_INTERNAL",
    messageTh: "CMD Studio เองมีปัญหา รายละเอียดบอกว่าตรงไหน",
    messageEn: "CMD Studio itself ran into a problem; the details say where.",
    details,
  });
}
```

- [ ] **Step 3: Write `app/ui/src/ipc/tauri.ts`**

```ts
// The real IPC: Tauri's `invoke` for each command of spec §5.7, and `listen` for the
// `apply-progress` event while `apply` runs. Argument names are the Rust parameters in
// camelCase, as Tauri expects them.
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { ApplyProgress } from "../bindings/ApplyProgress";
import { toIpcError, type Ipc } from "./types";

export const APPLY_PROGRESS_EVENT = "apply-progress";

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (thrown) {
    throw toIpcError(thrown);
  }
}

export const tauriIpc: Ipc = {
  systemDetect: () => call("system_detect"),
  themesList: () => call("themes_list"),
  themeGet: (id) => call("theme_get", { id }),
  themeSave: (theme, draft) => call("theme_save", { theme, draft }),
  themeDuplicate: (id, name) => call("theme_duplicate", { id, name }),
  themeDelete: (id, draftOnly) => call("theme_delete", { id, draftOnly }),
  themeImportPreview: () => call("theme_import_preview"),
  themeImportConfirm: (token) => call("theme_import_confirm", { token }),
  themeExport: (id) => call("theme_export", { id }),
  paletteGenerate: (tone, accent, tint) => call("palette_generate", { tone, accent, tint }),
  contrastCheck: (colors) => call("contrast_check", { colors }),
  contrastFix: (colors) => call("contrast_fix", { colors }),
  previewRender: (theme, context) => call("preview_render", { theme, context }),
  planApply: (request) => call("plan_apply", { request }),
  apply: async (request, digest, onProgress) => {
    const unlisten = await listen<ApplyProgress>(APPLY_PROGRESS_EVENT, (event) => {
      onProgress(event.payload);
    });
    try {
      return await call("apply", { request, digest });
    } finally {
      unlisten();
    }
  },
  undoLast: () => call("undo_last"),
  snapshotsList: () => call("snapshots_list"),
  snapshotRestore: (id) => call("snapshot_restore", { id }),
  revertAll: () => call("revert_all"),
  doctorRun: () => call("doctor_run"),
  doctorFix: (checkId) => call("doctor_fix", { checkId }),
  settingsGet: () => call("settings_get"),
  settingsSet: (settings) => call("settings_set", { settings }),
  openCmd: () => call("open_cmd"),
  openExternal: (target) => call("open_external", { target }),
  diagnosticsCopy: () => call("diagnostics_copy"),
  advancedRead: () => call("advanced_read"),
};
```

- [ ] **Step 4: Write `app/ui/src/ipc/context.tsx`**

```tsx
import { createContext, useContext, type ReactNode } from "react";
import type { Ipc } from "./types";

const IpcContext = createContext<Ipc | null>(null);

export function IpcProvider({ ipc, children }: { ipc: Ipc; children: ReactNode }) {
  return <IpcContext.Provider value={ipc}>{children}</IpcContext.Provider>;
}

/** The IPC of the app: Tauri's in the window, the mock in tests and the visual check. */
export function useIpc(): Ipc {
  const ipc = useContext(IpcContext);
  if (!ipc) throw new Error("useIpc needs an IpcProvider");
  return ipc;
}
```

- [ ] **Step 5: Write `app/ui/src/ipc/presets.ts`**

```ts
// The twelve preset files of the repository, as the mock's library. Vite reads them as text
// (`?raw`); the real app never loads them this way (the Rust side embeds them). Only the mock
// imports this module, and the mock is never in the production bundle.
import type { Theme } from "../bindings/Theme";
import terracotta from "../../../../presets/terracotta.cmdtheme?raw";
import tokyoNight from "../../../../presets/tokyo-night.cmdtheme?raw";
import catppuccinMocha from "../../../../presets/catppuccin-mocha.cmdtheme?raw";
import dracula from "../../../../presets/dracula.cmdtheme?raw";
import nord from "../../../../presets/nord.cmdtheme?raw";
import gruvbox from "../../../../presets/gruvbox.cmdtheme?raw";
import oneDark from "../../../../presets/one-dark.cmdtheme?raw";
import retroGreen from "../../../../presets/retro-green.cmdtheme?raw";
import catppuccinLatte from "../../../../presets/catppuccin-latte.cmdtheme?raw";
import solarizedLight from "../../../../presets/solarized-light.cmdtheme?raw";
import daylight from "../../../../presets/daylight.cmdtheme?raw";
import paper from "../../../../presets/paper.cmdtheme?raw";

const SOURCES = [
  terracotta,
  tokyoNight,
  catppuccinMocha,
  dracula,
  nord,
  gruvbox,
  oneDark,
  retroGreen,
  catppuccinLatte,
  solarizedLight,
  daylight,
  paper,
];

export const PRESETS: Theme[] = SOURCES.map((text) => JSON.parse(text) as Theme);
```

- [ ] **Step 6: Write `app/ui/src/ipc/mock.ts`**

```ts
// An in-memory `Ipc` for the tests and the visual check (`VITE_MOCK_IPC=1`). It never reaches
// Tauri or this computer. It is a stand-in, not a second implementation of CMD Studio: the
// palette and the preview it answers are fixed data taken from the presets, not computed (spec
// §5.6: no palette, contrast or prompt logic in TypeScript; the real answers come from Rust).
import type { AdvancedInfo } from "../bindings/AdvancedInfo";
import type { AppError } from "../bindings/AppError";
import type { AppSettings } from "../bindings/AppSettings";
import type { ApplyOutcome } from "../bindings/ApplyOutcome";
import type { ApplyPlan } from "../bindings/ApplyPlan";
import type { ApplyProgress } from "../bindings/ApplyProgress";
import type { ApplyRequest } from "../bindings/ApplyRequest";
import type { CheckReport } from "../bindings/CheckReport";
import type { Colors } from "../bindings/Colors";
import type { ContrastIssue } from "../bindings/ContrastIssue";
import type { DoctorReport } from "../bindings/DoctorReport";
import type { FixProposal } from "../bindings/FixProposal";
import type { ImportPreview } from "../bindings/ImportPreview";
import type { Library } from "../bindings/Library";
import type { Line } from "../bindings/Line";
import type { PlanOutcome } from "../bindings/PlanOutcome";
import type { PreviewContext } from "../bindings/PreviewContext";
import type { PreviewResult } from "../bindings/PreviewResult";
import type { RestoreOutcome } from "../bindings/RestoreOutcome";
import type { RestorePoint } from "../bindings/RestorePoint";
import type { RestorePoints } from "../bindings/RestorePoints";
import type { RevertOutcome } from "../bindings/RevertOutcome";
import type { SettingsLoad } from "../bindings/SettingsLoad";
import type { SystemReport } from "../bindings/SystemReport";
import type { Theme } from "../bindings/Theme";
import type { ThemeDetail } from "../bindings/ThemeDetail";
import type { ThemeSource } from "../bindings/ThemeSource";
import type { ThemeSummary } from "../bindings/ThemeSummary";
import type { Tone } from "../bindings/Tone";
import type { UndoOutcome } from "../bindings/UndoOutcome";
import { IpcError, type Ipc } from "./types";
import { PRESETS } from "./presets";

export const SAMPLE_CONTEXT: PreviewContext = {
  user: "you",
  host: "PC",
  home: "C:\\Users\\you",
  cwd: "C:\\Users\\you\\projects\\cmd-studio",
  clock: { year: 2026, month: 10, day: 9, hour: 16, minute: 47 },
  git: { branch: "main", dirty: true },
  exitCode: 1,
  durationMs: 3500,
  width: 80,
  windowsTerminal: true,
  command: "npm test",
};

function error(code: string, details: string): IpcError {
  return new IpcError({ code, messageTh: `(${code})`, messageEn: `(${code})`, details });
}

function uuid(): string {
  const hex = () => Math.floor(Math.random() * 16).toString(16);
  const part = (n: number) => Array.from({ length: n }, hex).join("");
  const variant = ["8", "9", "a", "b"][Math.floor(Math.random() * 4)];
  return `${part(8)}-${part(4)}-4${part(3)}-${variant}${part(3)}-${part(12)}`;
}

export interface MockOptions {
  /** Another Clink in AutoRun: `planApply` answers `otherClink` until `replaceOtherClink`. */
  otherClink?: string[];
  /** Every apply fails its check and rolls back with this code. */
  failApplyWith?: string;
  /** Windows Terminal is not installed. */
  noWindowsTerminal?: boolean;
  settings?: Partial<AppSettings>;
}

/** The mock's own state, open to the tests. */
export interface MockState {
  mine: Theme[];
  drafts: Theme[];
  applied: { theme: Theme; request: ApplyRequest; snapshotId: string } | null;
  points: RestorePoint[];
  settings: AppSettings;
  calls: string[];
  clipboardSuggestion: string;
}

/** A frame-and-text preview from the theme's own colors: fixed data, not a rendering. */
function fakeLines(theme: Theme): PreviewResult {
  const c = theme.colors;
  const span = (text: string, fg: string, bold = false) => ({ text, fg, bg: null, bold });
  const banner: Line[] = [
    [span("╭──────────────────────────────────────────╮", c.accent)],
    [span("│ ", c.accent), span(`✻ ${theme.banner.title.replace("{user}", "you")}`, c.foreground), span(" │", c.accent)],
    [span("╰──────────────────────────────────────────╯", c.accent)],
    [span(" Tips: help for commands · cls to clear · ↑ for history", c.muted)],
    [span(" ", c.foreground)],
  ];
  const prompt: Line[] = [
    [span("> ", c.accent), span("npm test", c.foreground)],
    [span(" ", c.foreground)],
    [span("╭─ ", c.frame), span("~\\projects\\cmd-studio", c.accent), span(" · main *", c.ansi[3] ?? c.foreground), span(" ──────── ", c.frame), span("16:47", c.muted), span(" ─╮", c.frame)],
    [span("│ ", c.frame), span(">", c.accent), span(" ", c.foreground)],
  ];
  return { banner, prompt };
}

export function createMockIpc(options: MockOptions = {}): Ipc & { state: MockState } {
  const presets: Theme[] = PRESETS.map((t) => structuredClone(t));
  const byTone = (tone: Tone) => presets.find((p) => p.tone === tone) ?? presets[0]!;
  const state: MockState = {
    mine: [],
    drafts: [],
    applied: null,
    points: [],
    settings: {
      language: "en",
      target: "all",
      setWtDefaultProfile: true,
      updates: "auto",
      wizardDone: false,
      ...options.settings,
    },
    calls: [],
    clipboardSuggestion: "",
  };
  let nextMs = 1791694709000;
  const now = () => nextMs++;

  const all = (): Theme[] => [...presets, ...state.mine];
  const find = (id: string): [Theme, ThemeSource] => {
    const preset = presets.find((t) => t.id === id);
    if (preset) return [preset, "preset"];
    const mine = state.mine.find((t) => t.id === id);
    if (mine) return [mine, "mine"];
    const draft = state.drafts.find((t) => t.id === id);
    if (draft) return [draft, "draft"];
    throw error("E_THEME_INVALID", `there is no theme with the id ${id}`);
  };
  const summary = (t: Theme, source: ThemeSource): ThemeSummary => ({
    id: t.id,
    name: t.name,
    tone: t.tone,
    layout: t.prompt.layout,
    source,
    inUse: state.applied?.theme.id === t.id,
    hasDraft: state.drafts.some((d) => d.id === t.id),
  });
  const plan = (request: ApplyRequest, theme: Theme): ApplyPlan => {
    const steps: ApplyPlan["steps"] = [
      { kind: "installOhMyPosh", detail: "runtime\\omp-31.5.0" },
      { kind: "installClink", detail: "runtime\\clink-1.9.34" },
      { kind: "writeGenerated", detail: "theme.omp.json" },
      { kind: "writeGenerated", detail: "init.lua" },
      { kind: "writeGenerated", detail: "banner.lua" },
      { kind: "writeGenerated", detail: "clink_settings" },
    ];
    const summaryEn = ["Update the prompt and the welcome box"];
    const summaryTh = ["ปรับ prompt และกล่องต้อนรับตามธีม"];
    const warnings: ApplyPlan["warnings"] = [];
    if (options.noWindowsTerminal) {
      warnings.push({ kind: "noWindowsTerminal" });
    } else {
      steps.push({ kind: "windowsTerminal", detail: "settings.json" });
      summaryEn.push("Colors and font of cmd in Windows Terminal");
      summaryTh.push("สีและตัวอักษรของหน้าต่าง cmd ใน Windows Terminal");
      if (request.setWtDefaultProfile) {
        summaryEn.push("Open Windows Terminal with cmd");
        summaryTh.push("เปิด Windows Terminal มาเป็น cmd");
      }
    }
    if (request.replaceOtherClink && options.otherClink?.length) {
      summaryEn.push("Turn off the cmd add-on that was set up before (can be undone)");
      summaryTh.push("ปิดส่วนเสริมของ cmd ตัวเดิมที่ติดตั้งไว้ก่อน (ย้อนกลับได้)");
    }
    if (request.target === "all") {
      steps.push({ kind: "autoRun", detail: "" });
      summaryEn.push("Use this theme in every cmd window");
      summaryTh.push("ให้ cmd ทุกหน้าต่างใช้ธีมนี้");
    }
    steps.push({ kind: "saveState", detail: "state.json" });
    return {
      digest: `digest-${theme.id}-${request.target}-${request.setWtDefaultProfile}-${request.replaceOtherClink}-${options.noWindowsTerminal ? "nowt" : "wt"}`,
      themeName: theme.name,
      summaryEn,
      summaryTh,
      warnings,
      steps,
      notes: [],
      unrestored: [],
    };
  };
  const checks = (): CheckReport[] => {
    const ok = (id: string, en: string, th: string): CheckReport => ({
      id,
      status: "ok",
      code: null,
      messageTh: th,
      messageEn: en,
      details: "",
      detailsTh: null,
      fix: null,
    });
    const applied = state.applied !== null;
    const skip = (id: string): CheckReport => ({
      ...ok(id, "No theme has been applied yet", "ยังไม่ได้ใช้ธีม"),
      status: "skipped",
    });
    const list: CheckReport[] = [
      applied ? ok("D1", "Runtime files are complete", "ไฟล์ runtime ครบ") : skip("D1"),
      ok("D2", "Oh My Posh 31.5.0 runs", "Oh My Posh 31.5.0 รันได้"),
      applied
        ? {
            id: "D3",
            status: "warn",
            code: "E_THEME_NOT_LOADED",
            messageTh: "ยังไม่มีหน้าต่าง cmd ที่โหลดธีม",
            messageEn: "No cmd window has loaded the theme yet",
            details: "",
            detailsTh: null,
            fix: "openNewCmdWindow",
          }
        : skip("D3"),
      applied ? ok("D4", "AutoRun holds CMD Studio's entry first", "AutoRun มีส่วนของเราอยู่หน้าสุด") : skip("D4"),
      options.otherClink?.length
        ? {
            id: "D5",
            status: "warn",
            code: "E_AUTORUN_FOREIGN_CLINK",
            messageTh: "เจอ Clink ตัวอื่นใน AutoRun",
            messageEn: "Another Clink runs from AutoRun",
            details: options.otherClink.join("\n"),
            detailsTh: null,
            fix: applied ? "replaceOtherClink" : null,
          }
        : ok("D5", "No other Clink in AutoRun", "ไม่มี Clink ตัวอื่นใน AutoRun"),
      options.noWindowsTerminal
        ? {
            id: "D6",
            status: "warn",
            code: "E_WT_NOT_FOUND",
            messageTh: "ไม่พบ Windows Terminal",
            messageEn: "Windows Terminal was not found",
            details: "",
            detailsTh: null,
            fix: "installWindowsTerminal",
          }
        : ok("D6", "Windows Terminal is installed and its settings parse", "มี Windows Terminal และอ่าน settings.json ได้"),
      applied ? ok("D7", "The cmd profile matches the theme", "โปรไฟล์ cmd ตรงกับธีม") : skip("D7"),
      applied ? ok("D8", "The theme's font is installed", "ฟอนต์ของธีมติดตั้งอยู่") : skip("D8"),
      applied ? ok("D9", "The generated files are as written", "ไฟล์ที่สร้างตรงกับที่บันทึกไว้") : skip("D9"),
      ok("D10", "The app runs unpackaged", "แอปไม่ได้อยู่ในแพ็กเกจ"),
      ok("D11", "Windows Terminal is the default terminal", "แอปเทอร์มินัลเริ่มต้นคือ Windows Terminal"),
    ];
    return list;
  };

  const ipc: Ipc & { state: MockState } = {
    state,
    async systemDetect(): Promise<SystemReport> {
      state.calls.push("systemDetect");
      return {
        build: 26100,
        arch: "x64",
        packaged: false,
        unreadable: [],
        windowsTerminal: options.noWindowsTerminal
          ? [{ kind: "store", installed: false, hasSettings: false }]
          : [{ kind: "store", installed: true, hasSettings: true }],
        wtInstalled: !options.noWindowsTerminal,
        storeAvailable: true,
        autorun: {
          otherCommands: ['if exist "C:\\conda\\conda_hook.bat" "C:\\conda\\conda_hook.bat"'],
          foreignClink: options.otherClink ?? [],
          oursPresent: state.applied !== null && state.applied.request.target === "all",
          machineWideClink: false,
          notText: false,
        },
        fonts: { nerdFontInstalled: false, monospace: ["Cascadia Mono", "Consolas"] },
        dataDir: "C:\\Users\\you\\AppData\\Local\\ai.resincore.cmdstudio",
        versions: { app: "0.1.0", ohMyPosh: "31.5.0", clink: "1.9.34", nerdFonts: "3.5.1" },
      };
    },
    async themesList(): Promise<Library> {
      state.calls.push("themesList");
      const mine = [...state.mine].sort((a, b) => a.name.localeCompare(b.name));
      const known = new Set(all().map((t) => t.id));
      const drafts = state.drafts.filter((d) => !known.has(d.id));
      return {
        themes: [
          ...presets.map((t) => summary(t, "preset")),
          ...mine.map((t) => summary(t, "mine")),
          ...drafts.map((t) => summary(t, "draft")),
        ],
        applied: state.applied
          ? {
              themeId: state.applied.theme.id,
              themeName: state.applied.theme.name,
              target: state.applied.request.target,
              appliedAtMs: 1791694709000,
              snapshotId: state.applied.snapshotId,
            }
          : null,
        problems: [],
        stateProblem: null,
      };
    },
    async themeGet(id): Promise<ThemeDetail> {
      const [theme, source] = find(id);
      const draft = source === "draft" ? null : (state.drafts.find((d) => d.id === id) ?? null);
      return { theme: structuredClone(theme), source, draft: draft ? structuredClone(draft) : null };
    },
    async themeSave(theme, draft): Promise<ThemeSummary> {
      state.calls.push(draft ? "themeSave:draft" : "themeSave");
      if (presets.some((p) => p.id === theme.id)) {
        throw error("E_THEME_INVALID", `"${theme.name}" is a built-in preset`);
      }
      if (theme.name.trim().length === 0) throw error("E_THEME_INVALID", "invalid name: must be 1–64 characters");
      const copy = structuredClone(theme);
      if (draft) {
        state.drafts = [...state.drafts.filter((d) => d.id !== theme.id), copy];
        return summary(copy, state.mine.some((t) => t.id === theme.id) ? "mine" : "draft");
      }
      const i = state.mine.findIndex((t) => t.id === theme.id);
      if (i >= 0) state.mine[i] = copy;
      else state.mine.push(copy);
      state.drafts = state.drafts.filter((d) => d.id !== theme.id);
      return summary(copy, "mine");
    },
    async themeDuplicate(id, name): Promise<ThemeSummary> {
      const [theme] = find(id);
      const copy = structuredClone(theme);
      copy.id = uuid();
      copy.name = name;
      return ipc.themeSave(copy, false);
    },
    async themeDelete(id, draftOnly): Promise<void> {
      state.drafts = state.drafts.filter((d) => d.id !== id);
      if (draftOnly) return;
      const [theme, source] = find(id);
      if (source === "preset") throw error("E_THEME_INVALID", `"${theme.name}" is a built-in preset`);
      state.mine = state.mine.filter((t) => t.id !== id);
    },
    async themeImportPreview(): Promise<ImportPreview | null> {
      state.calls.push("themeImportPreview");
      const theme = structuredClone(byTone("dark"));
      theme.name = "Imported theme";
      return { token: "mock-token", theme, idTaken: true, nameTaken: false };
    },
    async themeImportConfirm(token): Promise<ThemeSummary> {
      if (token !== "mock-token") throw error("E_THEME_INVALID", "the import preview is no longer there");
      const theme = structuredClone(byTone("dark"));
      theme.id = uuid();
      theme.name = "Imported theme";
      return ipc.themeSave(theme, false);
    },
    async themeExport(id): Promise<string | null> {
      const [theme] = find(id);
      state.calls.push("themeExport");
      return `C:\\Users\\you\\Desktop\\${theme.name}.cmdtheme`;
    },
    // Fixed data, not generated: the dark preset's palette with the accent put in; the light
    // preset's for a light tone. The real answer is palette::generate in Rust.
    async paletteGenerate(tone, accent): Promise<Colors> {
      if (!/^#[0-9a-fA-F]{6}$/.test(accent)) throw error("E_THEME_INVALID", `accent: not a #rrggbb color: "${accent}"`);
      const colors = structuredClone(byTone(tone).colors);
      colors.accent = accent;
      colors.cursor = accent;
      return colors;
    },
    async contrastCheck(colors): Promise<ContrastIssue[]> {
      // The only check the mock knows: the same text and background.
      const issues: ContrastIssue[] = [];
      if (colors.foreground === colors.background) issues.push({ slot: "foreground", ratio: 1, required: 4.5 });
      if (colors.muted === colors.background) issues.push({ slot: "muted", ratio: 1, required: 3 });
      return issues;
    },
    async contrastFix(colors): Promise<Colors> {
      const fixed = structuredClone(colors);
      const other = byTone(colors.background === "#ffffff" ? "light" : "dark").colors;
      if (fixed.foreground === fixed.background) fixed.foreground = other.foreground;
      if (fixed.muted === fixed.background) fixed.muted = other.muted;
      return fixed;
    },
    async previewRender(theme): Promise<PreviewResult> {
      return fakeLines(theme);
    },
    async planApply(request): Promise<PlanOutcome> {
      state.calls.push("planApply");
      const [theme] = find(request.themeId);
      if (options.otherClink?.length && !request.replaceOtherClink) {
        return { kind: "otherClink", commands: options.otherClink };
      }
      return { kind: "ready", plan: plan(request, theme) };
    },
    async apply(request, digest, onProgress): Promise<ApplyOutcome> {
      state.calls.push("apply");
      const [theme] = find(request.themeId);
      const expected = plan(request, theme);
      if (expected.digest !== digest) {
        return { status: "planChanged", themeName: theme.name, snapshotId: null, error: null, unrestored: [], notes: [], plan: expected };
      }
      const total = expected.steps.length;
      const tell = (progress: ApplyProgress) => onProgress(progress);
      for (let step = 1; step <= total; step++) tell({ phase: "backingUp", step, total, kind: null });
      for (let step = 1; step <= total; step++) tell({ phase: "applying", step, total, kind: expected.steps[step - 1]?.kind ?? null });
      tell({ phase: "verifying", step: 0, total: 0, kind: null });
      const snapshotId = `${now()}-${uuid().slice(0, 8)}`;
      if (options.failApplyWith) {
        for (let step = 1; step <= total; step++) tell({ phase: "rollingBack", step, total, kind: null });
        const appError: AppError = {
          code: options.failApplyWith,
          messageTh: `(${options.failApplyWith})`,
          messageEn: `(${options.failApplyWith})`,
          details: "the mock was told to fail",
        };
        return { status: "rolledBack", themeName: theme.name, snapshotId, error: appError, unrestored: [], notes: [], plan: null };
      }
      state.points.unshift({
        id: snapshotId,
        kind: "apply",
        createdMs: Number(snapshotId.split("-")[0]),
        theme: { id: theme.id, name: theme.name },
        target: request.target,
        summary: expected.summaryEn,
        undoes: null,
        undoneBy: null,
        restorable: true,
      });
      state.applied = { theme: structuredClone(theme), request, snapshotId };
      return { status: "committed", themeName: theme.name, snapshotId, error: null, unrestored: [], notes: [], plan: null };
    },
    async undoLast(): Promise<UndoOutcome> {
      state.calls.push("undoLast");
      const target = state.points.find((p) => p.kind === "apply" && p.undoneBy === null);
      if (!target) {
        return { status: "nothingToUndo", undid: null, error: null, unrestored: [], notes: [], fontsLeft: [] };
      }
      const id = `${now()}-${uuid().slice(0, 8)}`;
      target.undoneBy = id;
      target.restorable = false;
      state.points.unshift({ ...target, id, kind: "undo", undoes: target.id, undoneBy: null, restorable: false, summary: [`Undo ${target.id}`] });
      const previous = state.points.find((p) => p.kind === "apply" && p.undoneBy === null);
      state.applied = previous && previous.theme ? { theme: structuredClone(find(previous.theme.id)[0]), request: { themeId: previous.theme.id, target: previous.target ?? "all", setWtDefaultProfile: true, replaceOtherClink: false }, snapshotId: previous.id } : null;
      return { status: "committed", undid: target.id, error: null, unrestored: [], notes: [], fontsLeft: [] };
    },
    async snapshotsList(): Promise<RestorePoints> {
      return { points: structuredClone(state.points), problems: [] };
    },
    async snapshotRestore(id): Promise<RestoreOutcome> {
      const point = state.points.find((p) => p.id === id);
      if (!point) throw error("E_SNAPSHOT_READ", `there is no restore point ${id}`);
      if (!point.restorable) return { status: "notRestorable", undone: [], last: null };
      const undone: string[] = [];
      let last: UndoOutcome | null = null;
      while (state.points.find((p) => p.id === id)?.undoneBy === null) {
        last = await ipc.undoLast();
        if (last.undid) undone.push(last.undid);
        if (last.status !== "committed") return { status: "stopped", undone, last };
      }
      return { status: "done", undone, last };
    },
    async revertAll(): Promise<RevertOutcome> {
      state.calls.push("revertAll");
      const id = `${now()}-${uuid().slice(0, 8)}`;
      for (const p of state.points) if (p.kind === "apply" && p.undoneBy === null) { p.undoneBy = id; p.restorable = false; }
      state.points.unshift({ id, kind: "revertAll", createdMs: Number(id.split("-")[0]), theme: null, target: null, summary: ["Revert everything"], undoes: null, undoneBy: null, restorable: false });
      state.applied = null;
      return {
        ok: true,
        steps: [
          { what: "AutoRun and Windows Terminal's settings", ok: true, detail: "CMD Studio's part is taken out", code: null, fixEn: null, fixTh: null },
          { what: "Fonts", ok: true, detail: "nothing to uninstall", code: null, fixEn: null, fixTh: null },
        ],
        notes: [],
        status: "committed",
        error: null,
        unrestored: [],
      };
    },
    async doctorRun(): Promise<DoctorReport> {
      state.calls.push("doctorRun");
      return { checks: checks(), checkedAtMs: now() };
    },
    async doctorFix(checkId): Promise<FixProposal> {
      const check = checks().find((c) => c.id === checkId);
      if (!check) throw error("E_INTERNAL", `there is no check ${checkId}`);
      const kind = check.fix;
      if ((kind === "reapply" || kind === "replaceOtherClink") && state.applied) {
        const request: ApplyRequest = { ...state.applied.request, replaceOtherClink: kind === "replaceOtherClink" };
        return { kind, request, plan: await ipc.planApply(request) };
      }
      return { kind, request: null, plan: null };
    },
    async settingsGet(): Promise<SettingsLoad> {
      return { settings: { ...state.settings }, resetFrom: null };
    },
    async settingsSet(settings): Promise<AppSettings> {
      state.calls.push("settingsSet");
      state.settings = { ...settings };
      return { ...settings };
    },
    async openCmd(): Promise<void> {
      state.calls.push("openCmd");
    },
    async openExternal(target): Promise<void> {
      state.calls.push(`openExternal:${target}`);
    },
    async diagnosticsCopy(): Promise<string> {
      state.calls.push("diagnosticsCopy");
      return "CMD Studio diagnostics\napp 0.1.0 · Oh My Posh 31.5.0 · Clink 1.9.34 · Nerd Fonts 3.5.1\n(mock)";
    },
    async advancedRead(): Promise<AdvancedInfo> {
      return {
        dataDir: "C:\\Users\\you\\AppData\\Local\\ai.resincore.cmdstudio",
        logTail: "2026-10-16T12:00:00Z cmd=themes_list ms=3 ok",
        generated: state.applied
          ? [
              { name: "theme.omp.json", text: '{\n  "version": 4\n}' },
              { name: "init.lua", text: "-- init.lua" },
              { name: "banner.lua", text: "-- banner.lua" },
              { name: "clink_settings", text: "clink.logo = none" },
            ]
          : [],
        versions: { app: "0.1.0", ohMyPosh: "31.5.0", clink: "1.9.34", nerdFonts: "3.5.1" },
      };
    },
  };
  return ipc;
}
```

- [ ] **Step 7: Run the tests**

Run: `npm test`
Expected: `Test Files 2 passed`, `Tests 9 passed` (7 + 2). `npm run typecheck` clean — in particular `tauriIpc` and the mock both satisfy `Ipc`, so a command missing on either side is a type error.

- [ ] **Step 8: Commit**

Stage `app/ui/src/ipc`. Message: `ui: the Ipc contract, Tauri's invoke/listen implementation, and the in-memory mock for tests`.

---

### Task 4: Routing, app state, and the shared components

The plumbing and the pieces every screen uses. `TerminalPreview` paints the core's spans (`fg`, `bg`, `bold`) in a `<pre role="img">` with the theme's background, font and opacity. `Notices` shows an `AppError` in the UI's language with the code and a "Details" toggle (spec §5.9), the start-of-command notes, and the steps a rollback could not put back with their fix. `Dialog` is a modal that keeps focus inside, closes on Escape and returns focus. `ThemeCard` shows the name, the badges and the eight ANSI swatches. `ApplyFlow` is the apply conversation of spec §2.1/§4.4 (index Decision 3): plan → `PlanView` ("What will change", warnings, the steps behind a toggle, notes) → Apply → `ProgressView` (the phase and `step/total`) → the outcome (done, or put back with the error and "Copy details"), with the other-Clink question and the `planChanged` notice.

**Files:**
- Create: `app/ui/src/router.tsx`, `app/ui/src/state.tsx`, `app/ui/src/components/TerminalPreview.tsx`, `app/ui/src/components/Notices.tsx`, `app/ui/src/components/Dialog.tsx`, `app/ui/src/components/ThemeCard.tsx`, `app/ui/src/components/ApplyFlow.tsx`
- Test: `npm run typecheck` here; Task 6's `Themes.test.tsx` exercises every component through the screens (the plan dialog, the progress, the outcome, the other-Clink question, the failed apply, the cards and the preview).

**Interfaces:**
- Consumes: `i18n` (Task 2), `ipc/context` and `ipc/types` (Task 3), the bindings.
- Produces:
  ```ts
  // router.tsx
  export type HelpTab = "doctor" | "restore" | "revert" | "report" | "guide" | "credits";
  export type Route = { name: "wizard" } | { name: "home" } | { name: "themes" } | { name: "editor"; id: string } | { name: "settings" } | { name: "help"; tab: HelpTab } | { name: "advanced" };
  export interface Nav { route: Route; go: (route: Route) => void }   export const NavContext; export function useNav(): Nav;
  // state.tsx
  export interface AppState { settings: AppSettings; saveSettings: (s: AppSettings) => Promise<void>; lastApply: ApplyOutcome | null; setLastApply: (o: ApplyOutcome | null) => void; version: number; bump: () => void }
  export const AppStateContext; export function useAppState(): AppState;
  export const DEFAULT_SETTINGS: AppSettings;   // en, all, true, "auto", false
  // components
  export function TerminalPreview(props: { theme: Theme; result: PreviewResult | null; compact?: boolean }): JSX.Element;
  export function ErrorNotice(props: { error: AppError | unknown; title?: string }): JSX.Element;
  export function Notice(props: { kind?: "info" | "ok" | "warn"; children: ReactNode }): JSX.Element;
  export function NotesList(props: { notes: Note[] }): JSX.Element | null;   export function UnrestoredList(props: { steps: Unrestored[] }): JSX.Element | null;
  export function Dialog(props: { title: string; onClose?: () => void; children: ReactNode }): JSX.Element;
  export function ThemeCard(props: { summary: ThemeSummary; theme?: Theme | null; selected?: boolean; onSelect?: () => void; children?: ReactNode }): JSX.Element;
  export function PlanView(props: { plan: ApplyPlan; titled?: boolean }): JSX.Element;   // titled=false inside a Dialog whose title is already "What will change"
  export function ProgressView(props: { progress: ApplyProgress | null }): JSX.Element;
  export function ApplyFlow(props: { request: ApplyRequest; titled?: boolean; onApplied?: (o: ApplyOutcome) => void; onCancel?: () => void; showBackup?: boolean }): JSX.Element;
  ```

- [ ] **Step 1: Write `app/ui/src/router.tsx`**

```tsx
// A tiny in-house router (the brief: no routing library): the route is React state in `App`,
// screens read it and move with `useNav()`. On every change the main heading takes focus, so a
// keyboard or screen-reader user lands on the new screen's title (spec §4.5).
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
```

- [ ] **Step 2: Write `app/ui/src/state.tsx`**

```tsx
// App-wide state the screens share: the settings (language, where themes go), the last apply
// (for the after-apply bar, spec §4.4) and a counter that tells screens the library or the
// machine changed, so they reload.
import { createContext, useContext } from "react";
import type { AppSettings } from "./bindings/AppSettings";
import type { ApplyOutcome } from "./bindings/ApplyOutcome";

export interface AppState {
  settings: AppSettings;
  /** Saves and remembers the settings; `language` is applied to the UI at once. */
  saveSettings: (settings: AppSettings) => Promise<void>;
  lastApply: ApplyOutcome | null;
  setLastApply: (outcome: ApplyOutcome | null) => void;
  /** Bumped after an apply, undo, restore or revert: screens that show the machine reload. */
  version: number;
  bump: () => void;
}

export const AppStateContext = createContext<AppState | null>(null);

export function useAppState(): AppState {
  const state = useContext(AppStateContext);
  if (!state) throw new Error("useAppState needs an AppStateContext");
  return state;
}

export const DEFAULT_SETTINGS: AppSettings = {
  language: "en",
  target: "all",
  setWtDefaultProfile: true,
  updates: "auto",
  wizardDone: false,
};
```

- [ ] **Step 3: Write `app/ui/src/components/TerminalPreview.tsx`**

```tsx
// Paints the spans the core drew (spec §5.6): a mock cmd window whose background, font and
// opacity come from the theme. Nothing here decides a color: `fg`/`bg` are the core's.
import type { PreviewResult } from "../bindings/PreviewResult";
import type { Theme } from "../bindings/Theme";
import type { Line } from "../bindings/Line";
import { useT } from "../i18n";

function Row({ line }: { line: Line }) {
  return (
    <>
      {line.map((span, i) => (
        <span
          key={i}
          style={{
            color: span.fg,
            backgroundColor: span.bg ?? undefined,
            fontWeight: span.bold ? 700 : 400,
          }}
        >
          {span.text}
        </span>
      ))}
      {"\n"}
    </>
  );
}

export function TerminalPreview({
  theme,
  result,
  compact = false,
}: {
  theme: Theme;
  result: PreviewResult | null;
  /** Only the prompt rows (a theme card). */
  compact?: boolean;
}) {
  const t = useT();
  const { colors, terminal } = theme;
  const lines: Line[] = result ? (compact ? result.prompt : [...result.banner, ...result.prompt]) : [];
  return (
    <pre
      className="terminal"
      role="img"
      aria-label={t("a11y.terminalPreview")}
      style={{
        backgroundColor: colors.background,
        color: colors.foreground,
        fontFamily: `"${terminal.font}", "Cascadia Mono", Consolas, monospace`,
        opacity: terminal.opacity / 100,
        padding: `${terminal.padding[1] ?? 10}px ${terminal.padding[0] ?? 14}px`,
      }}
    >
      {lines.map((line, i) => (
        <Row key={i} line={line} />
      ))}
    </pre>
  );
}
```

- [ ] **Step 4: Write `app/ui/src/components/Notices.tsx`**

```tsx
// What the user reads when something is wrong or worth knowing (spec §5.9: the problem in
// their language, the code, and the technical detail behind a "Details" toggle).
import type { ReactNode } from "react";
import type { AppError } from "../bindings/AppError";
import type { Note } from "../bindings/Note";
import type { Unrestored } from "../bindings/Unrestored";
import { pick, useI18n } from "../i18n";
import { toIpcError } from "../ipc/types";

export function ErrorNotice({ error, title }: { error: AppError | unknown; title?: string }) {
  const { language, t } = useI18n();
  const e = isAppError(error) ? error : toIpcError(error).toAppError();
  return (
    <div className="notice error" role="alert">
      <strong>{title ?? t("error.title")}</strong>
      <p>{pick(language, e.messageTh, e.messageEn)}</p>
      <details>
        <summary>
          {t("common.details")} · {t("error.code", { code: e.code })}
        </summary>
        <pre>{e.details}</pre>
      </details>
    </div>
  );
}

function isAppError(value: unknown): value is AppError {
  if (typeof value !== "object" || value === null) return false;
  const v = value as Record<string, unknown>;
  return typeof v.code === "string" && typeof v.messageEn === "string";
}

export function Notice({ kind = "info", children }: { kind?: "info" | "ok" | "warn"; children: ReactNode }) {
  return (
    <div className={`notice ${kind === "info" ? "" : kind}`} role={kind === "warn" ? "alert" : "status"}>
      {children}
    </div>
  );
}

/** Notes from the start of a command (spec §5.5), in the UI's language. */
export function NotesList({ notes }: { notes: Note[] }) {
  const { language, t } = useI18n();
  if (notes.length === 0) return null;
  return (
    <div className="notice warn">
      <strong>{t("apply.notesTitle")}</strong>
      <ul className="plain-list">
        {notes.map((n, i) => (
          <li key={i}>
            {pick(language, n.messageTh, n.messageEn)} <span className="badge">{n.code}</span>
          </li>
        ))}
      </ul>
    </div>
  );
}

/** The steps a rollback could not put back, each with how to fix it by hand (spec §5.5). */
export function UnrestoredList({ steps }: { steps: Unrestored[] }) {
  const { language, t } = useI18n();
  if (steps.length === 0) return null;
  return (
    <div className="notice error" role="alert">
      <strong>{t("apply.unrestoredTitle")}</strong>
      <ul className="plain-list">
        {steps.map((s, i) => (
          <li key={i}>
            <div>
              {s.target} <span className="badge">{s.code}</span>
            </div>
            <div>{pick(language, s.fixTh, s.fix)}</div>
          </li>
        ))}
      </ul>
    </div>
  );
}
```

- [ ] **Step 5: Write `app/ui/src/components/Dialog.tsx`**

```tsx
// A modal that keeps focus inside, closes on Escape, and names itself for screen readers.
import { useEffect, useId, useRef, type ReactNode } from "react";
import { useT } from "../i18n";

export function Dialog({
  title,
  onClose,
  children,
}: {
  title: string;
  onClose?: () => void;
  children: ReactNode;
}) {
  const t = useT();
  const titleId = useId();
  const box = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null;
    box.current?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && onClose) onClose();
      if (e.key === "Tab" && box.current) {
        const focusable = box.current.querySelectorAll<HTMLElement>(
          'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])',
        );
        const first = focusable[0];
        const last = focusable[focusable.length - 1];
        if (!first || !last) return;
        if (e.shiftKey && document.activeElement === first) {
          e.preventDefault();
          last.focus();
        } else if (!e.shiftKey && document.activeElement === last) {
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
  }, [onClose]);

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
```

- [ ] **Step 6: Write `app/ui/src/components/ThemeCard.tsx`**

```tsx
import type { ReactNode } from "react";
import type { ThemeSummary } from "../bindings/ThemeSummary";
import type { Theme } from "../bindings/Theme";
import { useT } from "../i18n";

/** A theme in the library or the wizard: name, badges, its 16 colors as swatches. */
export function ThemeCard({
  summary,
  theme,
  selected = false,
  onSelect,
  children,
}: {
  summary: ThemeSummary;
  /** The full theme, when known, for the swatches. */
  theme?: Theme | null;
  selected?: boolean;
  onSelect?: () => void;
  children?: ReactNode;
}) {
  const t = useT();
  const colors = theme?.colors;
  return (
    <div className={`theme-card${selected ? " selected" : ""}`}>
      {onSelect ? (
        <button type="button" onClick={onSelect} aria-pressed={selected} style={{ width: "100%", textAlign: "left" }}>
          <strong>{summary.name}</strong>
        </button>
      ) : (
        <strong>{summary.name}</strong>
      )}
      {summary.inUse && <span className="badge in-use">{t("themes.inUse")}</span>}
      {summary.source === "preset" && <span className="badge">{t("themes.preset")}</span>}
      {summary.source === "draft" && <span className="badge">{t("themes.draft")}</span>}
      <div className="badge">{t(summary.tone === "dark" ? "themes.filter.dark" : "themes.filter.light")}</div>
      {colors && (
        <div className="swatches" aria-hidden="true" style={{ background: colors.background, padding: "0.3rem", borderRadius: "0.3rem" }}>
          {colors.ansi.slice(0, 8).map((c, i) => (
            <span key={i} style={{ background: c }} />
          ))}
        </div>
      )}
      {children}
    </div>
  );
}
```

- [ ] **Step 7: Write `app/ui/src/components/ApplyFlow.tsx`**

```tsx
// The one way a theme reaches the machine (spec §2.1, §4.2 page 5, §4.4): plan → show what
// will change → the user presses Apply → progress step by step → the outcome. Used by the
// wizard, the editor's "Save and apply", the home page's quick switch, the Settings page and
// the doctor's fixes. A plan that changed while the user read it is shown again, never applied.
import { useCallback, useEffect, useState } from "react";
import type { ApplyOutcome } from "../bindings/ApplyOutcome";
import type { ApplyPlan } from "../bindings/ApplyPlan";
import type { ApplyProgress } from "../bindings/ApplyProgress";
import type { ApplyRequest } from "../bindings/ApplyRequest";
import type { PlanWarning } from "../bindings/PlanWarning";
import type { StepKind } from "../bindings/StepKind";
import { useI18n, type Key } from "../i18n";
import { useIpc } from "../ipc/context";
import { IpcError, toIpcError } from "../ipc/types";
import { ErrorNotice, NotesList, UnrestoredList } from "./Notices";

const STEP_KEY: Record<StepKind, Key> = {
  installOhMyPosh: "apply.step.installOhMyPosh",
  installClink: "apply.step.installClink",
  writeGenerated: "apply.step.writeGenerated",
  windowsTerminal: "apply.step.windowsTerminal",
  autoRun: "apply.step.autoRun",
  installFont: "apply.step.installFont",
  saveState: "apply.step.saveState",
};

function warningText(w: PlanWarning, t: (k: Key, v?: Record<string, string>) => string): string {
  switch (w.kind) {
    case "noWindowsTerminal":
      return t("apply.warning.noWindowsTerminal");
    case "machineWideClink":
      return t("apply.warning.machineWideClink");
    case "fontFallback":
      return t("apply.warning.fontFallback", { wanted: w.wanted });
  }
}

export function PlanView({ plan, titled = true }: { plan: ApplyPlan; titled?: boolean }) {
  const { language, t } = useI18n();
  const summary = language === "th" ? plan.summaryTh : plan.summaryEn;
  return (
    <div>
      {titled && <h3>{t("apply.planTitle")}</h3>}
      <ul className="check-list">
        {summary.map((line, i) => (
          <li key={i}>{line}</li>
        ))}
      </ul>
      {plan.warnings.length > 0 && (
        <>
          <h3>{t("apply.warnings")}</h3>
          <ul className="check-list">
            {plan.warnings.map((w, i) => (
              <li key={i}>{warningText(w, t)}</li>
            ))}
          </ul>
        </>
      )}
      <details>
        <summary>
          {t("apply.steps")} ({plan.steps.length})
        </summary>
        <ol>
          {plan.steps.map((s, i) => (
            <li key={i}>{t(STEP_KEY[s.kind], { detail: s.detail })}</li>
          ))}
        </ol>
      </details>
      <NotesList notes={plan.notes} />
      <UnrestoredList steps={plan.unrestored} />
    </div>
  );
}

export function ProgressView({ progress }: { progress: ApplyProgress | null }) {
  const t = useI18n().t;
  if (!progress) return <p role="status">{t("apply.working")}</p>;
  const text =
    progress.phase === "verifying"
      ? t("apply.phase.verifying")
      : t(`apply.phase.${progress.phase}`, { step: progress.step, total: progress.total });
  const value = progress.phase === "verifying" ? progress.total || 1 : progress.step;
  return (
    <div role="status">
      <p>{text}</p>
      <progress max={Math.max(progress.total, 1)} value={progress.phase === "verifying" ? Math.max(progress.total, 1) : value} />
    </div>
  );
}

type Stage =
  | { kind: "planning" }
  | { kind: "otherClink"; commands: string[] }
  | { kind: "ready"; plan: ApplyPlan; changed: boolean }
  | { kind: "applying"; progress: ApplyProgress | null }
  | { kind: "done"; outcome: ApplyOutcome }
  | { kind: "error"; error: IpcError };

export function ApplyFlow({
  request: initial,
  onApplied,
  onCancel,
  showBackup = false,
  titled = true,
}: {
  request: ApplyRequest;
  /** Whether the plan carries its own "What will change" heading (a dialog has it as its title). */
  titled?: boolean;
  /** Called once an apply committed (the after-apply bar and the caller's refresh). */
  onApplied?: (outcome: ApplyOutcome) => void;
  onCancel?: () => void;
  /** The wizard's "we save a restore point first" line. */
  showBackup?: boolean;
}) {
  const ipc = useIpc();
  const { t } = useI18n();
  const [request, setRequest] = useState(initial);
  const [stage, setStage] = useState<Stage>({ kind: "planning" });

  const plan = useCallback(
    async (req: ApplyRequest) => {
      setStage({ kind: "planning" });
      try {
        const outcome = await ipc.planApply(req);
        if (outcome.kind === "otherClink") setStage({ kind: "otherClink", commands: outcome.commands });
        else setStage({ kind: "ready", plan: outcome.plan, changed: false });
      } catch (thrown) {
        setStage({ kind: "error", error: toIpcError(thrown) });
      }
    },
    [ipc],
  );

  useEffect(() => {
    void plan(request);
  }, [plan, request]);

  const replace = () => setRequest({ ...request, replaceOtherClink: true });

  const run = async (current: ApplyPlan) => {
    setStage({ kind: "applying", progress: null });
    try {
      const outcome = await ipc.apply(request, current.digest, (progress) =>
        setStage({ kind: "applying", progress }),
      );
      if (outcome.status === "planChanged" && outcome.plan) {
        setStage({ kind: "ready", plan: outcome.plan, changed: true });
        return;
      }
      setStage({ kind: "done", outcome });
      if (outcome.status === "committed") onApplied?.(outcome);
    } catch (thrown) {
      setStage({ kind: "error", error: toIpcError(thrown) });
    }
  };

  const copyDiagnostics = async () => {
    try {
      const text = await ipc.diagnosticsCopy();
      await navigator.clipboard.writeText(text);
    } catch {
      // The report page offers the same; a failed copy here is not an error to the user.
    }
  };

  switch (stage.kind) {
    case "planning":
      return <p role="status">{t("common.loading")}</p>;
    case "otherClink":
      return (
        <div>
          <h3>{t("apply.otherClink.title")}</h3>
          <p>{t("apply.otherClink.text")}</p>
          <ul className="check-list">
            {stage.commands.map((c, i) => (
              <li key={i}>
                <code>{c}</code>
              </li>
            ))}
          </ul>
          <div className="row-actions">
            <button type="button" className="primary" onClick={replace}>
              {t("apply.otherClink.replace")}
            </button>
            {onCancel && (
              <button type="button" onClick={onCancel}>
                {t("common.cancel")}
              </button>
            )}
          </div>
        </div>
      );
    case "ready":
      return (
        <div>
          {stage.changed && (
            <div className="notice warn" role="alert">
              {t("apply.planChanged")}
            </div>
          )}
          <PlanView plan={stage.plan} titled={titled} />
          {showBackup && <p>{t("wizard.confirm.backup")}</p>}
          <div className="row-actions">
            <button type="button" className="primary" onClick={() => void run(stage.plan)}>
              {t("apply.start")}
            </button>
            {onCancel && (
              <button type="button" onClick={onCancel}>
                {t("common.cancel")}
              </button>
            )}
          </div>
        </div>
      );
    case "applying":
      return <ProgressView progress={stage.progress} />;
    case "done": {
      const o = stage.outcome;
      if (o.status === "committed") {
        return (
          <div className="notice ok" role="status">
            {t(request.target === "wt" ? "apply.doneWt" : "apply.done", { name: o.themeName })}
            <NotesList notes={o.notes} />
          </div>
        );
      }
      return (
        <div>
          <div className="notice error" role="alert">
            {o.status === "rollbackIncomplete" ? t("apply.failedIncomplete") : t("apply.failed")}
          </div>
          {o.error && <ErrorNotice error={o.error} />}
          <UnrestoredList steps={o.unrestored} />
          <NotesList notes={o.notes} />
          <div className="row-actions">
            <button type="button" onClick={() => void copyDiagnostics()}>
              {t("apply.copyDiagnostics")}
            </button>
            <button type="button" onClick={() => void plan(request)}>
              {t("common.retry")}
            </button>
          </div>
        </div>
      );
    }
    case "error":
      return (
        <div>
          <ErrorNotice error={stage.error.toAppError()} />
          <div className="row-actions">
            <button type="button" onClick={() => void plan(request)}>
              {t("common.retry")}
            </button>
            {onCancel && (
              <button type="button" onClick={onCancel}>
                {t("common.cancel")}
              </button>
            )}
          </div>
        </div>
      );
  }
}
```

- [ ] **Step 8: Type-check and test**

Run: `npm run typecheck` → clean (every `t("…")` key exists: a typo is a type error). `npm test` → still `9 passed`.

- [ ] **Step 9: Commit**

Stage `app/ui/src/router.tsx app/ui/src/state.tsx app/ui/src/components`. Message: `ui: router, app state, and the shared components (preview painter, notices, dialog, theme card, apply flow)`.

---

### Task 5: The editor's reducer

Spec §3.4 as a pure function: the theme being edited, 100 steps of undo and redo, the mode, and the rule that ties them — any color edited by hand makes `simple` null; going back to simple mode keeps those edits, with a notice, until the tone, the main color or the tint change, which regenerates every color (through `palette_generate`, in Rust — the reducer only stores the colors it is given).

**Files:**
- Create: `app/ui/src/screens/editor/reducer.ts`, `app/ui/src/screens/editor/reducer.test.ts`
- Test: `reducer.test.ts` (5 tests)

**Interfaces:**
- Produces:
  ```ts
  export const HISTORY = 100;   export type Mode = "simple" | "advanced";
  export interface EditorState { theme: Theme; past: Theme[]; future: Theme[]; mode: Mode; keptEditsNotice: boolean; dirty: boolean }
  export type ColorSlot = Exclude<keyof Colors, "ansi"> | `ansi${number}`;
  export type EditorAction = { type: "load"; theme: Theme; mode?: Mode } | { type: "edit"; theme: Theme } | { type: "setColor"; slot: ColorSlot; value: string } | { type: "setColors"; colors: Colors; simple: SimpleRecipe | null } | { type: "setMode"; mode: Mode } | { type: "undo" } | { type: "redo" } | { type: "saved" };
  export function initialState(theme: Theme): EditorState;   // mode from theme.simple
  export function setSlot(colors: Colors, slot: ColorSlot, value: string): Colors;   export function getSlot(colors: Colors, slot: ColorSlot): string;
  export function reduce(state: EditorState, action: EditorAction): EditorState;
  ```

- [ ] **Step 1: Write the failing test**

```ts
import { describe, expect, it } from "vitest";
import { PRESETS } from "../../ipc/presets";
import { HISTORY, initialState, reduce, type EditorState } from "./reducer";

const terracotta = () => structuredClone(PRESETS.find((p) => p.name === "Terracotta")!);

describe("the editor's history", () => {
  it("undoes and redoes, and keeps at most 100 steps", () => {
    let s = initialState(terracotta());
    expect(s.mode).toBe("advanced"); // Terracotta's simple is null (hand-tuned)
    for (let i = 0; i < HISTORY + 20; i++) {
      s = reduce(s, { type: "edit", theme: { ...s.theme, name: `Name ${i}` } });
    }
    expect(s.past.length).toBe(HISTORY);
    expect(s.theme.name).toBe(`Name ${HISTORY + 19}`);
    for (let i = 0; i < HISTORY; i++) s = reduce(s, { type: "undo" });
    expect(s.theme.name).toBe("Name 19");
    expect(s.past.length).toBe(0);
    // One more undo does nothing.
    expect(reduce(s, { type: "undo" })).toBe(s);
    s = reduce(s, { type: "redo" });
    expect(s.theme.name).toBe("Name 20");
    // A new edit clears the redo stack.
    s = reduce(s, { type: "edit", theme: { ...s.theme, name: "fresh" } });
    expect(s.future).toEqual([]);
    expect(reduce(s, { type: "redo" })).toBe(s);
    expect(s.dirty).toBe(true);
    expect(reduce(s, { type: "saved" }).dirty).toBe(false);
  });
});

describe("simple and advanced mode (spec §3.4)", () => {
  const simpleTheme = () => ({ ...terracotta(), simple: { accent: "#d97757", tint: 0.1 } });

  it("opens in simple mode when the theme has a recipe", () => {
    expect(initialState(simpleTheme()).mode).toBe("simple");
  });

  it("makes simple null as soon as one color is edited by hand", () => {
    let s: EditorState = initialState(simpleTheme());
    s = reduce(s, { type: "setMode", mode: "advanced" });
    s = reduce(s, { type: "setColor", slot: "ansi3", value: "#ffff00" });
    expect(s.theme.simple).toBeNull();
    expect(s.theme.colors.ansi[3]).toBe("#ffff00");
    s = reduce(s, { type: "setColor", slot: "frame", value: "#123456" });
    expect(s.theme.colors.frame).toBe("#123456");
  });

  it("keeps the hand edits when going back to simple mode, with a notice, until a regenerate", () => {
    let s: EditorState = initialState(simpleTheme());
    s = reduce(s, { type: "setMode", mode: "advanced" });
    s = reduce(s, { type: "setColor", slot: "muted", value: "#777777" });
    s = reduce(s, { type: "setMode", mode: "simple" });
    expect(s.keptEditsNotice).toBe(true);
    expect(s.theme.colors.muted).toBe("#777777");
    // Tone, accent or tint changed: the Rust palette comes back and replaces the edits.
    const regenerated = { ...s.theme.colors, muted: "#8a8780" };
    s = reduce(s, { type: "setColors", colors: regenerated, simple: { accent: "#00ff00", tint: 0.2 } });
    expect(s.keptEditsNotice).toBe(false);
    expect(s.theme.simple).toEqual({ accent: "#00ff00", tint: 0.2 });
    expect(s.theme.colors.muted).toBe("#8a8780");
    // A readability fix is a hand edit: simple goes null again, the notice is not shown
    // until the mode is switched again.
    s = reduce(s, { type: "setColors", colors: regenerated, simple: null });
    expect(s.theme.simple).toBeNull();
    expect(s.keptEditsNotice).toBe(false);
  });

  it("does not show the notice when the theme still has its recipe", () => {
    let s: EditorState = initialState(simpleTheme());
    s = reduce(s, { type: "setMode", mode: "advanced" });
    s = reduce(s, { type: "setMode", mode: "simple" });
    expect(s.keptEditsNotice).toBe(false);
  });
});
```

Run: `npm test`
Expected: FAIL — `Failed to resolve import "./reducer"`.

- [ ] **Step 2: Write `app/ui/src/screens/editor/reducer.ts`**

```ts
// The editor's state (spec §3.4): the theme being edited, 100 steps of undo and redo, the
// mode, and the rule that ties them: any color edited by hand makes `simple` null; going back
// to simple mode keeps those edits, with a notice, until the tone, the main color or the tint
// change, which regenerates every color (through `palette_generate`, in Rust).
import type { Colors } from "../../bindings/Colors";
import type { SimpleRecipe } from "../../bindings/SimpleRecipe";
import type { Theme } from "../../bindings/Theme";

export const HISTORY = 100;

export type Mode = "simple" | "advanced";

export interface EditorState {
  theme: Theme;
  past: Theme[];
  future: Theme[];
  mode: Mode;
  /** Simple mode is shown while `simple` is null: the hand edits are kept (spec §3.4). */
  keptEditsNotice: boolean;
  /** Changed since it was loaded or saved. */
  dirty: boolean;
}

export type ColorSlot = Exclude<keyof Colors, "ansi"> | `ansi${number}`;

export type EditorAction =
  | { type: "load"; theme: Theme; mode?: Mode }
  /** A field other than a color (name, font, prompt, banner…): the new theme. */
  | { type: "edit"; theme: Theme }
  /** One color by hand (advanced mode): `simple` becomes null. */
  | { type: "setColor"; slot: ColorSlot; value: string }
  /** Every color at once, from simple mode's recipe (the Rust palette), or a readability fix
   * (`simple: null`). */
  | { type: "setColors"; colors: Colors; simple: SimpleRecipe | null }
  | { type: "setMode"; mode: Mode }
  | { type: "undo" }
  | { type: "redo" }
  | { type: "saved" };

export function initialState(theme: Theme): EditorState {
  return {
    theme,
    past: [],
    future: [],
    mode: theme.simple ? "simple" : "advanced",
    keptEditsNotice: false,
    dirty: false,
  };
}

function push(state: EditorState, theme: Theme): EditorState {
  const past = [...state.past, state.theme];
  if (past.length > HISTORY) past.shift();
  return { ...state, theme, past, future: [], dirty: true };
}

export function setSlot(colors: Colors, slot: ColorSlot, value: string): Colors {
  if (slot.startsWith("ansi")) {
    const index = Number(slot.slice(4));
    const ansi = [...colors.ansi];
    if (index >= 0 && index < ansi.length) ansi[index] = value;
    return { ...colors, ansi };
  }
  return { ...colors, [slot]: value };
}

export function getSlot(colors: Colors, slot: ColorSlot): string {
  if (slot.startsWith("ansi")) return colors.ansi[Number(slot.slice(4))] ?? "#000000";
  return colors[slot as Exclude<keyof Colors, "ansi">];
}

export function reduce(state: EditorState, action: EditorAction): EditorState {
  switch (action.type) {
    case "load":
      return { ...initialState(action.theme), mode: action.mode ?? initialState(action.theme).mode };
    case "edit":
      return push(state, action.theme);
    case "setColor": {
      const theme: Theme = { ...state.theme, colors: setSlot(state.theme.colors, action.slot, action.value), simple: null };
      return push(state, theme);
    }
    case "setColors": {
      const theme: Theme = { ...state.theme, colors: action.colors, simple: action.simple };
      // A regenerate from the recipe replaces the hand edits: the notice is over.
      return { ...push(state, theme), keptEditsNotice: action.simple ? false : state.keptEditsNotice };
    }
    case "setMode":
      if (action.mode === state.mode) return state;
      return {
        ...state,
        mode: action.mode,
        keptEditsNotice: action.mode === "simple" && state.theme.simple === null,
      };
    case "undo": {
      const previous = state.past[state.past.length - 1];
      if (!previous) return state;
      return { ...state, theme: previous, past: state.past.slice(0, -1), future: [state.theme, ...state.future], dirty: true };
    }
    case "redo": {
      const [next, ...rest] = state.future;
      if (!next) return state;
      return { ...state, theme: next, past: [...state.past, state.theme], future: rest, dirty: true };
    }
    case "saved":
      return { ...state, dirty: false };
  }
}
```

- [ ] **Step 3: Run the tests**

Run: `npm test` → `Tests 14 passed` (9 + 5). `npm run typecheck` clean.

- [ ] **Step 4: See that the rule bites**

In `reduce`, case `"setColor"`, change `simple: null` to `simple: state.theme.simple`; `npm test` FAILS `makes simple null as soon as one color is edited by hand`. Restore; passes.

- [ ] **Step 5: Commit**

Stage `app/ui/src/screens/editor`. Message: `ui: the editor's reducer — 100 undo steps, simple and advanced mode, hand edits make the recipe null`.

---

### Task 6: The Themes and Editor screens behind an interim shell, and the visual check

Spec §3.1 (the library: presets with Customize, my themes with edit/rename/duplicate/delete/export/apply, drafts, the all/dark/light filter, the "In use" badge, import with a preview, New theme in simple mode with `#d97757` and tint 0.10) and §3.4 (the editor: name and author, simple mode with tone/main color/tint, advanced mode with all 23 colors, the readability panel with "Fix automatically", font and cursor and opacity and padding, the prompt's layout and segments ordered by keyboard-reachable buttons, the welcome box, the live preview from the core with editable sample user/folder/width, drafts a second after an edit, Ctrl+Z/Ctrl+Y, "Save and apply" as the only action that touches the machine; Powerline switches the font to the Nerd Font and says so). The interim `App.tsx` loads the settings, opens English (Thai when saved), and shows Themes and the editor behind a one-item sidebar with the after-apply bar (spec §4.4: "Theme X applied · open a new cmd window…" with Undo); `main.tsx` mounts it with Tauri's IPC, or the mock under `VITE_MOCK_IPC=1`.

**Files:**
- Create: `app/ui/src/screens/Themes.tsx`, `app/ui/src/screens/Editor.tsx`, `app/ui/src/App.tsx` (interim; the phase 3 plan replaces it), `app/ui/src/main.tsx`, `app/ui/src/Themes.test.tsx`
- Test: `Themes.test.tsx` (6 tests); the visual check in the browser pane

**Interfaces:**
- Consumes: Tasks 2–5; `SAMPLE_CONTEXT` from `ipc/mock.ts` (the editor's default sample values — fixed data, not logic).
- Produces: `export function Themes(): JSX.Element`, `export const NEW_THEME_ACCENT = "#d97757"`, `export const NEW_THEME_TINT = 0.1`; `export function Editor(props: { id: string }): JSX.Element`, `export const NERD_FONT = "CaskaydiaCove NF"`, `export const DRAFT_DELAY_MS = 1000`; `export function App(): JSX.Element` (interim: routes `themes` and `editor`); the mount in `main.tsx`.

- [ ] **Step 1: Write the failing test** — `app/ui/src/Themes.test.tsx` (its `openThemes` helper works for this shell, which starts on Themes, and for the final one, which starts on Home):

```tsx
// The theme library and the editor against the mock IPC: Customize copies a preset into my
// themes and opens the editor; the filter and the "in use" badge; a plan shown and applied
// from a card; the after-apply bar's Undo; the editor's draft and save; a failed apply is said.
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { App } from "./App";
import { IpcProvider } from "./ipc/context";
import { createMockIpc, type MockOptions } from "./ipc/mock";

function mount(options: MockOptions = {}) {
  const ipc = createMockIpc({ ...options, settings: { wizardDone: true, ...options.settings } });
  const user = userEvent.setup();
  render(
    <IpcProvider ipc={ipc}>
      <App />
    </IpcProvider>,
  );
  return { ipc, user };
}

/** Opens the Themes page: the shell starts on it in the themes plan, on Home afterwards. */
async function openThemes(user: ReturnType<typeof userEvent.setup>) {
  await screen.findByRole("heading", { name: /^(Home|Themes)$/ });
  await user.click(screen.getByRole("button", { name: "Themes" }));
  await screen.findByRole("heading", { name: "Themes" }, { timeout: 3000 });
}

async function card(name: string): Promise<HTMLElement> {
  const labels = await screen.findAllByText(name);
  return labels[0]!.closest(".theme-card") as HTMLElement;
}

describe("the theme library", () => {
  it("lists the twelve presets and customizes one into a copy", async () => {
    const { ipc, user } = mount();
    await openThemes(user);
    expect((await screen.findAllByText("Built in")).length).toBe(12);
    await user.click(within(await card("Terracotta")).getByRole("button", { name: "Customize" }));
    await screen.findByRole("heading", { name: "Edit theme" });
    expect(ipc.state.mine.map((t) => t.name)).toEqual(["My Terracotta"]);
    expect((screen.getByLabelText("Theme name") as HTMLInputElement).value).toBe("My Terracotta");
    // Advanced mode shows the 23 colors; a hand edit makes the recipe null.
    await user.click(screen.getByRole("tab", { name: "Advanced" }));
    expect(screen.getByLabelText("Frame")).toBeTruthy();
    expect(screen.getByLabelText("Bright white")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Save" }));
    expect(await screen.findByText("Saved.")).toBeTruthy();
    expect(ipc.state.mine[0]?.name).toBe("My Terracotta");
  });

  it("filters by tone, shows the plan before applying, and marks the theme in use", async () => {
    const { ipc, user } = mount();
    await openThemes(user);
    await user.click(screen.getByRole("button", { name: "Light" }));
    await waitFor(() => expect(screen.queryByText("Terracotta")).toBeNull());
    expect(screen.getByText("Paper")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "All" }));
    await user.click(within(await card("Paper")).getByRole("button", { name: "Apply" }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("Update the prompt and the welcome box")).toBeTruthy();
    expect(within(dialog).getByText("Use this theme in every cmd window")).toBeTruthy();
    expect(ipc.state.applied).toBeNull();
    await user.click(within(dialog).getByRole("button", { name: "Apply" }));
    await within(dialog).findByText(/Theme "Paper" applied/);
    expect(ipc.state.applied?.theme.name).toBe("Paper");
    await user.click(within(dialog).getByRole("button", { name: "Close" }));
    await waitFor(() => expect(screen.getAllByText("In use").length).toBe(1));
    // The after-apply bar undoes it.
    await user.click(screen.getByRole("button", { name: "Undo" }));
    await waitFor(() => expect(ipc.state.applied).toBeNull());
  });

  it("asks about another cmd add-on and goes on with the user's yes", async () => {
    const { ipc, user } = mount({ otherClink: ['"C:\\clink\\clink_x64.exe" inject --autorun'] });
    await openThemes(user);
    await user.click(within(await card("Nord")).getByRole("button", { name: "Apply" }));
    const dialog = await screen.findByRole("dialog");
    await within(dialog).findByRole("heading", { name: "Another cmd add-on is already set up" });
    expect(within(dialog).getByText(/clink_x64\.exe/)).toBeTruthy();
    await user.click(within(dialog).getByRole("button", { name: "Use CMD Studio's instead" }));
    await within(dialog).findByText("Turn off the cmd add-on that was set up before (can be undone)");
    await user.click(within(dialog).getByRole("button", { name: "Apply" }));
    await within(dialog).findByText(/applied/);
    expect(ipc.state.applied?.request.replaceOtherClink).toBe(true);
  });

  it("says a failed apply was put back, with a way to copy the details", async () => {
    const { ipc, user } = mount({ failApplyWith: "E_VERIFY_OMP" });
    await openThemes(user);
    await user.click(within(await card("Dracula")).getByRole("button", { name: "Apply" }));
    const dialog = await screen.findByRole("dialog");
    await user.click(await within(dialog).findByRole("button", { name: "Apply" }));
    expect(await within(dialog).findByText("The theme could not be applied. Everything was put back as it was.")).toBeTruthy();
    expect(within(dialog).getByText(/Code E_VERIFY_OMP/)).toBeTruthy();
    expect(within(dialog).getByRole("button", { name: "Copy details for a problem report" })).toBeTruthy();
    expect(ipc.state.applied).toBeNull();
  });

  it("makes a new theme in simple mode and keeps a draft", async () => {
    const { ipc, user } = mount();
    await openThemes(user);
    await user.click(screen.getByRole("button", { name: "New theme" }));
    await screen.findByRole("heading", { name: "Edit theme" });
    expect(screen.getByRole("tab", { name: "Simple", selected: true })).toBeTruthy();
    expect(ipc.state.drafts).toHaveLength(1);
    expect(ipc.state.drafts[0]?.simple).toEqual({ accent: "#d97757", tint: 0.1 });
    // Powerline switches the font to the Nerd Font and says so.
    await user.click(screen.getByLabelText("Powerline"));
    expect(await screen.findByText(/switched to CaskaydiaCove NF/)).toBeTruthy();
    expect((screen.getByLabelText("Font") as HTMLSelectElement).value).toBe("CaskaydiaCove NF");
    await user.click(screen.getByRole("button", { name: "Back to themes" }));
    expect((await screen.findAllByText("Draft")).length).toBe(1);
  });

  it("imports after a preview", async () => {
    const { ipc, user } = mount();
    await openThemes(user);
    await user.click(screen.getByRole("button", { name: "Import a theme file" }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("A theme with the same id already exists; the import gets a new one.")).toBeTruthy();
    await user.click(within(dialog).getByRole("button", { name: "Add to my themes" }));
    await waitFor(() => expect(ipc.state.mine.map((t) => t.name)).toEqual(["Imported theme"]));
  });
});
```

Run: `npm test`
Expected: FAIL — `Failed to resolve import "./App"`.

- [ ] **Step 2: Write `app/ui/src/screens/Themes.tsx`**

```tsx
// The theme library (spec §3.1): presets (Customize copies one into my themes), my themes
// (edit, rename, duplicate, delete, export, apply), drafts of unsaved themes, a filter
// all / dark / light, the "in use" badge, import (file → check → preview → confirm) and a new
// theme (simple mode, dark, #d97757, tint 0.10).
import { useCallback, useEffect, useState } from "react";
import type { ApplyRequest } from "../bindings/ApplyRequest";
import type { ImportPreview } from "../bindings/ImportPreview";
import type { Library } from "../bindings/Library";
import type { PreviewResult } from "../bindings/PreviewResult";
import type { Theme } from "../bindings/Theme";
import type { ThemeSummary } from "../bindings/ThemeSummary";
import type { Tone } from "../bindings/Tone";
import { ApplyFlow } from "../components/ApplyFlow";
import { Dialog } from "../components/Dialog";
import { ErrorNotice, Notice } from "../components/Notices";
import { TerminalPreview } from "../components/TerminalPreview";
import { ThemeCard } from "../components/ThemeCard";
import { useI18n } from "../i18n";
import { useIpc } from "../ipc/context";
import { SAMPLE_CONTEXT } from "../ipc/mock";
import { useNav } from "../router";
import { useAppState } from "../state";

export const NEW_THEME_ACCENT = "#d97757";
export const NEW_THEME_TINT = 0.1;

function uuidV4(): string {
  const bytes = new Uint8Array(16);
  crypto.getRandomValues(bytes);
  bytes[6] = ((bytes[6] ?? 0) & 0x0f) | 0x40;
  bytes[8] = ((bytes[8] ?? 0) & 0x3f) | 0x80;
  const hex = Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

type Filter = "all" | Tone;

export function Themes() {
  const ipc = useIpc();
  const { t, language } = useI18n();
  const { go } = useNav();
  const { settings, version, bump, setLastApply } = useAppState();
  const [library, setLibrary] = useState<Library | null>(null);
  const [themes, setThemes] = useState<Record<string, Theme>>({});
  const [filter, setFilter] = useState<Filter>("all");
  const [error, setError] = useState<unknown>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [applying, setApplying] = useState<ApplyRequest | null>(null);
  const [deleting, setDeleting] = useState<ThemeSummary | null>(null);
  const [renaming, setRenaming] = useState<ThemeSummary | null>(null);
  const [newName, setNewName] = useState("");
  const [importing, setImporting] = useState<ImportPreview | null>(null);
  const [importPreview, setImportPreview] = useState<PreviewResult | null>(null);

  const load = useCallback(async () => {
    setError(null);
    try {
      const lib = await ipc.themesList();
      setLibrary(lib);
      const details = await Promise.all(lib.themes.map((s) => ipc.themeGet(s.id).then((d) => [s.id, d.theme] as const).catch(() => null)));
      const map: Record<string, Theme> = {};
      for (const entry of details) if (entry) map[entry[0]] = entry[1];
      setThemes(map);
    } catch (e) {
      setError(e);
    }
  }, [ipc]);

  useEffect(() => {
    void load();
  }, [load, version]);

  const run = async (action: () => Promise<void>) => {
    setError(null);
    try {
      await action();
      await load();
    } catch (e) {
      setError(e);
    }
  };

  const customize = (s: ThemeSummary) =>
    run(async () => {
      const copy = await ipc.themeDuplicate(s.id, t("themes.copyName", { name: s.name }));
      go({ name: "editor", id: copy.id });
    });

  const duplicate = (s: ThemeSummary) => run(async () => void (await ipc.themeDuplicate(s.id, t("themes.copyName", { name: s.name }))));

  const createNew = () =>
    run(async () => {
      const base = themes[library?.themes.find((s) => s.source === "preset")?.id ?? ""];
      if (!base) return;
      const colors = await ipc.paletteGenerate("dark", NEW_THEME_ACCENT, NEW_THEME_TINT);
      const theme: Theme = {
        ...base,
        id: uuidV4(),
        name: t("themes.newName"),
        author: "",
        tone: "dark",
        simple: { accent: NEW_THEME_ACCENT, tint: NEW_THEME_TINT },
        colors,
      };
      const saved = await ipc.themeSave(theme, true);
      go({ name: "editor", id: saved.id });
    });

  const exportTheme = (s: ThemeSummary) =>
    run(async () => {
      const path = await ipc.themeExport(s.id);
      setMessage(path ? t("themes.exported", { path }) : null);
    });

  const startImport = () =>
    run(async () => {
      const preview = await ipc.themeImportPreview();
      if (!preview) {
        setMessage(t("themes.import.cancelled"));
        return;
      }
      setImporting(preview);
      setImportPreview(await ipc.previewRender(preview.theme, SAMPLE_CONTEXT).catch(() => null));
    });

  const confirmImport = () =>
    run(async () => {
      if (!importing) return;
      await ipc.themeImportConfirm(importing.token);
      setImporting(null);
    });

  const confirmDelete = () =>
    run(async () => {
      if (!deleting) return;
      await ipc.themeDelete(deleting.id, deleting.source === "draft");
      setDeleting(null);
    });

  const confirmRename = () =>
    run(async () => {
      if (!renaming) return;
      const theme = themes[renaming.id];
      if (!theme) return;
      await ipc.themeSave({ ...theme, name: newName }, false);
      setRenaming(null);
    });

  const visible = (library?.themes ?? []).filter((s) => filter === "all" || s.tone === filter);
  const groups: { key: "preset" | "mine" | "draft"; title: string }[] = [
    { key: "preset", title: t("themes.presets") },
    { key: "mine", title: t("themes.mine") },
    { key: "draft", title: t("themes.drafts") },
  ];
  const applyRequest = (s: ThemeSummary): ApplyRequest => ({
    themeId: s.id,
    target: settings.target,
    setWtDefaultProfile: settings.setWtDefaultProfile,
    replaceOtherClink: false,
  });

  return (
    <section>
      <h1 tabIndex={-1}>{t("themes.title")}</h1>
      {error !== null && <ErrorNotice error={error} />}
      {message && <Notice kind="ok">{message}</Notice>}
      {library?.stateProblem && <ErrorNotice error={library.stateProblem} />}
      {library && library.problems.length > 0 && (
        <Notice kind="warn">
          {t("themes.problems", { n: library.problems.length })}
          <ul className="plain-list">
            {library.problems.map((p) => (
              <li key={p.file}>
                <code>{p.file}</code>: {p.message}
              </li>
            ))}
          </ul>
        </Notice>
      )}
      <div className="row-actions" role="group" aria-label={t("themes.filter.label")}>
        {(["all", "dark", "light"] as Filter[]).map((f) => (
          <button key={f} type="button" aria-pressed={filter === f} onClick={() => setFilter(f)}>
            {t(`themes.filter.${f}`)}
          </button>
        ))}
        <span style={{ flex: 1 }} />
        <button type="button" onClick={() => void createNew()}>
          {t("themes.new")}
        </button>
        <button type="button" onClick={() => void startImport()}>
          {t("themes.import")}
        </button>
      </div>
      {visible.length === 0 && library && <p>{t("themes.empty")}</p>}
      {groups.map((g) => {
        const items = visible.filter((s) => s.source === g.key);
        if (items.length === 0) return null;
        return (
          <div key={g.key}>
            <h2>{g.title}</h2>
            <div className="grid">
              {items.map((s) => (
                <ThemeCard key={s.id} summary={s} theme={themes[s.id] ?? null}>
                  <div className="row-actions">
                    <button type="button" className="primary" disabled={s.inUse} onClick={() => setApplying(applyRequest(s))}>
                      {t("themes.apply")}
                    </button>
                    {s.source === "preset" && (
                      <button type="button" onClick={() => void customize(s)}>
                        {t("common.customize")}
                      </button>
                    )}
                    {s.source !== "preset" && (
                      <button type="button" onClick={() => go({ name: "editor", id: s.id })}>
                        {t("common.edit")}
                      </button>
                    )}
                    {s.source === "mine" && (
                      <>
                        <button
                          type="button"
                          onClick={() => {
                            setRenaming(s);
                            setNewName(s.name);
                          }}
                        >
                          {t("common.rename")}
                        </button>
                        <button type="button" onClick={() => void duplicate(s)}>
                          {t("common.duplicate")}
                        </button>
                        <button type="button" onClick={() => void exportTheme(s)}>
                          {t("common.export")}
                        </button>
                        <button type="button" className="danger" onClick={() => setDeleting(s)}>
                          {t("common.delete")}
                        </button>
                      </>
                    )}
                    {s.source === "draft" && (
                      <button type="button" className="danger" onClick={() => setDeleting(s)}>
                        {t("themes.discardDraft")}
                      </button>
                    )}
                  </div>
                </ThemeCard>
              ))}
            </div>
          </div>
        );
      })}
      {applying && (
        <Dialog title={t("apply.planTitle")} onClose={() => setApplying(null)}>
          <ApplyFlow
            titled={false}
            request={applying}
            onCancel={() => setApplying(null)}
            onApplied={(o) => {
              setLastApply(o);
              bump();
            }}
          />
        </Dialog>
      )}
      {deleting && (
        <Dialog title={t("common.delete")} onClose={() => setDeleting(null)}>
          <p>{t("themes.deleteConfirm", { name: deleting.name })}</p>
          {deleting.inUse && <p>{t("themes.deleteInUse")}</p>}
          <div className="row-actions">
            <button type="button" className="danger" onClick={() => void confirmDelete()}>
              {deleting.source === "draft" ? t("themes.discardDraft") : t("common.delete")}
            </button>
          </div>
        </Dialog>
      )}
      {renaming && (
        <Dialog title={t("common.rename")} onClose={() => setRenaming(null)}>
          <label>
            {t("themes.renamePrompt")}
            <input value={newName} maxLength={64} onChange={(e) => setNewName(e.target.value)} />
          </label>
          <div className="row-actions">
            <button type="button" className="primary" disabled={newName.trim().length === 0} onClick={() => void confirmRename()}>
              {t("common.save")}
            </button>
          </div>
        </Dialog>
      )}
      {importing && (
        <Dialog title={t("themes.import.title")} onClose={() => setImporting(null)}>
          <p>
            <strong>{importing.theme.name}</strong>
            {importing.theme.author && ` · ${importing.theme.author}`}
          </p>
          {importing.idTaken && <p>{t("themes.import.idTaken")}</p>}
          {importing.nameTaken && <p>{t("themes.import.nameTaken")}</p>}
          <TerminalPreview theme={importing.theme} result={importPreview} />
          <div className="row-actions">
            <button type="button" className="primary" onClick={() => void confirmImport()}>
              {t("themes.import.confirm")}
            </button>
          </div>
          <span lang={language} />
        </Dialog>
      )}
    </section>
  );
}
```

- [ ] **Step 3: Write `app/ui/src/screens/Editor.tsx`**

```tsx
// The theme editor (spec §3.4): simple mode (tone, main color, tint → every color from the
// Rust palette), advanced mode (all 23 colors), the readability panel with "Fix automatically",
// font and cursor, the prompt's look and segments (order by buttons, keyboard-reachable), the
// welcome box, the live preview painted from the core's spans, drafts saved one second after an
// edit, Ctrl+Z / Ctrl+Y with 100 steps, and "Save and apply" as the only action that changes
// the machine. Presets never open here: Customize copies one first.
import { useCallback, useEffect, useReducer, useRef, useState } from "react";
import type { ApplyRequest } from "../bindings/ApplyRequest";
import type { ContrastIssue } from "../bindings/ContrastIssue";
import type { CursorShape } from "../bindings/CursorShape";
import type { Layout } from "../bindings/Layout";
import type { PreviewResult } from "../bindings/PreviewResult";
import type { Segment } from "../bindings/Segment";
import type { SystemReport } from "../bindings/SystemReport";
import type { Theme } from "../bindings/Theme";
import type { Tone } from "../bindings/Tone";
import { ApplyFlow } from "../components/ApplyFlow";
import { Dialog } from "../components/Dialog";
import { ErrorNotice, Notice } from "../components/Notices";
import { TerminalPreview } from "../components/TerminalPreview";
import { useI18n, type Key } from "../i18n";
import { useIpc } from "../ipc/context";
import { SAMPLE_CONTEXT } from "../ipc/mock";
import { useNav } from "../router";
import { useAppState } from "../state";
import { getSlot, initialState, reduce, type ColorSlot, type EditorAction, type EditorState } from "./editor/reducer";

export const NERD_FONT = "CaskaydiaCove NF";
export const DRAFT_DELAY_MS = 1000;
const SEGMENTS: Segment[] = ["path", "git", "time", "user", "host", "status", "duration"];
const BASIC_SLOTS: ColorSlot[] = ["background", "foreground", "cursor", "selection", "accent", "muted", "frame"];
const ANSI_SLOTS: ColorSlot[] = Array.from({ length: 16 }, (_, i) => `ansi${i}` as ColorSlot);

function useDebounced<T>(value: T, ms: number): T {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    const handle = setTimeout(() => setDebounced(value), ms);
    return () => clearTimeout(handle);
  }, [value, ms]);
  return debounced;
}

export function Editor({ id }: { id: string }) {
  const ipc = useIpc();
  const { t, language } = useI18n();
  const { go } = useNav();
  const { settings, setLastApply, bump } = useAppState();
  const [state, dispatch] = useReducer(reduceNullable, null);
  const [loadError, setLoadError] = useState<unknown>(null);
  const [error, setError] = useState<unknown>(null);
  const [draftOffer, setDraftOffer] = useState<Theme | null>(null);
  const [preview, setPreview] = useState<PreviewResult | null>(null);
  const [issues, setIssues] = useState<ContrastIssue[]>([]);
  const [report, setReport] = useState<SystemReport | null>(null);
  const [customFont, setCustomFont] = useState(false);
  const [powerlineNotice, setPowerlineNotice] = useState(false);
  const [status, setStatus] = useState<string | null>(null);
  const [applying, setApplying] = useState<ApplyRequest | null>(null);
  const [sampleUser, setSampleUser] = useState(SAMPLE_CONTEXT.user);
  const [sampleCwd, setSampleCwd] = useState(SAMPLE_CONTEXT.cwd);
  const [sampleWidth, setSampleWidth] = useState(SAMPLE_CONTEXT.width);
  const loadedId = useRef<string | null>(null);

  // Load the theme (and its draft, if any) once per id.
  useEffect(() => {
    let alive = true;
    loadedId.current = null;
    Promise.all([ipc.themeGet(id), ipc.systemDetect().catch(() => null)])
      .then(([detail, sys]) => {
        if (!alive) return;
        setReport(sys);
        dispatch({ type: "load", theme: detail.theme });
        setDraftOffer(detail.draft);
        loadedId.current = id;
      })
      .catch((e: unknown) => alive && setLoadError(e));
    return () => {
      alive = false;
    };
  }, [ipc, id]);

  const theme = state?.theme ?? null;
  const debouncedTheme = useDebounced(theme, 150);
  const draftTheme = useDebounced(theme, DRAFT_DELAY_MS);

  // The preview, from the core, after each edit settles.
  useEffect(() => {
    if (!debouncedTheme) return;
    let alive = true;
    ipc
      .previewRender(debouncedTheme, { ...SAMPLE_CONTEXT, user: sampleUser, cwd: sampleCwd, width: sampleWidth })
      .then((r) => alive && setPreview(r))
      .catch(() => {});
    ipc
      .contrastCheck(debouncedTheme.colors)
      .then((list) => alive && setIssues(list))
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [ipc, debouncedTheme, sampleUser, sampleCwd, sampleWidth]);

  // Autosave a draft one second after the last edit (spec §3.4).
  useEffect(() => {
    if (!draftTheme || !state?.dirty || loadedId.current !== id) return;
    let alive = true;
    ipc
      .themeSave(draftTheme, true)
      .then(() => alive && setStatus(t("editor.draftSaved")))
      .catch(() => {});
    return () => {
      alive = false;
    };
    // `t` changes with the language only; the draft is about the theme.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ipc, draftTheme, id]);

  const edit = useCallback((next: Theme) => dispatch({ type: "edit", theme: next }), []);

  const onKey = (e: React.KeyboardEvent<HTMLDivElement>) => {
    const target = e.target as HTMLElement;
    const typing = target.tagName === "INPUT" && (target as HTMLInputElement).type === "text";
    if (typing || target.tagName === "TEXTAREA") return;
    if (e.ctrlKey && !e.shiftKey && e.key.toLowerCase() === "z") {
      e.preventDefault();
      dispatch({ type: "undo" });
    } else if (e.ctrlKey && (e.key.toLowerCase() === "y" || (e.shiftKey && e.key.toLowerCase() === "z"))) {
      e.preventDefault();
      dispatch({ type: "redo" });
    }
  };

  if (loadError !== null) return <ErrorNotice error={loadError} />;
  if (!state || !theme) return <p role="status">{t("common.loading")}</p>;

  const recipe = theme.simple ?? { accent: theme.colors.accent, tint: 0.1 };

  const regenerate = async (tone: Tone, accent: string, tint: number) => {
    setError(null);
    try {
      const colors = await ipc.paletteGenerate(tone, accent, tint);
      dispatch({ type: "setColors", colors, simple: { accent, tint } });
      if (tone !== theme.tone) edit({ ...theme, tone, colors, simple: { accent, tint } });
    } catch (e) {
      setError(e);
    }
  };

  const fixReadability = async () => {
    setError(null);
    try {
      const colors = await ipc.contrastFix(theme.colors);
      dispatch({ type: "setColors", colors, simple: null });
    } catch (e) {
      setError(e);
    }
  };

  const setLayout = (layout: Layout) => {
    let next: Theme = { ...theme, prompt: { ...theme.prompt, layout } };
    if (layout === "powerline" && theme.terminal.font !== NERD_FONT) {
      next = { ...next, terminal: { ...next.terminal, font: NERD_FONT } };
      setPowerlineNotice(true);
    }
    edit(next);
  };

  const toggleSegment = (segment: Segment, on: boolean) => {
    const segments = on ? [...theme.prompt.segments, segment] : theme.prompt.segments.filter((s) => s !== segment);
    edit({ ...theme, prompt: { ...theme.prompt, segments } });
  };

  const moveSegment = (segment: Segment, by: -1 | 1) => {
    const list = [...theme.prompt.segments];
    const i = list.indexOf(segment);
    const j = i + by;
    if (i < 0 || j < 0 || j >= list.length) return;
    const other = list[j];
    if (other === undefined) return;
    list[j] = segment;
    list[i] = other;
    edit({ ...theme, prompt: { ...theme.prompt, segments: list } });
  };

  const save = async (): Promise<boolean> => {
    setError(null);
    try {
      await ipc.themeSave(theme, false);
      dispatch({ type: "saved" });
      setStatus(t("editor.saved"));
      return true;
    } catch (e) {
      setError(e);
      return false;
    }
  };

  const saveAndApply = async () => {
    if (await save()) {
      setApplying({ themeId: theme.id, target: settings.target, setWtDefaultProfile: settings.setWtDefaultProfile, replaceOtherClink: false });
    }
  };

  const fonts = report?.fonts.monospace ?? [];
  const knownFont = fonts.includes(theme.terminal.font) || theme.terminal.font === NERD_FONT;

  return (
    <section onKeyDown={onKey}>
      <h1 tabIndex={-1}>{t("editor.title")}</h1>
      <div className="row-actions">
        <button type="button" onClick={() => go({ name: "themes" })}>
          {t("editor.back")}
        </button>
        <button type="button" disabled={state.past.length === 0} onClick={() => dispatch({ type: "undo" })}>
          {t("editor.undo")}
        </button>
        <button type="button" disabled={state.future.length === 0} onClick={() => dispatch({ type: "redo" })}>
          {t("editor.redo")}
        </button>
        <button type="button" onClick={() => void save()}>
          {t("editor.save")}
        </button>
        <button type="button" className="primary" onClick={() => void saveAndApply()}>
          {t("editor.saveApply")}
        </button>
        {status && (
          <span role="status" aria-live="polite">
            {status}
          </span>
        )}
      </div>
      {error !== null && <ErrorNotice error={error} />}
      {draftOffer && (
        <Notice kind="warn">
          {t("editor.draftFound")}
          <div className="row-actions">
            <button
              type="button"
              onClick={() => {
                dispatch({ type: "load", theme: draftOffer });
                dispatch({ type: "edit", theme: draftOffer });
                setDraftOffer(null);
              }}
            >
              {t("editor.useDraft")}
            </button>
            <button
              type="button"
              onClick={() => {
                void ipc.themeDelete(id, true).catch(() => {});
                setDraftOffer(null);
              }}
            >
              {t("editor.discardDraft")}
            </button>
          </div>
        </Notice>
      )}
      <div className="editor">
        <div>
          <fieldset>
            <legend>{t("common.name")}</legend>
            <label>
              {t("editor.name")}
              <input type="text" value={theme.name} maxLength={64} onChange={(e) => edit({ ...theme, name: e.target.value })} />
            </label>
            <label>
              {t("editor.author")}
              <input type="text" value={theme.author} maxLength={64} onChange={(e) => edit({ ...theme, author: e.target.value })} />
            </label>
          </fieldset>

          <div className="tabs" role="tablist" aria-label={t("editor.colors.title")}>
            <button type="button" role="tab" aria-selected={state.mode === "simple"} onClick={() => dispatch({ type: "setMode", mode: "simple" })}>
              {t("editor.mode.simple")}
            </button>
            <button type="button" role="tab" aria-selected={state.mode === "advanced"} onClick={() => dispatch({ type: "setMode", mode: "advanced" })}>
              {t("editor.mode.advanced")}
            </button>
          </div>

          {state.mode === "simple" && (
            <fieldset>
              <legend>{t("editor.mode.simple")}</legend>
              {state.keptEditsNotice && <Notice>{t("editor.simple.keptNotice")}</Notice>}
              <div role="group" aria-label={t("editor.simple.tone")}>
                {(["dark", "light"] as Tone[]).map((tone) => (
                  <label className="row" key={tone}>
                    <input type="radio" name="tone" checked={theme.tone === tone} onChange={() => void regenerate(tone, recipe.accent, recipe.tint)} />
                    {t(tone === "dark" ? "editor.simple.dark" : "editor.simple.light")}
                  </label>
                ))}
              </div>
              <label className="row">
                {t("editor.simple.accent")}
                <input type="color" value={recipe.accent} onChange={(e) => void regenerate(theme.tone, e.target.value, recipe.tint)} />
                <code>{recipe.accent}</code>
              </label>
              <label className="row">
                {t("editor.simple.tint")}
                <input
                  type="range"
                  min={0}
                  max={0.3}
                  step={0.01}
                  value={recipe.tint}
                  onChange={(e) => void regenerate(theme.tone, recipe.accent, Number(e.target.value))}
                />
                <span>{recipe.tint.toFixed(2)}</span>
              </label>
            </fieldset>
          )}

          {state.mode === "advanced" && (
            <fieldset>
              <legend>{t("editor.colors.title")}</legend>
              <div className="color-grid">
                {BASIC_SLOTS.map((slot) => (
                  <label key={slot}>
                    <input type="color" value={getSlot(theme.colors, slot)} onChange={(e) => dispatch({ type: "setColor", slot, value: e.target.value })} />
                    {t(`editor.colors.${slot}` as Key)}
                  </label>
                ))}
              </div>
              <h3>{t("editor.colors.ansi")}</h3>
              <div className="color-grid">
                {ANSI_SLOTS.map((slot) => (
                  <label key={slot}>
                    <input type="color" value={getSlot(theme.colors, slot)} onChange={(e) => dispatch({ type: "setColor", slot, value: e.target.value })} />
                    {t(`editor.colors.${slot}` as Key)}
                  </label>
                ))}
              </div>
            </fieldset>
          )}

          <fieldset>
            <legend>{t("editor.readability.title")}</legend>
            {issues.length === 0 ? (
              <p className="status-ok">{t("editor.readability.ok")}</p>
            ) : (
              <>
                <ul className="plain-list">
                  {issues.map((issue) => (
                    <li key={issue.slot} className="status-warn">
                      {t("editor.readability.issue", {
                        slot: t(`editor.colors.${issue.slot}` as Key),
                        ratio: issue.ratio.toFixed(1),
                        required: issue.required.toFixed(1),
                      })}
                    </li>
                  ))}
                </ul>
                <button type="button" onClick={() => void fixReadability()}>
                  {t("editor.readability.fix")}
                </button>
              </>
            )}
          </fieldset>

          <fieldset>
            <legend>{t("editor.font.title")}</legend>
            {powerlineNotice && <Notice>{t("editor.font.powerlineNotice")}</Notice>}
            <label>
              {t("editor.font.family")}
              <select
                value={customFont || !knownFont ? "__custom" : theme.terminal.font}
                onChange={(e) => {
                  if (e.target.value === "__custom") setCustomFont(true);
                  else {
                    setCustomFont(false);
                    edit({ ...theme, terminal: { ...theme.terminal, font: e.target.value } });
                  }
                }}
              >
                {[NERD_FONT, ...fonts.filter((f) => f !== NERD_FONT)].map((f) => (
                  <option key={f} value={f}>
                    {f}
                  </option>
                ))}
                <option value="__custom">{t("editor.font.custom")}</option>
              </select>
            </label>
            {(customFont || !knownFont) && (
              <label>
                {t("editor.font.custom")}
                <input type="text" value={theme.terminal.font} maxLength={64} onChange={(e) => edit({ ...theme, terminal: { ...theme.terminal, font: e.target.value } })} />
              </label>
            )}
            <label>
              {t("editor.font.size")}
              <input type="number" min={8} max={32} value={theme.terminal.fontSize} onChange={(e) => edit({ ...theme, terminal: { ...theme.terminal, fontSize: clamp(Number(e.target.value), 8, 32) } })} />
            </label>
            <label>
              {t("editor.font.cursor")}
              <select value={theme.terminal.cursorShape} onChange={(e) => edit({ ...theme, terminal: { ...theme.terminal, cursorShape: e.target.value as CursorShape } })}>
                {(["filledBox", "bar", "underscore"] as CursorShape[]).map((shape) => (
                  <option key={shape} value={shape}>
                    {t(`editor.font.cursor.${shape}`)}
                  </option>
                ))}
              </select>
            </label>
            <label>
              {t("editor.font.opacity")}
              <input type="range" min={30} max={100} value={theme.terminal.opacity} onChange={(e) => edit({ ...theme, terminal: { ...theme.terminal, opacity: Number(e.target.value) } })} />
              <span>{theme.terminal.opacity}%</span>
            </label>
            <label className="row">
              <input type="checkbox" checked={theme.terminal.acrylic} onChange={(e) => edit({ ...theme, terminal: { ...theme.terminal, acrylic: e.target.checked } })} />
              {t("editor.font.acrylic")}
            </label>
            <label>
              {t("editor.font.paddingX")}
              <input type="number" min={0} max={40} value={theme.terminal.padding[0] ?? 0} onChange={(e) => edit({ ...theme, terminal: { ...theme.terminal, padding: [clamp(Number(e.target.value), 0, 40), theme.terminal.padding[1] ?? 0] } })} />
            </label>
            <label>
              {t("editor.font.paddingY")}
              <input type="number" min={0} max={40} value={theme.terminal.padding[1] ?? 0} onChange={(e) => edit({ ...theme, terminal: { ...theme.terminal, padding: [theme.terminal.padding[0] ?? 0, clamp(Number(e.target.value), 0, 40)] } })} />
            </label>
          </fieldset>

          <fieldset>
            <legend>{t("editor.prompt.title")}</legend>
            <div role="group" aria-label={t("editor.prompt.layout")}>
              {(["box", "line", "powerline"] as Layout[]).map((layout) => (
                <label className="row" key={layout}>
                  <input type="radio" name="layout" checked={theme.prompt.layout === layout} onChange={() => setLayout(layout)} />
                  {t(`editor.prompt.layout.${layout}`)}
                </label>
              ))}
            </div>
            <h3>{t("editor.prompt.segments")}</h3>
            <ul className="plain-list segments">
              {theme.prompt.segments.map((segment, i) => (
                <li key={segment}>
                  <label className="row">
                    <input type="checkbox" checked onChange={() => toggleSegment(segment, false)} />
                    {t(`editor.prompt.segment.${segment}`)}
                  </label>
                  <button type="button" disabled={i === 0} aria-label={t("editor.prompt.moveUp", { segment: t(`editor.prompt.segment.${segment}`) })} onClick={() => moveSegment(segment, -1)}>
                    ↑
                  </button>
                  <button
                    type="button"
                    disabled={i === theme.prompt.segments.length - 1}
                    aria-label={t("editor.prompt.moveDown", { segment: t(`editor.prompt.segment.${segment}`) })}
                    onClick={() => moveSegment(segment, 1)}
                  >
                    ↓
                  </button>
                </li>
              ))}
              {SEGMENTS.filter((s) => !theme.prompt.segments.includes(s)).map((segment) => (
                <li key={segment}>
                  <label className="row">
                    <input type="checkbox" checked={false} onChange={() => toggleSegment(segment, true)} />
                    {t(`editor.prompt.segment.${segment}`)}
                  </label>
                </li>
              ))}
            </ul>
            <label className="row">
              <input type="checkbox" checked={theme.prompt.transient} onChange={(e) => edit({ ...theme, prompt: { ...theme.prompt, transient: e.target.checked } })} />
              {t("editor.prompt.transient")}
            </label>
          </fieldset>

          <fieldset>
            <legend>{t("editor.banner.title")}</legend>
            <label className="row">
              <input type="checkbox" checked={theme.banner.enabled} onChange={(e) => edit({ ...theme, banner: { ...theme.banner, enabled: e.target.checked } })} />
              {t("editor.banner.enabled")}
            </label>
            <label>
              {t("editor.banner.titleField")}
              <input type="text" value={theme.banner.title} maxLength={80} onChange={(e) => edit({ ...theme, banner: { ...theme.banner, title: e.target.value } })} />
            </label>
            <p>
              <small>{t("editor.banner.variables")}</small>
            </p>
            <div>{t("editor.banner.lines")}</div>
            {[0, 1, 2, 3].map((i) => (
              <label key={i}>
                {t("editor.banner.line", { n: i + 1 })}
                <input
                  type="text"
                  value={theme.banner.lines[i] ?? ""}
                  maxLength={80}
                  onChange={(e) => {
                    const lines = [...theme.banner.lines];
                    while (lines.length <= i) lines.push("");
                    lines[i] = e.target.value;
                    while (lines.length > 0 && lines[lines.length - 1] === "") lines.pop();
                    edit({ ...theme, banner: { ...theme.banner, lines } });
                  }}
                />
              </label>
            ))}
            <label className="row">
              <input type="checkbox" checked={theme.banner.tips} onChange={(e) => edit({ ...theme, banner: { ...theme.banner, tips: e.target.checked } })} />
              {t("editor.banner.tips")}
            </label>
            <label className="row">
              <input type="checkbox" checked={theme.banner.clearLogo} onChange={(e) => edit({ ...theme, banner: { ...theme.banner, clearLogo: e.target.checked } })} />
              {t("editor.banner.clearLogo")}
            </label>
          </fieldset>
        </div>

        <div className="preview-column">
          <h2 style={{ marginTop: 0 }}>{t("editor.preview.title")}</h2>
          <TerminalPreview theme={theme} result={preview} />
          <label>
            {t("editor.preview.user")}
            <input type="text" value={sampleUser} maxLength={64} onChange={(e) => setSampleUser(e.target.value)} />
          </label>
          <label>
            {t("editor.preview.folder")}
            <input type="text" value={sampleCwd} maxLength={200} onChange={(e) => setSampleCwd(e.target.value)} />
          </label>
          <label>
            {t("editor.preview.width")}
            <input type="number" min={20} max={200} value={sampleWidth} onChange={(e) => setSampleWidth(clamp(Number(e.target.value), 20, 200))} />
          </label>
          <span lang={language} />
        </div>
      </div>
      {applying && (
        <Dialog title={t("apply.planTitle")} onClose={() => setApplying(null)}>
          <ApplyFlow
            titled={false}
            request={applying}
            onCancel={() => setApplying(null)}
            onApplied={(o) => {
              setLastApply(o);
              bump();
            }}
          />
        </Dialog>
      )}
    </section>
  );
}

function clamp(n: number, min: number, max: number): number {
  if (Number.isNaN(n)) return min;
  return Math.min(max, Math.max(min, n));
}

// The reducer's action type is re-exported for the tests of this screen.
export type { EditorAction };

/** The reducer before a theme is loaded: only `load` makes a state. */
function reduceNullable(state: EditorState | null, action: EditorAction): EditorState | null {
  if (state === null) return action.type === "load" ? reduce(initialState(action.theme), action) : null;
  return reduce(state, action);
}
```

- [ ] **Step 4: Write the interim `app/ui/src/App.tsx`**

```tsx
// The shell as the themes plan leaves it: the settings loaded once, English unless the saved
// language is Thai, the Themes page and the editor behind the sidebar, and the after-apply bar
// (spec §4.4). The wizard, the home page, Settings, Help and Advanced come with the next plan,
// which replaces this file.
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { ApplyOutcome } from "./bindings/ApplyOutcome";
import type { AppSettings } from "./bindings/AppSettings";
import { ErrorNotice, Notice } from "./components/Notices";
import { I18nProvider, useI18n, type Language } from "./i18n";
import { useIpc } from "./ipc/context";
import { NavContext, useNav, type Route } from "./router";
import { Editor } from "./screens/Editor";
import { Themes } from "./screens/Themes";
import { AppStateContext, DEFAULT_SETTINGS, useAppState } from "./state";

export function App() {
  const ipc = useIpc();
  const [language, setLanguage] = useState<Language>("en");
  const [settings, setSettings] = useState<AppSettings>(DEFAULT_SETTINGS);
  const [route, setRoute] = useState<Route | null>(null);
  const [lastApply, setLastApply] = useState<ApplyOutcome | null>(null);
  const [version, setVersion] = useState(0);
  const [startError, setStartError] = useState<unknown>(null);
  const [resetFrom, setResetFrom] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    ipc
      .settingsGet()
      .then((loaded) => {
        if (!alive) return;
        setSettings(loaded.settings);
        setLanguage(loaded.settings.language);
        setResetFrom(loaded.resetFrom);
        setRoute({ name: "themes" });
      })
      .catch((e: unknown) => {
        if (!alive) return;
        setStartError(e);
        setRoute({ name: "themes" });
      });
    return () => {
      alive = false;
    };
  }, [ipc]);

  const saveSettings = useCallback(
    async (next: AppSettings) => {
      const saved = await ipc.settingsSet(next);
      setSettings(saved);
      setLanguage(saved.language);
    },
    [ipc],
  );

  const bump = useCallback(() => setVersion((v) => v + 1), []);
  const appState = useMemo(
    () => ({ settings, saveSettings, lastApply, setLastApply, version, bump }),
    [settings, saveSettings, lastApply, version, bump],
  );
  const nav = useMemo(() => ({ route: route ?? { name: "themes" as const }, go: (r: Route) => setRoute(r) }), [route]);

  return (
    <I18nProvider language={language} setLanguage={setLanguage}>
      <AppStateContext.Provider value={appState}>
        <NavContext.Provider value={nav}>
          <div lang={language}>
            {route === null ? (
              <p role="status" style={{ padding: "2rem" }}>
                …
              </p>
            ) : (
              <Shell startError={startError} resetFrom={resetFrom} />
            )}
          </div>
        </NavContext.Provider>
      </AppStateContext.Provider>
    </I18nProvider>
  );
}

function Shell({ startError, resetFrom }: { startError: unknown; resetFrom: string | null }) {
  const { t } = useI18n();
  const { route, go } = useNav();
  const heading = useRef<HTMLDivElement>(null);

  useEffect(() => {
    heading.current?.focus();
  }, [route]);

  return (
    <>
      <a className="skip-link" href="#content">
        {t("a11y.skipToContent")}
      </a>
      <AfterApplyBar />
      <div className="shell">
        <nav className="sidebar" aria-label={t("a11y.mainMenu")}>
          <div className="brand">{t("app.name")}</div>
          <button type="button" aria-current="page" onClick={() => go({ name: "themes" })}>
            {t("nav.themes")}
          </button>
        </nav>
        <main id="content">
          {startError !== null && <ErrorNotice error={startError} />}
          {resetFrom && <Notice kind="warn">{t("settings.reset", { file: resetFrom })}</Notice>}
          <div ref={heading} tabIndex={-1} className="visually-hidden">
            {t("app.name")}
          </div>
          {route.name === "editor" ? <Editor id={route.id} /> : <Themes />}
        </main>
      </div>
    </>
  );
}

/** Spec §4.4: after every apply, "Theme X applied · open a new cmd window…" with Undo. */
function AfterApplyBar() {
  const ipc = useIpc();
  const { t } = useI18n();
  const { lastApply, setLastApply, bump } = useAppState();
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<unknown>(null);
  if (!lastApply || lastApply.status !== "committed") return null;

  const undo = async () => {
    setError(null);
    try {
      const outcome = await ipc.undoLast();
      setMessage(outcome.status === "nothingToUndo" ? t("apply.undoNothing") : t("apply.undone"));
      if (outcome.status === "committed") {
        setLastApply(null);
        bump();
      } else if (outcome.error) {
        setError(outcome.error);
      }
    } catch (e) {
      setError(e);
    }
  };

  return (
    <div className="bar" role="status">
      <span>{t("apply.done", { name: lastApply.themeName })}</span>
      <button type="button" onClick={() => void undo()}>
        {t("apply.undo")}
      </button>
      <button type="button" className="link" onClick={() => setLastApply(null)}>
        {t("common.dismiss")}
      </button>
      {message && <span>{message}</span>}
      {error !== null && <ErrorNotice error={error} />}
    </div>
  );
}
```

- [ ] **Step 5: Write `app/ui/src/main.tsx`**

```tsx
// Mounts the app. In the Tauri window the IPC is Tauri's; `VITE_MOCK_IPC=1` (the visual check in
// a plain browser, never in the shipped app) swaps in the in-memory mock through a dynamic
// import, so the mock and the preset files stay out of the production bundle.
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { IpcProvider } from "./ipc/context";
import type { Ipc } from "./ipc/types";
import "./styles.css";

async function pickIpc(): Promise<Ipc> {
  if (import.meta.env.VITE_MOCK_IPC === "1") {
    const { createMockIpc } = await import("./ipc/mock");
    return createMockIpc();
  }
  const { tauriIpc } = await import("./ipc/tauri");
  return tauriIpc;
}

const root = document.getElementById("root");
if (root) {
  void pickIpc().then((ipc) => {
    createRoot(root).render(
      <StrictMode>
        <IpcProvider ipc={ipc}>
          <App />
        </IpcProvider>
      </StrictMode>,
    );
  });
}
```

- [ ] **Step 6: Run the tests**

Run: `npm test`
Expected: `Test Files 4 passed`, `Tests 20 passed` (7 + 2 + 5 + 6). `Themes.test` shows: the twelve "Built in" cards; Customize makes "My Terracotta" and opens the editor with the name filled; Advanced mode lists "Frame" and "Bright white"; Save says "Saved."; the Light filter hides Terracotta and keeps Paper; the plan dialog shows "Update the prompt and the welcome box" and "Use this theme in every cmd window" before anything is applied; after Apply the card says "In use" and the bar's Undo takes it back; another add-on is asked about; a failed apply is said with `Code E_VERIFY_OMP` and "Copy details for a problem report"; New theme opens in Simple mode with a draft `{accent "#d97757", tint 0.1}`, Powerline switches the font to CaskaydiaCove NF and says so; Import previews, then adds "Imported theme".
Run: `npm run typecheck` → clean. `npm run build` → `ui/dist/` written (ignored); the output lists one JS chunk of roughly 330 KB and no `mock` or `.cmdtheme` content in it (`Select-String -Path ui\dist\assets\*.js -Pattern "Terracotta" -Quiet` → `False`).

- [ ] **Step 7: The visual check in the browser pane** (the dev server with the mock; nothing reaches Tauri or the machine)

Run (in `app/`, in a terminal you keep open): `$env:VITE_MOCK_IPC = "1"; npm run dev` — Vite serves `http://localhost:1420`. Open it in the built-in browser pane (never in the Tauri window). Look at, and note in the task's report:
- the Themes page: twelve cards with swatches, "Built in" badges, the filter buttons, "New theme" and "Import a theme file"; light and dark follow the OS (the pane's color scheme toggle, if it has one, or the OS setting);
- Customize on Terracotta → the editor: the preview paints the welcome box and the prompt in the theme's colors with no stray empty box; Advanced mode's 23 color inputs; the readability panel reads "Readable" for the presets; changing the layout to Powerline shows the Nerd Font notice; Ctrl+Z after a name edit restores the name; Tab reaches every control and the focus ring is visible;
- Apply from a card: the dialog "What will change", Apply, the progress text, "Theme … applied", the bar with Undo; Escape closes the dialog and focus returns to the card's button;
- the browser console: no error, no CSP message (Vite's dev page has no CSP; the CSP check is on the VM).
Stop the server (`Ctrl+C`) and `Remove-Item Env:VITE_MOCK_IPC`. Fix only what the check shows is broken; a cosmetic preference is a note for the founder, not a change.

- [ ] **Step 8: Commit**

Stage `app/ui/src/screens/Themes.tsx app/ui/src/screens/Editor.tsx app/ui/src/App.tsx app/ui/src/main.tsx app/ui/src/Themes.test.tsx`. Message: `ui: the theme library and the editor (spec §3.1, §3.4) behind an interim shell; tests against the mock`.

---

### Task 7: CI's `app` job builds the UI and the app

The shell plan's Rust-only job becomes the full one: the pinned Node from `.nvmrc`, `npm ci`, type-check, tests, build, then the bindings check, clippy and tests of the app crate, and `tauri build --no-bundle` (the Tauri CLI runs `beforeBuildCommand` = `npm run build`, then the release build; Phase 4 turns bundling on). The exe is uploaded as an artifact for the phase 3 plan's VM check.

**Files:**
- Modify: `.github/workflows/ci.yml` (replace the `app` job)

- [ ] **Step 1: Replace the `app` job** — from the line `  app:` to the end of the job the shell plan's Task 15 added, with:

```yaml
  # The app (Phase 2b): the UI's checks, the bindings' freshness, the app crate, and the Tauri
  # build. Separate from the Rust job so a UI failure and a Rust failure read apart.
  app:
    runs-on: windows-latest
    timeout-minutes: 60
    env:
      CARGO_TERM_COLOR: always
    steps:
      - uses: actions/checkout@v4
      - name: Install the toolchain pinned in rust-toolchain.toml
        shell: bash
        run: |
          rustup --version
          rustup toolchain install
          cargo --version
      - name: Install the Node pinned in app/.nvmrc
        uses: actions/setup-node@v4
        with:
          node-version-file: app/.nvmrc
          cache: npm
          cache-dependency-path: app/package-lock.json
      - name: Install the UI's packages exactly as locked
        working-directory: app
        run: npm ci
      - name: Type-check the UI
        working-directory: app
        run: npm run typecheck
      - name: UI tests (vitest, against the mocked IPC)
        working-directory: app
        run: npm test
      - name: Build the UI
        working-directory: app
        run: npm run build
      - name: Cache the pinned runtimes and font
        uses: actions/cache@v4
        with:
          path: resources
          key: resources-${{ runner.os }}-${{ hashFiles('scripts/fetch-resources.ps1') }}
      - name: Fetch the pinned runtimes and font
        shell: powershell
        run: ./scripts/fetch-resources.ps1
      # The committed bindings must be what the Rust types export now (G11). The script runs the
      # core's and the app's export tests, then git shows any difference.
      - name: Bindings are fresh
        shell: bash
        run: |
          pwsh -File ./scripts/export-bindings.ps1
          if ! git diff --exit-code -- app/ui/src/bindings; then
            echo "::error::app/ui/src/bindings is stale: run scripts/export-bindings.ps1 and commit"
            exit 1
          fi
      - name: Clippy of the app crate
        run: cargo clippy -p cmdstudio-app --all-targets -- -D warnings
      - name: Tests of the app crate (backend over the in-memory machine)
        run: cargo test -p cmdstudio-app
      # The Tauri pipeline end to end (config, icon, capabilities, codegen, the built UI) in
      # release, without bundling: Phase 4 turns the bundle on.
      - name: Tauri build (no bundle)
        working-directory: app
        run: npx tauri build --no-bundle
      # For the VM check of spec §1 criterion 1 (phase 3 plan, Task 4): the exe and the
      # resources it expects next to it.
      - uses: actions/upload-artifact@v4
        with:
          name: cmdstudio-exe
          path: |
            target/release/cmdstudio.exe
            resources/
          if-no-files-found: error
```

- [ ] **Step 2: Check the file and commit**

Run (in `<repo>`): `git diff -- .github/workflows/ci.yml` shows the job replaced and nothing else; the YAML indents by two spaces under `jobs:`. Stage `.github/workflows/ci.yml`. Message: `ci: the app job runs npm ci, typecheck, tests, build, the bindings check and tauri build --no-bundle`. The controller pushes; the job must be green before the phase 3 plan starts (it is this plan's gate for `ipc.rs`, `lib.rs`, `main.rs` and the Tauri config, which were not compiled on the dev PC).

---

## Appendix A: `app/package-lock.json`

Written verbatim by Task 1 (`lockfileVersion` 3; 134 package entries; generated by `npm ci` of the exact versions of `package.json` on 2026-10-16).

```json
{
  "name": "cmd-studio-app",
  "version": "0.1.0",
  "lockfileVersion": 3,
  "requires": true,
  "packages": {
    "": {
      "name": "cmd-studio-app",
      "version": "0.1.0",
      "dependencies": {
        "@tauri-apps/api": "2.12.4",
        "react": "19.3.1",
        "react-dom": "19.3.1"
      },
      "devDependencies": {
        "@tauri-apps/cli": "2.12.3",
        "@testing-library/react": "16.3.3",
        "@testing-library/user-event": "14.6.8",
        "@types/node": "24.19.2",
        "@types/react": "19.3.0",
        "@types/react-dom": "19.3.0",
        "@vitejs/plugin-react": "6.1.2",
        "jsdom": "30.1.3",
        "typescript": "5.9.3",
        "vite": "8.3.4",
        "vitest": "5.0.5"
      },
      "engines": {
        "node": ">=24 <25"
      }
    },
    "node_modules/@asamuzakjp/css-color": {
      "version": "7.1.6",
      "resolved": "https://registry.npmjs.org/@asamuzakjp/css-color/-/css-color-7.1.6.tgz",
      "integrity": "sha512-jZJUZX0mR6JqnjQGb79yTYbR5WXmACmcLoFSgZk+7W+0qMJ8jWGRp9ncYvE1ihwL3dwbmfeCe6/O0rRwWTko4g==",
      "dev": true,
      "license": "MIT",
      "dependencies": {
        "@csstools/css-calc": "^3.4.3",
        "@csstools/css-color-parser": "^4.2.6",
        "@csstools/css-parser-algorithms": "^4.0.2",
        "@csstools/css-tokenizer": "^4.0.2",
        "lru-cache": "^11.5.3"
      },
      "engines": {
        "node": "^22.22.2 || ^24.15.0 || >=26.0.0"
      }
    },
    "node_modules/@asamuzakjp/dom-selector": {
      "version": "10.0.3",
      "resolved": "https://registry.npmjs.org/@asamuzakjp/dom-selector/-/dom-selector-10.0.3.tgz",
      "integrity": "sha512-3+777rEPsOOjoy5/sRmIzHnoRDaLpm3XWY2dmWn/N7f/XwDG0bcpnFaKL3X9FYJUbNwLmpcJC1bUYexJRxsFlw==",
      "dev": true,
      "license": "MIT",
      "dependencies": {
        "bidi-js": "^1.1.0",
        "css-tree": "^3.2.1",
        "is-potential-custom-element-name": "^1.0.1",
        "lru-cache": "^11.5.3"
      },
      "engines": {
        "node": "^22.22.2 || ^24.15.0 || >=26.0.0"
      }
    },
    "node_modules/@babel/code-frame": {
      "version": "7.29.7",
      "resolved": "https://registry.npmjs.org/@babel/code-frame/-/code-frame-7.29.7.tgz",
      "integrity": "sha512-Aup7aUOfpbAUg2ROOJN6Iw5f9DMBlzu0mIkm/malLQFN/YQgO48wCj0Kxa3sEHJvPVFg7siR+qRInwXd2qhQKw==",
      "dev": true,
      "license": "MIT",
      "peer": true,
      "dependencies": {
        "@babel/helper-validator-identifier": "^7.29.7",
        "js-tokens": "^4.0.0",
        "picocolors": "^1.1.1"
      },
      "engines": {
        "node": ">=6.9.0"
      }
    },
    "node_modules/@babel/helper-validator-identifier": {
      "version": "7.29.7",
      "resolved": "https://registry.npmjs.org/@babel/helper-validator-identifier/-/helper-validator-identifier-7.29.7.tgz",
      "integrity": "sha512-qehxGkRj55h/ff8EMaJ+cYhyaKlHIxqYDn682wQD7RNp9UujOQsHog2uS0r2vzr4pW+sXf90NeeayjcNaX3fFg==",
      "dev": true,
      "license": "MIT",
      "peer": true,
      "engines": {
        "node": ">=6.9.0"
      }
    },
    "node_modules/@babel/runtime": {
      "version": "7.29.10",
      "resolved": "https://registry.npmjs.org/@babel/runtime/-/runtime-7.29.10.tgz",
      "integrity": "sha512-NpugTzbKgGrNe45Ui32cN8vCFFMxVtP/9rtfwv0nP5FRDAjvYBxJcLVpYA9mr1jxP6cnIT7x9LUzuDTbWYYNRA==",
      "dev": true,
      "license": "MIT",
      "engines": {
        "node": ">=6.9.0"
      }
    },
    "node_modules/@bramus/specificity": {
      "version": "2.4.3",
      "resolved": "https://registry.npmjs.org/@bramus/specificity/-/specificity-2.4.3.tgz",
      "integrity": "sha512-0GEz8qJWokkUVYQYRukmkSHIFNoBH8QTreeHrAFbvz1XFYc3Y+puhMs8PQ5x8GNugiC2l5jvDBXKLbBIByeSBw==",
      "dev": true,
      "license": "MIT",
      "dependencies": {
        "css-tree": "^3.2.1"
      },
      "bin": {
        "specificity": "bin/cli.js"
      }
    },
    "node_modules/@csstools/color-helpers": {
      "version": "6.1.2",
      "resolved": "https://registry.npmjs.org/@csstools/color-helpers/-/color-helpers-6.1.2.tgz",
      "integrity": "sha512-grhRy3OKmniaAEKXMjua5z/EODX0MSqBGjunw8+j/3HQjOnahs2AGhvEOIYVUWcU6ScApbhLhVrQTX8XqrMrow==",
      "dev": true,
      "funding": [
        {
          "type": "github",
          "url": "https://github.com/sponsors/csstools"
        },
        {
          "type": "opencollective",
          "url": "https://opencollective.com/csstools"
        }
      ],
      "license": "MIT-0",
      "engines": {
        "node": ">=20.19.0"
      }
    },
    "node_modules/@csstools/css-calc": {
      "version": "3.4.3",
      "resolved": "https://registry.npmjs.org/@csstools/css-calc/-/css-calc-3.4.3.tgz",
      "integrity": "sha512-iex20d8CHVkyvg6B7UKV7uHnI2Bqo9g+EFfT9E0y+GvTvhZ/DwONJ+9aKb1dlqm0ZiGsL5RXjp0fCoJYnkeDjA==",
      "dev": true,
      "funding": [
        {
          "type": "github",
          "url": "https://github.com/sponsors/csstools"
        },
        {
          "type": "opencollective",
          "url": "https://opencollective.com/csstools"
        }
      ],
      "license": "MIT",
      "engines": {
        "node": ">=20.19.0"
      },
      "peerDependencies": {
        "@csstools/css-parser-algorithms": "^4.0.2",
        "@csstools/css-tokenizer": "^4.0.2"
      }
    },
    "node_modules/@csstools/css-color-parser": {
      "version": "4.2.6",
      "resolved": "https://registry.npmjs.org/@csstools/css-color-parser/-/css-color-parser-4.2.6.tgz",
      "integrity": "sha512-iiPQ3iRWwnJkeEn6RIu6SJPr7hYrLz6XZ9s/QZl+2/LI5KQVjpl2fdmDSZKuD4xP6GMmMPgHFFXg6k1Wkz0Trg==",
      "dev": true,
      "funding": [
        {
          "type": "github",
          "url": "https://github.com/sponsors/csstools"
        },
        {
          "type": "opencollective",
          "url": "https://opencollective.com/csstools"
        }
      ],
      "license": "MIT",
      "dependencies": {
        "@csstools/color-helpers": "^6.1.2",
        "@csstools/css-calc": "^3.4.3"
      },
      "engines": {
        "node": ">=20.19.0"
      },
      "peerDependencies": {
        "@csstools/css-parser-algorithms": "^4.0.2",
        "@csstools/css-tokenizer": "^4.0.2"
      }
    },
    "node_modules/@csstools/css-parser-algorithms": {
      "version": "4.0.2",
      "resolved": "https://registry.npmjs.org/@csstools/css-parser-algorithms/-/css-parser-algorithms-4.0.2.tgz",
      "integrity": "sha512-40cSKyMvK+tq4qz6Awrlye2WGuOKt3FwPgtGg6KTfbHOWNw+Rk1rzbAtZnZ6IBhsY491HLRnDXwoyBAijmmILA==",
      "dev": true,
      "funding": [
        {
          "type": "github",
          "url": "https://github.com/sponsors/csstools"
        },
        {
          "type": "opencollective",
          "url": "https://opencollective.com/csstools"
        }
      ],
      "license": "MIT",
      "engines": {
        "node": ">=20.19.0"
      },
      "peerDependencies": {
        "@csstools/css-tokenizer": "^4.0.2"
      }
    },
    "node_modules/@csstools/css-syntax-patches-for-csstree": {
      "version": "1.1.15",
      "resolved": "https://registry.npmjs.org/@csstools/css-syntax-patches-for-csstree/-/css-syntax-patches-for-csstree-1.1.15.tgz",
      "integrity": "sha512-J0u7HkVl2nzSlhsiTOp4AmwcUQ3D+mGEEKfBy/7To5/y7F2OHwyLrXfrhR0SMgr4p5Lo+eaMVSeai24zUcBIxA==",
      "dev": true,
      "funding": [
        {
          "type": "github",
          "url": "https://github.com/sponsors/csstools"
        },
        {
          "type": "opencollective",
          "url": "https://opencollective.com/csstools"
        }
      ],
      "license": "MIT-0",
      "peerDependencies": {
        "css-tree": "^3.2.1"
      },
      "peerDependenciesMeta": {
        "css-tree": {
          "optional": true
        }
      }
    },
    "node_modules/@csstools/css-tokenizer": {
      "version": "4.0.2",
      "resolved": "https://registry.npmjs.org/@csstools/css-tokenizer/-/css-tokenizer-4.0.2.tgz",
      "integrity": "sha512-OoKoR0f76dCY666JlcbhmVTs2drYj1GUXZTYTcbUgJjh9Nv41aFfZ21bPQTERm5+L5cBDo466NltB2lplS5GBw==",
      "dev": true,
      "funding": [
        {
          "type": "github",
          "url": "https://github.com/sponsors/csstools"
        },
        {
          "type": "opencollective",
          "url": "https://opencollective.com/csstools"
        }
      ],
      "license": "MIT",
      "engines": {
        "node": ">=20.19.0"
      }
    },
    "node_modules/@exodus/bytes": {
      "version": "1.16.0",
      "resolved": "https://registry.npmjs.org/@exodus/bytes/-/bytes-1.16.0.tgz",
      "integrity": "sha512-IcpW84uEn3N7ETtNZMlxKhfl6Pec8rUNGOTBtWbK1FKhJxIFAptZyVrvVRVBimAJxJCgc3PxepxkdWWG4DVzfA==",
      "dev": true,
      "license": "MIT",
      "engines": {
        "node": "^20.19.0 || ^22.12.0 || >=24.0.0"
      },
      "peerDependencies": {
        "@noble/hashes": "^1.8.0 || ^2.0.0"
      },
      "peerDependenciesMeta": {
        "@noble/hashes": {
          "optional": true
        }
      }
    },
    "node_modules/@jridgewell/resolve-uri": {
      "version": "3.1.2",
      "resolved": "https://registry.npmjs.org/@jridgewell/resolve-uri/-/resolve-uri-3.1.2.tgz",
      "integrity": "sha512-bRISgCIjP20/tbWSPWMEi54QVPRZExkuD9lJL+UIxUKtwVJA8wW1Trb1jMs1RFXo1CBTNZ/5hpC9QvmKWdopKw==",
      "dev": true,
      "license": "MIT",
      "engines": {
        "node": ">=6.0.0"
      }
    },
    "node_modules/@jridgewell/sourcemap-codec": {
      "version": "1.6.0",
      "resolved": "https://registry.npmjs.org/@jridgewell/sourcemap-codec/-/sourcemap-codec-1.6.0.tgz",
      "integrity": "sha512-T7jf+5zgsZHwNJ4lvQ7/aezbyk0nNX+zJVWpmHA7VYsEx7a7qr5Rg5IbtJFqkgze5Y2sruq1RUY8Q837Od7iFw==",
      "dev": true,
      "license": "MIT"
    },
    "node_modules/@jridgewell/trace-mapping": {
      "version": "0.3.31",
      "resolved": "https://registry.npmjs.org/@jridgewell/trace-mapping/-/trace-mapping-0.3.31.tgz",
      "integrity": "sha512-zzNR+SdQSDJzc8joaeP8QQoCQr8NuYx2dIIytl1QeBEZHJ9uW6hebsrYgbz8hJwUQao3TWCMtmfV8Nu1twOLAw==",
      "dev": true,
      "license": "MIT",
      "dependencies": {
        "@jridgewell/resolve-uri": "^3.1.0",
        "@jridgewell/sourcemap-codec": "^1.4.14"
      }
    },
    "node_modules/@oxc-project/types": {
      "version": "0.154.0",
      "resolved": "https://registry.npmjs.org/@oxc-project/types/-/types-0.154.0.tgz",
      "integrity": "sha512-KYOUGQFlLWR+s4QMV2Q9ko38TZm7EdO87Qm7rvZLqk/webse/cDjtyxKhg/47l0byCbs/k46LT8qc4PKjAikzA==",
      "dev": true,
      "license": "MIT",
      "funding": {
        "url": "https://github.com/sponsors/oxc-project"
      }
    },
    "node_modules/@rolldown/binding-android-arm-eabi": {
      "version": "1.2.14",
      "resolved": "https://registry.npmjs.org/@rolldown/binding-android-arm-eabi/-/binding-android-arm-eabi-1.2.14.tgz",
      "integrity": "sha512-aWR+XKH9+rW7m5fWq/5360J3mHycrx/o6xZM2DQFdJnTMdyP7/WzxPjhtLbVWdKo2y612/4U1bWqb3HQhiF7vg==",
      "cpu": [
        "arm"
      ],
      "dev": true,
      "license": "MIT",
      "optional": true,
      "os": [
        "android"
      ],
      "engines": {
        "node": "^20.19.0 || >=22.12.0"
      }
    },
    "node_modules/@rolldown/binding-android-arm64": {
      "version": "1.2.14",
      "resolved": "https://registry.npmjs.org/@rolldown/binding-android-arm64/-/binding-android-arm64-1.2.14.tgz",
      "integrity": "sha512-QiVxHofl6A8lQU7WAtOUY4QMVZXohxwz7WJJoeh8NBEUyKfIkMRKhYyq8az/1RypNiIXzNTtiS6KwHz0erxYRQ==",
      "cpu": [
        "arm64"
      ],
      "dev": true,
      "license": "MIT",
      "optional": true,
      "os": [
        "android"
      ],
      "engines": {
        "node": "^20.19.0 || >=22.12.0"
      }
    },
    "node_modules/@rolldown/binding-darwin-arm64": {
      "version": "1.2.14",
      "resolved": "https://registry.npmjs.org/@rolldown/binding-darwin-arm64/-/binding-darwin-arm64-1.2.14.tgz",
      "integrity": "sha512-lTQOzHuBIooh3jIr5XINQkjPGEuGy+Dsy0meXDr6Sjty0b99kYgGh4uBC/8mJX1T0kC8d6S0aPP1Y+bHkh6aBw==",
      "cpu": [
        "arm64"
      ],
      "dev": true,
      "license": "MIT",
      "optional": true,
      "os": [
        "darwin"
      ],
      "engines": {
        "node": "^20.19.0 || >=22.12.0"
      }
    },
    "node_modules/@rolldown/binding-darwin-x64": {
      "version": "1.2.14",
      "resolved": "https://registry.npmjs.org/@rolldown/binding-darwin-x64/-/binding-darwin-x64-1.2.14.tgz",
      "integrity": "sha512-hyMKEGatjmBYCs/Q4sbIcL/GqpFEqsX3wc+HYVQ/85A4rxc1o7rPm4DUI4GV4k/h0ITddwFSGpqQunI14GXjTA==",
      "cpu": [
        "x64"
      ],
      "dev": true,
      "license": "MIT",
      "optional": true,
      "os": [
        "darwin"
      ],
      "engines": {
        "node": "^20.19.0 || >=22.12.0"
      }
    },
    "node_modules/@rolldown/binding-freebsd-x64": {
      "version": "1.2.14",
      "resolved": "https://registry.npmjs.org/@rolldown/binding-freebsd-x64/-/binding-freebsd-x64-1.2.14.tgz",
      "integrity": "sha512-u371GbXoVXrEgP20PwzoGNOdAOqAMhzo19JzzY+V4YmmnC+DTOoqdbMu+O+54emM59Mik/UdrtWgcuEDKWRPKA==",
      "cpu": [
        "x64"
      ],
      "dev": true,
      "license": "MIT",
      "optional": true,
      "os": [
        "freebsd"
      ],
      "engines": {
        "node": "^20.19.0 || >=22.12.0"
      }
    },
    "node_modules/@rolldown/binding-linux-arm-gnueabihf": {
      "version": "1.2.14",
      "resolved": "https://registry.npmjs.org/@rolldown/binding-linux-arm-gnueabihf/-/binding-linux-arm-gnueabihf-1.2.14.tgz",
      "integrity": "sha512-BqwKsdZQf6EqVWr8fv2MrDYzAvOZ7+NYFSJESRGs87fkseEhs0TU9TXgRJTkZoRRkxTizyEg1LKUXfU3/78uqg==",
      "cpu": [
        "arm"
      ],
      "dev": true,
      "license": "MIT",
      "optional": true,
      "os": [
        "linux"
      ],
      "engines": {
        "node": "^20.19.0 || >=22.12.0"
      }
    },
    "node_modules/@rolldown/binding-linux-arm64-gnu": {
      "version": "1.2.14",
      "resolved": "https://registry.npmjs.org/@rolldown/binding-linux-arm64-gnu/-/binding-linux-arm64-gnu-1.2.14.tgz",
      "integrity": "sha512-SD9kY8M009x/7s57Z1dgUVlP5+uYBgKQK0Tkr1WIWngrtUJmRCW1RJFMY7QDla2khXhmg2i5Io7MUjcmZNZ/KQ==",
      "cpu": [
        "arm64"
      ],
      "dev": true,
      "libc": [
        "glibc"
      ],
      "license": "MIT",
      "optional": true,
      "os": [
        "linux"
      ],
      "engines": {
        "node": "^20.19.0 || >=22.12.0"
      }
    },
    "node_modules/@rolldown/binding-linux-arm64-musl": {
      "version": "1.2.14",
      "resolved": "https://registry.npmjs.org/@rolldown/binding-linux-arm64-musl/-/binding-linux-arm64-musl-1.2.14.tgz",
      "integrity": "sha512-BJ2CL19M6uuWesMgnqJzgxoI+N+eV7b0A9O9vLV42y+gitC41fNa8KElau1tHVzuLWX5GzEuxMPhAO0tQXQMkg==",
      "cpu": [
        "arm64"
      ],
      "dev": true,
      "libc": [
        "musl"
      ],
      "license": "MIT",
      "optional": true,
      "os": [
        "linux"
      ],
      "engines": {
        "node": "^20.19.0 || >=22.12.0"
      }
    },
    "node_modules/@rolldown/binding-linux-ppc64-gnu": {
      "version": "1.2.14",
      "resolved": "https://registry.npmjs.org/@rolldown/binding-linux-ppc64-gnu/-/binding-linux-ppc64-gnu-1.2.14.tgz",
      "integrity": "sha512-4+Kl8daPV8NDHaD7mfCcc1pRM+Qg7ab8DyMm5FG5WoZgv4h2i3KFN1CybYBNbQqmuYm7ftJLvmMI1eHYJ22S/Q==",
      "cpu": [
        "ppc64"
      ],
      "dev": true,
      "libc": [
        "glibc"
      ],
      "license": "MIT",
      "optional": true,
      "os": [
        "linux"
      ],
      "engines": {
        "node": "^20.19.0 || >=22.12.0"
      }
    },
    "node_modules/@rolldown/binding-linux-s390x-gnu": {
      "version": "1.2.14",
      "resolved": "https://registry.npmjs.org/@rolldown/binding-linux-s390x-gnu/-/binding-linux-s390x-gnu-1.2.14.tgz",
      "integrity": "sha512-V/h8ePq1fKDrmtFMz6xcCN3DKvuvMRrwwD1q0PkBeeo4DpK64biaKvqWGVxqCeYKqeRk7gE7/iKPxLCn08O97A==",
      "cpu": [
        "s390x"
      ],
      "dev": true,
      "libc": [
        "glibc"
      ],
      "license": "MIT",
      "optional": true,
      "os": [
        "linux"
      ],
      "engines": {
        "node": "^20.19.0 || >=22.12.0"
      }
    },
    "node_modules/@rolldown/binding-linux-x64-gnu": {
      "version": "1.2.14",
      "resolved": "https://registry.npmjs.org/@rolldown/binding-linux-x64-gnu/-/binding-linux-x64-gnu-1.2.14.tgz",
      "integrity": "sha512-WxalXzgrLVpKC93xfpaJ3gCXUWzeCr+FRCbDVD2mUAy4L6Vo3lNVz0epUEUpxPWJkLNYUwM4OSIikv36OJ9mLg==",
      "cpu": [
        "x64"
      ],
      "dev": true,
      "libc": [
        "glibc"
      ],
      "license": "MIT",
      "optional": true,
      "os": [
        "linux"
      ],
      "engines": {
        "node": "^20.19.0 || >=22.12.0"
      }
    },
    "node_modules/@rolldown/binding-linux-x64-musl": {
      "version": "1.2.14",
      "resolved": "https://registry.npmjs.org/@rolldown/binding-linux-x64-musl/-/binding-linux-x64-musl-1.2.14.tgz",
      "integrity": "sha512-3G3tnAmTpdk9jO3GQtyHXqeFD2SlecfTws/Y3M2mmj9nMBwwuLT4kur8sZKeu7ENUHSpVO3cIsm5A52LEWWwlg==",
      "cpu": [
        "x64"
      ],
      "dev": true,
      "libc": [
        "musl"
      ],
      "license": "MIT",
      "optional": true,
      "os": [
        "linux"
      ],
      "engines": {
        "node": "^20.19.0 || >=22.12.0"
      }
    },
    "node_modules/@rolldown/binding-openharmony-arm64": {
      "version": "1.2.14",
      "resolved": "https://registry.npmjs.org/@rolldown/binding-openharmony-arm64/-/binding-openharmony-arm64-1.2.14.tgz",
      "integrity": "sha512-qyr5XiNGxm2ytx17vvtcJ9nXn6v1gENETi503bJx3OGwZ1tSonj7BosxbMa/xeAfSl5iS9X6KjEWYXLu6v1L7Q==",
      "cpu": [
        "arm64"
      ],
      "dev": true,
      "license": "MIT",
      "optional": true,
      "os": [
        "openharmony"
      ],
      "engines": {
        "node": "^20.19.0 || >=22.12.0"
      }
    },
    "node_modules/@rolldown/binding-win32-arm64-msvc": {
      "version": "1.2.14",
      "resolved": "https://registry.npmjs.org/@rolldown/binding-win32-arm64-msvc/-/binding-win32-arm64-msvc-1.2.14.tgz",
      "integrity": "sha512-oA4o5o0S2yB199YyYXDzKUv1Z56Rc+mgOjwm7Z/vJ9Z8igiDPYvoOuDyo6g7AK/ZaF09nPhDKVnGBcYHO11GXA==",
      "cpu": [
        "arm64"
      ],
      "dev": true,
      "license": "MIT",
      "optional": true,
      "os": [
        "win32"
      ],
      "engines": {
        "node": "^20.19.0 || >=22.12.0"
      }
    },
    "node_modules/@rolldown/binding-win32-x64-msvc": {
      "version": "1.2.14",
      "resolved": "https://registry.npmjs.org/@rolldown/binding-win32-x64-msvc/-/binding-win32-x64-msvc-1.2.14.tgz",
      "integrity": "sha512-9mE94bf2GZUaR/fUSXCMpBoWkctu421Mi0ORpm4zqfbkjKep7RxzPFa5etcGs5h+2KApoWBOA/hAJa/7saGgEQ==",
      "cpu": [
        "x64"
      ],
      "dev": true,
      "license": "MIT",
      "optional": true,
      "os": [
        "win32"
      ],
      "engines": {
        "node": "^20.19.0 || >=22.12.0"
      }
    },
    "node_modules/@rolldown/pluginutils": {
      "version": "1.0.1",
      "resolved": "https://registry.npmjs.org/@rolldown/pluginutils/-/pluginutils-1.0.1.tgz",
      "integrity": "sha512-2j9bGt5Jh8hj+vPtgzPtl72j0yRxHAyumoo6TNfAjsLB04UtpSvPbPcDcBMxz7n+9CYB0c1GxQFxYRg2jimqGw==",
      "dev": true,
      "license": "MIT"
    },
    "node_modules/@tauri-apps/api": {
      "version": "2.12.4",
      "resolved": "https://registry.npmjs.org/@tauri-apps/api/-/api-2.12.4.tgz",
      "integrity": "sha512-pVmfis+4zT8H0Qhp3xPZx9Ju7kpzvA6rr/PSJPyuwfvucTQ8sVt61NYU9KMkK703UFlYadFmjKA4HhybOtN0gg==",
      "license": "Apache-2.0 OR MIT",
      "funding": {
        "type": "opencollective",
        "url": "https://opencollective.com/tauri"
      }
    },
    "node_modules/@tauri-apps/cli": {
      "version": "2.12.3",
      "resolved": "https://registry.npmjs.org/@tauri-apps/cli/-/cli-2.12.3.tgz",
      "integrity": "sha512-f7/LsY74SVb1ChmOPw1ajOpxii0GSzDAgmWHRci8JUYuuai04rXsSbFobpmB0sP2DdlvMD/B5ZotEn8GOGMRUg==",
      "dev": true,
      "license": "Apache-2.0 OR MIT",
      "bin": {
        "tauri": "tauri.js"
      },
      "engines": {
        "node": ">= 10"
      },
      "funding": {
        "type": "opencollective",
        "url": "https://opencollective.com/tauri"
      },
      "optionalDependencies": {
        "@tauri-apps/cli-darwin-arm64": "2.12.3",
        "@tauri-apps/cli-darwin-x64": "2.12.3",
        "@tauri-apps/cli-linux-arm-gnueabihf": "2.12.3",
        "@tauri-apps/cli-linux-arm64-gnu": "2.12.3",
        "@tauri-apps/cli-linux-arm64-musl": "2.12.3",
        "@tauri-apps/cli-linux-riscv64-gnu": "2.12.3",
        "@tauri-apps/cli-linux-x64-gnu": "2.12.3",
        "@tauri-apps/cli-linux-x64-musl": "2.12.3",
        "@tauri-apps/cli-win32-arm64-msvc": "2.12.3",
        "@tauri-apps/cli-win32-ia32-msvc": "2.12.3",
        "@tauri-apps/cli-win32-x64-msvc": "2.12.3"
      }
    },
    "node_modules/@tauri-apps/cli-darwin-arm64": {
      "version": "2.12.3",
      "resolved": "https://registry.npmjs.org/@tauri-apps/cli-darwin-arm64/-/cli-darwin-arm64-2.12.3.tgz",
      "integrity": "sha512-xmVNoJOOKzKLj7GLHXmXLimRby9M42aBIJa0sB80N7o24eKV5JiVmToKTRLVH9yhL05GZ/B6rSSOmbzNFJwBAA==",
      "cpu": [
        "arm64"
      ],
      "dev": true,
      "license": "Apache-2.0 OR MIT",
      "optional": true,
      "os": [
        "darwin"
      ],
      "engines": {
        "node": ">= 10"
      }
    },
    "node_modules/@tauri-apps/cli-darwin-x64": {
      "version": "2.12.3",
      "resolved": "https://registry.npmjs.org/@tauri-apps/cli-darwin-x64/-/cli-darwin-x64-2.12.3.tgz",
      "integrity": "sha512-URd+0usfmUQQyb5kNPiKt2OT8gp7oGU7QR+qOppB6e35mpVBFFVNhWc9oWb3p0u71fDJ09DpF4T6m9xYxoIB8g==",
      "cpu": [
        "x64"
      ],
      "dev": true,
      "license": "Apache-2.0 OR MIT",
      "optional": true,
      "os": [
        "darwin"
      ],
      "engines": {
        "node": ">= 10"
      }
    },
    "node_modules/@tauri-apps/cli-linux-arm-gnueabihf": {
      "version": "2.12.3",
      "resolved": "https://registry.npmjs.org/@tauri-apps/cli-linux-arm-gnueabihf/-/cli-linux-arm-gnueabihf-2.12.3.tgz",
      "integrity": "sha512-0kwUVx2LbmOoTRlwF1oDww7th4/NSluv4ZBQ6KRZsFKg4g0TlTcIiJ0VehqkQYUeC9xIT1Mk6XtF52mI155VSQ==",
      "cpu": [
        "arm"
      ],
      "dev": true,
      "license": "Apache-2.0 OR MIT",
      "optional": true,
      "os": [
        "linux"
      ],
      "engines": {
        "node": ">= 10"
      }
    },
    "node_modules/@tauri-apps/cli-linux-arm64-gnu": {
      "version": "2.12.3",
      "resolved": "https://registry.npmjs.org/@tauri-apps/cli-linux-arm64-gnu/-/cli-linux-arm64-gnu-2.12.3.tgz",
      "integrity": "sha512-gKSGDx6fx+HBLQmG6guChFdisbrCSdx9Yli+j5teOa64gm1CeM+C6NLru5Pj3ZkkEep05BmCFjxvX+cjqAycZQ==",
      "cpu": [
        "arm64"
      ],
      "dev": true,
      "libc": [
        "glibc"
      ],
      "license": "Apache-2.0 OR MIT",
      "optional": true,
      "os": [
        "linux"
      ],
      "engines": {
        "node": ">= 10"
      }
    },
    "node_modules/@tauri-apps/cli-linux-arm64-musl": {
      "version": "2.12.3",
      "resolved": "https://registry.npmjs.org/@tauri-apps/cli-linux-arm64-musl/-/cli-linux-arm64-musl-2.12.3.tgz",
      "integrity": "sha512-Yr59qMy/gcvAfBuFdCGlAmGiLsYWMFDbsP4bAI6iNBzcJTGrYWZGeiQhGDo29MQwfF/NUBqyM2qljXu5tZ6wjQ==",
      "cpu": [
        "arm64"
      ],
      "dev": true,
      "libc": [
        "musl"
      ],
      "license": "Apache-2.0 OR MIT",
      "optional": true,
      "os": [
        "linux"
      ],
      "engines": {
        "node": ">= 10"
      }
    },
    "node_modules/@tauri-apps/cli-linux-riscv64-gnu": {
      "version": "2.12.3",
      "resolved": "https://registry.npmjs.org/@tauri-apps/cli-linux-riscv64-gnu/-/cli-linux-riscv64-gnu-2.12.3.tgz",
      "integrity": "sha512-jKr0/REBMnLKC0OwPQZkyge0mKDaNZ0b/AbfGwVyCgFfwS730EVLfVDo3nt7MUu6jJaytRq08+xSj5qfy+U1+Q==",
      "cpu": [
        "riscv64"
      ],
      "dev": true,
      "libc": [
        "glibc"
      ],
      "license": "Apache-2.0 OR MIT",
      "optional": true,
      "os": [
        "linux"
      ],
      "engines": {
        "node": ">= 10"
      }
    },
    "node_modules/@tauri-apps/cli-linux-x64-gnu": {
      "version": "2.12.3",
      "resolved": "https://registry.npmjs.org/@tauri-apps/cli-linux-x64-gnu/-/cli-linux-x64-gnu-2.12.3.tgz",
      "integrity": "sha512-gTkEqJAGCMROGanxHnzaJzQ+/rXl39HWVg0/hDODS1wDWBGqoceqGC6Qm1+mQbyT11xnsB8mKu/gyVID/YpFIw==",
      "cpu": [
        "x64"
      ],
      "dev": true,
      "libc": [
        "glibc"
      ],
      "license": "Apache-2.0 OR MIT",
      "optional": true,
      "os": [
        "linux"
      ],
      "engines": {
        "node": ">= 10"
      }
    },
    "node_modules/@tauri-apps/cli-linux-x64-musl": {
      "version": "2.12.3",
      "resolved": "https://registry.npmjs.org/@tauri-apps/cli-linux-x64-musl/-/cli-linux-x64-musl-2.12.3.tgz",
      "integrity": "sha512-T2dTxL1svRym71fZtaW4pZYXbthbfFsEJr1NeOUg/p7laWXZbUt/8DMSZf8recu9f7+1F/Lv00HtzsEu6vDO9w==",
      "cpu": [
        "x64"
      ],
      "dev": true,
      "libc": [
        "musl"
      ],
      "license": "Apache-2.0 OR MIT",
      "optional": true,
      "os": [
        "linux"
      ],
      "engines": {
        "node": ">= 10"
      }
    },
    "node_modules/@tauri-apps/cli-win32-arm64-msvc": {
      "version": "2.12.3",
      "resolved": "https://registry.npmjs.org/@tauri-apps/cli-win32-arm64-msvc/-/cli-win32-arm64-msvc-2.12.3.tgz",
      "integrity": "sha512-eoiQwpEZWbpsVHcM9BAzqqe4TCHszvr3rsM7pUQ4ofLK0lF1i1d/jX0Qo5DYMqcrUtwQuDdDuOCbcT0Izf1VwQ==",
      "cpu": [
        "arm64"
      ],
      "dev": true,
      "license": "Apache-2.0 OR MIT",
      "optional": true,
      "os": [
        "win32"
      ],
      "engines": {
        "node": ">= 10"
      }
    },
    "node_modules/@tauri-apps/cli-win32-ia32-msvc": {
      "version": "2.12.3",
      "resolved": "https://registry.npmjs.org/@tauri-apps/cli-win32-ia32-msvc/-/cli-win32-ia32-msvc-2.12.3.tgz",
      "integrity": "sha512-kE+KUg6Dbbkgvcw0m99wJVIToFQzJqpMBAXah74WqxaLzssNwSTUccrHp6+QVzMhjQP330tvekFmnQF+0NbE6Q==",
      "cpu": [
        "ia32"
      ],
      "dev": true,
      "license": "Apache-2.0 OR MIT",
      "optional": true,
      "os": [
        "win32"
      ],
      "engines": {
        "node": ">= 10"
      }
    },
    "node_modules/@tauri-apps/cli-win32-x64-msvc": {
      "version": "2.12.3",
      "resolved": "https://registry.npmjs.org/@tauri-apps/cli-win32-x64-msvc/-/cli-win32-x64-msvc-2.12.3.tgz",
      "integrity": "sha512-L7wndWAgX2mLW7alswpL3d1R0P9FN2WYBgHaQkAYEBxHHGkY4vFX9CjcRQ1LAxDAMIJxrY7/aDziFJeChS84NQ==",
      "cpu": [
        "x64"
      ],
      "dev": true,
      "license": "Apache-2.0 OR MIT",
      "optional": true,
      "os": [
        "win32"
      ],
      "engines": {
        "node": ">= 10"
      }
    },
    "node_modules/@testing-library/dom": {
      "version": "10.4.2",
      "resolved": "https://registry.npmjs.org/@testing-library/dom/-/dom-10.4.2.tgz",
      "integrity": "sha512-yzr2S9HyAIdhz2/6qHgbs665Q7PKVcDF05vsOlHPxG1mo36gKVesdYVeDLnXgfjJ03CrKRk08knc6+E/9m8v2Q==",
      "dev": true,
      "license": "MIT",
      "peer": true,
      "dependencies": {
        "@babel/code-frame": "^7.10.4",
        "@babel/runtime": "^7.12.5",
        "@types/aria-query": "^5.0.1",
        "aria-query": "5.3.0",
        "dom-accessibility-api": "^0.5.9",
        "lz-string": "^1.5.0",
        "picocolors": "1.1.1",
        "pretty-format": "^27.0.2"
      },
      "engines": {
        "node": ">=18"
      }
    },
    "node_modules/@testing-library/react": {
      "version": "16.3.3",
      "resolved": "https://registry.npmjs.org/@testing-library/react/-/react-16.3.3.tgz",
      "integrity": "sha512-Uo193NgQbPMz6lrrhtRQQFcMC6Re/ELLFbbuVL30WDlZxlpZf9/lMHTAVxPRLw1q1iu9OJmR1c2BLiENRstdBg==",
      "dev": true,
      "license": "MIT",
      "dependencies": {
        "@babel/runtime": "^7.12.5"
      },
      "engines": {
        "node": ">=18"
      },
      "peerDependencies": {
        "@testing-library/dom": "^10.0.0",
        "@types/react": "^18.0.0 || ^19.0.0",
        "@types/react-dom": "^18.0.0 || ^19.0.0",
        "react": "^18.0.0 || ^19.0.0",
        "react-dom": "^18.0.0 || ^19.0.0"
      },
      "peerDependenciesMeta": {
        "@types/react": {
          "optional": true
        },
        "@types/react-dom": {
          "optional": true
        }
      }
    },
    "node_modules/@testing-library/user-event": {
      "version": "14.6.8",
      "resolved": "https://registry.npmjs.org/@testing-library/user-event/-/user-event-14.6.8.tgz",
      "integrity": "sha512-vgSSqnANMnkcYR/vUNDbwS7aM3OErRLNUG8vCVVlj4oc6cKffUrccX7UAS5AmJQQNZTSibi9z5GvvRZ0aYgUCQ==",
      "dev": true,
      "license": "MIT",
      "engines": {
        "node": ">=12",
        "npm": ">=6"
      },
      "peerDependencies": {
        "@testing-library/dom": ">=7.21.4"
      }
    },
    "node_modules/@types/aria-query": {
      "version": "5.0.4",
      "resolved": "https://registry.npmjs.org/@types/aria-query/-/aria-query-5.0.4.tgz",
      "integrity": "sha512-rfT93uj5s0PRL7EzccGMs3brplhcrghnDoV26NqKhCAS1hVo+WdNsPvE/yb6ilfr5hi2MEk6d5EWJTKdxg8jVw==",
      "dev": true,
      "license": "MIT",
      "peer": true
    },
    "node_modules/@types/chai": {
      "version": "5.2.3",
      "resolved": "https://registry.npmjs.org/@types/chai/-/chai-5.2.3.tgz",
      "integrity": "sha512-Mw558oeA9fFbv65/y4mHtXDs9bPnFMZAL/jxdPFUpOHHIXX91mcgEHbS5Lahr+pwZFR8A7GQleRWeI6cGFC2UA==",
      "dev": true,
      "license": "MIT",
      "dependencies": {
        "@types/deep-eql": "*",
        "assertion-error": "^2.0.1"
      }
    },
    "node_modules/@types/deep-eql": {
      "version": "4.0.2",
      "resolved": "https://registry.npmjs.org/@types/deep-eql/-/deep-eql-4.0.2.tgz",
      "integrity": "sha512-c9h9dVVMigMPc4bwTvC5dxqtqJZwQPePsWjPlpSOnojbor6pGqdk541lfA7AqFQr5pB1BRdq0juY9db81BwyFw==",
      "dev": true,
      "license": "MIT"
    },
    "node_modules/@types/estree": {
      "version": "1.0.9",
      "resolved": "https://registry.npmjs.org/@types/estree/-/estree-1.0.9.tgz",
      "integrity": "sha512-GhdPgy1el4/ImP05X05Uw4cw2/M93BCUmnEvWZNStlCzEKME4Fkk+YpoA5OiHNQmoS7Cafb8Xa3Pya8m1Qrzeg==",
      "dev": true,
      "license": "MIT"
    },
    "node_modules/@types/node": {
      "version": "24.19.2",
      "resolved": "https://registry.npmjs.org/@types/node/-/node-24.19.2.tgz",
      "integrity": "sha512-93d49wCWJkCyN87MxLpNN6KXPCwFw7bVUg79mGUV1OS40JGWwVCr8n61DXCSqz7CV6/VfPkb7xec7uP2M+QHWQ==",
      "dev": true,
      "license": "MIT",
      "dependencies": {
        "undici-types": ">=7.24.0 <7.24.7"
      }
    },
    "node_modules/@types/react": {
      "version": "19.3.0",
      "resolved": "https://registry.npmjs.org/@types/react/-/react-19.3.0.tgz",
      "integrity": "sha512-N0rFCuH9YoxG9/m61l9MfpJKfmLOVU0em7ipIz6TRgSSkvReLB9vL85GB+yr8Bs5leqpvg96JSwF4ZS1s4viQg==",
      "dev": true,
      "license": "MIT",
      "dependencies": {
        "csstype": "^3.2.2"
      }
    },
    "node_modules/@types/react-dom": {
      "version": "19.3.0",
      "resolved": "https://registry.npmjs.org/@types/react-dom/-/react-dom-19.3.0.tgz",
      "integrity": "sha512-ZI7bU42mZXXKHn/qNLEw2IrbiINU7X5+vfgdixBHkCNpYWXjKgfQ/P+uyGb5CjOLB9UcnTeg3rylQtV2hym44Q==",
      "dev": true,
      "license": "MIT",
      "peerDependencies": {
        "@types/react": "^19.3.0"
      }
    },
    "node_modules/@vitejs/plugin-react": {
      "version": "6.1.2",
      "resolved": "https://registry.npmjs.org/@vitejs/plugin-react/-/plugin-react-6.1.2.tgz",
      "integrity": "sha512-fhJNGUdscQa4HkI0xFxwonWyhnVReCPDbXI7FH7jmzpnVCrFQI7PRrhWecO263610vOrs9tr2Y4xcf8uAdFxbw==",
      "dev": true,
      "license": "MIT",
      "dependencies": {
        "@rolldown/pluginutils": "^1.0.1"
      },
      "engines": {
        "node": "^20.19.0 || >=22.12.0"
      },
      "peerDependencies": {
        "@rolldown/plugin-babel": "^0.1.7 || ^0.2.0",
        "babel-plugin-react-compiler": "^1.0.0",
        "oxc-transform-react": "^0.152.0",
        "vite": "^8.0.0"
      },
      "peerDependenciesMeta": {
        "@rolldown/plugin-babel": {
          "optional": true
        },
        "babel-plugin-react-compiler": {
          "optional": true
        },
        "oxc-transform-react": {
          "optional": true
        }
      }
    },
    "node_modules/@vitest/mocker": {
      "version": "5.0.5",
      "resolved": "https://registry.npmjs.org/@vitest/mocker/-/mocker-5.0.5.tgz",
      "integrity": "sha512-xLL3lkDegJZw+MGoGaCCsFayDZthGgpAtr3Vziyfc4p7xH89K7KRPcRBBh/kEYm0Aqkt5DtTmkYpBtRV/FrHZg==",
      "dev": true,
      "license": "MIT",
      "dependencies": {
        "@jridgewell/trace-mapping": "0.3.31",
        "@vitest/spy": "5.0.5",
        "estree-walker": "^3.0.3",
        "magic-string": "^1.4.3"
      },
      "funding": {
        "url": "https://opencollective.com/vitest"
      },
      "peerDependencies": {
        "msw": "^2.4.9 || ^3.0.0",
        "vite": "^6.0.0 || ^7.0.0 || ^8.0.0"
      },
      "peerDependenciesMeta": {
        "msw": {
          "optional": true
        },
        "vite": {
          "optional": true
        }
      }
    },
    "node_modules/@vitest/spy": {
      "version": "5.0.5",
      "resolved": "https://registry.npmjs.org/@vitest/spy/-/spy-5.0.5.tgz",
      "integrity": "sha512-lwJRExuH417257Q5wv9QYugN9IMJkoxcrKrYxGPQkBzl7wlNpTUxD/5yNX4ESKVUIShKPgkGchYPFMWOOk0wzw==",
      "dev": true,
      "license": "MIT",
      "funding": {
        "url": "https://opencollective.com/vitest"
      }
    },
    "node_modules/ansi-regex": {
      "version": "5.0.1",
      "resolved": "https://registry.npmjs.org/ansi-regex/-/ansi-regex-5.0.1.tgz",
      "integrity": "sha512-quJQXlTSUGL2LH9SUXo8VwsY4soanhgo6LNSm84E1LBcE8s3O0wpdiRzyR9z/ZZJMlMWv37qOOb9pdJlMUEKFQ==",
      "dev": true,
      "license": "MIT",
      "peer": true,
      "engines": {
        "node": ">=8"
      }
    },
    "node_modules/ansi-styles": {
      "version": "5.2.0",
      "resolved": "https://registry.npmjs.org/ansi-styles/-/ansi-styles-5.2.0.tgz",
      "integrity": "sha512-Cxwpt2SfTzTtXcfOlzGEee8O+c+MmUgGrNiBcXnuWxuFJHe6a5Hz7qwhwe5OgaSYI0IJvkLqWX1ASG+cJOkEiA==",
      "dev": true,
      "license": "MIT",
      "peer": true,
      "engines": {
        "node": ">=10"
      },
      "funding": {
        "url": "https://github.com/chalk/ansi-styles?sponsor=1"
      }
    },
    "node_modules/aria-query": {
      "version": "5.3.0",
      "resolved": "https://registry.npmjs.org/aria-query/-/aria-query-5.3.0.tgz",
      "integrity": "sha512-b0P0sZPKtyu8HkeRAfCq0IfURZK+SuwMjY1UXGBU27wpAiTwQAIlq56IbIO+ytk/JjS1fMR14ee5WBBfKi5J6A==",
      "dev": true,
      "license": "Apache-2.0",
      "peer": true,
      "dependencies": {
        "dequal": "^2.0.3"
      }
    },
    "node_modules/assertion-error": {
      "version": "2.0.1",
      "resolved": "https://registry.npmjs.org/assertion-error/-/assertion-error-2.0.1.tgz",
      "integrity": "sha512-Izi8RQcffqCeNVgFigKli1ssklIbpHnCYc6AknXGYoB6grJqyeby7jv12JUQgmTAnIDnbck1uxksT4dzN3PWBA==",
      "dev": true,
      "license": "MIT",
      "engines": {
        "node": ">=12"
      }
    },
    "node_modules/bidi-js": {
      "version": "1.1.0",
      "resolved": "https://registry.npmjs.org/bidi-js/-/bidi-js-1.1.0.tgz",
      "integrity": "sha512-fX1Onk0tdVPC7obPWB5EbJ1z7NVhLq4m2xZLq2YXBkxzMXIGRpNMU88n0EPgWseKl12J7zXs7qrDxPK4sRs2fg==",
      "dev": true,
      "license": "MIT",
      "dependencies": {
        "require-from-string": "^2.0.2"
      }
    },
    "node_modules/chai": {
      "version": "6.3.0",
      "resolved": "https://registry.npmjs.org/chai/-/chai-6.3.0.tgz",
      "integrity": "sha512-XWAtwJ6OHO+tj0EKCs0Y2UamnyOxseZWltU4x2U2wh8g4AigdjwvtUjvLP2tqkA/avxHEtzxNaqGq/YGNwckKg==",
      "dev": true,
      "license": "MIT",
      "engines": {
        "node": ">=18"
      }
    },
    "node_modules/css-tree": {
      "version": "3.2.1",
      "resolved": "https://registry.npmjs.org/css-tree/-/css-tree-3.2.1.tgz",
      "integrity": "sha512-X7sjQzceUhu1u7Y/ylrRZFU2FS6LRiFVp6rKLPg23y3x3c3DOKAwuXGDp+PAGjh6CSnCjYeAul8pcT8bAl+lSA==",
      "dev": true,
      "license": "MIT",
      "dependencies": {
        "mdn-data": "2.27.1",
        "source-map-js": "^1.2.1"
      },
      "engines": {
        "node": "^10 || ^12.20.0 || ^14.13.0 || >=15.0.0"
      }
    },
    "node_modules/csstype": {
      "version": "3.2.3",
      "resolved": "https://registry.npmjs.org/csstype/-/csstype-3.2.3.tgz",
      "integrity": "sha512-z1HGKcYy2xA8AGQfwrn0PAy+PB7X/GSj3UVJW9qKyn43xWa+gl5nXmU4qqLMRzWVLFC8KusUX8T/0kCiOYpAIQ==",
      "dev": true,
      "license": "MIT"
    },
    "node_modules/data-urls": {
      "version": "8.0.0",
      "resolved": "https://registry.npmjs.org/data-urls/-/data-urls-8.0.0.tgz",
      "integrity": "sha512-JQUCCK+/CAGbGGx618PlFZMXH81Jtj7t73CHv9dd/zVWvxgGT2zLFRjnKb+Ng46GIwcy+f7f88qd4duqllK+WQ==",
      "dev": true,
      "license": "MIT",
      "dependencies": {
        "whatwg-mimetype": "^5.0.0",
        "whatwg-url": "^17.1.2"
      },
      "engines": {
        "node": "^22.14.0 || >=24.0.0"
      }
    },
    "node_modules/decimal.js": {
      "version": "10.6.0",
      "resolved": "https://registry.npmjs.org/decimal.js/-/decimal.js-10.6.0.tgz",
      "integrity": "sha512-YpgQiITW3JXGntzdUmyUR1V812Hn8T1YVXhCu+wO3OpS4eU9l4YdD3qjyiKdV6mvV29zapkMeD390UVEf2lkUg==",
      "dev": true,
      "license": "MIT"
    },
    "node_modules/dequal": {
      "version": "2.0.3",
      "resolved": "https://registry.npmjs.org/dequal/-/dequal-2.0.3.tgz",
      "integrity": "sha512-0je+qPKHEMohvfRTCEo3CrPG6cAzAYgmzKyxRiYSSDkS6eGJdyVJm7WaYA5ECaAD9wLB2T4EEeymA5aFVcYXCA==",
      "dev": true,
      "license": "MIT",
      "peer": true,
      "engines": {
        "node": ">=6"
      }
    },
    "node_modules/detect-libc": {
      "version": "2.1.2",
      "resolved": "https://registry.npmjs.org/detect-libc/-/detect-libc-2.1.2.tgz",
      "integrity": "sha512-Btj2BOOO83o3WyH59e8MgXsxEQVcarkUOpEYrubB0urwnN10yQ364rsiByU11nZlqWYZm05i/of7io4mzihBtQ==",
      "dev": true,
      "license": "Apache-2.0",
      "engines": {
        "node": ">=8"
      }
    },
    "node_modules/dom-accessibility-api": {
      "version": "0.5.16",
      "resolved": "https://registry.npmjs.org/dom-accessibility-api/-/dom-accessibility-api-0.5.16.tgz",
      "integrity": "sha512-X7BJ2yElsnOJ30pZF4uIIDfBEVgF4XEBxL9Bxhy6dnrm5hkzqmsWHGTiHqRiITNhMyFLyAiWndIJP7Z1NTteDg==",
      "dev": true,
      "license": "MIT",
      "peer": true
    },
    "node_modules/entities": {
      "version": "8.1.0",
      "resolved": "https://registry.npmjs.org/entities/-/entities-8.1.0.tgz",
      "integrity": "sha512-kxL7msIffSuh9aaFAMD7rxAIuTRMAHMeBtgHW2yUdWw732ZNh4MehkF2gdjvtdmikkaIP9bFDDJOPlsvm7avrA==",
      "dev": true,
      "license": "BSD-2-Clause",
      "engines": {
        "node": ">=20.19.0"
      },
      "funding": {
        "url": "https://github.com/fb55/entities?sponsor=1"
      }
    },
    "node_modules/es-module-lexer": {
      "version": "2.3.2",
      "resolved": "https://registry.npmjs.org/es-module-lexer/-/es-module-lexer-2.3.2.tgz",
      "integrity": "sha512-poHGpORABojJJucnV9KbOavETW8lBVnphkW77ER5/BQ5Fz7oXSoCNek7IH3vR5nRjdsEz926ibFYX8KtLQmdyw==",
      "dev": true,
      "license": "MIT"
    },
    "node_modules/estree-walker": {
      "version": "3.0.3",
      "resolved": "https://registry.npmjs.org/estree-walker/-/estree-walker-3.0.3.tgz",
      "integrity": "sha512-7RUKfXgSMMkzt6ZuXmqapOurLGPPfgj6l9uRZ7lRGolvk0y2yocc35LdcxKC5PQZdn2DMqioAQ2NoWcrTKmm6g==",
      "dev": true,
      "license": "MIT",
      "dependencies": {
        "@types/estree": "^1.0.0"
      }
    },
    "node_modules/expect-type": {
      "version": "1.4.0",
      "resolved": "https://registry.npmjs.org/expect-type/-/expect-type-1.4.0.tgz",
      "integrity": "sha512-KfYbmpRm0VbLjEvVa9yGwCi9GI34xvi7A/HXYWQO65CSD2u3MczUJSuwXKFIxlGsgBQizV9q5J9NHj4VG0n+pA==",
      "dev": true,
      "license": "Apache-2.0",
      "engines": {
        "node": ">=12.0.0"
      }
    },
    "node_modules/fdir": {
      "version": "6.5.0",
      "resolved": "https://registry.npmjs.org/fdir/-/fdir-6.5.0.tgz",
      "integrity": "sha512-tIbYtZbucOs0BRGqPJkshJUYdL+SDH7dVM8gjy+ERp3WAUjLEFJE+02kanyHtwjWOnwrKYBiwAmM0p4kLJAnXg==",
      "dev": true,
      "license": "MIT",
      "engines": {
        "node": ">=12.0.0"
      },
      "peerDependencies": {
        "picomatch": "^3 || ^4"
      },
      "peerDependenciesMeta": {
        "picomatch": {
          "optional": true
        }
      }
    },
    "node_modules/fsevents": {
      "version": "2.3.3",
      "resolved": "https://registry.npmjs.org/fsevents/-/fsevents-2.3.3.tgz",
      "integrity": "sha512-5xoDfX+fL7faATnagmWPpbFtwh/R77WmMMqqHGS65C3vvB0YHrgF+B1YmZ3441tMj5n63k0212XNoJwzlhffQw==",
      "dev": true,
      "hasInstallScript": true,
      "license": "MIT",
      "optional": true,
      "os": [
        "darwin"
      ],
      "engines": {
        "node": "^8.16.0 || ^10.6.0 || >=11.0.0"
      }
    },
    "node_modules/html-encoding-sniffer": {
      "version": "7.0.0",
      "resolved": "https://registry.npmjs.org/html-encoding-sniffer/-/html-encoding-sniffer-7.0.0.tgz",
      "integrity": "sha512-UikN5yr7xsCDAq87Or5or0PAlD3HJJOKVzM05az588WnpDJ4Ux7a2A53Qi6gofGg2/EtvF/H4hCi/TXfCW4Y6w==",
      "dev": true,
      "license": "MIT",
      "dependencies": {
        "@exodus/bytes": "^1.15.1"
      },
      "engines": {
        "node": "^22.13.0 || >=24.0.0"
      }
    },
    "node_modules/is-potential-custom-element-name": {
      "version": "1.0.1",
      "resolved": "https://registry.npmjs.org/is-potential-custom-element-name/-/is-potential-custom-element-name-1.0.1.tgz",
      "integrity": "sha512-bCYeRA2rVibKZd+s2625gGnGF/t7DSqDs4dP7CrLA1m7jKWz6pps0LpYLJN8Q64HtmPKJ1hrN3nzPNKFEKOUiQ==",
      "dev": true,
      "license": "MIT"
    },
    "node_modules/js-tokens": {
      "version": "4.0.0",
      "resolved": "https://registry.npmjs.org/js-tokens/-/js-tokens-4.0.0.tgz",
      "integrity": "sha512-RdJUflcE3cUzKiMqQgsCu06FPu9UdIJO0beYbPhHN4k6apgJtifcoCtT9bcxOpYBtpD2kCM6Sbzg4CausW/PKQ==",
      "dev": true,
      "license": "MIT",
      "peer": true
    },
    "node_modules/jsdom": {
      "version": "30.1.3",
      "resolved": "https://registry.npmjs.org/jsdom/-/jsdom-30.1.3.tgz",
      "integrity": "sha512-hQMUPvRgN7KxsfIytDQji7bimuJeRQa6Qo5+iuiZQk5CafeRxLIA10/cG5YCBFNpD9RO3oqIiN4CO1Y44kxqXg==",
      "dev": true,
      "license": "MIT",
      "dependencies": {
        "@asamuzakjp/css-color": "^7.1.3",
        "@asamuzakjp/dom-selector": "^10.0.3",
        "@bramus/specificity": "^2.4.2",
        "@csstools/css-syntax-patches-for-csstree": "^1.1.15",
        "@exodus/bytes": "^1.16.0",
        "css-tree": "^3.2.1",
        "data-urls": "^8.0.0",
        "decimal.js": "^10.6.0",
        "html-encoding-sniffer": "^7.0.0",
        "is-potential-custom-element-name": "^1.0.1",
        "lru-cache": "^11.5.3",
        "parse5": "^8.0.1",
        "saxes": "^6.0.0",
        "tough-cookie": "^6.0.2",
        "undici": "^8.12.1",
        "w3c-xmlserializer": "^6.0.0",
        "webidl-conversions": "^8.0.1",
        "whatwg-mimetype": "^5.0.0",
        "whatwg-url": "^17.2.0",
        "xml-name-validator": "^5.0.0"
      },
      "engines": {
        "node": "^22.22.2 || ^24.15.0 || >=26.0.0"
      },
      "peerDependencies": {
        "canvas": "^3.2.3"
      },
      "peerDependenciesMeta": {
        "canvas": {
          "optional": true
        }
      }
    },
    "node_modules/lightningcss": {
      "version": "1.33.0",
      "resolved": "https://registry.npmjs.org/lightningcss/-/lightningcss-1.33.0.tgz",
      "integrity": "sha512-WkUDrojuJs0xkgGf2udWxa3yGBRxPtxUkB79i6aCZLRgc7PM8fZe9TosfPDcvEpQZbuFASnHYmRLBLUbmLOIIA==",
      "dev": true,
      "license": "MPL-2.0",
      "dependencies": {
        "detect-libc": "^2.0.3"
      },
      "engines": {
        "node": ">= 12.0.0"
      },
      "funding": {
        "type": "opencollective",
        "url": "https://opencollective.com/parcel"
      },
      "optionalDependencies": {
        "lightningcss-android-arm64": "1.33.0",
        "lightningcss-darwin-arm64": "1.33.0",
        "lightningcss-darwin-x64": "1.33.0",
        "lightningcss-freebsd-x64": "1.33.0",
        "lightningcss-linux-arm-gnueabihf": "1.33.0",
        "lightningcss-linux-arm64-gnu": "1.33.0",
        "lightningcss-linux-arm64-musl": "1.33.0",
        "lightningcss-linux-x64-gnu": "1.33.0",
        "lightningcss-linux-x64-musl": "1.33.0",
        "lightningcss-win32-arm64-msvc": "1.33.0",
        "lightningcss-win32-x64-msvc": "1.33.0"
      }
    },
    "node_modules/lightningcss-android-arm64": {
      "version": "1.33.0",
      "resolved": "https://registry.npmjs.org/lightningcss-android-arm64/-/lightningcss-android-arm64-1.33.0.tgz",
      "integrity": "sha512-gEpRTalKdosp4Bb8qWtc2iOgE5SeIHlpS1up9bFq2wAyYhl1UdTObYiHe98zEM9SQvSoqQZ1IQD0JNpg3Ml5pg==",
      "cpu": [
        "arm64"
      ],
      "dev": true,
      "license": "MPL-2.0",
      "optional": true,
      "os": [
        "android"
      ],
      "engines": {
        "node": ">= 12.0.0"
      },
      "funding": {
        "type": "opencollective",
        "url": "https://opencollective.com/parcel"
      }
    },
    "node_modules/lightningcss-darwin-arm64": {
      "version": "1.33.0",
      "resolved": "https://registry.npmjs.org/lightningcss-darwin-arm64/-/lightningcss-darwin-arm64-1.33.0.tgz",
      "integrity": "sha512-Sciaz8eenNTKn9b3t7+xr0ipTp9YxKQY4npwQ3mrRuL0BAVHBLyZxofhaKBAVtzmtRZ/zTyo0/to4B1uWG/Djg==",
      "cpu": [
        "arm64"
      ],
      "dev": true,
      "license": "MPL-2.0",
      "optional": true,
      "os": [
        "darwin"
      ],
      "engines": {
        "node": ">= 12.0.0"
      },
      "funding": {
        "type": "opencollective",
        "url": "https://opencollective.com/parcel"
      }
    },
    "node_modules/lightningcss-darwin-x64": {
      "version": "1.33.0",
      "resolved": "https://registry.npmjs.org/lightningcss-darwin-x64/-/lightningcss-darwin-x64-1.33.0.tgz",
      "integrity": "sha512-Z5UPAxzrjlWNNyGy6i65cJzzvgJ5D3T6wMvs+gWpY9d7qRhANrxqAp6LhxIgZhWEw18RfJTGcRxjuLIBr+m8XQ==",
      "cpu": [
        "x64"
      ],
      "dev": true,
      "license": "MPL-2.0",
      "optional": true,
      "os": [
        "darwin"
      ],
      "engines": {
        "node": ">= 12.0.0"
      },
      "funding": {
        "type": "opencollective",
        "url": "https://opencollective.com/parcel"
      }
    },
    "node_modules/lightningcss-freebsd-x64": {
      "version": "1.33.0",
      "resolved": "https://registry.npmjs.org/lightningcss-freebsd-x64/-/lightningcss-freebsd-x64-1.33.0.tgz",
      "integrity": "sha512-QQM/Ti/hQajJwCY+RiWuCZ9sdtI/XQk7nDK5vC8kkdwixezOlDgvDx7+RT+QjK6FcFT4MpsuoBnHIo/O3StRRg==",
      "cpu": [
        "x64"
      ],
      "dev": true,
      "license": "MPL-2.0",
      "optional": true,
      "os": [
        "freebsd"
      ],
      "engines": {
        "node": ">= 12.0.0"
      },
      "funding": {
        "type": "opencollective",
        "url": "https://opencollective.com/parcel"
      }
    },
    "node_modules/lightningcss-linux-arm-gnueabihf": {
      "version": "1.33.0",
      "resolved": "https://registry.npmjs.org/lightningcss-linux-arm-gnueabihf/-/lightningcss-linux-arm-gnueabihf-1.33.0.tgz",
      "integrity": "sha512-N7FVBe6iS24MlM6R/4RBTxGhQheZGs7tiQ9U32UtF75NzP5Q7xWPRqLBCKxlRQRk3rY1jCIPLzx7WzOhuUIRLQ==",
      "cpu": [
        "arm"
      ],
      "dev": true,
      "license": "MPL-2.0",
      "optional": true,
      "os": [
        "linux"
      ],
      "engines": {
        "node": ">= 12.0.0"
      },
      "funding": {
        "type": "opencollective",
        "url": "https://opencollective.com/parcel"
      }
    },
    "node_modules/lightningcss-linux-arm64-gnu": {
      "version": "1.33.0",
      "resolved": "https://registry.npmjs.org/lightningcss-linux-arm64-gnu/-/lightningcss-linux-arm64-gnu-1.33.0.tgz",
      "integrity": "sha512-j2v/itmy4HlNxlc6voKXYgBqNi0Ng2LShg4z7GufpEgs05P+2suBVyi9I6YHq5uoVFx9ETin3eCEhLVyXGQnKg==",
      "cpu": [
        "arm64"
      ],
      "dev": true,
      "libc": [
        "glibc"
      ],
      "license": "MPL-2.0",
      "optional": true,
      "os": [
        "linux"
      ],
      "engines": {
        "node": ">= 12.0.0"
      },
      "funding": {
        "type": "opencollective",
        "url": "https://opencollective.com/parcel"
      }
    },
    "node_modules/lightningcss-linux-arm64-musl": {
      "version": "1.33.0",
      "resolved": "https://registry.npmjs.org/lightningcss-linux-arm64-musl/-/lightningcss-linux-arm64-musl-1.33.0.tgz",
      "integrity": "sha512-yiO5ROMuYQgXbC60yjZU5CYSFZGKXL0HFATXt9mHJn1+zW55oCtMI9NfcVhYLMFDL7gV7oBPon/EmMMGg2OvtQ==",
      "cpu": [
        "arm64"
      ],
      "dev": true,
      "libc": [
        "musl"
      ],
      "license": "MPL-2.0",
      "optional": true,
      "os": [
        "linux"
      ],
      "engines": {
        "node": ">= 12.0.0"
      },
      "funding": {
        "type": "opencollective",
        "url": "https://opencollective.com/parcel"
      }
    },
    "node_modules/lightningcss-linux-x64-gnu": {
      "version": "1.33.0",
      "resolved": "https://registry.npmjs.org/lightningcss-linux-x64-gnu/-/lightningcss-linux-x64-gnu-1.33.0.tgz",
      "integrity": "sha512-ar+Ju7LmcN0Jo4FpL4hpFybwNG9/3A/Br5KW2n2jyODg3MEZXaDYADdemoNS+BDNfMgKvylJLj4S5tyRActuAg==",
      "cpu": [
        "x64"
      ],
      "dev": true,
      "libc": [
        "glibc"
      ],
      "license": "MPL-2.0",
      "optional": true,
      "os": [
        "linux"
      ],
      "engines": {
        "node": ">= 12.0.0"
      },
      "funding": {
        "type": "opencollective",
        "url": "https://opencollective.com/parcel"
      }
    },
    "node_modules/lightningcss-linux-x64-musl": {
      "version": "1.33.0",
      "resolved": "https://registry.npmjs.org/lightningcss-linux-x64-musl/-/lightningcss-linux-x64-musl-1.33.0.tgz",
      "integrity": "sha512-RYiYbkokw0trfKqqzfF55lginwEPrD3OJDfTuJzFs1MK6iFnDenaz1fqLLtX4ITG3OktJQXOeTaw1awrBAlZPw==",
      "cpu": [
        "x64"
      ],
      "dev": true,
      "libc": [
        "musl"
      ],
      "license": "MPL-2.0",
      "optional": true,
      "os": [
        "linux"
      ],
      "engines": {
        "node": ">= 12.0.0"
      },
      "funding": {
        "type": "opencollective",
        "url": "https://opencollective.com/parcel"
      }
    },
    "node_modules/lightningcss-win32-arm64-msvc": {
      "version": "1.33.0",
      "resolved": "https://registry.npmjs.org/lightningcss-win32-arm64-msvc/-/lightningcss-win32-arm64-msvc-1.33.0.tgz",
      "integrity": "sha512-1K+MPfLSFVpphzpdbfkhlWk6wBrTObBzS2T6db10PNOZgR9GoVsAWzwNyuhUYYbTp23j+4RrncfujZ4uAzXvwA==",
      "cpu": [
        "arm64"
      ],
      "dev": true,
      "license": "MPL-2.0",
      "optional": true,
      "os": [
        "win32"
      ],
      "engines": {
        "node": ">= 12.0.0"
      },
      "funding": {
        "type": "opencollective",
        "url": "https://opencollective.com/parcel"
      }
    },
    "node_modules/lightningcss-win32-x64-msvc": {
      "version": "1.33.0",
      "resolved": "https://registry.npmjs.org/lightningcss-win32-x64-msvc/-/lightningcss-win32-x64-msvc-1.33.0.tgz",
      "integrity": "sha512-OlEICDx/Xl0FqSp4bry8zFnCvGpig3Gl4gCquvYwHuqJKEC1+n9NgDniFvqHGmMv1ZkqDJrDqKKSykTDX+ehuA==",
      "cpu": [
        "x64"
      ],
      "dev": true,
      "license": "MPL-2.0",
      "optional": true,
      "os": [
        "win32"
      ],
      "engines": {
        "node": ">= 12.0.0"
      },
      "funding": {
        "type": "opencollective",
        "url": "https://opencollective.com/parcel"
      }
    },
    "node_modules/lru-cache": {
      "version": "11.5.3",
      "resolved": "https://registry.npmjs.org/lru-cache/-/lru-cache-11.5.3.tgz",
      "integrity": "sha512-U4N8FgzmWxc8k1VH8Kr6lQg18U7Fjvby6wXHVRX/ZZ7IwWbRMgrRbP0Wrb5q5NVinryp4SQampHKdvtecItxUg==",
      "dev": true,
      "license": "BlueOak-1.0.0",
      "engines": {
        "node": "20 || >=22"
      }
    },
    "node_modules/lz-string": {
      "version": "1.5.0",
      "resolved": "https://registry.npmjs.org/lz-string/-/lz-string-1.5.0.tgz",
      "integrity": "sha512-h5bgJWpxJNswbU7qCrV0tIKQCaS3blPDrqKWx+QxzuzL1zGUzij9XCWLrSLsJPu5t+eWA/ycetzYAO5IOMcWAQ==",
      "dev": true,
      "license": "MIT",
      "peer": true,
      "bin": {
        "lz-string": "bin/bin.js"
      }
    },
    "node_modules/magic-string": {
      "version": "1.4.3",
      "resolved": "https://registry.npmjs.org/magic-string/-/magic-string-1.4.3.tgz",
      "integrity": "sha512-z12OxmaPGE0F4xlpGdjuAUckipLpQlTAuAmyGhNoD7ODpYUzvDfvtxUbjM/jwznyP178oju/KDdyi220wNJ0RQ==",
      "dev": true,
      "license": "MIT",
      "dependencies": {
        "@jridgewell/sourcemap-codec": "^1.6.0"
      }
    },
    "node_modules/mdn-data": {
      "version": "2.27.1",
      "resolved": "https://registry.npmjs.org/mdn-data/-/mdn-data-2.27.1.tgz",
      "integrity": "sha512-9Yubnt3e8A0OKwxYSXyhLymGW4sCufcLG6VdiDdUGVkPhpqLxlvP5vl1983gQjJl3tqbrM731mjaZaP68AgosQ==",
      "dev": true,
      "license": "CC0-1.0"
    },
    "node_modules/nanoid": {
      "version": "3.3.20",
      "resolved": "https://registry.npmjs.org/nanoid/-/nanoid-3.3.20.tgz",
      "integrity": "sha512-uKdg2G3GNCKQn9byYOpxbGqrT2fGO5KRt5J/8b3pok8rT6qxGWF6hxMyJiEYtAf+FVyYuD9hRaDqX5uPFYJ4ZQ==",
      "dev": true,
      "funding": [
        {
          "type": "github",
          "url": "https://github.com/sponsors/ai"
        }
      ],
      "license": "MIT",
      "bin": {
        "nanoid": "bin/nanoid.cjs"
      },
      "engines": {
        "node": "^10 || ^12 || ^13.7 || ^14 || >=15.0.1"
      }
    },
    "node_modules/obug": {
      "version": "3.0.0",
      "resolved": "https://registry.npmjs.org/obug/-/obug-3.0.0.tgz",
      "integrity": "sha512-5vvB5+W7ePv+p3uqxi+RcW1XAzLW0/hxt3/4X4Lc4qHudzOhmBiBwOY6DRob4WnanAEGvNcLjF+KNOufrUoEQw==",
      "dev": true,
      "funding": [
        "https://github.com/sponsors/sxzz",
        "https://opencollective.com/debug"
      ],
      "license": "MIT",
      "engines": {
        "node": ">=12.20.0"
      }
    },
    "node_modules/parse5": {
      "version": "8.0.1",
      "resolved": "https://registry.npmjs.org/parse5/-/parse5-8.0.1.tgz",
      "integrity": "sha512-z1e/HMG90obSGeidlli3hj7cbocou0/wa5HacvI3ASx34PecNjNQeaHNo5WIZpWofN9kgkqV1q5YvXe3F0FoPw==",
      "dev": true,
      "license": "MIT",
      "dependencies": {
        "entities": "^8.0.0"
      },
      "funding": {
        "url": "https://github.com/inikulin/parse5?sponsor=1"
      }
    },
    "node_modules/picocolors": {
      "version": "1.1.1",
      "resolved": "https://registry.npmjs.org/picocolors/-/picocolors-1.1.1.tgz",
      "integrity": "sha512-xceH2snhtb5M9liqDsmEw56le376mTZkEX/jEb/RxNFyegNul7eNslCXP9FDj/Lcu0X8KEyMceP2ntpaHrDEVA==",
      "dev": true,
      "license": "ISC"
    },
    "node_modules/picomatch": {
      "version": "4.0.7",
      "resolved": "https://registry.npmjs.org/picomatch/-/picomatch-4.0.7.tgz",
      "integrity": "sha512-qcJu88Q2IWqJsDD529JKMdwGm/dvInW4HvQnRwiH9JtihJvzGOscDtHE3x1pBKeUOTysQ8kVmLnJ2kJu7yhcGA==",
      "dev": true,
      "license": "MIT",
      "engines": {
        "node": ">=12"
      },
      "funding": {
        "url": "https://github.com/sponsors/jonschlinkert"
      }
    },
    "node_modules/postcss": {
      "version": "8.5.30",
      "resolved": "https://registry.npmjs.org/postcss/-/postcss-8.5.30.tgz",
      "integrity": "sha512-zkd+e9S1AQHhmaBKye9zlFHWjOwVgqXl1q3ECYy8561K7qxR2cREf/STLskulBFd78k6AOFeA+ff1TtEzk9bAA==",
      "dev": true,
      "funding": [
        {
          "type": "opencollective",
          "url": "https://opencollective.com/postcss/"
        },
        {
          "type": "tidelift",
          "url": "https://tidelift.com/funding/github/npm/postcss"
        },
        {
          "type": "github",
          "url": "https://github.com/sponsors/ai"
        }
      ],
      "license": "MIT",
      "dependencies": {
        "nanoid": "^3.3.20",
        "picocolors": "^1.1.1",
        "source-map-js": "^1.2.2"
      },
      "engines": {
        "node": "^10 || ^12 || >=14"
      }
    },
    "node_modules/pretty-format": {
      "version": "27.5.1",
      "resolved": "https://registry.npmjs.org/pretty-format/-/pretty-format-27.5.1.tgz",
      "integrity": "sha512-Qb1gy5OrP5+zDf2Bvnzdl3jsTf1qXVMazbvCoKhtKqVs4/YK4ozX4gKQJJVyNe+cajNPn0KoC0MC3FUmaHWEmQ==",
      "dev": true,
      "license": "MIT",
      "peer": true,
      "dependencies": {
        "ansi-regex": "^5.0.1",
        "ansi-styles": "^5.0.0",
        "react-is": "^17.0.1"
      },
      "engines": {
        "node": "^10.13.0 || ^12.13.0 || ^14.15.0 || >=15.0.0"
      }
    },
    "node_modules/punycode": {
      "version": "2.3.1",
      "resolved": "https://registry.npmjs.org/punycode/-/punycode-2.3.1.tgz",
      "integrity": "sha512-vYt7UD1U9Wg6138shLtLOvdAu+8DsC/ilFtEVHcH+wydcSpNE20AfSOduf6MkRFahL5FY7X1oU7nKVZFtfq8Fg==",
      "dev": true,
      "license": "MIT",
      "engines": {
        "node": ">=6"
      }
    },
    "node_modules/react": {
      "version": "19.3.1",
      "resolved": "https://registry.npmjs.org/react/-/react-19.3.1.tgz",
      "integrity": "sha512-QNhWdt7nrgVWWQG58QuGsAR3jv3Hbe4mb4vz7w2csnHavciZJgda1CPNaldF6eVxv90ud3jUXpKlHL69gO38yw==",
      "license": "MIT",
      "engines": {
        "node": ">=0.10.0"
      }
    },
    "node_modules/react-dom": {
      "version": "19.3.1",
      "resolved": "https://registry.npmjs.org/react-dom/-/react-dom-19.3.1.tgz",
      "integrity": "sha512-Y0bARz53lhEC1NMCASs0/j+faB/zqt3R8hpvg1e8+61ZVJ4UL0UdC4cBbXfZMcIuhrfgeILTLe+ZeBCavQPtKQ==",
      "license": "MIT",
      "dependencies": {
        "scheduler": "^0.28.0"
      },
      "peerDependencies": {
        "react": "^19.3.1"
      }
    },
    "node_modules/react-is": {
      "version": "17.0.2",
      "resolved": "https://registry.npmjs.org/react-is/-/react-is-17.0.2.tgz",
      "integrity": "sha512-w2GsyukL62IJnlaff/nRegPQR94C/XXamvMWmSHRJ4y7Ts/4ocGRmTHvOs8PSE6pB3dWOrD/nueuU5sduBsQ4w==",
      "dev": true,
      "license": "MIT",
      "peer": true
    },
    "node_modules/require-from-string": {
      "version": "2.0.2",
      "resolved": "https://registry.npmjs.org/require-from-string/-/require-from-string-2.0.2.tgz",
      "integrity": "sha512-Xf0nWe6RseziFMu+Ap9biiUbmplq6S9/p+7w7YXP/JBHhrUDDUhwa+vANyubuqfZWTveU//DYVGsDG7RKL/vEw==",
      "dev": true,
      "license": "MIT",
      "engines": {
        "node": ">=0.10.0"
      }
    },
    "node_modules/rolldown": {
      "version": "1.2.14",
      "resolved": "https://registry.npmjs.org/rolldown/-/rolldown-1.2.14.tgz",
      "integrity": "sha512-eTBTTqMR6uHKLgcUqpJP8ntzDI6V4LYA8negAMBEI7gkgVvOdrx6VsvS2Ym+83KhOPdLCHWJu0w7UQFJZhHKNw==",
      "dev": true,
      "license": "MIT",
      "dependencies": {
        "@oxc-project/types": "=0.154.0",
        "@rolldown/pluginutils": "^1.0.0"
      },
      "bin": {
        "rolldown": "bin/cli.mjs"
      },
      "engines": {
        "node": "^20.19.0 || >=22.12.0"
      },
      "optionalDependencies": {
        "@rolldown/binding-android-arm-eabi": "1.2.14",
        "@rolldown/binding-android-arm64": "1.2.14",
        "@rolldown/binding-darwin-arm64": "1.2.14",
        "@rolldown/binding-darwin-x64": "1.2.14",
        "@rolldown/binding-freebsd-x64": "1.2.14",
        "@rolldown/binding-linux-arm-gnueabihf": "1.2.14",
        "@rolldown/binding-linux-arm64-gnu": "1.2.14",
        "@rolldown/binding-linux-arm64-musl": "1.2.14",
        "@rolldown/binding-linux-ppc64-gnu": "1.2.14",
        "@rolldown/binding-linux-s390x-gnu": "1.2.14",
        "@rolldown/binding-linux-x64-gnu": "1.2.14",
        "@rolldown/binding-linux-x64-musl": "1.2.14",
        "@rolldown/binding-openharmony-arm64": "1.2.14",
        "@rolldown/binding-win32-arm64-msvc": "1.2.14",
        "@rolldown/binding-win32-x64-msvc": "1.2.14"
      }
    },
    "node_modules/saxes": {
      "version": "6.0.0",
      "resolved": "https://registry.npmjs.org/saxes/-/saxes-6.0.0.tgz",
      "integrity": "sha512-xAg7SOnEhrm5zI3puOOKyy1OMcMlIJZYNJY7xLBwSze0UjhPLnWfj2GF2EpT0jmzaJKIWKHLsaSSajf35bcYnA==",
      "dev": true,
      "license": "ISC",
      "dependencies": {
        "xmlchars": "^2.2.0"
      },
      "engines": {
        "node": ">=v12.22.7"
      }
    },
    "node_modules/scheduler": {
      "version": "0.28.0",
      "resolved": "https://registry.npmjs.org/scheduler/-/scheduler-0.28.0.tgz",
      "integrity": "sha512-juorfCmIkIw8tT+p5BXSm6PJjQF/ycEYmKyzURCIt/RaZIhL+PulbQ9Yu2z1HdOJDdqDTlxA1+xKBmHXJsczAw==",
      "license": "MIT"
    },
    "node_modules/source-map-js": {
      "version": "1.2.2",
      "resolved": "https://registry.npmjs.org/source-map-js/-/source-map-js-1.2.2.tgz",
      "integrity": "sha512-KGj/8Y43x35aZVDtt+J4mK1hoLGHULMYfSkODJNQjNDC3oW1PqPoxMwo0pLUsWM/UEGzON/NxeHywEfNXNP3Vw==",
      "dev": true,
      "license": "BSD-3-Clause",
      "engines": {
        "node": ">=0.10.0"
      }
    },
    "node_modules/std-env": {
      "version": "4.3.0",
      "resolved": "https://registry.npmjs.org/std-env/-/std-env-4.3.0.tgz",
      "integrity": "sha512-OtU/EgQ1kIm5KwqQpBC6ZEMXrZRui11w8zgfTWp8cdO9B8OaPsbA8bTHO2P+HNo1VlUTGMVBwPhydu6poeXiag==",
      "dev": true,
      "license": "MIT"
    },
    "node_modules/tinybench": {
      "version": "6.2.2",
      "resolved": "https://registry.npmjs.org/tinybench/-/tinybench-6.2.2.tgz",
      "integrity": "sha512-nIZ2N2dFpjBsg2hgitKu7mA+FvsIJuGJOwysRngmVRRmWgR/UyPjwJj8c/fT/dhm8EkHnF5EOogRf+0Z2qrimA==",
      "dev": true,
      "license": "MIT",
      "engines": {
        "node": ">=20.0.0"
      }
    },
    "node_modules/tinyexec": {
      "version": "1.3.1",
      "resolved": "https://registry.npmjs.org/tinyexec/-/tinyexec-1.3.1.tgz",
      "integrity": "sha512-GCvB3aoys96IuDFBMcTB46JOR6mdMtAToqwiW8JlWhsoh1mhHi/xn9ss/Dg7N555GiJyEt2qzoG/NHCwM6h1EA==",
      "dev": true,
      "license": "MIT",
      "engines": {
        "node": ">=18"
      }
    },
    "node_modules/tinyglobby": {
      "version": "0.2.17",
      "resolved": "https://registry.npmjs.org/tinyglobby/-/tinyglobby-0.2.17.tgz",
      "integrity": "sha512-wXR/dYpcqKmfWpEdZjiKJOwCNFndD0DMnrW/cYjVGttEkBfVgcLFHoNrlj47mjOVic9yyNu65alsgF4NQyTa2g==",
      "dev": true,
      "license": "MIT",
      "dependencies": {
        "fdir": "^6.5.0",
        "picomatch": "^4.0.4"
      },
      "engines": {
        "node": ">=12.0.0"
      },
      "funding": {
        "url": "https://github.com/sponsors/SuperchupuDev"
      }
    },
    "node_modules/tldts": {
      "version": "7.4.18",
      "resolved": "https://registry.npmjs.org/tldts/-/tldts-7.4.18.tgz",
      "integrity": "sha512-somxA8xKAUzlAxl0TX/jMSYIbRNFIVGnI96pVGJPeMc5BsHmfVXn6mOJT6U++ELnl04eGfzpfcEuZe8WiASkXg==",
      "dev": true,
      "license": "MIT",
      "dependencies": {
        "tldts-core": "^7.4.18"
      },
      "bin": {
        "tldts": "bin/cli.js"
      }
    },
    "node_modules/tldts-core": {
      "version": "7.4.18",
      "resolved": "https://registry.npmjs.org/tldts-core/-/tldts-core-7.4.18.tgz",
      "integrity": "sha512-KI2PKvCmvQI/xD6ypPqkU0i7oxNXR/O58RLRaHR4svsgVUQ2JE/XC8BtMbG6PSehD27I8sZJRcC1ASnCy6lp3g==",
      "dev": true,
      "license": "MIT"
    },
    "node_modules/tough-cookie": {
      "version": "6.0.2",
      "resolved": "https://registry.npmjs.org/tough-cookie/-/tough-cookie-6.0.2.tgz",
      "integrity": "sha512-exgYmnmL/sJpR3upZfXG5PoatXQii55xAiXGXzY+sROLZ/Y+SLcp9PgJNI9Vz37HpQ74WvDcLT8eqm+kV3FzrA==",
      "dev": true,
      "license": "BSD-3-Clause",
      "dependencies": {
        "tldts": "^7.0.5"
      },
      "engines": {
        "node": ">=16"
      }
    },
    "node_modules/tr46": {
      "version": "7.0.0",
      "resolved": "https://registry.npmjs.org/tr46/-/tr46-7.0.0.tgz",
      "integrity": "sha512-BcDSDW+SIGWVOns1Zm6Yg91q9AEj7eaID0QDY9oIqJD1A8dscNwMcl9Sms2DeuWCNov1X0/oXanHGYQBSLwZsg==",
      "dev": true,
      "license": "MIT",
      "dependencies": {
        "punycode": "^2.3.1"
      },
      "engines": {
        "node": "^22.14.0 || >=24.0.0"
      }
    },
    "node_modules/typescript": {
      "version": "5.9.3",
      "resolved": "https://registry.npmjs.org/typescript/-/typescript-5.9.3.tgz",
      "integrity": "sha512-jl1vZzPDinLr9eUt3J/t7V6FgNEw9QjvBPdysz9KfQDD41fQrC2Y4vKQdiaUpFT4bXlb1RHhLpp8wtm6M5TgSw==",
      "dev": true,
      "license": "Apache-2.0",
      "bin": {
        "tsc": "bin/tsc",
        "tsserver": "bin/tsserver"
      },
      "engines": {
        "node": ">=14.17"
      }
    },
    "node_modules/undici": {
      "version": "8.12.2",
      "resolved": "https://registry.npmjs.org/undici/-/undici-8.12.2.tgz",
      "integrity": "sha512-XtuDWVdqTLr3NAxm2ly9/hU6kUsJOgGh1RASzMBbsw6aWCOaQzHdW+QQDfdERjfJ2V5vL5pRPa4RetO9UfivDg==",
      "dev": true,
      "license": "MIT",
      "engines": {
        "node": ">=22.19.0"
      }
    },
    "node_modules/undici-types": {
      "version": "7.24.6",
      "resolved": "https://registry.npmjs.org/undici-types/-/undici-types-7.24.6.tgz",
      "integrity": "sha512-WRNW+sJgj5OBN4/0JpHFqtqzhpbnV0GuB+OozA9gCL7a993SmU+1JBZCzLNxYsbMfIeDL+lTsphD5jN5N+n0zg==",
      "dev": true,
      "license": "MIT"
    },
    "node_modules/vite": {
      "version": "8.3.4",
      "resolved": "https://registry.npmjs.org/vite/-/vite-8.3.4.tgz",
      "integrity": "sha512-IftLE8phJhdNRIGNEV4JCCvfNScoZQVfcXaN8TQropWGJtOHcu+iAB9pHcuGvmEb/kddw0LBe/jS8ztdC6xdiQ==",
      "dev": true,
      "license": "MIT",
      "dependencies": {
        "lightningcss": "^1.33.0",
        "picomatch": "^4.0.7",
        "postcss": "^8.5.29",
        "rolldown": "~1.2.12",
        "tinyglobby": "^0.2.17"
      },
      "bin": {
        "vite": "bin/vite.js"
      },
      "engines": {
        "node": "^20.19.0 || >=22.12.0"
      },
      "funding": {
        "url": "https://github.com/vitejs/vite?sponsor=1"
      },
      "optionalDependencies": {
        "fsevents": "~2.3.3"
      },
      "peerDependencies": {
        "@types/node": "^20.19.0 || >=22.12.0",
        "@vitejs/devtools": "^0.7.1",
        "esbuild": "^0.27.0 || ^0.28.0",
        "jiti": ">=1.21.0",
        "less": "^4.0.0",
        "sass": "^1.70.0",
        "sass-embedded": "^1.70.0",
        "stylus": ">=0.54.8",
        "sugarss": "^5.0.0",
        "terser": "^5.16.0",
        "tsx": "^4.8.1",
        "yaml": "^2.4.2"
      },
      "peerDependenciesMeta": {
        "@types/node": {
          "optional": true
        },
        "@vitejs/devtools": {
          "optional": true
        },
        "esbuild": {
          "optional": true
        },
        "jiti": {
          "optional": true
        },
        "less": {
          "optional": true
        },
        "sass": {
          "optional": true
        },
        "sass-embedded": {
          "optional": true
        },
        "stylus": {
          "optional": true
        },
        "sugarss": {
          "optional": true
        },
        "terser": {
          "optional": true
        },
        "tsx": {
          "optional": true
        },
        "yaml": {
          "optional": true
        }
      }
    },
    "node_modules/vitest": {
      "version": "5.0.5",
      "resolved": "https://registry.npmjs.org/vitest/-/vitest-5.0.5.tgz",
      "integrity": "sha512-cPODOMF8YnkiiYtDdHo6n4wWmBrOBBbOIYH2d33M4l3CdNn72h+qMmO6uXffUN6A5AZFXNND/d+krS9mgkXRfg==",
      "dev": true,
      "license": "MIT",
      "dependencies": {
        "@types/chai": "^5.2.2",
        "@vitest/mocker": "5.0.5",
        "chai": "^6.3.0",
        "es-module-lexer": "^2.3.2",
        "expect-type": "^1.4.0",
        "magic-string": "^1.4.3",
        "obug": "^3.0.0",
        "picomatch": "^4.0.7",
        "std-env": "^4.3.0",
        "tinybench": "^6.2.2",
        "tinyexec": "^1.3.1",
        "tinyglobby": "^0.2.17",
        "why-is-node-running": "3.2.1"
      },
      "bin": {
        "vitest": "vitest.mjs"
      },
      "engines": {
        "node": "^22.12.0 || ^24.0.0 || >=26.0.0"
      },
      "funding": {
        "url": "https://opencollective.com/vitest"
      },
      "peerDependencies": {
        "@edge-runtime/vm": "*",
        "@opentelemetry/api": "^1.9.0",
        "@types/node": "^22.0.0 || >=24.0.0",
        "@vitest/browser-playwright": "5.0.5",
        "@vitest/browser-preview": "5.0.5",
        "@vitest/browser-webdriverio": "^5.0.0-beta.5 || >=5.0.0",
        "@vitest/coverage-istanbul": "5.0.5",
        "@vitest/coverage-v8": "5.0.5",
        "@vitest/ui": "5.0.5",
        "happy-dom": "*",
        "jsdom": "*",
        "vite": "^6.4.0 || ^7.0.0 || ^8.0.0"
      },
      "peerDependenciesMeta": {
        "@edge-runtime/vm": {
          "optional": true
        },
        "@opentelemetry/api": {
          "optional": true
        },
        "@types/node": {
          "optional": true
        },
        "@vitest/browser-playwright": {
          "optional": true
        },
        "@vitest/browser-preview": {
          "optional": true
        },
        "@vitest/browser-webdriverio": {
          "optional": true
        },
        "@vitest/coverage-istanbul": {
          "optional": true
        },
        "@vitest/coverage-v8": {
          "optional": true
        },
        "@vitest/ui": {
          "optional": true
        },
        "happy-dom": {
          "optional": true
        },
        "jsdom": {
          "optional": true
        },
        "vite": {
          "optional": false
        }
      }
    },
    "node_modules/w3c-xmlserializer": {
      "version": "6.0.0",
      "resolved": "https://registry.npmjs.org/w3c-xmlserializer/-/w3c-xmlserializer-6.0.0.tgz",
      "integrity": "sha512-4Nsy8K5Tr6SPDH9jhKJOHf7ChDrc1zufZTVSF7x72hwuEXBqxqk9G6cK+K2NRUtB3iELRJqjXb4JPDMBjMTl2Q==",
      "dev": true,
      "license": "MIT",
      "dependencies": {
        "xml-name-validator": "^5.0.0"
      },
      "engines": {
        "node": "^22.22.2 || ^24.15.0 || >=26.0.0"
      }
    },
    "node_modules/webidl-conversions": {
      "version": "8.0.1",
      "resolved": "https://registry.npmjs.org/webidl-conversions/-/webidl-conversions-8.0.1.tgz",
      "integrity": "sha512-BMhLD/Sw+GbJC21C/UgyaZX41nPt8bUTg+jWyDeg7e7YN4xOM05YPSIXceACnXVtqyEw/LMClUQMtMZ+PGGpqQ==",
      "dev": true,
      "license": "BSD-2-Clause",
      "engines": {
        "node": ">=20"
      }
    },
    "node_modules/whatwg-mimetype": {
      "version": "5.0.0",
      "resolved": "https://registry.npmjs.org/whatwg-mimetype/-/whatwg-mimetype-5.0.0.tgz",
      "integrity": "sha512-sXcNcHOC51uPGF0P/D4NVtrkjSU2fNsm9iog4ZvZJsL3rjoDAzXZhkm2MWt1y+PUdggKAYVoMAIYcs78wJ51Cw==",
      "dev": true,
      "license": "MIT",
      "engines": {
        "node": ">=20"
      }
    },
    "node_modules/whatwg-url": {
      "version": "17.3.0",
      "resolved": "https://registry.npmjs.org/whatwg-url/-/whatwg-url-17.3.0.tgz",
      "integrity": "sha512-KwOAJrpf+GTgBvcBQJcP38sMF0DsQKdwtG+TxM2pW44P6E5k0hDbWrDmyuJDIF0Uef0aQq0ZoJs+5oI+k8ddhg==",
      "dev": true,
      "license": "MIT",
      "dependencies": {
        "@exodus/bytes": "^1.16.0",
        "tr46": "^7.0.0",
        "webidl-conversions": "^8.0.1"
      },
      "engines": {
        "node": "^22.14.0 || >=24.0.0"
      }
    },
    "node_modules/why-is-node-running": {
      "version": "3.2.1",
      "resolved": "https://registry.npmjs.org/why-is-node-running/-/why-is-node-running-3.2.1.tgz",
      "integrity": "sha512-Tb2FUhB4vUsGQlfSquQLYkApkuPAFQXGFzxWKHHumVz2dK+X1RUm/HnID4+TfIGYJ1kTcwOaCk/buYCEJr6YjQ==",
      "dev": true,
      "license": "MIT",
      "bin": {
        "why-is-node-running": "cli.js"
      },
      "engines": {
        "node": ">=20.11"
      }
    },
    "node_modules/xml-name-validator": {
      "version": "5.0.0",
      "resolved": "https://registry.npmjs.org/xml-name-validator/-/xml-name-validator-5.0.0.tgz",
      "integrity": "sha512-EvGK8EJ3DhaHfbRlETOWAS5pO9MZITeauHKJyb8wyajUfQUenkIg2MvLDTZ4T/TgIcm3HU0TFBgWWboAZ30UHg==",
      "dev": true,
      "license": "Apache-2.0",
      "engines": {
        "node": ">=18"
      }
    },
    "node_modules/xmlchars": {
      "version": "2.2.0",
      "resolved": "https://registry.npmjs.org/xmlchars/-/xmlchars-2.2.0.tgz",
      "integrity": "sha512-JZnDKK8B0RCDw84FNdDAIpZK+JuJw+s7Lz8nksI7SIuU3UXJJslUthsi+uWBUYOwPFwW7W7PRLRfUKpxjtjFCw==",
      "dev": true,
      "license": "MIT"
    }
  }
}
```

