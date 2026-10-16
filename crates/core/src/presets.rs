//! Built-in presets, embedded at compile time from `/presets` (spec §3.2). The upstream ones
//! keep their projects' colors (licenses confirmed in spec gate G14, credits in
//! `docs/credits.md`); the choices made where a project has no terminal color for a slot
//! are recorded in spec appendix B (2026-10-13).
use crate::model::{parse_theme, Theme, ThemeError};

/// Every preset by key, in the order of spec §3.2: the dark ones, then the light ones.
const SOURCES: &[(&str, &str)] = &[
    (
        "terracotta",
        include_str!("../../../presets/terracotta.cmdtheme"),
    ),
    (
        "tokyo-night",
        include_str!("../../../presets/tokyo-night.cmdtheme"),
    ),
    (
        "catppuccin-mocha",
        include_str!("../../../presets/catppuccin-mocha.cmdtheme"),
    ),
    ("dracula", include_str!("../../../presets/dracula.cmdtheme")),
    ("nord", include_str!("../../../presets/nord.cmdtheme")),
    ("gruvbox", include_str!("../../../presets/gruvbox.cmdtheme")),
    (
        "one-dark",
        include_str!("../../../presets/one-dark.cmdtheme"),
    ),
    (
        "retro-green",
        include_str!("../../../presets/retro-green.cmdtheme"),
    ),
    (
        "catppuccin-latte",
        include_str!("../../../presets/catppuccin-latte.cmdtheme"),
    ),
    (
        "solarized-light",
        include_str!("../../../presets/solarized-light.cmdtheme"),
    ),
    (
        "daylight",
        include_str!("../../../presets/daylight.cmdtheme"),
    ),
    ("paper", include_str!("../../../presets/paper.cmdtheme")),
];

/// Keys of presets whose colors are our own; they must pass the
/// readability check (spec §3.2).
pub const OWN: &[&str] = &["terracotta", "retro-green", "daylight", "paper"];

pub fn builtin() -> Result<Vec<(&'static str, Theme)>, ThemeError> {
    SOURCES
        .iter()
        .map(|(key, src)| parse_theme(src.as_bytes()).map(|t| (*key, t)))
        .collect()
}

/// The Terracotta preset from [`builtin`]. Test code only: test modules across the crate call
/// this instead of writing their own copy of the theme.
#[cfg(test)]
pub(crate) fn terracotta() -> Theme {
    builtin()
        .expect("the built-in presets parse")
        .into_iter()
        .find(|(key, _)| *key == "terracotta")
        .expect("terracotta is a built-in preset")
        .1
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Layout;
    use crate::palette::{contrast_issues, Slot, Tone};
    use std::collections::{BTreeSet, HashSet};

    /// The 23 colors of a theme in one line: background, foreground, cursor, selection,
    /// accent, muted, frame, then ansi 0 to 15.
    fn colors_of(theme: &Theme) -> String {
        let c = &theme.colors;
        let mut all = vec![
            c.background,
            c.foreground,
            c.cursor,
            c.selection,
            c.accent,
            c.muted,
            c.frame,
        ];
        all.extend(c.ansi);
        all.iter()
            .map(|rgb| rgb.to_string())
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn preset(key: &str) -> Theme {
        builtin()
            .unwrap()
            .into_iter()
            .find(|(k, _)| *k == key)
            .unwrap_or_else(|| panic!("{key} is not built in"))
            .1
    }

    #[test]
    fn every_preset_parses_with_unique_ids_and_names() {
        let all = builtin().unwrap();
        assert_eq!(all[0].0, "terracotta");
        let ids: HashSet<_> = all.iter().map(|(_, t)| t.id).collect();
        let names: HashSet<_> = all.iter().map(|(_, t)| t.name.clone()).collect();
        let keys: HashSet<_> = all.iter().map(|(key, _)| *key).collect();
        assert_eq!(ids.len(), all.len());
        assert_eq!(names.len(), all.len());
        assert_eq!(keys.len(), all.len());
    }

    #[test]
    fn the_presets_are_the_twelve_of_the_spec_in_its_order() {
        // spec §3.2, typed again here
        let want = [
            ("Terracotta", Tone::Dark),
            ("Tokyo Night", Tone::Dark),
            ("Catppuccin Mocha", Tone::Dark),
            ("Dracula", Tone::Dark),
            ("Nord", Tone::Dark),
            ("Gruvbox", Tone::Dark),
            ("One Dark", Tone::Dark),
            ("Retro green", Tone::Dark),
            ("Catppuccin Latte", Tone::Light),
            ("Solarized Light", Tone::Light),
            ("Daylight", Tone::Light),
            ("Paper", Tone::Light),
        ];
        let got: Vec<(String, Tone)> = builtin()
            .unwrap()
            .into_iter()
            .map(|(_, t)| (t.name, t.tone))
            .collect();
        let want: Vec<(String, Tone)> = want.iter().map(|(n, t)| (n.to_string(), *t)).collect();
        assert_eq!(got, want);
    }

    #[test]
    fn our_own_presets_pass_the_readability_check() {
        let all = builtin().unwrap();
        assert!(
            !OWN.is_empty(),
            "OWN names no preset, so the readability check would check nothing"
        );
        for own in OWN {
            assert!(
                all.iter().any(|(key, _)| key == own),
                "OWN names {own}, which builtin() does not return"
            );
        }
        for (key, theme) in &all {
            if OWN.contains(key) {
                let issues = contrast_issues(&theme.colors);
                assert!(issues.is_empty(), "{key}: {issues:?}");
            }
        }
    }

    #[test]
    fn upstream_presets_keep_the_upstream_colors() {
        // spec §3.2: every value as the project has it, typed again here so that a changed
        // file fails. Sources and the choices for accent, muted and frame: spec appendix B.
        for (key, colors) in [
            ("tokyo-night", "#1a1b26 #a9b1d6 #c0caf5 #2b2f41 #7aa2f7 #787c99 #414868 #363b54 #f7768e #73daca #e0af68 #7aa2f7 #bb9af7 #7dcfff #787c99 #363b54 #f7768e #73daca #e0af68 #7aa2f7 #bb9af7 #7dcfff #acb0d0"),
            ("catppuccin-mocha", "#1e1e2e #cdd6f4 #f5e0dc #585b70 #cba6f7 #9399b2 #6c7086 #45475a #f38ba8 #a6e3a1 #f9e2af #89b4fa #f5c2e7 #94e2d5 #bac2de #585b70 #f38ba8 #a6e3a1 #f9e2af #89b4fa #f5c2e7 #94e2d5 #a6adc8"),
            ("dracula", "#282a36 #f8f8f2 #f8f8f2 #44475a #bd93f9 #6272a4 #44475a #21222c #ff5555 #50fa7b #f1fa8c #bd93f9 #ff79c6 #8be9fd #f8f8f2 #6272a4 #ff6e6e #69ff94 #ffffa5 #d6acff #ff92df #a4ffff #ffffff"),
            ("nord", "#2e3440 #d8dee9 #d8dee9 #4c566a #88c0d0 #a5abb6 #3b4252 #3b4252 #bf616a #a3be8c #ebcb8b #81a1c1 #b48ead #88c0d0 #e5e9f0 #4c566a #bf616a #a3be8c #ebcb8b #81a1c1 #b48ead #8fbcbb #eceff4"),
            ("gruvbox", "#282828 #ebdbb2 #ebdbb2 #665c54 #fe8019 #928374 #665c54 #282828 #cc241d #98971a #d79921 #458588 #b16286 #689d6a #a89984 #928374 #fb4934 #b8bb26 #fabd2f #83a598 #d3869b #8ec07c #ebdbb2"),
            ("one-dark", "#282c34 #abb2bf #528bff #3e4451 #528bff #828997 #3e4451 #282c34 #e06c75 #98c379 #e5c07b #61afef #c678dd #56b6c2 #abb2bf #5c6370 #e06c75 #98c379 #e5c07b #61afef #c678dd #56b6c2 #ffffff"),
            ("catppuccin-latte", "#eff1f5 #4c4f69 #dc8a78 #acb0be #8839ef #7c7f93 #9ca0b0 #5c5f77 #d20f39 #40a02b #df8e1d #1e66f5 #ea76cb #179299 #acb0be #acb0be #d20f39 #40a02b #df8e1d #1e66f5 #ea76cb #179299 #bcc0cc"),
            ("solarized-light", "#fdf6e3 #657b83 #657b83 #eee8d5 #268bd2 #93a1a1 #839496 #073642 #dc322f #859900 #b58900 #268bd2 #d33682 #2aa198 #eee8d5 #002b36 #cb4b16 #586e75 #657b83 #839496 #6c71c4 #93a1a1 #fdf6e3"),
        ] {
            assert_eq!(colors_of(&preset(key)), colors, "{key}");
        }
    }

    #[test]
    fn every_contrast_exception_is_a_known_upstream_one() {
        // spec §3.2: an upstream color under the readability minimum stays as the project has
        // it, and is listed here. A new failure, or one that went away, fails this test.
        let known: [(&str, &[Slot]); 8] = [
            ("tokyo-night", &[]),
            ("catppuccin-mocha", &[]),
            ("dracula", &[]),
            ("nord", &[]),
            ("gruvbox", &[Slot::Ansi(1)]),
            ("one-dark", &[]),
            (
                "catppuccin-latte",
                &[
                    Slot::Ansi(2),
                    Slot::Ansi(3),
                    Slot::Ansi(5),
                    Slot::Ansi(10),
                    Slot::Ansi(11),
                    Slot::Ansi(13),
                ],
            ),
            (
                "solarized-light",
                &[
                    Slot::Foreground,
                    Slot::Muted,
                    Slot::Ansi(2),
                    Slot::Ansi(3),
                    Slot::Ansi(6),
                    Slot::Ansi(12),
                    Slot::Ansi(14),
                ],
            ),
        ];
        let all = builtin().unwrap();
        for (key, theme) in &all {
            let failing: Vec<Slot> = contrast_issues(&theme.colors)
                .iter()
                .map(|i| i.slot)
                .collect();
            match known.iter().find(|(k, _)| k == key) {
                Some((_, slots)) => assert_eq!(failing, slots.to_vec(), "{key}"),
                None => assert!(OWN.contains(key), "{key} is neither ours nor in the list"),
            }
        }
        // the list names upstream presets only, and every one of them
        assert_eq!(known.len() + OWN.len(), all.len());
        assert_eq!(known.iter().map(|(_, s)| s.len()).sum::<usize>(), 14);
    }

    #[test]
    fn presets_use_cascadia_mono_unless_their_layout_needs_the_nerd_font() {
        for (key, theme) in builtin().unwrap() {
            let want = if theme.prompt.layout == Layout::Powerline {
                "CaskaydiaCove NF"
            } else {
                "Cascadia Mono"
            };
            assert_eq!(theme.terminal.font, want, "{key}");
        }
    }

    #[test]
    fn every_preset_file_is_built_in() {
        // a file in presets/ that no key embeds would never reach the app
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../presets");
        let files: BTreeSet<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        let keys: BTreeSet<String> = SOURCES
            .iter()
            .map(|(key, _)| format!("{key}.cmdtheme"))
            .collect();
        assert_eq!(files, keys);
    }

    #[test]
    fn terracotta_matches_the_prototype() {
        let t = terracotta();
        assert_eq!(t.colors.background.to_string(), "#1f1e1d");
        assert_eq!(t.colors.accent.to_string(), "#d97757");
        assert_eq!(t.colors.ansi[15].to_string(), "#faf9f5");
    }
}
