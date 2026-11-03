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
    let health = under_data("health");
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
                    ("init.lua", init_lua(&omp_exe, &omp_config, &health)),
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
    let (mut found, mut too_few) = (Vec::new(), Vec::new());
    let (mut cases, mut box_rows_seen, mut prompt_rows_seen) = (0, 0, 0);
    for (key, preset) in &presets {
        for layout in [Layout::Box, Layout::Line, Layout::Powerline] {
            for transient in [true, false] {
                let mut theme = preset.clone();
                theme.prompt.layout = layout;
                theme.prompt.transient = transient;
                let banner = render_banner(&theme, &ctx);
                let prompt = render_prompt(&theme, &ctx);
                // An empty render would pass the check below without checking a row, so the
                // rows are counted: the box the theme describes has its top and bottom, its
                // title, an empty line and its lines when it has any, its tips and the closing
                // space; the prompt has the earlier command, the empty line and the prompt now.
                let b = &theme.banner;
                let box_rows = if b.enabled {
                    4 + if b.lines.is_empty() {
                        0
                    } else {
                        1 + b.lines.len()
                    } + usize::from(b.tips)
                } else {
                    0
                };
                if banner.len() < box_rows || prompt.len() < 3 {
                    too_few.push(format!(
                        "{key}, {layout:?}, transient {transient}: {} rows of the box (at least {box_rows}) and {} of the prompt (at least 3)",
                        banner.len(),
                        prompt.len()
                    ));
                }
                box_rows_seen += banner.len();
                prompt_rows_seen += prompt.len();
                for (i, row) in banner.iter().chain(&prompt).enumerate() {
                    let letters = thai(&row.text());
                    if !letters.is_empty() {
                        found.push(format!(
                            "{key}, {layout:?}, transient {transient}: row {} holds \"{letters}\"",
                            i + 1
                        ));
                    }
                }
                cases += 1;
            }
        }
    }
    assert!(
        too_few.is_empty(),
        "the preview drew too few rows to check:\n{}",
        too_few.join("\n")
    );
    assert!(
        found.is_empty(),
        "Thai in what the preview shows for cmd:\n{}",
        found.join("\n")
    );
    // every preset in every layout with the transient prompt on and off, and rows in them
    assert_eq!(cases, presets.len() * 3 * 2);
    assert!(box_rows_seen > 0 && prompt_rows_seen > 0);
}
