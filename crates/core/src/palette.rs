//! Simple-mode palette generation and the readability (contrast) check
//! (spec §3.4).
use crate::color::{contrast_ratio, Rgb};
use crate::oklch::Oklch;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum Tone {
    Dark,
    Light,
}

/// The 23 colors of a theme. `ansi` uses the standard order: black, red,
/// green, yellow, blue, magenta, cyan, white, then the 8 bright variants.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Colors {
    #[cfg_attr(feature = "ts", ts(type = "string"))]
    pub background: Rgb,
    #[cfg_attr(feature = "ts", ts(type = "string"))]
    pub foreground: Rgb,
    #[cfg_attr(feature = "ts", ts(type = "string"))]
    pub cursor: Rgb,
    #[cfg_attr(feature = "ts", ts(type = "string"))]
    pub selection: Rgb,
    #[cfg_attr(feature = "ts", ts(type = "string"))]
    pub accent: Rgb,
    #[cfg_attr(feature = "ts", ts(type = "string"))]
    pub muted: Rgb,
    #[cfg_attr(feature = "ts", ts(type = "string"))]
    pub frame: Rgb,
    #[cfg_attr(feature = "ts", ts(type = "string[]"))]
    pub ansi: [Rgb; 16],
}

pub const MIN_TEXT_CONTRAST: f64 = 4.5;
pub const MIN_OTHER_CONTRAST: f64 = 3.0;

/// One color of a [`Colors`], as the readability check addresses it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    Foreground,
    Muted,
    Accent,
    /// An index into [`Colors::ansi`], 0–15. A larger index names no color
    /// (see [`Colors::get`] and [`Colors::set`]).
    Ansi(usize),
}

/// A checked color whose contrast against the background is below `required`.
#[derive(Clone, Debug, PartialEq)]
pub struct ContrastIssue {
    pub slot: Slot,
    pub ratio: f64,
    pub required: f64,
}

/// Every color the readability check looks at, with its minimum ratio.
pub fn checked_slots() -> Vec<(Slot, f64)> {
    let mut v = vec![
        (Slot::Foreground, MIN_TEXT_CONTRAST),
        (Slot::Muted, MIN_OTHER_CONTRAST),
        (Slot::Accent, MIN_OTHER_CONTRAST),
    ];
    v.extend(
        (1..=6)
            .chain(9..=14)
            .map(|i| (Slot::Ansi(i), MIN_OTHER_CONTRAST)),
    );
    v
}

impl Colors {
    /// The color in `slot`. An `Ansi` index above 15 names no color and reads
    /// as the background, which can never pass a contrast check.
    pub fn get(&self, slot: Slot) -> Rgb {
        match slot {
            Slot::Foreground => self.foreground,
            Slot::Muted => self.muted,
            Slot::Accent => self.accent,
            Slot::Ansi(i) => self.ansi.get(i).copied().unwrap_or(self.background),
        }
    }

    /// Sets the color in `slot`. An `Ansi` index above 15 names no color, so
    /// nothing changes.
    pub fn set(&mut self, slot: Slot, value: Rgb) {
        match slot {
            Slot::Foreground => self.foreground = value,
            Slot::Muted => self.muted = value,
            Slot::Accent => self.accent = value,
            Slot::Ansi(i) => {
                if let Some(color) = self.ansi.get_mut(i) {
                    *color = value;
                }
            }
        }
    }
}

/// Every checked color that misses its minimum contrast against
/// `colors.background`. An empty list means the palette is readable.
pub fn contrast_issues(colors: &Colors) -> Vec<ContrastIssue> {
    checked_slots()
        .into_iter()
        .filter_map(|(slot, required)| {
            let ratio = contrast_ratio(colors.get(slot), colors.background);
            (ratio < required).then_some(ContrastIssue {
                slot,
                ratio,
                required,
            })
        })
        .collect()
}

/// Changes only the OKLCH lightness of `color`, away from `background`, until
/// it reaches `min` (or lightness runs out). Hue never changes.
pub fn fix_lightness(color: Rgb, background: Rgb, min: f64) -> Rgb {
    if contrast_ratio(color, background) >= min {
        return color;
    }
    let white = Rgb {
        r: 255,
        g: 255,
        b: 255,
    };
    let black = Rgb { r: 0, g: 0, b: 0 };
    let step = if contrast_ratio(white, background) >= contrast_ratio(black, background) {
        0.005
    } else {
        -0.005
    };
    let mut lch = Oklch::from_rgb(color);
    let mut out = color;
    while (0.0..=1.0).contains(&lch.l) {
        lch.l += step;
        out = Oklch {
            l: lch.l.clamp(0.0, 1.0),
            ..lch
        }
        .to_rgb_clamped();
        if contrast_ratio(out, background) >= min {
            break;
        }
    }
    out
}

/// Raises every checked color to its minimum contrast.
pub fn fix_all(colors: &mut Colors) {
    for (slot, min) in checked_slots() {
        let fixed = fix_lightness(colors.get(slot), colors.background, min);
        colors.set(slot, fixed);
    }
}

/// Hues (degrees) of ansi 1–6: red, green, yellow, blue, magenta, cyan.
const ANSI_HUES: [f64; 6] = [25.0, 145.0, 90.0, 255.0, 330.0, 200.0];

/// An accent with less OKLCH chroma than this has no hue. `Oklch::from_rgb`
/// still reports one for it (about 89.9 degrees for every grey), but that is a
/// rounding artifact, not a color.
const ACHROMATIC_BELOW: f64 = 1e-4;

/// Simple mode: all 23 colors from a tone, one accent and a tint (0–0.30).
/// Lightness and chroma values are the starting values in spec §3.4.
///
/// The neutrals (background, foreground, muted, frame, selection and ansi
/// 0, 7, 8, 15) are tinted with the accent's hue. An achromatic accent such as
/// grey, white or black has none, so its neutrals get no chroma at all. The
/// six ansi colors keep their own fixed hues either way.
pub fn generate(tone: Tone, accent: Rgb, tint: f64) -> Colors {
    let dark = tone == Tone::Dark;
    let pick = |d: f64, l: f64| if dark { d } else { l };
    let t = tint.clamp(0.0, 0.30);
    let accent_lch = Oklch::from_rgb(accent);
    let h = accent_lch.h;
    let achromatic = accent_lch.c < ACHROMATIC_BELOW;
    let mk = |l: f64, c: f64, hue: f64| Oklch { l, c, h: hue }.to_rgb_clamped();
    let neutral = |l: f64, c: f64| mk(l, if achromatic { 0.0 } else { c }, h);

    let background = neutral(pick(0.20, 0.97), t * pick(0.08, 0.04));
    let [red, green, yellow, blue, magenta, cyan] =
        ANSI_HUES.map(|hue| mk(pick(0.72, 0.52), 0.12, hue));
    let [bright_red, bright_green, bright_yellow, bright_blue, bright_magenta, bright_cyan] =
        ANSI_HUES.map(|hue| mk(pick(0.80, 0.45), 0.12, hue));
    let ansi = [
        neutral(pick(0.30, 0.25), 0.01), // 0 black
        red,
        green,
        yellow,
        blue,
        magenta,
        cyan,
        neutral(0.85, 0.01),             // 7 white
        neutral(pick(0.50, 0.45), 0.01), // 8 bright black
        bright_red,
        bright_green,
        bright_yellow,
        bright_blue,
        bright_magenta,
        bright_cyan,
        neutral(pick(0.97, 0.98), 0.01), // 15 bright white
    ];
    let mut colors = Colors {
        background,
        foreground: neutral(pick(0.94, 0.25), 0.01),
        cursor: accent,
        selection: neutral(pick(0.32, 0.88), 0.02),
        accent,
        muted: neutral(pick(0.68, 0.50), 0.02),
        frame: neutral(pick(0.45, 0.80), 0.015),
        ansi,
    };
    fix_all(&mut colors);
    colors.cursor = colors.accent;
    colors
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::{contrast_ratio, Rgb};
    use crate::oklch::Oklch;

    fn c(s: &str) -> Rgb {
        s.parse().unwrap()
    }

    #[test]
    fn terracotta_like_dark_palette_passes_and_keeps_the_accent() {
        let p = generate(Tone::Dark, c("#d97757"), 0.10);
        assert!(contrast_issues(&p).is_empty(), "{:?}", contrast_issues(&p));
        assert_eq!(
            p.accent,
            c("#d97757"),
            "the accent already passes on dark and must not move"
        );
        assert_eq!(p.cursor, p.accent);
    }

    #[test]
    fn yellow_accent_on_light_gets_darker_but_keeps_its_hue() {
        let p = generate(Tone::Light, c("#e3b341"), 0.0);
        assert!(contrast_ratio(p.accent, p.background) >= MIN_OTHER_CONTRAST);
        let (before, after) = (Oklch::from_rgb(c("#e3b341")), Oklch::from_rgb(p.accent));
        assert!(after.l < before.l, "{before:?} {after:?}");
        assert!((before.h - after.h).abs() < 3.0, "{before:?} {after:?}");
    }

    #[test]
    fn the_cursor_follows_the_accent_when_the_fix_moves_it() {
        // The readability fix may move the accent; the cursor is set to the accent after it, so
        // the cursor never keeps the color the fix took away.
        let p = generate(Tone::Light, c("#e3b341"), 0.0);
        assert_ne!(p.accent, c("#e3b341"), "the fix moved this accent");
        assert_eq!(p.cursor, p.accent);
    }

    #[test]
    fn a_coloured_accent_tints_the_neutrals_with_its_hue() {
        // Only an accent without chroma leaves the neutrals grey (ruling P23). A real one gives
        // them a little of its hue, the background as much as the tint asks for.
        let accent = c("#d97757");
        let hue = Oklch::from_rgb(accent).h;
        let p = generate(Tone::Dark, accent, 0.30);
        for (name, color) in [
            ("background", p.background),
            ("muted", p.muted),
            ("selection", p.selection),
        ] {
            let o = Oklch::from_rgb(color);
            assert!(o.c > 0.01, "{name} {color}: chroma {}", o.c);
            assert!(
                (o.h - hue).abs() < 15.0,
                "{name} {color}: hue {} for {hue}",
                o.h
            );
        }
        let untinted = generate(Tone::Dark, accent, 0.0);
        assert!(
            Oklch::from_rgb(untinted.background).c < 0.005,
            "no tint, no chroma in the background"
        );
    }

    #[test]
    fn achromatic_accents_still_give_valid_palettes() {
        // Review Focus 2
        for s in ["#000000", "#ffffff", "#808080"] {
            for tone in [Tone::Dark, Tone::Light] {
                let p = generate(tone, c(s), 0.30);
                assert!(
                    contrast_issues(&p).is_empty(),
                    "{s} {tone:?}: {:?}",
                    contrast_issues(&p)
                );
            }
        }
    }

    #[test]
    fn achromatic_accents_leave_the_neutrals_neutral() {
        // `Oklch::from_rgb` gives every grey the rounding-artifact hue 89.876 degrees, so tinting
        // the neutrals with it would add a yellow cast nobody chose. Each neutral must keep the
        // lightness it has in a palette built from a real hue (a NaN anywhere would collapse a
        // channel to 0 and show up there) and lose only its chroma.
        let neutrals = |p: &Colors| {
            [
                ("background", p.background),
                ("foreground", p.foreground),
                ("muted", p.muted),
                ("frame", p.frame),
                ("selection", p.selection),
                ("ansi 0", p.ansi[0]),
                ("ansi 7", p.ansi[7]),
                ("ansi 8", p.ansi[8]),
                ("ansi 15", p.ansi[15]),
            ]
        };
        for s in ["#808080", "#ffffff", "#000000"] {
            for tone in [Tone::Dark, Tone::Light] {
                for tint in [0.0, 0.15, 0.30] {
                    let p = generate(tone, c(s), tint);
                    // The same recipe with a real hue: the greys must differ from it in chroma only.
                    let reference = generate(tone, c("#d97757"), tint);
                    for ((name, got), (_, want)) in
                        neutrals(&p).into_iter().zip(neutrals(&reference))
                    {
                        let (got_lch, want_lch) = (Oklch::from_rgb(got), Oklch::from_rgb(want));
                        let why = format!("{s} {tone:?} tint {tint}: {name} is {got}");
                        assert!(
                            got_lch.l.is_finite() && got_lch.c.is_finite() && got_lch.h.is_finite(),
                            "{why}: {got_lch:?}"
                        );
                        assert!(got_lch.c <= 1e-3, "{why}, chroma {}", got_lch.c);
                        assert!(
                            (got_lch.l - want_lch.l).abs() < 0.01,
                            "{why}: lightness {} should be near {}",
                            got_lch.l,
                            want_lch.l
                        );
                    }
                    assert!(
                        contrast_issues(&p).is_empty(),
                        "{s} {tone:?} tint {tint}: {:?}",
                        contrast_issues(&p)
                    );
                }
            }
        }
    }

    #[test]
    fn fix_lightness_leaves_passing_colors_alone() {
        assert_eq!(fix_lightness(c("#faf9f5"), c("#1f1e1d"), 4.5), c("#faf9f5"));
    }

    #[test]
    fn fix_lightness_rescues_dark_grey_on_dark() {
        let fixed = fix_lightness(c("#333333"), c("#1f1e1d"), 4.5);
        assert!(contrast_ratio(fixed, c("#1f1e1d")) >= 4.5);
    }

    #[test]
    fn terracotta_preset_values_pass() {
        let ansi = [
            "#3a3936", "#e5736a", "#8fbf7f", "#e3b66b", "#7fa7d9", "#b79bdb", "#7fc1b8", "#d8d6cf",
            "#8a8780", "#f0907f", "#a8d196", "#eecb8a", "#9bbde8", "#cbb4e8", "#9dd3cb", "#faf9f5",
        ]
        .map(c);
        let p = Colors {
            background: c("#1f1e1d"),
            foreground: c("#faf9f5"),
            cursor: c("#d97757"),
            selection: c("#4a4845"),
            accent: c("#d97757"),
            muted: c("#8a8780"),
            frame: c("#5f5d58"),
            ansi,
        };
        assert!(contrast_issues(&p).is_empty(), "{:?}", contrast_issues(&p));
    }

    #[test]
    fn checked_slots_cover_text_muted_accent_and_twelve_ansi() {
        let slots = checked_slots();
        assert_eq!(slots.len(), 15);
        assert_eq!(slots[0], (Slot::Foreground, MIN_TEXT_CONTRAST));
        assert!(!slots
            .iter()
            .any(|(s, _)| matches!(s, Slot::Ansi(0 | 7 | 8 | 15))));
    }

    #[test]
    fn ansi_slot_past_the_last_color_is_a_defined_no_op() {
        // `Slot::Ansi` holds any usize, so get and set must not index blindly.
        let mut p = generate(Tone::Dark, c("#d97757"), 0.10);
        let before = p.clone();
        for i in [16, 17, usize::MAX] {
            assert_eq!(
                p.get(Slot::Ansi(i)),
                p.background,
                "Ansi({i}) names no color and reads as the background"
            );
            p.set(Slot::Ansi(i), c("#123456"));
        }
        assert_eq!(
            p, before,
            "writing a slot that does not exist changes nothing"
        );
        // 15 is the last real slot.
        p.set(Slot::Ansi(15), c("#123456"));
        assert_eq!(p.get(Slot::Ansi(15)), c("#123456"));
        assert_eq!(p.ansi[15], c("#123456"));
    }
}
