//! `cmdstudio.com`: the CLI as a console-subsystem program (spec §5.7). cmd waits for it, shows
//! what it prints as it prints, and gets its exit code; `cmdstudio.exe` stays the window, a
//! Windows-subsystem exe cmd does not wait for. Built as `cmdstudio-cli.exe`, which the installer
//! puts beside the app and renames to `cmdstudio.com` (`windows/hooks.nsh`), where cmd's PATHEXT
//! (`.COM` before `.EXE`) picks it for a bare `cmdstudio`. The uninstaller runs
//! `cmdstudio.com --cleanup` (spec §6.3) and the installer `cmdstudio.com --add-to-path`.
//!
//! `--recovery` alone belongs to the window: it is handed to `cmdstudio.exe` in the same folder
//! (recovery spec §4.4) and this process ends with 0 once that one has started. Everything else
//! is `cmdstudio_cli::run`, as in `main.rs`, and the crash-note hook comes first, as there.
#[cfg(windows)]
fn main() {
    use cmdstudio_app::console::{console_start, start_app_beside, ConsoleStart};
    if let Ok(backend) = cmdstudio_app::machine::real_backend() {
        cmdstudio_app::crash::install_hook(
            backend.paths().logs(),
            backend.redaction(),
            cmdstudio_cli::now_ms,
        );
    }
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    let code = match console_start(&args) {
        ConsoleStart::OpenRecovery => match start_app_beside(&args) {
            Ok(()) => cmdstudio_cli::EXIT_OK,
            Err(why) => {
                eprintln!("cmdstudio: E_INTERNAL: {why}");
                cmdstudio_cli::EXIT_FAILED
            }
        },
        ConsoleStart::Cli => {
            let (mut out, mut err) = (std::io::stdout(), std::io::stderr());
            cmdstudio_cli::run(
                &args,
                &mut cmdstudio_cli::Io {
                    out: &mut out,
                    err: &mut err,
                },
            )
        }
    };
    std::process::exit(code);
}

#[cfg(not(windows))]
fn main() {
    eprintln!("CMD Studio runs on Windows only.");
    std::process::exit(cmdstudio_cli::EXIT_INVALID);
}
