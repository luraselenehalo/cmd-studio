//! The commands that compute without touching the machine (spec §5.6: palette, contrast and
//! prompt logic live in Rust; the UI paints spans): `preview_render`, `palette_generate`,
//! `contrast_check`, `contrast_fix`.
use crate::error::AppError;
use cmdstudio_core::color::Rgb;
use cmdstudio_core::model::Theme;
use cmdstudio_core::palette::{self, Colors, Slot, Tone};
use cmdstudio_core::preview::{render_banner, render_prompt, Line, PreviewContext};
use cmdstudio_core::text::sanitize_text;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// The rows of the mock terminal: the welcome box, then the prompt rows (spec §5.6).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PreviewResult {
    pub banner: Vec<Line>,
    pub prompt: Vec<Line>,
}

/// One color below its minimum contrast against the background (spec §3.4). `slot` is
/// `foreground`, `muted`, `accent` or `ansi<n>` (n = 0–15), the names the editor's panel uses.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ContrastIssue {
    pub slot: String,
    pub ratio: f64,
    pub required: f64,
}

/// Draws the welcome box and the prompt for a theme and the sample the UI chose. The sample's
/// texts are cleaned like a theme's (escape sequences, control characters, line breaks and
/// bidi controls go), so a user-typed sample cannot reach the spans raw.
pub fn preview_render(theme: &Theme, ctx: &PreviewContext) -> PreviewResult {
    let ctx = PreviewContext {
        user: sanitize_text(&ctx.user),
        host: sanitize_text(&ctx.host),
        home: sanitize_text(&ctx.home),
        cwd: sanitize_text(&ctx.cwd),
        command: sanitize_text(&ctx.command),
        git: ctx.git.as_ref().map(|g| cmdstudio_core::preview::GitState {
            branch: sanitize_text(&g.branch),
            dirty: g.dirty,
        }),
        ..ctx.clone()
    };
    PreviewResult {
        banner: render_banner(theme, &ctx),
        prompt: render_prompt(theme, &ctx),
    }
}

/// Simple mode (spec §3.4): all 23 colors from a tone, an accent and a tint. The accent is
/// `#rrggbb`; anything else is `E_THEME_INVALID` with the text as typed.
pub fn palette_generate(tone: Tone, accent: &str, tint: f64) -> Result<Colors, AppError> {
    let accent: Rgb = accent
        .parse()
        .map_err(|e: cmdstudio_core::color::ParseColorError| {
            AppError::coded("E_THEME_INVALID", format!("accent: {e}"))
        })?;
    if !tint.is_finite() {
        return Err(AppError::coded("E_THEME_INVALID", "tint: must be a number"));
    }
    Ok(palette::generate(tone, accent, tint))
}

fn slot_name(slot: Slot) -> String {
    match slot {
        Slot::Foreground => "foreground".to_string(),
        Slot::Muted => "muted".to_string(),
        Slot::Accent => "accent".to_string(),
        Slot::Ansi(i) => format!("ansi{i}"),
    }
}

/// Every checked color below its minimum (spec §3.4). Empty means readable.
pub fn contrast_check(colors: &Colors) -> Vec<ContrastIssue> {
    palette::contrast_issues(colors)
        .into_iter()
        .map(|i| ContrastIssue {
            slot: slot_name(i.slot),
            ratio: i.ratio,
            required: i.required,
        })
        .collect()
}

/// "Fix automatically": every checked color raised to its minimum by lightness alone.
pub fn contrast_fix(mut colors: Colors) -> Colors {
    palette::fix_all(&mut colors);
    colors
}

#[cfg(test)]
mod tests {
    use super::*;
    use cmdstudio_core::presets;

    fn terracotta() -> Theme {
        presets::builtin()
            .unwrap()
            .into_iter()
            .find(|(k, _)| *k == "terracotta")
            .unwrap()
            .1
    }

    #[test]
    fn the_preview_is_the_cores_rows_and_the_sample_is_cleaned_first() {
        let theme = terracotta();
        let sample = PreviewContext::sample();
        let drawn = preview_render(&theme, &sample);
        assert_eq!(drawn.banner, render_banner(&theme, &sample));
        assert_eq!(drawn.prompt, render_prompt(&theme, &sample));
        assert!(drawn.banner.len() >= 4 && drawn.prompt.len() >= 3);
        // An escape sequence typed into the sample command never reaches a span.
        let mut hostile = sample.clone();
        hostile.command = "dir \u{1b}[31mred\u{1b}[0m\nnext".to_string();
        hostile.user = "you\u{202e}".to_string();
        let drawn = preview_render(&theme, &hostile);
        let text: String = drawn
            .prompt
            .iter()
            .chain(&drawn.banner)
            .map(Line::text)
            .collect();
        assert!(!text.contains('\u{1b}') && !text.contains('\n') && !text.contains('\u{202e}'));
        assert!(text.contains("dir red"), "{text}");
    }

    #[test]
    fn simple_mode_generates_and_the_check_and_fix_are_the_cores() {
        let colors = palette_generate(Tone::Dark, "#d97757", 0.10).unwrap();
        assert_eq!(
            colors,
            palette::generate(Tone::Dark, "#d97757".parse().unwrap(), 0.10)
        );
        assert!(contrast_check(&colors).is_empty());
        let e = palette_generate(Tone::Light, "d97757", 0.1).unwrap_err();
        assert_eq!(e.code, "E_THEME_INVALID");
        assert!(e.details.contains("\"d97757\""), "{}", e.details);
        assert_eq!(
            palette_generate(Tone::Light, "#d97757", f64::NAN)
                .unwrap_err()
                .code,
            "E_THEME_INVALID"
        );
        // A grey text on a grey background is unreadable; the fix makes it readable.
        let mut bad = colors.clone();
        bad.foreground = bad.background;
        bad.ansi[3] = bad.background;
        let issues = contrast_check(&bad);
        let slots: Vec<&str> = issues.iter().map(|i| i.slot.as_str()).collect();
        assert_eq!(slots, ["foreground", "ansi3"]);
        assert_eq!(issues[0].required, 4.5);
        assert_eq!(issues[1].required, 3.0);
        let fixed = contrast_fix(bad);
        assert!(contrast_check(&fixed).is_empty());
    }
}
