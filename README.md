# CMD Studio

Theme the Windows command prompt (cmd) without typing a command: pick one of twelve built-in themes or design your own, preview it live, apply it, and undo it at any time. Made for people who have never opened cmd, with a CLI for those who have.

Status: pre-release (unsigned builds for testers). The design is in `docs/superpowers/specs/`.

## What you need

- Windows 11, or Windows 10 version 2004 (build 19041) and later. x64 only; the installer refuses ARM64.
- Windows Terminal for the colors (Windows 11 usually has it; CMD Studio offers the Store page when it is missing). Without it you still get the prompt and the welcome box.
- No administrator rights. No Node, no npm, nothing to download at first start: Oh My Posh, Clink and the font come inside the installer.

## Install

1. Download `CMD Studio_<version>_x64-setup.exe` from [Releases](https://github.com/luraselenehalo/cmd-studio/releases).
2. Run it. **Windows SmartScreen will warn** ("Windows protected your PC"): choose *More info*, then *Run anyway*. The build is not code-signed yet; signing through SignPath Foundation is planned once the project qualifies, and even a signed program earns SmartScreen's trust only over time (since 2024 an EV certificate no longer skips the warning either). Check the SHA-256 in `SHA256SUMS.txt` on the release page if you want to be sure of the file.
3. The installer puts CMD Studio in `%LOCALAPPDATA%\CMD Studio` for your user only, adds that folder to your Path (cmd windows that were already open do not see it; open a new one), makes two Start-menu entries, "CMD Studio" and "CMD Studio Recovery", and starts the app. The first run is a short wizard: pick a theme, choose where it goes, apply.

## What it changes on your PC, and how to undo it

Every change is listed before it is made, and a restore point is taken first. CMD Studio touches only: cmd's AutoRun value under `HKEY_CURRENT_USER` (so every cmd window loads the theme; or only Windows Terminal's cmd profile, your choice), Windows Terminal's `settings.json` (a color scheme named "CMD Studio" and the cmd profile), a per-user font (CaskaydiaCove Nerd Font, installed only when a theme needs it), its own data folder `%LOCALAPPDATA%\ai.resincore.cmdstudio`, and your user Path (the install folder, added by the installer and removed by the uninstaller; every other entry stays as it was). Nothing needs administrator rights; nothing is sent anywhere (no telemetry; diagnostics leave your PC only when you copy them yourself).

- *Undo*, the button that appears after an apply, returns to the restore point of that apply.
- *Undo everything* in Help puts back everything CMD Studio ever changed.
- Uninstalling (Settings > Apps, or the Start menu) does the same and removes the program; your themes stay unless you tick the box to delete the app's data. If the uninstaller says it could not put everything back (a font file still open in Windows Terminal is the usual cause), it keeps the data folder on purpose: install CMD Studio again, press *Undo everything* in Help once every cmd and Terminal window is closed, then uninstall.
- Installing a newer version by hand over an older one runs the older version's uninstaller first, with its full cleanup: your applied theme is undone and the app's data is cleared (your themes are kept), and the first-run wizard starts again. It is safe, just not smooth; keeping your setup across versions is for the updater, which comes later.
- If cmd will not open: Start menu > *CMD Studio Recovery* puts back any cut-off change, pauses the theme, and opens CMD Studio in recovery mode.

## Theme files

A theme is a `.cmdtheme` file: open JSON with a published schema (`schema/cmdtheme-1.schema.json`; a `$schema` line in a file is accepted and ignored, and `uuid` needs `ajv-formats` in Ajv). Export from the app, import on another PC, and it looks the same. The built-in themes are in `presets/`.

## The command line

The installer puts `cmdstudio` on your Path. In a new cmd or PowerShell window:

| Command | Does |
|---|---|
| `cmdstudio doctor [--json]` | checks the install and says what is wrong |
| `cmdstudio apply <file.cmdtheme> [--target all\|wt] [--replace-other-clink]` | applies a theme |
| `cmdstudio undo` | undoes the last apply |
| `cmdstudio --cleanup [--remove-themes]` | what the uninstaller runs: puts everything back |
| `cmdstudio --help` | the full usage, with `--data-dir` and `--resources` |

Exit codes: 0 done, 1 failed and rolled back (or refused before anything was changed), 2 invalid input, 3 rollback incomplete (it lists what to fix by hand). `cmdstudio` is `cmdstudio.com`, a console program cmd waits for; `cmdstudio.exe` is the app's window, and typed by name it prints after the prompt returns.

## Building from source

Rust (`rust-toolchain.toml` pins it), Node (`app/.nvmrc`), and the pinned runtimes: `./scripts/fetch-resources.ps1` downloads Oh My Posh, Clink and the font, checks their SHA-256 and writes `resources/`. Then `cargo test --workspace`, and in `app/`: `npm ci`, `npm test`, `npm run tauri -- build` (the installer lands in `target/release/bundle/nsis/`). CI runs the same on every push (`.github/workflows/ci.yml`); a tag `v<version>` builds the draft release (`release.yml`).

## Credits and licenses

CMD Studio is MIT (`LICENSE`). It bundles, unmodified: [Clink](https://github.com/chrisant996/clink) 1.9.34 (GPL-3.0; the source of that exact release is linked from `resources\LICENSES\clink-SOURCE.txt`), [Oh My Posh](https://github.com/JanDeDobbeleer/oh-my-posh) 31.5.0 (MIT), and the CaskaydiaCove Nerd Font from [Nerd Fonts](https://github.com/ryanoasis/nerd-fonts) 3.5.1 (SIL OFL 1.1). The built-in themes' colors come from Tokyo Night, Catppuccin, Dracula, Nord, Gruvbox, One Dark and Solarized, all MIT; `docs/credits.md` has every copyright line, and so does the app's credits page.

## Contributing and security

Issues and pull requests at [GitHub](https://github.com/luraselenehalo/cmd-studio). For a security problem, open an issue with "security" in the title and no details, and the maintainer will reply with a private channel.
