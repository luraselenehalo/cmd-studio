//! The planner (spec §5.2 `plan`): theme + request + machine facts → the
//! operations to run and a plain-language summary (spec §4.2 page 5).
//! Pure: it reads nothing and writes nothing.
use crate::generate::{clink, lua, omp, wt};
use crate::model::{Target, Theme};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::ops::Range;
use std::path::{Path, PathBuf};

pub const OMP_VERSION: &str = "31.5.0";
pub const CLINK_VERSION: &str = "1.9.34";
/// GDI family name of the bundled Nerd Font: the name detect looks for, the
/// `font.face` written to Windows Terminal, and the fallback font
/// (spec §3.3, §5.4, G12).
pub const NERD_FONT_FAMILY: &str = "CaskaydiaCove NF";

/// Where CMD Studio keeps its data (spec §5.3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Paths {
    pub data: PathBuf,
}

impl Paths {
    pub fn generated(&self) -> PathBuf {
        self.data.join("generated")
    }
    pub fn clink_profile(&self) -> PathBuf {
        self.data.join("clink-profile")
    }
    pub fn omp_dir(&self) -> PathBuf {
        self.data.join("runtime").join(format!("omp-{OMP_VERSION}"))
    }
    pub fn clink_dir(&self) -> PathBuf {
        self.data
            .join("runtime")
            .join(format!("clink-{CLINK_VERSION}"))
    }
    pub fn omp_exe(&self) -> PathBuf {
        self.omp_dir().join("oh-my-posh.exe")
    }
    pub fn clink_exe(&self) -> PathBuf {
        self.clink_dir().join("clink_x64.exe")
    }
    pub fn omp_config(&self) -> PathBuf {
        self.generated().join("theme.omp.json")
    }
}

/// What the planner needs to know about the machine. Filled by `detect` (Phase 1b).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Machine {
    /// settings.json of every Windows Terminal variant found.
    pub wt_settings: Vec<PathBuf>,
    /// Current HKCU AutoRun value.
    pub autorun_hkcu: Option<String>,
    /// Current HKLM AutoRun value (read only; CMD Studio never writes HKLM).
    pub autorun_hklm: Option<String>,
    /// Family names of the installed fonts, as GDI lists them. Whether the
    /// fonts packaged inside Windows Terminal count is decided in Phase 1b.
    pub fonts: Vec<String>,
    /// `GetShortPathNameW` of `Paths::data`. On a volume that makes no 8.3
    /// names it is the long path itself (G19).
    pub data_short: Option<PathBuf>,
    /// `%ProgramData%\ai.resincore.cmdstudio-<SID>`, given only when it does
    /// not exist yet or passed the owner and permission check (spec §5.4).
    pub program_data_dir: Option<PathBuf>,
}

/// `<clink-launch>`: the path cmd uses to start Clink (spec §5.4). Clink
/// 1.9.34 cannot inject from a path with non-ASCII letters (G19), so every
/// variant holds an all-ASCII path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClinkLaunch {
    /// The real path under the data folder.
    Direct(PathBuf),
    /// The data folder's 8.3 path joined to the long ASCII rest.
    ShortPath(PathBuf),
    /// Clink installed in the user's own folder under `%ProgramData%`.
    ProgramData { dir: PathBuf },
}

impl ClinkLaunch {
    pub fn exe(&self) -> PathBuf {
        match self {
            ClinkLaunch::Direct(exe) | ClinkLaunch::ShortPath(exe) => exe.clone(),
            ClinkLaunch::ProgramData { dir } => dir.join("clink_x64.exe"),
        }
    }

    /// Where Clink's runtime is installed.
    pub fn runtime_dir(&self, paths: &Paths) -> PathBuf {
        match self {
            ClinkLaunch::ProgramData { dir } => dir.clone(),
            ClinkLaunch::Direct(_) | ClinkLaunch::ShortPath(_) => paths.clink_dir(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub target: Target,
    pub set_wt_default_profile: bool,
    pub lang: lua::Lang,
    /// The user agreed to turn off another Clink found in AutoRun (spec §5.4).
    pub replace_foreign_clink: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Component {
    Omp,
    Clink,
}

/// One operation of a plan. The journal keeps these as JSON (spec §5.5), with the kind of
/// operation in `kind` and every name in camelCase:
/// `{"kind": "wtApply", "settings": …, "scheme": …, "profile": …, "setDefault": true}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Op {
    InstallRuntime {
        component: Component,
        dir: PathBuf,
    },
    WriteFile {
        path: PathBuf,
        contents: String,
    },
    WtApply {
        settings: PathBuf,
        scheme: Value,
        profile: Map<String, Value>,
        set_default: bool,
    },
    /// The complete new HKCU AutoRun value; `None` deletes it.
    SetAutoRun {
        value: Option<String>,
    },
    InstallFont {
        family: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Warning {
    NoWindowsTerminal,
    MachineWideClink,
    /// The theme's font is not installed, so Windows Terminal gets
    /// `NERD_FONT_FAMILY` instead (spec §3.3).
    FontFallback {
        wanted: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    pub ops: Vec<Op>,
    pub summary_th: Vec<&'static str>,
    pub summary_en: Vec<&'static str>,
    pub warnings: Vec<Warning>,
    /// The `<clink-launch>` this plan uses. Phase 1b records it in `state.json`.
    pub launch: ClinkLaunch,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PlanError {
    #[error("another Clink runs from cmd's AutoRun: {existing}")]
    ForeignClink { existing: String },
    /// A path that would go into a cmd command or into Clink's settings holds `%`, `"` or
    /// `;`, or is not Unicode (spec §5.4).
    #[error("cmd or Clink can't take this path: it holds %, a double quote, ; or text that is not Unicode: {0:?}")]
    UnsupportedPath(PathBuf),
    #[error("Clink can't start from {0:?}, and no 8.3 or ProgramData path is available instead")]
    NoClinkPath(PathBuf),
    #[error("could not build the Oh My Posh config: {0}")]
    Generate(String),
    /// AutoRun holds a command that the plan would take out, ours or another Clink the user
    /// agreed to turn off, combined with other commands by a pipe, parentheses or an unclosed
    /// quote, so taking it out would change them too. `part` is that command. The user edits
    /// AutoRun by hand.
    #[error(
        "AutoRun holds a command combined with others that CMD Studio can't safely change: {part}"
    )]
    AutoRunNeedsHandEdit { part: String },
}

impl PlanError {
    /// Stable code shown to users (spec §5.9).
    pub fn code(&self) -> &'static str {
        match self {
            PlanError::AutoRunNeedsHandEdit { .. } => "E_AUTORUN_WRITE",
            PlanError::ForeignClink { .. } => "E_AUTORUN_FOREIGN_CLINK",
            PlanError::UnsupportedPath(_) | PlanError::NoClinkPath(_) => "E_CLINK_PATH",
            PlanError::Generate(_) => "E_THEME_INVALID",
        }
    }
}

fn is_ascii(p: &Path) -> bool {
    p.to_str().is_some_and(str::is_ascii)
}

/// Picks `<clink-launch>`: the first all-ASCII path in the order of spec §5.4.
pub fn clink_launch(paths: &Paths, machine: &Machine) -> Result<ClinkLaunch, PlanError> {
    let real = paths.clink_exe();
    if is_ascii(&real) {
        return Ok(ClinkLaunch::Direct(real));
    }
    if let Some(short) = &machine.data_short {
        let exe = Paths {
            data: short.clone(),
        }
        .clink_exe();
        if is_ascii(&exe) {
            return Ok(ClinkLaunch::ShortPath(exe));
        }
    }
    if let Some(base) = &machine.program_data_dir {
        let dir = base.join(format!("clink-{CLINK_VERSION}"));
        if is_ascii(&dir) {
            return Ok(ClinkLaunch::ProgramData { dir });
        }
    }
    Err(PlanError::NoClinkPath(real))
}

/// The text of a path that cmd or Clink will be given, or `UnsupportedPath` (spec §5.4).
/// Refused are:
/// - `%` and `"`: cmd would expand or split the path, and every cmd window would show an
///   error (G13);
/// - `;`: `clink.path` in clink_settings is a list split at `;`;
/// - a path that is not Unicode: the generated files would hold U+FFFD in its place.
fn checked(p: &Path) -> Result<&str, PlanError> {
    match p.to_str() {
        Some(text) if !text.contains(['%', '"', ';']) => Ok(text),
        _ => Err(PlanError::UnsupportedPath(p.to_path_buf())),
    }
}

/// Our AutoRun entry (spec §5.4). It refuses the paths that cmd or Clink can't take (see
/// `checked`): `<clink-launch>`, and the profile path. The profile path stands for every
/// path under `<data>` that the generated files hold (`generated`, `runtime\omp-<ver>`),
/// because what follows `<data>` in each of them is fixed ASCII.
pub fn autorun_entry(launch: &ClinkLaunch, paths: &Paths) -> Result<String, PlanError> {
    let (exe_path, profile_path) = (launch.exe(), paths.clink_profile());
    let exe = checked(&exe_path)?;
    let profile = checked(&profile_path)?;
    Ok(format!(
        r#"if exist "{exe}" "{exe}" inject --autorun --quiet --profile "{profile}""#
    ))
}

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
    combined: bool,
}

impl Part {
    /// The bytes that go when this command is taken out of the value: the command, one
    /// separator next to it with the whitespace between them, and one space or tab on the far
    /// side of that separator, where our own ` & ` puts one. The separator is the one before
    /// the command, or for the first command the one after it. Every other byte stays as it
    /// was, so taking out an entry that was added with ` & `, after the value or in front of
    /// it, gives back the value as it was before.
    ///
    /// One exception keeps the commands that are left as cmd read them: when taking out the
    /// separator before would put a `|` right before the `|` of a `||` after the command,
    /// cmd would pair the two, so the separator after the command goes instead.
    fn cut(&self, value: &str) -> Range<usize> {
        let bar_at = |i: Option<usize>| i.and_then(|i| value.as_bytes().get(i)) == Some(&b'|');
        match (&self.before, &self.after) {
            (Some(before), Some(after))
                if bar_at(before.start.checked_sub(1)) && bar_at(Some(self.span.end)) =>
            {
                self.span.start..after.end
            }
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
    /// The value ends cleanly: not inside quotes, not with a lone `^`, and not with a
    /// separator. Text added after a value that does not would be quoted, escaped, or read
    /// after an empty command.
    pub(crate) clean_end: bool,
}

/// Reads an AutoRun value the way cmd reads it: `"` starts and ends quoted text; outside
/// quotes `^` makes the character after it plain text, so `^&`, `^|` and `^"` are text; and
/// outside quotes `&`, `&&` and `||` separate commands, while a single `|` is a pipe inside
/// one command. Each command is trimmed of the whitespace around it (quoted or escaped
/// whitespace is text), and an empty command, as in `a & & b`, is no part.
pub(crate) fn scan(value: &str) -> Scan {
    let mut parts = Vec::new();
    let mut chars = value.char_indices().peekable();
    let mut quoted = false;
    // The command being read: where its text is, the separator before it, and whether it
    // holds a pipe or a parenthesis.
    let mut span: Option<Range<usize>> = None;
    let mut before = None;
    let mut combined = false;
    // Whether the last character read is a space or tab between words: not quoted, not
    // escaped.
    let mut blank = false;
    let mut lone_caret = false;
    while let Some((i, c)) = chars.next() {
        // `&&` or `||`.
        let doubled = chars.peek().is_some_and(|&(_, next)| next == c);
        if !quoted && (c == '&' || (c == '|' && doubled)) {
            let mut end = i + 1;
            if doubled {
                chars.next();
                end += 1;
            }
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
            continue;
        }
        let mut end = i + c.len_utf8();
        let text = match c {
            '"' => {
                quoted = !quoted;
                true
            }
            '^' if !quoted => {
                match chars.next() {
                    Some((j, escaped)) => end = j + escaped.len_utf8(),
                    None => lone_caret = true,
                }
                true
            }
            '|' | '(' | ')' if !quoted => {
                combined = true;
                true
            }
            _ => quoted || !c.is_whitespace(),
        };
        blank = !text && matches!(c, ' ' | '\t');
        if text {
            span = Some(span.map_or(i..end, |s| s.start..end));
        }
    }
    let clean_end = !quoted && !lone_caret && (span.is_some() || before.is_none());
    // An unclosed quote runs the last command into whatever the quote holds.
    push_part(&mut parts, value, span, before, None, combined || quoted);
    Scan { parts, clean_end }
}

/// The commands of an AutoRun value (see [`scan`]).
pub(crate) fn parts(value: &str) -> Vec<Part> {
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
        *value = without(value, &part.cut(value));
        took = true;
    }
    took
}

/// The HKCU AutoRun value after the plan, and whether another Clink was taken out of it
/// (spec §5.4). The value is edited where it stands, not taken apart and written again, so
/// every byte of other programs' commands stays as it was (spec §7, doctrine guard (ค)):
/// 1. Our entries go, old and current: they are the commands that hold `ours`, our profile
///    path, in any mix of upper and lower case, as Windows reads paths. A command of ours
///    that is combined with another one (a pipe, parentheses or an unclosed quote; see
///    [`Part`]) is not taken apart: the plan fails with `E_AUTORUN_WRITE`, and the user
///    edits AutoRun by hand.
/// 2. Another command that runs Clink fails the plan, unless the user agreed to turn it off
///    (`replace_foreign`); then it goes too. When it is combined with another command, it
///    is not taken apart either: the plan fails with `E_AUTORUN_WRITE`.
/// 3. For `Target::All` our entry is added with ` & `: after the value when the value ends
///    cleanly (see [`Scan::clean_end`]), and in front of it, as `<entry> & <value>`, when it
///    does not. When nothing else is left, our entry is the whole value.
///
/// Limit: text joined to a command that goes with no separator at all, as in
/// `echo ^& <ours>`, goes with it.
fn edit_autorun(
    current: &str,
    target: Target,
    entry: &str,
    ours: &str,
    replace_foreign: bool,
) -> Result<(String, bool), PlanError> {
    let ours = ours.to_lowercase();
    let is_ours = |p: &str| p.to_lowercase().contains(&ours);
    if let Some(part) = parts(current)
        .into_iter()
        .find(|p| p.combined && is_ours(&p.text))
    {
        return Err(PlanError::AutoRunNeedsHandEdit { part: part.text });
    }
    let mut value = current.to_string();
    take_out(&mut value, is_ours);
    let foreign = |p: &str| p.to_ascii_lowercase().contains("clink");
    if !replace_foreign {
        if let Some(existing) = parts(&value).into_iter().find(|p| foreign(&p.text)) {
            return Err(PlanError::ForeignClink {
                existing: existing.text,
            });
        }
    }
    if let Some(part) = parts(&value)
        .into_iter()
        .find(|p| p.combined && foreign(&p.text))
    {
        return Err(PlanError::AutoRunNeedsHandEdit { part: part.text });
    }
    let replaced = take_out(&mut value, foreign);
    if target == Target::All {
        value = if value.trim().is_empty() {
            entry.to_string()
        } else if scan(&value).clean_end {
            format!("{value} & {entry}")
        } else {
            format!("{entry} & {value}")
        };
    }
    Ok((value, replaced))
}

/// The installed family that matches `family`, ignoring ASCII case, in the
/// machine's own spelling.
fn installed_font<'a>(machine: &'a Machine, family: &str) -> Option<&'a str> {
    let family = family.trim();
    machine
        .fonts
        .iter()
        .map(|f| f.trim())
        .find(|f| f.eq_ignore_ascii_case(family))
}

/// The font face for Windows Terminal (spec §3.3): the theme's font when it is
/// installed, otherwise the bundled Nerd Font with a warning.
fn wt_font(theme: &Theme, machine: &Machine, warnings: &mut Vec<Warning>) -> String {
    let wanted = theme.terminal.font.trim();
    if let Some(installed) = installed_font(machine, wanted) {
        return installed.to_string();
    }
    if !wanted.eq_ignore_ascii_case(NERD_FONT_FAMILY) {
        warnings.push(Warning::FontFallback {
            wanted: wanted.to_string(),
        });
    }
    NERD_FONT_FAMILY.to_string()
}

/// Plans applying `theme` to `machine` (spec §5.2). It fails, before anything on the machine
/// is touched, with `E_CLINK_PATH` when cmd or Clink can't take a path (see
/// [`clink_launch`] and [`autorun_entry`]), with `E_AUTORUN_FOREIGN_CLINK` when another
/// Clink runs from AutoRun and the user has not agreed to turn it off, and with
/// `E_AUTORUN_WRITE` when a command it would take out of AutoRun, ours or that other Clink,
/// is combined with others in a way it can't take apart safely.
pub fn plan(
    theme: &Theme,
    req: &Request,
    machine: &Machine,
    paths: &Paths,
) -> Result<Plan, PlanError> {
    let launch = clink_launch(paths, machine)?;
    // Built for both targets: it also checks the paths that the WT-only
    // target's `/k` line uses.
    let entry = autorun_entry(&launch, paths)?;
    let omp_text = omp::config_text(theme).map_err(|e| PlanError::Generate(e.to_string()))?;
    let mut ops = vec![
        Op::InstallRuntime {
            component: Component::Omp,
            dir: paths.omp_dir(),
        },
        Op::InstallRuntime {
            component: Component::Clink,
            dir: launch.runtime_dir(paths),
        },
        Op::WriteFile {
            path: paths.omp_config(),
            contents: omp_text,
        },
        Op::WriteFile {
            path: paths.generated().join("init.lua"),
            contents: lua::init_lua(&paths.omp_exe(), &paths.omp_config()),
        },
        Op::WriteFile {
            path: paths.generated().join("banner.lua"),
            contents: lua::banner_lua(theme, req.lang),
        },
        Op::WriteFile {
            path: paths.clink_profile().join("clink_settings"),
            contents: clink::settings(theme.prompt.transient, &paths.generated()),
        },
    ];
    let mut th = vec!["ปรับ prompt และกล่องต้อนรับตามธีม"];
    let mut en = vec!["Update the prompt and the welcome box"];
    let mut warnings = Vec::new();

    if matches!(launch, ClinkLaunch::ProgramData { .. }) {
        th.push("เก็บส่วนเสริมไว้นอกโฟลเดอร์ผู้ใช้ เพราะชื่อโฟลเดอร์ผู้ใช้มีอักษรที่ส่วนเสริมใช้ไม่ได้");
        en.push("Keep the add-on outside your user folder, because your user folder's name has letters it can't use");
    }

    if machine.wt_settings.is_empty() {
        warnings.push(Warning::NoWindowsTerminal);
    } else {
        let font = wt_font(theme, machine, &mut warnings);
        if font == NERD_FONT_FAMILY && installed_font(machine, NERD_FONT_FAMILY).is_none() {
            ops.push(Op::InstallFont {
                family: NERD_FONT_FAMILY.to_string(),
            });
            th.push("ติดตั้งฟอนต์ 1 ตัว (เฉพาะบัญชีนี้)");
            en.push("Install 1 font for this account only");
        }
        let mut wt_theme = theme.clone();
        wt_theme.terminal.font = font;
        let scheme = wt::scheme(&wt_theme);
        let profile =
            wt::profile_values(&wt_theme, req.target, &launch.exe(), &paths.clink_profile());
        for settings in &machine.wt_settings {
            ops.push(Op::WtApply {
                settings: settings.clone(),
                scheme: scheme.clone(),
                profile: profile.clone(),
                set_default: req.set_wt_default_profile,
            });
        }
        th.push("สีและตัวอักษรของหน้าต่าง cmd ใน Windows Terminal");
        en.push("Colors and font of cmd in Windows Terminal");
        if req.set_wt_default_profile {
            th.push("เปิด Windows Terminal มาเป็น cmd");
            en.push("Open Windows Terminal with cmd");
        }
    }

    if machine
        .autorun_hklm
        .as_deref()
        .is_some_and(|v| v.to_ascii_lowercase().contains("clink"))
    {
        warnings.push(Warning::MachineWideClink);
    }

    let current = machine.autorun_hkcu.as_deref().unwrap_or_default();
    // `autorun_entry` has refused a profile path cmd can't take; this gets its text.
    let profile = paths.clink_profile();
    let (next, replaced) = edit_autorun(
        current,
        req.target,
        &entry,
        checked(&profile)?,
        req.replace_foreign_clink,
    )?;
    if replaced {
        th.push("ปิดส่วนเสริมของ cmd ตัวเดิมที่ติดตั้งไว้ก่อน (ย้อนกลับได้)");
        en.push("Turn off the cmd add-on that was set up before (can be undone)");
    }
    if next != current {
        match req.target {
            Target::All => {
                th.push("ให้ cmd ทุกหน้าต่างใช้ธีมนี้");
                en.push("Use this theme in every cmd window");
            }
            Target::Wt => {
                th.push("หยุดโหลดธีมในหน้าต่าง cmd นอก Windows Terminal");
                en.push("Stop loading the theme in cmd windows outside Windows Terminal");
            }
        }
        // A value with nothing but whitespace left is deleted.
        let value = (!next.trim().is_empty()).then_some(next);
        ops.push(Op::SetAutoRun { value });
    }

    Ok(Plan {
        ops,
        summary_th: th,
        summary_en: en,
        warnings,
        launch,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::lua::Lang;
    use crate::model::Target;
    use crate::presets::terracotta;
    use std::path::PathBuf;

    // The Thai user name and the 8.3 names that VM-C gave
    // (docs/spikes/G4-G5-G16-G17-clink.md, G19).
    const DATA: &str = r"C:\Users\ผู้ทดสอบ ไทย\AppData\Local\ai.resincore.cmdstudio";
    const DATA_SHORT: &str = r"C:\Users\3607~1\AppData\Local\AIRESI~1.CMD";
    const SHORT_EXE: &str =
        r"C:\Users\3607~1\AppData\Local\AIRESI~1.CMD\runtime\clink-1.9.34\clink_x64.exe";
    const PROGRAM_DATA: &str = r"C:\ProgramData\ai.resincore.cmdstudio-S-1-5-21-1-2-3-1001";

    fn paths() -> Paths {
        Paths {
            data: PathBuf::from(DATA),
        }
    }

    fn req(target: Target) -> Request {
        Request {
            target,
            set_wt_default_profile: true,
            lang: Lang::Th,
            replace_foreign_clink: false,
        }
    }

    fn machine() -> Machine {
        Machine {
            wt_settings: vec![PathBuf::from(r"C:\wt\settings.json")],
            fonts: vec!["Cascadia Mono".to_string()],
            data_short: Some(PathBuf::from(DATA_SHORT)),
            ..Machine::default()
        }
    }

    /// Our entry for `paths()` on `machine()`, built without `clink_launch`.
    fn entry() -> String {
        autorun_entry(&ClinkLaunch::ShortPath(PathBuf::from(SHORT_EXE)), &paths()).unwrap()
    }

    /// Our entry as an older version wrote it: Clink 1.9.0, run from its real path.
    fn old_entry() -> String {
        format!(
            r#"if exist "{DATA}\runtime\clink-1.9.0\clink_x64.exe" "{DATA}\runtime\clink-1.9.0\clink_x64.exe" inject --autorun --quiet --profile "{DATA}\clink-profile""#
        )
    }

    fn autorun(p: &Plan) -> Option<Option<String>> {
        p.ops.iter().find_map(|o| match o {
            Op::SetAutoRun { value } => Some(value.clone()),
            _ => None,
        })
    }

    /// The `SetAutoRun` of a plan for `target` on `machine()` whose AutoRun value is
    /// `current`; `None` when the plan leaves the value as it is.
    fn set_autorun(current: Option<&str>, target: Target) -> Option<Option<String>> {
        let mut m = machine();
        m.autorun_hkcu = current.map(str::to_string);
        autorun(&plan(&terracotta(), &req(target), &m, &paths()).unwrap())
    }

    fn wt_profile(p: &Plan) -> Map<String, Value> {
        p.ops
            .iter()
            .find_map(|o| match o {
                Op::WtApply { profile, .. } => Some(profile.clone()),
                _ => None,
            })
            .unwrap()
    }

    /// The commands `parts` finds in `value`.
    fn texts(value: &str) -> Vec<String> {
        parts(value).into_iter().map(|p| p.text).collect()
    }

    #[test]
    fn clean_machine_gets_runtime_files_wt_and_autorun() {
        let p = plan(&terracotta(), &req(Target::All), &machine(), &paths()).unwrap();
        assert_eq!(
            entry(),
            format!(
                r#"if exist "{SHORT_EXE}" "{SHORT_EXE}" inject --autorun --quiet --profile "{DATA}\clink-profile""#
            )
        );
        assert_eq!(autorun(&p), Some(Some(entry())));
        assert_eq!(
            p.ops
                .iter()
                .filter(|o| matches!(o, Op::WtApply { .. }))
                .count(),
            1
        );
        assert_eq!(
            p.ops
                .iter()
                .filter(|o| matches!(o, Op::WriteFile { .. }))
                .count(),
            4
        );
        assert!(p.summary_th.contains(&"ให้ cmd ทุกหน้าต่างใช้ธีมนี้"));
        assert!(p.summary_en.contains(&"Use this theme in every cmd window"));
        assert!(p.warnings.is_empty());
    }

    #[test]
    fn an_ascii_data_path_runs_clink_from_its_real_path() {
        let p = Paths {
            data: PathBuf::from(r"C:\Users\tester\AppData\Local\ai.resincore.cmdstudio"),
        };
        assert_eq!(
            clink_launch(&p, &machine()),
            Ok(ClinkLaunch::Direct(p.clink_exe()))
        );
    }

    #[test]
    fn a_thai_user_name_runs_clink_through_the_8_3_path() {
        // Review Focus 3, G19
        let p = plan(&terracotta(), &req(Target::All), &machine(), &paths()).unwrap();
        assert_eq!(p.launch, ClinkLaunch::ShortPath(PathBuf::from(SHORT_EXE)));
        assert!(p.ops.contains(&Op::InstallRuntime {
            component: Component::Clink,
            dir: paths().clink_dir()
        }));
    }

    #[test]
    fn without_an_8_3_name_clink_goes_to_the_users_program_data_folder() {
        let mut m = machine();
        // A volume that makes no 8.3 names gives back the long path.
        m.data_short = Some(PathBuf::from(DATA));
        m.program_data_dir = Some(PathBuf::from(PROGRAM_DATA));
        let p = plan(&terracotta(), &req(Target::All), &m, &paths()).unwrap();
        let dir = PathBuf::from(PROGRAM_DATA).join("clink-1.9.34");
        assert_eq!(p.launch, ClinkLaunch::ProgramData { dir: dir.clone() });
        assert!(p.ops.contains(&Op::InstallRuntime {
            component: Component::Clink,
            dir: dir.clone()
        }));
        assert!(!p.ops.contains(&Op::InstallRuntime {
            component: Component::Clink,
            dir: paths().clink_dir()
        }));
        let value = autorun(&p).unwrap().unwrap();
        assert!(value.starts_with(&format!(
            r#"if exist "{PROGRAM_DATA}\clink-1.9.34\clink_x64.exe" "#
        )));
        assert!(p
            .summary_th
            .contains(&"เก็บส่วนเสริมไว้นอกโฟลเดอร์ผู้ใช้ เพราะชื่อโฟลเดอร์ผู้ใช้มีอักษรที่ส่วนเสริมใช้ไม่ได้"));
        assert!(p.summary_en.contains(
            &"Keep the add-on outside your user folder, because your user folder's name has letters it can't use"
        ));
    }

    #[test]
    fn no_ascii_path_for_clink_fails_the_plan() {
        // G19: never write a command that makes every cmd window show an error.
        let mut m = machine();
        m.data_short = None;
        assert_eq!(
            plan(&terracotta(), &req(Target::All), &m, &paths())
                .unwrap_err()
                .code(),
            "E_CLINK_PATH"
        );
        // Each candidate must itself be all ASCII (spec §5.4).
        m.data_short = Some(PathBuf::from(r"C:\Users\ÉTÉ~1\AppData\Local\AIRESI~1.CMD"));
        m.program_data_dir = Some(PathBuf::from(
            r"D:\ข้อมูล\ai.resincore.cmdstudio-S-1-5-21-1-2-3-1001",
        ));
        assert_eq!(
            clink_launch(&paths(), &m),
            Err(PlanError::NoClinkPath(paths().clink_exe()))
        );
        assert_eq!(
            plan(&terracotta(), &req(Target::Wt), &m, &paths())
                .unwrap_err()
                .code(),
            "E_CLINK_PATH"
        );
    }

    #[test]
    fn keeps_other_programs_autorun_verbatim() {
        let anaconda = r#""C:\ProgramData\anaconda3\condabin\conda_hook.bat""#;
        let mut m = machine();
        m.autorun_hkcu = Some(anaconda.to_string());
        let p = plan(&terracotta(), &req(Target::All), &m, &paths()).unwrap();
        assert_eq!(autorun(&p), Some(Some(format!("{anaconda} & {}", entry()))));
    }

    #[test]
    fn replaces_our_older_entry_instead_of_adding_another() {
        let old = old_entry();
        let mut m = machine();
        m.autorun_hkcu = Some(format!("echo hi & {old}"));
        let p = plan(&terracotta(), &req(Target::All), &m, &paths()).unwrap();
        assert_eq!(autorun(&p), Some(Some(format!("echo hi & {}", entry()))));
    }

    #[test]
    fn nothing_to_do_when_autorun_is_already_ours() {
        let mut m = machine();
        m.autorun_hkcu = Some(entry());
        let p = plan(&terracotta(), &req(Target::All), &m, &paths()).unwrap();
        assert_eq!(autorun(&p), None);
    }

    #[test]
    fn a_foreign_clink_needs_permission() {
        let foreign = r#""C:\Program Files (x86)\clink\clink.bat" inject --autorun"#;
        let mut m = machine();
        m.autorun_hkcu = Some(foreign.to_string());
        let err = plan(&terracotta(), &req(Target::All), &m, &paths()).unwrap_err();
        assert_eq!(err.code(), "E_AUTORUN_FOREIGN_CLINK");
        assert_eq!(
            err,
            PlanError::ForeignClink {
                existing: foreign.to_string()
            }
        );
        let mut r = req(Target::All);
        r.replace_foreign_clink = true;
        let p = plan(&terracotta(), &r, &m, &paths()).unwrap();
        assert_eq!(autorun(&p), Some(Some(entry())));
        assert!(p
            .summary_th
            .contains(&"ปิดส่วนเสริมของ cmd ตัวเดิมที่ติดตั้งไว้ก่อน (ย้อนกลับได้)"));
        assert!(p
            .summary_en
            .contains(&"Turn off the cmd add-on that was set up before (can be undone)"));
    }

    #[test]
    fn wt_target_removes_our_entry_and_injects_from_the_profile() {
        let mut m = machine();
        m.autorun_hkcu = Some(format!("echo hi & {}", entry()));
        let p = plan(&terracotta(), &req(Target::Wt), &m, &paths()).unwrap();
        assert_eq!(autorun(&p), Some(Some("echo hi".to_string())));
        assert_eq!(
            wt_profile(&p)["commandline"],
            format!(
                r#"%SystemRoot%\System32\cmd.exe /k ""{SHORT_EXE}" inject --quiet --profile "{DATA}\clink-profile"""#
            )
        );
    }

    #[test]
    fn wt_target_with_only_our_entry_deletes_the_value() {
        let mut m = machine();
        m.autorun_hkcu = Some(entry());
        let p = plan(&terracotta(), &req(Target::Wt), &m, &paths()).unwrap();
        assert_eq!(autorun(&p), Some(None));
    }

    #[test]
    fn missing_windows_terminal_is_a_warning_not_an_error() {
        let mut m = machine();
        m.wt_settings.clear();
        m.fonts.clear();
        let p = plan(&terracotta(), &req(Target::All), &m, &paths()).unwrap();
        // Without Windows Terminal there is no font to set, so no font is installed.
        assert_eq!(p.warnings, vec![Warning::NoWindowsTerminal]);
        assert!(!p
            .ops
            .iter()
            .any(|o| matches!(o, Op::WtApply { .. } | Op::InstallFont { .. })));
    }

    #[test]
    fn machine_wide_clink_is_flagged() {
        let mut m = machine();
        m.autorun_hklm = Some(r#""C:\Program Files\clink\clink.bat" inject --autorun"#.to_string());
        let p = plan(&terracotta(), &req(Target::All), &m, &paths()).unwrap();
        assert!(p.warnings.contains(&Warning::MachineWideClink));
    }

    #[test]
    fn refuses_paths_cmd_cannot_quote() {
        // Review Focus 3
        for bad in [r"C:\Users\50%off\x", r#"C:\Users\a"b\x"#] {
            let p = Paths {
                data: PathBuf::from(bad),
            };
            let launch = clink_launch(&p, &Machine::default()).unwrap();
            assert!(
                matches!(
                    autorun_entry(&launch, &p),
                    Err(PlanError::UnsupportedPath(_))
                ),
                "{bad}"
            );
        }
        // The profile path is checked too, even when Clink runs from the 8.3 path.
        let p = Paths {
            data: PathBuf::from(r"C:\Users\ผู้ใช้ 100%\x"),
        };
        let m = Machine {
            data_short: Some(PathBuf::from(r"C:\Users\E0B8~1\x")),
            ..Machine::default()
        };
        let launch = clink_launch(&p, &m).unwrap();
        assert!(matches!(
            autorun_entry(&launch, &p),
            Err(PlanError::UnsupportedPath(_))
        ));
    }

    #[test]
    fn wt_target_refuses_those_paths_too() {
        // Review Focus 3: the profile's `/k` line runs the same command.
        let p = Paths {
            data: PathBuf::from(r"C:\Users\50%off\x"),
        };
        let err = plan(&terracotta(), &req(Target::Wt), &machine(), &p).unwrap_err();
        assert_eq!(err.code(), "E_CLINK_PATH");
    }

    #[test]
    fn a_semicolon_in_the_data_folder_is_refused() {
        // `clink.path` in clink_settings is a list split at `;`, so `<data>\generated`
        // must not hold one.
        let p = Paths {
            data: PathBuf::from(r"D:\Profiles;old\tester\ai.resincore.cmdstudio"),
        };
        for target in [Target::All, Target::Wt] {
            let err = plan(&terracotta(), &req(target), &machine(), &p).unwrap_err();
            assert!(matches!(err, PlanError::UnsupportedPath(_)), "{err:?}");
            assert_eq!(err.code(), "E_CLINK_PATH");
        }
        // Also when Clink itself runs from an 8.3 path that has none: the profile and the
        // generated folder still sit under the long path.
        let p = Paths {
            data: PathBuf::from(r"D:\ข้อมูล;เก่า\ai.resincore.cmdstudio"),
        };
        let m = Machine {
            data_short: Some(PathBuf::from(r"D:\E0B8~1\AIRESI~1.CMD")),
            ..machine()
        };
        let err = plan(&terracotta(), &req(Target::All), &m, &p).unwrap_err();
        assert_eq!(err, PlanError::UnsupportedPath(p.clink_profile()));
    }

    #[cfg(windows)]
    #[test]
    fn a_path_that_is_not_unicode_is_refused() {
        use std::ffi::OsString;
        use std::os::windows::ffi::OsStringExt;
        // `C:\Users\<an unpaired surrogate>\x`: Windows takes it as a folder name, but it is
        // not Unicode, and the generated files would hold U+FFFD in its place.
        let mut wide: Vec<u16> = r"C:\Users\".encode_utf16().collect();
        wide.push(0xD800);
        wide.extend(r"\x".encode_utf16());
        let data = PathBuf::from(OsString::from_wide(&wide));
        assert_eq!(data.to_str(), None);
        let p = Paths { data };
        let m = Machine {
            data_short: Some(PathBuf::from(r"C:\Users\ABCD~1\x")),
            ..machine()
        };
        for target in [Target::All, Target::Wt] {
            let err = plan(&terracotta(), &req(target), &m, &p).unwrap_err();
            assert_eq!(err, PlanError::UnsupportedPath(p.clink_profile()));
            assert_eq!(err.code(), "E_CLINK_PATH");
        }
        // A launch path that is not Unicode is refused as well.
        let launch = ClinkLaunch::Direct(p.clink_exe());
        let ascii = Paths {
            data: PathBuf::from(r"C:\Users\tester\x"),
        };
        assert_eq!(
            autorun_entry(&launch, &ascii),
            Err(PlanError::UnsupportedPath(p.clink_exe()))
        );
    }

    #[test]
    fn ampersand_and_spaces_in_paths_are_quoted_and_stay_one_part() {
        // Review Focus 3
        let p = Paths {
            data: PathBuf::from(r"C:\Users\R&D Team\x"),
        };
        let launch = clink_launch(&p, &Machine::default()).unwrap();
        let entry = autorun_entry(&launch, &p).unwrap();
        assert!(entry.contains(r#""C:\Users\R&D Team\x\runtime\clink-1.9.34\clink_x64.exe""#));
        assert_eq!(parts(&entry).len(), 1);
    }

    #[test]
    fn double_ampersand_between_foreign_parts_is_preserved() {
        let mut m = machine();
        m.autorun_hkcu = Some("a && b".to_string());
        let p = plan(&terracotta(), &req(Target::All), &m, &paths()).unwrap();
        assert_eq!(autorun(&p), Some(Some(format!("a && b & {}", entry()))));
    }

    #[test]
    fn a_caret_escaped_ampersand_is_not_a_separator() {
        // cmd reads `^&` as a plain `&`, so `echo a ^& b` is one command, kept as written.
        assert_eq!(
            set_autorun(Some("echo a ^& b"), Target::All),
            Some(Some(format!("echo a ^& b & {}", entry())))
        );
    }

    #[test]
    fn other_programs_separators_keep_their_spacing() {
        for foreign in ["a&&b", "a&b", "a  &   b"] {
            assert_eq!(
                set_autorun(Some(foreign), Target::All),
                Some(Some(format!("{foreign} & {}", entry()))),
                "{foreign}"
            );
        }
    }

    #[test]
    fn our_older_entry_is_replaced_wherever_it_stands() {
        let (old, new) = (old_entry(), entry());
        for (current, next) in [
            (format!("{old} & y"), format!("y & {new}")),
            (format!("x & {old} & y"), format!("x & y & {new}")),
            (format!("x & {old}"), format!("x & {new}")),
            (format!("x&&{old}&y"), format!("x&y & {new}")),
        ] {
            assert_eq!(
                set_autorun(Some(&current), Target::All),
                Some(Some(next)),
                "{current}"
            );
        }
    }

    #[test]
    fn wt_target_takes_our_entry_out_wherever_it_stands() {
        let ours = entry();
        for (current, next) in [
            (format!("x & {ours} & y"), "x & y"),
            (format!("{ours} & y"), "y"),
            (format!("x & {ours}"), "x"),
            (format!("x && {ours}"), "x"),
            (format!("x & {}", old_entry()), "x"),
        ] {
            assert_eq!(
                set_autorun(Some(&current), Target::Wt),
                Some(Some(next.to_string())),
                "{current}"
            );
        }
    }

    #[test]
    fn an_ampersand_inside_quotes_does_not_split_a_command() {
        let current = format!(r#""C:\a&b\x.bat" & {}"#, entry());
        assert_eq!(
            set_autorun(Some(&current), Target::Wt),
            Some(Some(r#""C:\a&b\x.bat""#.to_string()))
        );
    }

    #[test]
    fn a_caret_escaped_quote_does_not_start_quoting() {
        // Were `^"` to open a quote, the `&` after it would be quoted, our entry would be one
        // command with `echo`, and taking ours out would take `echo` with it.
        let current = format!(r#"echo ^"a & {}"#, entry());
        assert_eq!(
            set_autorun(Some(&current), Target::Wt),
            Some(Some(r#"echo ^"a"#.to_string()))
        );
    }

    #[test]
    fn a_value_left_blank_is_deleted() {
        let current = format!(" \t{} ", entry());
        assert_eq!(set_autorun(Some(&current), Target::Wt), Some(None));
    }

    #[test]
    fn wt_target_leaves_a_value_without_ours_alone() {
        for current in [None, Some(""), Some("  "), Some("echo hi"), Some("a&&b")] {
            assert_eq!(set_autorun(current, Target::Wt), None, "{current:?}");
        }
    }

    #[test]
    fn a_foreign_clink_goes_without_touching_the_parts_around_it() {
        let clink = r#""C:\Program Files (x86)\clink\clink.bat" inject --autorun"#;
        let mut m = machine();
        m.autorun_hkcu = Some(format!("x&&{clink} & echo a ^& b"));
        let mut r = req(Target::All);
        r.replace_foreign_clink = true;
        let p = plan(&terracotta(), &r, &m, &paths()).unwrap();
        assert_eq!(
            autorun(&p),
            Some(Some(format!("x & echo a ^& b & {}", entry())))
        );
    }

    #[test]
    fn all_then_wt_gives_back_every_foreign_value() {
        // Doctrine guard (ค), spec §7: the AutoRun writer never removes or changes a part that
        // is not ours. Applying a theme to every cmd window and then to Windows Terminal only
        // gives back what other programs wrote, byte for byte.
        let foreign = [
            r#""C:\ProgramData\anaconda3\condabin\conda_hook.bat""#,
            r#"if exist "C:\Users\me\miniconda3\condabin\conda_hook.bat" "C:\Users\me\miniconda3\condabin\conda_hook.bat""#,
            "echo a ^& b",
            r#"echo ^"a"#,
            r#""C:\a^"&b"#,
            "a&&b",
            "a && b",
            "a&b",
            "a\t&\tb",
            r#""C:\a&b\x.bat""#,
            "@chcp 65001>nul",
            r#"doskey /macrofile="C:\Users\me\macros.txt" & cls"#,
            "prompt $p$g",
            "cls  ",
            "  cls",
            "x||y",
            "x || y",
            "a | b",
            "(echo a & echo b)",
            r#"if exist "C:\x.bat" (call "C:\x.bat")"#,
            r#"echo สวัสดี & cd /d "D:\งาน""#,
        ];
        for v in foreign {
            let all = format!("{v} & {}", entry());
            assert_eq!(
                set_autorun(Some(v), Target::All),
                Some(Some(all.clone())),
                "{v}"
            );
            assert_eq!(set_autorun(Some(&all), Target::All), None, "again: {v}");
            assert_eq!(
                set_autorun(Some(&all), Target::Wt),
                Some(Some(v.to_string())),
                "{v}"
            );
        }
    }

    #[test]
    fn parts_follow_cmds_quotes_and_carets() {
        assert_eq!(texts("a & b && c"), ["a", "b", "c"]);
        assert_eq!(texts("a&b&&c"), ["a", "b", "c"]);
        assert_eq!(texts(r#""x & y" & z"#), [r#""x & y""#, "z"]);
        assert_eq!(texts("echo a ^& b"), ["echo a ^& b"]);
        assert_eq!(texts(r#"echo ^"a & b"#), [r#"echo ^"a"#, "b"]);
        // Inside quotes a caret is plain text, so the quote after it still closes.
        assert_eq!(texts(r#""a^" & b"#), [r#""a^""#, "b"]);
        assert_eq!(texts(" \ta  b \t& b"), ["a  b", "b"]);
        // An empty command is no part.
        assert_eq!(texts("a & & b"), ["a", "b"]);
        assert!(texts("").is_empty());
        assert!(texts(" \t ").is_empty());
        // `||` separates commands like `&&`; a single `|` is a pipe inside one command.
        assert_eq!(texts("a||b || c"), ["a", "b", "c"]);
        assert_eq!(texts("a | b & c"), ["a | b", "c"]);
        assert_eq!(texts("a ^|| b"), ["a ^|| b"]);
    }

    #[test]
    fn scan_tells_whether_a_value_ends_cleanly() {
        for v in [
            "",
            "  ",
            "a",
            "a & b",
            "a ^& b",
            r#""a & b""#,
            "a^^",
            "a|b",
            "a || b",
        ] {
            assert!(scan(v).clean_end, "{v}");
        }
        // Inside an unclosed quote, after a lone caret, or after a separator.
        for v in [
            r#""a"#,
            r#"a "b & c"#,
            "a ^",
            "^",
            "a &",
            "a&&",
            "a && \t",
            "a ||",
            "&",
        ] {
            assert!(!scan(v).clean_end, "{v}");
        }
    }

    #[test]
    fn a_value_that_does_not_end_cleanly_gets_our_entry_in_front() {
        // Text after such a value would be quoted, escaped or read after an empty command,
        // so our entry goes first and the value stays as it was, byte for byte.
        for v in [
            r#""C:\x.bat"#,
            r#"call "C:\my tools\x.bat"#,
            "a &",
            "a && \t",
            "a ||",
            "echo hi ^",
        ] {
            let all = format!("{} & {v}", entry());
            assert_eq!(
                set_autorun(Some(v), Target::All),
                Some(Some(all.clone())),
                "{v}"
            );
            assert_eq!(set_autorun(Some(&all), Target::All), None, "again: {v}");
            assert_eq!(
                set_autorun(Some(&all), Target::Wt),
                Some(Some(v.to_string())),
                "{v}"
            );
            // An older entry of ours in front gives way to the current one.
            let old = format!("{} & {v}", old_entry());
            assert_eq!(
                set_autorun(Some(&old), Target::All),
                Some(Some(all)),
                "old: {v}"
            );
        }
    }

    #[test]
    fn a_double_bar_next_to_our_entry_goes_with_it() {
        let ours = entry();
        for (current, next) in [
            (format!("x || {ours}"), "x"),
            (format!("{ours} || y"), "y"),
            (format!("x||{ours}||y"), "x||y"),
        ] {
            assert_eq!(
                set_autorun(Some(&current), Target::Wt),
                Some(Some(next.to_string())),
                "{current}"
            );
        }
    }

    #[test]
    fn taking_ours_out_never_pairs_a_bar_with_another() {
        // Were the `&` before ours taken out, the `|` of `x|` would meet the `||` after ours,
        // and cmd would read `x||` and `|y`. The separator after ours goes instead.
        let current = format!("x|&{}||y", entry());
        assert_eq!(
            set_autorun(Some(&current), Target::Wt),
            Some(Some("x|&y".to_string()))
        );
        assert_eq!(texts("x|&y"), ["x|", "y"]);
    }

    #[test]
    fn ours_combined_with_another_command_is_left_for_a_hand_edit() {
        let ours = entry();
        for current in [
            format!("x | {ours}"),
            format!("{ours} | more"),
            format!("(a & {ours})"),
            format!("if defined X ({ours})"),
            // An unclosed quote runs `y.bat` into ours.
            format!(r#""C:\y.bat & {ours}"#),
        ] {
            for target in [Target::All, Target::Wt] {
                let mut m = machine();
                m.autorun_hkcu = Some(current.clone());
                let err = plan(&terracotta(), &req(target), &m, &paths()).unwrap_err();
                assert!(
                    matches!(err, PlanError::AutoRunNeedsHandEdit { .. }),
                    "{current}: {err:?}"
                );
                assert_eq!(err.code(), "E_AUTORUN_WRITE");
            }
        }
        let mut m = machine();
        m.autorun_hkcu = Some(format!("x | {ours}"));
        let err = plan(&terracotta(), &req(Target::Wt), &m, &paths()).unwrap_err();
        assert_eq!(
            err,
            PlanError::AutoRunNeedsHandEdit {
                part: format!("x | {ours}")
            }
        );
        assert!(err.to_string().contains("can't safely change"), "{err}");
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
            // An unclosed quote runs `x.bat` into the other Clink.
            (r#""C:\x.bat & clink inject"#, r#""C:\x.bat & clink inject"#),
        ] {
            let mut m = machine();
            m.autorun_hkcu = Some(current.to_string());
            for target in [Target::All, Target::Wt] {
                // Without the user's agreement, the question about the other Clink comes first.
                let err = plan(&terracotta(), &req(target), &m, &paths()).unwrap_err();
                assert_eq!(
                    err,
                    PlanError::ForeignClink {
                        existing: part.to_string()
                    }
                );
                let mut r = req(target);
                r.replace_foreign_clink = true;
                let err = plan(&terracotta(), &r, &m, &paths()).unwrap_err();
                assert_eq!(
                    err,
                    PlanError::AutoRunNeedsHandEdit {
                        part: part.to_string()
                    },
                    "{current}"
                );
                assert_eq!(err.code(), "E_AUTORUN_WRITE");
            }
        }
    }

    #[test]
    fn parentheses_in_our_own_paths_are_quoted_and_do_not_count() {
        // A user folder may be named `John (Work)`; our entry quotes its paths.
        let p = Paths {
            data: PathBuf::from(r"C:\Users\John (Work)\AppData\Local\ai.resincore.cmdstudio"),
        };
        let mut m = machine();
        let ours = autorun_entry(&clink_launch(&p, &m).unwrap(), &p).unwrap();
        m.autorun_hkcu = Some(format!("x & {ours}"));
        let planned = plan(&terracotta(), &req(Target::Wt), &m, &p).unwrap();
        assert_eq!(autorun(&planned), Some(Some("x".to_string())));
    }

    #[test]
    fn our_older_entry_is_found_whatever_the_case_of_its_profile_path() {
        // Windows reads `...\CLINK-PROFILE` and `...\clink-profile` as one folder.
        let profile = format!(r"{DATA}\clink-profile");
        let old = old_entry().replace(&profile, &profile.to_uppercase());
        assert_ne!(old, old_entry());
        let current = format!("echo hi & {old}");
        assert_eq!(
            set_autorun(Some(&current), Target::All),
            Some(Some(format!("echo hi & {}", entry())))
        );
        assert_eq!(
            set_autorun(Some(&current), Target::Wt),
            Some(Some("echo hi".to_string()))
        );
    }

    #[test]
    fn summaries_use_plain_words() {
        // Spec §4.5: the list of changes the wizard shows has no technical terms, and Clink
        // and Oh My Posh are "the add-on".
        let mut m = machine();
        m.data_short = None;
        m.program_data_dir = Some(PathBuf::from(PROGRAM_DATA));
        m.fonts.clear();
        m.autorun_hkcu = Some(r#""C:\clink\clink.bat" inject"#.to_string());
        let mut lines = std::collections::BTreeSet::new();
        for target in [Target::All, Target::Wt] {
            let mut r = req(target);
            r.replace_foreign_clink = true;
            let p = plan(&terracotta(), &r, &m, &paths()).unwrap();
            lines.extend(p.summary_th);
            lines.extend(p.summary_en);
        }
        // Every line the planner has, in both languages.
        assert_eq!(lines.len(), 16, "{lines:#?}");
        for line in lines {
            let lower = line.to_lowercase();
            for term in [
                "clink",
                "programdata",
                "path",
                "autorun",
                "oh my posh",
                "snapshot",
            ] {
                assert!(!lower.contains(term), "{line}");
            }
        }
    }

    #[test]
    fn a_missing_font_falls_back_to_the_bundled_nerd_font() {
        // spec §3.3
        let mut m = machine();
        m.fonts.clear();
        let p = plan(&terracotta(), &req(Target::All), &m, &paths()).unwrap();
        assert_eq!(
            p.warnings,
            vec![Warning::FontFallback {
                wanted: "Cascadia Mono".to_string()
            }]
        );
        assert!(p.ops.contains(&Op::InstallFont {
            family: NERD_FONT_FAMILY.to_string()
        }));
        assert_eq!(wt_profile(&p)["font"]["face"], NERD_FONT_FAMILY);
    }

    #[test]
    fn the_fallback_font_is_not_installed_twice() {
        let mut m = machine();
        m.fonts = vec![NERD_FONT_FAMILY.to_string()];
        let p = plan(&terracotta(), &req(Target::All), &m, &paths()).unwrap();
        assert_eq!(
            p.warnings,
            vec![Warning::FontFallback {
                wanted: "Cascadia Mono".to_string()
            }]
        );
        assert!(!p.ops.iter().any(|o| matches!(o, Op::InstallFont { .. })));
        assert_eq!(wt_profile(&p)["font"]["face"], NERD_FONT_FAMILY);
    }

    #[test]
    fn an_installed_font_is_kept_in_the_machines_spelling() {
        let mut m = machine();
        m.fonts = vec!["CASCADIA MONO".to_string()];
        let p = plan(&terracotta(), &req(Target::All), &m, &paths()).unwrap();
        assert_eq!(wt_profile(&p)["font"]["face"], "CASCADIA MONO");
        assert!(!p.ops.iter().any(|o| matches!(o, Op::InstallFont { .. })));
        assert!(p.warnings.is_empty());
    }

    #[test]
    fn a_theme_that_names_the_bundled_font_gets_it_without_a_warning() {
        let mut t = terracotta();
        t.terminal.font = NERD_FONT_FAMILY.to_string();
        let p = plan(&t, &req(Target::All), &machine(), &paths()).unwrap();
        assert!(p.ops.contains(&Op::InstallFont {
            family: NERD_FONT_FAMILY.to_string()
        }));
        assert!(p.warnings.is_empty());
        assert_eq!(wt_profile(&p)["font"]["face"], NERD_FONT_FAMILY);
    }

    #[test]
    fn ops_serialize_for_the_journal() {
        let p = plan(&terracotta(), &req(Target::All), &machine(), &paths()).unwrap();
        let json = serde_json::to_string(&p.ops).unwrap();
        let back: Vec<Op> = serde_json::from_str(&json).unwrap();
        assert_eq!(back, p.ops);
    }

    #[test]
    fn journal_names_are_camel_case() {
        let mut m = machine();
        m.fonts.clear();
        let p = plan(&terracotta(), &req(Target::All), &m, &paths()).unwrap();
        let json = serde_json::to_value(&p.ops).unwrap();
        let ops = json.as_array().unwrap();
        let kinds: Vec<&str> = ops.iter().map(|o| o["kind"].as_str().unwrap()).collect();
        for kind in [
            "installRuntime",
            "writeFile",
            "wtApply",
            "setAutoRun",
            "installFont",
        ] {
            assert!(kinds.contains(&kind), "{kind} is not in {kinds:?}");
        }
        let wt = ops.iter().find(|o| o["kind"] == "wtApply").unwrap();
        assert_eq!(wt["setDefault"], true);
        for op in ops {
            for key in op.as_object().unwrap().keys() {
                assert!(!key.contains('_'), "{key} in {op}");
            }
        }
        let back: Vec<Op> = serde_json::from_value(json).unwrap();
        assert_eq!(back, p.ops);
    }
}
