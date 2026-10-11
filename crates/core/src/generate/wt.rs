//! Theme → the Windows Terminal values CMD Studio owns (spec §5.4): one
//! color scheme and the keys of the cmd profile. Patching settings.json is
//! the adapter's job (Phase 1b).
use crate::model::{CursorShape, Target, Theme};
use serde_json::{json, Map, Value};
use std::path::Path;

pub const SCHEME_NAME: &str = "CMD Studio";
pub const CMD_PROFILE_GUID: &str = "{0caa0dad-35be-5f56-a8ff-afceeeaa6101}";
pub const CMD_EXE: &str = r"%SystemRoot%\System32\cmd.exe";

const ANSI_KEYS: [&str; 16] = [
    "black",
    "red",
    "green",
    "yellow",
    "blue",
    "purple",
    "cyan",
    "white",
    "brightBlack",
    "brightRed",
    "brightGreen",
    "brightYellow",
    "brightBlue",
    "brightPurple",
    "brightCyan",
    "brightWhite",
];

pub fn scheme(theme: &Theme) -> Value {
    let c = &theme.colors;
    let mut m = Map::new();
    m.insert("name".into(), json!(SCHEME_NAME));
    m.insert("background".into(), json!(c.background.to_string()));
    m.insert("foreground".into(), json!(c.foreground.to_string()));
    m.insert("cursorColor".into(), json!(c.cursor.to_string()));
    m.insert("selectionBackground".into(), json!(c.selection.to_string()));
    for (key, color) in ANSI_KEYS.iter().zip(c.ansi.iter()) {
        m.insert((*key).into(), json!(color.to_string()));
    }
    Value::Object(m)
}

/// Keys CMD Studio sets on the cmd profile. With `Target::Wt`, cmd loads
/// Clink through `/k`. The whole command is wrapped in one more pair of
/// quotes because `cmd /k` strips the outermost pair. `clink_launch` is
/// `<clink-launch>` from the planner: all ASCII (spec §5.4, G19).
pub fn profile_values(
    theme: &Theme,
    target: Target,
    clink_launch: &Path,
    clink_profile: &Path,
) -> Map<String, Value> {
    let t = &theme.terminal;
    let cursor = match t.cursor_shape {
        CursorShape::FilledBox => "filledBox",
        // bar and underscore are documented by WT; they are verified on the Phase 1b VMs.
        CursorShape::Bar => "bar",
        CursorShape::Underscore => "underscore",
    };
    let [horizontal, vertical] = t.padding;
    let commandline = match target {
        Target::All => CMD_EXE.to_string(),
        Target::Wt => format!(
            r#"{} /k ""{}" inject --quiet --profile "{}"""#,
            CMD_EXE,
            clink_launch.display(),
            clink_profile.display()
        ),
    };
    let mut m = Map::new();
    m.insert("colorScheme".into(), json!(SCHEME_NAME));
    m.insert(
        "font".into(),
        json!({ "face": t.font, "size": t.font_size }),
    );
    m.insert("cursorShape".into(), json!(cursor));
    m.insert("opacity".into(), json!(t.opacity));
    m.insert("useAcrylic".into(), json!(t.acrylic));
    m.insert("padding".into(), json!(format!("{horizontal}, {vertical}")));
    m.insert("icon".into(), json!("\u{273b}"));
    m.insert("commandline".into(), json!(commandline));
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presets::terracotta;

    #[test]
    fn scheme_uses_wt_key_names() {
        let s = scheme(&terracotta());
        let o = s.as_object().unwrap();
        assert_eq!(o.len(), 21);
        assert_eq!(s["name"], "CMD Studio");
        assert_eq!(s["cursorColor"], "#d97757");
        assert_eq!(s["purple"], "#b79bdb");
        assert_eq!(s["brightWhite"], "#faf9f5");
    }

    #[test]
    fn all_target_keeps_a_plain_commandline() {
        let p = profile_values(
            &terracotta(),
            Target::All,
            Path::new(r"C:\c\clink_x64.exe"),
            Path::new(r"C:\c\p"),
        );
        assert_eq!(p["commandline"], r"%SystemRoot%\System32\cmd.exe");
        assert_eq!(p["colorScheme"], "CMD Studio");
        assert_eq!(p["padding"], "14, 10");
        assert_eq!(p["cursorShape"], "filledBox");
        assert_eq!(p["font"]["face"], "Cascadia Mono");
        assert_eq!(p["font"]["size"], 12);
        assert_eq!(p["icon"], "\u{273b}");
    }

    #[test]
    fn wt_target_wraps_the_inject_command_for_cmd_k() {
        // <clink-launch> is always ASCII (G19); the profile path may hold Thai letters.
        let launch = Path::new(r"C:\Users\3607~1\x\clink_x64.exe");
        let prof = Path::new(r"C:\Users\ผู้ทดสอบ ไทย\x\clink-profile");
        let p = profile_values(&terracotta(), Target::Wt, launch, prof);
        assert_eq!(
            p["commandline"],
            r#"%SystemRoot%\System32\cmd.exe /k ""C:\Users\3607~1\x\clink_x64.exe" inject --quiet --profile "C:\Users\ผู้ทดสอบ ไทย\x\clink-profile"""#
        );
    }
}
