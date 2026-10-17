// The sample the preview draws (spec §5.6, ruling R1): the user, the computer and the home folder
// are this computer's own, which `system_detect` sends as `previewSample` (the UI cannot read the
// environment itself); cmd sits in `<home>\projects\cmd-studio`; the clock, git, the last command
// and the window are the core's fixed sample (`PreviewContext::sample()` in crates/core). Fixed
// data and a path join, no prompt logic: the core draws everything from it.
import type { PreviewContext } from "./bindings/PreviewContext";
import type { SampleIdentity } from "./bindings/SampleIdentity";

const PROJECT = "projects\\cmd-studio";

/**
 * The core's own sample identity (`PreviewContext::sample()`), for when `system_detect` could not
 * answer: the preview still has something to draw.
 */
export const CORE_SAMPLE_IDENTITY: SampleIdentity = { user: "you", host: "PC", home: "C:\\Users\\you" };

/** `<home>\projects\cmd-studio`. A home that ends with a separator (a drive's root such as `H:\`,
 * spec §5.6) keeps it, so the path has no doubled `\`. */
export function sampleFolder(home: string): string {
  return /[\\/]$/.test(home) ? `${home}${PROJECT}` : `${home}\\${PROJECT}`;
}

export function sampleContext(identity: SampleIdentity): PreviewContext {
  return {
    user: identity.user,
    host: identity.host,
    home: identity.home,
    cwd: sampleFolder(identity.home),
    clock: { year: 2026, month: 10, day: 9, hour: 16, minute: 47 },
    git: { branch: "main", dirty: true },
    exitCode: 1,
    durationMs: 3500,
    width: 80,
    windowsTerminal: true,
    command: "npm test",
  };
}
