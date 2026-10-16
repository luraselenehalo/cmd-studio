//! The preview emulator (spec §5.6): the prompt and the welcome box as cmd shows them, drawn
//! from the theme without running Oh My Posh or Clink, so the editor can redraw on every
//! change and show git even on a machine without git. It gives lines of styled spans; the
//! app only paints them. Palette, contrast and prompt logic stay here, in Rust, and never in
//! TypeScript (spec §5.6). The preview is English, as everything CMD Studio writes into a
//! cmd window is (spec §3.6); Thai shows only where the user's own data holds it.
//!
//! The parity tests hold it to the real thing: `tests/parity.rs` compares the prompt, cell
//! by cell and color by color, with what the pinned Oh My Posh prints for the config
//! `generate::omp` writes, and `tests/banner_parity.rs` compares the welcome box with what
//! the generated `banner.lua` prints.
mod banner;
mod cells;
mod prompt;
mod values;

pub use banner::render_banner;
pub use cells::{banner_cells, prompt_cells};
pub use prompt::{prompt, render_prompt, Prompt};

use crate::color::Rgb;
use serde::{Deserialize, Serialize};

/// The colors of a run of text: what the terminal draws, after reverse video.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Style {
    pub fg: Rgb,
    /// `None` is the terminal's own background.
    pub bg: Option<Rgb>,
    pub bold: bool,
}

impl Style {
    /// Text in `fg` on the terminal's background.
    pub fn fg(fg: Rgb) -> Style {
        Style {
            fg,
            bg: None,
            bold: false,
        }
    }
}

/// A run of text in one style.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Span {
    pub text: String,
    pub fg: Rgb,
    pub bg: Option<Rgb>,
    pub bold: bool,
}

impl Span {
    pub fn style(&self) -> Style {
        Style {
            fg: self.fg,
            bg: self.bg,
            bold: self.bold,
        }
    }
}

/// One line of the preview: spans from left to right. [`Line::push`] keeps it in one shape,
/// with no empty span and no two neighbours in the same style, so two lines that look the
/// same are equal.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Line {
    pub spans: Vec<Span>,
}

impl Line {
    /// Adds `text` in `style` at the end. Empty text adds nothing, and text in the style of
    /// the last span joins it.
    pub fn push(&mut self, text: &str, style: Style) {
        if text.is_empty() {
            return;
        }
        if let Some(last) = self.spans.last_mut() {
            if last.style() == style {
                last.text.push_str(text);
                return;
            }
        }
        self.spans.push(Span {
            text: text.to_string(),
            fg: style.fg,
            bg: style.bg,
            bold: style.bold,
        });
    }

    /// Adds every span of `other` at the end.
    pub fn append(&mut self, other: &Line) {
        for span in &other.spans {
            self.push(&span.text, span.style());
        }
    }

    /// The text of the line, without its colors.
    pub fn text(&self) -> String {
        self.spans.iter().map(|span| span.text.as_str()).collect()
    }
}

/// The sample values the preview shows: who, where, when, and how the command before went.
/// The app chooses them; the welcome box and the prompt read the same ones.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviewContext {
    /// The Windows user name: `{user}` in the welcome box, and the prompt's `user`.
    pub user: String,
    /// The computer name: `{host}`, and the prompt's `host`. (A real window shows
    /// `COMPUTERNAME` in the box and the DNS host name in the prompt; the preview shows one.)
    pub host: String,
    /// The folder Oh My Posh takes as home, which the prompt shows as `~`: `HOME` when it is
    /// set and not empty, else `HOMEDRIVE` followed by `HOMEPATH` when those give a folder,
    /// else `USERPROFILE` (measured on Oh My Posh 31.5.0 by `tests/parity.rs`). The app must
    /// send that folder as cmd's environment gives it. Windows sets `HOMEDRIVE` and `HOMEPATH`
    /// and usually not `HOME`, so it is mostly the profile folder (`C:\Users\you`).
    pub home: String,
    /// The folder cmd is in: `{cwd}` as it is, and the prompt's `path` with `home` as `~`.
    pub cwd: String,
    /// The local date and time: `{date}`, `{time}` and the prompt's `time`.
    pub clock: Clock,
    /// The git repository `cwd` is in, or `None` when it is in none (git is then hidden).
    pub git: Option<GitState>,
    /// The exit code of the command before; the prompt's `status` shows when it is not 0.
    pub exit_code: i32,
    /// How long the command before ran; the prompt's `duration` shows from 2 seconds on.
    pub duration_ms: u64,
    /// The width of the window in cells. The box's top line is filled to it, and the right
    /// prompt sits at its right edge. 0 is taken as 1.
    pub width: u16,
    /// Whether the window is a Windows Terminal tab: the welcome box's icon is `✻` there and
    /// `*` elsewhere (spec §3.6).
    pub windows_terminal: bool,
    /// The command typed at the prompt before this one, shown after it (spec §3.5: with
    /// the transient prompt on, that prompt has shrunk to `> `).
    pub command: String,
}

/// A local date and time, as `os.date("*t")` gives it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Clock {
    pub year: u16,
    /// 1 to 12.
    pub month: u8,
    /// 1 to 31.
    pub day: u8,
    /// 0 to 23.
    pub hour: u8,
    /// 0 to 59.
    pub minute: u8,
}

/// A git repository as the prompt shows it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GitState {
    pub branch: String,
    /// Whether anything changed: an edited, staged or untracked file (`*`, else `✓`).
    pub dirty: bool,
}

impl PreviewContext {
    /// The sample the editor starts from: a failed command that ran 3.5 seconds, so the
    /// status and duration segments show when the theme has them, and Friday 9 October
    /// 2026 at 16:47, the date spec §3.6 writes out (`Fri 09 Oct 2026`).
    pub fn sample() -> PreviewContext {
        PreviewContext {
            user: "you".to_string(),
            host: "PC".to_string(),
            home: r"C:\Users\you".to_string(),
            cwd: r"C:\Users\you\projects\cmd-studio".to_string(),
            clock: Clock {
                year: 2026,
                month: 10,
                day: 9,
                hour: 16,
                minute: 47,
            },
            git: Some(GitState {
                branch: "main".to_string(),
                dirty: true,
            }),
            exit_code: 1,
            duration_ms: 3500,
            width: 80,
            windows_terminal: true,
            command: "npm test".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgb(s: &str) -> Rgb {
        s.parse().unwrap()
    }

    #[test]
    fn push_joins_text_of_the_same_style_and_drops_empty_text() {
        let (a, b) = (Style::fg(rgb("#111111")), Style::fg(rgb("#222222")));
        let mut line = Line::default();
        line.push("ab", a);
        line.push("", b);
        line.push("c", a);
        line.push("d", b);
        assert_eq!(line.spans.len(), 2);
        assert_eq!(line.spans[0].text, "abc");
        assert_eq!(line.text(), "abcd");
        // the background and bold are part of the style
        line.push(
            "e",
            Style {
                bg: Some(rgb("#333333")),
                ..b
            },
        );
        line.push("f", Style { bold: true, ..b });
        assert_eq!(line.spans.len(), 4);
        // bold alone makes a style: "g" has the colors of "f" but is not bold, and "h" has
        // them and is bold, so each starts a span, while "i" joins "h"
        let bold = Style { bold: true, ..b };
        line.push("g", b);
        line.push("h", bold);
        assert_eq!(line.spans.len(), 6);
        line.push("i", bold);
        assert_eq!(line.spans.len(), 6);
        let bolds: Vec<(&str, bool)> = line
            .spans
            .iter()
            .map(|span| (span.text.as_str(), span.bold))
            .collect();
        assert_eq!(
            bolds,
            [
                ("abc", false),
                ("d", false),
                ("e", false),
                ("f", true),
                ("g", false),
                ("hi", true)
            ]
        );
    }

    #[test]
    fn two_lines_that_look_the_same_are_equal_however_they_were_built() {
        let s = Style::fg(rgb("#123456"));
        let mut one = Line::default();
        one.push("hello", s);
        let mut two = Line::default();
        for piece in ["he", "", "l", "lo"] {
            two.push(piece, s);
        }
        assert_eq!(one, two);
        let mut three = Line::default();
        three.append(&two);
        assert_eq!(three, one);
    }

    #[test]
    fn spans_and_lines_serialize_as_the_app_reads_them() {
        let mut line = Line::default();
        line.push("~ ", Style::fg(rgb("#8a8780")));
        line.push(
            "x",
            Style {
                bg: Some(rgb("#5f5d58")),
                ..Style::fg(rgb("#faf9f5"))
            },
        );
        line.push(
            "!",
            Style {
                bold: true,
                ..Style::fg(rgb("#d97757"))
            },
        );
        let json = serde_json::to_string(&line).unwrap();
        assert_eq!(
            json,
            r##"[{"text":"~ ","fg":"#8a8780","bg":null,"bold":false},{"text":"x","fg":"#faf9f5","bg":"#5f5d58","bold":false},{"text":"!","fg":"#d97757","bg":null,"bold":true}]"##
        );
        assert_eq!(serde_json::from_str::<Line>(&json).unwrap(), line);
    }

    #[test]
    fn the_context_round_trips_through_json_in_camel_case() {
        let ctx = PreviewContext::sample();
        let json = serde_json::to_value(&ctx).unwrap();
        assert_eq!(json["exitCode"], 1);
        assert_eq!(json["durationMs"], 3500);
        assert_eq!(json["windowsTerminal"], true);
        assert_eq!(json["git"]["dirty"], true);
        assert_eq!(serde_json::from_value::<PreviewContext>(json).unwrap(), ctx);
        // a field the format does not know is refused, like a theme's
        let mut extra = serde_json::to_value(&ctx).unwrap();
        extra["shell"] = serde_json::json!("pwsh");
        assert!(serde_json::from_value::<PreviewContext>(extra).is_err());
    }

    /// `value` is read as a `T`, and the same object with one field more is refused.
    fn refuses_a_field_it_does_not_know<T: serde::de::DeserializeOwned>(value: serde_json::Value) {
        assert!(
            serde_json::from_value::<T>(value.clone()).is_ok(),
            "{value}"
        );
        let mut extra = value;
        extra["extra"] = serde_json::json!(true);
        assert!(serde_json::from_value::<T>(extra).is_err());
    }

    #[test]
    fn spans_clocks_and_git_states_refuse_a_field_the_format_does_not_know() {
        // a span is read on its own: a line is a bare array, so it has no object to extend
        let mut line = Line::default();
        line.push("x", Style::fg(rgb("#123456")));
        refuses_a_field_it_does_not_know::<Span>(serde_json::to_value(&line.spans[0]).unwrap());
        let sample = PreviewContext::sample();
        refuses_a_field_it_does_not_know::<Clock>(serde_json::to_value(sample.clock).unwrap());
        refuses_a_field_it_does_not_know::<GitState>(
            serde_json::to_value(sample.git.unwrap()).unwrap(),
        );
    }
}
