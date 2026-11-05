//! Guards over `tauri.conf.json` (tests only). No cargo build enables `tauri/custom-protocol`, so
//! every exe is a "dev build" to tauri-codegen, which loads `build.devUrl` instead of the embedded
//! assets when the key is set: whatever served that port would then hold the IPC of all 37
//! commands. The config must not have it, must carry the CSP of the Global Constraints and
//! nothing else under `app.security`, and must not create the window at startup (Task 15 builds
//! it with a data directory).
use serde_json::Value;

const CONFIG: &str = include_str!("../tauri.conf.json");

/// The bundle's configuration, passed with `--config` only when bundling (G-CFG): never part of
/// the committed tauri.conf.json, so that a cargo build needs no resources/.
const BUNDLE: &str = include_str!("../tauri.bundle.conf.json");
const HOOKS: &str = include_str!("../windows/hooks.nsh");

/// The hooks file without its comments: everything from `;` to the end of a line is NSIS
/// comment, and the pins below count code only (no text of this file holds a `;` in a string).
/// Each line is trimmed: the pins hold no indentation, which differs with the nesting.
fn hooks_code() -> String {
    HOOKS
        .lines()
        .map(|line| line.split(';').next().unwrap_or("").trim())
        .collect::<Vec<_>>()
        .join("\n")
}

/// The text of the one `!macro NAME` … `!macroend` block of the code.
fn hook(code: &str, name: &str) -> String {
    let start = code
        .find(&format!("!macro {name}"))
        .unwrap_or_else(|| panic!("no macro {name}"));
    let end = code
        .get(start..)
        .and_then(|rest| rest.find("!macroend"))
        .unwrap_or_else(|| panic!("{name} has no end"));
    code.get(start..start + end).unwrap().to_string()
}

/// The lines inside the `${If}` whose first line is `opener` (matched whole, on the trimmed
/// code), up to its own `${EndIf}`: nested `${If}` … `${EndIf}` pairs are counted, so an inner
/// `${EndIf}` does not end it (`${IfThen}` is one line and opens nothing).
fn inside_if(code: &str, opener: &str) -> String {
    let mut lines = code.lines().skip_while(|line| *line != opener);
    assert_eq!(lines.next(), Some(opener), "missing: {opener}");
    let mut depth = 1;
    let mut inside = Vec::new();
    // Blank lines (a comment's, once stripped) are left out.
    for line in lines.filter(|line| !line.is_empty()) {
        if line.starts_with("${If} ") {
            depth += 1;
        }
        if line == "${EndIf}" {
            depth -= 1;
            if depth == 0 {
                return inside.join("\n");
            }
        }
        inside.push(line);
    }
    panic!("{opener} has no ${{EndIf}}");
}

/// `a` comes before `b` in `text`, and both are there.
fn before(text: &str, a: &str, b: &str) {
    let ia = text.find(a).unwrap_or_else(|| panic!("missing: {a}"));
    let ib = text.find(b).unwrap_or_else(|| panic!("missing: {b}"));
    assert!(ia < ib, "{a:?} must come before {b:?}");
}

/// A JSON deep merge, as Tauri merges a `--config` file over tauri.conf.json: objects by key,
/// anything else replaced.
fn merge(base: &mut Value, over: Value) {
    match (base, over) {
        (Value::Object(b), Value::Object(o)) => {
            for (k, v) in o {
                merge(b.entry(k).or_insert(Value::Null), v);
            }
        }
        (b, o) => *b = o,
    }
}

/// The three-line result check that follows every nsExec call (P8): nsExec answers the text
/// `error` when the program could not start and `timeout` when it did not end, so the exit
/// code is compared as text, never as a number.
const RESULT_CHECK: &str = "${If} $0 == \"error\"\n${OrIf} $0 == \"timeout\"\n${OrIf} $0 S!= \"0\"";

/// Spec §6.1, §6.3, G10, G-CFG: the bundle file is a per-user NSIS installer with the hooks
/// file, the resources with their licenses and the app's LICENSE (two entries, no more: the
/// console exe is bundled by Tauri under its cargo name), one Start-menu folder, Tauri's default
/// WebView2 bootstrapper, no updater artifacts until 4b; the committed config keeps the bundle
/// off and names no resource; the two merged are a config Tauri accepts.
#[test]
fn the_bundle_file_is_a_per_user_nsis_installer_and_the_committed_config_stays_buildable() {
    let bundle_file: Value = serde_json::from_str(BUNDLE).unwrap();
    let bundle = bundle_file.get("bundle").filter(|b| b.is_object()).unwrap();
    assert_eq!(
        bundle_file.as_object().unwrap().keys().collect::<Vec<_>>(),
        ["bundle"],
        "the bundle block only"
    );
    assert_eq!(bundle.get("active"), Some(&Value::Bool(true)));
    assert_eq!(bundle.get("targets"), Some(&serde_json::json!(["nsis"])));
    assert!(
        bundle.get("createUpdaterArtifacts").is_none(),
        "4b turns the updater artifacts on"
    );
    let resources = bundle.get("resources").and_then(Value::as_object).unwrap();
    assert_eq!(resources.len(), 2);
    assert_eq!(
        resources.get("../../resources").and_then(Value::as_str),
        Some("resources")
    );
    assert_eq!(
        resources.get("../../LICENSE").and_then(Value::as_str),
        Some("LICENSE.txt")
    );
    let nsis = bundle
        .pointer("/windows/nsis")
        .and_then(Value::as_object)
        .unwrap();
    assert_eq!(
        nsis.get("installMode").and_then(Value::as_str),
        Some("currentUser")
    );
    assert_eq!(
        nsis.get("installerHooks").and_then(Value::as_str),
        Some("windows/hooks.nsh")
    );
    assert_eq!(
        nsis.get("startMenuFolder").and_then(Value::as_str),
        Some("CMD Studio")
    );
    assert!(
        bundle.pointer("/windows/webviewInstallMode").is_none(),
        "Tauri's default: downloadBootstrapper (G10)"
    );

    let mut config: Value = serde_json::from_str(CONFIG).unwrap();
    assert_eq!(
        config.pointer("/bundle/active"),
        Some(&Value::Bool(false)),
        "the committed config bundles nothing"
    );
    assert!(
        config.pointer("/bundle/resources").is_none(),
        "a resource here would be copied at every cargo build (G-CFG)"
    );
    assert!(config.pointer("/build/beforeBundleCommand").is_none());
    assert_eq!(
        config.get("mainBinaryName").and_then(Value::as_str),
        Some("cmdstudio")
    );
    merge(&mut config, bundle_file);
    let merged: tauri::utils::config::Config = serde_json::from_value(config).unwrap();
    assert!(merged.bundle.active);
}

/// Review Focus 5 (the install half): the console exe renamed, the rename checked, and the
/// Recovery shortcut made after the files, on every install, an update's included (POSTINSTALL
/// carries no update guard), but not under `/NS`, where Tauri makes no shortcut either; ARM64
/// refused before any file.
#[test]
fn the_recovery_shortcut_is_made_after_every_install() {
    let code = hooks_code();
    for name in [
        "NSIS_HOOK_PREINSTALL",
        "NSIS_HOOK_POSTINSTALL",
        "NSIS_HOOK_PREUNINSTALL",
        "NSIS_HOOK_POSTUNINSTALL",
    ] {
        assert_eq!(code.matches(&format!("!macro {name}")).count(), 1, "{name}");
    }
    let pre = hook(&code, "NSIS_HOOK_PREINSTALL");
    before(&pre, "${If} ${IsNativeARM64}", "Abort");
    let post = hook(&code, "NSIS_HOOK_POSTINSTALL");
    let recovery = "CreateShortcut \"$SMPROGRAMS\\$AppStartMenuFolder\\CMD Studio Recovery.lnk\" \"$INSTDIR\\cmdstudio.exe\" \"--recovery\"";
    let rename = "Rename \"$INSTDIR\\cmdstudio-cli.exe\" \"$INSTDIR\\cmdstudio.com\"";
    // The Delete and the Rename are checked together: a .com that cannot be deleted makes the
    // Rename fail too (Rename does not replace a file), and Delete sets no error for a file that
    // is not there.
    before(&post, "ClearErrors", "Delete \"$INSTDIR\\cmdstudio.com\"");
    before(&post, "Delete \"$INSTDIR\\cmdstudio.com\"", rename);
    before(&post, rename, "${If} ${Errors}");
    let failed = inside_if(&post, "${If} ${Errors}");
    assert!(
        failed.contains("MessageBox"),
        "the failed rename has its box"
    );
    assert!(failed.contains("console program could not be put in place"));
    assert!(
        failed.contains("cmdstudio.exe"),
        "the box says the app still runs"
    );
    assert!(
        !failed.contains("Abort"),
        "the install goes on to the Path step"
    );
    before(
        &post,
        "${If} ${Errors}",
        "CreateDirectory \"$SMPROGRAMS\\$AppStartMenuFolder\"",
    );
    // /NS: Tauri's CreateOrUpdateStartMenuShortcut returns when $NoShortcutMode = 1 unless it
    // migrates from WiX (template ~:27639-27643); the Recovery shortcut follows the same rule.
    let shortcut = inside_if(&post, "${If} $WixMode = 1");
    assert_eq!(
        shortcut.lines().next(),
        Some("${OrIf} $NoShortcutMode <> 1")
    );
    before(
        &shortcut,
        "CreateDirectory \"$SMPROGRAMS\\$AppStartMenuFolder\"",
        recovery,
    );
    assert!(
        !shortcut.contains("nsExec"),
        "the Path step runs under /NS too"
    );
    before(
        &post,
        recovery,
        "nsExec::ExecToStack '\"$INSTDIR\\cmdstudio.com\" --add-to-path'",
    );
    assert!(
        !post.contains("$UpdateMode"),
        "an update gets the .com and the shortcut again"
    );
    assert!(
        !HOOKS
            .chars()
            .any(|c| ('\u{0E00}'..='\u{0E7F}').contains(&c)),
        "no Thai"
    );
    assert!(
        !HOOKS.contains("cmdstudio.exe\" --"),
        "every call goes through the console exe"
    );
}

/// Review Focus 5 (the uninstall half), spec §6.3: the app-running check, the cleanup and the
/// Path removal run only outside update mode, before any file is deleted, the check first (Tauri
/// runs its own CheckIfAppIsRunning only after PREUNINSTALL, and a cleanup under a running app
/// would be undone by it); the two deletes run in POSTUNINSTALL under the same guard, after
/// Tauri's own deletes, and the folders they empty are removed. An update's uninstaller leaves
/// the Path entry, the shortcut and the .com for the new installer's POSTINSTALL.
#[test]
fn the_path_and_the_cleanup_run_only_outside_update_mode() {
    let code = hooks_code();
    assert_eq!(
        code.matches("$UpdateMode").count(),
        2,
        "one guard in PREUNINSTALL, one in POSTUNINSTALL"
    );
    let pre = hook(&code, "NSIS_HOOK_PREUNINSTALL");
    let inside = inside_if(&pre, "${If} $UpdateMode <> 1");
    assert_eq!(
        inside.lines().next(),
        Some("!insertmacro CheckIfAppIsRunning \"$INSTDIR\\${MAINBINARYNAME}.exe\" \"${PRODUCTNAME}\""),
        "the app-running check comes first"
    );
    // Final review Minor 2: the console exe is checked too, inside the guard, before the first
    // nsExec (a `cmdstudio apply` still running would make the cleanup busy and keep the .com).
    let com_check =
        "!insertmacro CheckIfAppIsRunning \"$INSTDIR\\cmdstudio.com\" \"${PRODUCTNAME}\"";
    assert_eq!(
        inside.lines().nth(1),
        Some(com_check),
        "the console exe's check follows the app's"
    );
    assert_eq!(pre.matches(com_check).count(), 1);
    before(&inside, com_check, "nsExec::ExecToStack");
    before(
        &inside,
        "!insertmacro CheckIfAppIsRunning",
        "nsExec::ExecToStack",
    );
    for line in [
        "nsExec::ExecToStack '\"$INSTDIR\\cmdstudio.com\" --cleanup --remove-themes'",
        "nsExec::ExecToStack '\"$INSTDIR\\cmdstudio.com\" --cleanup'",
        "nsExec::ExecToStack '\"$INSTDIR\\cmdstudio.com\" --remove-from-path'",
    ] {
        assert!(inside.contains(line), "inside the update guard: {line}");
    }
    assert_eq!(
        pre.matches("nsExec::ExecToStack").count(),
        inside.matches("nsExec::ExecToStack").count(),
        "no call outside the guard"
    );
    assert!(!pre.contains("Delete "), "PREUNINSTALL deletes nothing");
    before(&inside, "--cleanup'", "--remove-from-path'");

    let post = hook(&code, "NSIS_HOOK_POSTUNINSTALL");
    let deletes = inside_if(&post, "${If} $UpdateMode <> 1");
    let lnk = "Delete \"$SMPROGRAMS\\$AppStartMenuFolder\\CMD Studio Recovery.lnk\"";
    let com = "Delete \"$INSTDIR\\cmdstudio.com\"";
    assert_eq!(post.matches("Delete ").count(), 2, "the two deletes only");
    assert!(
        deletes.contains(lnk) && deletes.contains(com),
        "inside the guard"
    );
    // Tauri's RMDir of the Start-menu folder and of $INSTDIR ran before this hook, while these
    // two files were still there; each RMDir removes only an empty folder.
    before(&deletes, lnk, "RMDir \"$SMPROGRAMS\\$AppStartMenuFolder\"");
    before(&deletes, com, "RMDir \"$INSTDIR\"");
}

/// Review Focus 3, spec §6.3: `--remove-themes` only when the box is ticked; a cleanup that
/// does not answer 0 (a failed step, a refusal, or nsExec's `error`/`timeout`) un-ticks the box
/// inside its own failure branch, so NSIS keeps the data folder; the box's text sends the user
/// to a new install and Undo everything, never to a program this uninstall removes.
#[test]
fn a_failed_cleanup_keeps_the_data_folder() {
    let code = hooks_code();
    let pre = hook(&code, "NSIS_HOOK_PREUNINSTALL");
    before(
        &pre,
        "${If} $DeleteAppDataCheckboxState = 1",
        "--cleanup --remove-themes'",
    );
    before(&pre, "--cleanup --remove-themes'", "${Else}");
    before(&pre, "${Else}", "--cleanup'");
    let after_cleanup = pre.get(pre.find("--cleanup'").unwrap()..).unwrap();
    before(after_cleanup, "Pop $0", "Pop $1");
    before(after_cleanup, "Pop $1", RESULT_CHECK);
    let branch = inside_if(after_cleanup, "${If} $0 == \"error\"");
    assert!(
        branch.contains("StrCpy $DeleteAppDataCheckboxState 0"),
        "inside the failure branch"
    );
    // Four calls in three places: the two cleanups are the arms of one ${If}, with one check.
    assert_eq!(code.matches("nsExec::ExecToStack").count(), 4);
    assert_eq!(
        code.matches(RESULT_CHECK).count(),
        3,
        "every call's result is compared as text"
    );
    let text = HOOKS;
    let cleanup_box = text
        .get(text.find("could not put everything back").unwrap()..)
        .unwrap();
    let cleanup_box = cleanup_box
        .get(..cleanup_box.find("/SD IDOK").unwrap())
        .unwrap();
    assert!(cleanup_box.contains("install CMD Studio again"));
    assert!(cleanup_box.contains("Undo everything"));
    assert!(
        !cleanup_box.contains("cmdstudio --cleanup") && !cleanup_box.contains("cmdstudio.com"),
        "names no program the uninstall removes"
    );
}

/// The Start-menu folder: the bundle file's default and the hooks' use of Tauri's own variable
/// (the user may change the folder on the installer's page) agree, and the shortcut's name is
/// written once for the making and once for the deleting.
#[test]
fn the_start_menu_folder_is_one_name() {
    let bundle_file: Value = serde_json::from_str(BUNDLE).unwrap();
    assert_eq!(
        bundle_file
            .pointer("/bundle/windows/nsis/startMenuFolder")
            .and_then(Value::as_str),
        Some("CMD Studio")
    );
    let code = hooks_code();
    assert_eq!(
        code.matches("$SMPROGRAMS\\$AppStartMenuFolder\\CMD Studio Recovery.lnk")
            .count(),
        2
    );
    assert!(
        !code.contains("$SMPROGRAMS\\CMD Studio\\"),
        "never the literal folder: the user may have renamed it"
    );
    // The uninstaller is a new process: $AppStartMenuFolder holds the folder only after Tauri's
    // MUI_STARTMENU_GETFOLDER (template ~:27527), which runs inside Tauri's own
    // `$UpdateMode <> 1` block, after PREUNINSTALL and before POSTUNINSTALL. The shortcut is
    // deleted in POSTUNINSTALL under the same guard, so the folder is set there.
    let post = hook(&code, "NSIS_HOOK_POSTUNINSTALL");
    assert!(
        inside_if(&post, "${If} $UpdateMode <> 1")
            .contains("Delete \"$SMPROGRAMS\\$AppStartMenuFolder\\CMD Studio Recovery.lnk\""),
        "the shortcut's Delete runs where Tauri has read the folder back"
    );
    assert!(
        !hook(&code, "NSIS_HOOK_PREUNINSTALL").contains("$AppStartMenuFolder"),
        "PREUNINSTALL, where the folder is not read yet, never uses it"
    );
}

/// The CSP of the Global Constraints (spec §5.10): no remote origin.
const CSP: &str = "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self'; \
                   font-src 'self'; connect-src ipc: http://ipc.localhost; object-src 'none'; \
                   base-uri 'self'; frame-ancestors 'none'";

#[test]
fn the_config_serves_only_the_embedded_ui_under_the_csp_and_creates_no_window() {
    let config: Value = serde_json::from_str(CONFIG).unwrap();

    // (a) No dev server: the window may not load http://localhost:<port> with the IPC.
    let build = config.get("build").and_then(Value::as_object).unwrap();
    assert!(!build.contains_key("devUrl"), "build.devUrl is set");

    // (b) The CSP, exactly, and no other security key (no devCsp, no dangerous* switch).
    let security = config
        .pointer("/app/security")
        .and_then(Value::as_object)
        .unwrap();
    assert_eq!(
        security.keys().map(String::as_str).collect::<Vec<_>>(),
        ["csp"]
    );
    assert_eq!(security.get("csp").and_then(Value::as_str), Some(CSP));

    // (c) One window, not created at startup.
    let windows = config
        .pointer("/app/windows")
        .and_then(Value::as_array)
        .unwrap();
    assert_eq!(windows.len(), 1, "one window");
    assert_eq!(windows[0].get("create"), Some(&Value::Bool(false)));
}

/// The window's default is 1100x760 and its minimum 900x620 (logical pixels). A screen smaller than
/// the default, such as a 1024x768 one, has a work area (the screen less the taskbar) of about
/// 1024x728: Windows then makes the window as big as the screen, and its bottom, with the page's
/// last buttons, lies under the taskbar (phase 3 VM check, Run 5). `preventOverflow` makes Tauri
/// reduce the window's inner size, as the window is made, so that the whole window, its title bar
/// included, fits the work area of the primary monitor, and centers it there. Tauri does the sums in
/// physical pixels with the monitor's scale factor, adds the title bar and the frame itself, and
/// leaves the size of the config alone when no monitor can be read. A work area smaller than the
/// minimum gives the minimum, which Windows enforces. The sizes themselves stay as they are.
#[test]
fn the_window_is_made_to_fit_the_work_area_and_keeps_its_sizes() {
    use tauri::utils::config::{Config, PreventOverflowConfig};

    let config: Config = serde_json::from_str(CONFIG).unwrap();
    let main = config
        .app
        .windows
        .iter()
        .find(|w| w.label == "main")
        .unwrap();
    assert_eq!(
        main.prevent_overflow,
        Some(PreventOverflowConfig::Enable(true))
    );
    assert!(main.center, "centered in the work area");
    assert_eq!((main.width, main.height), (1100.0, 760.0));
    assert_eq!(
        (main.min_width, main.min_height),
        (Some(900.0), Some(620.0))
    );
}

/// Spec §6.4, phase 4a: the version is `[workspace.package] version` in the root Cargo.toml and
/// nowhere else. tauri.conf.json carries none (Tauri reads Cargo's: the installer's file name
/// and the updater's own version come from it); the UI's package.json and its lockfile say the
/// same string; every crate inherits it.
#[test]
fn the_version_has_one_source() {
    let version = env!("CARGO_PKG_VERSION");
    let config: Value = serde_json::from_str(CONFIG).unwrap();
    assert!(
        config.get("version").is_none(),
        "tauri.conf.json must not carry a version"
    );
    let package: Value = serde_json::from_str(include_str!("../../package.json")).unwrap();
    assert_eq!(
        package.get("version").and_then(Value::as_str),
        Some(version),
        "app/package.json"
    );
    let lock: Value = serde_json::from_str(include_str!("../../package-lock.json")).unwrap();
    assert_eq!(
        lock.get("version").and_then(Value::as_str),
        Some(version),
        "app/package-lock.json"
    );
    assert_eq!(
        lock.pointer("/packages//version").and_then(Value::as_str),
        Some(version),
        "the lockfile's root package"
    );
    let root = include_str!("../../../Cargo.toml");
    assert!(
        root.contains(&format!("\nversion = \"{version}\"\n")),
        "the workspace's version"
    );
    for (name, toml) in [
        ("app", include_str!("../Cargo.toml")),
        ("core", include_str!("../../../crates/core/Cargo.toml")),
        ("cli", include_str!("../../../crates/cli/Cargo.toml")),
    ] {
        assert!(
            toml.contains("version.workspace = true"),
            "{name} inherits the version"
        );
    }
}

/// The app crate has two binaries (`cmdstudio` and `cmdstudio-cli`), so the Tauri CLI finds the
/// main one only through `default-run`; it is `mainBinaryName` of `tauri.conf.json`.
#[test]
fn the_main_binary_is_the_default_run_of_the_app_crate() {
    let toml = include_str!("../Cargo.toml");
    assert!(
        toml.contains("\ndefault-run = \"cmdstudio\"\n"),
        "default-run names the main binary"
    );
    let config: Value = serde_json::from_str(CONFIG).unwrap();
    assert_eq!(config["mainBinaryName"], "cmdstudio");
}
