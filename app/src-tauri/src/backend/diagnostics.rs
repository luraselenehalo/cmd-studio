//! `diagnostics_copy` (spec §4.3 "แจ้งปัญหา", §5.10): the text the user copies into a GitHub
//! issue. English only. Every path that holds the Windows user name, as the profile folder's
//! name or its 8.3 alias, has it redacted to `%USERNAME%`, and every account SID to `<SID>`.
//! Of AutoRun it holds counts and flags, never the user's commands, and of the user's own
//! settings in checks' details nothing; of the preview's sample (the user, the computer, the
//! home folder), nothing. The text is the answer and is written nowhere: it leaves the machine
//! only when the user pastes it.
use super::{versions, Backend, Machine};
use crate::error::AppError;
use crate::settings::{AppSettings, Language, UpdateMode};
use cmdstudio_core::adapters::{Arch, DefaultTerminal, WtVariant};
use cmdstudio_core::doctor::{doctor, Check, Status};
use cmdstudio_core::model::Target;
use cmdstudio_core::snapshot::SnapshotKind;
use std::path::{Path, PathBuf};

/// What a user name becomes in the copied text (spec §5.10).
const USER_PLACEHOLDER: &str = "%USERNAME%";
/// What an account SID becomes in the copied text.
const SID_PLACEHOLDER: &str = "<SID>";
/// How the SIDs of user accounts start: a local or domain account's, an Entra ID account's.
const USER_SID_PREFIXES: [&str; 2] = ["S-1-5-21-", "S-1-12-1-"];

impl<M: Machine> Backend<M> {
    /// The diagnostics, as text for the user to copy. It reads what the checks read and keeps
    /// nothing of it: no file, no log line holds the text. (The settings are read as by any
    /// command, so a damaged settings.json met here first is set aside, as `settings::load`
    /// does.) When the user name can't be told, nothing is copied: the error says why.
    pub fn diagnostics_copy(&self) -> Result<String, AppError> {
        let names = self.user_names()?;
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
                // Read, not `exists`: a file that can't be read is said so, never taken for
                // one that is not there.
                let settings = match sys.fs.read(&l.settings) {
                    Ok(Some(_)) => "yes",
                    Ok(None) => "no",
                    Err(_) => "could not be read",
                };
                format!(
                    "{} (installed {}, settings.json {settings})",
                    variant_name(l.variant),
                    yes_no(l.installed)
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
            if quotes_the_users_own(&c) {
                if !c.details.is_empty() {
                    out.push_str(
                        "           (details left out: they quote the user's own commands or \
                         settings)\n",
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
        match self.log.try_tail(sys.fs, 100) {
            Ok(tail) if tail.is_empty() => out.push_str("(no lines yet)\n"),
            Ok(tail) => {
                out.push_str(&tail);
                out.push('\n');
            }
            Err(e) => out.push_str(&format!("could not be read: {e}\n")),
        }
        let out = redact_sids(&out, sys.host.user_sid().ok().as_deref());
        Ok(names.iter().fold(out, |text, name| redact(&text, name)))
    }

    /// The forms of the user name that a path in the text can hold, longest first: the profile
    /// folder's name (the folder before `AppData` in the data folder's own path) and its 8.3
    /// alias (for a profile path that cmd can't take, the planner starts Clink from the alias,
    /// and the checks print that path). Taken from the data folder, the name is never empty:
    /// when its path has no `AppData` folder, or the alias can't be read, this is the error,
    /// and nothing is copied.
    fn user_names(&self) -> Result<Vec<String>, AppError> {
        let Some(profile) = profile_of(&self.paths().data) else {
            return Err(AppError::internal(
                "the data folder's path holds no AppData folder, so the user name in the \
                 diagnostics could not be told; they are not copied with it in them",
            ));
        };
        let short = self.sys().host.short_path(&profile).map_err(|_| {
            AppError::coded(
                "E_MACHINE_READ",
                "the 8.3 name of the profile folder could not be read, so the diagnostics are \
                 not copied: it could be in them",
            )
        })?;
        let mut names: Vec<String> = [profile.as_path(), short.as_path()]
            .iter()
            .filter_map(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .collect();
        names.sort_by_key(|n| std::cmp::Reverse(n.chars().count()));
        names.dedup();
        Ok(names)
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
}

/// Checks whose details can quote the user's own commands or settings, which the diagnostics
/// leave out: D4's AutoRun problems and D5 put AutoRun's values there, R3's way to fix an
/// AutoRun step by hand can give the value it had, and D7's mismatch (`E_WT_WRITE`) quotes the
/// cmd profile's command line in Windows Terminal as the user set it. AutoRun is given as counts
/// and flags instead.
fn quotes_the_users_own(c: &Check) -> bool {
    matches!(
        (c.id, c.code),
        ("D5", _)
            | ("D4", Some("E_AUTORUN_WRITE"))
            | ("D7", Some("E_WT_WRITE"))
            | ("R3", Some("E_ROLLBACK_INCOMPLETE"))
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

/// The profile folder in a path under it: `C:\Users\<name>\AppData\Local` → `C:\Users\<name>`,
/// the folders up to the one before `AppData`; `None` when the path has no `AppData` folder
/// with a folder before it.
fn profile_of(path: &Path) -> Option<PathBuf> {
    let mut profile = PathBuf::new();
    for part in path.components() {
        if part.as_os_str().eq_ignore_ascii_case("AppData") {
            return profile.file_name().is_some().then_some(profile);
        }
        profile.push(part);
    }
    None
}

/// `C:\Users\<name>\AppData\Local` → `<name>`; empty when the path has no `AppData` folder.
pub fn user_name_of(local_app_data: &Path) -> String {
    profile_of(local_app_data)
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_default()
}

/// Replaces the user name `user` by `%USERNAME%` wherever it is a whole folder name: right
/// after `\` or `/`, and up to a character that is neither a letter or digit nor a combining
/// mark (a mark belongs to the letter before it), or the end of the text. So with user
/// `tester`, `\testers\` and `\tester2\` stay: they are other folders (a profile folder named
/// `testers` is its own name, and is redacted as that); and with user `สม`, `\สม่ำเสมอ` stays.
/// The name is matched letter by letter on the text as it is, without regard to case, so a
/// letter that is longer or shorter in another case (İ, the Kelvin sign) never moves or hides a
/// match.
pub fn redact(text: &str, user: &str) -> String {
    if user.is_empty() {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(['\\', '/']) {
        // A slash is one byte, so the text after it starts at a character.
        let (Some(through_slash), Some(after)) = (rest.get(..=at), rest.get(at + 1..)) else {
            break;
        };
        out.push_str(through_slash);
        rest = after;
        if let Some(len) = name_at(rest, user) {
            out.push_str(USER_PLACEHOLDER);
            rest = rest.get(len..).unwrap_or("");
        }
    }
    out.push_str(rest);
    out
}

/// The length in bytes of `user` at the start of `text`, when it is there as a whole name (see
/// [`redact`]).
fn name_at(text: &str, user: &str) -> Option<usize> {
    let mut chars = text.char_indices();
    for wanted in user.chars() {
        let (_, found) = chars.next()?;
        if !same_letter(found, wanted) {
            return None;
        }
    }
    match chars.next() {
        None => Some(text.len()),
        Some((end, next)) => (!continues_word(next)).then_some(end),
    }
}

/// The same letter, in any case: equal, or equal in lower or in upper case.
fn same_letter(a: char, b: char) -> bool {
    a == b || a.to_lowercase().eq(b.to_lowercase()) || a.to_uppercase().eq(b.to_uppercase())
}

/// A character that carries a word on: a letter or a digit, or a combining mark, which belongs
/// to the letter before it.
fn continues_word(c: char) -> bool {
    c.is_alphanumeric()
        || COMBINING_MARKS
            .iter()
            .any(|(from, to)| (*from..=*to).contains(&c))
}

/// Combining marks (Unicode category M) that `char::is_alphanumeric` does not count as letters.
/// The standard library has no table of categories, so these are the marks of the scripts a
/// Windows user name is likely written in; most other marks, such as the Indic vowel signs, are
/// letters to `is_alphanumeric` already.
const COMBINING_MARKS: &[(char, char)] = &[
    // The general combining blocks: diacritics, their extensions, marks for symbols, half marks.
    ('\u{0300}', '\u{036F}'),
    ('\u{1AB0}', '\u{1AFF}'),
    ('\u{1DC0}', '\u{1DFF}'),
    ('\u{20D0}', '\u{20FF}'),
    ('\u{FE20}', '\u{FE2F}'),
    // Cyrillic.
    ('\u{0483}', '\u{0489}'),
    // Hebrew points.
    ('\u{0591}', '\u{05BD}'),
    ('\u{05BF}', '\u{05BF}'),
    ('\u{05C1}', '\u{05C2}'),
    ('\u{05C4}', '\u{05C5}'),
    ('\u{05C7}', '\u{05C7}'),
    // Arabic marks.
    ('\u{0610}', '\u{061A}'),
    ('\u{064B}', '\u{065F}'),
    ('\u{0670}', '\u{0670}'),
    ('\u{06D6}', '\u{06DC}'),
    ('\u{06DF}', '\u{06E4}'),
    ('\u{06E7}', '\u{06E8}'),
    ('\u{06EA}', '\u{06ED}'),
    // Indic nukta, virama and stress signs: Devanagari to Sinhala.
    ('\u{093C}', '\u{093C}'),
    ('\u{094D}', '\u{094D}'),
    ('\u{0951}', '\u{0954}'),
    ('\u{09BC}', '\u{09BC}'),
    ('\u{09CD}', '\u{09CD}'),
    ('\u{0A3C}', '\u{0A3C}'),
    ('\u{0A4D}', '\u{0A4D}'),
    ('\u{0ABC}', '\u{0ABC}'),
    ('\u{0ACD}', '\u{0ACD}'),
    ('\u{0B3C}', '\u{0B3C}'),
    ('\u{0B4D}', '\u{0B4D}'),
    ('\u{0BCD}', '\u{0BCD}'),
    ('\u{0C4D}', '\u{0C4D}'),
    ('\u{0CBC}', '\u{0CBC}'),
    ('\u{0CCD}', '\u{0CCD}'),
    ('\u{0D3B}', '\u{0D3C}'),
    ('\u{0D4D}', '\u{0D4D}'),
    ('\u{0DCA}', '\u{0DCA}'),
    // Thai vowel and tone marks.
    ('\u{0E31}', '\u{0E31}'),
    ('\u{0E34}', '\u{0E3A}'),
    ('\u{0E47}', '\u{0E4E}'),
    // Lao vowel and tone marks.
    ('\u{0EB1}', '\u{0EB1}'),
    ('\u{0EB4}', '\u{0EBC}'),
    ('\u{0EC8}', '\u{0ECE}'),
];

/// Replaces every account SID by `<SID>`: `sid`, this account's as the host reads it, wherever
/// it is, and any other in the form Windows gives a user account's (`S-1-5-21-` or `S-1-12-1-`,
/// then its numbers).
pub fn redact_sids(text: &str, sid: Option<&str>) -> String {
    let text = match sid.filter(|s| !s.is_empty()) {
        Some(sid) => text.replace(sid, SID_PLACEHOLDER),
        None => text.to_string(),
    };
    let mut out = String::with_capacity(text.len());
    let mut rest = text.as_str();
    while let Some((at, numbers)) = USER_SID_PREFIXES
        .iter()
        .filter_map(|p| rest.find(p).map(|at| (at, at + p.len())))
        .min()
    {
        // The prefixes are ASCII, so these are at characters.
        let (Some(before), Some(prefix), Some(after)) =
            (rest.get(..at), rest.get(at..numbers), rest.get(numbers..))
        else {
            break;
        };
        out.push_str(before);
        let run = after
            .find(|c: char| !c.is_ascii_digit() && c != '-')
            .unwrap_or(after.len());
        let run = after.get(..run).unwrap_or("").trim_end_matches('-').len();
        if after.starts_with(|c: char| c.is_ascii_digit()) {
            out.push_str(SID_PLACEHOLDER);
            rest = after.get(run..).unwrap_or("");
        } else {
            out.push_str(prefix);
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::testing::{apply_terracotta, backend, fake_fs, WT};
    use crate::settings::Language;
    use cmdstudio_core::adapters::{AutoRun, RegValue, REG_SZ};
    use cmdstudio_core::plan::Paths;
    use std::path::PathBuf;

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
        // İ and i are two letters, in any case, as they are to Windows.
        assert_eq!(
            redact(r"C:\Users\İsa\x \isa", "İsa"),
            r"C:\Users\%USERNAME%\x \isa"
        );
    }

    /// The rule: the name is the user's where it is a whole folder name, right after `\` or `/`
    /// and up to a character that is not a letter or a digit, or the end of the text. With
    /// user `tester`, `\testers\` and `\tester2\` are other folders and stay.
    #[test]
    fn the_name_is_redacted_after_either_slash_up_to_any_non_letter() {
        assert_eq!(
            redact("C:/Users/tester/AppData", "tester"),
            "C:/Users/%USERNAME%/AppData"
        );
        for end in [".", ",", "&&", ":", "\n", ""] {
            assert_eq!(
                redact(&format!(r"in \tester{end}"), "tester"),
                format!(r"in \%USERNAME%{end}"),
                "{end:?}"
            );
        }
        assert_eq!(
            redact(r"C:\USERS\TeStEr\x c:/users/TESTER", "tester"),
            r"C:\USERS\%USERNAME%\x c:/users/%USERNAME%"
        );
        for other in [
            r"C:\Users\testers\x",
            r"C:\Users\tester2\x",
            r"C:\Users\testerä\x",
            r"C:\Users\attester\x",
        ] {
            assert_eq!(redact(other, "tester"), other);
        }
    }

    /// Letters whose lower case is longer (İ) or shorter (the Kelvin sign) than they are never
    /// move where the name is found, nor stop it being found.
    #[test]
    fn letters_that_change_length_in_lower_case_do_not_move_the_name() {
        let text = "İstanbul İzmir C:\\Users\\tester\\x C:\\Users\\Tester\\y 300 \u{212A}";
        assert_eq!(
            redact(text, "tester"),
            "İstanbul İzmir C:\\Users\\%USERNAME%\\x C:\\Users\\%USERNAME%\\y 300 \u{212A}"
        );
    }

    #[test]
    fn a_thai_name_is_redacted_whole_and_only_whole() {
        let name = "ผู้ทดสอบ";
        assert_eq!(
            redact(r"C:\Users\ผู้ทดสอบ\AppData C:/Users/ผู้ทดสอบ/x", name),
            r"C:\Users\%USERNAME%\AppData C:/Users/%USERNAME%/x"
        );
        // A longer Thai word is another folder.
        assert_eq!(
            redact(r"C:\Users\ผู้ทดสอบระบบ\x", name),
            r"C:\Users\ผู้ทดสอบระบบ\x"
        );
    }

    /// A combining mark after the name belongs to the letter before it, so the name goes on:
    /// with user `สม`, `\สม่ำเสมอ` is another word, and with user `jose`, `\jose` + U+0301 is
    /// `josé`. A mark never ends a name, and none is left behind.
    #[test]
    fn combining_marks_belong_to_the_name() {
        for other in [r"C:\Users\สม่ำเสมอ\x", r"C:\Users\สมิง\x", r"C:\Users\สม์\x"]
        {
            assert_eq!(redact(other, "สม"), other);
        }
        assert_eq!(
            redact(r"C:\Users\สม\x C:/Users/สม", "สม"),
            r"C:\Users\%USERNAME%\x C:/Users/%USERNAME%"
        );
        let jose_accent = "C:\\Users\\jose\u{0301}\\x";
        assert_eq!(redact(jose_accent, "jose"), jose_accent);
        assert_eq!(
            redact(jose_accent, "jose\u{0301}"),
            r"C:\Users\%USERNAME%\x"
        );
    }

    /// The profile folder's 8.3 name is the user's too: for a profile path that cmd can't take,
    /// the planner starts Clink from it, and the checks print that path. The name and its alias
    /// are read from the data folder's own path, so the copied text quotes neither.
    #[test]
    fn the_profile_folders_short_name_is_redacted_too() {
        let mut b = backend();
        b.paths = Paths {
            data: PathBuf::from(r"C:\Users\testername\AppData\Local\ai.resincore.cmdstudio"),
        };
        b.machine.host.short.insert(
            PathBuf::from(r"C:\Users\testername"),
            PathBuf::from(r"C:\Users\TESTER~1"),
        );
        // The log is quoted as it is: a line that holds both forms.
        b.sys()
            .fs
            .write_atomic(
                &b.log.file(),
                b"2026-10-17T00:00:00Z cmd=x C:\\Users\\TESTER~1\\AppData\\Local\\x and \
                  C:/Users/testername/y\n",
            )
            .unwrap();
        let text = b.diagnostics_copy().unwrap();
        assert!(
            text.contains(r"data folder: C:\Users\%USERNAME%\AppData\Local\ai.resincore.cmdstudio"),
            "{text}"
        );
        assert!(
            text.contains(r"cmd=x C:\Users\%USERNAME%\AppData\Local\x and C:/Users/%USERNAME%/y"),
            "{text}"
        );
        assert!(
            !text.contains("TESTER~1") && !text.to_lowercase().contains("testername"),
            "{text}"
        );
    }

    /// Without a user name to redact, nothing is copied: never a report with the name in it.
    #[test]
    fn with_no_user_name_to_redact_nothing_is_copied() {
        let mut b = backend();
        b.paths = Paths {
            data: PathBuf::from(r"D:\CMD Studio data"),
        };
        assert_eq!(b.diagnostics_copy().unwrap_err().code, "E_INTERNAL");
    }

    /// D7's details quote the user's own cmd command line in Windows Terminal: left out, as
    /// AutoRun's are.
    #[test]
    fn the_users_own_terminal_command_line_is_left_out() {
        let b = backend();
        apply_terracotta(&b);
        let wt = String::from_utf8(b.sys().fs.read(Path::new(WT)).unwrap().unwrap()).unwrap();
        let ours = r#""%SystemRoot%\\System32\\cmd.exe""#;
        assert_eq!(wt.matches(ours).count(), 1, "{wt}");
        let mine = wt.replacen(ours, r#""cmd.exe /k C:\\Tools\\my-secret-init.cmd""#, 1);
        b.sys()
            .fs
            .write_atomic(Path::new(WT), mine.as_bytes())
            .unwrap();
        let text = b.diagnostics_copy().unwrap();
        assert!(
            text.contains("[E_WT_WRITE]\n           (details left out"),
            "{text}"
        );
        assert!(!text.contains("my-secret-init"), "{text}");
    }

    /// What could not be read is said so: never "no" for a file that may be there, never an
    /// empty section.
    #[test]
    fn what_could_not_be_read_is_said_so() {
        let b = backend();
        let text = b.diagnostics_copy().unwrap();
        assert!(
            text.contains("Windows Terminal: Store (installed yes, settings.json yes)\n"),
            "{text}"
        );
        assert!(
            text.ends_with("== log (last 100 lines) ==\n(no lines yet)\n"),
            "{text}"
        );
        let _ = b.logged("themes_list", |_| Ok(()));
        fake_fs(&b).fail_reads_of(Path::new(WT));
        fake_fs(&b).fail_reads_of(b.log.file());
        let text = b.diagnostics_copy().unwrap();
        assert!(
            text.contains(
                "Windows Terminal: Store (installed yes, settings.json could not be read)\n"
            ),
            "{text}"
        );
        assert!(
            text.contains("== log (last 100 lines) ==\ncould not be read: "),
            "{text}"
        );
    }

    /// The account's SID names the user too, in the private ProgramData folder the planner may
    /// start Clink from: this account's, as the host reads it (here an Entra ID one), and any
    /// other user's SID, read by its form.
    #[test]
    fn account_sids_are_redacted() {
        let mut b = backend();
        b.machine.host.sid = "S-1-12-1-11-22-33-44".to_string();
        b.sys()
            .fs
            .write_atomic(
                &b.log.file(),
                b"2026-10-17T00:00:00Z cmd=x \
                  C:\\ProgramData\\ai.resincore.cmdstudio-S-1-12-1-11-22-33-44\\clink_x64.exe \
                  owner S-1-5-21-555-666-777-1001.\n",
            )
            .unwrap();
        let text = b.diagnostics_copy().unwrap();
        assert!(
            text.contains(
                r"cmd=x C:\ProgramData\ai.resincore.cmdstudio-<SID>\clink_x64.exe owner <SID>."
            ),
            "{text}"
        );
        assert!(
            !text.contains("S-1-12-1") && !text.contains("S-1-5-21"),
            "{text}"
        );
        // By its form alone: the numbers go, a dash after them stays; a well-known SID, or the
        // start of one with no number after it, is no account's.
        assert_eq!(
            redact_sids("a S-1-5-21-1-2-3-1001-b S-1-5-18 S-1-5-21-x", None),
            "a <SID>-b S-1-5-18 S-1-5-21-x"
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
        assert!(quotes_the_users_own(&check("R3", "E_ROLLBACK_INCOMPLETE")));
        assert!(!quotes_the_users_own(&check("R3", "E_SNAPSHOT_READ")));
        assert!(!quotes_the_users_own(&check("D4", "E_RUNTIME_MISSING")));
        assert!(!quotes_the_users_own(&check("D9", "E_GENERATED_EDITED")));
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
