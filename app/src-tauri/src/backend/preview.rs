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
/// `foreground`, `muted`, `accent` or `ansi<n>` with n = 1–6 or 9–14, the slots the core checks.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ContrastIssue {
    pub slot: String,
    pub ratio: f64,
    pub required: f64,
}

/// The Windows user name, the computer name and the folder Oh My Posh takes as home: this
/// computer's own, which the preview's sample starts from. Send them back as `user`, `host` and
/// `home` of the `PreviewContext`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SampleIdentity {
    // The user name is the user's own: it goes to the UI, which is the user's own app, and
    // nowhere else. It is never logged (`Backend::logged` writes no value of a command) and the
    // diagnostics redact it. A plain comment, so that it stays out of the TypeScript binding.
    pub user: String,
    pub host: String,
    pub home: String,
}

impl SampleIdentity {
    /// The identity from an environment lookup (`std::env::var` in the real app, a table in a
    /// test). `user` is `USERNAME` and `host` is `COMPUTERNAME`. `home` is the folder Oh My Posh
    /// 31.5.0 takes as home (spec §5.6): `HOME`, else `HOMEDRIVE` followed by `HOMEPATH`, else
    /// `USERPROFILE`. A variable that is empty is not set, and `HOMEDRIVE` counts only with a
    /// `HOMEPATH` that is set too. What is missing is the core's sample value for that field, so
    /// the preview always has something to draw.
    pub fn from_env(get: impl Fn(&str) -> Option<String>) -> SampleIdentity {
        let sample = PreviewContext::sample();
        let var = |name: &str| get(name).filter(|value| !value.is_empty());
        let home = var("HOME")
            .or_else(|| match (var("HOMEDRIVE"), var("HOMEPATH")) {
                (Some(drive), Some(path)) => Some(format!("{drive}{path}")),
                _ => None,
            })
            .or_else(|| var("USERPROFILE"));
        SampleIdentity {
            user: var("USERNAME").unwrap_or(sample.user),
            host: var("COMPUTERNAME").unwrap_or(sample.host),
            home: home.unwrap_or(sample.home),
        }
    }
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
    use cmdstudio_core::model::Segment;
    use cmdstudio_core::presets;
    use cmdstudio_core::preview::GitState;

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
    fn a_hostile_sample_reaches_no_span_whichever_text_field_carries_it() {
        // The core's prompt draws `cwd`, `home` and the branch as given, so the cleaning here is
        // all that stands between a typed escape sequence and a span. The first test types into
        // `command` and `user`; this one types into `cwd`, `home`, `host` and the branch, and
        // into `user` again, with a prompt that shows the user and the host.
        let mut theme = terracotta();
        theme.prompt.segments = vec![Segment::User, Segment::Host, Segment::Path, Segment::Git];
        let dirt = "\u{1b}[31m\n\u{202e}";
        let hostile = PreviewContext {
            user: format!("you{dirt}"),
            host: format!("pc{dirt}HOST"),
            home: format!(r"C:\Users\you{dirt}"),
            cwd: format!(r"C:\Users\you\pro{dirt}jects"),
            git: Some(GitState {
                branch: format!("ma{dirt}in"),
                dirty: true,
            }),
            ..PreviewContext::sample()
        };
        let drawn = preview_render(&theme, &hostile);
        for line in drawn.prompt.iter().chain(&drawn.banner) {
            for span in &line.spans {
                for bad in ['\u{1b}', '\n', '\u{202e}'] {
                    assert!(
                        !span.text.contains(bad),
                        "{:?} reached a span: {:?}",
                        bad,
                        span.text
                    );
                }
            }
        }
        // What is drawn is what the cleaned texts draw: the dirt went, the rest stayed, so the
        // home is still the home of the cwd (`~`) and the branch is still the branch.
        let cleaned = PreviewContext {
            user: "you".to_string(),
            host: "pcHOST".to_string(),
            home: r"C:\Users\you".to_string(),
            cwd: r"C:\Users\you\projects".to_string(),
            git: Some(GitState {
                branch: "main".to_string(),
                dirty: true,
            }),
            ..PreviewContext::sample()
        };
        assert_eq!(drawn, preview_render(&theme, &cleaned));
        let text: String = drawn
            .prompt
            .iter()
            .chain(&drawn.banner)
            .map(Line::text)
            .collect();
        assert!(text.contains(r"~\projects"), "{text}");
        assert!(text.contains("main"), "{text}");
        assert!(text.contains("you@pcHOST"), "{text}");
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

    /// An environment of the test's own: the lookup `from_env` takes, over a table.
    fn identity(vars: &[(&str, &str)]) -> SampleIdentity {
        SampleIdentity::from_env(|name| {
            vars.iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| value.to_string())
        })
    }

    #[test]
    fn the_sample_home_is_the_folder_oh_my_posh_takes_as_home() {
        let sample = PreviewContext::sample();
        let profile = (r"USERPROFILE", r"C:\Users\tester");
        let drive_and_path = [("HOMEDRIVE", "D:"), ("HOMEPATH", r"\Users\tester")];
        // HOME wins over everything else.
        let all = [
            ("HOME", r"E:\home"),
            drive_and_path[0],
            drive_and_path[1],
            profile,
        ];
        assert_eq!(identity(&all).home, r"E:\home");
        // An empty HOME is not set: HOMEDRIVE followed by HOMEPATH comes next, and beats
        // USERPROFILE.
        let empty_home = [("HOME", ""), drive_and_path[0], drive_and_path[1], profile];
        assert_eq!(identity(&empty_home).home, r"D:\Users\tester");
        // HOMEDRIVE needs HOMEPATH, and HOMEPATH needs HOMEDRIVE; one alone, or an empty one,
        // is skipped and USERPROFILE is used.
        for lone in [
            vec![("HOMEDRIVE", "D:"), profile],
            vec![("HOMEPATH", r"\Users\tester"), profile],
            vec![("HOMEDRIVE", "D:"), ("HOMEPATH", ""), profile],
            vec![("HOMEDRIVE", ""), ("HOMEPATH", r"\Users\tester"), profile],
        ] {
            assert_eq!(identity(&lone).home, r"C:\Users\tester", "{lone:?}");
        }
        // USERPROFILE alone, and an empty HOME above it.
        assert_eq!(identity(&[profile]).home, r"C:\Users\tester");
        assert_eq!(identity(&[("HOME", ""), profile]).home, r"C:\Users\tester");
        // Nothing set, or only empty values: the core's sample home.
        assert_eq!(identity(&[]).home, sample.home);
        assert_eq!(
            identity(&[("HOME", ""), ("HOMEDRIVE", ""), ("USERPROFILE", "")]).home,
            sample.home
        );
    }

    #[test]
    fn the_sample_user_and_host_are_the_windows_ones_else_the_samples() {
        let sample = PreviewContext::sample();
        let named = identity(&[("USERNAME", "tester"), ("COMPUTERNAME", "TESTPC")]);
        assert_eq!(
            (named.user.as_str(), named.host.as_str()),
            ("tester", "TESTPC")
        );
        // Each field falls back by itself, and an empty value is not a name.
        let half = identity(&[("USERNAME", "tester"), ("COMPUTERNAME", "")]);
        assert_eq!(half.user, "tester");
        assert_eq!(half.host, sample.host);
        let none = identity(&[]);
        assert_eq!(
            none,
            SampleIdentity {
                user: sample.user,
                host: sample.host,
                home: sample.home,
            }
        );
    }
}
