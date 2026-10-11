//! The AutoRun editor (spec §5.4): reads the HKCU AutoRun value the way cmd reads it and edits
//! it where it stands, so other programs' commands keep every byte (spec §7, doctrine guard
//! (ค)). Pure, like the rest of the planner.
//!
//! Phase 1b reuses it: the uninstaller's cleanup takes our entries out with [`remove_ours`]
//! (spec §6.3), and the doctor reads the value with [`scan`], [`is_our_entry`] (D4) and
//! [`runs_clink`] (D5).
use super::PlanError;
use crate::model::Target;
use std::ops::Range;

/// One command in an AutoRun value, as [`scan`] finds it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Part {
    /// The command, without the whitespace around it.
    pub(crate) text: String,
    /// Where `text` is in the value, in bytes.
    span: Range<usize>,
    /// The separator before the command, if any, with the one space or tab right before it.
    before: Option<Range<usize>>,
    /// The separator after the command, if any, with the one space or tab right after it.
    after: Option<Range<usize>>,
    /// The command is combined with others in a way this reading does not take apart: it
    /// holds a pipe or a parenthesis that is not quoted or escaped, or an unclosed quote runs
    /// it into the rest of the value.
    pub(crate) combined: bool,
}

impl Part {
    /// The bytes that go when this command is taken out of the value: the command, one
    /// separator next to it with the whitespace between them, and one space or tab on the far
    /// side of that separator, where our own ` & ` puts one. The separator is the one before
    /// the command, or for the first command the one after it. Every other byte stays as it
    /// was, so taking out our entry, which goes in front with ` & `, gives back the value as
    /// it was before.
    ///
    /// In a value that cmd accepts (see [`Scan::rejected`]), the commands left read as before:
    /// no pipe or redirection waits at the end of a command there, so the characters that a
    /// cut puts side by side never pair into something new.
    fn cut(&self) -> Range<usize> {
        match (&self.before, &self.after) {
            (Some(before), _) => before.start..self.span.end,
            (None, Some(after)) => self.span.start..after.end,
            (None, None) => self.span.clone(),
        }
    }
}

/// An AutoRun value as cmd reads it (see [`scan`]).
pub(crate) struct Scan {
    /// The commands, in order.
    pub(crate) parts: Vec<Part>,
    /// cmd itself rejects the value, so nothing in it runs, an entry in front of it included
    /// (checked with `cmd.exe /d /s /c` on Windows 11). That is an empty command before a
    /// separator (a separator first, or two with nothing between), `&&` or `||` last, a pipe
    /// with no command before or after it, a redirection with no file or handle after it, a
    /// `(` never closed, or an empty command inside parentheses (`(& a)`, `(a &)`, `()`). A
    /// single `&` last is fine, and so is a `)` that closes nothing, which cmd reads as text
    /// in a command (`echo a)`). Right after a block, as in `(echo a) )`, cmd refuses such a
    /// `)`, and this reading does not: it is one of the few values cmd rejects that pass here
    /// (ruling P68), and the doctor finds them from a real cmd window (D3).
    ///
    /// Every `(` starts a block here, as after `if` and `for`. In the middle of a command such
    /// as `echo (& a)`, cmd reads it as text and runs the value; it is refused all the same,
    /// which only asks the user for a hand edit.
    pub(crate) rejected: bool,
}

/// Reads an AutoRun value the way cmd reads it: `"` starts and ends quoted text; outside
/// quotes `^` makes the character after it plain text, so `^&`, `^|` and `^"` are text;
/// outside quotes `&`, `&&` and `||` separate commands, while a single `|` is a pipe inside
/// one command, and a `&` right after `<` or `>` names a handle (`2>&1`). Each command is
/// trimmed of the whitespace around it (quoted or escaped whitespace is text), and an empty
/// command, as in `a & & b`, is no part.
pub(crate) fn scan(value: &str) -> Scan {
    let mut parts = Vec::new();
    let mut chars = value.char_indices().peekable();
    let mut quoted = false;
    // The command being read: where its text is, the separator before it, and whether it
    // holds a pipe or a parenthesis.
    let mut span: Option<Range<usize>> = None;
    let mut before = None;
    let mut combined = false;
    // Whether the last character read is a space or tab between words (not quoted, not
    // escaped), and whether it is the `<` or `>` of a redirection.
    let mut blank = false;
    let mut redirect = false;
    // A pipe still waits for its command, or a redirection for its file or handle; and
    // whether the command being read, since the last separator, pipe or `(`, has any text.
    let mut pipe_waits = false;
    let mut redirect_waits = false;
    let mut command_text = false;
    let mut depth = 0usize;
    let mut rejected = false;
    let mut last_doubled = false;
    while let Some((i, c)) = chars.next() {
        // `&&` or `||`.
        let doubled = chars.peek().is_some_and(|&(_, next)| next == c);
        // A `&` right after `<` or `>` belongs to the redirection.
        let handle = c == '&' && redirect;
        if !quoted && !handle && (c == '&' || (c == '|' && doubled)) {
            // No command before it, or a pipe or redirection still waiting.
            rejected |= !command_text || pipe_waits || redirect_waits;
            let mut end = i + 1;
            if doubled {
                chars.next();
                end += 1;
            }
            last_doubled = doubled;
            let blank_after = matches!(chars.peek(), Some(&(_, ' ' | '\t')));
            // A space or tab is one byte, so it starts one byte before the separator.
            let start = i - usize::from(blank);
            let after = i..end + usize::from(blank_after);
            push_part(
                &mut parts,
                value,
                span.take(),
                before.take(),
                Some(after),
                combined,
            );
            before = Some(start..end);
            combined = false;
            blank = false;
            redirect = false;
            pipe_waits = false;
            redirect_waits = false;
            command_text = false;
            continue;
        }
        let operator = !quoted && matches!(c, '|' | '<' | '>');
        let mut end = i + c.len_utf8();
        let text = match c {
            '"' => {
                quoted = !quoted;
                command_text = true;
                true
            }
            '^' if !quoted => {
                if let Some((j, escaped)) = chars.next() {
                    end = j + escaped.len_utf8();
                }
                command_text = true;
                true
            }
            '|' if !quoted => {
                // A pipe needs a command on both sides.
                rejected |= !command_text || pipe_waits || redirect_waits;
                pipe_waits = true;
                combined = true;
                command_text = false;
                true
            }
            '<' | '>' if !quoted => {
                redirect_waits = true;
                command_text = true;
                true
            }
            '(' if !quoted => {
                // A block: a command starts inside it.
                depth += 1;
                combined = true;
                command_text = false;
                true
            }
            ')' if !quoted => {
                // The end of a block, whose last command must be whole; a `)` that closes
                // nothing is text.
                if depth > 0 {
                    rejected |= !command_text || pipe_waits || redirect_waits;
                    depth -= 1;
                }
                combined = true;
                command_text = true;
                true
            }
            _ => {
                let text = quoted || !c.is_whitespace();
                command_text |= text;
                text
            }
        };
        if text && !operator && !handle {
            pipe_waits = false;
            redirect_waits = false;
        }
        redirect = operator && c != '|';
        blank = !text && matches!(c, ' ' | '\t');
        if text {
            span = Some(span.map_or(i..end, |s| s.start..end));
        }
    }
    // `&&` or `||` last, with nothing after it; a single `&` last is fine.
    rejected |= !command_text && before.is_some() && last_doubled;
    rejected |= pipe_waits || redirect_waits || depth > 0;
    // An unclosed quote runs the last command into whatever the quote holds.
    push_part(&mut parts, value, span, before, None, combined || quoted);
    Scan { parts, rejected }
}

/// The commands of an AutoRun value (see [`scan`]).
fn parts(value: &str) -> Vec<Part> {
    scan(value).parts
}

/// Adds the command at `span` to `parts`, when there is one.
fn push_part(
    parts: &mut Vec<Part>,
    value: &str,
    span: Option<Range<usize>>,
    before: Option<Range<usize>>,
    after: Option<Range<usize>>,
    combined: bool,
) {
    if let Some(span) = span {
        // `scan` puts both ends of `span` on character boundaries, so `get` finds the text.
        let text = value.get(span.clone()).unwrap_or_default().to_string();
        parts.push(Part {
            text,
            span,
            before,
            after,
            combined,
        });
    }
}

/// `value` without the bytes in `cut`.
fn without(value: &str, cut: &Range<usize>) -> String {
    value
        .char_indices()
        .filter(|(i, _)| !cut.contains(i))
        .map(|(_, c)| c)
        .collect()
}

/// Takes every command that `pick` chooses out of `value`, each as [`Part::cut`] says, and
/// says whether it took any. One at a time: what is left is read again before the next one
/// goes, so two commands side by side each take their own separator.
fn take_out(value: &mut String, pick: impl Fn(&str) -> bool) -> bool {
    let mut took = false;
    while let Some(part) = parts(value).into_iter().find(|p| pick(&p.text)) {
        *value = without(value, &part.cut());
        took = true;
    }
    took
}

/// Whether `command` is one of our AutoRun entries (spec §5.4):
/// `(if exist "<x>" "<x>" inject --autorun --quiet --profile "<profile>")`, or the same
/// without the parentheses, as builds before them wrote it. `<x>` is any path, the same both
/// times; the profile path is ours. Paths and keywords may be in any case, words may be apart
/// by any spaces or tabs, and a leading `@` is allowed.
pub(crate) fn is_our_entry(command: &str, profile: &str) -> bool {
    let command = command.strip_prefix('@').unwrap_or(command);
    let body = match command.strip_prefix('(') {
        Some(inner) => match inner.strip_suffix(')') {
            Some(body) => body,
            None => return false,
        },
        None => command,
    };
    let same = |a: &str, b: &str| a.to_lowercase() == b.to_lowercase();
    match words(body).as_deref() {
        Some(
            [Word::Plain(w_if), Word::Plain(w_exist), Word::Quoted(launch), Word::Quoted(again), Word::Plain(w_inject), Word::Plain(w_autorun), Word::Plain(w_quiet), Word::Plain(w_profile), Word::Quoted(p)],
        ) => {
            w_if.eq_ignore_ascii_case("if")
                && w_exist.eq_ignore_ascii_case("exist")
                && !launch.is_empty()
                && same(launch, again)
                && w_inject.eq_ignore_ascii_case("inject")
                && w_autorun.eq_ignore_ascii_case("--autorun")
                && w_quiet.eq_ignore_ascii_case("--quiet")
                && w_profile.eq_ignore_ascii_case("--profile")
                && same(p, profile)
        }
        _ => false,
    }
}

/// A word of a command: plain, or in double quotes (given without them).
enum Word<'a> {
    Plain(&'a str),
    Quoted(&'a str),
}

/// The words of `text`, apart by spaces and tabs; a word in double quotes is one word. `None`
/// when a quote is never closed, or a word is part quoted and part plain.
fn words(text: &str) -> Option<Vec<Word<'_>>> {
    let blank = [' ', '\t'];
    let mut words = Vec::new();
    let mut rest = text.trim_start_matches(blank);
    while !rest.is_empty() {
        let (word, after) = match rest.strip_prefix('"') {
            Some(quoted) => {
                let (inner, after) = quoted.split_once('"')?;
                (Word::Quoted(inner), after)
            }
            None => {
                let end = rest.find([' ', '\t', '"']).unwrap_or(rest.len());
                let (plain, after) = rest.split_at_checked(end)?;
                (Word::Plain(plain), after)
            }
        };
        if !(after.is_empty() || after.starts_with(blank)) {
            return None;
        }
        words.push(word);
        rest = after.trim_start_matches(blank);
    }
    Some(words)
}

/// What an edit did to an AutoRun value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AutoRunEdit {
    /// The new value. It may be empty, or nothing but whitespace.
    pub(crate) value: String,
    /// One or more entries of ours were taken out.
    pub(crate) removed_ours: bool,
    /// Commands of another Clink were taken out, as the user agreed.
    pub(crate) removed_foreign: bool,
}

impl AutoRunEdit {
    fn unchanged(value: &str) -> Self {
        AutoRunEdit {
            value: value.to_string(),
            removed_ours: false,
            removed_foreign: false,
        }
    }
}

/// Whether `text` holds our profile path, in any case: every form of our entry holds it.
fn holds_ours(text: &str, profile: &str) -> bool {
    text.to_lowercase().contains(&profile.to_lowercase())
}

/// The file names that start Clink: its launcher script, its program, and `clink` alone.
const CLINK_LAUNCHERS: [&str; 7] = [
    "clink",
    "clink.bat",
    "clink.cmd",
    "clink.exe",
    "clink_x64.exe",
    "clink_x86.exe",
    "clink_arm64.exe",
];

/// Whether `command` injects Clink into cmd: a word that names one of [`CLINK_LAUNCHERS`],
/// with or without a folder, quoted or not, whose verb (see [`clink_verb`]) is `inject`, both
/// in any case. Only `inject` loads Clink into cmd, so a call of Clink with another verb
/// (`clink.bat set clink.logo none`) is not an injection, nor is a command where `inject` is
/// not Clink's verb (`type "C:\clink\clink.bat" | find "inject"`), and a command that only
/// mentions Clink, such as `doskey /macrofile="C:\tools\clink\macros.txt"` or
/// `cd /d C:\clink-notes`, is another program's own. The AutoRun editor (another Clink, asked
/// about and taken out with the user's agreement), the warning about Clink in HKLM and the
/// doctor (D5) all read commands this way.
pub(crate) fn runs_clink(command: &str) -> bool {
    let words = command_words(command);
    words.iter().enumerate().any(|(i, word)| {
        let name = word.rsplit(['\\', '/']).next().unwrap_or(word);
        CLINK_LAUNCHERS.iter().any(|l| name.eq_ignore_ascii_case(l))
            && clink_verb(words.get(i + 1..).unwrap_or_default())
                .is_some_and(|verb| verb.eq_ignore_ascii_case("inject"))
    })
}

/// The verb of a Clink command, given the words after the launcher: the first word that is not
/// one of Clink's options. `--profile` takes the word after it as its value (`clink
/// --profile <dir> set clink.logo`, as the verifier runs it); any other word that starts with
/// `-` is an option on its own. `None` when the words hold no verb.
fn clink_verb(after_launcher: &[String]) -> Option<&str> {
    let mut words = after_launcher.iter();
    while let Some(word) = words.next() {
        if word.eq_ignore_ascii_case("--profile") {
            words.next();
        } else if !word.starts_with('-') {
            return Some(word);
        }
    }
    None
}

/// The words of a command as cmd splits them: at spaces and tabs, and at `<`, `>`, `|`, `(`
/// and `)`, none of them quoted or escaped. Quotes group a word and are not part of it, a `^`
/// makes the character after it plain, and the `@` that may start a command is dropped.
fn command_words(command: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quoted = false;
    let mut chars = command.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => quoted = !quoted,
            '^' if !quoted => word.extend(chars.next()),
            ' ' | '\t' | '<' | '>' | '|' | '(' | ')' if !quoted => {
                if !word.is_empty() {
                    words.push(std::mem::take(&mut word));
                }
            }
            '@' if !quoted && word.is_empty() && words.is_empty() => {}
            c => word.push(c),
        }
    }
    if !word.is_empty() {
        words.push(word);
    }
    words
}

/// Whether `text` runs another Clink: it injects Clink (see [`runs_clink`]) and is not ours.
fn is_foreign(text: &str, profile: &str) -> bool {
    !holds_ours(text, profile) && runs_clink(text)
}

fn hand_edit(part: &str) -> PlanError {
    PlanError::AutoRunNeedsHandEdit {
        part: part.to_string(),
    }
}

/// Refuses a value that can't be edited safely, with `E_AUTORUN_WRITE`: one that cmd itself
/// rejects (see [`Scan::rejected`]), and one where a command holds our profile path without
/// being one of our entries (see [`is_our_entry`]), so taking it out would take other text
/// with it.
fn check(read: &Scan, current: &str, profile: &str) -> Result<(), PlanError> {
    if read.rejected {
        return Err(hand_edit(current));
    }
    match read
        .parts
        .iter()
        .find(|p| holds_ours(&p.text, profile) && !is_our_entry(&p.text, profile))
    {
        Some(p) => Err(hand_edit(&p.text)),
        None => Ok(()),
    }
}

/// Takes our entries, old and current, out of `current` and leaves everything else as it was,
/// another Clink included: the first step of [`edit_autorun`], and the AutoRun step of the
/// uninstaller's cleanup (spec §6.3). A value with none of ours comes back as it is; one with
/// ours fails with `E_AUTORUN_WRITE` when it can't be edited safely (see `check`).
pub(crate) fn remove_ours(current: &str, profile: &str) -> Result<AutoRunEdit, PlanError> {
    let read = scan(current);
    if !read.parts.iter().any(|p| holds_ours(&p.text, profile)) {
        return Ok(AutoRunEdit::unchanged(current));
    }
    check(&read, current, profile)?;
    let mut value = current.to_string();
    let removed_ours = take_out(&mut value, |t| holds_ours(t, profile));
    Ok(AutoRunEdit {
        value,
        removed_ours,
        removed_foreign: false,
    })
}

/// The HKCU AutoRun value after the plan (spec §5.4), edited where it stands so that every
/// byte of other programs' commands stays as it was (spec §7, doctrine guard (ค)):
/// 1. A value that cmd itself rejects, or where a command holds our profile path without
///    being one of our entries, is not edited: the plan fails with `E_AUTORUN_WRITE`, and the
///    user edits AutoRun by hand (see `check`).
/// 2. Another command that injects Clink (see [`runs_clink`]) fails the plan with
///    `E_AUTORUN_FOREIGN_CLINK`, naming every such command, unless the user agreed to turn
///    them off; one of them that is combined with others (see [`Part::combined`]) fails it
///    with `E_AUTORUN_WRITE`. A command that only mentions Clink is neither asked about nor
///    taken out.
/// 3. Our entries go, old and current, and with the user's agreement the other Clink's.
/// 4. For `Target::All` our entry goes in front, as `<entry> & <value>`, or is the whole value
///    when nothing else is left. In front and in parentheses, it is never part of another
///    command's `if` or `for`, and its own `if` takes in nothing after it.
///
/// With `Target::Wt` and nothing to take out, the value stays as it is, whatever it holds.
pub(crate) fn edit_autorun(
    current: &str,
    target: Target,
    entry: &str,
    profile: &str,
    replace_foreign: bool,
) -> Result<AutoRunEdit, PlanError> {
    let read = scan(current);
    let foreign: Vec<&Part> = read
        .parts
        .iter()
        .filter(|p| is_foreign(&p.text, profile))
        .collect();
    let has_ours = read.parts.iter().any(|p| holds_ours(&p.text, profile));
    if target == Target::Wt && !has_ours && foreign.is_empty() {
        return Ok(AutoRunEdit::unchanged(current));
    }
    check(&read, current, profile)?;
    if !foreign.is_empty() && !replace_foreign {
        return Err(PlanError::ForeignClink {
            existing: foreign.iter().map(|p| p.text.clone()).collect(),
        });
    }
    if let Some(p) = foreign.iter().find(|p| p.combined) {
        return Err(hand_edit(&p.text));
    }
    let mut edit = remove_ours(current, profile)?;
    edit.removed_foreign = take_out(&mut edit.value, |t| is_foreign(t, profile));
    if target == Target::All {
        edit.value = if edit.value.trim().is_empty() {
            entry.to_string()
        } else {
            format!("{entry} & {}", edit.value)
        };
    }
    Ok(edit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::{autorun_entry, ClinkLaunch, Paths};
    use proptest::prelude::*;
    use proptest::test_runner::{Config, RngAlgorithm, TestCaseError, TestRng, TestRunner};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    // The Thai user name and the 8.3 names that VM-C gave
    // (docs/spikes/G4-G5-G16-G17-clink.md, G19).
    const DATA: &str = r"C:\Users\ผู้ทดสอบ ไทย\AppData\Local\ai.resincore.cmdstudio";
    const SHORT_EXE: &str =
        r"C:\Users\3607~1\AppData\Local\AIRESI~1.CMD\runtime\clink-1.9.34\clink_x64.exe";

    fn profile() -> String {
        format!(r"{DATA}\clink-profile")
    }

    /// Our entry, as `autorun_entry` writes it.
    fn entry() -> String {
        let paths = Paths {
            data: PathBuf::from(DATA),
        };
        autorun_entry(&ClinkLaunch::ShortPath(PathBuf::from(SHORT_EXE)), &paths).unwrap()
    }

    /// Our entry as an earlier build wrote it: no parentheses, and Clink 1.9.0 from its real
    /// path.
    fn old_entry() -> String {
        format!(
            r#"if exist "{DATA}\runtime\clink-1.9.0\clink_x64.exe" "{DATA}\runtime\clink-1.9.0\clink_x64.exe" inject --autorun --quiet --profile "{DATA}\clink-profile""#
        )
    }

    /// `current` edited for `target`, without leave to turn off another Clink.
    fn try_edit(current: &str, target: Target) -> Result<String, PlanError> {
        edit_autorun(current, target, &entry(), &profile(), false).map(|e| e.value)
    }

    fn edit(current: &str, target: Target) -> String {
        try_edit(current, target).unwrap()
    }

    fn refusal(current: &str, target: Target, replace_foreign: bool) -> PlanError {
        edit_autorun(current, target, &entry(), &profile(), replace_foreign).unwrap_err()
    }

    /// The commands `scan` finds in `value`.
    fn texts(value: &str) -> Vec<String> {
        scan(value).parts.into_iter().map(|p| p.text).collect()
    }

    #[test]
    fn parts_follow_cmds_quotes_carets_and_redirections() {
        assert_eq!(texts("a & b && c"), ["a", "b", "c"]);
        assert_eq!(texts("a&b&&c"), ["a", "b", "c"]);
        assert_eq!(texts(r#""x & y" & z"#), [r#""x & y""#, "z"]);
        assert_eq!(texts("echo a ^& b"), ["echo a ^& b"]);
        assert_eq!(texts(r#"echo ^"a & b"#), [r#"echo ^"a"#, "b"]);
        // Inside quotes a caret is plain text, so the quote after it still closes.
        assert_eq!(texts(r#""a^" & b"#), [r#""a^""#, "b"]);
        assert_eq!(texts(" \ta  b \t& b"), ["a  b", "b"]);
        assert!(texts("").is_empty());
        assert!(texts(" \t ").is_empty());
        // `||` separates commands like `&&`; a single `|` is a pipe inside one command.
        assert_eq!(texts("a||b || c"), ["a", "b", "c"]);
        assert_eq!(texts("a | b & c"), ["a | b", "c"]);
        assert_eq!(texts("a ^|| b"), ["a ^|| b"]);
        // A `&` right after `<` or `>` names a handle (`2>&1`) and separates nothing.
        assert_eq!(texts("x >nul 2>&1"), ["x >nul 2>&1"]);
        assert_eq!(texts("x 1>&2 & echo y"), ["x 1>&2", "echo y"]);
        assert_eq!(texts("x <&0 & y"), ["x <&0", "y"]);
        // A quoted or escaped `>` is text, so the `&` after it separates.
        assert_eq!(texts(r#"echo ">"& y"#), [r#"echo ">""#, "y"]);
        assert_eq!(texts("echo ^>& y"), ["echo ^>", "y"]);
    }

    #[test]
    fn scan_rejects_what_cmd_rejects() {
        // Each of these fails in `cmd.exe /d /s /c "<value>"`, so nothing in it runs, an entry
        // in front of it included (Windows 11, 2026-10-11).
        for v in [
            "echo A & & echo B",
            "& echo A",
            "&& echo A",
            "|| echo A",
            "echo A &&",
            "echo A && ",
            "echo A ||",
            "(echo A",
            "echo A |",
            "echo A | ",
            "echo A | & echo B",
            "echo A | && echo B",
            "| echo A",
            r#"echo A & | find "A""#,
            "echo A >",
            "echo A > & echo B",
            "echo A 2>& & echo B",
            "(&echo a)",
            "( & echo a)",
            "(&&echo a)",
            "(|echo a)",
            "(echo a &)",
            "(echo a & )",
            "(echo a &&)",
            "(echo a ||)",
            "(echo a |)",
            "()",
            "( )",
        ] {
            assert!(scan(v).rejected, "{v}");
        }
        // These run. A single `&` last is fine, and so is a `)` that closes nothing.
        for v in [
            "",
            "  ",
            "echo A &",
            "echo A & ",
            "echo A ^",
            r#"echo "A"#,
            "echo A ^& B",
            r#"echo A | find "A""#,
            "(echo A) & echo B",
            "echo (A) & echo B",
            "echo A)",
            "echo A) & echo B",
            "echo x >nul 2>&1 & echo y",
            "echo x 1>&2 & echo y",
            "a&&b",
            "a||b",
            "(echo a) & (echo b)",
            "echo a & (echo b)",
        ] {
            assert!(!scan(v).rejected, "{v}");
        }
    }

    #[test]
    fn our_entry_goes_first_and_comes_out_exactly() {
        // Doctrine guard (ค), spec §7: the AutoRun writer never removes or changes a command
        // that is not ours. Our entry goes in front, in parentheses, so it is never part of
        // another command's `if` or `for` and nothing of theirs is part of ours. Applying a
        // theme to every cmd window and then to Windows Terminal only gives back what other
        // programs wrote, byte for byte.
        let foreign = [
            r#""C:\ProgramData\anaconda3\condabin\conda_hook.bat""#,
            r#"if exist "C:\Users\me\miniconda3\condabin\conda_hook.bat" "C:\Users\me\miniconda3\condabin\conda_hook.bat""#,
            r"if exist C:\nope echo A & echo C",
            "for %i in (1 2) do echo %i",
            "echo a ^& b",
            r#"echo ^"a"#,
            r#""C:\a^"&b"#,
            "a&&b",
            "a && b",
            "a&b",
            "a\t&\tb",
            r#""C:\a&b\x.bat""#,
            "@chcp 65001>nul",
            "x >nul 2>&1 & echo y",
            r#"doskey /macrofile="C:\Users\me\macros.txt" & cls"#,
            "prompt $p$g",
            "cls  ",
            "  cls",
            "x||y",
            "x || y",
            "a | b",
            "(echo a & echo b)",
            r#"if exist "C:\x.bat" (call "C:\x.bat")"#,
            "echo a)",
            r#""C:\x.bat"#,
            r#"call "C:\my tools\x.bat"#,
            "a &",
            "echo hi ^",
            r#"echo สวัสดี & cd /d "D:\งาน""#,
        ];
        let ours = entry();
        for v in foreign {
            let all = format!("{ours} & {v}");
            assert_eq!(edit(v, Target::All), all, "{v}");
            assert_eq!(edit(&all, Target::All), all, "again: {v}");
            assert_eq!(edit(&all, Target::Wt), v, "{v}");
            assert_eq!(edit(v, Target::Wt), v, "{v}");
        }
        assert_eq!(edit("", Target::All), ours);
        assert_eq!(edit(" \t", Target::All), ours);
    }

    #[test]
    fn our_older_entries_give_way_to_the_current_one() {
        let (old, new) = (old_entry(), entry());
        for (current, next) in [
            (format!("{old} & y"), format!("{new} & y")),
            (format!("x & {old} & y"), format!("{new} & x & y")),
            (format!("x & {old}"), format!("{new} & x")),
            (format!("x&&{old}&y"), format!("{new} & x&y")),
            (old.clone(), new.clone()),
            (format!("{new} & x"), format!("{new} & x")),
        ] {
            assert_eq!(edit(&current, Target::All), next, "{current}");
        }
    }

    #[test]
    fn wt_target_takes_our_entry_out_wherever_it_stands() {
        let (ours, old) = (entry(), old_entry());
        for (current, next) in [
            (format!("x & {ours} & y"), "x & y"),
            (format!("{ours} & y"), "y"),
            (format!("x & {ours}"), "x"),
            (format!("x && {ours}"), "x"),
            (format!("x || {ours}"), "x"),
            (format!("{ours} || y"), "y"),
            (format!("x||{ours}||y"), "x||y"),
            (format!("x & {old}"), "x"),
            (format!("{old} & {ours} & y"), "y"),
        ] {
            assert_eq!(edit(&current, Target::Wt), next, "{current}");
        }
    }

    #[test]
    fn quotes_and_carets_keep_other_commands_whole() {
        let ours = entry();
        // An `&` inside quotes does not split a command.
        assert_eq!(
            edit(&format!(r#""C:\a&b\x.bat" & {ours}"#), Target::Wt),
            r#""C:\a&b\x.bat""#
        );
        // Were `^"` to open a quote, the `&` after it would be quoted and ours would be one
        // command with `echo`.
        assert_eq!(
            edit(&format!(r#"echo ^"a & {ours}"#), Target::Wt),
            r#"echo ^"a"#
        );
        // cmd reads `^&` as a plain `&`, so `echo a ^& b` is one command, kept as written.
        assert_eq!(
            edit("echo a ^& b", Target::All),
            format!("{ours} & echo a ^& b")
        );
        for v in ["a&&b", "a&b", "a  &   b"] {
            assert_eq!(edit(v, Target::All), format!("{ours} & {v}"), "{v}");
        }
    }

    #[test]
    fn a_redirection_keeps_its_ampersand() {
        // `2>&1` sends errors where the output goes; its `&` separates nothing. Taking out the
        // other Clink takes all of its command and nothing else.
        let current = r#"echo a & "C:\clink\clink.bat" inject --autorun >nul 2>&1 & echo b"#;
        let e = edit_autorun(current, Target::All, &entry(), &profile(), true).unwrap();
        assert_eq!(e.value, format!("{} & echo a & echo b", entry()));
        assert!(e.removed_foreign && !e.removed_ours);
        let e = edit_autorun(current, Target::Wt, &entry(), &profile(), true).unwrap();
        assert_eq!(e.value, "echo a & echo b");
    }

    #[test]
    fn only_our_exact_entry_counts_as_ours() {
        let p = profile();
        // Any launch path and Clink version, keywords in any case, more spaces or tabs, a
        // leading `@`, the profile path in any case, with or without the parentheses.
        for ours in [
            entry(),
            old_entry(),
            format!(
                r#"@(if exist "C:\x\clink_x64.exe" "C:\x\clink_x64.exe" inject --autorun --quiet --profile "{p}")"#
            ),
            format!(
                "( IF  EXIST \"C:\\X.EXE\"\t\"c:\\x.exe\" INJECT --AutoRun --Quiet --PROFILE \"{}\" )",
                p.to_uppercase()
            ),
            format!(
                r#"@if exist "C:\x.exe" "C:\x.exe" inject --autorun --quiet --profile "{p}""#
            ),
        ] {
            assert!(is_our_entry(&ours, &p), "{ours}");
            assert_eq!(edit(&format!("x & {ours}"), Target::Wt), "x", "{ours}");
        }
        // Any other command that holds our profile path is left for a hand edit: taking it
        // out would take other text with it.
        let entry = entry();
        for (current, part) in [
            // A quote that opens before ours runs past it: one `echo`, as cmd reads it.
            (format!(r#"echo "a & {entry} & echo b""#), None),
            (format!("echo ^& {entry}"), None),
            (format!("call {entry}"), None),
            (format!("{entry} extra"), None),
            (format!("x | {entry}"), None),
            (format!("{entry} | more"), None),
            (format!("if defined X ({entry})"), None),
            (format!(r#""C:\y.bat & {entry}"#), None),
            (format!("(a & {entry})"), Some(format!("{entry})"))),
            (
                format!(
                    r#"if exist "C:\a.exe" "C:\b.exe" inject --autorun --quiet --profile "{p}""#
                ),
                None,
            ),
            (
                format!(r#"if exist "C:\a.exe" "C:\a.exe" inject --autorun --profile "{p}""#),
                None,
            ),
            (
                format!(
                    r#"if exist "C:\a.exe" "C:\a.exe" inject --autorun --quiet --profile "{p}\x""#
                ),
                None,
            ),
        ] {
            let part = part.unwrap_or_else(|| current.clone());
            for target in [Target::All, Target::Wt] {
                let err = refusal(&current, target, false);
                assert_eq!(err, hand_edit(&part), "{current}");
                assert_eq!(err.code(), "E_AUTORUN_WRITE");
            }
        }
    }

    #[test]
    fn every_command_of_another_clink_is_named_before_any_goes() {
        let doskey = r#"doskey /macrofile="C:\tools\clink\macros.txt""#;
        let clink = r#""C:\clink\clink.bat" inject --autorun"#;
        let x64 = r"C:\clink\clink_x64.exe inject --quiet";
        let current = format!("{doskey} & {clink} & echo hi & {x64}");
        for target in [Target::All, Target::Wt] {
            assert_eq!(
                refusal(&current, target, false),
                PlanError::ForeignClink {
                    existing: vec![clink.to_string(), x64.to_string()]
                }
            );
        }
        // The doskey line only names a folder called clink: it is not asked about, and stays.
        let e = edit_autorun(&current, Target::All, &entry(), &profile(), true).unwrap();
        assert_eq!(e.value, format!("{} & {doskey} & echo hi", entry()));
        assert!(e.removed_foreign && !e.removed_ours);
    }

    #[test]
    fn only_a_command_that_injects_clink_is_another_clink() {
        // A launcher of Clink as a whole word, with or without a folder, quoted or not, and
        // `inject` as its verb, in any case. Clink takes `--profile <dir>` before its verb.
        for command in [
            "clink inject",
            r#""C:\x\clink.bat" inject --autorun"#,
            "clink_x64.exe inject",
            r#""C:\Program Files (x86)\clink\clink.bat" inject --autorun"#,
            r"C:\c\clink.exe inject",
            r#""C:\c\clink_arm64.exe" inject --profile "C:\p""#,
            "clink_x86.exe inject",
            "CLINK.CMD INJECT",
            "@clink inject",
            r#"call "C:\c\clink.bat" inject"#,
            r#"if exist "C:\c\clink.bat" ("C:\c\clink.bat" inject)"#,
            "x | clink inject",
            "clink inject>nul",
            r#""C:\clink\clink.bat" inject --autorun >nul 2>&1"#,
            r#""C:\clink\clink.bat" --profile "D:\my profile" inject --autorun"#,
            "clink --PROFILE p INJECT",
        ] {
            assert!(runs_clink(command), "{command}");
        }
        // Commands that only mention Clink are other programs' own; so is a call of Clink that
        // does not inject it into cmd, and one where `inject` is not Clink's verb.
        for command in [
            r#"doskey /macrofile="C:\tools\clink\macros.txt""#,
            r"cd /d C:\clink-notes",
            r#""C:\clink\clink.bat" set clink.logo none"#,
            "clink.bat set clink.logo",
            "notclink.bat inject",
            r"C:\clink\clink.bat.old inject",
            "inject clink.bat",
            "echo clinker inject",
            // Inside an unclosed quote cmd runs no Clink: the quote holds the whole rest.
            r#""C:\x.bat & clink inject"#,
            r#"type "C:\clink\clink.bat" | find "inject""#,
            "clink.bat set inject",
            "clink --profile inject",
            "clink",
        ] {
            assert!(!runs_clink(command), "{command}");
        }
    }

    #[test]
    fn a_command_that_only_mentions_clink_stays() {
        // Doctrine guard (ค): only another Clink's injection may go, and only with the user's
        // agreement. A command that merely names a folder or file called clink is not asked
        // about and is never taken out.
        for v in [
            r#"doskey /macrofile="C:\tools\clink\macros.txt""#,
            r"cd /d C:\clink-notes",
            r#""C:\clink\clink.bat" set clink.logo none"#,
            r#""C:\x.bat & clink inject"#,
        ] {
            for replace_foreign in [false, true] {
                let e = edit_autorun(v, Target::All, &entry(), &profile(), replace_foreign);
                let e = e.unwrap();
                assert_eq!(e.value, format!("{} & {v}", entry()), "{v}");
                assert!(!e.removed_foreign, "{v}");
                let e = edit_autorun(v, Target::Wt, &entry(), &profile(), replace_foreign);
                assert_eq!(e.unwrap(), AutoRunEdit::unchanged(v), "{v}");
            }
        }
    }

    #[test]
    fn a_combined_foreign_clink_is_left_for_a_hand_edit() {
        for (current, part) in [
            ("x | clink inject", "x | clink inject"),
            ("clink inject | more", "clink inject | more"),
            ("(a & clink inject)", "clink inject)"),
            (
                r#"if exist "C:\c\clink.bat" ("C:\c\clink.bat" inject)"#,
                r#"if exist "C:\c\clink.bat" ("C:\c\clink.bat" inject)"#,
            ),
            (
                r#"x | "C:\c\clink_x64.exe" inject"#,
                r#"x | "C:\c\clink_x64.exe" inject"#,
            ),
        ] {
            for target in [Target::All, Target::Wt] {
                // Without the user's agreement, the question about the other Clink comes first.
                assert_eq!(
                    refusal(current, target, false),
                    PlanError::ForeignClink {
                        existing: vec![part.to_string()]
                    }
                );
                let err = refusal(current, target, true);
                assert_eq!(err, hand_edit(part), "{current}");
                assert_eq!(err.code(), "E_AUTORUN_WRITE");
            }
        }
    }

    #[test]
    fn a_foreign_clink_goes_without_touching_the_commands_around_it() {
        let clink = r#""C:\Program Files (x86)\clink\clink.bat" inject --autorun"#;
        let current = format!("x&&{clink} & echo a ^& b");
        let e = edit_autorun(&current, Target::All, &entry(), &profile(), true).unwrap();
        assert_eq!(e.value, format!("{} & x & echo a ^& b", entry()));
    }

    #[test]
    fn a_value_cmd_rejects_is_left_for_a_hand_edit() {
        let ours = entry();
        for current in [
            "echo A & & echo B".to_string(),
            "& echo A".to_string(),
            "echo A &&".to_string(),
            "echo A ||".to_string(),
            "(echo A".to_string(),
            "echo A |".to_string(),
            "echo A > & echo B".to_string(),
            // A pipe with nothing after it before ours: taking ours out would let it meet the
            // `||` after ours, and cmd would pair them.
            format!("x|&{ours}||y"),
            format!("{ours} & echo A & & echo B"),
        ] {
            assert_eq!(
                refusal(&current, Target::All, false),
                hand_edit(&current),
                "{current}"
            );
        }
        // With nothing to take out and nothing to add, Windows Terminal only leaves even such
        // a value as it is; with ours in it, it does not take it apart.
        assert_eq!(edit("echo A & & echo B", Target::Wt), "echo A & & echo B");
        let current = format!("{ours} & echo A & & echo B");
        assert_eq!(refusal(&current, Target::Wt, false), hand_edit(&current));
    }

    #[test]
    fn our_older_entry_is_found_whatever_the_case_of_its_profile_path() {
        // Windows reads `...\CLINK-PROFILE` and `...\clink-profile` as one folder.
        let p = profile();
        let old = old_entry().replace(&p, &p.to_uppercase());
        assert_ne!(old, old_entry());
        let current = format!("echo hi & {old}");
        assert_eq!(
            edit(&current, Target::All),
            format!("{} & echo hi", entry())
        );
        assert_eq!(edit(&current, Target::Wt), "echo hi");
    }

    #[test]
    fn parentheses_in_our_own_paths_are_quoted_and_do_not_count() {
        // A user folder may be named `John (Work)`; our entry quotes its paths.
        let paths = Paths {
            data: PathBuf::from(r"C:\Users\John (Work)\AppData\Local\ai.resincore.cmdstudio"),
        };
        let ours = autorun_entry(&ClinkLaunch::Direct(paths.clink_exe()), &paths).unwrap();
        let profile = paths.clink_profile().to_str().unwrap().to_string();
        assert!(!scan(&ours).rejected);
        let e = edit_autorun(&format!("x & {ours}"), Target::Wt, &ours, &profile, false);
        assert_eq!(e.unwrap().value, "x");
    }

    #[test]
    fn remove_ours_takes_out_our_entries_and_nothing_else() {
        // Uninstalling takes only our part out of AutoRun (spec §6.3), another Clink stays.
        let ours = entry();
        let current = format!(r#"{ours} & x & "C:\clink\clink.bat" inject"#);
        let e = remove_ours(&current, &profile()).unwrap();
        assert_eq!(e.value, r#"x & "C:\clink\clink.bat" inject"#);
        assert!(e.removed_ours && !e.removed_foreign);
        let e = remove_ours("x & y", &profile()).unwrap();
        assert_eq!(e.value, "x & y");
        assert!(!e.removed_ours);
        let bad = format!("echo ^& {ours}");
        assert_eq!(remove_ours(&bad, &profile()).unwrap_err(), hand_edit(&bad));
    }

    /// AutoRun values made of the characters that matter to how cmd reads a line, and a few
    /// that don't.
    fn values() -> impl Strategy<Value = String> {
        proptest::collection::vec(
            prop::sample::select(vec![
                'a', 'b', '1', ' ', '\t', '&', '|', '^', '"', '(', ')', '<', '>', 'ก', '\u{3000}',
            ]),
            0..20,
        )
        .prop_map(|chars| chars.into_iter().collect())
    }

    /// Runs `test` on `CASES` values that cmd accepts, drawn from a fixed seed so every run
    /// checks the same values, and checks that they all ran.
    fn for_accepted_values(name: &str, test: impl Fn(&str) -> Result<(), TestCaseError>) {
        const CASES: u32 = 4_000;
        let executed = AtomicU32::new(0);
        let config = Config {
            cases: CASES,
            max_global_rejects: 1_000_000,
            failure_persistence: None,
            ..Config::default()
        };
        let mut runner =
            TestRunner::new_with_rng(config, TestRng::deterministic_rng(RngAlgorithm::ChaCha));
        let result = runner.run(&values(), |v| {
            prop_assume!(!scan(&v).rejected);
            executed.fetch_add(1, Ordering::Relaxed);
            test(&v)
        });
        if let Err(e) = result {
            panic!("{name}: {e}\n{runner}");
        }
        let executed = executed.load(Ordering::Relaxed);
        assert!(executed >= CASES, "{name}: only {executed} of {CASES} ran");
    }

    #[test]
    fn property_taking_commands_out_keeps_the_rest() {
        // Doctrine guard (ค) beyond the fixed lists: taking commands out of a value cmd
        // accepts leaves every other command as cmd reads it, and a value cmd still accepts.
        // The commands taken out are of the kind the editor takes out: none holds a pipe or a
        // parenthesis, since another Clink that does is refused, and ours are whole blocks
        // (the round-trip property takes those out).
        let pick = |t: &str| t.contains('a') && !t.contains(['(', ')', '|']);
        for_accepted_values("keep the rest", |v| {
            let mut left = v.to_string();
            take_out(&mut left, pick);
            let want: Vec<String> = texts(v).into_iter().filter(|t| !pick(t)).collect();
            prop_assert_eq!(texts(&left), want, "{:?} left {:?}", v, left);
            prop_assert!(!scan(&left).rejected, "{:?} left {:?}", v, left);
            Ok(())
        });
    }

    #[test]
    fn property_our_entry_goes_first_and_comes_out_exactly() {
        // Doctrine guard (ค) beyond the fixed lists.
        let ours = entry();
        let edit =
            |v: &str, target| try_edit(v, target).map_err(|e| TestCaseError::fail(e.to_string()));
        for_accepted_values("round trip", |v| {
            let all = edit(v, Target::All)?;
            if v.trim().is_empty() {
                prop_assert_eq!(&all, &ours);
                return Ok(());
            }
            prop_assert_eq!(&all, &format!("{ours} & {v}"));
            prop_assert_eq!(&edit(&all, Target::All)?, &all, "again");
            prop_assert_eq!(edit(&all, Target::Wt)?, v);
            prop_assert_eq!(edit(v, Target::Wt)?, v);
            Ok(())
        });
    }
}
