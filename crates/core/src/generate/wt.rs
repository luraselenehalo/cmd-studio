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
/// quotes because `cmd /k` strips the outermost pair.
///
/// Both paths are trusted as given. `%` and `"` are written unescaped, and a
/// path that is not Unicode becomes U+FFFD through `Path::display`. Callers
/// must refuse those first with E_CLINK_PATH: `plan::clink_launch` and
/// `plan::autorun_entry` must do that before this is called. `clink_launch`
/// is `<clink-launch>`, which is all ASCII for `Target::Wt` (spec §5.4, G19).
pub fn profile_values(
    theme: &Theme,
    target: Target,
    clink_launch: &Path,
    clink_profile: &Path,
) -> Map<String, Value> {
    let t = &theme.terminal;
    let cursor = match t.cursor_shape {
        CursorShape::FilledBox => "filledBox",
        // G6 proved only filledBox; bar and underscore are documented by Windows Terminal and still to be verified on the Phase 1b VMs.
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

    #[test]
    fn the_scheme_is_exactly_the_wt_object() {
        let mut t = terracotta();
        // distinct from accent and ansi[15], so the source field of each is observable
        t.colors.cursor = "#010203".parse().unwrap();
        t.colors.foreground = "#040506".parse().unwrap();
        assert_eq!(
            scheme(&t),
            json!({
                "name": "CMD Studio",
                "background": "#1f1e1d", "foreground": "#040506",
                "cursorColor": "#010203", "selectionBackground": "#4a4845",
                "black": "#3a3936", "red": "#e5736a", "green": "#8fbf7f", "yellow": "#e3b66b",
                "blue": "#7fa7d9", "purple": "#b79bdb", "cyan": "#7fc1b8", "white": "#d8d6cf",
                "brightBlack": "#8a8780", "brightRed": "#f0907f", "brightGreen": "#a8d196",
                "brightYellow": "#eecb8a", "brightBlue": "#9bbde8", "brightPurple": "#cbb4e8",
                "brightCyan": "#9dd3cb", "brightWhite": "#faf9f5"
            })
        );
    }

    #[test]
    fn the_cmd_profile_holds_exactly_these_keys() {
        let p = profile_values(&terracotta(), Target::All, Path::new("x"), Path::new("y"));
        assert_eq!(
            Value::Object(p),
            json!({
                "colorScheme": "CMD Studio",
                "font": { "face": "Cascadia Mono", "size": 12 },
                "cursorShape": "filledBox",
                "opacity": 96,
                "useAcrylic": true,
                "padding": "14, 10",
                "icon": "\u{273b}",
                "commandline": r"%SystemRoot%\System32\cmd.exe"
            })
        );
    }

    #[test]
    fn every_cursor_shape_has_its_wt_name() {
        for (shape, name) in [
            (CursorShape::FilledBox, "filledBox"),
            (CursorShape::Bar, "bar"),
            (CursorShape::Underscore, "underscore"),
        ] {
            let mut t = terracotta();
            t.terminal.cursor_shape = shape;
            let p = profile_values(&t, Target::All, Path::new("x"), Path::new("y"));
            assert_eq!(p["cursorShape"], name);
        }
    }

    #[test]
    fn the_cmd_profile_guid_is_the_stock_one() {
        assert_eq!(CMD_PROFILE_GUID, "{0caa0dad-35be-5f56-a8ff-afceeeaa6101}");
    }
}
