//! `cmdstudio.exe`: the app and the CLI in one exe (spec §5.7). Started with no arguments it
//! opens the window; with any, it runs `cmdstudio_cli::run` and exits with its code, so the
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
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    if args.is_empty() {
        cmdstudio_app::run_gui();
        return;
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
