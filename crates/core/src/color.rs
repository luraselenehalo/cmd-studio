//! sRGB colors and the WCAG 2.x contrast ratio.
use std::borrow::Cow;
use std::fmt;
use std::str::FromStr;

/// An sRGB color with 8-bit channels. Serialized as `#rrggbb`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("not a #rrggbb color: {0:?}")]
pub struct ParseColorError(pub String);

impl FromStr for Rgb {
    type Err = ParseColorError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || ParseColorError(s.to_string());
        let hex = s.strip_prefix('#').ok_or_else(err)?;
        // Exactly six ASCII hex digits: `u8::from_str_radix` alone would also take a leading '+'.
        if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(err());
        }
        let channel = |digits: std::ops::Range<usize>| {
            hex.get(digits)
                .and_then(|pair| u8::from_str_radix(pair, 16).ok())
                .ok_or_else(err)
        };
        Ok(Rgb {
            r: channel(0..2)?,
            g: channel(2..4)?,
            b: channel(4..6)?,
        })
    }
}

impl fmt::Display for Rgb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }
}

impl serde::Serialize for Rgb {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> serde::Deserialize<'de> for Rgb {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

impl schemars::JsonSchema for Rgb {
    fn schema_name() -> Cow<'static, str> {
        "Color".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({ "type": "string", "pattern": "^#[0-9a-fA-F]{6}$" })
    }
}

/// sRGB channel (0–255) to linear light (0–1).
pub fn srgb_to_linear(c: u8) -> f64 {
    let c = f64::from(c) / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Linear light (0–1) to an sRGB channel, rounded and clamped to 0–255.
pub fn linear_to_srgb(l: f64) -> u8 {
    let c = if l <= 0.003_130_8 {
        12.92 * l
    } else {
        1.055 * l.powf(1.0 / 2.4) - 0.055
    };
    (c * 255.0).round().clamp(0.0, 255.0) as u8
}

/// WCAG 2.x relative luminance.
pub fn relative_luminance(c: Rgb) -> f64 {
    0.2126 * srgb_to_linear(c.r) + 0.7152 * srgb_to_linear(c.g) + 0.0722 * srgb_to_linear(c.b)
}

/// WCAG 2.x contrast ratio, from 1.0 to 21.0.
pub fn contrast_ratio(a: Rgb, b: Rgb) -> f64 {
    let (la, lb) = (relative_luminance(a), relative_luminance(b));
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(s: &str) -> Rgb {
        s.parse().unwrap()
    }

    #[test]
    fn parses_any_case_and_prints_lowercase() {
        assert_eq!(c("#D97757").to_string(), "#d97757");
        assert_eq!(
            c("#d97757"),
            Rgb {
                r: 0xd9,
                g: 0x77,
                b: 0x57
            }
        );
    }

    #[test]
    fn rejects_malformed_colors() {
        // The last two have six bytes after the '#', so the length check lets them through and
        // only the ASCII-hex check rejects them: `u8::from_str_radix` accepts a leading '+',
        // and U+00E9 is two bytes wide, so it straddles a byte pair.
        for s in [
            "d97757",
            "#d9775",
            "#d977577",
            "#gg0000",
            "",
            "#",
            "#+12345",
            "#1\u{e9}345",
        ] {
            assert!(s.parse::<Rgb>().is_err(), "{s:?} should not parse");
        }
    }

    #[test]
    fn black_on_white_is_21() {
        assert!((contrast_ratio(c("#000000"), c("#ffffff")) - 21.0).abs() < 1e-9);
    }

    #[test]
    fn grey_777_on_white_is_about_4_48() {
        let r = contrast_ratio(c("#777777"), c("#ffffff"));
        assert!((r - 4.48).abs() < 0.01, "{r}");
    }

    #[test]
    fn contrast_is_symmetric() {
        let (a, b) = (c("#d97757"), c("#1f1e1d"));
        assert_eq!(contrast_ratio(a, b), contrast_ratio(b, a));
    }

    #[test]
    fn terracotta_text_matches_the_prototype_check() {
        // spec Appendix A: foreground 15.8:1, muted 4.6:1
        let bg = c("#1f1e1d");
        assert!((contrast_ratio(c("#faf9f5"), bg) - 15.80).abs() < 0.01);
        assert!((contrast_ratio(c("#8a8780"), bg) - 4.64).abs() < 0.01);
    }

    #[test]
    fn linear_round_trip_is_exact_for_every_byte() {
        for v in 0..=255u8 {
            assert_eq!(linear_to_srgb(srgb_to_linear(v)), v);
        }
    }

    #[test]
    fn serde_uses_the_hex_string() {
        let json = serde_json::to_string(&c("#D97757")).unwrap();
        assert_eq!(json, "\"#d97757\"");
        let back: Rgb = serde_json::from_str("\"#D97757\"").unwrap();
        assert_eq!(back, c("#d97757"));
        assert!(serde_json::from_str::<Rgb>("\"#12345\"").is_err());
    }
}
