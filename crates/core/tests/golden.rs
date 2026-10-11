//! Golden snapshots of every generator (spec §7: a "unit + snapshot test" for each file the
//! generators write).
//!
//! Each test makes one generated file from fixed inputs and compares it, character for
//! character, with the file of the same name under `tests/golden/`. The inputs are the
//! Terracotta preset and the paths of the Thai test user of spec §7 (VM-C), so the paths have
//! Thai letters and a space in them; only `<clink-launch>` is all ASCII, as it always is
//! (spec §5.4). Nothing here reads the machine or the clock, and a test that fails names the
//! file and the first line that differs.
//!
//! A plain run only reads the golden files. With `UPDATE_GOLDEN=1` each test writes its file
//! first and compares after, so a file that could not be written still fails:
//!
//! ```text
//! UPDATE_GOLDEN=1 cargo test -p cmdstudio-core --test golden
//! ```
//!
//! After a change to a generator, run that once and read the diff of the golden files before
//! committing them: the diff is the change to what CMD Studio writes on a user's machine.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use cmdstudio_core::generate::lua::{self, Lang};
use cmdstudio_core::generate::{clink, omp, wt};
use cmdstudio_core::model::{Layout, Segment, Target, Theme};
use cmdstudio_core::presets;
use serde_json::Value;
use std::path::PathBuf;

/// `<data>`: the data folder of the Thai test user (spec §5.3, §7).
const DATA: &str = r"C:\Users\ผู้ทดสอบ ไทย\AppData\Local\ai.resincore.cmdstudio";

/// A path under `<data>`. It is joined as text, so the golden files come out the same on every
/// platform.
fn under_data(tail: &str) -> PathBuf {
    PathBuf::from(format!(r"{DATA}\{tail}"))
}

/// `<clink-launch>` as step 2 of spec §5.4 makes it: the 8.3 path of `<data>`, joined to the
/// rest. It is all ASCII, because Clink cannot start from a path with Thai letters in it (G19).
fn clink_launch() -> PathBuf {
    PathBuf::from(r"C:\Users\3607~1\AppData\Local\AIRESI~1.CMD\runtime\clink-1.9.34\clink_x64.exe")
}

/// The Terracotta preset, picked by its key.
fn terracotta() -> Theme {
    presets::builtin()
        .unwrap()
        .into_iter()
        .find(|(key, _)| *key == "terracotta")
        .unwrap()
        .1
}

/// Terracotta with this layout; its segments stay as the preset lists them.
fn with_layout(layout: Layout) -> Theme {
    let mut theme = terracotta();
    theme.prompt.layout = layout;
    theme
}

/// Terracotta in the line layout with all seven segments, in an order that is not the order of
/// the left side: time comes first in the list and is drawn last, and `user` and `host` are
/// listed apart and are drawn as one `session` segment.
fn every_segment() -> Theme {
    let mut theme = with_layout(Layout::Line);
    theme.prompt.segments = vec![
        Segment::Time,
        Segment::Status,
        Segment::User,
        Segment::Path,
        Segment::Duration,
        Segment::Host,
        Segment::Git,
    ];
    theme
}

/// The Oh My Posh config of a theme that passes `Theme::validate`, as the file holds it.
fn omp_text(theme: &Theme) -> String {
    theme.validate().unwrap();
    omp::config_text(theme).unwrap()
}

/// JSON the way CMD Studio writes it: pretty-printed, with one final newline.
fn json_text(value: &Value) -> String {
    let mut text = serde_json::to_string_pretty(value).unwrap();
    text.push('\n');
    text
}

/// The keys CMD Studio sets on the cmd profile of Windows Terminal, for this target.
fn wt_profile(target: Target) -> String {
    let keys = wt::profile_values(
        &terracotta(),
        target,
        &clink_launch(),
        &under_data("clink-profile"),
    );
    json_text(&Value::Object(keys))
}

// ---- the comparison ----

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
}

/// Whether this run writes the golden files. Only the exact value `1` does, so that
/// `UPDATE_GOLDEN=0` cannot rewrite them by mistake.
fn updating() -> bool {
    std::env::var("UPDATE_GOLDEN").is_ok_and(|value| value == "1")
}

/// A line cut to a length that fits in a failure message, written the way `{:?}` writes it.
fn shown(line: &str) -> String {
    const MAX_CHARS: usize = 100;
    let head: String = line.chars().take(MAX_CHARS).collect();
    let more = if line.chars().nth(MAX_CHARS).is_some() {
        "..."
    } else {
        ""
    };
    format!("{head:?}{more}")
}

/// Where two texts first differ, in a sentence short enough for a failure message.
fn first_difference(golden: &str, generated: &str) -> String {
    let mut golden_lines = golden.lines();
    let mut generated_lines = generated.lines();
    let mut number = 1;
    loop {
        match (golden_lines.next(), generated_lines.next()) {
            (Some(g), Some(n)) if g == n => number += 1,
            (Some(g), Some(n)) => {
                return format!(
                    "line {number}: the golden file has {}, the generator made {}",
                    shown(g),
                    shown(n)
                )
            }
            (Some(g), None) => {
                return format!(
                    "line {number}: the golden file has {}, and the generator's text has ended",
                    shown(g)
                )
            }
            (None, Some(n)) => {
                return format!(
                    "line {number}: the golden file has ended, and the generator made {}",
                    shown(n)
                )
            }
            (None, None) => {
                return "the lines are the same, so the end of the text differs (a final newline)"
                    .to_string()
            }
        }
    }
}

/// How `generated` compares with the golden file `name`, whose text is `golden` (`None` when
/// there is no such file). `Err` is the failure message. A golden file with CRLF line ends (a
/// checkout that was not told to keep LF) is read as if it had LF.
fn verdict(name: &str, golden: Option<&str>, generated: &str) -> Result<(), String> {
    let Some(golden) = golden else {
        return Err(format!(
            "missing golden file {name}: run with UPDATE_GOLDEN=1 once"
        ));
    };
    let golden = golden.replace("\r\n", "\n");
    if golden == generated {
        return Ok(());
    }
    Err(format!(
        "golden file {name} is not what the generator makes. First difference, {}.\n\
         If the change is meant: run with UPDATE_GOLDEN=1, read the diff of the golden \
         files and commit it.",
        first_difference(&golden, generated)
    ))
}

/// Compares `generated` with the golden file `name`. When the run is an update, the file is
/// written first, and the comparison that follows still has to pass.
fn check(name: &str, generated: &str) {
    let path = golden_dir().join(name);
    if updating() {
        std::fs::create_dir_all(golden_dir()).unwrap();
        std::fs::write(&path, generated)
            .unwrap_or_else(|e| panic!("cannot write golden file {name}: {e}"));
    }
    let golden = match std::fs::read_to_string(&path) {
        Ok(text) => Some(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => panic!("cannot read golden file {name}: {e}"),
    };
    if let Err(message) = verdict(name, golden.as_deref(), generated) {
        panic!("{message}");
    }
}

/// One test for each golden file: `test_name: "file name" = text of the file;`. The names of
/// the files are also collected in `FILES`, for the test that looks for strays.
macro_rules! golden_tests {
    ($($test:ident: $file:literal = $text:expr;)*) => {
        $(
            #[test]
            fn $test() {
                check($file, &$text);
            }
        )*

        /// The golden files that have a test.
        const FILES: &[&str] = &[$($file),*];
    };
}

golden_tests! {
    // the Oh My Posh config, spec §3.5: one file for each layout, and one with every segment
    box_prompt: "terracotta.box.omp.json" = omp_text(&with_layout(Layout::Box));
    line_prompt: "terracotta.line.omp.json" = omp_text(&with_layout(Layout::Line));
    powerline_prompt: "terracotta.powerline.omp.json" = omp_text(&with_layout(Layout::Powerline));
    every_segment_in_a_line_prompt: "terracotta.all-segments.line.omp.json" = omp_text(&every_segment());

    // the Clink scripts and settings, spec §3.6 and §5.4
    init_script: "init.lua" = lua::init_lua(
        &under_data(r"runtime\omp-31.5.0\oh-my-posh.exe"),
        &under_data(r"generated\theme.omp.json"),
    );
    thai_banner: "terracotta.banner.th.lua" = lua::banner_lua(&terracotta(), Lang::Th);
    english_banner: "terracotta.banner.en.lua" = lua::banner_lua(&terracotta(), Lang::En);
    clink_settings: "terracotta.clink_settings" = clink::settings(true, &under_data("generated"));
    clink_settings_without_transient: "terracotta.no-transient.clink_settings" =
        clink::settings(false, &under_data("generated"));

    // the Windows Terminal values, spec §5.4
    wt_scheme: "terracotta.wt.scheme.json" = json_text(&wt::scheme(&terracotta()));
    wt_profile_for_every_cmd_window: "terracotta.wt.profile.all.json" = wt_profile(Target::All);
    wt_profile_for_windows_terminal_only: "terracotta.wt.profile.wt.json" = wt_profile(Target::Wt);

    // the theme file itself, spec §3.3: what saving and exporting write
    saved_theme_file: "terracotta.cmdtheme" =
        String::from_utf8(terracotta().to_file_bytes().unwrap()).unwrap();
}

#[test]
fn the_golden_folder_holds_no_file_that_no_test_checks() {
    // A golden file whose test was renamed or removed would sit there and go stale unseen.
    let entries = match std::fs::read_dir(golden_dir()) {
        Ok(entries) => entries,
        // the tests above say that the files are missing
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
        Err(e) => panic!("cannot read the golden folder: {e}"),
    };
    let strays: Vec<String> = entries
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| !FILES.contains(&name.as_str()))
        .collect();
    assert!(
        strays.is_empty(),
        "golden files that no test checks: {strays:?}. Delete them, or add a test for them."
    );
}

// ---- the comparison itself ----
//
// These tests give `verdict` texts of their own, so they read and write no golden file, and an
// update run cannot make them touch one.

#[test]
fn a_missing_golden_file_says_how_to_make_it() {
    assert_eq!(
        verdict("terracotta.x", None, "text\n"),
        Err("missing golden file terracotta.x: run with UPDATE_GOLDEN=1 once".to_string())
    );
}

#[test]
fn a_difference_names_the_file_and_the_first_line_and_not_the_whole_texts() {
    let golden = "one\ntwo\nthree\n".repeat(1000);
    let generated = golden.replacen("two", "TWO", 1);
    let message = verdict("terracotta.x", Some(&golden), &generated).unwrap_err();
    assert!(message.contains("golden file terracotta.x "), "{message}");
    assert!(
        message.contains("line 2: the golden file has \"two\", the generator made \"TWO\""),
        "{message}"
    );
    assert!(message.contains("UPDATE_GOLDEN=1"), "{message}");
    assert!(message.len() < 400, "{} bytes: {message}", message.len());

    // a line that is far too long to print is cut, in the middle of Thai letters as well
    let long = "ก".repeat(500);
    let message = verdict("x", Some(&format!("{long}\n")), "short\n").unwrap_err();
    assert!(message.contains("line 1:"), "{message}");
    assert!(message.contains("...,"), "{message}");
    assert!(message.len() < 700, "{} bytes", message.len());
}

#[test]
fn a_text_that_ends_early_or_late_is_told_from_one_that_differs() {
    let ended = |golden: &str, generated: &str| verdict("x", Some(golden), generated).unwrap_err();
    assert!(ended("a\nb\n", "a\n")
        .contains("line 2: the golden file has \"b\", and the generator's text has ended"));
    assert!(ended("a\n", "a\nb\n")
        .contains("line 2: the golden file has ended, and the generator made \"b\""));
    // the same lines, and only the final newline is missing
    assert!(ended("a\nb\n", "a\nb").contains("final newline"));
    assert!(ended("a\nb", "a\nb\n").contains("final newline"));
}

#[test]
fn a_golden_file_with_crlf_line_ends_is_read_as_lf() {
    let generated = "a\nb\n";
    assert_eq!(verdict("x", Some("a\r\nb\r\n"), generated), Ok(()));
    assert_eq!(verdict("x", Some(generated), generated), Ok(()));
    // and the line numbers are the same ones
    let message = verdict("x", Some("a\r\nb\r\nc\r\n"), "a\nb\nX\n").unwrap_err();
    assert!(message.contains("line 3:"), "{message}");
}
