//! Command-line parsing (spec §5.7). Hand-written so every mistake maps to
//! exit code 2 with the usage text. Everything it says is English, on every Windows.
use cmdstudio_core::model::Target;
use std::ffi::OsString;
use std::path::PathBuf;

pub const USAGE: &str = "\
Usage:
  cmdstudio apply <file.cmdtheme> [--target all|wt] [--replace-other-clink]
  cmdstudio doctor [--json]
  cmdstudio undo
  cmdstudio --cleanup [--remove-themes]
  cmdstudio set-aside <restore point id>

apply:
  --target all|wt          every cmd window (the default), or Windows Terminal only
  --replace-other-clink    turn off another Clink that starts from AutoRun (undo turns it back on)
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    Apply {
        theme: PathBuf,
        target: Target,
        replace_other_clink: bool,
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

/// A mistake on the command line.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct UsageError {
    pub message: String,
}

fn bad(message: impl Into<String>) -> UsageError {
    UsageError {
        message: message.into(),
    }
}

fn twice(opt: &str) -> UsageError {
    bad(format!("{opt} is given twice"))
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
        return Err(bad(format!(
            "{opt} needs a full path, such as C:\\folder; \"{shown}\" is not one"
        )));
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
            Some(opt @ ("--data-dir" | "--resources" | "--target")) => {
                let value = it
                    .next()
                    .ok_or_else(|| bad(format!("{opt} needs a value")))?
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
            return Err(bad("--cleanup takes no command"));
        }
        Command::Cleanup {
            remove_themes: take_flag(&mut flags, "--remove-themes"),
        }
    } else {
        let (first, rest) = words.split_first().ok_or_else(|| bad("no command given"))?;
        match first.to_str() {
            Some("apply") => {
                let [theme] = rest else {
                    return Err(bad("apply needs exactly one theme file"));
                };
                let mut target = Target::All;
                for (name, value) in values.drain(..) {
                    match (name.as_str(), value.to_str()) {
                        ("--target", Some("all")) => target = Target::All,
                        ("--target", Some("wt")) => target = Target::Wt,
                        (name, _) => {
                            return Err(bad(format!(
                                "{name} does not take \"{}\"",
                                value.to_string_lossy()
                            )))
                        }
                    }
                }
                Command::Apply {
                    theme: PathBuf::from(theme),
                    target,
                    replace_other_clink: take_flag(&mut flags, "--replace-other-clink"),
                }
            }
            Some(command @ ("doctor" | "undo")) => {
                if let Some(extra) = rest.first() {
                    return Err(bad(format!(
                        "{command} does not take \"{}\"",
                        extra.to_string_lossy()
                    )));
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
                    return Err(bad("set-aside needs exactly one restore point id"));
                };
                let Some(id) = id.to_str() else {
                    return Err(bad(format!(
                        "\"{}\" is not a restore point id",
                        id.to_string_lossy()
                    )));
                };
                Command::SetAside { id: id.to_string() }
            }
            _ => {
                return Err(bad(format!(
                    "unknown command \"{}\"",
                    first.to_string_lossy()
                )))
            }
        }
    };
    if let Some(f) = flags.first() {
        return Err(bad(format!("unknown option {f}")));
    }
    if let Some((name, _)) = values.first() {
        return Err(bad(format!("{name} does not belong to this command")));
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

    /// The Thai characters in `text`, in order.
    fn thai(text: &str) -> String {
        text.chars()
            .filter(|c| ('\u{0E00}'..='\u{0E7F}').contains(c))
            .collect()
    }

    #[test]
    fn the_four_commands_parse() {
        assert_eq!(
            p(&["apply", r"C:\t\ธีม ของฉัน.cmdtheme"]).unwrap().command,
            Command::Apply {
                theme: PathBuf::from(r"C:\t\ธีม ของฉัน.cmdtheme"),
                target: Target::All,
                replace_other_clink: false,
            }
        );
        assert_eq!(
            p(&[
                "apply",
                "x.cmdtheme",
                "--target",
                "wt",
                "--replace-other-clink"
            ])
            .unwrap()
            .command,
            Command::Apply {
                theme: PathBuf::from("x.cmdtheme"),
                target: Target::Wt,
                replace_other_clink: true,
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
            // There is no --lang any more: the CLI and the welcome box are English only.
            vec!["apply", "a", "--lang", "th"],
            vec!["apply", "a", "--lang"],
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
                assert!(
                    e.message.contains(opt) && e.message.contains("full path"),
                    "{e:?}"
                );
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
        ] {
            assert!(p(&bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn every_mistake_is_said_in_english() {
        let e = p(&["frobnicate"]).unwrap_err();
        assert_eq!(e.message, "unknown command \"frobnicate\"");
        assert_eq!(e.to_string(), e.message);
        for bad in [
            vec![],
            vec!["apply"],
            vec!["apply", "a", "--target"],
            vec!["apply", "a", "--target", "x"],
            vec!["apply", "a", "--lang", "th"],
            vec!["doctor", "now"],
            vec!["doctor", "--fast"],
            vec!["undo", "--target", "wt"],
            vec!["--cleanup", "undo"],
        ] {
            let e = p(&bad).unwrap_err();
            assert!(!e.message.is_empty(), "{bad:?}");
            assert_eq!(thai(&e.message), "", "{bad:?}: {e:?}");
        }
    }

    #[test]
    fn what_was_typed_is_shown_as_it_was_typed() {
        // Thai vowels and tone marks are characters Rust's `{:?}` would escape. What a user
        // typed is shown as it is, Thai or not: it is not text of CMD Studio's own.
        for (args, said) in [
            (vec!["ลบธีม"], "unknown command \"ลบธีม\""),
            (
                vec!["apply", "x", "--target", "ทุกที่"],
                "--target does not take \"ทุกที่\"",
            ),
            (vec!["undo", "เดี๋ยวนี้"], "undo does not take \"เดี๋ยวนี้\""),
        ] {
            let e = p(&args).unwrap_err();
            assert_eq!(e.message, said, "{args:?}");
        }
    }

    #[test]
    fn no_usage_line_stops_mid_sentence() {
        for line in USAGE.lines() {
            assert!(!line.trim_end().ends_with([',', ';', '\'']), "{line}");
        }
        // A refusal may come after a leftover font file of CMD Studio's was deleted, so exit
        // code 1 promises only that the user's settings were not changed.
        assert!(USAGE.contains(
            "\n  1  failed and rolled back, or refused before your settings were changed\n"
        ));
    }

    #[test]
    fn the_usage_text_is_english_and_names_every_command() {
        assert!(USAGE.starts_with("Usage:\n"));
        assert_eq!(thai(USAGE), "");
        for command in [
            "cmdstudio apply <file.cmdtheme>",
            "cmdstudio doctor [--json]",
            "cmdstudio undo",
            "cmdstudio --cleanup [--remove-themes]",
            "cmdstudio set-aside <",
            "--data-dir <dir>",
            "--resources <dir>",
        ] {
            assert!(USAGE.contains(command), "{command}: {USAGE}");
        }
        assert!(!USAGE.contains("--lang"), "{USAGE}");
    }
}
