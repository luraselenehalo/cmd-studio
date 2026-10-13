//! The real Oh My Posh 31.5.0 renders every layout the generator writes, with
//! every segment on and transient on and off (spec §3.5; the Phase 1a plan
//! left this check to Phase 1b). It needs the bundled runtime, so it runs only
//! when CMDSTUDIO_OMP_TESTS is exactly `1`, after scripts/fetch-resources.ps1.
//! It has a gate of its own, apart from CMDSTUDIO_SYSTEM_TESTS: it changes
//! nothing on the computer (the configs are in a temp folder, and Oh My Posh
//! gets that folder as its %LOCALAPPDATA%, so its cache stays out of the user's
//! own), and it is the one real-machine test a developer runs.
#![cfg(windows)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use cmdstudio_core::generate::omp::config_text;
use cmdstudio_core::model::{Layout, Segment};
use cmdstudio_core::presets::builtin;
use cmdstudio_core::verify::omp_config_error;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

const TEST: &str = "system_real_oh_my_posh_renders_every_layout";

/// The gate: only the exact value `1` runs the test (`0`, `true` or an empty value skip it).
fn enabled() -> bool {
    if std::env::var("CMDSTUDIO_OMP_TESTS").as_deref() == Ok("1") {
        return true;
    }
    println!("SKIPPED {TEST}: set CMDSTUDIO_OMP_TESTS=1");
    false
}

#[test]
fn system_real_oh_my_posh_renders_every_layout() {
    if !enabled() {
        return;
    }
    let exe = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(r"..\..\resources\omp\oh-my-posh.exe");
    assert!(exe.is_file(), "run scripts/fetch-resources.ps1 first");
    let tmp = tempfile::tempdir().unwrap();
    // Oh My Posh 31.5.0 writes a cache under %LOCALAPPDATA%\oh-my-posh on every run, and the
    // developer may have a setup of their own there: the child gets this folder instead.
    let local = tmp.path().join("Local");
    std::fs::create_dir_all(&local).unwrap();
    // The Terracotta preset, picked by its key (the core's own helper is test-only).
    let mut theme = builtin()
        .unwrap()
        .into_iter()
        .find(|(key, _)| *key == "terracotta")
        .unwrap()
        .1;
    theme.prompt.segments = vec![
        Segment::Path,
        Segment::Git,
        Segment::Time,
        Segment::User,
        Segment::Host,
        Segment::Status,
        Segment::Duration,
    ];
    for layout in [Layout::Box, Layout::Line, Layout::Powerline] {
        for transient in [false, true] {
            theme.prompt.layout = layout;
            theme.prompt.transient = transient;
            let config = tmp.path().join(format!("{layout:?}-{transient}.omp.json"));
            std::fs::write(&config, config_text(&theme).unwrap()).unwrap();
            let out = Command::new(&exe)
                .args(["print", "primary", "--config"])
                .arg(&config)
                .args(["--shell", "cmd", "--plain"])
                // The cache goes to `local`; OMP_CACHE_DIR, if set, would win over it.
                .env("LOCALAPPDATA", &local)
                .env_remove("OMP_CACHE_DIR")
                .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
                .output()
                .unwrap();
            let stdout = String::from_utf8_lossy(&out.stdout);
            let what = format!("{layout:?}, transient {transient}");
            assert!(
                out.status.success(),
                "{what}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            assert_eq!(omp_config_error(&stdout), None, "{what}: {stdout}");
            assert!(!stdout.trim().is_empty(), "{what}: printed nothing");
        }
    }
    // The proof that the cache went to the temp folder and not to the user's own.
    assert!(
        local.join("oh-my-posh").is_dir(),
        "Oh My Posh did not use the LOCALAPPDATA it was given, so it may have written to the real one"
    );
}
