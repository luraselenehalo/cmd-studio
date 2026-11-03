//! `cmdstudio.exe`: the app and the CLI in one exe (spec §5.7). Started with no arguments it
//! opens the window; with `--recovery` alone, the window in recovery mode (recovery spec §4.4);
//! with any other, it runs `cmdstudio_cli::run` and exits with its code, so the
//! uninstaller's `--cleanup`, `cmdstudio doctor` and the rest are the same program.
//!
//! A release build is a Windows-subsystem exe (no console window flashes when the app starts),
//! so in CLI mode it first attaches to the console it was started from, unless its output was
//! redirected (`console::attach_parent`: `> out.txt` and `| more` keep their redirection).
//! cmd does not wait for a Windows-subsystem program, so `cmdstudio doctor` typed in cmd prints
//! after the prompt came back; `cmdstudio doctor | more`, or `start /wait cmdstudio doctor`,
//! waits. The uninstaller runs it with `ExecWait`, and tests call the library: both wait.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(windows)]
fn main() {
    use cmdstudio_app::backend::run::GuiStart;
    // The crash notes, for the app and the CLI alike (recovery spec §5.3): the default data
    // folder's logs\ (a `--data-dir` run's note goes there too), redacted as the diagnostics
    // redact. A backend that cannot be made (no local app data) installs no hook.
    if let Ok(backend) = cmdstudio_app::machine::real_backend() {
        cmdstudio_app::crash::install_hook(
            backend.paths().logs(),
            backend.redaction(),
            cmdstudio_cli::now_ms,
        );
    }
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    // `--recovery` alone opens the window in recovery mode (recovery spec §4.4, §8): the
    // Start-menu shortcut "CMD Studio Recovery". Every other argument is the CLI's.
    match args.as_slice() {
        [] => return cmdstudio_app::run_gui(GuiStart::Normal),
        [one] if one == cmdstudio_app::RECOVERY_ARG => {
            return cmdstudio_app::run_gui(GuiStart::Recovery)
        }
        _ => {}
    }
    cmdstudio_app::console::attach_parent();
    let (mut out, mut err) = (std::io::stdout(), std::io::stderr());
    let code = cmdstudio_cli::run(
        &args,
        &mut cmdstudio_cli::Io {
            out: &mut out,
            err: &mut err,
        },
    );
    std::process::exit(code);
}

#[cfg(not(windows))]
fn main() {
    eprintln!("CMD Studio runs on Windows only.");
    std::process::exit(cmdstudio_cli::EXIT_INVALID);
}
