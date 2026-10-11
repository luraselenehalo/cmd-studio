//! OKLab / OKLCH (Björn Ottosson, 2020) on top of `color::Rgb`. Used by
//! simple mode so generated colors have even perceived lightness.
use crate::color::{linear_to_srgb, srgb_to_linear, Rgb};

/// A color in OKLCH: lightness 0–1, chroma ≥ 0, hue in degrees 0–360.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Oklch {
    pub l: f64,
    pub c: f64,
    pub h: f64,
}

fn linear_to_oklab(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    let l = 0.412_221_470_8 * r + 0.536_332_536_3 * g + 0.051_445_992_9 * b;
    let m = 0.211_903_498_2 * r + 0.680_699_545_1 * g + 0.107_396_956_6 * b;
    let s = 0.088_302_461_9 * r + 0.281_718_837_6 * g + 0.629_978_700_5 * b;
    let (l, m, s) = (l.cbrt(), m.cbrt(), s.cbrt());
    (
        0.210_454_255_3 * l + 0.793_617_785_0 * m - 0.004_072_046_8 * s,
        1.977_998_495_1 * l - 2.428_592_205_0 * m + 0.450_593_709_9 * s,
        0.025_904_037_1 * l + 0.782_771_766_2 * m - 0.808_675_766_0 * s,
    )
}

fn oklab_to_linear(l: f64, a: f64, b: f64) -> (f64, f64, f64) {
    let l_ = l + 0.396_337_777_4 * a + 0.215_803_757_3 * b;
    let m_ = l - 0.105_561_345_8 * a - 0.063_854_172_8 * b;
    let s_ = l - 0.089_484_177_5 * a - 1.291_485_548_0 * b;
    let (l3, m3, s3) = (l_ * l_ * l_, m_ * m_ * m_, s_ * s_ * s_);
    (
        4.076_741_662_1 * l3 - 3.307_711_591_3 * m3 + 0.230_969_929_2 * s3,
        -1.268_438_004_6 * l3 + 2.609_757_401_1 * m3 - 0.341_319_396_5 * s3,
        -0.004_196_086_3 * l3 - 0.703_418_614_7 * m3 + 1.707_614_701_0 * s3,
    )
}

const GAMUT_EPS: f64 = 1e-7;

fn in_gamut((r, g, b): (f64, f64, f64)) -> bool {
    [r, g, b]
        .iter()
        .all(|v| (-GAMUT_EPS..=1.0 + GAMUT_EPS).contains(v))
}

impl Oklch {
    pub fn from_rgb(c: Rgb) -> Self {
        let (l, a, b) = linear_to_oklab(
            srgb_to_linear(c.r),
            srgb_to_linear(c.g),
            srgb_to_linear(c.b),
        );
        let chroma = (a * a + b * b).sqrt();
        let mut h = b.atan2(a).to_degrees();
        if h < 0.0 {
            h += 360.0;
        }
        Oklch { l, c: chroma, h }
    }

    fn to_linear(self) -> (f64, f64, f64) {
        let (sin, cos) = self.h.to_radians().sin_cos();
        oklab_to_linear(self.l, self.c * cos, self.c * sin)
    }

    /// Converts to sRGB. Out-of-gamut colors keep lightness and hue and lose
    /// chroma until they fit.
    pub fn to_rgb_clamped(self) -> Rgb {
        let mut lch = Oklch {
            l: self.l.clamp(0.0, 1.0),
            c: self.c.max(0.0),
            h: self.h,
        };
        if !in_gamut(lch.to_linear()) {
            let (mut lo, mut hi) = (0.0, lch.c);
            for _ in 0..40 {
                let mid = (lo + hi) / 2.0;
                if in_gamut(Oklch { c: mid, ..lch }.to_linear()) {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            lch.c = lo;
        }
        let (r, g, b) = lch.to_linear();
        Rgb {
            r: linear_to_srgb(r),
            g: linear_to_srgb(g),
            b: linear_to_srgb(b),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Rgb;

    fn c(s: &str) -> Rgb {
        s.parse().unwrap()
    }

    #[test]
    fn red_matches_the_css_color_4_reference() {
        let o = Oklch::from_rgb(c("#ff0000"));
        assert!((o.l - 0.6280).abs() < 1e-3, "{o:?}");
        assert!((o.c - 0.2577).abs() < 1e-3, "{o:?}");
        assert!((o.h - 29.23).abs() < 0.05, "{o:?}");
    }

    #[test]
    fn white_has_full_lightness_and_no_chroma() {
        let o = Oklch::from_rgb(c("#ffffff"));
        assert!((o.l - 1.0).abs() < 1e-4 && o.c < 1e-4, "{o:?}");
    }

    #[test]
    fn grey_has_no_nan() {
        let o = Oklch::from_rgb(c("#808080"));
        assert!(o.h.is_finite() && o.l.is_finite() && o.c < 1e-4, "{o:?}");
    }

    #[test]
    fn hue_is_always_from_0_up_to_360() {
        // atan2 gives -180 to 180 degrees; the hues below 0 are moved up by a full turn. Blue's
        // is one of them: 264.05 in CSS Color 4, -95.95 without the turn.
        let blue = Oklch::from_rgb(c("#0000ff"));
        assert!((blue.h - 264.05).abs() < 0.05, "{blue:?}");
        for r in (0..=255u8).step_by(17) {
            for g in (0..=255u8).step_by(17) {
                for b in (0..=255u8).step_by(17) {
                    let o = Oklch::from_rgb(Rgb { r, g, b });
                    assert!((0.0..360.0).contains(&o.h), "#{r:02x}{g:02x}{b:02x}: {o:?}");
                }
            }
        }
    }

    #[test]
    fn round_trips_within_one_step_per_channel() {
        let d = |x: u8, y: u8| (i16::from(x) - i16::from(y)).abs();
        for r in (0..=255u8).step_by(17) {
            for g in (0..=255u8).step_by(17) {
                for b in (0..=255u8).step_by(17) {
                    let src = Rgb { r, g, b };
                    let back = Oklch::from_rgb(src).to_rgb_clamped();
                    assert!(
                        d(src.r, back.r) <= 1 && d(src.g, back.g) <= 1 && d(src.b, back.b) <= 1,
                        "{src} -> {back}"
                    );
                }
            }
        }
    }

    #[test]
    fn out_of_gamut_keeps_lightness_and_hue() {
        let want = Oklch {
            l: 0.7,
            c: 0.4,
            h: 145.0,
        };
        let got = Oklch::from_rgb(want.to_rgb_clamped());
        assert!((got.l - 0.7).abs() < 0.01, "{got:?}");
        assert!((got.h - 145.0).abs() < 2.0, "{got:?}");
        assert!(got.c < 0.4, "{got:?}");
    }
}
