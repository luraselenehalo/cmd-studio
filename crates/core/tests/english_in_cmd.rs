//! Guard of the founder's rule (2026-10-13): English is CMD Studio's main language, and nothing
//! CMD Studio itself writes into a cmd window is Thai. Thai is for the app's own UI only.
//!
//! The files the generators write are what a cmd window shows: the welcome box (`banner.lua`),
//! the prompt (`init.lua` and the Oh My Posh config) and Clink's settings. For every built-in
//! preset, every layout and the transient prompt on and off, none of them may hold a Thai
//! character (U+0E00 to U+0E7F). The themes hold none and the paths are ASCII, so a Thai
//! character there could only be text of CMD Studio's own. A user's own Thai name, folder or
//! theme text is not CMD Studio's text: it is written as it is, and other tests cover that.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use cmdstudio_core::generate::lua::{banner_lua, init_lua};
use cmdstudio_core::generate::{clink, omp};
use cmdstudio_core::model::Layout;
use cmdstudio_core::presets;
use cmdstudio_core::preview::{render_banner, render_prompt, PreviewContext};
use std::path::PathBuf;

/// `<data>` of a user whose name is ASCII.
const DATA: &str = r"C:\Users\tester\AppData\Local\ai.resincore.cmdstudio";

/// A path under `<data>`, joined as text so that it is the same on every platform.
fn under_data(tail: &str) -> PathBuf {
    PathBuf::from(format!(r"{DATA}\{tail}"))
}

/// The Thai characters in `text`, in order.
fn thai(text: &str) -> String {
    text.chars()
        .filter(|c| ('\u{0E00}'..='\u{0E7F}').contains(c))
        .collect()
}

#[test]
fn no_file_that_cmd_shows_holds_a_thai_character() {
    assert!(DATA.is_ascii());
    let omp_exe = under_data(r"runtime\omp-31.5.0\oh-my-posh.exe");
    let omp_config = under_data(r"generated\theme.omp.json");
    let scripts = under_data("generated");
    let presets = presets::builtin().unwrap();
    assert!(!presets.is_empty());
    let mut found = Vec::new();
    for (key, preset) in presets {
        // The input holds no Thai, so whatever Thai comes out is CMD Studio's own.
        let file = String::from_utf8(preset.to_file_bytes().unwrap()).unwrap();
        assert_eq!(thai(&file), "", "the preset {key} itself holds Thai");
        for layout in [Layout::Box, Layout::Line, Layout::Powerline] {
            for transient in [true, false] {
                let mut theme = preset.clone();
                theme.prompt.layout = layout;
                theme.prompt.transient = transient;
                let files = [
                    ("banner.lua", banner_lua(&theme)),
                    ("init.lua", init_lua(&omp_exe, &omp_config)),
                    ("clink_settings", clink::settings(transient, &scripts)),
                    ("theme.omp.json", omp::config_text(&theme).unwrap()),
                ];
                for (name, text) in files {
                    let letters = thai(&text);
                    if !letters.is_empty() {
                        found.push(format!(
                            "{key}, {layout:?}, transient {transient}: {name} holds \"{letters}\""
                        ));
                    }
                }
            }
        }
    }
    assert!(
        found.is_empty(),
        "Thai in what CMD Studio writes for cmd:\n{}",
        found.join("\n")
    );
}

#[test]
fn the_preview_of_every_preset_holds_no_thai_character() {
    // The preview shows what cmd shows (spec §5.6), so the same rule holds for it. The sample's
    // names, paths, branch and command are ASCII, so a Thai character in a row could only be
    // CMD Studio's own: the date, the tips, the frame, the prompt.
    let ctx = PreviewContext::sample();
    for text in [&ctx.user, &ctx.host, &ctx.home, &ctx.cwd, &ctx.command] {
        assert!(text.is_ascii(), "the sample is not ASCII: {text:?}");
    }
    assert!(ctx.git.as_ref().is_some_and(|git| git.branch.is_ascii()));
    let presets = presets::builtin().unwrap();
    assert!(!presets.is_empty());
    let mut found = Vec::new();
    for (key, preset) in presets {
        for layout in [Layout::Box, Layout::Line, Layout::Powerline] {
            for transient in [true, false] {
                let mut theme = preset.clone();
                theme.prompt.layout = layout;
                theme.prompt.transient = transient;
                let rows = render_banner(&theme, &ctx)
                    .into_iter()
                    .chain(render_prompt(&theme, &ctx));
                for (i, row) in rows.enumerate() {
                    let letters = thai(&row.text());
                    if !letters.is_empty() {
                        found.push(format!(
                            "{key}, {layout:?}, transient {transient}: row {} holds \"{letters}\"",
                            i + 1
                        ));
                    }
                }
            }
        }
    }
    assert!(
        found.is_empty(),
        "Thai in what the preview shows for cmd:\n{}",
        found.join("\n")
    );
}
