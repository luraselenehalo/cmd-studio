//! `cmdstudio.exe`: see `cmdstudio_cli::run`.
#[cfg(windows)]
fn main() {
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
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
