//! The planner (spec §5.2 `plan`): theme + request + machine facts → the
//! operations to run and a plain-language summary (spec §4.2 page 5).
//! Pure: it reads nothing and writes nothing.
use crate::adapters::RegText;
use crate::generate::{clink, lua, omp, wt};
use crate::model::{Target, Theme};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

pub(crate) mod autorun;

pub const OMP_VERSION: &str = "31.5.0";
pub const CLINK_VERSION: &str = "1.9.34";
/// GDI family name of the bundled Nerd Font: the name detect looks for, the
/// `font.face` written to Windows Terminal, and the fallback font
/// (spec §3.3, §5.4, G12).
pub const NERD_FONT_FAMILY: &str = "CaskaydiaCove NF";

/// The `part` of [`PlanError::AutoRunNeedsHandEdit`] when the AutoRun value is there but is not
/// plain text: it is never edited, nor taken for an absent value (spec §5.4).
pub const AUTORUN_NOT_TEXT: &str = "the AutoRun value is not plain text (it is not a String \
                                    Value (REG_SZ) or an Expandable String Value \
                                    (REG_EXPAND_SZ), or its text is not valid)";
/// [`AUTORUN_NOT_TEXT`] in Thai.
pub const AUTORUN_NOT_TEXT_TH: &str = "ค่า AutoRun ไม่ใช่ข้อความธรรมดา (ไม่ใช่ชนิด String Value \
                                       (REG_SZ) หรือ Expandable String Value (REG_EXPAND_SZ) \
                                       หรือมีข้อความที่ไม่ถูกต้อง)";

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
    /// Current HKCU AutoRun value as the registry holds it: the raw text, never expanded, so a
    /// `%USERPROFILE%` in it stays as written; or absent; or there but not plain text, which
    /// the planner never edits (spec §5.4).
    pub autorun_hkcu: RegText,
    /// Current HKLM AutoRun value, read in the same way (read only; CMD Studio never writes
    /// HKLM).
    pub autorun_hklm: RegText,
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
    /// Writes the `CMD Studio` scheme and the cmd profile's keys into one settings.json. A key
    /// whose value is an object (`font`) is merged into the user's object key by key, so keys
    /// of the user's own in it, such as `font.weight`, stay.
    WtApply {
        settings: PathBuf,
        scheme: Value,
        profile: Map<String, Value>,
        set_default: bool,
    },
    /// The complete new HKCU AutoRun value, raw as the registry holds it (never expanded);
    /// `None` deletes it. Writing it keeps the value's registry type: a `REG_EXPAND_SZ` stays
    /// `REG_EXPAND_SZ`, and a value that was not there is written as `REG_SZ` (spec §5.4).
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
    /// Another Clink runs from AutoRun. `existing` is every command that turning it off takes
    /// out, so the user sees all of them before agreeing.
    #[error(
        "another Clink runs from cmd's AutoRun, in these commands: {}",
        quoted(existing)
    )]
    ForeignClink { existing: Vec<String> },
    /// A path that would go into a cmd command or into Clink's settings holds `%`, `"` or
    /// `;`, or is not Unicode (spec §5.4).
    #[error(
        "cmd or Clink can't take this path: it holds %, a double quote, ; or text that is not \
         Unicode: \"{}\"",
        .0.display()
    )]
    UnsupportedPath(PathBuf),
    #[error(
        "Clink can't start from \"{}\", and no 8.3 or ProgramData path is available instead",
        .0.display()
    )]
    NoClinkPath(PathBuf),
    #[error("could not build the Oh My Posh config: {0}")]
    Generate(String),
    /// AutoRun can't be changed safely, so the user edits it by hand. `part` is the command at
    /// fault: one that holds our profile path without being one of our entries, or another
    /// Clink the user agreed to turn off that a pipe, parentheses or an unclosed quote combine
    /// with other commands. It is the whole value when cmd itself rejects the value: nothing
    /// in it runs then, and our entry in front of it would not run either. It is
    /// [`AUTORUN_NOT_TEXT`] when the value is there but is not plain text.
    #[error("AutoRun holds a command that CMD Studio can't safely change, so it needs a hand edit: {part}")]
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

/// Each text in plain quotes, as it is written, one after the other.
fn quoted(texts: &[String]) -> String {
    texts
        .iter()
        .map(|t| format!("\"{t}\""))
        .collect::<Vec<_>>()
        .join(", ")
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

/// Our AutoRun entry (spec §5.4):
/// `(if exist "<clink-launch>" "<clink-launch>" inject --autorun --quiet --profile "<profile>")`.
/// cmd's `if` takes the rest of its line as its body, so the parentheses end ours: placed in
/// front of other programs' commands, it neither takes them into its `if` nor becomes part of
/// theirs.
///
/// It refuses the paths that cmd or Clink can't take (see `checked`): `<clink-launch>`, and
/// the profile path. The profile path stands for every path under `<data>` that the generated
/// files hold (`generated`, `runtime\omp-<ver>`), because what follows `<data>` in each of
/// them is fixed ASCII.
pub fn autorun_entry(launch: &ClinkLaunch, paths: &Paths) -> Result<String, PlanError> {
    let (exe_path, profile_path) = (launch.exe(), paths.clink_profile());
    let exe = checked(&exe_path)?;
    let profile = checked(&profile_path)?;
    Ok(format!(
        r#"(if exist "{exe}" "{exe}" inject --autorun --quiet --profile "{profile}")"#
    ))
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
/// `E_AUTORUN_WRITE` when AutoRun can't be changed safely (see `autorun::edit_autorun`) or, for
/// every cmd window, is there but is not plain text ([`AUTORUN_NOT_TEXT`]).
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
            contents: lua::banner_lua(theme),
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

    // A Clink that cmd injects for every user of the machine, in any command of the value; a
    // command that only mentions Clink is none (see `autorun::runs_clink`). A value that is not
    // plain text gives no warning: what it holds cannot be told (doctor D5 says so).
    if let RegText::Text(v) = &machine.autorun_hklm {
        if autorun::scan(v)
            .parts
            .iter()
            .any(|p| autorun::runs_clink(&p.text))
        {
            warnings.push(Warning::MachineWideClink);
        }
    }

    let current = match (&machine.autorun_hkcu, req.target) {
        (RegText::Text(text), _) => Some(text.as_str()),
        (RegText::Absent, _) => Some(""),
        // A value that is there but is not plain text is never edited, nor taken for an
        // absent one: writing our entry would replace it (spec §5.4).
        (RegText::NotText, Target::All) => {
            return Err(PlanError::AutoRunNeedsHandEdit {
                part: AUTORUN_NOT_TEXT.to_string(),
            })
        }
        // Windows Terminal only: AutoRun is left as it is.
        (RegText::NotText, Target::Wt) => None,
    };
    if let Some(current) = current {
        // `autorun_entry` has refused a profile path cmd can't take; this gets its text.
        let profile = paths.clink_profile();
        let edit = autorun::edit_autorun(
            current,
            req.target,
            &entry,
            checked(&profile)?,
            req.replace_foreign_clink,
        )?;
        if edit.removed_foreign {
            th.push("ปิดส่วนเสริมของ cmd ตัวเดิมที่ติดตั้งไว้ก่อน (ย้อนกลับได้)");
            en.push("Turn off the cmd add-on that was set up before (can be undone)");
        }
        if edit.value != current {
            match req.target {
                Target::All => {
                    th.push("ให้ cmd ทุกหน้าต่างใช้ธีมนี้");
                    en.push("Use this theme in every cmd window");
                }
                // Only when ours goes: taking out another Clink stops no theme of ours.
                Target::Wt if edit.removed_ours => {
                    th.push("หยุดโหลดธีมในหน้าต่าง cmd นอก Windows Terminal");
                    en.push("Stop loading the theme in cmd windows outside Windows Terminal");
                }
                Target::Wt => {}
            }
            // A value with nothing but whitespace left is deleted.
            let value = (!edit.value.trim().is_empty()).then_some(edit.value);
            ops.push(Op::SetAutoRun { value });
        }
    }

    Ok(Plan {
        ops,
        summary_th: th,
        summary_en: en,
        warnings,
        launch,
    })
}

impl Paths {
    /// `<data>\drafts`: the editor's autosaved drafts, one `<theme id>.cmdtheme` each (spec
    /// §3.4, §5.3).
    pub fn drafts(&self) -> PathBuf {
        self.data.join("drafts")
    }
    /// `<data>\logs`: the app's log, `cmdstudio.log` and its rotated copies (spec §5.3).
    pub fn logs(&self) -> PathBuf {
        self.data.join("logs")
    }
    /// `<data>\webview`: WebView2's user data folder (its cache and storage). The app gives it
    /// to its window on purpose, so that it sits in CMD Studio's data folder under a name of CMD
    /// Studio's choosing, which `--cleanup --data-dir` knows (spec §5.3, §6.3).
    pub fn webview(&self) -> PathBuf {
        self.data.join("webview")
    }
    /// `<data>\settings.json`: the app's own settings (language, where themes go, updates).
    /// Not in state.json, because an undo puts state.json back as it was before an apply, and
    /// the settings must not go back with it.
    pub fn settings_file(&self) -> PathBuf {
        self.data.join("settings.json")
    }
}

/// What cmd's AutoRun holds, for the wizard's machine check and the app's system report (spec
/// §4.2 page 2): other programs' commands, which stay; every command that starts another Clink,
/// which an apply asks about before turning off (spec §5.4); whether a Clink starts from the
/// machine-wide value, which CMD Studio never changes; and whether the user's value is there
/// but not plain text, which CMD Studio leaves alone. The same reading as [`plan`] and doctor
/// D5 use (`autorun::scan`, `autorun::runs_clink`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AutoRunSummary {
    /// Other programs' commands in the user's value, as written, in order.
    pub other_commands: Vec<String>,
    /// Commands in the user's value that start another Clink, as written, in order.
    pub foreign_clink: Vec<String>,
    /// CMD Studio's own entry is in the user's value.
    pub ours_present: bool,
    /// A Clink starts from the machine-wide (HKLM) value.
    pub machine_wide_clink: bool,
    /// The user's value is there but is not plain text, so what it holds cannot be told.
    pub not_text: bool,
}

pub fn autorun_summary(machine: &Machine, paths: &Paths) -> AutoRunSummary {
    let mut summary = AutoRunSummary::default();
    let profile = paths.clink_profile();
    let profile = profile.to_str();
    match &machine.autorun_hkcu {
        RegText::Absent => {}
        RegText::NotText => summary.not_text = true,
        RegText::Text(value) => {
            for part in autorun::scan(value).parts {
                if profile.is_some_and(|p| autorun::holds_ours(&part.text, p)) {
                    summary.ours_present = true;
                } else if autorun::runs_clink(&part.text) {
                    summary.foreign_clink.push(part.text);
                } else {
                    summary.other_commands.push(part.text);
                }
            }
        }
    }
    if let RegText::Text(value) = &machine.autorun_hklm {
        summary.machine_wide_clink = autorun::scan(value)
            .parts
            .iter()
            .any(|p| autorun::runs_clink(&p.text));
    }
    summary
}

#[cfg(test)]
mod tests {
    use super::autorun::scan;
    use super::*;
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

    /// Our entry as an earlier build wrote it: no parentheses, and Clink 1.9.0 from its real
    /// path.
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
        m.autorun_hkcu = current.map_or(RegText::Absent, |c| RegText::Text(c.to_string()));
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

    #[test]
    fn clean_machine_gets_runtime_files_wt_and_autorun() {
        let p = plan(&terracotta(), &req(Target::All), &machine(), &paths()).unwrap();
        // In parentheses, so that no `if` or `for` of another program around it takes it in,
        // and its own `if` takes in nothing after it.
        assert_eq!(
            entry(),
            format!(
                r#"(if exist "{SHORT_EXE}" "{SHORT_EXE}" inject --autorun --quiet --profile "{DATA}\clink-profile")"#
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
            r#"(if exist "{PROGRAM_DATA}\clink-1.9.34\clink_x64.exe" "#
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
        let err = plan(&terracotta(), &req(Target::All), &m, &paths()).unwrap_err();
        assert_eq!(err.code(), "E_CLINK_PATH");
        assert_eq!(
            err.to_string(),
            format!(
                "Clink can't start from \"{}\", and no 8.3 or ProgramData path is available \
                 instead",
                paths().clink_exe().display()
            )
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
        m.autorun_hkcu = RegText::Text(anaconda.to_string());
        let p = plan(&terracotta(), &req(Target::All), &m, &paths()).unwrap();
        assert_eq!(autorun(&p), Some(Some(format!("{} & {anaconda}", entry()))));
    }

    #[test]
    fn replaces_our_older_entry_instead_of_adding_another() {
        let old = old_entry();
        let mut m = machine();
        m.autorun_hkcu = RegText::Text(format!("echo hi & {old}"));
        let p = plan(&terracotta(), &req(Target::All), &m, &paths()).unwrap();
        assert_eq!(autorun(&p), Some(Some(format!("{} & echo hi", entry()))));
    }

    #[test]
    fn nothing_to_do_when_autorun_is_already_ours() {
        let mut m = machine();
        m.autorun_hkcu = RegText::Text(entry());
        let p = plan(&terracotta(), &req(Target::All), &m, &paths()).unwrap();
        assert_eq!(autorun(&p), None);
    }

    #[test]
    fn a_foreign_clink_needs_permission() {
        let foreign = r#""C:\Program Files (x86)\clink\clink.bat" inject --autorun"#;
        let mut m = machine();
        m.autorun_hkcu = RegText::Text(foreign.to_string());
        let err = plan(&terracotta(), &req(Target::All), &m, &paths()).unwrap_err();
        assert_eq!(err.code(), "E_AUTORUN_FOREIGN_CLINK");
        assert_eq!(
            err,
            PlanError::ForeignClink {
                existing: vec![foreign.to_string()]
            }
        );
        // Each command as it is written, in plain quotes (no `{:?}` escapes).
        assert_eq!(
            err.to_string(),
            r#"another Clink runs from cmd's AutoRun, in these commands: ""C:\Program Files (x86)\clink\clink.bat" inject --autorun""#
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
        m.autorun_hkcu = RegText::Text(format!("{} & echo hi", entry()));
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
        m.autorun_hkcu = RegText::Text(entry());
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
        let clink = r#""C:\Program Files\clink\clink.bat" inject --autorun"#;
        let mut m = machine();
        for v in [clink.to_string(), format!("echo hi & {clink}")] {
            m.autorun_hklm = RegText::Text(v.clone());
            let p = plan(&terracotta(), &req(Target::All), &m, &paths()).unwrap();
            assert!(p.warnings.contains(&Warning::MachineWideClink), "{v}");
        }
        // A value that only mentions Clink runs no Clink for the whole machine.
        for v in [
            r#"doskey /macrofile="C:\tools\clink\macros.txt""#,
            r"cd /d C:\clink-notes",
        ] {
            m.autorun_hklm = RegText::Text(v.to_string());
            let p = plan(&terracotta(), &req(Target::All), &m, &paths()).unwrap();
            assert!(!p.warnings.contains(&Warning::MachineWideClink), "{v}");
        }
    }

    #[test]
    fn a_value_that_only_mentions_clink_does_not_stop_the_plan() {
        // Nothing to ask about and nothing to take out: Windows Terminal only leaves the value
        // alone, and every cmd window gets our entry in front of it.
        let doskey = r#"doskey /macrofile="C:\tools\clink\macros.txt""#;
        assert_eq!(set_autorun(Some(doskey), Target::Wt), None);
        assert_eq!(
            set_autorun(Some(doskey), Target::All),
            Some(Some(format!("{} & {doskey}", entry())))
        );
    }

    #[test]
    fn a_value_that_is_not_plain_text_is_never_edited() {
        // Every cmd window: refused, nothing planned, a hand edit in Registry Editor.
        let mut m = machine();
        m.autorun_hkcu = RegText::NotText;
        let err = plan(&terracotta(), &req(Target::All), &m, &paths()).unwrap_err();
        assert_eq!(
            err,
            PlanError::AutoRunNeedsHandEdit {
                part: AUTORUN_NOT_TEXT.to_string()
            }
        );
        assert_eq!(err.code(), "E_AUTORUN_WRITE");
        // Windows Terminal only: AutoRun is not touched, so the rest is planned.
        let p = plan(&terracotta(), &req(Target::Wt), &m, &paths()).unwrap();
        assert_eq!(autorun(&p), None);
        assert!(p.ops.iter().any(|o| matches!(o, Op::WtApply { .. })));
        // A machine-wide value that is not text gives no warning: nothing says what it holds.
        let mut m = machine();
        m.autorun_hklm = RegText::NotText;
        let p = plan(&terracotta(), &req(Target::All), &m, &paths()).unwrap();
        assert!(!p.warnings.contains(&Warning::MachineWideClink));
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
        // The path is said as it is written, Thai letters and all (no `{:?}` escapes).
        assert!(
            err.to_string()
                .ends_with(r#": "D:\ข้อมูล;เก่า\ai.resincore.cmdstudio\clink-profile""#),
            "{err}"
        );
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
        let read = scan(&entry);
        assert_eq!(read.parts.len(), 1);
        assert!(!read.rejected);
    }

    #[test]
    fn double_ampersand_between_foreign_parts_is_preserved() {
        let mut m = machine();
        m.autorun_hkcu = RegText::Text("a && b".to_string());
        let p = plan(&terracotta(), &req(Target::All), &m, &paths()).unwrap();
        assert_eq!(autorun(&p), Some(Some(format!("{} & a && b", entry()))));
    }

    #[test]
    fn a_value_left_blank_is_deleted() {
        let current = format!(" \t{} ", entry());
        assert_eq!(set_autorun(Some(&current), Target::Wt), Some(None));
    }

    #[test]
    fn wt_target_leaves_a_value_without_ours_alone() {
        // A value cmd rejects included: with nothing to take out, it is not ours to fix.
        for current in [
            None,
            Some(""),
            Some("  "),
            Some("echo hi"),
            Some("a&&b"),
            Some("a & & b"),
        ] {
            assert_eq!(set_autorun(current, Target::Wt), None, "{current:?}");
        }
    }

    #[test]
    fn wt_target_says_it_stops_loading_the_theme_only_when_our_entry_goes() {
        let stop_th = "หยุดโหลดธีมในหน้าต่าง cmd นอก Windows Terminal";
        let stop_en = "Stop loading the theme in cmd windows outside Windows Terminal";
        let mut r = req(Target::Wt);
        r.replace_foreign_clink = true;
        // Only another Clink goes: nothing of ours was loading the theme there.
        let mut m = machine();
        m.autorun_hkcu = RegText::Text(r#""C:\clink\clink.bat" inject & echo hi"#.to_string());
        let p = plan(&terracotta(), &r, &m, &paths()).unwrap();
        assert_eq!(autorun(&p), Some(Some("echo hi".to_string())));
        assert!(!p.summary_th.contains(&stop_th) && !p.summary_en.contains(&stop_en));
        assert!(p
            .summary_en
            .contains(&"Turn off the cmd add-on that was set up before (can be undone)"));
        // Our entry goes.
        m.autorun_hkcu = RegText::Text(format!("{} & echo hi", entry()));
        let p = plan(&terracotta(), &r, &m, &paths()).unwrap();
        assert_eq!(autorun(&p), Some(Some("echo hi".to_string())));
        assert!(p.summary_th.contains(&stop_th) && p.summary_en.contains(&stop_en));
    }

    #[test]
    fn summaries_use_plain_words() {
        // Spec §4.5: the list of changes the wizard shows has no technical terms, and Clink
        // and Oh My Posh are "the add-on".
        let mut m = machine();
        m.data_short = None;
        m.program_data_dir = Some(PathBuf::from(PROGRAM_DATA));
        m.fonts.clear();
        // An older entry of ours and another Clink: every AutoRun line, for both targets.
        m.autorun_hkcu = RegText::Text(format!(r#"{} & "C:\clink\clink.bat" inject"#, entry()));
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

    #[test]
    fn the_autorun_summary_sorts_each_command_of_the_value() {
        let mut m = machine();
        let anaconda = r#"if exist "C:\Users\me\anaconda3\condabin\conda_hook.bat" "C:\Users\me\anaconda3\condabin\conda_hook.bat""#;
        let other_clink = r#""C:\clink\clink_x64.exe" inject --autorun"#;
        m.autorun_hkcu = RegText::Text(format!("{} & {anaconda} & {other_clink}", entry()));
        m.autorun_hklm = RegText::Text(
            r#""C:\Program Files\clink\clink_x64.exe" inject --autorun --quiet"#.to_string(),
        );
        let s = autorun_summary(&m, &paths());
        assert!(s.ours_present);
        assert_eq!(s.other_commands, vec![anaconda.to_string()]);
        assert_eq!(s.foreign_clink, vec![other_clink.to_string()]);
        assert!(s.machine_wide_clink);
        assert!(!s.not_text);
        // A command that only mentions Clink is another program's own, not a Clink.
        m.autorun_hkcu = RegText::Text(r"cd /d C:\clink-notes".to_string());
        m.autorun_hklm = RegText::Absent;
        let s = autorun_summary(&m, &paths());
        assert_eq!(s.other_commands, vec![r"cd /d C:\clink-notes".to_string()]);
        assert!(s.foreign_clink.is_empty() && !s.machine_wide_clink && !s.ours_present);
        // Nothing, and a value that is not text.
        let mut m = machine();
        assert_eq!(autorun_summary(&m, &paths()), AutoRunSummary::default());
        m.autorun_hkcu = RegText::NotText;
        assert!(autorun_summary(&m, &paths()).not_text);
    }

    #[test]
    fn the_apps_paths_are_under_the_data_folder() {
        let p = paths();
        assert_eq!(p.drafts(), PathBuf::from(format!(r"{DATA}\drafts")));
        assert_eq!(p.logs(), PathBuf::from(format!(r"{DATA}\logs")));
        assert_eq!(p.webview(), PathBuf::from(format!(r"{DATA}\webview")));
        assert_eq!(
            p.settings_file(),
            PathBuf::from(format!(r"{DATA}\settings.json"))
        );
    }
}
