//! The prompt as Oh My Posh 31.5.0 prints the config `generate::omp` writes, for cmd
//! (spec §3.5). Measured on the pinned binary, and checked again by `tests/parity.rs`:
//! - A segment the sample hides (git outside a repository, status 0, a duration under 2 s)
//!   leaves nothing behind, not even its spaces.
//! - A powerline segment starts with the arrow U+E0B0 in the color of the segment before it,
//!   on its own color. When no powerline segment comes before it (it starts the line), Oh My
//!   Posh draws that arrow with a transparent foreground, which it writes as reverse video:
//!   the terminal's background on the segment's color. After the last powerline segment the
//!   arrow is that segment's color on the terminal's background.
//! - The box's top line is filled with `─` up to its right side ([`filler_cells`]).
//! - `final_space` puts one space in the terminal's colors at the end of the last line. The
//!   window title Oh My Posh prints just before that space is not drawn.
//! - The right prompt and the transient prompt are printed alone, with no final space.
use super::cells::prompt_cells;
use super::values::{austin, hh_mm, prompt_path};
use super::{Line, PreviewContext, Style};
use crate::color::Rgb;
use crate::generate::omp::{self, Look, Part, Swatch};
use crate::model::{Layout, Theme};
use crate::palette::Colors;
use serde::{Deserialize, Serialize};

/// Oh My Posh's own branch icon (a Nerd Font glyph). The config replaces it with nothing in
/// the box and line layouts; powerline keeps it.
const POWERLINE_BRANCH_ICON: &str = "\u{e0a0}";

/// The cells Oh My Posh wants to stay free on the line before it draws a right-aligned block
/// on it (engine.go, `promptBreathingRoom`).
const BREATHING_ROOM: usize = 5;

/// What Oh My Posh prints when Clink asks it for the prompt.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Prompt {
    /// `print primary`: one line, or two for the box. The command is typed at the end of
    /// the last one, after the final space.
    pub primary: Vec<Line>,
    /// `print right`: the box's right side, which Clink draws at the right edge of the line
    /// the command is typed on. Empty in the line and powerline layouts.
    pub right: Line,
    /// `print transient`: what Clink shrinks an earlier prompt to, when the theme asks for it.
    pub transient: Line,
}

/// The prompt Oh My Posh 31.5.0 prints for `theme` and the sample `ctx`.
pub fn prompt(theme: &Theme, ctx: &PreviewContext) -> Prompt {
    let colors = &theme.colors;
    let layout = theme.prompt.layout;
    let paint = |swatch: Swatch| Style::fg(swatch.color(colors));
    let mut primary = Vec::new();
    let mut right = Line::default();
    match layout {
        Layout::Box => {
            let mut left = Block::new(colors);
            left.text(Swatch::Frame, omp::BOX_TOP);
            for part in omp::left_parts(theme) {
                left.part(layout, part, ctx);
            }
            let mut top = left.finish();
            let mut side = Block::new(colors);
            if omp::has_time(theme) {
                side.part(layout, Part::Time, ctx);
                side.text(Swatch::Frame, omp::BOX_CLOSE_AFTER_TIME);
            } else {
                side.text(Swatch::Frame, omp::BOX_CLOSE);
            }
            let side = side.finish();
            let cells = filler_cells(
                usize::from(ctx.width),
                prompt_cells(&top.text()),
                prompt_cells(&side.text()),
            );
            // as `shouldFill` does: the filler as often as it fits, then spaces
            let unit = prompt_cells(omp::BOX_FILLER).max(1);
            top.push(&omp::BOX_FILLER.repeat(cells / unit), paint(Swatch::Frame));
            top.push(&" ".repeat(cells % unit), Style::fg(colors.foreground));
            top.append(&side);
            let mut second = Line::default();
            second.push(omp::BOX_SIDE, paint(Swatch::Frame));
            second.push(omp::BOX_PROMPT, paint(Swatch::Accent));
            primary.push(top);
            primary.push(second);
            right.push(omp::BOX_RIGHT, paint(Swatch::Frame));
        }
        Layout::Line | Layout::Powerline => {
            let mut line = Block::new(colors);
            for part in omp::left_parts(theme) {
                line.part(layout, part, ctx);
            }
            line.text(Swatch::Accent, omp::prompt_char(layout));
            primary.push(line.finish());
        }
    }
    // final_space
    if let Some(last) = primary.last_mut() {
        last.push(" ", Style::fg(colors.foreground));
    }
    let mut transient = Line::default();
    transient.push(omp::TRANSIENT_PROMPT, paint(Swatch::Accent));
    Prompt {
        primary,
        right,
        transient,
    }
}

/// How many cells Oh My Posh 31.5.0 fills before a right-aligned block of `length` cells,
/// when `current` cells of the line are already written (engine.go: `canWriteRightBlock`,
/// then `shouldFill`): the cells that are left, if at least [`BREATHING_ROOM`] of them stay
/// free; otherwise the whole width less the block, as if the block began a new row. A left
/// side wider than the window counts from where its last row ends. A width of 0 is taken as
/// 1.
fn filler_cells(width: usize, current: usize, length: usize) -> usize {
    let width = width.max(1);
    let free = if current <= width {
        width - current
    } else {
        width - current % width
    };
    match free.checked_sub(length) {
        Some(room) if room >= BREATHING_ROOM => room,
        _ => width.saturating_sub(length),
    }
}

/// The data of `part` for the sample, or `None` when Oh My Posh hides the segment.
fn data(layout: Layout, part: Part, ctx: &PreviewContext) -> Option<String> {
    match part {
        Part::Path => Some(prompt_path(&ctx.home, &ctx.cwd)),
        Part::Git => ctx.git.as_ref().map(|git| {
            let icon = match layout {
                Layout::Powerline => POWERLINE_BRANCH_ICON,
                Layout::Box | Layout::Line => "",
            };
            let mark = if git.dirty {
                omp::GIT_DIRTY_MARK
            } else {
                omp::GIT_CLEAN_MARK
            };
            format!("{icon}{}{mark}", git.branch)
        }),
        Part::Time => Some(hh_mm(&ctx.clock)),
        Part::Session { user, host } => Some(match (user, host) {
            (true, true) => format!("{}@{}", ctx.user, ctx.host),
            (true, false) => ctx.user.clone(),
            (false, _) => ctx.host.clone(),
        }),
        Part::Status => {
            (ctx.exit_code != 0).then(|| format!("{}{}", omp::STATUS_MARK, ctx.exit_code))
        }
        Part::Duration => {
            (ctx.duration_ms >= omp::DURATION_THRESHOLD_MS).then(|| austin(ctx.duration_ms))
        }
    }
}

/// The segments of one block, joined the way Oh My Posh joins them.
struct Block<'a> {
    colors: &'a Colors,
    line: Line,
    /// The color behind the last segment, while that segment is a powerline one.
    open: Option<Rgb>,
}

impl<'a> Block<'a> {
    fn new(colors: &'a Colors) -> Self {
        Block {
            colors,
            line: Line::default(),
            open: None,
        }
    }

    /// One of the generator's own `text` segments: `text` in `swatch`.
    fn text(&mut self, swatch: Swatch, text: &str) {
        self.close();
        self.line.push(text, Style::fg(swatch.color(self.colors)));
    }

    /// The segment of `part`, unless the sample hides it.
    fn part(&mut self, layout: Layout, part: Part, ctx: &PreviewContext) {
        let Some(data) = data(layout, part, ctx) else {
            return;
        };
        let drawn = omp::drawn(layout, part);
        let changed = part == Part::Git && ctx.git.as_ref().is_some_and(|git| git.dirty);
        let look = if changed {
            drawn.look.changed()
        } else {
            drawn.look
        };
        let style = match look {
            Look::Plain { fg } => {
                self.close();
                Style::fg(fg.color(self.colors))
            }
            Look::Powerline { bg, fg } => {
                let bg = bg.color(self.colors);
                // the arrow in: from the segment before, or transparent (reverse video)
                let from = self.open.unwrap_or(self.colors.background);
                self.line.push(
                    omp::POWERLINE_SYMBOL,
                    Style {
                        fg: from,
                        bg: Some(bg),
                        bold: false,
                    },
                );
                self.open = Some(bg);
                Style {
                    fg: fg.color(self.colors),
                    bg: Some(bg),
                    bold: false,
                }
            }
        };
        if let Some((swatch, lead)) = drawn.lead {
            let lead_style = Style {
                fg: swatch.color(self.colors),
                ..style
            };
            self.line.push(lead, lead_style);
        }
        self.line.push(drawn.before, style);
        self.line.push(&data, style);
        self.line.push(drawn.after, style);
    }

    /// Ends a run of powerline segments: the arrow out of the last one.
    fn close(&mut self) {
        if let Some(bg) = self.open.take() {
            self.line.push(omp::POWERLINE_SYMBOL, Style::fg(bg));
        }
    }

    fn finish(mut self) -> Line {
        self.close();
        self.line
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Segment;
    use crate::presets::terracotta;
    use crate::preview::GitState;

    const ALL: [Segment; 7] = [
        Segment::Path,
        Segment::Git,
        Segment::Time,
        Segment::User,
        Segment::Host,
        Segment::Status,
        Segment::Duration,
    ];

    // Terracotta's palette, by the names the config gives it
    const BG: &str = "#1f1e1d";
    const FG: &str = "#faf9f5";
    const ACCENT: &str = "#d97757";
    const MUTED: &str = "#8a8780";
    const FRAME: &str = "#5f5d58";
    const RED: &str = "#e5736a";
    const OK: &str = "#8fbf7f";
    const DIRTY: &str = "#e3b66b";
    const BLUE: &str = "#7fa7d9";

    fn themed(layout: Layout, segments: &[Segment]) -> Theme {
        let mut t = terracotta();
        t.prompt.layout = layout;
        t.prompt.segments = segments.to_vec();
        t
    }

    /// A line as (text, foreground, background), for short comparisons.
    fn spans(line: &Line) -> Vec<(String, String, Option<String>)> {
        line.spans
            .iter()
            .map(|s| {
                (
                    s.text.clone(),
                    s.fg.to_string(),
                    s.bg.map(|b| b.to_string()),
                )
            })
            .collect()
    }

    /// Text in `fg` on the terminal's background.
    fn s(text: &str, fg: &str) -> (String, String, Option<String>) {
        (text.to_string(), fg.to_string(), None)
    }

    /// Text in `fg` on `bg`.
    fn on(text: &str, fg: &str, bg: &str) -> (String, String, Option<String>) {
        (text.to_string(), fg.to_string(), Some(bg.to_string()))
    }

    fn sample(width: u16) -> PreviewContext {
        PreviewContext {
            width,
            ..PreviewContext::sample()
        }
    }

    #[test]
    fn the_box_fills_its_top_line_to_the_width_and_closes_after_the_time() {
        // Terracotta's own prompt as Oh My Posh 31.5.0 prints it 60 cells wide: the left side
        // is 34 cells and the right side 9, so the filler is 17
        let p = prompt(&terracotta(), &sample(60));
        assert_eq!(p.primary.len(), 2);
        assert_eq!(
            spans(&p.primary[0]),
            vec![
                s("\u{256d}\u{2500} ", FRAME),
                s("~\\projects\\cmd-studio \u{b7} ", MUTED),
                s("main * ", DIRTY),
                s(&"\u{2500}".repeat(17), FRAME),
                s(" 16:47", MUTED),
                s(" \u{2500}\u{256e}", FRAME),
            ]
        );
        assert_eq!(prompt_cells(&p.primary[0].text()), 60);
        assert_eq!(
            spans(&p.primary[1]),
            vec![s("\u{2502} ", FRAME), s(">", ACCENT), s(" ", FG)]
        );
        assert_eq!(spans(&p.right), vec![s("\u{2502}", FRAME)]);
        assert_eq!(spans(&p.transient), vec![s("> ", ACCENT)]);
    }

    #[test]
    fn without_time_the_filler_runs_into_the_corner() {
        let p = prompt(&themed(Layout::Box, &[Segment::Path]), &sample(40));
        let top = spans(&p.primary[0]);
        let close = format!("{}\u{2500}\u{256e}", "\u{2500}".repeat(13));
        assert_eq!(top.last().unwrap(), &s(&close, FRAME));
        assert_eq!(prompt_cells(&p.primary[0].text()), 40);
    }

    #[test]
    fn the_line_layout_is_one_line_with_time_last() {
        let theme = themed(Layout::Line, &[Segment::Time, Segment::Path, Segment::Git]);
        let p = prompt(&theme, &sample(80));
        assert_eq!(p.primary.len(), 1);
        assert_eq!(
            spans(&p.primary[0]),
            vec![
                s("~\\projects\\cmd-studio ", BLUE),
                s("main * ", DIRTY),
                s("16:47 ", MUTED),
                s("\u{276f}", ACCENT),
                s(" ", FG),
            ]
        );
        assert!(p.right.spans.is_empty());
    }

    #[test]
    fn powerline_starts_with_a_reversed_arrow_and_joins_blocks_with_arrows() {
        let theme = themed(
            Layout::Powerline,
            &[Segment::Path, Segment::Git, Segment::Time],
        );
        let p = prompt(&theme, &sample(80));
        assert_eq!(
            spans(&p.primary[0]),
            vec![
                // the transparent arrow: the terminal's background on the first block's color
                on("\u{e0b0}", BG, FRAME),
                on(" ~\\projects\\cmd-studio ", FG, FRAME),
                on("\u{e0b0}", FRAME, DIRTY),
                on(" \u{e0a0}main * ", BG, DIRTY),
                on("\u{e0b0}", DIRTY, FRAME),
                on(" 16:47 ", FG, FRAME),
                s("\u{e0b0}", FRAME),
                s(" \u{276f}", ACCENT),
                s(" ", FG),
            ]
        );
    }

    #[test]
    fn segments_the_sample_hides_leave_nothing_behind() {
        // no repository, exit code 0, a duration under 2 seconds
        let ctx = PreviewContext {
            git: None,
            exit_code: 0,
            duration_ms: 1999,
            ..sample(80)
        };
        for layout in [Layout::Line, Layout::Powerline] {
            let text = prompt(&themed(layout, &ALL), &ctx).primary[0].text();
            assert!(!text.contains("main"), "{layout:?}: {text}");
            assert!(!text.contains('\u{2717}'), "{layout:?}: {text}");
            assert!(!text.contains("1.999"), "{layout:?}: {text}");
            assert!(
                !text.contains("  "),
                "{layout:?}: a gap where a segment was: {text:?}"
            );
        }
        // and they show when the sample has them: 2000 ms is enough
        let ctx = PreviewContext {
            exit_code: -1,
            duration_ms: 2000,
            ..sample(80)
        };
        let theme = themed(Layout::Line, &[Segment::Status, Segment::Duration]);
        assert_eq!(
            prompt(&theme, &ctx).primary[0].text(),
            "\u{2717} -1 2s \u{276f} "
        );
    }

    #[test]
    fn git_is_ok_colored_while_clean_and_dirty_colored_once_changed() {
        let clean = PreviewContext {
            git: Some(GitState {
                branch: "feature/x".to_string(),
                dirty: false,
            }),
            ..sample(80)
        };
        let line = &prompt(&themed(Layout::Line, &[Segment::Git]), &clean).primary[0];
        assert_eq!(spans(line)[0], s("feature/x \u{2713} ", OK));
        // git's text is the terminal's background color, like the arrow in front of it, so
        // the two are one span
        let block = &prompt(&themed(Layout::Powerline, &[Segment::Git]), &clean).primary[0];
        assert_eq!(
            spans(block),
            vec![
                on("\u{e0b0} \u{e0a0}feature/x \u{2713} ", BG, OK),
                s("\u{e0b0}", OK),
                s(" \u{276f}", ACCENT),
                s(" ", FG),
            ]
        );
        // the box puts a muted separator first
        let boxed = &prompt(&themed(Layout::Box, &[Segment::Git]), &clean).primary[0];
        assert_eq!(spans(boxed)[1], s("\u{b7} ", MUTED));
        assert_eq!(spans(boxed)[2], s("feature/x \u{2713} ", OK));
    }

    #[test]
    fn user_and_host_are_one_segment_in_every_combination() {
        for (list, text) in [
            (&[Segment::User, Segment::Host][..], "you@PC "),
            (&[Segment::User][..], "you "),
            (&[Segment::Host][..], "PC "),
        ] {
            let line = &prompt(&themed(Layout::Line, list), &sample(80)).primary[0];
            assert_eq!(spans(line)[0], s(text, FG), "{list:?}");
        }
        // the arrow out of the accent block and the prompt character are one span too
        let theme = themed(Layout::Powerline, &[Segment::Host, Segment::User]);
        let block = &prompt(&theme, &sample(80)).primary[0];
        assert_eq!(
            spans(block),
            vec![
                on("\u{e0b0} you@PC ", BG, ACCENT),
                s("\u{e0b0} \u{276f}", ACCENT),
                s(" ", FG),
            ]
        );
    }

    #[test]
    fn status_and_duration_use_red_and_the_dirty_color() {
        let list = [Segment::Status, Segment::Duration];
        let line = &prompt(&themed(Layout::Line, &list), &sample(80)).primary[0];
        assert_eq!(spans(line)[0], s("\u{2717} 1 ", RED));
        assert_eq!(spans(line)[1], s("3.5s ", DIRTY));
        let block = &prompt(&themed(Layout::Powerline, &list), &sample(80)).primary[0];
        assert_eq!(
            spans(block),
            vec![
                on("\u{e0b0} \u{2717} 1 ", BG, RED),
                on("\u{e0b0}", RED, DIRTY),
                on(" 3.5s ", BG, DIRTY),
                s("\u{e0b0}", DIRTY),
                s(" \u{276f}", ACCENT),
                s(" ", FG),
            ]
        );
    }

    #[test]
    fn the_filler_follows_oh_my_posh_at_every_width() {
        // (width, cells written, cells of the right side, filler), measured on Oh My Posh
        // 31.5.0 with Terracotta's box: a left side of 19 cells and a right side of 9, and
        // the empty box (3 and 2)
        for (width, current, length, filler) in [
            (60, 19, 9, 32),
            (33, 19, 9, 5),  // five cells free: room enough
            (32, 19, 9, 23), // four free: the right side goes as if on a new row
            (19, 19, 9, 10),
            (18, 19, 9, 8), // the left side is wider than the window: its last row counts
            (10, 19, 9, 1),
            (9, 19, 9, 0),
            (1, 19, 9, 0),
            (5, 3, 2, 3),
            (30, 3, 2, 25),
        ] {
            assert_eq!(
                filler_cells(width, current, length),
                filler,
                "{width} {current} {length}"
            );
        }
        // a width of 0 is taken as 1
        assert_eq!(filler_cells(0, 19, 9), 0);
    }

    #[test]
    fn the_branch_icon_is_the_one_the_config_leaves() {
        // the box and line configs set `branch_icon` to nothing; powerline keeps Oh My Posh's
        // own, a Nerd Font glyph
        for layout in [Layout::Box, Layout::Line, Layout::Powerline] {
            let theme = themed(layout, &[Segment::Git]);
            let config = omp::config(&theme);
            let git = config["blocks"][0]["segments"]
                .as_array()
                .unwrap()
                .iter()
                .find(|s| s["type"] == "git")
                .unwrap()
                .clone();
            let text = prompt(&theme, &sample(80)).primary[0].text();
            match git["options"].get("branch_icon") {
                Some(icon) => {
                    assert_eq!(icon, "", "{layout:?}");
                    assert!(!text.contains(POWERLINE_BRANCH_ICON), "{layout:?}: {text}");
                }
                None => assert!(
                    text.contains(&format!("{POWERLINE_BRANCH_ICON}main *")),
                    "{layout:?}: {text}"
                ),
            }
        }
    }
}
