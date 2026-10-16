//! `diagnostics_copy` (spec §4.3 "แจ้งปัญหา", §5.10): the text the user copies into a GitHub
//! issue. English only, with every path that holds the Windows user name redacted to
//! `%USERNAME%`. Of AutoRun it holds counts and flags, never the user's commands; of the
//! preview's sample (the user, the computer, the home folder), nothing. The text is the answer
//! and is written nowhere: it leaves the machine only when the user pastes it.
use super::{versions, Backend, Machine};
use crate::error::AppError;
use crate::settings::{AppSettings, Language, UpdateMode};
use cmdstudio_core::adapters::{Arch, DefaultTerminal, WtVariant};
use cmdstudio_core::doctor::{doctor, Check, Status};
use cmdstudio_core::model::Target;
use cmdstudio_core::snapshot::SnapshotKind;
use std::path::Path;

impl<M: Machine> Backend<M> {
    /// The diagnostics, as text for the user to copy. It reads what the checks read and keeps
    /// nothing of it: no file, no log line holds the text. (The settings are read as by any
    /// command, so a damaged settings.json met here first is set aside, as `settings::load`
    /// does.)
    pub fn diagnostics_copy(&self) -> Result<String, AppError> {
        let sys = self.sys();
        let ctx = self.ctx();
        let v = versions();
        let info = sys.host.system_info();
        let mut out = String::new();
        out.push_str("CMD Studio diagnostics\n");
        out.push_str(&format!(
            "app {} · Oh My Posh {} · Clink {} · Nerd Fonts {}\n",
            v.app, v.oh_my_posh, v.clink, v.nerd_fonts
        ));
        out.push_str(&format!(
            "Windows build {} · {} · packaged {} · default terminal {}\n",
            info.build
                .map_or_else(|| "unknown".to_string(), |b| b.to_string()),
            arch_name(info.arch),
            yes_no(info.packaged),
            terminal_name(info.default_terminal)
        ));
        for u in &info.unreadable {
            out.push_str(&format!("unreadable: {u}\n"));
        }
        let terminals: Vec<String> = sys
            .wt
            .locations()
            .iter()
            .map(|l| {
                format!(
                    "{} (installed {}, settings.json {})",
                    variant_name(l.variant),
                    yes_no(l.installed),
                    yes_no(sys.fs.exists(&l.settings))
                )
            })
            .collect();
        out.push_str(&format!(
            "Windows Terminal: {}\n",
            if terminals.is_empty() {
                "none found".to_string()
            } else {
                terminals.join(" · ")
            }
        ));
        // Counts and flags only: the commands are the user's own.
        match self.autorun_now() {
            Ok(a) => out.push_str(&format!(
                "autorun: CMD Studio's entry {} · other commands {} · other Clink {} · \
                 machine-wide Clink {} · not plain text {}\n",
                yes_no(a.ours_present),
                a.other_commands.len(),
                yes_no(!a.foreign_clink.is_empty()),
                yes_no(a.machine_wide_clink),
                yes_no(a.not_text)
            )),
            Err(e) => out.push_str(&format!(
                "autorun: could not be read (E_MACHINE_READ): {e}\n"
            )),
        }
        out.push_str(&format!("data folder: {}\n", self.paths().data.display()));
        out.push_str(&self.settings_line());
        let (applied, state_problem) = self.applied_info();
        match (applied, state_problem) {
            (Some(a), _) => out.push_str(&format!(
                "applied: \"{}\" ({}) target {} at {} restore point {}\n",
                a.theme_name,
                a.theme_id,
                target_name(a.target),
                crate::logs::iso_utc(a.applied_at_ms),
                a.snapshot_id
            )),
            (None, Some(e)) => out.push_str(&format!("state.json: {}: {}\n", e.code, e.details)),
            (None, None) => out.push_str("applied: nothing\n"),
        }
        out.push_str("\n== doctor ==\n");
        for c in doctor(sys, &ctx) {
            let code = c.code.map(|c| format!(" [{c}]")).unwrap_or_default();
            out.push_str(&format!(
                "{:<4} {:<5} {}{code}\n",
                c.id,
                status_name(c.status),
                c.message_en
            ));
            if quotes_autorun(&c) {
                if !c.details.is_empty() {
                    out.push_str(
                        "           (details left out: they quote AutoRun, which holds other \
                         programs' commands)\n",
                    );
                }
                continue;
            }
            for line in c.details.lines() {
                out.push_str(&format!("           {line}\n"));
            }
        }
        out.push_str("\n== restore points ==\n");
        match self.snapshots_list() {
            Ok(list) => {
                for p in &list.points {
                    let mut line = format!("{} {}", p.id, kind_name(p.kind));
                    if let Some(t) = &p.theme {
                        line.push_str(&format!(" \"{}\"", t.name));
                    }
                    if let Some(u) = &p.undoes {
                        line.push_str(&format!(" undoes {u}"));
                    }
                    if let Some(u) = &p.undone_by {
                        line.push_str(&format!(" undone by {u}"));
                    }
                    if p.restorable {
                        line.push_str(" restorable");
                    }
                    out.push_str(&line);
                    out.push('\n');
                }
                for n in &list.problems {
                    out.push_str(&format!("problem [{}]: {}\n", n.code, n.message_en));
                }
                if list.points.is_empty() && list.problems.is_empty() {
                    out.push_str("none\n");
                }
            }
            Err(e) => out.push_str(&format!(
                "could not be listed [{}]: {}\n",
                e.code, e.details
            )),
        }
        out.push_str("\n== log (last 100 lines) ==\n");
        out.push_str(&self.log_tail(100));
        out.push('\n');
        Ok(redact(&out, &self.user_name()))
    }

    /// The settings line. Settings that could not be loaded are said to be so, and are never
    /// shown as the user's own: every command works from the defaults then
    /// (`settings_or_default`), and so do these lines. The notice of a set-aside file is read,
    /// not taken: the Settings page still gives it.
    fn settings_line(&self) -> String {
        match self.with_settings(|cache| Ok(cache.reset_from.clone())) {
            // The read that succeeded is held, so `settings_or_default` gives what it read.
            Ok(None) => format!("settings: {}\n", settings_text(&self.settings_or_default())),
            Ok(Some(aside)) => format!(
                "settings: settings.json could not be used and was set aside as {aside}; in use: \
                 {}\n",
                settings_text(&self.settings_or_default())
            ),
            Err(e) => format!(
                "settings: could not be loaded ({}), defaults shown: {}\n",
                e.code,
                settings_text(&AppSettings::default())
            ),
        }
    }

    /// The Windows user name, as the folder before `AppData` in the local app data path.
    fn user_name(&self) -> String {
        let Ok(local) = self.sys().host.local_app_data() else {
            return String::new();
        };
        user_name_of(&local)
    }
}

/// Checks whose details can hold AutoRun's text, and with it the user's own commands: D4's
/// AutoRun problems and D5 put the values there, and R3's way to fix an AutoRun step by hand
/// can give the value it had. The diagnostics give AutoRun as counts and flags instead.
fn quotes_autorun(c: &Check) -> bool {
    matches!(
        (c.id, c.code),
        ("D5", _) | ("D4", Some("E_AUTORUN_WRITE")) | ("R3", Some("E_ROLLBACK_INCOMPLETE"))
    )
}

fn yes_no(b: bool) -> &'static str {
    if b {
        "yes"
    } else {
        "no"
    }
}

fn arch_name(arch: Arch) -> &'static str {
    match arch {
        Arch::X64 => "x64",
        Arch::Arm64 => "arm64",
        Arch::Other => "other",
    }
}

/// The default terminal setting in words, as doctor D11 says it.
fn terminal_name(t: DefaultTerminal) -> &'static str {
    match t {
        DefaultTerminal::LetWindowsDecide => "not set (Windows decides)",
        DefaultTerminal::WindowsTerminal => "Windows Terminal",
        DefaultTerminal::WindowsTerminalPreview => "Windows Terminal Preview",
        DefaultTerminal::Conhost => "Windows Console Host (conhost)",
        DefaultTerminal::Other => "another app",
    }
}

fn variant_name(v: WtVariant) -> &'static str {
    match v {
        WtVariant::Store => "Store",
        WtVariant::Preview => "Preview",
        WtVariant::Unpackaged => "unpackaged",
    }
}

fn status_name(s: Status) -> &'static str {
    match s {
        Status::Ok => "ok",
        Status::Warn => "warn",
        Status::Fail => "FAIL",
        Status::Skipped => "skip",
    }
}

fn kind_name(k: SnapshotKind) -> &'static str {
    match k {
        SnapshotKind::Apply => "apply",
        SnapshotKind::Undo => "undo",
        SnapshotKind::RevertAll => "revert all",
    }
}

/// As the CLI's `--target` names it.
fn target_name(t: Target) -> &'static str {
    match t {
        Target::All => "all",
        Target::Wt => "wt",
    }
}

fn settings_text(s: &AppSettings) -> String {
    format!(
        "language {} · target {} · wt default {} · updates {} · wizard done {}",
        match s.language {
            Language::En => "en",
            Language::Th => "th",
        },
        target_name(s.target),
        yes_no(s.set_wt_default_profile),
        match s.updates {
            UpdateMode::Auto => "auto",
            UpdateMode::Notify => "notify",
            UpdateMode::Off => "off",
        },
        yes_no(s.wizard_done)
    )
}

/// `C:\Users\<name>\AppData\Local` → `<name>`; empty when the path has no `AppData` folder.
pub fn user_name_of(local_app_data: &Path) -> String {
    let parts: Vec<String> = local_app_data
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    parts
        .iter()
        .position(|p| p.eq_ignore_ascii_case("AppData"))
        .and_then(|i| i.checked_sub(1))
        .and_then(|i| parts.get(i))
        .cloned()
        .unwrap_or_default()
}

/// Replaces `\<user>` by `\%USERNAME%` wherever it ends a folder name (followed by `\`, `/`, a
/// quote, `)`, `]`, `;`, whitespace or the end), without regard to letter case. A name that
/// changes length when lower-cased falls back to an exact-case replacement.
pub fn redact(text: &str, user: &str) -> String {
    if user.is_empty() {
        return text.to_string();
    }
    let needle = format!("\\{user}");
    let (lower, lower_needle) = (text.to_lowercase(), needle.to_lowercase());
    if lower.len() != text.len() || lower_needle.len() != needle.len() {
        return text.replace(&needle, "\\%USERNAME%");
    }
    let ends_name = |rest: &str| {
        rest.chars().next().is_none_or(|c| {
            matches!(c, '\\' | '/' | '"' | '\'' | ')' | ']' | ';') || c.is_whitespace()
        })
    };
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while let Some(pos) = lower.get(i..).and_then(|rest| rest.find(&lower_needle)) {
        let at = i + pos;
        let end = at + lower_needle.len();
        let (Some(before), Some(found), Some(after)) =
            (text.get(i..at), text.get(at..end), text.get(end..))
        else {
            return text.replace(&needle, "\\%USERNAME%");
        };
        out.push_str(before);
        out.push_str(if ends_name(after) {
            "\\%USERNAME%"
        } else {
            found
        });
        i = end;
    }
    out.push_str(text.get(i..).unwrap_or(""));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testing::{apply_terracotta, backend, fake_fs};
    use crate::settings::Language;
    use cmdstudio_core::adapters::{AutoRun, RegValue, REG_SZ};

    #[test]
    fn the_user_name_is_the_folder_before_appdata() {
        assert_eq!(
            user_name_of(Path::new(r"C:\Users\tester\AppData\Local")),
            "tester"
        );
        assert_eq!(
            user_name_of(Path::new(r"C:\Users\ผู้ทดสอบ ไทย\AppData\Local")),
            "ผู้ทดสอบ ไทย"
        );
        assert_eq!(user_name_of(Path::new(r"D:\Local")), "");
    }

    #[test]
    fn paths_with_the_user_name_are_redacted_whatever_their_case() {
        let text = r"C:\Users\Tester\AppData\Local\x; c:\users\tester\Desktop; C:\Users\testers\x; \tester";
        assert_eq!(
            redact(text, "tester"),
            r"C:\Users\%USERNAME%\AppData\Local\x; c:\users\%USERNAME%\Desktop; C:\Users\testers\x; \%USERNAME%"
        );
        assert_eq!(
            redact(r"C:\Users\ผู้ทดสอบ ไทย\AppData", "ผู้ทดสอบ ไทย"),
            r"C:\Users\%USERNAME%\AppData"
        );
        assert_eq!(redact("nothing here", "tester"), "nothing here");
        assert_eq!(redact(r"C:\Users\tester", ""), r"C:\Users\tester");
        // A name whose lower case is longer falls back to exact-case replacement.
        assert_eq!(
            redact(r"C:\Users\İsa\x \isa", "İsa"),
            r"C:\Users\%USERNAME%\x \isa"
        );
    }

    #[test]
    fn the_diagnostics_hold_the_versions_the_doctor_and_no_user_name() {
        let b = backend();
        apply_terracotta(&b);
        let text = b.diagnostics_copy().unwrap();
        assert!(
            text.starts_with(
                "CMD Studio diagnostics\napp 0.1.0 · Oh My Posh 31.5.0 · Clink 1.9.34"
            ),
            "{text}"
        );
        assert!(text.contains("applied: \"Terracotta\""), "{text}");
        assert!(text.contains("== doctor ==\nD1   ok"), "{text}");
        assert!(text.contains("== restore points ==\n1791694709"), "{text}");
        assert!(text.contains(r"\%USERNAME%\"), "{text}");
        assert!(!text.contains(r"\tester\"), "{text}");
    }

    /// Settings that could not be loaded are said, never shown as the user's own: the defaults
    /// stand in for them (`settings_or_default`).
    #[test]
    fn settings_that_could_not_be_loaded_are_said_not_shown_as_the_users() {
        let b = backend();
        let file = b.paths().settings_file();
        fake_fs(&b).fail_reads_of(&file);
        let text = b.diagnostics_copy().unwrap();
        assert!(
            text.contains(
                "settings: could not be loaded (E_MACHINE_READ), defaults shown: language en · \
                 target all · wt default yes · updates auto · wizard done no\n"
            ),
            "{text}"
        );
        // A damaged file is set aside, and the defaults are in use; the Settings page still has
        // the notice to give.
        fake_fs(&b).stop_failing(&file);
        b.sys().fs.write_atomic(&file, b"{ not json").unwrap();
        let text = b.diagnostics_copy().unwrap();
        assert!(
            text.contains(
                "settings: settings.json could not be used and was set aside as \
                 settings.json.damaged-"
            ),
            "{text}"
        );
        assert!(b.settings_get().unwrap().reset_from.is_some());
        // The user's own settings, once saved, are shown as they are.
        let mine = crate::settings::AppSettings {
            language: Language::Th,
            wizard_done: true,
            ..crate::settings::AppSettings::default()
        };
        b.settings_set(mine).unwrap();
        let text = b.diagnostics_copy().unwrap();
        assert!(
            text.contains(
                "settings: language th · target all · wt default yes · updates auto · \
                 wizard done yes\n"
            ),
            "{text}"
        );
    }

    /// AutoRun holds the user's own commands, and the preview's sample the user's name, computer
    /// and home: the diagnostics go to a public issue, so they hold counts and flags, never
    /// those texts, not even in the doctor's details.
    #[test]
    fn autorun_is_counted_never_quoted_and_the_sample_is_left_out() {
        let b = backend();
        apply_terracotta(&b);
        let ours = b
            .machine
            .autorun
            .read_user()
            .unwrap()
            .unwrap()
            .text()
            .unwrap();
        let program = r#""C:\Tools\secret-sync.cmd" --token abc123"#;
        let clink = r#""C:\clink\clink_x64.exe" inject --autorun"#;
        // Another program's command first: D4 fails and D5 warns, each with AutoRun in its
        // details.
        b.machine
            .autorun
            .write_user(&RegValue::from_text(
                REG_SZ,
                &format!("{program} & {ours} & {clink}"),
            ))
            .unwrap();
        let text = b.diagnostics_copy().unwrap();
        assert!(
            text.contains(
                "autorun: CMD Studio's entry yes · other commands 1 · other Clink yes · \
                 machine-wide Clink no · not plain text no\n"
            ),
            "{text}"
        );
        assert!(text.contains("D4   FAIL"), "{text}");
        assert!(text.contains("D5   warn"), "{text}");
        for secret in ["secret-sync", "abc123", r"C:\clink", "inject --autorun"] {
            assert!(!text.contains(secret), "{secret} in {text}");
        }
        assert!(
            !text.contains("TESTPC") && !text.contains(r"C:\Users\tester"),
            "{text}"
        );
        // The record of a rollback that did not finish can give AutoRun's old value in the way
        // to fix it by hand; its other codes, and the other checks, quote no AutoRun.
        let check = |id: &'static str, code: &'static str| Check {
            id,
            status: Status::Fail,
            code: Some(code),
            message_th: String::new(),
            message_en: String::new(),
            details: String::new(),
            details_th: None,
        };
        assert!(quotes_autorun(&check("R3", "E_ROLLBACK_INCOMPLETE")));
        assert!(!quotes_autorun(&check("R3", "E_SNAPSHOT_READ")));
        assert!(!quotes_autorun(&check("D4", "E_RUNTIME_MISSING")));
        assert!(!quotes_autorun(&check("D9", "E_GENERATED_EDITED")));
    }

    /// Nothing CMD Studio itself writes in the diagnostics is Thai: the core's English words
    /// only, never its Thai ones (the test fixtures hold no Thai path or name).
    #[test]
    fn the_diagnostics_hold_no_thai() {
        let thai = |text: &str| -> Vec<char> {
            text.chars()
                .filter(|c| ('\u{0E00}'..='\u{0E7F}').contains(c))
                .collect()
        };
        let b = backend();
        // Nothing applied: the checks are skipped, with their reasons.
        let text = b.diagnostics_copy().unwrap();
        assert!(thai(&text).is_empty(), "{:?} in {text}", thai(&text));
        // Applied, then checks that warn and fail, and settings that could not be loaded.
        apply_terracotta(&b);
        b.sys()
            .fs
            .write_atomic(&b.paths().generated().join("banner.lua"), b"-- edited")
            .unwrap();
        b.sys().fs.remove_file(&b.paths().omp_exe()).unwrap();
        fake_fs(&b).fail_reads_of(b.paths().settings_file());
        let text = b.diagnostics_copy().unwrap();
        assert!(
            text.contains("D1   FAIL") && text.contains("D9   warn"),
            "{text}"
        );
        assert!(thai(&text).is_empty(), "{:?} in {text}", thai(&text));
    }
}
