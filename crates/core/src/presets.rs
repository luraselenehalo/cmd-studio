//! Built-in presets, embedded at compile time from `/presets` (spec §3.2).
//! Presets from other projects are added after their licenses are confirmed
//! (spec gate G14).
use crate::model::{parse_theme, Theme, ThemeError};

const SOURCES: &[(&str, &str)] = &[(
    "terracotta",
    include_str!("../../../presets/terracotta.cmdtheme"),
)];

/// Keys of presets whose colors are our own; they must pass the
/// readability check (spec §3.2).
pub const OWN: &[&str] = &["terracotta"];

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
    use crate::palette::contrast_issues;
    use std::collections::HashSet;

    #[test]
    fn every_preset_parses_with_unique_ids_and_names() {
        let all = builtin().unwrap();
        assert_eq!(all[0].0, "terracotta");
        let ids: HashSet<_> = all.iter().map(|(_, t)| t.id).collect();
        let names: HashSet<_> = all.iter().map(|(_, t)| t.name.clone()).collect();
        assert_eq!(ids.len(), all.len());
        assert_eq!(names.len(), all.len());
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
    fn terracotta_matches_the_prototype() {
        let t = terracotta();
        assert_eq!(t.colors.background.to_string(), "#1f1e1d");
        assert_eq!(t.colors.accent.to_string(), "#d97757");
        assert_eq!(t.colors.ansi[15].to_string(), "#faf9f5");
    }
}
