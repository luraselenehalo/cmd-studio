//! Command-line parsing (spec §5.7). Hand-written so every mistake maps to
//! exit code 2 with the usage text, in English and in Thai (spec §5.9).
use cmdstudio_core::generate::lua::Lang;
use cmdstudio_core::model::Target;
use std::ffi::OsString;
use std::path::PathBuf;

pub const USAGE: &str = "\
Usage:
  cmdstudio apply <file.cmdtheme> [--target all|wt] [--replace-other-clink] [--lang th|en]
  cmdstudio doctor [--json]
  cmdstudio undo
  cmdstudio --cleanup [--remove-themes]
  cmdstudio set-aside <restore point id>

apply:
  --target all|wt          every cmd window (the default), or Windows Terminal only
  --replace-other-clink    turn off another Clink that starts from AutoRun (undo turns it back on)
  --lang th|en             language of the welcome box and of these messages (default: Windows' language)
set-aside: only when doctor says so, for a restore point that cannot be read any more.

Options for every command:
  --data-dir <dir>    use this data folder instead of %LOCALAPPDATA%\\ai.resincore.cmdstudio
  --resources <dir>   use these bundled files instead of <exe folder>\\resources
  Both take a full path, such as C:\\folder.

Exit codes:
  0  done
  1  failed and rolled back, or refused before your settings were changed
  2  invalid input
  3  rollback incomplete: fix by hand what it lists
";

/// [`USAGE`] in Thai.
pub const USAGE_TH: &str = "\
วิธีใช้:
  cmdstudio apply <file.cmdtheme> [--target all|wt] [--replace-other-clink] [--lang th|en]
  cmdstudio doctor [--json]
  cmdstudio undo
  cmdstudio --cleanup [--remove-themes]
  cmdstudio set-aside <รหัสจุดย้อนกลับ>

apply:
  --target all|wt          ใช้กับหน้าต่าง cmd ทุกหน้าต่าง (ค่าเริ่มต้น) หรือเฉพาะใน Windows Terminal
  --replace-other-clink    ปิด Clink ตัวอื่นที่เริ่มจาก AutoRun (undo จะเปิดกลับให้)
  --lang th|en             ภาษาของกล่องต้อนรับและของข้อความเหล่านี้ (ค่าเริ่มต้น: ภาษาของ Windows)
set-aside: ใช้เฉพาะเมื่อ doctor บอกให้ใช้ กับจุดย้อนกลับที่อ่านไม่ได้อีกแล้ว

ตัวเลือกที่ใช้ได้กับทุกคำสั่ง:
  --data-dir <dir>    ใช้โฟลเดอร์ข้อมูลนี้แทน %LOCALAPPDATA%\\ai.resincore.cmdstudio
  --resources <dir>   ใช้ไฟล์ที่แนบมาจากโฟลเดอร์นี้แทน <โฟลเดอร์ของ exe>\\resources
  ทั้งสองตัวต้องเป็น path เต็ม เช่น C:\\folder

exit code:
  0  สำเร็จ
  1  ล้มเหลวและย้อนกลับแล้ว หรือถูกปฏิเสธก่อนเปลี่ยนการตั้งค่าของคุณ
  2  input ไม่ถูกต้อง
  3  ย้อนกลับไม่ครบ ต้องแก้ด้วยมือตามรายการที่แจ้ง
";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    Apply {
        theme: PathBuf,
        target: Target,
        replace_other_clink: bool,
        lang: Option<Lang>,
    },
    Doctor {
        json: bool,
    },
    Undo,
    Cleanup {
        remove_themes: bool,
    },
    /// Sets aside a restore point that cannot be read (doctor's R1 names it), so that commands
    /// that change the machine are no longer refused because of it.
    SetAside {
        id: String,
    },
    Help,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Args {
    pub command: Command,
    /// A full path (a relative one is refused).
    pub data_dir: Option<PathBuf>,
    /// A full path (a relative one is refused).
    pub resources: Option<PathBuf>,
}

/// A mistake on the command line, in English and in Thai.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{en}")]
pub struct UsageError {
    pub en: String,
    pub th: String,
}

fn bad(en: impl Into<String>, th: impl Into<String>) -> UsageError {
    UsageError {
        en: en.into(),
        th: th.into(),
    }
}

fn twice(opt: &str) -> UsageError {
    bad(
        format!("{opt} is given twice"),
        format!("ใส่ {opt} ซ้ำสองครั้ง"),
    )
}

/// Keeps the folder of `--data-dir` or `--resources`. It must be a full path: a relative data
/// folder would go into AutoRun as it is, so cmd would look for Clink from wherever a window
/// starts, and the programs CMD Studio runs are allowed only by their full path.
fn set_folder(slot: &mut Option<PathBuf>, opt: &str, value: OsString) -> Result<(), UsageError> {
    if slot.is_some() {
        return Err(twice(opt));
    }
    let path = PathBuf::from(value);
    if !path.is_absolute() {
        let shown = path.display();
        return Err(bad(
            format!("{opt} needs a full path, such as C:\\folder; \"{shown}\" is not one"),
            format!("{opt} ต้องเป็น path เต็ม เช่น C:\\folder แต่ \"{shown}\" ไม่ใช่"),
        ));
    }
    *slot = Some(path);
    Ok(())
}

/// Parses the arguments after the program name.
pub fn parse(args: &[OsString]) -> Result<Args, UsageError> {
    let mut data_dir = None;
    let mut resources = None;
    let mut words: Vec<OsString> = Vec::new();
    let mut flags: Vec<String> = Vec::new();
    let mut values: Vec<(String, OsString)> = Vec::new();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.to_str() {
            Some(opt @ ("--data-dir" | "--resources" | "--target" | "--lang")) => {
                let value = it
                    .next()
                    .ok_or_else(|| {
                        bad(
                            format!("{opt} needs a value"),
                            format!("{opt} ต้องมีค่าตามหลัง"),
                        )
                    })?
                    .clone();
                match opt {
                    "--data-dir" => set_folder(&mut data_dir, opt, value)?,
                    "--resources" => set_folder(&mut resources, opt, value)?,
                    _ if values.iter().any(|(name, _)| name == opt) => return Err(twice(opt)),
                    _ => values.push((opt.to_string(), value)),
                }
            }
            Some(flag) if flag.starts_with('-') => flags.push(flag.to_string()),
            _ => words.push(arg.clone()),
        }
    }
    let take_flag = |flags: &mut Vec<String>, name: &str| -> bool {
        let before = flags.len();
        flags.retain(|f| f != name);
        flags.len() != before
    };
    let command = if take_flag(&mut flags, "--help") | take_flag(&mut flags, "-h") {
        Command::Help
    } else if take_flag(&mut flags, "--cleanup") {
        if !words.is_empty() {
            return Err(bad(
                "--cleanup takes no command",
                "--cleanup ใช้ร่วมกับคำสั่งอื่นไม่ได้",
            ));
        }
        Command::Cleanup {
            remove_themes: take_flag(&mut flags, "--remove-themes"),
        }
    } else {
        let (first, rest) = words
            .split_first()
            .ok_or_else(|| bad("no command given", "ไม่ได้ใส่คำสั่ง"))?;
        match first.to_str() {
            Some("apply") => {
                let [theme] = rest else {
                    return Err(bad(
                        "apply needs exactly one theme file",
                        "apply ต้องมีไฟล์ธีมหนึ่งไฟล์พอดี",
                    ));
                };
                let mut target = Target::All;
                let mut lang = None;
                for (name, value) in values.drain(..) {
                    match (name.as_str(), value.to_str()) {
                        ("--target", Some("all")) => target = Target::All,
                        ("--target", Some("wt")) => target = Target::Wt,
                        ("--lang", Some("th")) => lang = Some(Lang::Th),
                        ("--lang", Some("en")) => lang = Some(Lang::En),
                        (name, _) => {
                            return Err(bad(
                                format!("{name} does not take \"{}\"", value.to_string_lossy()),
                                format!("{name} ไม่รับค่า \"{}\"", value.to_string_lossy()),
                            ))
                        }
                    }
                }
                Command::Apply {
                    theme: PathBuf::from(theme),
                    target,
                    replace_other_clink: take_flag(&mut flags, "--replace-other-clink"),
                    lang,
                }
            }
            Some(command @ ("doctor" | "undo")) => {
                if let Some(extra) = rest.first() {
                    return Err(bad(
                        format!("{command} does not take \"{}\"", extra.to_string_lossy()),
                        format!("{command} ไม่รับ \"{}\"", extra.to_string_lossy()),
                    ));
                }
                if command == "doctor" {
                    Command::Doctor {
                        json: take_flag(&mut flags, "--json"),
                    }
                } else {
                    Command::Undo
                }
            }
            Some("set-aside") => {
                let [id] = rest else {
                    return Err(bad(
                        "set-aside needs exactly one restore point id",
                        "set-aside ต้องมีรหัสจุดย้อนกลับหนึ่งรหัสพอดี",
                    ));
                };
                let Some(id) = id.to_str() else {
                    return Err(bad(
                        format!("\"{}\" is not a restore point id", id.to_string_lossy()),
                        format!("\"{}\" ไม่ใช่รหัสจุดย้อนกลับ", id.to_string_lossy()),
                    ));
                };
                Command::SetAside { id: id.to_string() }
            }
            _ => {
                return Err(bad(
                    format!("unknown command \"{}\"", first.to_string_lossy()),
                    format!("ไม่รู้จักคำสั่ง \"{}\"", first.to_string_lossy()),
                ))
            }
        }
    };
    if let Some(f) = flags.first() {
        return Err(bad(
            format!("unknown option {f}"),
            format!("ไม่รู้จักตัวเลือก {f}"),
        ));
    }
    if let Some((name, _)) = values.first() {
        return Err(bad(
            format!("{name} does not belong to this command"),
            format!("คำสั่งนี้ไม่ใช้ {name}"),
        ));
    }
    Ok(Args {
        command,
        data_dir,
        resources,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &[&str]) -> Result<Args, UsageError> {
        parse(&s.iter().map(OsString::from).collect::<Vec<_>>())
    }

    #[test]
    fn the_four_commands_parse() {
        assert_eq!(
            p(&["apply", r"C:\t\ธีม ของฉัน.cmdtheme"]).unwrap().command,
            Command::Apply {
                theme: PathBuf::from(r"C:\t\ธีม ของฉัน.cmdtheme"),
                target: Target::All,
                replace_other_clink: false,
                lang: None
            }
        );
        assert_eq!(
            p(&[
                "apply",
                "x.cmdtheme",
                "--target",
                "wt",
                "--replace-other-clink",
                "--lang",
                "th"
            ])
            .unwrap()
            .command,
            Command::Apply {
                theme: PathBuf::from("x.cmdtheme"),
                target: Target::Wt,
                replace_other_clink: true,
                lang: Some(Lang::Th)
            }
        );
        assert_eq!(
            p(&["doctor", "--json"]).unwrap().command,
            Command::Doctor { json: true }
        );
        assert_eq!(p(&["undo"]).unwrap().command, Command::Undo);
        assert_eq!(
            p(&["--cleanup", "--remove-themes"]).unwrap().command,
            Command::Cleanup {
                remove_themes: true
            }
        );
        assert_eq!(p(&["--help"]).unwrap().command, Command::Help);
    }

    #[test]
    fn global_options_go_anywhere() {
        let a = p(&["--data-dir", r"C:\d", "doctor", "--resources", r"C:\r"]).unwrap();
        assert_eq!(a.data_dir, Some(PathBuf::from(r"C:\d")));
        assert_eq!(a.resources, Some(PathBuf::from(r"C:\r")));
    }

    #[test]
    fn mistakes_are_usage_errors() {
        for bad in [
            vec![],
            vec!["apply"],
            vec!["apply", "a", "b"],
            vec!["apply", "a", "--target", "everywhere"],
            vec!["apply", "a", "--target"],
            vec!["doctor", "--fast"],
            vec!["doctor", "--target", "wt"],
            vec!["undo", "now"],
            vec!["--cleanup", "undo"],
            vec!["remove"],
        ] {
            assert!(p(&bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn set_aside_takes_one_restore_point_id() {
        assert_eq!(
            p(&["set-aside", "1791694709000-abcdef12"]).unwrap().command,
            Command::SetAside {
                id: "1791694709000-abcdef12".to_string()
            }
        );
        for bad in [
            vec!["set-aside"],
            vec!["set-aside", "a", "b"],
            vec!["set-aside", "a", "--json"],
            vec!["--cleanup", "set-aside", "a"],
        ] {
            assert!(p(&bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn the_data_and_resources_folders_need_a_full_path() {
        // A relative data folder would go into AutoRun as it is, and depend on the folder each
        // cmd window starts in.
        for opt in ["--data-dir", "--resources"] {
            for relative in [r"rel\dir", r"..\up", r"\no-drive", "C:no-root"] {
                let e = p(&["doctor", opt, relative]).unwrap_err();
                assert!(e.en.contains(opt) && e.en.contains("full path"), "{e:?}");
                assert!(e.th.contains(opt) && e.th.contains("path เต็ม"), "{e:?}");
            }
            assert!(p(&["doctor", opt, r"C:\abs"]).is_ok());
            assert!(p(&["doctor", opt, r"\\server\share\dir"]).is_ok());
        }
    }

    #[test]
    fn an_option_given_twice_is_a_mistake() {
        for bad in [
            vec!["doctor", "--data-dir", r"C:\a", "--data-dir", r"C:\b"],
            vec!["doctor", "--resources", r"C:\a", "--resources", r"C:\a"],
            vec!["apply", "x", "--target", "wt", "--target", "all"],
            vec!["apply", "x", "--lang", "th", "--lang", "en"],
        ] {
            assert!(p(&bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn every_mistake_is_said_in_thai_and_english() {
        let e = p(&["frobnicate"]).unwrap_err();
        assert_eq!(e.en, "unknown command \"frobnicate\"");
        assert_eq!(e.th, "ไม่รู้จักคำสั่ง \"frobnicate\"");
        assert_eq!(e.to_string(), e.en);
        for bad in [
            vec![],
            vec!["apply"],
            vec!["apply", "a", "--target"],
            vec!["apply", "a", "--target", "x"],
            vec!["doctor", "now"],
            vec!["doctor", "--fast"],
            vec!["undo", "--target", "wt"],
            vec!["--cleanup", "undo"],
        ] {
            let e = p(&bad).unwrap_err();
            assert!(!e.en.is_empty() && !e.th.is_empty(), "{bad:?}");
            assert!(
                e.th.chars().any(|c| ('\u{0E00}'..='\u{0E7F}').contains(&c)),
                "{e:?}"
            );
        }
    }

    #[test]
    fn what_was_typed_is_shown_as_it_was_typed() {
        // Thai vowels and tone marks are characters Rust's `{:?}` would escape.
        for (args, en, th) in [
            (
                vec!["ลบธีม"],
                "unknown command \"ลบธีม\"",
                "ไม่รู้จักคำสั่ง \"ลบธีม\"",
            ),
            (
                vec!["apply", "x", "--target", "ทุกที่"],
                "--target does not take \"ทุกที่\"",
                "--target ไม่รับค่า \"ทุกที่\"",
            ),
            (
                vec!["undo", "เดี๋ยวนี้"],
                "undo does not take \"เดี๋ยวนี้\"",
                "undo ไม่รับ \"เดี๋ยวนี้\"",
            ),
        ] {
            let e = p(&args).unwrap_err();
            assert_eq!((e.en.as_str(), e.th.as_str()), (en, th), "{args:?}");
        }
    }

    #[test]
    fn no_usage_line_stops_mid_sentence() {
        for usage in [USAGE, USAGE_TH] {
            for line in usage.lines() {
                assert!(!line.trim_end().ends_with([',', ';', '\'']), "{line}");
            }
        }
        let lang = |usage: &'static str| {
            usage
                .lines()
                .find(|l| l.trim_start().starts_with("--lang"))
                .unwrap()
        };
        assert!(
            lang(USAGE).ends_with("(default: Windows' language)"),
            "{}",
            lang(USAGE)
        );
        assert!(
            lang(USAGE_TH).ends_with("(ค่าเริ่มต้น: ภาษาของ Windows)"),
            "{}",
            lang(USAGE_TH)
        );
        // A refusal may come after a leftover font file of CMD Studio's was deleted, so exit
        // code 1 promises only that the user's settings were not changed.
        assert!(USAGE.contains(
            "\n  1  failed and rolled back, or refused before your settings were changed\n"
        ));
        assert!(USAGE_TH.contains("\n  1  ล้มเหลวและย้อนกลับแล้ว หรือถูกปฏิเสธก่อนเปลี่ยนการตั้งค่าของคุณ\n"));
    }

    #[test]
    fn the_usage_text_comes_in_both_languages_and_names_set_aside() {
        assert!(USAGE.starts_with("Usage:\n"));
        assert!(USAGE_TH.starts_with("วิธีใช้:\n"));
        for usage in [USAGE, USAGE_TH] {
            for command in [
                "cmdstudio apply <file.cmdtheme>",
                "cmdstudio doctor [--json]",
                "cmdstudio undo",
                "cmdstudio --cleanup [--remove-themes]",
                "cmdstudio set-aside <",
                "--data-dir <dir>",
                "--resources <dir>",
            ] {
                assert!(usage.contains(command), "{command}: {usage}");
            }
        }
    }
}
