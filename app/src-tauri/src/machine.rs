//! The real machine behind the app: the core's Windows adapters, wired as the CLI's `run` wires
//! them (`crates/cli/src/lib.rs`), so the app and the CLI see the same data folder, the same
//! bundled files beside the exe, and may run programs only from the runtime folders (spec §5.10).
use crate::backend::preview::SampleIdentity;
use crate::backend::{Backend, Machine};
use cmdstudio_core::adapters::windows::{
    eventlog::WinEventLog, fonts::WinFonts, fs::WinFs, host::WinHost, process::WinProcess,
    registry::WinAutoRun, wt::WinWt,
};
use cmdstudio_core::adapters::{program_data_dir, Host, System};
use std::path::PathBuf;
use std::time::Duration;

pub struct RealMachine {
    fs: WinFs,
    autorun: WinAutoRun,
    fonts: WinFonts,
    wt: WinWt,
    process: WinProcess,
    host: WinHost,
    event_log: WinEventLog,
}

impl Machine for RealMachine {
    fn system(&self) -> System<'_> {
        System {
            fs: &self.fs,
            autorun: &self.autorun,
            fonts: &self.fonts,
            wt: &self.wt,
            process: &self.process,
            host: &self.host,
            event_log: &self.event_log,
        }
    }
}

/// The backend over this computer: `%LOCALAPPDATA%\ai.resincore.cmdstudio` as the data folder,
/// `<exe folder>\resources` as the bundled files, the clock. Fails only when Windows cannot say
/// where the user's local app data or the exe is.
pub fn real_backend() -> Result<Backend<RealMachine>, String> {
    let host = WinHost;
    let local = host
        .local_app_data()
        .map_err(|e| format!("the user's local app data folder could not be found: {e}"))?;
    let data = local.join(cmdstudio_cli::DATA_DIR_NAME);
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(PathBuf::from))
        .ok_or_else(|| "the folder of cmdstudio.exe could not be found".to_string())?;
    // Only programs from the runtime folders may run, each for 60 seconds at most.
    let mut allowed = vec![data.join("runtime")];
    if let (Ok(pd), Ok(sid)) = (host.program_data(), host.user_sid()) {
        allowed.push(program_data_dir(&pd, &sid));
    }
    let machine = RealMachine {
        fs: WinFs,
        autorun: WinAutoRun::real(),
        fonts: WinFonts::real(&local),
        wt: WinWt::new(local.clone()),
        process: WinProcess::new(allowed, Duration::from_secs(60)),
        host,
        event_log: WinEventLog::real(),
    };
    // The one place the app reads the environment for the preview's sample (spec §5.6); a test
    // builds a `SampleIdentity` from a table instead.
    let sample = SampleIdentity::from_env(|name| std::env::var(name).ok());
    Ok(Backend::new(
        machine,
        data,
        exe_dir.join("resources"),
        Box::new(cmdstudio_cli::now_ms),
        sample,
    ))
}
