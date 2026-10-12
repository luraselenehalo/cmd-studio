//! The real `txn::Verifier` (spec §5.5): after every op ran, (1) read every
//! written value back, (2) Oh My Posh renders the generated config, (3) Clink
//! loads the scripts without a Lua error, (4) every written settings.json
//! parses. Runs only CMD Studio's own bundled programs (spec §5.10).
use crate::adapters::System;
use crate::exec::font_target;
use crate::generate::wt::CMD_PROFILE_GUID;
use crate::plan::{ClinkLaunch, Op, Paths};
use crate::resources::{check_bundle, sha256_hex, Manifest, Resources};
use crate::txn::{Failure, Verifier};
use crate::wt_edit::{check_parses, read_value, WtKey};
use serde_json::Value;
use std::ffi::OsStr;
use std::path::Path;

/// What Oh My Posh 31.5.0 prints instead of the prompt when it cannot use the
/// config; it still exits 0 (`src/config/load.go` at v31.5.0).
pub const OMP_CONFIG_ERRORS: [&str; 6] = [
    "CONFIG NOT FOUND",
    "INVALID CONFIG EXTENSION",
    "INVALID CONFIG THEME",
    "CONFIG URL FETCH FAILED",
    "CONFIG PARSE ERROR",
    "NO CONFIG",
];

/// The first line that reports a Lua error: `<script>.lua:<line>:` (G5). Windows ignores the
/// letter case of a file name, so `.LUA:` counts too.
pub fn lua_error_line(output: &str) -> Option<&str> {
    output.lines().find(|line| {
        // ASCII lowering keeps every byte offset, so only the text after the match is read.
        let lower = line.to_ascii_lowercase();
        lower.match_indices(".lua:").any(|(i, m)| {
            let rest = lower.get(i + m.len()..).unwrap_or_default();
            let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
            digits > 0 && rest.as_bytes().get(digits) == Some(&b':')
        })
    })
}

/// The Oh My Posh config error in `stdout`, if any.
pub fn omp_config_error(stdout: &str) -> Option<&'static str> {
    OMP_CONFIG_ERRORS
        .into_iter()
        .find(|marker| stdout.contains(marker))
}

/// Where cmd's AutoRun value lives, for a message.
const AUTORUN_KEY: &str = r"HKEY_CURRENT_USER\Software\Microsoft\Command Processor";

fn fail(code: &'static str, message: impl std::fmt::Display) -> Failure {
    Failure {
        code,
        message: message.to_string(),
    }
}

/// The start of a program's output, for a message: a few lines at most.
fn clip(text: &str) -> String {
    const MAX: usize = 300;
    let text = text.trim();
    let mut clipped: String = text.chars().take(MAX).collect();
    if text.chars().nth(MAX).is_some() {
        clipped.push_str("...");
    }
    clipped
}

pub struct SystemVerifier<'a> {
    sys: System<'a>,
    paths: &'a Paths,
    resources: &'a Resources,
    launch: Option<ClinkLaunch>,
    manifest: Option<Manifest>,
}

impl<'a> SystemVerifier<'a> {
    /// `launch` is `<clink-launch>`; `None` when no Clink script is written (revert all).
    pub fn new(
        sys: System<'a>,
        paths: &'a Paths,
        resources: &'a Resources,
        launch: Option<ClinkLaunch>,
    ) -> Self {
        SystemVerifier {
            sys,
            paths,
            resources,
            launch,
            manifest: None,
        }
    }

    fn manifest(&mut self) -> Result<&Manifest, Failure> {
        if self.manifest.is_none() {
            let m = self
                .resources
                .manifest(self.sys.fs)
                .map_err(|e| fail(e.code(), e))?;
            self.manifest = Some(m);
        }
        self.manifest
            .as_ref()
            .ok_or_else(|| fail("E_RUNTIME_MISSING", "no manifest"))
    }

    /// Check 1 (and, for a settings.json, check 4): the op's target holds what the op wrote.
    fn read_back(&mut self, op: &Op) -> Result<(), Failure> {
        let fs = self.sys.fs;
        let mismatch = |what: String| {
            fail(
                "E_VERIFY_READBACK",
                format!("{what} does not hold what was written"),
            )
        };
        match op {
            Op::WriteFile { path, contents } => {
                let got = fs.read(path).map_err(|e| fail("E_VERIFY_READBACK", e))?;
                if got.as_deref() != Some(contents.as_bytes()) {
                    return Err(mismatch(path.display().to_string()));
                }
                // Check 4 for a settings.json that is written back as a plain file (a revert
                // of everything does): it is also read the way Windows Terminal reads it.
                // `contents` is what is there, as the check above showed.
                if self.is_wt_settings(path) {
                    check_parses(contents)
                        .map_err(|e| fail(e.code(), format!("{}: {e}", path.display())))?;
                }
            }
            Op::WtApply {
                settings,
                scheme,
                profile,
                set_default,
            } => {
                let bytes = fs
                    .read(settings)
                    .map_err(|e| fail("E_VERIFY_READBACK", e))?
                    .ok_or_else(|| {
                        fail(
                            "E_VERIFY_READBACK",
                            format!("{} is gone", settings.display()),
                        )
                    })?;
                let at = |e: &dyn std::fmt::Display| format!("{}: {e}", settings.display());
                let text = String::from_utf8(bytes).map_err(|e| fail("E_WT_PARSE", at(&e)))?;
                // Check 4: Windows Terminal can still read the file.
                check_parses(&text).map_err(|e| fail(e.code(), at(&e)))?;
                let read = |key: WtKey| read_value(&text, &key).map_err(|e| fail(e.code(), at(&e)));
                if read(WtKey::Scheme)?.as_ref() != Some(scheme) {
                    return Err(mismatch(format!(
                        "the CMD Studio scheme in {}",
                        settings.display()
                    )));
                }
                for (key, value) in profile {
                    let got = read(WtKey::Profile { key: key.clone() })?;
                    let ok = match (value, &got) {
                        // The user's other sub-keys, such as `font.weight`, may stay.
                        (Value::Object(want), Some(Value::Object(have))) => {
                            want.iter().all(|(k, v)| have.get(k) == Some(v))
                        }
                        _ => got.as_ref() == Some(value),
                    };
                    if !ok {
                        return Err(mismatch(format!(
                            "cmd profile key {key} in {}",
                            settings.display()
                        )));
                    }
                }
                if *set_default {
                    let ok = read(WtKey::DefaultProfile)?
                        .as_ref()
                        .and_then(Value::as_str)
                        .is_some_and(|g| g.eq_ignore_ascii_case(CMD_PROFILE_GUID));
                    if !ok {
                        return Err(mismatch(format!(
                            "defaultProfile in {}",
                            settings.display()
                        )));
                    }
                }
            }
            Op::SetAutoRun { value } => {
                let stored = self
                    .sys
                    .autorun
                    .read_user()
                    .map_err(|e| fail("E_VERIFY_READBACK", e))?;
                // A value that is there but is not text is not "absent".
                let ok = match (value, &stored) {
                    (None, None) => true,
                    (Some(want), Some(have)) => have.text().as_deref() == Some(want.as_str()),
                    _ => false,
                };
                if !ok {
                    return Err(mismatch(format!("AutoRun in {AUTORUN_KEY}")));
                }
            }
            Op::InstallFont { family } => {
                let (src, dest, value_name) = font_target(self.sys, self.resources, family)
                    .ok_or_else(|| {
                        fail("E_FONT_INSTALL", format!("no bundled file for {family}"))
                    })?;
                let name = src
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or_default()
                    .to_string();
                let pin = self
                    .manifest()?
                    .fonts
                    .files
                    .get(&name)
                    .cloned()
                    .ok_or_else(|| {
                        fail(
                            "E_RUNTIME_MISSING",
                            format!("resources/manifest.json pins no font file {name}"),
                        )
                    })?;
                let registered = self
                    .sys
                    .fonts
                    .registration(&value_name)
                    .map_err(|e| fail("E_VERIFY_READBACK", e))?;
                // Windows ignores the letter case of a path.
                let registered_here = registered
                    .as_deref()
                    .zip(dest.to_str())
                    .is_some_and(|(a, b)| a.eq_ignore_ascii_case(b));
                let file = fs.read(&dest).map_err(|e| fail("E_VERIFY_READBACK", e))?;
                // Say which part is wrong: the registration, the file, or both.
                let mut wrong = Vec::new();
                if !registered_here {
                    wrong.push(format!(
                        "its registration ({value_name}) does not name {}",
                        dest.display()
                    ));
                }
                match &file {
                    None => wrong.push(format!("{} is missing", dest.display())),
                    Some(bytes) if sha256_hex(bytes) != pin => wrong.push(format!(
                        "{} is not the pinned file (its hash differs)",
                        dest.display()
                    )),
                    Some(_) => {}
                }
                if !wrong.is_empty() {
                    return Err(fail(
                        "E_VERIFY_READBACK",
                        format!("font {family}: {}", wrong.join("; ")),
                    ));
                }
            }
            Op::InstallRuntime { component, dir } => {
                let bundle = self.manifest()?.bundle(*component).clone();
                check_bundle(fs, dir, &bundle).map_err(|e| fail("E_VERIFY_READBACK", e))?;
            }
        }
        Ok(())
    }

    /// Check 2: `oh-my-posh print primary --config <generated> --shell cmd --plain`.
    fn omp_renders(&self) -> Result<(), Failure> {
        let config = self.paths.omp_config();
        let args = [
            OsStr::new("print"),
            OsStr::new("primary"),
            OsStr::new("--config"),
            config.as_os_str(),
            OsStr::new("--shell"),
            OsStr::new("cmd"),
            OsStr::new("--plain"),
        ];
        let out = self
            .sys
            .process
            .run(&self.paths.omp_exe(), &args)
            .map_err(|e| fail("E_VERIFY_OMP", e))?;
        let config = config.display();
        if out.status != 0 {
            return Err(fail(
                "E_VERIFY_OMP",
                format!(
                    "oh-my-posh exited {} on {config}: {}",
                    out.status,
                    clip(&out.stderr)
                ),
            ));
        }
        if let Some(marker) = omp_config_error(&out.stdout) {
            return Err(fail(
                "E_VERIFY_OMP",
                format!(
                    "oh-my-posh says {marker} about {config}: {}",
                    clip(&out.stdout)
                ),
            ));
        }
        if out.stdout.trim().is_empty() {
            return Err(fail(
                "E_VERIFY_OMP",
                format!("oh-my-posh printed nothing for {config}"),
            ));
        }
        Ok(())
    }

    /// Check 3: `<clink-launch> --profile <p> set clink.logo`. It always exits 0 (G5), so a Lua
    /// error shows only in its output, and any other exit status is a failure.
    fn clink_loads(&self) -> Result<(), Failure> {
        let profile = self.paths.clink_profile();
        let args = [
            OsStr::new("--profile"),
            profile.as_os_str(),
            OsStr::new("set"),
            OsStr::new("clink.logo"),
        ];
        // A check that cannot run is a failure, never a pass.
        let launch = self
            .launch
            .as_ref()
            .ok_or_else(|| fail("E_VERIFY_CLINK", "no <clink-launch> was given"))?;
        let out = self
            .sys
            .process
            .run(&launch.exe(), &args)
            .map_err(|e| fail("E_VERIFY_CLINK", e))?;
        let all = format!("{}\n{}", out.stdout, out.stderr);
        let lua = lua_error_line(&all);
        // `set` always exits 0 (G5): any other status means Clink did not run as it should,
        // whatever it printed.
        if out.status != 0 {
            let said = lua.map(clip).unwrap_or_else(|| {
                clip(if out.stderr.trim().is_empty() {
                    &out.stdout
                } else {
                    &out.stderr
                })
            });
            let message = if said.is_empty() {
                format!("clink exited {}", out.status)
            } else {
                format!("clink exited {}: {said}", out.status)
            };
            return Err(fail("E_VERIFY_CLINK", message));
        }
        match lua {
            Some(line) => Err(fail("E_VERIFY_CLINK", clip(line))),
            None => Ok(()),
        }
    }

    /// Whether `path` is one of Windows Terminal's settings files on this machine. Windows
    /// ignores the letter case of a path.
    fn is_wt_settings(&self, path: &Path) -> bool {
        self.sys.wt.locations().iter().any(|l| {
            l.settings == path
                || l.settings
                    .to_str()
                    .zip(path.to_str())
                    .is_some_and(|(a, b)| a.eq_ignore_ascii_case(b))
        })
    }

    /// Whether Clink loads the file when it starts: its settings, or a Lua script.
    fn loaded_by_clink(&self, path: &Path) -> bool {
        let lua = path
            .extension()
            .and_then(OsStr::to_str)
            .is_some_and(|e| e.eq_ignore_ascii_case("lua"));
        path.starts_with(self.paths.clink_profile())
            || (lua && path.starts_with(self.paths.generated()))
    }
}

impl Verifier for SystemVerifier<'_> {
    fn verify(&mut self, ops: &[Op]) -> Result<(), Failure> {
        for op in ops {
            self.read_back(op)?;
        }
        let written = |keep: &dyn Fn(&Path) -> bool| {
            ops.iter()
                .any(|o| matches!(o, Op::WriteFile { path, .. } if keep(path)))
        };
        if written(&|p| p == self.paths.omp_config()) {
            self.omp_renders()?;
        }
        if written(&|p| self.loaded_by_clink(p)) {
            self.clink_loads()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::fake::FakeMachine;
    use crate::adapters::{AutoRun, Fonts, Fs, RegValue, REG_EXPAND_SZ, REG_SZ};
    use crate::exec::SystemExecutor;
    use crate::model::Target;
    use crate::plan::{plan, Component, Machine, Request};
    use crate::presets::terracotta;
    use crate::resources::{fake_resources, NERD_FONT_FILE};
    use crate::txn::{run, Executor, Journal, JournalStore, Outcome};
    use std::path::{Path, PathBuf};

    #[test]
    fn lua_errors_are_found_by_their_shape() {
        assert!(lua_error_line(r"C:\x\bad.lua:1: deliberate error").is_some());
        assert!(
            lua_error_line("ok\nC:\\ผู้ใช้\\generated\\banner.lua:12: attempt to call nil\n")
                .is_some()
        );
        assert!(lua_error_line("Setting 'clink.logo' is 'none'.").is_none());
        assert!(lua_error_line("C:\\x\\a.lua: no line number").is_none());
        assert!(lua_error_line("C:\\x\\a.lua:12 no colon").is_none());
    }

    #[test]
    fn omp_config_errors_are_recognised() {
        assert_eq!(
            omp_config_error(" CONFIG PARSE ERROR  ~ "),
            Some("CONFIG PARSE ERROR")
        );
        assert_eq!(omp_config_error("╭─ ~ ─╮"), None);
    }

    #[derive(Default)]
    struct Mem(Vec<Journal>);
    impl JournalStore for Mem {
        fn save(&mut self, j: &Journal) -> Result<(), Failure> {
            self.0.push(j.clone());
            Ok(())
        }
        fn load_unfinished(&mut self) -> Result<Option<Journal>, Failure> {
            Ok(None)
        }
    }

    fn applied(m: &FakeMachine) -> (Resources, Paths, Vec<Op>, Option<ClinkLaunch>) {
        let res = fake_resources(&m.fs, Path::new(r"C:\app\resources"));
        let paths = Paths {
            data: PathBuf::from(r"C:\Users\tester\AppData\Local\ai.resincore.cmdstudio"),
        };
        let machine = Machine {
            data_short: Some(paths.data.clone()),
            ..Machine::default()
        };
        let req = Request {
            target: Target::All,
            set_wt_default_profile: false,
            lang: crate::generate::lua::Lang::En,
            replace_foreign_clink: false,
        };
        let p = plan(&terracotta(), &req, &machine, &paths).unwrap();
        (res, paths, p.ops, Some(p.launch))
    }

    #[test]
    fn a_good_apply_verifies_and_commits() {
        let m = FakeMachine::new();
        m.process
            .answer("oh-my-posh.exe", "print", 0, "╭─ ~ ─╮\n│ >");
        let (res, paths, ops, launch) = applied(&m);
        let out = run(
            "t",
            ops,
            &mut SystemExecutor::new(m.system(), &res),
            &mut SystemVerifier::new(m.system(), &paths, &res, launch),
            &mut Mem::default(),
        );
        assert_eq!(out, Outcome::Committed);
        let calls = m.process.calls();
        assert_eq!(calls.len(), 2);
        assert_eq!(
            calls[0].args,
            vec![
                "print",
                "primary",
                "--config",
                &paths.omp_config().display().to_string(),
                "--shell",
                "cmd",
                "--plain"
            ]
        );
        assert_eq!(calls[1].exe, paths.clink_exe());
    }

    #[test]
    fn omp_config_error_rolls_back_with_e_verify_omp() {
        let m = FakeMachine::new();
        m.process
            .answer("oh-my-posh.exe", "print", 0, " CONFIG PARSE ERROR ");
        let (res, paths, ops, launch) = applied(&m);
        let files_before = m.fs.snapshot().0;
        let out = run(
            "t",
            ops,
            &mut SystemExecutor::new(m.system(), &res),
            &mut SystemVerifier::new(m.system(), &paths, &res, launch),
            &mut Mem::default(),
        );
        assert!(
            matches!(
                out,
                Outcome::RolledBack {
                    code: "E_VERIFY_OMP",
                    ..
                }
            ),
            "{out:?}"
        );
        assert_eq!(m.fs.snapshot().0, files_before);
        assert_eq!(m.autorun.read_user().unwrap(), None);
    }

    #[test]
    fn a_lua_error_rolls_back_with_e_verify_clink() {
        let m = FakeMachine::new();
        m.process.answer("oh-my-posh.exe", "print", 0, "> ");
        m.process.answer(
            "clink_x64.exe",
            "--profile",
            0,
            "C:\\d\\generated\\banner.lua:3: boom\n",
        );
        let (res, paths, ops, launch) = applied(&m);
        let out = run(
            "t",
            ops,
            &mut SystemExecutor::new(m.system(), &res),
            &mut SystemVerifier::new(m.system(), &paths, &res, launch),
            &mut Mem::default(),
        );
        assert!(
            matches!(
                out,
                Outcome::RolledBack {
                    code: "E_VERIFY_CLINK",
                    ..
                }
            ),
            "{out:?}"
        );
    }

    #[test]
    fn readback_catches_a_value_changed_behind_our_back() {
        let m = FakeMachine::new();
        let (res, paths, _, launch) = applied(&m);
        let op = Op::SetAutoRun {
            value: Some("ours".into()),
        };
        SystemExecutor::new(m.system(), &res).apply(&op).unwrap();
        m.autorun
            .write_user(&RegValue::from_text(REG_SZ, "someone else"))
            .unwrap();
        let err = SystemVerifier::new(m.system(), &paths, &res, launch.clone())
            .verify(&[op])
            .unwrap_err();
        assert_eq!(err.code, "E_VERIFY_READBACK");
        let rt = Op::InstallRuntime {
            component: Component::Omp,
            dir: paths.omp_dir(),
        };
        m.fs.put(paths.omp_exe(), b"not the pinned file");
        assert_eq!(
            SystemVerifier::new(m.system(), &paths, &res, launch)
                .verify(&[rt])
                .unwrap_err()
                .code,
            "E_VERIFY_READBACK"
        );
    }

    // ---- Focused extra tests (Q20) ----

    const WT_SETTINGS: &str = r"C:\Users\tester\AppData\Local\Packages\Microsoft.WindowsTerminal_8wekyb3d8bbwe\LocalState\settings.json";
    const OTHER_PROFILE: &str = "{61c54bbd-c2c6-5271-96e7-009a87ff44bf}";

    /// A settings.json with a comment, a default profile of the user's own and a cmd profile
    /// whose font has a weight of the user's own.
    fn user_settings() -> String {
        format!(
            "{{\n    // my settings\n    \"defaultProfile\": \"{OTHER_PROFILE}\",\n    \"profiles\": {{\n        \"list\": [\n            {{ \"guid\": \"{}\", \"name\": \"Command Prompt\", \"font\": {{ \"weight\": \"bold\" }} }}\n        ]\n    }}\n}}\n",
            crate::generate::wt::CMD_PROFILE_GUID
        )
    }

    /// Like `applied`, with the settings.json above, on a machine that lacks the Nerd Font, so
    /// the plan has a font op and a Windows Terminal op.
    fn applied_wt(
        m: &FakeMachine,
        set_default: bool,
    ) -> (Resources, Paths, Vec<Op>, Option<ClinkLaunch>) {
        let res = fake_resources(&m.fs, Path::new(r"C:\app\resources"));
        let paths = Paths {
            data: PathBuf::from(r"C:\Users\tester\AppData\Local\ai.resincore.cmdstudio"),
        };
        m.fs.put(WT_SETTINGS, user_settings());
        // A prompt, so the Oh My Posh check passes when the test is about something else.
        m.process.answer("oh-my-posh.exe", "print", 0, "> ");
        let machine = Machine {
            data_short: Some(paths.data.clone()),
            wt_settings: vec![PathBuf::from(WT_SETTINGS)],
            ..Machine::default()
        };
        let req = Request {
            target: Target::All,
            set_wt_default_profile: set_default,
            lang: crate::generate::lua::Lang::En,
            replace_foreign_clink: false,
        };
        let p = plan(&terracotta(), &req, &machine, &paths).unwrap();
        (res, paths, p.ops, Some(p.launch))
    }

    fn apply_all(m: &FakeMachine, res: &Resources, ops: &[Op]) {
        let mut exec = SystemExecutor::new(m.system(), res);
        for op in ops {
            exec.apply(op).unwrap();
        }
    }

    fn verify_ops(
        m: &FakeMachine,
        paths: &Paths,
        res: &Resources,
        launch: &Option<ClinkLaunch>,
        ops: &[Op],
    ) -> Result<(), Failure> {
        SystemVerifier::new(m.system(), paths, res, launch.clone()).verify(ops)
    }

    fn settings_text(m: &FakeMachine) -> String {
        String::from_utf8(m.fs.file(WT_SETTINGS).unwrap()).unwrap()
    }

    #[test]
    fn lua_error_shapes_that_must_and_must_not_count() {
        // Windows ignores the case of a file name.
        assert!(lua_error_line(r"C:\x\Banner.LUA:3: boom").is_some());
        // Lua cuts a long script name at its front.
        assert!(lua_error_line(r"...ink\generated\banner.lua:12: boom").is_some());
        // The error need not start the line.
        assert!(lua_error_line(r"Clink: failed C:\d\init.lua:1: x").is_some());
        // Only the line with the error is returned.
        assert_eq!(
            lua_error_line("fine\nC:\\d\\init.lua:7: bad\nalso fine"),
            Some("C:\\d\\init.lua:7: bad")
        );
        assert!(lua_error_line(r"C:\x\a.lua:x: letters").is_none());
        assert!(lua_error_line("lua:12: no dot").is_none());
        assert!(lua_error_line(r"C:\x\a.lua:: no number").is_none());
        assert!(lua_error_line(r"C:\x\a.luax:12: other name").is_none());
        assert!(lua_error_line("").is_none());
    }

    #[test]
    fn every_omp_config_error_is_recognised_inside_a_prompt() {
        for marker in OMP_CONFIG_ERRORS {
            assert_eq!(omp_config_error(&format!("> {marker} ~")), Some(marker));
        }
        assert_eq!(omp_config_error("no config here, only lower case"), None);
    }

    #[test]
    fn clip_keeps_a_message_short() {
        assert_eq!(clip("  one line  "), "one line");
        let long = "x".repeat(1000);
        let clipped = clip(&long);
        assert_eq!(clipped.chars().count(), 303);
        assert!(clipped.ends_with("..."));
    }

    #[test]
    fn a_windows_terminal_apply_verifies_and_the_users_font_weight_stays() {
        let m = FakeMachine::new();
        m.process.answer("oh-my-posh.exe", "print", 0, "> ");
        let (res, paths, ops, launch) = applied_wt(&m, true);
        assert!(ops.iter().any(|o| matches!(o, Op::WtApply { .. })));
        assert!(ops.iter().any(|o| matches!(o, Op::InstallFont { .. })));
        let out = run(
            "t",
            ops,
            &mut SystemExecutor::new(m.system(), &res),
            &mut SystemVerifier::new(m.system(), &paths, &res, launch),
            &mut Mem::default(),
        );
        assert_eq!(out, Outcome::Committed);
        assert!(settings_text(&m).contains("\"weight\": \"bold\""));
        assert_eq!(m.process.calls().len(), 2);
    }

    #[test]
    fn windows_terminal_readback_catches_each_changed_value() {
        let guid = crate::generate::wt::CMD_PROFILE_GUID;
        let cases = [
            // The cmd profile's scheme key.
            (
                "\"colorScheme\": \"CMD Studio\"".to_string(),
                "\"colorScheme\": \"Campbell\"".to_string(),
                "colorScheme",
            ),
            // One sub-key of the font object that was written.
            (
                "\"face\": \"CaskaydiaCove NF\"".to_string(),
                "\"face\": \"Arial\"".to_string(),
                "cmd profile key font",
            ),
            // The scheme itself.
            (
                "\"name\": \"CMD Studio\"".to_string(),
                "\"name\": \"Somebody else\"".to_string(),
                "the CMD Studio scheme",
            ),
            // The default profile was asked for.
            (
                format!("\"defaultProfile\": \"{guid}\""),
                format!("\"defaultProfile\": \"{OTHER_PROFILE}\""),
                "defaultProfile",
            ),
        ];
        for (from, to, wanted) in cases {
            let m = FakeMachine::new();
            let (res, paths, ops, launch) = applied_wt(&m, true);
            apply_all(&m, &res, &ops);
            verify_ops(&m, &paths, &res, &launch, &ops).unwrap();
            let text = settings_text(&m);
            assert!(text.contains(&from), "{from} is not in\n{text}");
            m.fs.put(WT_SETTINGS, text.replace(&from, &to));
            let err = verify_ops(&m, &paths, &res, &launch, &ops).unwrap_err();
            assert_eq!(err.code, "E_VERIFY_READBACK", "{from}: {err:?}");
            assert!(err.message.contains(wanted), "{err:?}");
            // Which file it was.
            assert!(err.message.contains("settings.json"), "{err:?}");
        }
    }

    #[test]
    fn the_default_profile_guid_matches_in_any_letter_case() {
        let m = FakeMachine::new();
        let (res, paths, ops, launch) = applied_wt(&m, true);
        apply_all(&m, &res, &ops);
        let upper = settings_text(&m).replace(
            crate::generate::wt::CMD_PROFILE_GUID,
            &crate::generate::wt::CMD_PROFILE_GUID.to_ascii_uppercase(),
        );
        m.fs.put(WT_SETTINGS, upper);
        verify_ops(&m, &paths, &res, &launch, &ops).unwrap();
    }

    #[test]
    fn a_default_profile_nobody_asked_for_is_not_checked() {
        let m = FakeMachine::new();
        let (res, paths, ops, launch) = applied_wt(&m, false);
        apply_all(&m, &res, &ops);
        // The user's own default stays, and that is fine.
        assert!(settings_text(&m).contains(OTHER_PROFILE));
        verify_ops(&m, &paths, &res, &launch, &ops).unwrap();
    }

    #[test]
    fn a_settings_file_that_stopped_parsing_is_e_wt_parse_and_says_which() {
        let deep = format!("{}{}", "[".repeat(80), "]".repeat(80));
        let cases = [
            ("not json at all {", "not valid JSON"),
            // A key CMD Studio owns, written twice: Windows Terminal would use one of them.
            (
                "{ \"defaultProfile\": \"a\", \"defaultProfile\": \"b\" }",
                "\"defaultProfile\" twice",
            ),
            // Nested deeper than the editor reads.
            (deep.as_str(), "deeper than 64 levels"),
        ];
        for (text, reason) in cases {
            let m = FakeMachine::new();
            let (res, paths, ops, launch) = applied_wt(&m, false);
            apply_all(&m, &res, &ops);
            m.fs.put(WT_SETTINGS, text);
            let err = verify_ops(&m, &paths, &res, &launch, &ops).unwrap_err();
            assert_eq!(err.code, "E_WT_PARSE", "{text}: {err:?}");
            // Which file, and the editor's own reason, not swallowed.
            assert!(err.message.starts_with(WT_SETTINGS), "{err:?}");
            assert!(err.message.contains(reason), "{err:?}");
        }
        // Not UTF-8: Windows Terminal cannot read it either.
        let m = FakeMachine::new();
        let (res, paths, ops, launch) = applied_wt(&m, false);
        apply_all(&m, &res, &ops);
        m.fs.put(WT_SETTINGS, [0xff, 0xfe, b'{']);
        let err = verify_ops(&m, &paths, &res, &launch, &ops).unwrap_err();
        assert_eq!(err.code, "E_WT_PARSE", "{err:?}");
        assert!(err.message.starts_with(WT_SETTINGS), "{err:?}");
    }

    #[test]
    fn a_settings_file_that_is_gone_is_a_readback_failure() {
        let m = FakeMachine::new();
        let (res, paths, ops, launch) = applied_wt(&m, false);
        apply_all(&m, &res, &ops);
        m.fs.remove_file(Path::new(WT_SETTINGS)).unwrap();
        let err = verify_ops(&m, &paths, &res, &launch, &ops).unwrap_err();
        assert_eq!(err.code, "E_VERIFY_READBACK");
        assert!(err.message.contains("is gone"), "{err:?}");
    }

    #[test]
    fn autorun_readback_keeps_absent_and_not_text_apart() {
        let m = FakeMachine::new();
        let (res, paths, _, launch) = applied_wt(&m, false);
        let delete = [Op::SetAutoRun { value: None }];
        // Absent: the deletion held.
        verify_ops(&m, &paths, &res, &launch, &delete).unwrap();
        // A value of another type is there: it is not "absent", although it has no text.
        m.autorun
            .write_user(&RegValue {
                kind: 4,
                bytes: vec![1, 0, 0, 0],
            })
            .unwrap();
        let err = verify_ops(&m, &paths, &res, &launch, &delete).unwrap_err();
        assert_eq!(err.code, "E_VERIFY_READBACK");
        // The message says whose value it is, so the user can look at it.
        let key = r"HKEY_CURRENT_USER\Software\Microsoft\Command Processor";
        assert!(err.message.contains(key), "{err:?}");
        assert!(err.message.contains("AutoRun"), "{err:?}");
        // A value is expected and none is there.
        m.autorun.delete_user().unwrap();
        let set = [Op::SetAutoRun {
            value: Some("ours".into()),
        }];
        let err = verify_ops(&m, &paths, &res, &launch, &set).unwrap_err();
        assert_eq!(err.code, "E_VERIFY_READBACK");
        assert!(err.message.contains(key), "{err:?}");
        // Another text is there.
        m.autorun
            .write_user(&RegValue::from_text(REG_SZ, "theirs"))
            .unwrap();
        let err = verify_ops(&m, &paths, &res, &launch, &set).unwrap_err();
        assert!(err.message.contains(key), "{err:?}");
        // REG_EXPAND_SZ keeps its text, unexpanded.
        m.autorun
            .write_user(&RegValue::from_text(REG_EXPAND_SZ, "ours"))
            .unwrap();
        verify_ops(&m, &paths, &res, &launch, &set).unwrap();
    }

    #[test]
    fn font_readback_checks_the_registration_and_the_pinned_file() {
        let m = FakeMachine::new();
        let (res, paths, ops, launch) = applied_wt(&m, false);
        apply_all(&m, &res, &ops);
        let font = [Op::InstallFont {
            family: crate::plan::NERD_FONT_FAMILY.to_string(),
        }];
        verify_ops(&m, &paths, &res, &launch, &font).unwrap();
        let value_name = format!("{} (TrueType)", crate::plan::NERD_FONT_FAMILY);
        let dest = m.fonts.user_fonts_dir().join(NERD_FONT_FILE);
        let registered = m.fonts.registration(&value_name).unwrap().unwrap();
        let failure = || verify_ops(&m, &paths, &res, &launch, &font).unwrap_err();

        // The file is another file than the pinned one: the message says it is the file (its
        // hash), not the registration.
        m.fs.put(&dest, b"another font");
        let err = failure();
        assert_eq!(err.code, "E_VERIFY_READBACK");
        assert!(err.message.contains("hash differs"), "{err:?}");
        assert!(err.message.contains(&dest.display().to_string()), "{err:?}");
        assert!(!err.message.contains("registration"), "{err:?}");
        // The file is gone.
        m.fs.remove_file(&dest).unwrap();
        let err = failure();
        assert!(err.message.contains("is missing"), "{err:?}");
        assert!(!err.message.contains("registration"), "{err:?}");
        m.fs.put(&dest, b"font-bytes");
        verify_ops(&m, &paths, &res, &launch, &font).unwrap();

        // The registration names another file, or none; letter case does not matter. The
        // message says it is the registration, not the file.
        m.fonts
            .register(&value_name, &registered.to_ascii_uppercase())
            .unwrap();
        verify_ops(&m, &paths, &res, &launch, &font).unwrap();
        m.fonts.register(&value_name, r"C:\other.ttf").unwrap();
        let err = failure();
        assert_eq!(err.code, "E_VERIFY_READBACK");
        assert!(err.message.contains("registration"), "{err:?}");
        assert!(err.message.contains(&value_name), "{err:?}");
        assert!(!err.message.contains("hash"), "{err:?}");
        m.fonts.unregister(&value_name).unwrap();
        let err = failure();
        assert_eq!(err.code, "E_VERIFY_READBACK");
        assert!(err.message.contains("registration"), "{err:?}");

        // Both wrong: both are named.
        m.fs.put(&dest, b"another font");
        let err = failure();
        assert!(err.message.contains("registration"), "{err:?}");
        assert!(err.message.contains("hash differs"), "{err:?}");

        // A family nothing is bundled for cannot be checked, so it fails.
        let unknown = [Op::InstallFont {
            family: "Comic Sans".into(),
        }];
        assert_eq!(
            verify_ops(&m, &paths, &res, &launch, &unknown)
                .unwrap_err()
                .code,
            "E_FONT_INSTALL"
        );
    }

    #[test]
    fn a_runtime_that_cannot_be_checked_fails_and_never_passes() {
        let m = FakeMachine::new();
        let (_, paths, _, launch) = applied(&m);
        // No manifest at all: the pinned hashes cannot be read.
        let nowhere = Resources {
            root: PathBuf::from(r"C:\nowhere"),
        };
        let rt = [Op::InstallRuntime {
            component: Component::Clink,
            dir: paths.clink_dir(),
        }];
        let err = verify_ops(&m, &paths, &nowhere, &launch, &rt).unwrap_err();
        assert_eq!(err.code, "E_RUNTIME_MISSING");
        // A pinned file that is not in the folder.
        let res = fake_resources(&m.fs, Path::new(r"C:\app\resources"));
        let err = verify_ops(&m, &paths, &res, &launch, &rt).unwrap_err();
        assert_eq!(err.code, "E_VERIFY_READBACK");
    }

    #[test]
    fn oh_my_posh_that_fails_or_prints_nothing_fails_the_check() {
        for (status, stdout, wanted) in [
            (1, "", "exited 1"),
            (0, "", "printed nothing"),
            (0, "  \r\n ", "printed nothing"),
            (0, " NO CONFIG ", "NO CONFIG"),
            (0, " CONFIG NOT FOUND ", "CONFIG NOT FOUND"),
        ] {
            let m = FakeMachine::new();
            m.process.answer("oh-my-posh.exe", "print", status, stdout);
            let (res, paths, ops, launch) = applied(&m);
            apply_all(&m, &res, &ops);
            let err = verify_ops(&m, &paths, &res, &launch, &ops).unwrap_err();
            assert_eq!(err.code, "E_VERIFY_OMP", "{stdout:?}");
            assert!(err.message.contains(wanted), "{err:?}");
            // Which config it was, and (for a marker) what Oh My Posh printed.
            let config = paths.omp_config().display().to_string();
            assert!(err.message.contains(&config), "{err:?}");
            if wanted != "exited 1" && wanted != "printed nothing" {
                assert!(err.message.contains(stdout.trim()), "{err:?}");
            }
            // The Clink check does not run after a failed Oh My Posh check.
            assert_eq!(m.process.calls().len(), 1);
        }
    }

    #[test]
    fn the_lua_error_line_is_the_message_and_clink_ran_from_its_launch_path() {
        let m = FakeMachine::new();
        m.process.answer("oh-my-posh.exe", "print", 0, "> ");
        m.process.answer(
            "clink_x64.exe",
            "--profile",
            0,
            "Setting 'clink.logo'\nC:\\d\\generated\\init.lua:9: nope\n",
        );
        let (res, paths, ops, _) = applied(&m);
        apply_all(&m, &res, &ops);
        let short = PathBuf::from(
            r"C:\Users\TESTER~1\AppData\Local\AIRESI~1.CMD\runtime\clink-1.9.34\clink_x64.exe",
        );
        let launch = Some(ClinkLaunch::ShortPath(short.clone()));
        let err = verify_ops(&m, &paths, &res, &launch, &ops).unwrap_err();
        assert_eq!(err.code, "E_VERIFY_CLINK");
        assert_eq!(err.message, "C:\\d\\generated\\init.lua:9: nope");
        let calls = m.process.calls();
        assert_eq!(calls[1].exe, short);
        assert_eq!(
            calls[1].args,
            vec![
                "--profile".to_string(),
                paths.clink_profile().display().to_string(),
                "set".into(),
                "clink.logo".into()
            ]
        );
    }

    type Answer = Result<crate::adapters::Output, crate::adapters::AdapterError>;

    fn printed(status: i32, stdout: &str, stderr: &str) -> Answer {
        Ok(crate::adapters::Output {
            status,
            stdout: stdout.into(),
            stderr: stderr.into(),
        })
    }

    /// A process adapter with one answer per program, an error or output on stderr included.
    struct Programs {
        omp: Answer,
        clink: Answer,
    }

    impl crate::adapters::Process for Programs {
        fn run(&self, exe: &Path, _: &[&std::ffi::OsStr]) -> Answer {
            if exe.file_name() == Some(std::ffi::OsStr::new("oh-my-posh.exe")) {
                self.omp.clone()
            } else {
                self.clink.clone()
            }
        }
    }

    /// A whole apply, verified while Oh My Posh and Clink answer as given.
    fn verify_programs(omp: Answer, clink: Answer) -> Result<(), Failure> {
        let m = FakeMachine::new();
        let (res, paths, ops, launch) = applied(&m);
        apply_all(&m, &res, &ops);
        let process = Programs { omp, clink };
        let mut sys = m.system();
        sys.process = &process;
        SystemVerifier::new(sys, &paths, &res, launch).verify(&ops)
    }

    #[test]
    fn a_program_that_cannot_run_or_prints_errors_elsewhere_fails_its_check() {
        use crate::adapters::AdapterError;
        let prompt = || printed(0, "> ", "");
        let clean = || printed(0, "Setting 'clink.logo' is 'none'.", "");
        verify_programs(prompt(), clean()).unwrap();

        // Oh My Posh cannot run: the check fails, with the adapter's own words.
        let omp = Err(AdapterError::new("run omp", "no answer after 20s"));
        let err = verify_programs(omp, clean()).unwrap_err();
        assert_eq!(err.code, "E_VERIFY_OMP");
        assert_eq!(err.message, "run omp: no answer after 20s");
        // When it fails, its error output is the message.
        let err = verify_programs(printed(2, "", "  bad flag \n"), clean()).unwrap_err();
        assert_eq!(err.code, "E_VERIFY_OMP");
        assert!(err.message.contains("exited 2 on "), "{err:?}");
        assert!(err.message.ends_with(": bad flag"), "{err:?}");

        // Clink cannot run.
        let clink = Err(AdapterError::new("run clink", "not allowed"));
        let err = verify_programs(prompt(), clink).unwrap_err();
        assert_eq!(err.code, "E_VERIFY_CLINK");
        assert_eq!(err.message, "run clink: not allowed");
        // Its Lua error is on standard error.
        let lua = printed(0, "", "C:\\d\\generated\\banner.lua:4: boom\n");
        let err = verify_programs(prompt(), lua).unwrap_err();
        assert_eq!(err.code, "E_VERIFY_CLINK");
        assert_eq!(err.message, r"C:\d\generated\banner.lua:4: boom");
        // A long error line is clipped.
        let long = printed(0, &format!(r"C:\d\init.lua:1: {}", "x".repeat(2000)), "");
        let err = verify_programs(prompt(), long).unwrap_err();
        assert!(err.message.chars().count() < 400, "{}", err.message.len());
    }

    #[test]
    fn a_clink_check_that_cannot_run_is_a_failure() {
        let m = FakeMachine::new();
        m.process.answer("oh-my-posh.exe", "print", 0, "> ");
        let (res, paths, ops, _) = applied(&m);
        apply_all(&m, &res, &ops);
        // Clink's scripts were written, and no <clink-launch> was given.
        let err = verify_ops(&m, &paths, &res, &None, &ops).unwrap_err();
        assert_eq!(err.code, "E_VERIFY_CLINK");
        assert!(err.message.contains("<clink-launch>"), "{err:?}");
    }

    #[test]
    fn a_long_oh_my_posh_message_is_clipped() {
        let m = FakeMachine::new();
        let long = format!(" CONFIG PARSE ERROR {}", "x".repeat(2000));
        m.process.answer("oh-my-posh.exe", "print", 0, &long);
        let (res, paths, ops, launch) = applied(&m);
        apply_all(&m, &res, &ops);
        let err = verify_ops(&m, &paths, &res, &launch, &ops).unwrap_err();
        assert_eq!(err.code, "E_VERIFY_OMP");
        assert!(err.message.contains("CONFIG PARSE ERROR"), "{err:?}");
        assert!(err.message.chars().count() < 600, "{}", err.message.len());
    }

    #[test]
    fn clink_that_exits_non_zero_fails_even_with_nothing_printed() {
        // Clink's `set` always exits 0 (G5), so any other status means it did not run as it
        // should, whatever it printed.
        let prompt = || printed(0, "> ", "");
        let err = verify_programs(prompt(), printed(3, "", "")).unwrap_err();
        assert_eq!(err.code, "E_VERIFY_CLINK");
        assert!(err.message.contains("exited 3"), "{err:?}");
        let err =
            verify_programs(prompt(), printed(-1, "Setting 'clink.logo'", " why \n")).unwrap_err();
        assert_eq!(err.code, "E_VERIFY_CLINK");
        assert!(err.message.contains("exited -1"), "{err:?}");
        assert!(err.message.ends_with("why"), "{err:?}");
        // A Lua error is still the message when Clink also exited non-zero.
        let lua = printed(1, r"C:\d\init.lua:2: boom", "");
        let err = verify_programs(prompt(), lua).unwrap_err();
        assert!(err.message.contains("exited 1"), "{err:?}");
        assert!(err.message.contains("init.lua:2: boom"), "{err:?}");
        // Exit 0 and nothing wrong passes.
        verify_programs(prompt(), printed(0, "", "")).unwrap();
    }

    #[test]
    fn a_written_file_that_changed_or_vanished_fails_the_readback() {
        let m = FakeMachine::new();
        let (res, paths, _, launch) = applied(&m);
        let path = paths.generated().join("notes.txt");
        let op = Op::WriteFile {
            path: path.clone(),
            contents: "one".into(),
        };
        apply_all(&m, &res, std::slice::from_ref(&op));
        verify_ops(&m, &paths, &res, &launch, std::slice::from_ref(&op)).unwrap();

        m.fs.put(&path, "two");
        let err = verify_ops(&m, &paths, &res, &launch, std::slice::from_ref(&op)).unwrap_err();
        assert_eq!(err.code, "E_VERIFY_READBACK");
        assert!(err.message.contains(&path.display().to_string()), "{err:?}");

        // One byte more, and an empty file, are not the written contents either.
        for other in ["one ", ""] {
            m.fs.put(&path, other);
            let err = verify_ops(&m, &paths, &res, &launch, std::slice::from_ref(&op)).unwrap_err();
            assert_eq!(err.code, "E_VERIFY_READBACK", "{other:?}");
        }

        m.fs.remove_file(&path).unwrap();
        let err = verify_ops(&m, &paths, &res, &launch, std::slice::from_ref(&op)).unwrap_err();
        assert_eq!(err.code, "E_VERIFY_READBACK");
        assert!(err.message.contains(&path.display().to_string()), "{err:?}");
    }

    fn wt_location(settings: &str) -> crate::adapters::WtLocation {
        crate::adapters::WtLocation {
            variant: crate::adapters::WtVariant::Store,
            installed: true,
            settings: PathBuf::from(settings),
        }
    }

    #[test]
    fn a_settings_json_written_as_a_plain_file_must_still_parse() {
        // A revert of everything puts a settings.json back with `WriteFile`; the file is also
        // checked as Windows Terminal reads it.
        let mut m = FakeMachine::new();
        m.wt.locations = vec![wt_location(WT_SETTINGS)];
        let (res, paths, _, launch) = applied(&m);
        let write = |path: &str, contents: &str| {
            let op = Op::WriteFile {
                path: PathBuf::from(path),
                contents: contents.into(),
            };
            apply_all(&m, &res, std::slice::from_ref(&op));
            op
        };
        let broken = "{ \"profiles\": [ ";

        // At a Windows Terminal location: the editor's reason, and which file.
        let op = write(WT_SETTINGS, broken);
        let err = verify_ops(&m, &paths, &res, &launch, &[op]).unwrap_err();
        assert_eq!(err.code, "E_WT_PARSE");
        assert!(err.message.starts_with(WT_SETTINGS), "{err:?}");
        assert!(err.message.contains("not valid JSON"), "{err:?}");
        // Windows ignores letter case in a path.
        let op = write(&WT_SETTINGS.to_ascii_uppercase(), broken);
        let err = verify_ops(&m, &paths, &res, &launch, &[op]).unwrap_err();
        assert_eq!(err.code, "E_WT_PARSE");
        // Nested deeper than the editor reads: refused with its reason, not passed on.
        let deep = format!("{}{}", "[".repeat(80), "]".repeat(80));
        let op = write(WT_SETTINGS, &deep);
        let err = verify_ops(&m, &paths, &res, &launch, &[op]).unwrap_err();
        assert_eq!(err.code, "E_WT_PARSE");
        assert!(err.message.contains("deeper than 64 levels"), "{err:?}");
        // Bytes that are not what was written are a readback failure first.
        let op = write(WT_SETTINGS, fine_json());
        m.fs.put(WT_SETTINGS, broken);
        let err = verify_ops(&m, &paths, &res, &launch, &[op]).unwrap_err();
        assert_eq!(err.code, "E_VERIFY_READBACK");

        // A file that parses passes: comments, a trailing comma and a BOM are fine.
        let op = write(
            WT_SETTINGS,
            "\u{feff}{\n  // mine\n  \"profiles\": { \"list\": [], },\n}\n",
        );
        verify_ops(&m, &paths, &res, &launch, &[op]).unwrap();

        // The same unparseable bytes anywhere else only get the byte-for-byte check.
        let op = write(r"C:\Users\tester\Documents\settings.json", broken);
        verify_ops(&m, &paths, &res, &launch, &[op]).unwrap();
        let op = write(
            r"C:\Users\tester\AppData\Local\Packages\Other\LocalState\settings.json",
            broken,
        );
        verify_ops(&m, &paths, &res, &launch, &[op]).unwrap();
    }

    fn fine_json() -> &'static str {
        "{ \"profiles\": { \"list\": [] } }"
    }

    #[test]
    fn each_check_runs_only_when_what_it_checks_was_written() {
        let m = FakeMachine::new();
        m.process.answer("oh-my-posh.exe", "print", 0, "> ");
        let (res, paths, _, launch) = applied(&m);
        let write = |path: PathBuf| {
            let op = Op::WriteFile {
                path,
                contents: "x".to_string(),
            };
            apply_all(&m, &res, std::slice::from_ref(&op));
            op
        };
        let names = |m: &FakeMachine| -> Vec<String> {
            m.process
                .calls()
                .iter()
                .map(|c| c.exe.file_name().unwrap().to_string_lossy().into_owned())
                .collect()
        };

        // Nothing Clink loads, no Oh My Posh config: nothing runs, even without a launch path
        // (a revert of everything).
        let autorun = Op::SetAutoRun { value: None };
        verify_ops(&m, &paths, &res, &None, &[autorun]).unwrap();
        let other = write(paths.generated().join("notes.txt"));
        verify_ops(&m, &paths, &res, &None, &[other]).unwrap();
        assert!(names(&m).is_empty());

        // The Oh My Posh config alone.
        let omp = write(paths.omp_config());
        verify_ops(&m, &paths, &res, &launch, &[omp]).unwrap();
        assert_eq!(names(&m), ["oh-my-posh.exe"]);

        // Clink's settings alone, then each script alone (any `.lua` in `generated`).
        for path in [
            paths.clink_profile().join("clink_settings"),
            paths.generated().join("init.lua"),
            paths.generated().join("banner.lua"),
            paths.generated().join("later.LUA"),
        ] {
            let before = names(&m).len();
            let op = write(path.clone());
            verify_ops(&m, &paths, &res, &launch, &[op]).unwrap();
            assert_eq!(
                &names(&m)[before..],
                ["clink_x64.exe"],
                "{}",
                path.display()
            );
        }
    }
}
