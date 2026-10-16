//! Reads what a program printed for a terminal into the preview's lines of spans, so the
//! parity tests can compare it with the emulator. Shared by `parity.rs` and
//! `banner_parity.rs`.
use cmdstudio_core::color::Rgb;
use cmdstudio_core::preview::{Line, Style};

/// The colors SGR has set so far.
#[derive(Clone, Copy, Default)]
struct Sgr {
    fg: Option<Rgb>,
    bg: Option<Rgb>,
    bold: bool,
    reverse: bool,
}

impl Sgr {
    /// Applies the parameters of one `ESC [ ... m`.
    fn apply(&mut self, params: &str) -> Result<(), String> {
        let mut codes = params.split(';').map(|p| {
            if p.is_empty() {
                Ok(0)
            } else {
                p.parse::<u16>().map_err(|e| format!("SGR {params:?}: {e}"))
            }
        });
        while let Some(code) = codes.next() {
            match code? {
                0 => *self = Sgr::default(),
                1 => self.bold = true,
                22 => self.bold = false,
                7 => self.reverse = true,
                27 => self.reverse = false,
                39 => self.fg = None,
                49 => self.bg = None,
                which @ (38 | 48) => {
                    let mut next = || {
                        codes
                            .next()
                            .ok_or_else(|| format!("SGR {params:?}: a color cut short"))?
                    };
                    if next()? != 2 {
                        return Err(format!("SGR {params:?}: only 24-bit colors are expected"));
                    }
                    let mut channel = || {
                        next().and_then(|v| {
                            u8::try_from(v).map_err(|_| format!("SGR {params:?}: {v} > 255"))
                        })
                    };
                    let rgb = Rgb {
                        r: channel()?,
                        g: channel()?,
                        b: channel()?,
                    };
                    if which == 38 {
                        self.fg = Some(rgb);
                    } else {
                        self.bg = Some(rgb);
                    }
                }
                other => return Err(format!("SGR {params:?}: code {other} is not expected")),
            }
        }
        Ok(())
    }

    /// What the terminal draws: the default foreground in `fg`, the default background as
    /// `None`, and the two swapped by reverse video (which brings the background color out
    /// as a foreground: `bg`).
    fn style(&self, fg: Rgb, bg: Rgb) -> Style {
        let front = self.fg.unwrap_or(fg);
        if self.reverse {
            Style {
                fg: self.bg.unwrap_or(bg),
                bg: Some(front),
                bold: self.bold,
            }
        } else {
            Style {
                fg: front,
                bg: self.bg,
                bold: self.bold,
            }
        }
    }
}

/// `out` as lines of spans. SGR sets the colors (24-bit foreground and background, the
/// defaults, reset, bold, reverse video); `\n` starts a line; an OSC sequence (the window
/// title) is dropped. Anything else that is not text is an error, so nothing the emulator
/// does not model can slip through. `fg` and `bg` are the terminal's own colors.
pub fn parse_sgr(out: &str, fg: Rgb, bg: Rgb) -> Result<Vec<Line>, String> {
    let mut lines = vec![Line::default()];
    let mut sgr = Sgr::default();
    let mut rest = out;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix("\u{1b}[") {
            let end = after
                .find(|c: char| !(c.is_ascii_digit() || c == ';'))
                .ok_or("a CSI sequence cut short")?;
            let (params, tail) = after.split_at(end);
            rest = tail.strip_prefix('m').ok_or_else(|| {
                format!("a CSI sequence that is not SGR: {:?}", tail.chars().next())
            })?;
            sgr.apply(params)?;
        } else if let Some(after) = rest.strip_prefix("\u{1b}]") {
            let end = after
                .find(['\u{7}', '\u{1b}'])
                .ok_or("an OSC sequence cut short")?;
            let (_, tail) = after.split_at(end);
            rest = tail
                .strip_prefix('\u{7}')
                .or_else(|| tail.strip_prefix("\u{1b}\\"))
                .ok_or("an OSC sequence with no end")?;
        } else if let Some(after) = rest.strip_prefix('\n') {
            lines.push(Line::default());
            rest = after;
        } else {
            let end = rest.find(['\u{1b}', '\n']).unwrap_or(rest.len());
            let (text, tail) = rest.split_at(end);
            if text.is_empty() {
                return Err(format!(
                    "an escape sequence that is not expected: {:?}",
                    tail.chars().take(4).collect::<String>()
                ));
            }
            if let Some(bad) = text.chars().find(|c| c.is_control()) {
                return Err(format!(
                    "the control character U+{:04X} in {text:?}",
                    u32::from(bad)
                ));
            }
            if let Some(line) = lines.last_mut() {
                line.push(text, sgr.style(fg, bg));
            }
            rest = tail;
        }
    }
    Ok(lines)
}

/// A short description of how two lists of lines differ: the first line that does, with
/// both versions.
pub fn difference(want: &[Line], got: &[Line]) -> Option<String> {
    if want == got {
        return None;
    }
    let shown = |line: Option<&Line>| match line {
        None => "(no line)".to_string(),
        Some(line) => line
            .spans
            .iter()
            .map(|s| {
                let bg = s.bg.map_or("-".to_string(), |bg| bg.to_string());
                format!(
                    "[{} on {bg}{}] {:?}",
                    s.fg,
                    if s.bold { " bold" } else { "" },
                    s.text
                )
            })
            .collect::<Vec<_>>()
            .join(" "),
    };
    let row = (0..want.len().max(got.len()))
        .find(|&i| want.get(i) != got.get(i))
        .unwrap_or(0);
    Some(format!(
        "line {} of {}/{}:\n  emulator: {}\n  real:     {}",
        row + 1,
        want.len(),
        got.len(),
        shown(want.get(row)),
        shown(got.get(row))
    ))
}

#[test]
fn the_parser_reads_colors_reverse_video_and_titles() {
    let fg = Rgb {
        r: 250,
        g: 249,
        b: 245,
    };
    let bg = Rgb {
        r: 31,
        g: 30,
        b: 29,
    };
    let frame = Rgb {
        r: 95,
        g: 93,
        b: 88,
    };
    // the leading arrow of a powerline prompt, then a segment, the title and the final space
    let out = "\u{1b}[0m\u{1b}[38;2;95;93;88;49m\u{1b}[7m\u{e0b0}\u{1b}[0m\u{1b}[48;2;95;93;88m\u{1b}[38;2;250;249;245m ~ \u{1b}[0m\u{1b}]0;title\u{7} ";
    let lines = parse_sgr(out, fg, bg).unwrap();
    assert_eq!(lines.len(), 1);
    let spans = &lines[0].spans;
    assert_eq!(spans.len(), 3, "{spans:?}");
    assert_eq!(
        (spans[0].text.as_str(), spans[0].fg, spans[0].bg),
        ("\u{e0b0}", bg, Some(frame))
    );
    assert_eq!(
        (spans[1].text.as_str(), spans[1].fg, spans[1].bg),
        (" ~ ", fg, Some(frame))
    );
    assert_eq!(
        (spans[2].text.as_str(), spans[2].fg, spans[2].bg),
        (" ", fg, None)
    );
    // lines, and what is refused
    assert_eq!(parse_sgr("a\nb", fg, bg).unwrap().len(), 2);
    assert!(parse_sgr("\u{1b}[2J", fg, bg).is_err());
    assert!(parse_sgr("\u{1b}[38;5;3m", fg, bg).is_err());
    assert!(parse_sgr("a\rb", fg, bg).is_err());
    assert!(parse_sgr("\u{1b}(B", fg, bg).is_err());
}
