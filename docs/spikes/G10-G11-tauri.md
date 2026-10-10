# G10, G11 — Tauri facts from shipped source

- **Date:** 2026-10-10.
- **Method:** read only. Raw files from the tauri repo at release tags, plus the crates.io and npm registries.
- **Where it ran:** dev machine, network reads only.

## Versions (2026-10-10)

| Package | Version | Source |
|---|---|---|
| `tauri` (crate) | 2.12.2 | crates.io `max_stable_version` |
| `@tauri-apps/cli` | 2.12.1 | `npm view` |
| `@tauri-apps/api` | 2.12.2 | `npm view` |
| `tauri-bundler` | 2.10.1 (newest 3.0.0-alpha.3) | crates.io |
| `tauri-plugin-updater` | 2.13.2 | crates.io |
| `tauri-specta` | 1.0.2 stable (Tauri 1 only), **2.0.0-rc.25** (Tauri 2, published 2026-05-08) | crates.io |
| `ts-rs` (alternative) | 12.0.1 | crates.io |

## G10 — NSIS installer: ✅

Source: `https://raw.githubusercontent.com/tauri-apps/tauri/tauri-bundler-v2.10.1/crates/tauri-bundler/src/bundle/windows/nsis/installer.nsi` (978 lines).

**Install folder for `currentUser`** (line 514–515):
```nsis
    !else if "${INSTALLMODE}" == "currentUser"
      StrCpy $INSTDIR "$LOCALAPPDATA\${PRODUCTNAME}"
```
So CMD Studio installs to `%LOCALAPPDATA%\CMD Studio`. No admin rights are needed.

**`NSISInstallerMode`** in `tauri-utils` at `tauri-v2.12.2` (`crates/tauri-utils/src/config.rs`, lines 821–837):
- It is serde camelCase.
- `#[default] CurrentUser` means "Install the app by default in a directory that doesn't require Administrator access. Installer metadata will be saved under the `HKCU` registry path."
- The config key is `bundle > windows > nsis > installMode`.

**Hooks.** Config key `installerHooks` (alias `installer-hooks`), `Option<PathBuf>`, at line 941–942. The template calls `NSIS_HOOK_PREINSTALL` (642), `NSIS_HOOK_POSTINSTALL` (734), `NSIS_HOOK_PREUNINSTALL` (779) and `NSIS_HOOK_POSTUNINSTALL` (887).

**Order in `Section Uninstall`** (lines 777 onward):
1. `NSIS_HOOK_PREUNINSTALL` runs first, **before** any file is deleted. The `--cleanup` design holds.
2. The hook runs with no condition, **including update mode**. `$UpdateMode` is set to 1 when the uninstaller runs with `/UPDATE`.
3. Later in the section, `${If} $DeleteAppDataCheckboxState = 1 ${AndIf} $UpdateMode <> 1` runs `RmDir /r "$APPDATA\${BUNDLEID}"` and `RmDir /r "$LOCALAPPDATA\${BUNDLEID}"`.

**Webview2** (`WebviewInstallMode`, lines 963–1006):
- The default is `DownloadBootstrapper { silent: true }`.
- `EmbedBootstrapper` "Requires an internet connection. Increases the installer size by around 1.8MB, but offers better support on Windows 7."
- Both need the internet only when WebView2 is missing.

## App data folder: ✅

`crates/tauri/src/path/desktop.rs` at `tauri-v2.12.2`, lines 265–271:
```rust
  pub fn app_local_data_dir(&self) -> Result<PathBuf> {
    self.app_dir(AppDirectory::LocalData, || {
      dirs::data_local_dir()
        .ok_or(Error::UnknownPath)
        .map(|dir| dir.join(&self.0.config().identifier))
    })
  }
```
With identifier `ai.resincore.cmdstudio`, the data folder is `%LOCALAPPDATA%\ai.resincore.cmdstudio`. It is separate from the install folder `%LOCALAPPDATA%\CMD Studio`, as the spec change in §5.3 intended.

## G11 — typed bindings between Rust and TypeScript: ✅ compatible, ⚠ pre-release

`tauri-specta 2.0.0-rc.25` depends on `tauri ^2` (works with 2.12.2) and `specta =2.0.0-rc.25`. There is no stable Tauri 2 release of tauri-specta.

There are two ways to proceed, and the founder decides in Phase 2:
- **tauri-specta rc.25, pinned exactly.** It generates both the types and the command wrappers, but it is a release candidate. Its last release was 5 months ago.
- **ts-rs 12.0.1 (stable).** It generates the types only. The roughly 20 IPC commands need small hand-written typed wrappers.
- **Recommendation:** ts-rs. The wrappers are trivial, and it avoids a pre-release dependency in a public app.

## Spec changes this implies (to apply in Task 9)

1. **§6.3 (cleanup hook).** The hook must skip `--cleanup` when `$UpdateMode = 1`. Otherwise every update would revert the user's theme. It must pass `--remove-themes` when `$DeleteAppDataCheckboxState = 1`. NSIS then deletes `%LOCALAPPDATA%\ai.resincore.cmdstudio` itself. The spec's "keep themes unless the user ticks it" maps onto that checkbox.
2. **§6.1.** Keep the default `downloadBootstrapper`. `embedBootstrapper` only helps on Windows 7, which is out of scope.
3. **§0 G11.** Record that tauri-specta for Tauri 2 is still a release candidate, and offer ts-rs as the stable alternative.
