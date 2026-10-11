# Phase 1a — cmdstudio-core (Pure Core) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build every part of `cmdstudio-core` that never touches the machine: colors and contrast, OKLCH palette generation, the `.cmdtheme` model and its JSON Schema, the Terracotta preset, the generators for Oh My Posh, Clink and Windows Terminal, the planner, and the transactional apply engine (tested with fakes).

**Architecture:** A single Rust library crate in a Cargo workspace. Each function takes data and returns data. Every side effect sits behind the `txn::Executor`, `txn::Verifier` and `txn::JournalStore` traits, which Phase 1b implements for real Windows. The generators reproduce the shapes the prototype proved (spec Appendix A).

**Tech Stack:** Rust (pinned by Phase 0 Task 1), serde 1, serde_json 1, thiserror 2, uuid 1, schemars 1, proptest 1 (tests), mlua 0.10 with vendored Lua 5.2 (tests; Clink embeds Lua 5.2).

**Spec:** `docs/superpowers/specs/2026-10-10-cmd-studio-design.md`

**Prerequisite:** Phase 0 Task 1 (Rust installed and pinned). This plan touches no gated API. Phase 0 closed on 2026-10-11 (G1–G18 ✅, G19 ❌ with a chosen workaround); its Task 6 (G7 and G8 on the dev machine) is still open and does not block this plan.

**Revised 2026-10-11 after Phase 0:**
- Task 11 starts Clink through `<clink-launch>` (spec §5.4, G19). Clink 1.9.34 fails to inject when its own path has non-ASCII letters, and every cmd window then shows an error.
- Task 11 falls back to the bundled Nerd Font when the theme's font is not installed (spec §3.3). The Nerd Font's family name is `CaskaydiaCove NF` (G12).
- Task 10's Windows Terminal test now passes an ASCII Clink path, as the planner always will.

**Not in this plan:**
- **Phase 1b plan:**
  - the adapters (`WtSettings`, `AutoRun`, `Fonts`, `Fs`), `detect`, `doctor`, the on-disk snapshot store, the CLI, and the byte-identical `settings.json` guard test (spec §7 (ง));
  - filling `plan::Machine` for real, including `data_short`, `program_data_dir` (with the ownership and permission check of spec §5.4) and `fonts`;
  - recording `Plan::launch` in `state.json`, and removing the user's ProgramData folder on uninstall (spec §6.3).
- **Upstream presets** (Tokyo Night and the rest of spec §3.2): G14 passed on 2026-10-11, so nothing blocks them now. They go into the Phase 2 plan with the theme screens.
- **Phase 2 plan** (built alongside the theme screens): our own Retro green, Daylight and Paper presets, the preview emulator, advanced-mode editing, drafts, and undo.

## Global Constraints

- Crate `cmdstudio-core`, Rust edition 2021, license MIT; app identifier `ai.resincore.cmdstudio`.
- Code in this crate never touches the machine: no registry access, and no files outside a test's own temp dir.
- Contrast (WCAG 2.x formula, against the background): foreground ≥ 4.5:1; muted, accent, ansi 1–6 and ansi 9–14 ≥ 3:1.
- `.cmdtheme` rules:
  - `schemaVersion` is 1; unknown fields are rejected; files are at most 64 KB.
  - `name` is 1–64 characters; `author` is 0–64.
  - `tint` is 0–0.30; `fontSize` is 8–32; `opacity` is 30–100; each `padding` value is 0–40.
  - `cursorShape` is `filledBox`, `bar` or `underscore`; `layout` is `box`, `line` or `powerline`.
  - `segments` come from `path git time user host status duration` with no repeats.
  - Banner `title` is at most 80 characters; at most 4 lines, each at most 80 characters.
  - Banner variables are `{user} {host} {cwd} {date} {time}`; anything else stays literal.
- Oh My Posh config uses `"version": 4` and `options`. It may emit only these segment types: `path git time session status executiontime text`. Never `command`. `text` holds only frame glyphs that the generator itself sets.
- Windows Terminal: scheme name `CMD Studio`; cmd profile guid `{0caa0dad-35be-5f56-a8ff-afceeeaa6101}`. With target `all`, `commandline` is exactly `%SystemRoot%\System32\cmd.exe`.
- Clink is always started through `<clink-launch>`, never from a path with non-ASCII letters (spec §5.4, G19). The first of these that is all ASCII wins:
  1. the real path `<data>\runtime\clink-<ver>\clink_x64.exe`;
  2. the 8.3 path of `<data>` joined to `runtime\clink-<ver>\clink_x64.exe`;
  3. `<the user's ProgramData folder>\clink-<ver>\clink_x64.exe`. Clink's runtime is then installed there and not under `<data>`.

  If none is ASCII, the plan fails with `E_CLINK_PATH`. The Clink profile stays at `<data>\clink-profile`, whatever letters it holds.
- AutoRun:
  - Our entry is `if exist "<clink-launch>" "<clink-launch>" inject --autorun --quiet --profile "<data>\clink-profile"`.
  - It is appended with ` & `. Other programs' parts are never changed.
- A Clink path or profile path that contains `%` or `"` fails the plan with `E_CLINK_PATH`, for both targets.
- Fonts: the bundled Nerd Font's family name is `CaskaydiaCove NF`. Fonts are planned only when Windows Terminal is installed, because nothing else uses the theme's font. When the theme's font is not installed, Windows Terminal gets `CaskaydiaCove NF` instead, the plan installs it if it is missing, and the user is told (spec §3.3).
- Banner:
  - It is printed only from `clink.oninject`, guarded by the env flag `CMDSTUDIO_BANNER`.
  - The last printed line is one space, never an empty line.
  - Thai dates use the Buddhist year.
- Pinned runtimes: Oh My Posh `31.5.0`, Clink `1.9.34`.
- Production code has no `unwrap`, `expect` or `panic!`. Tests may use them. Errors are typed.
- Implementation Gate: before calling an external crate API, read its docs for the version in `Cargo.lock` (`cargo doc -p <crate> --no-deps --open`). If a signature differs from this plan, follow the real API and say so in the commit message.
- Tests run from the repo root with `cargo test -p cmdstudio-core`.
- Commits: write the English message to `.git/CMDSTUDIO_COMMIT_MSG` with the Write tool, then run `git commit -F .git/CMDSTUDIO_COMMIT_MSG`. Never add Claude attribution lines. Follow the `auroraz-pr-workflow` skill.

## Review Focus

1. A theme file saved by Notepad with a UTF-8 BOM must import; a UTF-16 file must fail with `E_THEME_INVALID`, not crash. Pinned in Task 5.
2. Achromatic accents (`#000000`, `#ffffff`, `#808080`) must give a valid palette with no NaN. Pinned in Task 3.
3. A Windows user name with Thai letters, spaces or `&` must work. Clink must start from an all-ASCII path (G19) while the profile path keeps its Thai letters. Paths with `%` or `"` must fail with `E_CLINK_PATH`, never be written as a broken cmd line. Pinned in Task 11.
4. A theme with no prompt segments, with the banner off, or with no banner lines must still generate a valid config and valid Lua. Pinned in Tasks 8 and 9.
5. Theme names that Windows forbids as file names (`CON`, `a/b`, trailing dots) must export to safe names. Pinned in Task 4.

## File map

```
Cargo.toml                         workspace
crates/core/Cargo.toml
crates/core/src/lib.rs             module list
crates/core/src/color.rs           Rgb, WCAG contrast
crates/core/src/oklch.rs           OKLab/OKLCH conversions, gamut clamp
crates/core/src/palette.rs         Tone, Colors, generate, contrast check and fix
crates/core/src/text.rs            sanitize_text, lua_str, safe_file_stem
crates/core/src/model.rs           Theme types, Target, parse/validate
crates/core/src/schema.rs          JSON Schema text
crates/core/src/presets.rs         embedded presets
crates/core/src/generate/mod.rs
crates/core/src/generate/omp.rs    Oh My Posh config
crates/core/src/generate/lua.rs    init.lua and banner.lua
crates/core/src/generate/clink.rs  clink_settings
crates/core/src/generate/wt.rs     WT scheme and profile values
crates/core/src/plan.rs            Paths, Machine, ClinkLaunch, Request, Op, plan(), AutoRun chain
crates/core/src/txn.rs             Journal, Stage, traits, run(), recover()
crates/core/tests/palette_props.rs
crates/core/tests/schema_file.rs
crates/core/tests/banner_lua.rs
crates/core/tests/fixtures/terracotta-mine.cmdtheme
presets/terracotta.cmdtheme
schema/cmdtheme-1.schema.json
```

---

### Task 1: Workspace and color primitives

**Files:**
- Create: `Cargo.toml`, `crates/core/Cargo.toml`, `crates/core/src/lib.rs`, `crates/core/src/color.rs`

**Interfaces:**
- Produces:
  - `color::Rgb { r: u8, g: u8, b: u8 }`: `Copy + Eq + Hash`. It implements `FromStr` (`#rrggbb`, either case), `Display` (lowercase `#rrggbb`), and serde as that string. Its `JsonSchema` is named `Color`.
  - `color::ParseColorError`
  - `color::srgb_to_linear(u8) -> f64`
  - `color::linear_to_srgb(f64) -> u8`
  - `color::relative_luminance(Rgb) -> f64`
  - `color::contrast_ratio(Rgb, Rgb) -> f64`

- [ ] **Step 1: Create the workspace and a failing test**

`Cargo.toml`:
```toml
[workspace]
resolver = "2"
members = ["crates/core"]

[workspace.package]
version = "0.1.0"
edition = "2021"
license = "MIT"
```

`crates/core/Cargo.toml`:
```toml
[package]
name = "cmdstudio-core"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
uuid = { version = "1", features = ["v4", "serde"] }
schemars = { version = "1", features = ["uuid1"] }

[dev-dependencies]
proptest = "1"
```

`crates/core/src/lib.rs`:
```rust
//! CMD Studio core: theme model, palette, generators, planner and the
//! transactional apply engine. Nothing here touches the machine directly;
//! side effects go through the traits in `txn`.
pub mod color;
```

`crates/core/src/color.rs` (tests only for now):
```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn c(s: &str) -> Rgb {
        s.parse().unwrap()
    }

    #[test]
    fn parses_any_case_and_prints_lowercase() {
        assert_eq!(c("#D97757").to_string(), "#d97757");
        assert_eq!(c("#d97757"), Rgb { r: 0xd9, g: 0x77, b: 0x57 });
    }

    #[test]
    fn rejects_malformed_colors() {
        for s in ["d97757", "#d9775", "#d977577", "#gg0000", "", "#"] {
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
```

- [ ] **Step 2: Run the tests and confirm they fail**

Run: `cargo test -p cmdstudio-core color`
Expected: compile error `cannot find type Rgb in this scope`.

- [ ] **Step 3: Implement `color.rs` above the test module**

```rust
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
        if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(err());
        }
        let channel = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| err());
        Ok(Rgb { r: channel(0)?, g: channel(2)?, b: channel(4)? })
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
    let c = if l <= 0.003_130_8 { 12.92 * l } else { 1.055 * l.powf(1.0 / 2.4) - 0.055 };
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
```

- [ ] **Step 4: Run the tests and confirm they pass**

Run: `cargo test -p cmdstudio-core color`
Expected: 8 passed. If `schemars::json_schema!` or `SchemaGenerator` fails to resolve, check the schemars version in `Cargo.lock` against its docs (Implementation Gate) and adjust the impl.

- [ ] **Step 5: Commit**

Message:
```text
feat(core): add workspace and sRGB color primitives

Rgb parses and prints #rrggbb and serializes as that string. Contrast
follows WCAG 2.x and reproduces the prototype's Terracotta numbers.
```
Run:
```bash
git add -- Cargo.toml Cargo.lock crates/core/Cargo.toml crates/core/src/lib.rs crates/core/src/color.rs
git diff --cached --name-only
git commit -F .git/CMDSTUDIO_COMMIT_MSG
```

---

### Task 2: OKLCH conversions

**Files:**
- Create: `crates/core/src/oklch.rs`
- Modify: `crates/core/src/lib.rs` (add `pub mod oklch;`)

**Interfaces:**
- Consumes: `color::{Rgb, srgb_to_linear, linear_to_srgb}`
- Produces:
  - `oklch::Oklch { l: f64, c: f64, h: f64 }` (`Copy`), with `h` in degrees 0–360
  - `Oklch::from_rgb(Rgb) -> Oklch`
  - `Oklch::to_rgb_clamped(self) -> Rgb`: out-of-gamut colors keep L and h and lose chroma.

- [ ] **Step 1: Write the failing tests** in a new `crates/core/src/oklch.rs`, and add `pub mod oklch;` to `lib.rs`:
```rust
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
    fn round_trips_within_one_step_per_channel() {
        let d = |x: u8, y: u8| (i16::from(x) - i16::from(y)).abs();
        for r in (0..=255u8).step_by(17) {
            for g in (0..=255u8).step_by(17) {
                for b in (0..=255u8).step_by(17) {
                    let src = Rgb { r, g, b };
                    let back = Oklch::from_rgb(src).to_rgb_clamped();
                    assert!(d(src.r, back.r) <= 1 && d(src.g, back.g) <= 1 && d(src.b, back.b) <= 1, "{src} -> {back}");
                }
            }
        }
    }

    #[test]
    fn out_of_gamut_keeps_lightness_and_hue() {
        let want = Oklch { l: 0.7, c: 0.4, h: 145.0 };
        let got = Oklch::from_rgb(want.to_rgb_clamped());
        assert!((got.l - 0.7).abs() < 0.01, "{got:?}");
        assert!((got.h - 145.0).abs() < 2.0, "{got:?}");
        assert!(got.c < 0.4, "{got:?}");
    }
}
```

- [ ] **Step 2: Run the tests and confirm they fail**

Run: `cargo test -p cmdstudio-core oklch`
Expected: compile error `cannot find type Oklch`.

- [ ] **Step 3: Implement it above the tests**
```rust
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
    [r, g, b].iter().all(|v| (-GAMUT_EPS..=1.0 + GAMUT_EPS).contains(v))
}

impl Oklch {
    pub fn from_rgb(c: Rgb) -> Self {
        let (l, a, b) = linear_to_oklab(srgb_to_linear(c.r), srgb_to_linear(c.g), srgb_to_linear(c.b));
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
        let mut lch = Oklch { l: self.l.clamp(0.0, 1.0), c: self.c.max(0.0), h: self.h };
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
        Rgb { r: linear_to_srgb(r), g: linear_to_srgb(g), b: linear_to_srgb(b) }
    }
}
```

- [ ] **Step 4: Run the tests and confirm they pass**

Run: `cargo test -p cmdstudio-core oklch`
Expected: 5 passed.

- [ ] **Step 5: Commit**

Message: `feat(core): add OKLCH conversions with gamut clamping`
```bash
git add -- crates/core/src/oklch.rs crates/core/src/lib.rs
git diff --cached --name-only
git commit -F .git/CMDSTUDIO_COMMIT_MSG
```

---

### Task 3: Palette generation and the readability check

**Files:**
- Create: `crates/core/src/palette.rs`, `crates/core/tests/palette_props.rs`
- Modify: `crates/core/src/lib.rs` (add `pub mod palette;`)

**Interfaces:**
- Consumes: `color::{Rgb, contrast_ratio}`, `oklch::Oklch`
- Produces:
  - `palette::Tone { Dark, Light }`: serde `"dark"` / `"light"`.
  - `palette::Colors { background, foreground, cursor, selection, accent, muted, frame: Rgb, ansi: [Rgb; 16] }`: serde with `deny_unknown_fields`.
  - `palette::MIN_TEXT_CONTRAST = 4.5`, `palette::MIN_OTHER_CONTRAST = 3.0`
  - `palette::Slot { Foreground, Muted, Accent, Ansi(usize) }`, with `Colors::get(Slot) -> Rgb` and `Colors::set(Slot, Rgb)`.
  - `palette::checked_slots() -> Vec<(Slot, f64)>`
  - `palette::ContrastIssue { slot, ratio, required }`, from `palette::contrast_issues(&Colors) -> Vec<ContrastIssue>`.
  - `palette::fix_lightness(Rgb, Rgb, f64) -> Rgb`
  - `palette::fix_all(&mut Colors)`
  - `palette::generate(Tone, Rgb, f64) -> Colors`

- [ ] **Step 1: Write the failing tests**

Create `crates/core/src/palette.rs` with only this test module, and add `pub mod palette;` to `lib.rs`:
```rust
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
        assert_eq!(p.accent, c("#d97757"), "the accent already passes on dark and must not move");
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
    fn achromatic_accents_still_give_valid_palettes() {
        // Review Focus 2
        for s in ["#000000", "#ffffff", "#808080"] {
            for tone in [Tone::Dark, Tone::Light] {
                let p = generate(tone, c(s), 0.30);
                assert!(contrast_issues(&p).is_empty(), "{s} {tone:?}: {:?}", contrast_issues(&p));
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
        assert!(!slots.iter().any(|(s, _)| matches!(s, Slot::Ansi(0 | 7 | 8 | 15))));
    }
}
```

`crates/core/tests/palette_props.rs`:
```rust
use cmdstudio_core::color::{contrast_ratio, Rgb};
use cmdstudio_core::palette::{checked_slots, generate, Tone};
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    #[test]
    fn generated_palettes_meet_every_threshold(
        r in 0u8..=255, g in 0u8..=255, b in 0u8..=255,
        dark in any::<bool>(), tint_index in 0usize..3,
    ) {
        let tint = [0.0, 0.15, 0.30][tint_index];
        let tone = if dark { Tone::Dark } else { Tone::Light };
        let p = generate(tone, Rgb { r, g, b }, tint);
        for (slot, min) in checked_slots() {
            let ratio = contrast_ratio(p.get(slot), p.background);
            prop_assert!(ratio >= min, "{slot:?} {ratio:.2} < {min} for #{r:02x}{g:02x}{b:02x} {tone:?} tint {tint}");
        }
    }
}
```

- [ ] **Step 2: Run the tests and confirm they fail**

Run: `cargo test -p cmdstudio-core palette`
Expected: compile errors (`generate`, `Tone`, `Colors` not found).

- [ ] **Step 3: Implement it above the tests**
```rust
//! Simple-mode palette generation and the readability (contrast) check
//! (spec §3.4).
use crate::color::{contrast_ratio, Rgb};
use crate::oklch::Oklch;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Tone {
    Dark,
    Light,
}

/// The 23 colors of a theme. `ansi` uses the standard order: black, red,
/// green, yellow, blue, magenta, cyan, white, then the 8 bright variants.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Colors {
    pub background: Rgb,
    pub foreground: Rgb,
    pub cursor: Rgb,
    pub selection: Rgb,
    pub accent: Rgb,
    pub muted: Rgb,
    pub frame: Rgb,
    pub ansi: [Rgb; 16],
}

pub const MIN_TEXT_CONTRAST: f64 = 4.5;
pub const MIN_OTHER_CONTRAST: f64 = 3.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    Foreground,
    Muted,
    Accent,
    Ansi(usize),
}

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
    v.extend((1..=6).chain(9..=14).map(|i| (Slot::Ansi(i), MIN_OTHER_CONTRAST)));
    v
}

impl Colors {
    pub fn get(&self, slot: Slot) -> Rgb {
        match slot {
            Slot::Foreground => self.foreground,
            Slot::Muted => self.muted,
            Slot::Accent => self.accent,
            Slot::Ansi(i) => self.ansi[i],
        }
    }

    pub fn set(&mut self, slot: Slot, value: Rgb) {
        match slot {
            Slot::Foreground => self.foreground = value,
            Slot::Muted => self.muted = value,
            Slot::Accent => self.accent = value,
            Slot::Ansi(i) => self.ansi[i] = value,
        }
    }
}

pub fn contrast_issues(colors: &Colors) -> Vec<ContrastIssue> {
    checked_slots()
        .into_iter()
        .filter_map(|(slot, required)| {
            let ratio = contrast_ratio(colors.get(slot), colors.background);
            (ratio < required).then_some(ContrastIssue { slot, ratio, required })
        })
        .collect()
}

/// Changes only the OKLCH lightness of `color`, away from `background`, until
/// it reaches `min` (or lightness runs out). Hue never changes.
pub fn fix_lightness(color: Rgb, background: Rgb, min: f64) -> Rgb {
    if contrast_ratio(color, background) >= min {
        return color;
    }
    let white = Rgb { r: 255, g: 255, b: 255 };
    let black = Rgb { r: 0, g: 0, b: 0 };
    let step = if contrast_ratio(white, background) >= contrast_ratio(black, background) { 0.005 } else { -0.005 };
    let mut lch = Oklch::from_rgb(color);
    let mut out = color;
    while (0.0..=1.0).contains(&lch.l) {
        lch.l += step;
        out = Oklch { l: lch.l.clamp(0.0, 1.0), ..lch }.to_rgb_clamped();
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

/// Simple mode: all 23 colors from a tone, one accent and a tint (0–0.30).
/// Lightness and chroma values are the starting values in spec §3.4.
pub fn generate(tone: Tone, accent: Rgb, tint: f64) -> Colors {
    let dark = tone == Tone::Dark;
    let pick = |d: f64, l: f64| if dark { d } else { l };
    let t = tint.clamp(0.0, 0.30);
    let h = Oklch::from_rgb(accent).h;
    let mk = |l: f64, c: f64, hue: f64| Oklch { l, c, h: hue }.to_rgb_clamped();

    let background = mk(pick(0.20, 0.97), t * pick(0.08, 0.04), h);
    let mut ansi = [background; 16];
    ansi[0] = mk(pick(0.30, 0.25), 0.01, h);
    ansi[8] = mk(pick(0.50, 0.45), 0.01, h);
    ansi[7] = mk(0.85, 0.01, h);
    ansi[15] = mk(pick(0.97, 0.98), 0.01, h);
    for (i, hue) in ANSI_HUES.iter().enumerate() {
        ansi[i + 1] = mk(pick(0.72, 0.52), 0.12, *hue);
        ansi[i + 9] = mk(pick(0.80, 0.45), 0.12, *hue);
    }
    let mut colors = Colors {
        background,
        foreground: mk(pick(0.94, 0.25), 0.01, h),
        cursor: accent,
        selection: mk(pick(0.32, 0.88), 0.02, h),
        accent,
        muted: mk(pick(0.68, 0.50), 0.02, h),
        frame: mk(pick(0.45, 0.80), 0.015, h),
        ansi,
    };
    fix_all(&mut colors);
    colors.cursor = colors.accent;
    colors
}
```

- [ ] **Step 4: Run the tests and confirm they pass**

Run: `cargo test -p cmdstudio-core palette`
Expected: 7 unit tests and 1 property test (1000 cases) pass. If the property test finds a failing accent, keep its seed. Before you change any lightness value in `generate`, report the counterexample to the founder, because the spec table says those numbers are tunable only if the property still holds.

- [ ] **Step 5: Commit**

Message:
```text
feat(core): generate simple-mode palettes in OKLCH

generate() builds all 23 colors from a tone, one accent and a tint, then
raises any color below its WCAG threshold by changing lightness only.
A property test checks 1000 random accents across both tones.
```
```bash
git add -- crates/core/src/palette.rs crates/core/tests/palette_props.rs crates/core/src/lib.rs
git diff --cached --name-only
git commit -F .git/CMDSTUDIO_COMMIT_MSG
```

---

### Task 4: Text helpers

**Files:**
- Create: `crates/core/src/text.rs`
- Modify: `crates/core/src/lib.rs` (add `pub mod text;`)

**Interfaces:**
- Produces:
  - `text::sanitize_text(&str) -> String`: removes CSI escape sequences, two-character escapes, and every C0/C1 control character.
  - `text::lua_str(&str) -> String`: a double-quoted Lua 5.2 literal.
  - `text::safe_file_stem(&str) -> String`: a Windows-safe file name stem.

- [ ] **Step 1: Write the failing tests** in a new `crates/core/src/text.rs`, and add `pub mod text;` to `lib.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_escape_sequences_and_controls() {
        assert_eq!(sanitize_text("a\u{1b}[31mred\u{1b}[0m\u{7}b\n"), "aredb");
        assert_eq!(sanitize_text("x\u{1b}cy"), "xy");
        assert_eq!(sanitize_text("x\u{9b}31my"), "x31my");
    }

    #[test]
    fn keeps_thai_and_emoji() {
        assert_eq!(sanitize_text("สวัสดี 👋"), "สวัสดี 👋");
    }

    #[test]
    fn lua_str_escapes_quotes_backslashes_and_controls() {
        assert_eq!(lua_str(r#"a"b\c"#), r#""a\"b\\c""#);
        assert_eq!(lua_str("x]]y"), "\"x]]y\"");
        assert_eq!(lua_str("t\tz"), "\"t\\009z\"");
        assert_eq!(lua_str("ไทย"), "\"ไทย\"");
        assert_eq!(lua_str("\u{1b}[0m"), "\"\\027[0m\"");
    }

    #[test]
    fn file_stems_are_windows_safe() {
        // Review Focus 5
        assert_eq!(safe_file_stem("a/b:c*?"), "a_b_c__");
        assert_eq!(safe_file_stem("CON"), "CON_");
        assert_eq!(safe_file_stem("lpt1"), "lpt1_");
        assert_eq!(safe_file_stem("  ธีม.  "), "ธีม");
        assert_eq!(safe_file_stem("..."), "theme");
        assert_eq!(safe_file_stem(&"x".repeat(150)).chars().count(), 100);
    }
}
```

- [ ] **Step 2: Run the tests and confirm they fail**

Run: `cargo test -p cmdstudio-core text`
Expected: compile errors (functions not found).

- [ ] **Step 3: Implement it above the tests**
```rust
//! Text helpers shared by the theme model and the generators (spec §3.7).

/// Removes escape sequences and every C0/C1 control character, so text from a
/// shared theme file can never drive the terminal.
pub fn sanitize_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                // CSI: skip parameters and intermediates up to the final byte.
                for c in chars.by_ref() {
                    if ('\u{40}'..='\u{7e}').contains(&c) {
                        break;
                    }
                }
            } else {
                chars.next();
            }
            continue;
        }
        if !matches!(u32::from(ch), 0x00..=0x1F | 0x7F..=0x9F) {
            out.push(ch);
        }
    }
    out
}

/// Quotes `s` as a double-quoted Lua 5.2 string literal. Control characters
/// become three-digit decimal escapes; UTF-8 text passes through.
pub fn lua_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            c if u32::from(c) < 0x20 || u32::from(c) == 0x7F => out.push_str(&format!("\\{:03}", u32::from(c))),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

const RESERVED_NAMES: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Turns a theme name into a file name stem that Windows accepts.
pub fn safe_file_stem(name: &str) -> String {
    let replaced: String = name
        .chars()
        .map(|c| if matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || u32::from(c) < 0x20 { '_' } else { c })
        .collect();
    let mut stem = replaced.trim().trim_end_matches(['.', ' ']).to_string();
    if stem.is_empty() {
        stem = "theme".to_string();
    }
    if RESERVED_NAMES.iter().any(|r| r.eq_ignore_ascii_case(&stem)) {
        stem.push('_');
    }
    stem.chars().take(100).collect()
}
```

- [ ] **Step 4: Run the tests and confirm they pass**

Run: `cargo test -p cmdstudio-core text`
Expected: 4 passed.

- [ ] **Step 5: Commit**

Message: `feat(core): add text sanitizing, Lua quoting and safe file names`
```bash
git add -- crates/core/src/text.rs crates/core/src/lib.rs
git diff --cached --name-only
git commit -F .git/CMDSTUDIO_COMMIT_MSG
```

---

### Task 5: The `.cmdtheme` model

**Files:**
- Create: `crates/core/src/model.rs`, `crates/core/tests/fixtures/terracotta-mine.cmdtheme`
- Modify: `crates/core/src/lib.rs` (add `pub mod model;`)

**Interfaces:**
- Consumes: `color::Rgb`, `palette::{Colors, Tone}`, `text::sanitize_text`
- Produces:
  - `model::SCHEMA_VERSION = 1`, `model::MAX_FILE_BYTES = 65536`
  - `model::Theme { schema_version: u32, id: uuid::Uuid, name: String, author: String, tone: Tone, simple: Option<SimpleRecipe>, colors: Colors, terminal: TerminalSettings, prompt: PromptSettings, banner: BannerSettings }`
  - `model::SimpleRecipe { accent: Rgb, tint: f64 }`
  - `model::TerminalSettings { font: String, font_size: u8, cursor_shape: CursorShape, opacity: u8, acrylic: bool, padding: [u8; 2] }`
  - `model::CursorShape { FilledBox, Bar, Underscore }`
  - `model::PromptSettings { layout: Layout, segments: Vec<Segment>, transient: bool }`
  - `model::Layout { Box, Line, Powerline }`
  - `model::Segment { Path, Git, Time, User, Host, Status, Duration }`
  - `model::BannerSettings { enabled: bool, title: String, lines: Vec<String>, tips: bool, clear_logo: bool }`
  - `model::Target { All, Wt }`: serde `"all"` / `"wt"`.
  - `model::ThemeError { TooLarge, NewerSchema(u64), Invalid(String), Field { field: &'static str, reason: &'static str } }`, with `ThemeError::code() -> &'static str`.
  - `model::parse_theme(&[u8]) -> Result<Theme, ThemeError>`
  - `Theme::sanitize(&mut self)`, `Theme::validate(&self) -> Result<(), ThemeError>`, `Theme::to_file_bytes(&self) -> Result<Vec<u8>, ThemeError>`

- [ ] **Step 1: Add the fixture** (the spec §3.3 example) at `crates/core/tests/fixtures/terracotta-mine.cmdtheme`:
```json
{
  "schemaVersion": 1,
  "id": "2f0c8f0e-6a8e-4b7e-9a4f-3f1f0b9c1d2e",
  "name": "Terracotta ของฉัน",
  "author": "",
  "tone": "dark",
  "simple": { "accent": "#d97757", "tint": 0.10 },
  "colors": {
    "background": "#1f1e1d",
    "foreground": "#faf9f5",
    "cursor": "#d97757",
    "selection": "#4a4845",
    "accent": "#d97757",
    "muted": "#8a8780",
    "frame": "#5f5d58",
    "ansi": [
      "#3a3936", "#e5736a", "#8fbf7f", "#e3b66b", "#7fa7d9", "#b79bdb", "#7fc1b8", "#d8d6cf",
      "#8a8780", "#f0907f", "#a8d196", "#eecb8a", "#9bbde8", "#cbb4e8", "#9dd3cb", "#faf9f5"
    ]
  },
  "terminal": {
    "font": "Cascadia Mono",
    "fontSize": 12,
    "cursorShape": "filledBox",
    "opacity": 96,
    "acrylic": true,
    "padding": [14, 10]
  },
  "prompt": {
    "layout": "box",
    "segments": ["path", "git", "time"],
    "transient": true
  },
  "banner": {
    "enabled": true,
    "title": "Welcome back, {user}!",
    "lines": ["cwd: {cwd}", "{date} · {time} · {host}"],
    "tips": true,
    "clearLogo": true
  }
}
```

- [ ] **Step 2: Write the failing tests** in a new `crates/core/src/model.rs`, and add `pub mod model;` to `lib.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    const FIXTURE: &str = include_str!("../tests/fixtures/terracotta-mine.cmdtheme");

    fn with(edit: impl FnOnce(&mut Value)) -> Result<Theme, ThemeError> {
        let mut v: Value = serde_json::from_str(FIXTURE).unwrap();
        edit(&mut v);
        parse_theme(&serde_json::to_vec(&v).unwrap())
    }

    fn field_err(field: &'static str, reason: &'static str) -> ThemeError {
        ThemeError::Field { field, reason }
    }

    #[test]
    fn parses_the_spec_example() {
        let t = parse_theme(FIXTURE.as_bytes()).unwrap();
        assert_eq!(t.name, "Terracotta ของฉัน");
        assert_eq!(t.tone, Tone::Dark);
        assert_eq!(t.prompt.layout, Layout::Box);
        assert_eq!(t.prompt.segments, vec![Segment::Path, Segment::Git, Segment::Time]);
        assert_eq!(t.terminal.cursor_shape, CursorShape::FilledBox);
        assert!(t.banner.clear_logo);
    }

    #[test]
    fn rejects_unknown_fields_at_any_depth() {
        assert_eq!(with(|v| v["extra"] = json!(1)).unwrap_err().code(), "E_THEME_INVALID");
        assert!(with(|v| v["banner"]["sound"] = json!(true)).is_err());
        assert!(with(|v| v["colors"]["glow"] = json!("#ffffff")).is_err());
    }

    #[test]
    fn rejects_newer_and_zero_schema_versions() {
        let e = with(|v| v["schemaVersion"] = json!(2)).unwrap_err();
        assert_eq!(e, ThemeError::NewerSchema(2));
        assert_eq!(e.code(), "E_THEME_NEWER_SCHEMA");
        assert!(with(|v| v["schemaVersion"] = json!(0)).is_err());
        assert!(with(|v| v["schemaVersion"] = json!("1")).is_err());
    }

    #[test]
    fn rejects_bad_colors_and_short_ansi() {
        assert!(with(|v| v["colors"]["accent"] = json!("#12345")).is_err());
        assert!(with(|v| {
            v["colors"]["ansi"].as_array_mut().unwrap().pop();
        })
        .is_err());
    }

    #[test]
    fn rejects_values_outside_the_enums() {
        assert!(with(|v| v["prompt"]["layout"] = json!("fancy")).is_err());
        assert!(with(|v| v["prompt"]["segments"] = json!(["path", "command"])).is_err());
        assert!(with(|v| v["terminal"]["cursorShape"] = json!("vintage")).is_err());
    }

    #[test]
    fn strips_control_characters_from_text() {
        let t = with(|v| {
            v["name"] = json!("A\u{1b}[31mB\u{7}");
            v["banner"]["title"] = json!("hi\u{1b}]0;evil\u{7}");
        })
        .unwrap();
        assert_eq!(t.name, "AB");
        assert_eq!(t.banner.title, "hi0;evil");
    }

    #[test]
    fn enforces_limits() {
        let name = field_err("name", "must be 1–64 characters");
        assert_eq!(with(|v| v["name"] = json!("x".repeat(65))).unwrap_err(), name);
        assert_eq!(with(|v| v["name"] = json!("\u{7}\u{7}")).unwrap_err(), name);
        assert_eq!(with(|v| v["terminal"]["fontSize"] = json!(7)).unwrap_err(), field_err("terminal.fontSize", "must be 8–32"));
        assert_eq!(with(|v| v["terminal"]["opacity"] = json!(29)).unwrap_err(), field_err("terminal.opacity", "must be 30–100"));
        assert_eq!(with(|v| v["terminal"]["padding"] = json!([41, 10])).unwrap_err(), field_err("terminal.padding", "each value must be 0–40"));
        assert_eq!(with(|v| v["simple"]["tint"] = json!(0.31)).unwrap_err(), field_err("simple.tint", "must be 0–0.30"));
        assert_eq!(with(|v| v["prompt"]["segments"] = json!(["git", "git"])).unwrap_err(), field_err("prompt.segments", "must not repeat"));
        assert_eq!(with(|v| v["banner"]["title"] = json!("t".repeat(81))).unwrap_err(), field_err("banner.title", "must be at most 80 characters"));
        assert_eq!(with(|v| v["banner"]["lines"] = json!(["a", "b", "c", "d", "e"])).unwrap_err(), field_err("banner.lines", "at most 4 lines of at most 80 characters"));
        assert_eq!(with(|v| v["banner"]["lines"] = json!(["l".repeat(81)])).unwrap_err(), field_err("banner.lines", "at most 4 lines of at most 80 characters"));
    }

    #[test]
    fn accepts_a_utf8_bom() {
        // Review Focus 1
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(FIXTURE.as_bytes());
        assert!(parse_theme(&bytes).is_ok());
    }

    #[test]
    fn rejects_utf16_cleanly() {
        // Review Focus 1
        let bytes: Vec<u8> = [0xFF, 0xFE].into_iter().chain(FIXTURE.encode_utf16().flat_map(u16::to_le_bytes)).collect();
        assert!(matches!(parse_theme(&bytes), Err(ThemeError::Invalid(_))));
    }

    #[test]
    fn rejects_oversized_files() {
        let bytes = vec![b' '; MAX_FILE_BYTES + 1];
        assert_eq!(parse_theme(&bytes).unwrap_err(), ThemeError::TooLarge);
    }

    #[test]
    fn round_trips_through_the_file_format() {
        let t = parse_theme(FIXTURE.as_bytes()).unwrap();
        assert_eq!(parse_theme(&t.to_file_bytes().unwrap()).unwrap(), t);
    }

    #[test]
    fn target_uses_cli_words() {
        assert_eq!(serde_json::to_string(&Target::All).unwrap(), "\"all\"");
        assert_eq!(serde_json::to_string(&Target::Wt).unwrap(), "\"wt\"");
    }
}
```

- [ ] **Step 3: Run the tests and confirm they fail**

Run: `cargo test -p cmdstudio-core model`
Expected: compile errors (types not found).

- [ ] **Step 4: Implement it above the tests**
```rust
//! The `.cmdtheme` v1 model (spec §3.3): parsing, sanitizing, validation.
use crate::color::Rgb;
use crate::palette::{Colors, Tone};
use crate::text::sanitize_text;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_FILE_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Theme {
    pub schema_version: u32,
    pub id: uuid::Uuid,
    pub name: String,
    #[serde(default)]
    pub author: String,
    pub tone: Tone,
    pub simple: Option<SimpleRecipe>,
    pub colors: Colors,
    pub terminal: TerminalSettings,
    pub prompt: PromptSettings,
    pub banner: BannerSettings,
}

/// Inputs of simple mode, kept so the theme can be edited there again.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SimpleRecipe {
    pub accent: Rgb,
    pub tint: f64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TerminalSettings {
    pub font: String,
    pub font_size: u8,
    pub cursor_shape: CursorShape,
    pub opacity: u8,
    pub acrylic: bool,
    pub padding: [u8; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum CursorShape {
    FilledBox,
    Bar,
    Underscore,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PromptSettings {
    pub layout: Layout,
    pub segments: Vec<Segment>,
    pub transient: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum Layout {
    Box,
    Line,
    Powerline,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum Segment {
    Path,
    Git,
    Time,
    User,
    Host,
    Status,
    Duration,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct BannerSettings {
    pub enabled: bool,
    pub title: String,
    pub lines: Vec<String>,
    pub tips: bool,
    pub clear_logo: bool,
}

/// Where a theme is applied (spec §4.2 page 4; CLI `--target`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Target {
    All,
    Wt,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ThemeError {
    #[error("theme file is larger than 64 KB")]
    TooLarge,
    #[error("theme file comes from a newer CMD Studio (schemaVersion {0})")]
    NewerSchema(u64),
    #[error("not a valid theme file: {0}")]
    Invalid(String),
    #[error("invalid {field}: {reason}")]
    Field { field: &'static str, reason: &'static str },
}

impl ThemeError {
    /// Stable code shown to users (spec §5.9).
    pub fn code(&self) -> &'static str {
        match self {
            ThemeError::NewerSchema(_) => "E_THEME_NEWER_SCHEMA",
            _ => "E_THEME_INVALID",
        }
    }
}

fn field(field: &'static str, reason: &'static str) -> Result<(), ThemeError> {
    Err(ThemeError::Field { field, reason })
}

impl Theme {
    /// Removes escape sequences and control characters from all free text.
    pub fn sanitize(&mut self) {
        self.name = sanitize_text(&self.name);
        self.author = sanitize_text(&self.author);
        self.banner.title = sanitize_text(&self.banner.title);
        for line in &mut self.banner.lines {
            *line = sanitize_text(line);
        }
    }

    pub fn validate(&self) -> Result<(), ThemeError> {
        let name_len = self.name.chars().count();
        if name_len == 0 || name_len > 64 {
            return field("name", "must be 1–64 characters");
        }
        if self.author.chars().count() > 64 {
            return field("author", "must be at most 64 characters");
        }
        if let Some(s) = &self.simple {
            if !(0.0..=0.30).contains(&s.tint) {
                return field("simple.tint", "must be 0–0.30");
            }
        }
        let t = &self.terminal;
        let font_len = t.font.trim().chars().count();
        if font_len == 0 || font_len > 64 {
            return field("terminal.font", "must be 1–64 characters");
        }
        if !(8..=32).contains(&t.font_size) {
            return field("terminal.fontSize", "must be 8–32");
        }
        if !(30..=100).contains(&t.opacity) {
            return field("terminal.opacity", "must be 30–100");
        }
        if t.padding.iter().any(|p| *p > 40) {
            return field("terminal.padding", "each value must be 0–40");
        }
        let mut seen = HashSet::new();
        if !self.prompt.segments.iter().all(|s| seen.insert(*s)) {
            return field("prompt.segments", "must not repeat");
        }
        if self.banner.title.chars().count() > 80 {
            return field("banner.title", "must be at most 80 characters");
        }
        if self.banner.lines.len() > 4 || self.banner.lines.iter().any(|l| l.chars().count() > 80) {
            return field("banner.lines", "at most 4 lines of at most 80 characters");
        }
        Ok(())
    }

    /// The bytes written when the theme is exported or saved.
    pub fn to_file_bytes(&self) -> Result<Vec<u8>, ThemeError> {
        let mut bytes = serde_json::to_vec_pretty(self).map_err(|e| ThemeError::Invalid(e.to_string()))?;
        bytes.push(b'\n');
        Ok(bytes)
    }
}

/// Parses, sanitizes and validates a `.cmdtheme` file.
pub fn parse_theme(bytes: &[u8]) -> Result<Theme, ThemeError> {
    if bytes.len() > MAX_FILE_BYTES {
        return Err(ThemeError::TooLarge);
    }
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let value: serde_json::Value = serde_json::from_slice(bytes).map_err(|e| ThemeError::Invalid(e.to_string()))?;
    let version = value
        .get("schemaVersion")
        .and_then(serde_json::Value::as_u64)
        .ok_or(ThemeError::Field { field: "schemaVersion", reason: "missing or not a whole number" })?;
    if version == 0 {
        return Err(ThemeError::Field { field: "schemaVersion", reason: "must be 1 or higher" });
    }
    if version > u64::from(SCHEMA_VERSION) {
        return Err(ThemeError::NewerSchema(version));
    }
    let mut theme: Theme = serde_json::from_value(value).map_err(|e| ThemeError::Invalid(e.to_string()))?;
    theme.sanitize();
    theme.validate()?;
    Ok(theme)
}
```

- [ ] **Step 5: Run the tests and confirm they pass**

Run: `cargo test -p cmdstudio-core model`
Expected: 12 passed.

- [ ] **Step 6: Commit**

Message:
```text
feat(core): add the .cmdtheme v1 model

Strict parsing (unknown fields rejected, newer schemas refused), text
sanitizing, the spec's limits, and BOM handling for files saved by
Notepad.
```
```bash
git add -- crates/core/src/model.rs crates/core/tests/fixtures/terracotta-mine.cmdtheme crates/core/src/lib.rs
git diff --cached --name-only
git commit -F .git/CMDSTUDIO_COMMIT_MSG
```

---

### Task 6: The published JSON Schema

**Files:**
- Create: `crates/core/src/schema.rs`, `crates/core/tests/schema_file.rs`, `schema/cmdtheme-1.schema.json` (generated)
- Modify: `crates/core/src/lib.rs` (add `pub mod schema;`)

**Interfaces:**
- Consumes: `model::Theme`
- Produces: `schema::theme_schema_json() -> Result<String, serde_json::Error>`, plus the committed file `schema/cmdtheme-1.schema.json`.

- [ ] **Step 1: Write the failing test** `crates/core/tests/schema_file.rs`:
```rust
use std::path::PathBuf;

fn schema_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../schema/cmdtheme-1.schema.json")
}

#[test]
fn schema_file_matches_the_rust_types() {
    let generated = cmdstudio_core::schema::theme_schema_json().unwrap();
    let path = schema_path();
    if std::env::var_os("UPDATE_SCHEMA").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &generated).unwrap();
    }
    let on_disk = std::fs::read_to_string(&path).expect("missing schema file: run with UPDATE_SCHEMA=1 once");
    assert_eq!(on_disk.replace("\r\n", "\n"), generated, "schema file is stale: rerun with UPDATE_SCHEMA=1 and commit it");
}

#[test]
fn schema_is_strict_and_requires_the_version() {
    let schema: serde_json::Value = serde_json::from_str(&cmdstudio_core::schema::theme_schema_json().unwrap()).unwrap();
    assert_eq!(schema["additionalProperties"], serde_json::json!(false));
    assert!(schema["required"].as_array().unwrap().iter().any(|v| v == "schemaVersion"));
}
```
Then add `pub mod schema;` to `lib.rs` and create an empty `crates/core/src/schema.rs`.

- [ ] **Step 2: Run the test and confirm it fails**

Run: `cargo test -p cmdstudio-core --test schema_file`
Expected: compile error (`theme_schema_json` not found).

- [ ] **Step 3: Implement `crates/core/src/schema.rs`**
```rust
//! JSON Schema for `.cmdtheme` files, generated from the Rust types so the
//! published file can never drift from the parser (spec §3.3).

/// Pretty-printed schema text, ending with a newline.
pub fn theme_schema_json() -> Result<String, serde_json::Error> {
    let schema = schemars::schema_for!(crate::model::Theme);
    let mut text = serde_json::to_string_pretty(&schema)?;
    text.push('\n');
    Ok(text)
}
```

- [ ] **Step 4: Generate the file, then run the tests**

Run (PowerShell):
```powershell
$env:UPDATE_SCHEMA = '1'; cargo test -p cmdstudio-core --test schema_file; Remove-Item Env:UPDATE_SCHEMA
cargo test -p cmdstudio-core --test schema_file
```
Expected: both runs pass, and `schema/cmdtheme-1.schema.json` exists. Open it and check that `additionalProperties: false` appears on every object type. If schemars puts the strictness somewhere else in your version, adjust the second test to match where it actually is, and note that in the commit.

- [ ] **Step 5: Commit**

Message: `feat(core): publish the cmdtheme JSON Schema generated from the types`
```bash
git add -- crates/core/src/schema.rs crates/core/tests/schema_file.rs schema/cmdtheme-1.schema.json crates/core/src/lib.rs
git diff --cached --name-only
git commit -F .git/CMDSTUDIO_COMMIT_MSG
```

---

### Task 7: The Terracotta preset

**Files:**
- Create: `presets/terracotta.cmdtheme`, `crates/core/src/presets.rs`
- Modify: `crates/core/src/lib.rs` (add `pub mod presets;`)

**Interfaces:**
- Consumes: `model::{parse_theme, Theme, ThemeError}`, `palette::contrast_issues`
- Produces:
  - `presets::builtin() -> Result<Vec<(&'static str, Theme)>, ThemeError>`, with `"terracotta"` first.
  - `presets::OWN: &[&str]`: keys of our own presets, which must pass the readability check.

- [ ] **Step 1: Add the preset file** `presets/terracotta.cmdtheme` (colors from spec Appendix A):
```json
{
  "schemaVersion": 1,
  "id": "7d3c5f0e-2b1a-4c8e-9f6d-1a2b3c4d5e6f",
  "name": "Terracotta",
  "author": "CMD Studio",
  "tone": "dark",
  "simple": null,
  "colors": {
    "background": "#1f1e1d",
    "foreground": "#faf9f5",
    "cursor": "#d97757",
    "selection": "#4a4845",
    "accent": "#d97757",
    "muted": "#8a8780",
    "frame": "#5f5d58",
    "ansi": [
      "#3a3936", "#e5736a", "#8fbf7f", "#e3b66b", "#7fa7d9", "#b79bdb", "#7fc1b8", "#d8d6cf",
      "#8a8780", "#f0907f", "#a8d196", "#eecb8a", "#9bbde8", "#cbb4e8", "#9dd3cb", "#faf9f5"
    ]
  },
  "terminal": {
    "font": "Cascadia Mono",
    "fontSize": 12,
    "cursorShape": "filledBox",
    "opacity": 96,
    "acrylic": true,
    "padding": [14, 10]
  },
  "prompt": {
    "layout": "box",
    "segments": ["path", "git", "time"],
    "transient": true
  },
  "banner": {
    "enabled": true,
    "title": "Welcome back, {user}!",
    "lines": ["cwd: {cwd}", "{date} · {time} · {host}"],
    "tips": true,
    "clearLogo": true
  }
}
```

- [ ] **Step 2: Write the failing tests** in a new `crates/core/src/presets.rs`, and add `pub mod presets;` to `lib.rs`:
```rust
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
        for (key, theme) in builtin().unwrap() {
            if OWN.contains(&key) {
                assert!(contrast_issues(&theme.colors).is_empty(), "{key}: {:?}", contrast_issues(&theme.colors));
            }
        }
    }

    #[test]
    fn terracotta_matches_the_prototype() {
        let (_, t) = builtin().unwrap().remove(0);
        assert_eq!(t.colors.background.to_string(), "#1f1e1d");
        assert_eq!(t.colors.accent.to_string(), "#d97757");
        assert_eq!(t.colors.ansi[15].to_string(), "#faf9f5");
    }
}
```

- [ ] **Step 3: Run the tests and confirm they fail**

Run: `cargo test -p cmdstudio-core presets`
Expected: compile error (`builtin` not found).

- [ ] **Step 4: Implement it above the tests**
```rust
//! Built-in presets, embedded at compile time from `/presets` (spec §3.2).
//! Presets from other projects are added after their licenses are confirmed
//! (spec gate G14).
use crate::model::{parse_theme, Theme, ThemeError};

const SOURCES: &[(&str, &str)] = &[("terracotta", include_str!("../../../presets/terracotta.cmdtheme"))];

/// Keys of presets whose colors are our own; they must pass the
/// readability check (spec §3.2).
pub const OWN: &[&str] = &["terracotta"];

pub fn builtin() -> Result<Vec<(&'static str, Theme)>, ThemeError> {
    SOURCES
        .iter()
        .map(|(key, src)| parse_theme(src.as_bytes()).map(|t| (*key, t)))
        .collect()
}
```

- [ ] **Step 5: Run the tests and confirm they pass**

Run: `cargo test -p cmdstudio-core presets`
Expected: 3 passed.

- [ ] **Step 6: Commit**

Message: `feat(core): add the Terracotta preset from the prototype`
```bash
git add -- presets/terracotta.cmdtheme crates/core/src/presets.rs crates/core/src/lib.rs
git diff --cached --name-only
git commit -F .git/CMDSTUDIO_COMMIT_MSG
```

---

### Task 8: Oh My Posh config generator

**Files:**
- Create: `crates/core/src/generate/mod.rs`, `crates/core/src/generate/omp.rs`, `crates/core/examples/dump_omp.rs`
- Modify: `crates/core/src/lib.rs` (add `pub mod generate;`)

**Interfaces:**
- Consumes: `model::{Theme, Layout, Segment}`, `presets::builtin` (tests)
- Produces:
  - `generate::omp::ALLOWED_TYPES: &[&str]`
  - `generate::omp::config(&Theme) -> serde_json::Value`
  - `generate::omp::config_text(&Theme) -> Result<String, serde_json::Error>`

The box layout reproduces `%USERPROFILE%\cmd-claude-theme\claude.omp.json`, which was proven in a real window (spec Appendix A). The segment shapes `powerline_symbol`, `session` (`.UserName`, `.HostName`), `status` (`.Code`) and `executiontime` (`threshold`, `style`) were confirmed against the themes that ship with Oh My Posh 31.5.0. Real rendering of `line` and `powerline` is checked by the Phase 1b parity tests.

- [ ] **Step 1: Write the failing tests**

`crates/core/src/generate/mod.rs`:
```rust
//! Generators: theme → file contents. Pure functions (spec §5.2 `generate`).
pub mod omp;
```
Add `pub mod generate;` to `lib.rs`. Create `crates/core/src/generate/omp.rs` with:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Layout, Segment, Theme};
    use crate::presets;
    use serde_json::Value;

    fn terracotta() -> Theme {
        presets::builtin().unwrap().remove(0).1
    }

    fn segment_types(v: &Value, out: &mut Vec<String>) {
        if let Some(segs) = v.get("segments").and_then(Value::as_array) {
            out.extend(segs.iter().map(|s| s["type"].as_str().unwrap_or("<missing>").to_string()));
        }
        match v {
            Value::Array(a) => a.iter().for_each(|x| segment_types(x, out)),
            Value::Object(o) => o.values().for_each(|x| segment_types(x, out)),
            _ => {}
        }
    }

    const ALL: [Segment; 7] = [Segment::Path, Segment::Git, Segment::Time, Segment::User, Segment::Host, Segment::Status, Segment::Duration];

    #[test]
    fn box_layout_matches_the_prototype_shape() {
        let c = config(&terracotta());
        assert_eq!(c["version"], 4);
        assert_eq!(c["palette"]["accent"], "#d97757");
        let blocks = c["blocks"].as_array().unwrap();
        assert_eq!(blocks.len(), 4);
        assert_eq!(blocks[0]["segments"][0]["template"], "\u{256d}\u{2500} ");
        assert_eq!(blocks[1]["alignment"], "right");
        assert_eq!(blocks[1]["filler"], "<p:frame>\u{2500}</>");
        assert_eq!(blocks[2]["newline"], true);
        assert_eq!(blocks[3]["type"], "rprompt");
        assert_eq!(c["transient_prompt"]["template"], "> ");
        assert_eq!(blocks[0]["segments"][1]["options"]["style"], "full");
    }

    #[test]
    fn never_emits_a_segment_type_outside_the_whitelist() {
        // spec §7 doctrine guard (ข): every layout × every subset of segments
        for layout in [Layout::Box, Layout::Line, Layout::Powerline] {
            for mask in 0u8..128 {
                let mut t = terracotta();
                t.prompt.layout = layout;
                t.prompt.segments = ALL.iter().enumerate().filter(|(i, _)| mask & (1 << i) != 0).map(|(_, s)| *s).collect();
                let mut types = Vec::new();
                segment_types(&config(&t), &mut types);
                for ty in types {
                    assert!(ALLOWED_TYPES.contains(&ty.as_str()), "{layout:?} mask {mask}: {ty}");
                    assert_ne!(ty, "command");
                }
            }
        }
    }

    #[test]
    fn empty_segment_list_still_draws_the_frame() {
        // Review Focus 4
        let mut t = terracotta();
        t.prompt.segments.clear();
        let c = config(&t);
        assert_eq!(c["blocks"][0]["segments"].as_array().unwrap().len(), 1);
        assert_eq!(c["blocks"][1]["segments"].as_array().unwrap().len(), 1);
        assert!(config_text(&t).unwrap().ends_with("}\n"));
    }

    #[test]
    fn user_and_host_share_one_session_segment() {
        let mut t = terracotta();
        t.prompt.segments = vec![Segment::User, Segment::Path, Segment::Host];
        let mut types = Vec::new();
        segment_types(&config(&t), &mut types);
        assert_eq!(types.iter().filter(|t| *t == "session").count(), 1);
        assert_eq!(config(&t)["blocks"][0]["segments"][1]["template"], "{{ .UserName }}@{{ .HostName }} ");
    }

    #[test]
    fn time_sits_right_in_box_and_left_in_line() {
        let mut t = terracotta();
        let left_types = |c: &Value| c["blocks"][0]["segments"].as_array().unwrap().iter().map(|s| s["type"].as_str().unwrap().to_string()).collect::<Vec<_>>();
        assert!(!left_types(&config(&t)).contains(&"time".to_string()));
        t.prompt.layout = Layout::Line;
        assert!(left_types(&config(&t)).contains(&"time".to_string()));
    }

    #[test]
    fn powerline_segments_use_the_powerline_symbol() {
        let mut t = terracotta();
        t.prompt.layout = Layout::Powerline;
        let c = config(&t);
        for s in c["blocks"][0]["segments"].as_array().unwrap() {
            if s["type"] != "text" {
                assert_eq!(s["style"], "powerline");
                assert_eq!(s["powerline_symbol"], "\u{e0b0}");
            }
        }
    }
}
```

- [ ] **Step 2: Run the tests and confirm they fail**

Run: `cargo test -p cmdstudio-core generate::omp`
Expected: compile errors (`config`, `ALLOWED_TYPES` not found).

- [ ] **Step 3: Implement it above the tests**
```rust
//! Theme → Oh My Posh config, format version 4 (spec §3.5). Built only from
//! whitelisted segment types; `text` carries only our own frame glyphs.
use crate::model::{Layout, Segment, Theme};
use serde_json::{json, Value};

/// Segment types this generator may emit (doctrine guard, spec §3.7 #2).
pub const ALLOWED_TYPES: &[&str] = &["path", "git", "time", "session", "status", "executiontime", "text"];

const TIME_FORMAT: &str = "15:04";
const DIRTY: &str = "{{ if or .Working.Changed .Staging.Changed }}";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Style {
    Box,
    Line,
    Powerline,
}

pub fn config(theme: &Theme) -> Value {
    let blocks = match theme.prompt.layout {
        Layout::Box => box_blocks(theme),
        Layout::Line => line_blocks(theme),
        Layout::Powerline => powerline_blocks(theme),
    };
    json!({
        "$schema": "https://raw.githubusercontent.com/JanDeDobbeleer/oh-my-posh/main/themes/schema.json",
        "version": 4,
        "final_space": true,
        "console_title_template": "{{ .Folder }}",
        "palette": palette(theme),
        "blocks": blocks,
        "transient_prompt": { "foreground": "p:accent", "background": "transparent", "template": "> " }
    })
}

pub fn config_text(theme: &Theme) -> Result<String, serde_json::Error> {
    let mut text = serde_json::to_string_pretty(&config(theme))?;
    text.push('\n');
    Ok(text)
}

fn palette(theme: &Theme) -> Value {
    let c = &theme.colors;
    json!({
        "bg": c.background.to_string(),
        "fg": c.foreground.to_string(),
        "accent": c.accent.to_string(),
        "muted": c.muted.to_string(),
        "frame": c.frame.to_string(),
        "red": c.ansi[1].to_string(),
        "ok": c.ansi[2].to_string(),
        "dirty": c.ansi[3].to_string(),
        "blue": c.ansi[4].to_string()
    })
}

fn plain(kind: &str, fg: &str, template: &str) -> Value {
    json!({ "type": kind, "style": "plain", "foreground": fg, "template": template })
}

fn powerline(kind: &str, bg: &str, fg: &str, template: &str) -> Value {
    json!({ "type": kind, "style": "powerline", "powerline_symbol": "\u{e0b0}", "background": bg, "foreground": fg, "template": template })
}

fn with_options(mut v: Value, options: Value) -> Value {
    v["options"] = options;
    v
}

fn text(fg: &str, template: &str) -> Value {
    plain("text", fg, template)
}

fn git_options(nerd_icons: bool) -> Value {
    if nerd_icons {
        json!({ "fetch_status": true })
    } else {
        json!({
            "branch_icon": "", "commit_icon": "@", "tag_icon": "#", "rebase_icon": "rebase ",
            "cherry_pick_icon": "pick ", "revert_icon": "revert ", "merge_icon": "merge ",
            "no_commits_icon": "", "fetch_status": true
        })
    }
}

fn path(style: Style) -> Value {
    let options = json!({ "style": "full", "home_icon": "~" });
    match style {
        Style::Box => with_options(plain("path", "p:muted", "{{ .Path }} "), options),
        Style::Line => with_options(plain("path", "p:blue", "{{ .Path }} "), options),
        Style::Powerline => with_options(powerline("path", "p:frame", "p:fg", " {{ .Path }} "), options),
    }
}

fn git(style: Style) -> Value {
    match style {
        Style::Box => {
            let template = ["<p:muted>\u{b7} </>{{ .HEAD }}", DIRTY, " *{{ else }} \u{2713}{{ end }} "].concat();
            let mut v = with_options(plain("git", "p:ok", &template), git_options(false));
            v["foreground_templates"] = json!([[DIRTY, "p:dirty{{ end }}"].concat()]);
            v
        }
        Style::Line => with_options(plain("git", "p:dirty", &["{{ .HEAD }}", DIRTY, " *{{ end }} "].concat()), git_options(false)),
        Style::Powerline => with_options(powerline("git", "p:dirty", "p:bg", &[" {{ .HEAD }}", DIRTY, " *{{ end }} "].concat()), git_options(true)),
    }
}

fn time(style: Style) -> Value {
    let options = json!({ "time_format": TIME_FORMAT });
    match style {
        Style::Box => with_options(plain("time", "p:muted", " {{ .CurrentDate | date .Format }}"), options),
        Style::Line => with_options(plain("time", "p:muted", "{{ .CurrentDate | date .Format }} "), options),
        Style::Powerline => with_options(powerline("time", "p:frame", "p:fg", " {{ .CurrentDate | date .Format }} "), options),
    }
}

fn session(style: Style, user: bool, host: bool) -> Value {
    let body = match (user, host) {
        (true, true) => "{{ .UserName }}@{{ .HostName }}",
        (true, false) => "{{ .UserName }}",
        _ => "{{ .HostName }}",
    };
    match style {
        Style::Powerline => powerline("session", "p:accent", "p:bg", &format!(" {body} ")),
        _ => plain("session", "p:fg", &format!("{body} ")),
    }
}

fn status(style: Style) -> Value {
    match style {
        Style::Powerline => powerline("status", "p:red", "p:bg", " \u{2717} {{ .Code }} "),
        _ => plain("status", "p:red", "\u{2717} {{ .Code }} "),
    }
}

fn duration(style: Style) -> Value {
    let options = json!({ "threshold": 2000, "style": "austin" });
    match style {
        Style::Powerline => with_options(powerline("executiontime", "p:dirty", "p:bg", " {{ .FormattedMs }} "), options),
        _ => with_options(plain("executiontime", "p:dirty", "{{ .FormattedMs }} "), options),
    }
}

/// One emitted segment per theme segment, in theme order. `user` and `host`
/// share one `session` segment where the first of them appears. In the box
/// layout, time goes on the right instead.
fn segments(theme: &Theme, style: Style) -> Vec<Value> {
    let list = &theme.prompt.segments;
    let user = list.contains(&Segment::User);
    let host = list.contains(&Segment::Host);
    let mut session_done = false;
    let mut out = Vec::new();
    for seg in list {
        let v = match seg {
            Segment::Time if style == Style::Box => continue,
            Segment::User | Segment::Host => {
                if session_done {
                    continue;
                }
                session_done = true;
                session(style, user, host)
            }
            Segment::Path => path(style),
            Segment::Git => git(style),
            Segment::Time => time(style),
            Segment::Status => status(style),
            Segment::Duration => duration(style),
        };
        out.push(v);
    }
    out
}

fn box_blocks(theme: &Theme) -> Vec<Value> {
    let mut left = vec![text("p:frame", "\u{256d}\u{2500} ")];
    left.extend(segments(theme, Style::Box));
    let mut right = Vec::new();
    if theme.prompt.segments.contains(&Segment::Time) {
        right.push(time(Style::Box));
    }
    right.push(text("p:frame", " \u{2500}\u{256e}"));
    vec![
        json!({ "type": "prompt", "alignment": "left", "segments": left }),
        json!({ "type": "prompt", "alignment": "right", "filler": "<p:frame>\u{2500}</>", "segments": right }),
        json!({ "type": "prompt", "alignment": "left", "newline": true, "segments": [text("p:frame", "\u{2502} "), text("p:accent", ">")] }),
        json!({ "type": "rprompt", "segments": [text("p:frame", "\u{2502}")] }),
    ]
}

fn line_blocks(theme: &Theme) -> Vec<Value> {
    let mut list = segments(theme, Style::Line);
    list.push(text("p:accent", "\u{276f}"));
    vec![json!({ "type": "prompt", "alignment": "left", "segments": list })]
}

fn powerline_blocks(theme: &Theme) -> Vec<Value> {
    let mut list = segments(theme, Style::Powerline);
    list.push(text("p:accent", " \u{276f}"));
    vec![json!({ "type": "prompt", "alignment": "left", "segments": list })]
}
```

- [ ] **Step 4: Run the tests and confirm they pass**

Run: `cargo test -p cmdstudio-core generate::omp`
Expected: 6 passed.

- [ ] **Step 5: Check by eye against the real Oh My Posh** (not a unit test; this catches shape mistakes early). Create `crates/core/examples/dump_omp.rs`:
```rust
//! Writes the Terracotta Oh My Posh config to target/terracotta.omp.json so it
//! can be rendered by the real oh-my-posh binary.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (_, theme) = cmdstudio_core::presets::builtin()?.remove(0);
    std::fs::write("target/terracotta.omp.json", cmdstudio_core::generate::omp::config_text(&theme)?)?;
    Ok(())
}
```
Run:
```powershell
cargo run -p cmdstudio-core --example dump_omp
oh-my-posh print primary --config target/terracotta.omp.json --shell cmd --terminal-width 70 --plain --pwd $env:USERPROFILE
oh-my-posh print right --config target/terracotta.omp.json --shell cmd --plain
```
Expected: the same lines the prototype printed. Primary is `╭─ ~ ──…── HH:MM ─╮` followed by `│ > `, and right is `│`. Paste the output into the commit message. If it differs, compare with `%USERPROFILE%\cmd-claude-theme\claude.omp.json` and fix the generator, not the expectation.

- [ ] **Step 6: Commit**

Message:
```text
feat(core): generate the Oh My Posh config from a theme

Box, line and powerline layouts, built only from whitelisted segment
types. A guard test walks every layout and segment subset and fails on
any other type, including command.

oh-my-posh print primary (31.5.0) on the Terracotta config:
<paste the two lines from Step 5>
```
```bash
git add -- crates/core/src/generate/mod.rs crates/core/src/generate/omp.rs crates/core/src/lib.rs crates/core/examples/dump_omp.rs
git diff --cached --name-only
git commit -F .git/CMDSTUDIO_COMMIT_MSG
```

---

### Task 9: Clink scripts and settings (with real Lua 5.2 guard tests)

**Files:**
- Create: `crates/core/src/generate/lua.rs`, `crates/core/src/generate/clink.rs`, `crates/core/tests/banner_lua.rs`
- Modify: `crates/core/src/generate/mod.rs` (add `pub mod lua; pub mod clink;`), `crates/core/Cargo.toml` (dev-dependency mlua)

**Interfaces:**
- Consumes: `model::Theme`, `color::Rgb`, `text::lua_str`
- Produces:
  - `generate::lua::Lang { Th, En }`
  - `generate::lua::init_lua(omp_exe: &Path, omp_config: &Path) -> String`
  - `generate::lua::banner_lua(&Theme, Lang) -> String`
  - `generate::clink::settings(transient: bool, scripts_dir: &Path) -> String`

- [ ] **Step 1: Add the Lua test dependency** to `crates/core/Cargo.toml` under `[dev-dependencies]`:
```toml
mlua = { version = "0.10", features = ["lua52", "vendored"] }
```
Clink embeds Lua 5.2 (its docs say so), so the tests use the same language version. Run `cargo doc -p mlua --no-deps` and confirm the signatures of `Lua::new`, `Chunk::exec`, `Chunk::eval`, `Function::call` and `Table::sequence_values` (Implementation Gate).

- [ ] **Step 2: Write the failing tests**

Add `pub mod lua;` and `pub mod clink;` to `crates/core/src/generate/mod.rs`. Create `crates/core/src/generate/clink.rs` containing:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn writes_the_four_settings() {
        let s = settings(true, Path::new(r"C:\Data\generated"));
        assert!(s.contains("\nclink.logo = none\n"));
        assert!(s.contains("\nprompt.transient = always\n"));
        assert!(s.contains("\nclink.path = C:\\Data\\generated\n"));
        assert!(s.contains("\nclink.autoupdate = off\n"));
        assert!(settings(false, Path::new("x")).contains("\nprompt.transient = off\n"));
    }
}
```
Create `crates/core/src/generate/lua.rs` containing:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::presets;
    use std::path::Path;

    #[test]
    fn templates_become_lua_expressions() {
        assert_eq!(template_expr("Welcome back, {user}!"), "\"Welcome back, \" .. V.user() .. \"!\"");
        assert_eq!(template_expr("{date} · {time}"), "V.date() .. \" · \" .. V.time()");
        assert_eq!(template_expr("{foo} stays"), "\"{foo} stays\"");
        assert_eq!(template_expr(""), "\"\"");
    }

    #[test]
    fn init_lua_quotes_paths_and_wraps_the_popen_command() {
        let s = init_lua(Path::new(r"C:\Users\ทดสอบ ผู้ใช้\omp.exe"), Path::new(r"C:\x\theme.omp.json"));
        assert!(s.contains(r#"local omp = "C:\\Users\\ทดสอบ ผู้ใช้\\omp.exe""#));
        assert!(s.contains(r#"io.popen('"' .. '"' .. omp"#));
    }

    #[test]
    fn disabled_banner_registers_nothing() {
        // Review Focus 4
        let mut t = presets::builtin().unwrap().remove(0).1;
        t.banner.enabled = false;
        assert!(!banner_lua(&t, Lang::En).contains("oninject"));
    }
}
```
Create `crates/core/tests/banner_lua.rs`:
```rust
//! Doctrine guard (spec §7 (ก)): the generated banner.lua does nothing when
//! loaded, prints once per session, and keeps hostile text literal. Runs the
//! real script in Lua 5.2 with small stand-ins for Clink's API.
use cmdstudio_core::generate::lua::{banner_lua, Lang};
use cmdstudio_core::model::Theme;
use cmdstudio_core::presets;
use mlua::{Function, Lua, Table, Value};

const PRELUDE: &str = r#"
__out = {}
__env = {}
__lines = {}
NONL = {}
clink = {
  print = function(s, flag)
    if flag == NONL then s = "<NONL>" .. s end
    __out[#__out + 1] = s
  end,
  oninject = function(f) __oninject = f end,
}
console = {
  cellcount = function(s)
    local plain = s:gsub("\27%[[%d;]*m", "")
    local _, n = plain:gsub("[^\128-\191]", "")
    return n
  end,
  getlinetext = function(n) return __lines[n] end,
}
os.getenv = function(k)
  if __env[k] ~= nil then return __env[k] end
  if k == "USERNAME" then return "ทดสอบ" end
  if k == "COMPUTERNAME" then return "PC-01" end
  return nil
end
os.setenv = function(k, v) __env[k] = v end
os.getcwd = function() return "C:\\Users\\ทดสอบ" end
log = { info = function() end }
"#;

fn terracotta() -> Theme {
    presets::builtin().unwrap().remove(0).1
}

fn load(script: &str, screen: &[&str]) -> mlua::Result<Lua> {
    let lua = Lua::new();
    lua.load(PRELUDE).exec()?;
    let lines = lua.create_table()?;
    for (i, l) in screen.iter().enumerate() {
        lines.set(i + 1, *l)?;
    }
    lua.globals().set("__lines", lines)?;
    lua.load(script).exec()?;
    Ok(lua)
}

fn printed(lua: &Lua) -> mlua::Result<Vec<String>> {
    let out: Table = lua.globals().get("__out")?;
    out.sequence_values::<String>().collect()
}

fn inject(lua: &Lua) -> mlua::Result<()> {
    let f: Function = lua.globals().get("__oninject")?;
    f.call::<()>(())
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::new();
    let mut in_seq = false;
    for ch in s.chars() {
        match (in_seq, ch) {
            (false, '\u{1b}') => in_seq = true,
            (true, 'm') => in_seq = false,
            (true, _) => {}
            (false, c) => out.push(c),
        }
    }
    out
}

#[test]
fn loading_the_banner_prints_nothing() -> mlua::Result<()> {
    let lua = load(&banner_lua(&terracotta(), Lang::En), &[])?;
    assert!(printed(&lua)?.is_empty());
    Ok(())
}

#[test]
fn banner_prints_once_per_session() -> mlua::Result<()> {
    let lua = load(&banner_lua(&terracotta(), Lang::En), &[])?;
    inject(&lua)?;
    inject(&lua)?;
    assert_eq!(printed(&lua)?.len(), 1);
    Ok(())
}

#[test]
fn box_lines_line_up_and_the_last_line_is_one_space() -> mlua::Result<()> {
    let lua = load(&banner_lua(&terracotta(), Lang::Th), &[])?;
    inject(&lua)?;
    let out = printed(&lua)?;
    let lines: Vec<&str> = out[0].split('\n').collect();
    let bottom = lines.iter().position(|l| strip_ansi(l).starts_with('\u{2570}')).unwrap();
    let widths: Vec<usize> = lines[..=bottom].iter().map(|l| strip_ansi(l).chars().count()).collect();
    assert!(widths.windows(2).all(|w| w[0] == w[1]), "{widths:?}");
    assert_eq!(*lines.last().unwrap(), " ");
    assert!(out[0].contains("ทดสอบ"));
    Ok(())
}

#[test]
fn clears_the_cmd_header_only_when_it_is_on_top() -> mlua::Result<()> {
    let header = ["Microsoft Windows [Version 10.0.26300.9550]", "(c) Microsoft Corporation. All rights reserved."];
    let lua = load(&banner_lua(&terracotta(), Lang::En), &header)?;
    inject(&lua)?;
    assert!(printed(&lua)?[0].starts_with("<NONL>"));

    let lua = load(&banner_lua(&terracotta(), Lang::En), &["C:\\>dir", "Microsoft Windows [Version 10.0]"])?;
    inject(&lua)?;
    let out = printed(&lua)?;
    assert_eq!(out.len(), 1);
    assert!(!out[0].starts_with("<NONL>"));
    Ok(())
}

#[test]
fn hostile_titles_stay_literal() -> mlua::Result<()> {
    let mut t = terracotta();
    t.banner.title = r#"a"]]\x {foo} {user}"#.to_string();
    let lua = load(&banner_lua(&t, Lang::En), &[])?;
    inject(&lua)?;
    assert!(printed(&lua)?[0].contains(r#"a"]]\x {foo} ทดสอบ"#));
    Ok(())
}

#[test]
fn thai_dates_use_the_buddhist_year() -> mlua::Result<()> {
    let mut t = terracotta();
    t.banner.lines = vec!["{date}".to_string()];
    let lua = load(&banner_lua(&t, Lang::Th), &[])?;
    let year: i64 = lua.load("return os.date('*t').year + 543").eval()?;
    inject(&lua)?;
    assert!(printed(&lua)?[0].contains(&year.to_string()));
    Ok(())
}

#[test]
fn no_lines_and_no_tips_still_print_a_valid_box() -> mlua::Result<()> {
    // Review Focus 4
    let mut t = terracotta();
    t.banner.lines.clear();
    t.banner.tips = false;
    let lua = load(&banner_lua(&t, Lang::En), &[])?;
    inject(&lua)?;
    let out = printed(&lua)?;
    assert_eq!(strip_ansi(&out[0]).lines().count(), 4);
    Ok(())
}

#[test]
fn disabled_banner_registers_no_callback() -> mlua::Result<()> {
    let mut t = terracotta();
    t.banner.enabled = false;
    let lua = load(&banner_lua(&t, Lang::En), &[])?;
    let cb: Value = lua.globals().get("__oninject")?;
    assert!(cb.is_nil());
    Ok(())
}
```
The test `no_lines_and_no_tips_still_print_a_valid_box` expects 4 lines: the top border, the title, the bottom border, and the trailing space line.

- [ ] **Step 3: Run the tests and confirm they fail**

Run: `cargo test -p cmdstudio-core lua` and `cargo test -p cmdstudio-core --test banner_lua`
Expected: compile errors (`banner_lua`, `init_lua`, `template_expr`, `settings` not found).

- [ ] **Step 4: Implement `clink.rs`** above its tests:
```rust
//! The Clink settings file for CMD Studio's private profile (spec §5.4).
use std::path::Path;

pub fn settings(transient: bool, scripts_dir: &Path) -> String {
    format!(
        "# Generated by CMD Studio. Do not edit: changes are overwritten.\n\
         clink.logo = none\n\
         prompt.transient = {}\n\
         clink.path = {}\n\
         clink.autoupdate = off\n",
        if transient { "always" } else { "off" },
        scripts_dir.display()
    )
}
```
The value `off` for `clink.autoupdate` is gate G16. If Phase 0 Task 3 records a different value, use that.

- [ ] **Step 5: Implement `lua.rs`** above its tests:
```rust
//! Theme → Clink Lua scripts (spec §3.6): `init.lua` loads Oh My Posh and
//! `banner.lua` prints the welcome box. Every string goes through `lua_str`.
use crate::color::Rgb;
use crate::model::Theme;
use crate::text::lua_str;
use std::path::Path;

/// Language of generated user-visible text (dates, tips).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    Th,
    En,
}

const HEADER: &str = "-- Generated by CMD Studio. Do not edit: changes are overwritten.\n";

const INIT_BODY: &str = r#"-- cmd /c drops the outermost quotes, so the command is wrapped once more (spec gate G17).
local handle = io.popen('"' .. '"' .. omp .. '" init cmd --config "' .. config .. '"' .. '"')
local script = handle and handle:read("*a") or ""
if handle then handle:close() end
if script == "" then
  log.info("cmdstudio: oh-my-posh init returned nothing")
  return
end
local chunk, err = load(script)
if not chunk then
  log.info("cmdstudio: cannot load oh-my-posh init: " .. tostring(err))
  return
end
chunk()
log.info("cmdstudio: prompt loaded")
"#;

const VARS_TH: &str = r#"local DAYS = { "อา.", "จ.", "อ.", "พ.", "พฤ.", "ศ.", "ส." }
local MONTHS = { "ม.ค.", "ก.พ.", "มี.ค.", "เม.ย.", "พ.ค.", "มิ.ย.", "ก.ค.", "ส.ค.", "ก.ย.", "ต.ค.", "พ.ย.", "ธ.ค." }
local function today()
  local t = os.date("*t")
  return DAYS[t.wday] .. " " .. t.day .. " " .. MONTHS[t.month] .. " " .. (t.year + 543)
end
local V = {
  user = function() return os.getenv("USERNAME") or "" end,
  host = function() return os.getenv("COMPUTERNAME") or "" end,
  cwd = function() return os.getcwd() end,
  date = today,
  time = function() return os.date("%H:%M") end,
}
"#;

const VARS_EN: &str = r#"local DAYS = { "Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat" }
local MONTHS = { "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec" }
local function today()
  local t = os.date("*t")
  return string.format("%s %02d %s %d", DAYS[t.wday], t.day, MONTHS[t.month], t.year)
end
local V = {
  user = function() return os.getenv("USERNAME") or "" end,
  host = function() return os.getenv("COMPUTERNAME") or "" end,
  cwd = function() return os.getcwd() end,
  date = today,
  time = function() return os.date("%H:%M") end,
}
"#;

const TIPS_TH: &str = " ทิป: help ดูคำสั่ง \u{b7} cls ล้างจอ \u{b7} \u{2191} ดูประวัติ";
const TIPS_EN: &str = " Tips: help for commands \u{b7} cls to clear \u{b7} \u{2191} for history";

const PRINT_FN: &str = r#"local function print_banner()
  local lines = build()
  local width = 0
  for _, l in ipairs(lines) do
    width = math.max(width, console.cellcount(l))
  end
  width = width + 2
  local out = { ACCENT .. "╭" .. string.rep("─", width + 2) .. "╮" .. RESET }
  for _, l in ipairs(lines) do
    local pad = string.rep(" ", width - console.cellcount(l))
    out[#out + 1] = ACCENT .. "│" .. RESET .. " " .. l .. pad .. " " .. ACCENT .. "│" .. RESET
  end
  out[#out + 1] = ACCENT .. "╰" .. string.rep("─", width + 2) .. "╯" .. RESET
  if TIPS then out[#out + 1] = MUTED .. TIPS .. RESET end
  -- One space, never an empty line: an empty last line broke the first
  -- collapsed prompt in the prototype (spec 2.2 #3).
  out[#out + 1] = " "
  clink.print(table.concat(out, "\n"))
end
"#;

const CLEAR_LOGO_FN: &str = r#"local function clear_cmd_logo()
  local l1 = console.getlinetext(1) or ""
  local l2 = console.getlinetext(2) or ""
  if l1:find("^Microsoft Windows %[") and l2:find("^%(c%) Microsoft Corporation") then
    clink.print("\027[2J\027[3J\027[H", NONL)
  end
end
"#;

fn sgr(c: Rgb) -> String {
    format!("\u{1b}[38;2;{};{};{}m", c.r, c.g, c.b)
}

/// A banner template as a Lua expression. Known `{variables}` become calls;
/// everything else stays literal (spec §3.3).
pub(crate) fn template_expr(template: &str) -> String {
    const VARS: [(&str, &str); 5] = [
        ("{user}", "V.user()"),
        ("{host}", "V.host()"),
        ("{cwd}", "V.cwd()"),
        ("{date}", "V.date()"),
        ("{time}", "V.time()"),
    ];
    let mut parts: Vec<String> = Vec::new();
    let mut rest = template;
    'scan: while !rest.is_empty() {
        for (i, _) in rest.match_indices('{') {
            for (name, expr) in VARS {
                if rest[i..].starts_with(name) {
                    if i > 0 {
                        parts.push(lua_str(&rest[..i]));
                    }
                    parts.push(expr.to_string());
                    rest = &rest[i + name.len()..];
                    continue 'scan;
                }
            }
        }
        parts.push(lua_str(rest));
        break;
    }
    if parts.is_empty() {
        "\"\"".to_string()
    } else {
        parts.join(" .. ")
    }
}

pub fn init_lua(omp_exe: &Path, omp_config: &Path) -> String {
    let mut s = String::from(HEADER);
    s.push_str(&format!("local omp = {}\n", lua_str(&omp_exe.display().to_string())));
    s.push_str(&format!("local config = {}\n", lua_str(&omp_config.display().to_string())));
    s.push_str(INIT_BODY);
    s
}

pub fn banner_lua(theme: &Theme, lang: Lang) -> String {
    let b = &theme.banner;
    let mut s = String::from(HEADER);
    if !b.enabled {
        s.push_str("-- The banner is turned off in this theme.\n");
        return s;
    }
    s.push_str(&format!("local ACCENT = {}\n", lua_str(&sgr(theme.colors.accent))));
    s.push_str(&format!("local MUTED = {}\n", lua_str(&sgr(theme.colors.muted))));
    s.push_str("local RESET = \"\\027[0m\"\n");
    s.push_str(match lang {
        Lang::Th => VARS_TH,
        Lang::En => VARS_EN,
    });
    s.push_str("local function build()\n  return {\n");
    s.push_str(&format!("    ACCENT .. \"\u{273b}\" .. RESET .. \" \" .. ({}),\n", template_expr(&b.title)));
    if !b.lines.is_empty() {
        s.push_str("    \"\",\n");
        for line in &b.lines {
            s.push_str(&format!("    MUTED .. \"  \" .. ({}) .. RESET,\n", template_expr(line)));
        }
    }
    s.push_str("  }\nend\n");
    let tips = b.tips.then_some(match lang {
        Lang::Th => TIPS_TH,
        Lang::En => TIPS_EN,
    });
    s.push_str(&format!("local TIPS = {}\n", tips.map(lua_str).unwrap_or_else(|| "nil".to_string())));
    s.push_str(PRINT_FN);
    if b.clear_logo {
        s.push_str(CLEAR_LOGO_FN);
    }
    s.push_str("clink.oninject(function()\n  if os.getenv(\"CMDSTUDIO_BANNER\") then return end\n  os.setenv(\"CMDSTUDIO_BANNER\", \"1\")\n");
    if b.clear_logo {
        s.push_str("  clear_cmd_logo()\n");
    }
    s.push_str("  print_banner()\nend)\n");
    s
}
```

- [ ] **Step 6: Run the tests and confirm they pass**

Run: `cargo test -p cmdstudio-core lua clink` and `cargo test -p cmdstudio-core --test banner_lua`
Expected: 4 unit tests and 8 Lua tests pass. If an mlua call fails to compile, fix it to match the docs from Step 1. Never weaken an assertion.

- [ ] **Step 7: Commit**

Message:
```text
feat(core): generate Clink init, banner and settings files

The banner script registers only an oninject callback. Guard tests run
the real generated Lua 5.2 and check that loading prints nothing, that it
prints once per session, that the box lines line up, that the last line
is a space, and that hostile titles stay literal.
```
```bash
git add -- crates/core/Cargo.toml Cargo.lock crates/core/src/generate/mod.rs crates/core/src/generate/lua.rs crates/core/src/generate/clink.rs crates/core/tests/banner_lua.rs
git diff --cached --name-only
git commit -F .git/CMDSTUDIO_COMMIT_MSG
```

---

### Task 10: Windows Terminal values

**Files:**
- Create: `crates/core/src/generate/wt.rs`
- Modify: `crates/core/src/generate/mod.rs` (add `pub mod wt;`)

**Interfaces:**
- Consumes: `model::{Theme, Target, CursorShape}`
- Produces:
  - `generate::wt::SCHEME_NAME = "CMD Studio"`
  - `generate::wt::CMD_PROFILE_GUID = "{0caa0dad-35be-5f56-a8ff-afceeeaa6101}"`
  - `generate::wt::CMD_EXE = r"%SystemRoot%\System32\cmd.exe"`
  - `generate::wt::scheme(&Theme) -> serde_json::Value`
  - `generate::wt::profile_values(&Theme, Target, clink_launch: &Path, clink_profile: &Path) -> serde_json::Map<String, Value>`. `clink_launch` is `<clink-launch>` (spec §5.4), which Task 11 picks.

- [ ] **Step 1: Write the failing tests** in a new `crates/core/src/generate/wt.rs`, and add `pub mod wt;` to `generate/mod.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Target;
    use crate::presets;
    use std::path::Path;

    fn terracotta() -> crate::model::Theme {
        presets::builtin().unwrap().remove(0).1
    }

    #[test]
    fn scheme_uses_wt_key_names() {
        let s = scheme(&terracotta());
        let o = s.as_object().unwrap();
        assert_eq!(o.len(), 21);
        assert_eq!(s["name"], "CMD Studio");
        assert_eq!(s["cursorColor"], "#d97757");
        assert_eq!(s["purple"], "#b79bdb");
        assert_eq!(s["brightWhite"], "#faf9f5");
    }

    #[test]
    fn all_target_keeps_a_plain_commandline() {
        let p = profile_values(&terracotta(), Target::All, Path::new(r"C:\c\clink_x64.exe"), Path::new(r"C:\c\p"));
        assert_eq!(p["commandline"], r"%SystemRoot%\System32\cmd.exe");
        assert_eq!(p["colorScheme"], "CMD Studio");
        assert_eq!(p["padding"], "14, 10");
        assert_eq!(p["cursorShape"], "filledBox");
        assert_eq!(p["font"]["face"], "Cascadia Mono");
        assert_eq!(p["font"]["size"], 12);
        assert_eq!(p["icon"], "\u{273b}");
    }

    #[test]
    fn wt_target_wraps_the_inject_command_for_cmd_k() {
        // <clink-launch> is always ASCII (G19); the profile path may hold Thai letters.
        let launch = Path::new(r"C:\Users\3607~1\x\clink_x64.exe");
        let prof = Path::new(r"C:\Users\ผู้ทดสอบ ไทย\x\clink-profile");
        let p = profile_values(&terracotta(), Target::Wt, launch, prof);
        assert_eq!(
            p["commandline"],
            r#"%SystemRoot%\System32\cmd.exe /k ""C:\Users\3607~1\x\clink_x64.exe" inject --quiet --profile "C:\Users\ผู้ทดสอบ ไทย\x\clink-profile"""#
        );
    }
}
```

- [ ] **Step 2: Run the tests and confirm they fail**

Run: `cargo test -p cmdstudio-core generate::wt`
Expected: compile errors.

- [ ] **Step 3: Implement it above the tests**
```rust
//! Theme → the Windows Terminal values CMD Studio owns (spec §5.4): one
//! color scheme and the keys of the cmd profile. Patching settings.json is
//! the adapter's job (Phase 1b).
use crate::model::{CursorShape, Target, Theme};
use serde_json::{json, Map, Value};
use std::path::Path;

pub const SCHEME_NAME: &str = "CMD Studio";
pub const CMD_PROFILE_GUID: &str = "{0caa0dad-35be-5f56-a8ff-afceeeaa6101}";
pub const CMD_EXE: &str = r"%SystemRoot%\System32\cmd.exe";

const ANSI_KEYS: [&str; 16] = [
    "black", "red", "green", "yellow", "blue", "purple", "cyan", "white",
    "brightBlack", "brightRed", "brightGreen", "brightYellow", "brightBlue", "brightPurple", "brightCyan", "brightWhite",
];

pub fn scheme(theme: &Theme) -> Value {
    let c = &theme.colors;
    let mut m = Map::new();
    m.insert("name".into(), json!(SCHEME_NAME));
    m.insert("background".into(), json!(c.background.to_string()));
    m.insert("foreground".into(), json!(c.foreground.to_string()));
    m.insert("cursorColor".into(), json!(c.cursor.to_string()));
    m.insert("selectionBackground".into(), json!(c.selection.to_string()));
    for (key, color) in ANSI_KEYS.iter().zip(c.ansi.iter()) {
        m.insert((*key).into(), json!(color.to_string()));
    }
    Value::Object(m)
}

/// Keys CMD Studio sets on the cmd profile. With `Target::Wt`, cmd loads
/// Clink through `/k`. The whole command is wrapped in one more pair of
/// quotes because `cmd /k` strips the outermost pair. `clink_launch` is
/// `<clink-launch>` from the planner: all ASCII (spec §5.4, G19).
pub fn profile_values(theme: &Theme, target: Target, clink_launch: &Path, clink_profile: &Path) -> Map<String, Value> {
    let t = &theme.terminal;
    let cursor = match t.cursor_shape {
        CursorShape::FilledBox => "filledBox",
        CursorShape::Bar => "bar",
        CursorShape::Underscore => "underscore",
    };
    let commandline = match target {
        Target::All => CMD_EXE.to_string(),
        Target::Wt => format!(
            r#"{} /k ""{}" inject --quiet --profile "{}"""#,
            CMD_EXE,
            clink_launch.display(),
            clink_profile.display()
        ),
    };
    let mut m = Map::new();
    m.insert("colorScheme".into(), json!(SCHEME_NAME));
    m.insert("font".into(), json!({ "face": t.font, "size": t.font_size }));
    m.insert("cursorShape".into(), json!(cursor));
    m.insert("opacity".into(), json!(t.opacity));
    m.insert("useAcrylic".into(), json!(t.acrylic));
    m.insert("padding".into(), json!(format!("{}, {}", t.padding[0], t.padding[1])));
    m.insert("icon".into(), json!("\u{273b}"));
    m.insert("commandline".into(), json!(commandline));
    m
}
```

- [ ] **Step 4: Run the tests and confirm they pass**

Run: `cargo test -p cmdstudio-core generate::wt`
Expected: 3 passed.

- [ ] **Step 5: Commit**

Message: `feat(core): generate the Windows Terminal scheme and cmd profile values`
```bash
git add -- crates/core/src/generate/wt.rs crates/core/src/generate/mod.rs
git diff --cached --name-only
git commit -F .git/CMDSTUDIO_COMMIT_MSG
```

---

### Task 11: The planner, `<clink-launch>` and AutoRun composition

**Files:**
- Create: `crates/core/src/plan.rs`
- Modify: `crates/core/src/lib.rs` (add `pub mod plan;`)

**Interfaces:**
- Consumes:
  - `generate::{omp, lua, clink, wt}`, including `wt::profile_values(&Theme, Target, clink_launch: &Path, clink_profile: &Path)` from Task 10
  - `model::{Theme, Target}`
- Produces:
  - `plan::OMP_VERSION = "31.5.0"`, `plan::CLINK_VERSION = "1.9.34"`, `plan::NERD_FONT_FAMILY = "CaskaydiaCove NF"`
  - `plan::Paths { data: PathBuf }`, with methods `generated()`, `clink_profile()`, `omp_dir()`, `clink_dir()`, `omp_exe()`, `clink_exe()`, `omp_config()`, each returning `PathBuf`.
  - `plan::Machine { wt_settings: Vec<PathBuf>, autorun_hkcu: Option<String>, autorun_hklm: Option<String>, fonts: Vec<String>, data_short: Option<PathBuf>, program_data_dir: Option<PathBuf> }`, with `Default`.
  - `plan::ClinkLaunch { Direct(PathBuf), ShortPath(PathBuf), ProgramData { dir: PathBuf } }`, with `exe() -> PathBuf` and `runtime_dir(&Paths) -> PathBuf`.
  - `plan::clink_launch(&Paths, &Machine) -> Result<ClinkLaunch, PlanError>`
  - `plan::Request { target: Target, set_wt_default_profile: bool, lang: lua::Lang, replace_foreign_clink: bool }`
  - `plan::Component { Omp, Clink }`
  - `plan::Op` (serde, tag `kind`), with variants:
    - `InstallRuntime { component, dir }`
    - `WriteFile { path, contents }`
    - `WtApply { settings, scheme: Value, profile: Map<String, Value>, set_default }`
    - `SetAutoRun { value: Option<String> }`
    - `InstallFont { family }`
  - `plan::Warning { NoWindowsTerminal, MachineWideClink, FontFallback { wanted: String } }`
  - `plan::Plan { ops: Vec<Op>, summary_th: Vec<&'static str>, summary_en: Vec<&'static str>, warnings: Vec<Warning>, launch: ClinkLaunch }`
  - `plan::PlanError { ForeignClink { existing }, UnsupportedPath(PathBuf), NoClinkPath(PathBuf), Generate(String) }`, with `code()`.
  - `plan::autorun_entry(&ClinkLaunch, &Paths) -> Result<String, PlanError>`
  - `plan::plan(&Theme, &Request, &Machine, &Paths) -> Result<Plan, PlanError>`

**Rules this task pins** (spec §3.3 and §5.4; G12, G13 and G19):
- `<clink-launch>` is the first all-ASCII path of these three:
  1. `Paths::clink_exe()`;
  2. the same layout under `Machine::data_short`;
  3. `Machine::program_data_dir` joined to `clink-<ver>\clink_x64.exe`. Clink's runtime is then installed only there.

  If none is all ASCII, the plan fails with `NoClinkPath`.
- `%` or `"` in the launch path or the profile path fails with `UnsupportedPath`, for both targets. Both errors have the code `E_CLINK_PATH`.
- The profile always stays at `<data>\clink-profile`. Our older AutoRun entries are found by that profile path.
- Fonts matter only when Windows Terminal is installed:
  - When the theme's font is installed (ignoring ASCII case), Windows Terminal gets it in the machine's spelling.
  - Otherwise Windows Terminal gets `NERD_FONT_FAMILY`, with `Warning::FontFallback`. A theme that names the bundled font itself gets no warning.
  - `InstallFont` is added whenever `NERD_FONT_FAMILY` is used and not installed.
- The switch to the Nerd Font when a user picks the powerline layout belongs to the theme editor (spec §3.4, Phase 2). The planner only checks that the font exists.

- [ ] **Step 1: Write the failing tests** in a new `crates/core/src/plan.rs`, and add `pub mod plan;` to `lib.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::lua::Lang;
    use crate::model::Target;
    use std::path::PathBuf;

    // The Thai user name and the 8.3 names that VM-C gave
    // (docs/spikes/G4-G5-G16-G17-clink.md, G19).
    const DATA: &str = r"C:\Users\ผู้ทดสอบ ไทย\AppData\Local\ai.resincore.cmdstudio";
    const DATA_SHORT: &str = r"C:\Users\3607~1\AppData\Local\AIRESI~1.CMD";
    const SHORT_EXE: &str = r"C:\Users\3607~1\AppData\Local\AIRESI~1.CMD\runtime\clink-1.9.34\clink_x64.exe";
    const PROGRAM_DATA: &str = r"C:\ProgramData\ai.resincore.cmdstudio-S-1-5-21-1-2-3-1001";

    fn paths() -> Paths {
        Paths { data: PathBuf::from(DATA) }
    }

    fn req(target: Target) -> Request {
        Request { target, set_wt_default_profile: true, lang: Lang::Th, replace_foreign_clink: false }
    }

    fn machine() -> Machine {
        Machine {
            wt_settings: vec![PathBuf::from(r"C:\wt\settings.json")],
            fonts: vec!["Cascadia Mono".to_string()],
            data_short: Some(PathBuf::from(DATA_SHORT)),
            ..Machine::default()
        }
    }

    fn theme() -> crate::model::Theme {
        crate::presets::builtin().unwrap().remove(0).1
    }

    /// Our entry for `paths()` on `machine()`, built without `clink_launch`.
    fn entry() -> String {
        autorun_entry(&ClinkLaunch::ShortPath(PathBuf::from(SHORT_EXE)), &paths()).unwrap()
    }

    fn autorun(p: &Plan) -> Option<Option<String>> {
        p.ops.iter().find_map(|o| match o {
            Op::SetAutoRun { value } => Some(value.clone()),
            _ => None,
        })
    }

    fn wt_profile(p: &Plan) -> Map<String, Value> {
        p.ops
            .iter()
            .find_map(|o| match o {
                Op::WtApply { profile, .. } => Some(profile.clone()),
                _ => None,
            })
            .unwrap()
    }

    #[test]
    fn clean_machine_gets_runtime_files_wt_and_autorun() {
        let p = plan(&theme(), &req(Target::All), &machine(), &paths()).unwrap();
        assert_eq!(
            entry(),
            format!(r#"if exist "{SHORT_EXE}" "{SHORT_EXE}" inject --autorun --quiet --profile "{DATA}\clink-profile""#)
        );
        assert_eq!(autorun(&p), Some(Some(entry())));
        assert_eq!(p.ops.iter().filter(|o| matches!(o, Op::WtApply { .. })).count(), 1);
        assert_eq!(p.ops.iter().filter(|o| matches!(o, Op::WriteFile { .. })).count(), 4);
        assert!(p.summary_th.contains(&"ให้ cmd ทุกหน้าต่างโหลดธีมตอนเปิด"));
        assert!(p.warnings.is_empty());
    }

    #[test]
    fn an_ascii_data_path_runs_clink_from_its_real_path() {
        let p = Paths { data: PathBuf::from(r"C:\Users\tester\AppData\Local\ai.resincore.cmdstudio") };
        assert_eq!(clink_launch(&p, &machine()), Ok(ClinkLaunch::Direct(p.clink_exe())));
    }

    #[test]
    fn a_thai_user_name_runs_clink_through_the_8_3_path() {
        // Review Focus 3, G19
        let p = plan(&theme(), &req(Target::All), &machine(), &paths()).unwrap();
        assert_eq!(p.launch, ClinkLaunch::ShortPath(PathBuf::from(SHORT_EXE)));
        assert!(p.ops.contains(&Op::InstallRuntime { component: Component::Clink, dir: paths().clink_dir() }));
    }

    #[test]
    fn without_an_8_3_name_clink_goes_to_the_users_program_data_folder() {
        let mut m = machine();
        // A volume that makes no 8.3 names gives back the long path.
        m.data_short = Some(PathBuf::from(DATA));
        m.program_data_dir = Some(PathBuf::from(PROGRAM_DATA));
        let p = plan(&theme(), &req(Target::All), &m, &paths()).unwrap();
        let dir = PathBuf::from(PROGRAM_DATA).join("clink-1.9.34");
        assert_eq!(p.launch, ClinkLaunch::ProgramData { dir: dir.clone() });
        assert!(p.ops.contains(&Op::InstallRuntime { component: Component::Clink, dir: dir.clone() }));
        assert!(!p.ops.contains(&Op::InstallRuntime { component: Component::Clink, dir: paths().clink_dir() }));
        let value = autorun(&p).unwrap().unwrap();
        assert!(value.starts_with(&format!(r#"if exist "{PROGRAM_DATA}\clink-1.9.34\clink_x64.exe" "#)));
        assert!(p.summary_en.contains(&"Put Clink under ProgramData, because this account's folder has letters Clink can't use"));
    }

    #[test]
    fn no_ascii_path_for_clink_fails_the_plan() {
        // G19: never write a command that makes every cmd window show an error.
        let mut m = machine();
        m.data_short = None;
        assert_eq!(plan(&theme(), &req(Target::All), &m, &paths()).unwrap_err().code(), "E_CLINK_PATH");
        // Each candidate must itself be all ASCII (spec §5.4).
        m.data_short = Some(PathBuf::from(r"C:\Users\ÉTÉ~1\AppData\Local\AIRESI~1.CMD"));
        m.program_data_dir = Some(PathBuf::from(r"D:\ข้อมูล\ai.resincore.cmdstudio-S-1-5-21-1-2-3-1001"));
        assert_eq!(clink_launch(&paths(), &m), Err(PlanError::NoClinkPath(paths().clink_exe())));
        assert_eq!(plan(&theme(), &req(Target::Wt), &m, &paths()).unwrap_err().code(), "E_CLINK_PATH");
    }

    #[test]
    fn keeps_other_programs_autorun_verbatim() {
        let anaconda = r#""C:\ProgramData\anaconda3\condabin\conda_hook.bat""#;
        let mut m = machine();
        m.autorun_hkcu = Some(anaconda.to_string());
        let p = plan(&theme(), &req(Target::All), &m, &paths()).unwrap();
        assert_eq!(autorun(&p), Some(Some(format!("{anaconda} & {}", entry()))));
    }

    #[test]
    fn replaces_our_older_entry_instead_of_adding_another() {
        let old = format!(r#"if exist "{DATA}\runtime\clink-1.9.0\clink_x64.exe" "{DATA}\runtime\clink-1.9.0\clink_x64.exe" inject --autorun --quiet --profile "{DATA}\clink-profile""#);
        let mut m = machine();
        m.autorun_hkcu = Some(format!("echo hi & {old}"));
        let p = plan(&theme(), &req(Target::All), &m, &paths()).unwrap();
        assert_eq!(autorun(&p), Some(Some(format!("echo hi & {}", entry()))));
    }

    #[test]
    fn nothing_to_do_when_autorun_is_already_ours() {
        let mut m = machine();
        m.autorun_hkcu = Some(entry());
        let p = plan(&theme(), &req(Target::All), &m, &paths()).unwrap();
        assert_eq!(autorun(&p), None);
    }

    #[test]
    fn a_foreign_clink_needs_permission() {
        let foreign = r#""C:\Program Files (x86)\clink\clink.bat" inject --autorun"#;
        let mut m = machine();
        m.autorun_hkcu = Some(foreign.to_string());
        let err = plan(&theme(), &req(Target::All), &m, &paths()).unwrap_err();
        assert_eq!(err.code(), "E_AUTORUN_FOREIGN_CLINK");
        let mut r = req(Target::All);
        r.replace_foreign_clink = true;
        let p = plan(&theme(), &r, &m, &paths()).unwrap();
        assert_eq!(autorun(&p), Some(Some(entry())));
        assert!(p.summary_en.contains(&"Turn off the other Clink set up in cmd (can be undone)"));
    }

    #[test]
    fn wt_target_removes_our_entry_and_injects_from_the_profile() {
        let mut m = machine();
        m.autorun_hkcu = Some(format!("echo hi & {}", entry()));
        let p = plan(&theme(), &req(Target::Wt), &m, &paths()).unwrap();
        assert_eq!(autorun(&p), Some(Some("echo hi".to_string())));
        assert_eq!(
            wt_profile(&p)["commandline"],
            format!(r#"%SystemRoot%\System32\cmd.exe /k ""{SHORT_EXE}" inject --quiet --profile "{DATA}\clink-profile"""#)
        );
    }

    #[test]
    fn wt_target_with_only_our_entry_deletes_the_value() {
        let mut m = machine();
        m.autorun_hkcu = Some(entry());
        let p = plan(&theme(), &req(Target::Wt), &m, &paths()).unwrap();
        assert_eq!(autorun(&p), Some(None));
    }

    #[test]
    fn missing_windows_terminal_is_a_warning_not_an_error() {
        let mut m = machine();
        m.wt_settings.clear();
        m.fonts.clear();
        let p = plan(&theme(), &req(Target::All), &m, &paths()).unwrap();
        // Without Windows Terminal there is no font to set, so no font is installed.
        assert_eq!(p.warnings, vec![Warning::NoWindowsTerminal]);
        assert!(!p.ops.iter().any(|o| matches!(o, Op::WtApply { .. } | Op::InstallFont { .. })));
    }

    #[test]
    fn machine_wide_clink_is_flagged() {
        let mut m = machine();
        m.autorun_hklm = Some(r#""C:\Program Files\clink\clink.bat" inject --autorun"#.to_string());
        let p = plan(&theme(), &req(Target::All), &m, &paths()).unwrap();
        assert!(p.warnings.contains(&Warning::MachineWideClink));
    }

    #[test]
    fn refuses_paths_cmd_cannot_quote() {
        // Review Focus 3
        for bad in [r"C:\Users\50%off\x", r#"C:\Users\a"b\x"#] {
            let p = Paths { data: PathBuf::from(bad) };
            let launch = clink_launch(&p, &Machine::default()).unwrap();
            assert!(matches!(autorun_entry(&launch, &p), Err(PlanError::UnsupportedPath(_))), "{bad}");
        }
        // The profile path is checked too, even when Clink runs from the 8.3 path.
        let p = Paths { data: PathBuf::from(r"C:\Users\ผู้ใช้ 100%\x") };
        let m = Machine { data_short: Some(PathBuf::from(r"C:\Users\E0B8~1\x")), ..Machine::default() };
        let launch = clink_launch(&p, &m).unwrap();
        assert!(matches!(autorun_entry(&launch, &p), Err(PlanError::UnsupportedPath(_))));
    }

    #[test]
    fn wt_target_refuses_those_paths_too() {
        // Review Focus 3: the profile's `/k` line runs the same command.
        let p = Paths { data: PathBuf::from(r"C:\Users\50%off\x") };
        let err = plan(&theme(), &req(Target::Wt), &machine(), &p).unwrap_err();
        assert_eq!(err.code(), "E_CLINK_PATH");
    }

    #[test]
    fn ampersand_and_spaces_in_paths_are_quoted_and_stay_one_part() {
        // Review Focus 3
        let p = Paths { data: PathBuf::from(r"C:\Users\R&D Team\x") };
        let launch = clink_launch(&p, &Machine::default()).unwrap();
        let entry = autorun_entry(&launch, &p).unwrap();
        assert!(entry.contains(r#""C:\Users\R&D Team\x\runtime\clink-1.9.34\clink_x64.exe""#));
        assert_eq!(parse_chain(&entry).parts.len(), 1);
    }

    #[test]
    fn double_ampersand_between_foreign_parts_is_preserved() {
        let mut m = machine();
        m.autorun_hkcu = Some("a && b".to_string());
        let p = plan(&theme(), &req(Target::All), &m, &paths()).unwrap();
        assert_eq!(autorun(&p), Some(Some(format!("a && b & {}", entry()))));
    }

    #[test]
    fn a_missing_font_falls_back_to_the_bundled_nerd_font() {
        // spec §3.3
        let mut m = machine();
        m.fonts.clear();
        let p = plan(&theme(), &req(Target::All), &m, &paths()).unwrap();
        assert_eq!(p.warnings, vec![Warning::FontFallback { wanted: "Cascadia Mono".to_string() }]);
        assert!(p.ops.contains(&Op::InstallFont { family: NERD_FONT_FAMILY.to_string() }));
        assert_eq!(wt_profile(&p)["font"]["face"], NERD_FONT_FAMILY);
    }

    #[test]
    fn the_fallback_font_is_not_installed_twice() {
        let mut m = machine();
        m.fonts = vec![NERD_FONT_FAMILY.to_string()];
        let p = plan(&theme(), &req(Target::All), &m, &paths()).unwrap();
        assert_eq!(p.warnings, vec![Warning::FontFallback { wanted: "Cascadia Mono".to_string() }]);
        assert!(!p.ops.iter().any(|o| matches!(o, Op::InstallFont { .. })));
        assert_eq!(wt_profile(&p)["font"]["face"], NERD_FONT_FAMILY);
    }

    #[test]
    fn an_installed_font_is_kept_in_the_machines_spelling() {
        let mut m = machine();
        m.fonts = vec!["CASCADIA MONO".to_string()];
        let p = plan(&theme(), &req(Target::All), &m, &paths()).unwrap();
        assert_eq!(wt_profile(&p)["font"]["face"], "CASCADIA MONO");
        assert!(!p.ops.iter().any(|o| matches!(o, Op::InstallFont { .. })));
        assert!(p.warnings.is_empty());
    }

    #[test]
    fn a_theme_that_names_the_bundled_font_gets_it_without_a_warning() {
        let mut t = theme();
        t.terminal.font = NERD_FONT_FAMILY.to_string();
        let p = plan(&t, &req(Target::All), &machine(), &paths()).unwrap();
        assert!(p.ops.contains(&Op::InstallFont { family: NERD_FONT_FAMILY.to_string() }));
        assert!(p.warnings.is_empty());
        assert_eq!(wt_profile(&p)["font"]["face"], NERD_FONT_FAMILY);
    }

    #[test]
    fn ops_serialize_for_the_journal() {
        let p = plan(&theme(), &req(Target::All), &machine(), &paths()).unwrap();
        let json = serde_json::to_string(&p.ops).unwrap();
        let back: Vec<Op> = serde_json::from_str(&json).unwrap();
        assert_eq!(back, p.ops);
    }
}
```

- [ ] **Step 2: Run the tests and confirm they fail**

Run: `cargo test -p cmdstudio-core plan`
Expected: compile errors.

- [ ] **Step 3: Implement it above the tests**
```rust
//! The planner (spec §5.2 `plan`): theme + request + machine facts → the
//! operations to run and a plain-language summary (spec §4.2 page 5).
//! Pure: it reads nothing and writes nothing.
use crate::generate::{clink, lua, omp, wt};
use crate::model::{Target, Theme};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

pub const OMP_VERSION: &str = "31.5.0";
pub const CLINK_VERSION: &str = "1.9.34";
/// GDI family name of the bundled Nerd Font: the name detect looks for, the
/// `font.face` written to Windows Terminal, and the fallback font
/// (spec §3.3, §5.4, G12).
pub const NERD_FONT_FAMILY: &str = "CaskaydiaCove NF";

/// Where CMD Studio keeps its data (spec §5.3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Paths {
    pub data: PathBuf,
}

impl Paths {
    pub fn generated(&self) -> PathBuf {
        self.data.join("generated")
    }
    pub fn clink_profile(&self) -> PathBuf {
        self.data.join("clink-profile")
    }
    pub fn omp_dir(&self) -> PathBuf {
        self.data.join("runtime").join(format!("omp-{OMP_VERSION}"))
    }
    pub fn clink_dir(&self) -> PathBuf {
        self.data.join("runtime").join(format!("clink-{CLINK_VERSION}"))
    }
    pub fn omp_exe(&self) -> PathBuf {
        self.omp_dir().join("oh-my-posh.exe")
    }
    pub fn clink_exe(&self) -> PathBuf {
        self.clink_dir().join("clink_x64.exe")
    }
    pub fn omp_config(&self) -> PathBuf {
        self.generated().join("theme.omp.json")
    }
}

/// What the planner needs to know about the machine. Filled by `detect` (Phase 1b).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Machine {
    /// settings.json of every Windows Terminal variant found.
    pub wt_settings: Vec<PathBuf>,
    /// Current HKCU AutoRun value.
    pub autorun_hkcu: Option<String>,
    /// Current HKLM AutoRun value (read only; CMD Studio never writes HKLM).
    pub autorun_hklm: Option<String>,
    /// Family names of the installed fonts, as GDI lists them. Whether the
    /// fonts packaged inside Windows Terminal count is decided in Phase 1b.
    pub fonts: Vec<String>,
    /// `GetShortPathNameW` of `Paths::data`. On a volume that makes no 8.3
    /// names it is the long path itself (G19).
    pub data_short: Option<PathBuf>,
    /// `%ProgramData%\ai.resincore.cmdstudio-<SID>`, given only when it does
    /// not exist yet or passed the owner and permission check (spec §5.4).
    pub program_data_dir: Option<PathBuf>,
}

/// `<clink-launch>`: the path cmd uses to start Clink (spec §5.4). Clink
/// 1.9.34 cannot inject from a path with non-ASCII letters (G19), so every
/// variant holds an all-ASCII path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClinkLaunch {
    /// The real path under the data folder.
    Direct(PathBuf),
    /// The data folder's 8.3 path joined to the long ASCII rest.
    ShortPath(PathBuf),
    /// Clink installed in the user's own folder under `%ProgramData%`.
    ProgramData { dir: PathBuf },
}

impl ClinkLaunch {
    pub fn exe(&self) -> PathBuf {
        match self {
            ClinkLaunch::Direct(exe) | ClinkLaunch::ShortPath(exe) => exe.clone(),
            ClinkLaunch::ProgramData { dir } => dir.join("clink_x64.exe"),
        }
    }

    /// Where Clink's runtime is installed.
    pub fn runtime_dir(&self, paths: &Paths) -> PathBuf {
        match self {
            ClinkLaunch::ProgramData { dir } => dir.clone(),
            ClinkLaunch::Direct(_) | ClinkLaunch::ShortPath(_) => paths.clink_dir(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub target: Target,
    pub set_wt_default_profile: bool,
    pub lang: lua::Lang,
    /// The user agreed to turn off another Clink found in AutoRun (spec §5.4).
    pub replace_foreign_clink: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Component {
    Omp,
    Clink,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Op {
    InstallRuntime { component: Component, dir: PathBuf },
    WriteFile { path: PathBuf, contents: String },
    WtApply { settings: PathBuf, scheme: Value, profile: Map<String, Value>, set_default: bool },
    /// The complete new HKCU AutoRun value; `None` deletes it.
    SetAutoRun { value: Option<String> },
    InstallFont { family: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Warning {
    NoWindowsTerminal,
    MachineWideClink,
    /// The theme's font is not installed, so Windows Terminal gets
    /// `NERD_FONT_FAMILY` instead (spec §3.3).
    FontFallback { wanted: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    pub ops: Vec<Op>,
    pub summary_th: Vec<&'static str>,
    pub summary_en: Vec<&'static str>,
    pub warnings: Vec<Warning>,
    /// The `<clink-launch>` this plan uses. Phase 1b records it in `state.json`.
    pub launch: ClinkLaunch,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PlanError {
    #[error("another Clink runs from cmd's AutoRun: {existing}")]
    ForeignClink { existing: String },
    #[error("cmd can't safely quote this path: {0:?}")]
    UnsupportedPath(PathBuf),
    #[error("Clink can't start from {0:?}, and no 8.3 or ProgramData path is available instead")]
    NoClinkPath(PathBuf),
    #[error("could not build the Oh My Posh config: {0}")]
    Generate(String),
}

impl PlanError {
    /// Stable code shown to users (spec §5.9).
    pub fn code(&self) -> &'static str {
        match self {
            PlanError::ForeignClink { .. } => "E_AUTORUN_FOREIGN_CLINK",
            PlanError::UnsupportedPath(_) | PlanError::NoClinkPath(_) => "E_CLINK_PATH",
            PlanError::Generate(_) => "E_THEME_INVALID",
        }
    }
}

fn is_ascii(p: &Path) -> bool {
    p.to_str().is_some_and(str::is_ascii)
}

/// Picks `<clink-launch>`: the first all-ASCII path in the order of spec §5.4.
pub fn clink_launch(paths: &Paths, machine: &Machine) -> Result<ClinkLaunch, PlanError> {
    let real = paths.clink_exe();
    if is_ascii(&real) {
        return Ok(ClinkLaunch::Direct(real));
    }
    if let Some(short) = &machine.data_short {
        let exe = Paths { data: short.clone() }.clink_exe();
        if is_ascii(&exe) {
            return Ok(ClinkLaunch::ShortPath(exe));
        }
    }
    if let Some(base) = &machine.program_data_dir {
        let dir = base.join(format!("clink-{CLINK_VERSION}"));
        if is_ascii(&dir) {
            return Ok(ClinkLaunch::ProgramData { dir });
        }
    }
    Err(PlanError::NoClinkPath(real))
}

/// Paths with `%` or `"` are refused: cmd would expand or split them, and
/// every cmd window would show an error (spec §5.4, G13).
fn quotable(p: &Path) -> Result<(), PlanError> {
    let s = p.to_string_lossy();
    if s.contains('%') || s.contains('"') {
        return Err(PlanError::UnsupportedPath(p.to_path_buf()));
    }
    Ok(())
}

/// Our AutoRun entry (spec §5.4).
pub fn autorun_entry(launch: &ClinkLaunch, paths: &Paths) -> Result<String, PlanError> {
    let exe = launch.exe();
    let profile = paths.clink_profile();
    quotable(&exe)?;
    quotable(&profile)?;
    Ok(format!(
        r#"if exist "{e}" "{e}" inject --autorun --quiet --profile "{p}""#,
        e = exe.display(),
        p = profile.display()
    ))
}

/// An AutoRun value split on top-level `&` / `&&` (quotes respected).
pub(crate) struct Chain {
    pub(crate) parts: Vec<String>,
    seps: Vec<&'static str>,
}

pub(crate) fn parse_chain(value: &str) -> Chain {
    let mut raw = vec![String::new()];
    let mut raw_seps: Vec<&'static str> = Vec::new();
    let mut in_quotes = false;
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '"' {
            in_quotes = !in_quotes;
        }
        if ch == '&' && !in_quotes {
            let sep = if chars.peek() == Some(&'&') {
                chars.next();
                "&&"
            } else {
                "&"
            };
            raw_seps.push(sep);
            raw.push(String::new());
            continue;
        }
        if let Some(last) = raw.last_mut() {
            last.push(ch);
        }
    }
    let mut chain = Chain { parts: Vec::new(), seps: Vec::new() };
    for (i, part) in raw.into_iter().enumerate() {
        let part = part.trim().to_string();
        if part.is_empty() {
            continue;
        }
        if !chain.parts.is_empty() {
            chain.seps.push(if i > 0 { raw_seps[i - 1] } else { "&" });
        }
        chain.parts.push(part);
    }
    chain
}

impl Chain {
    fn remove_where(&mut self, pred: impl Fn(&str) -> bool) -> usize {
        let mut removed = 0;
        let mut i = 0;
        while i < self.parts.len() {
            if pred(&self.parts[i]) {
                self.parts.remove(i);
                if i > 0 {
                    self.seps.remove(i - 1);
                } else if !self.seps.is_empty() {
                    self.seps.remove(0);
                }
                removed += 1;
            } else {
                i += 1;
            }
        }
        removed
    }

    fn push(&mut self, part: String) {
        if !self.parts.is_empty() {
            self.seps.push("&");
        }
        self.parts.push(part);
    }

    fn render(&self) -> Option<String> {
        let first = self.parts.first()?;
        let mut s = first.clone();
        for (sep, part) in self.seps.iter().zip(self.parts.iter().skip(1)) {
            s.push(' ');
            s.push_str(sep);
            s.push(' ');
            s.push_str(part);
        }
        Some(s)
    }
}

/// The next AutoRun value, and whether another Clink was removed. Our older
/// entries are found by the profile path, which every form of our entry holds.
fn next_autorun(current: Option<&str>, target: Target, entry: &str, paths: &Paths, replace_foreign: bool) -> Result<(Option<String>, bool), PlanError> {
    let ours = paths.clink_profile().display().to_string();
    let mut chain = parse_chain(current.unwrap_or(""));
    chain.remove_where(|p| p.contains(&ours));
    let foreign = |p: &str| p.to_ascii_lowercase().contains("clink");
    if let Some(existing) = chain.parts.iter().find(|p| foreign(p.as_str())) {
        if !replace_foreign {
            return Err(PlanError::ForeignClink { existing: existing.clone() });
        }
    }
    let replaced = chain.remove_where(foreign) > 0;
    if target == Target::All {
        chain.push(entry.to_string());
    }
    Ok((chain.render(), replaced))
}

/// The installed family that matches `family`, ignoring ASCII case, in the
/// machine's own spelling.
fn installed_font<'a>(machine: &'a Machine, family: &str) -> Option<&'a str> {
    let family = family.trim();
    machine.fonts.iter().map(|f| f.trim()).find(|f| f.eq_ignore_ascii_case(family))
}

/// The font face for Windows Terminal (spec §3.3): the theme's font when it is
/// installed, otherwise the bundled Nerd Font with a warning.
fn wt_font(theme: &Theme, machine: &Machine, warnings: &mut Vec<Warning>) -> String {
    let wanted = theme.terminal.font.trim();
    if let Some(installed) = installed_font(machine, wanted) {
        return installed.to_string();
    }
    if !wanted.eq_ignore_ascii_case(NERD_FONT_FAMILY) {
        warnings.push(Warning::FontFallback { wanted: wanted.to_string() });
    }
    NERD_FONT_FAMILY.to_string()
}

pub fn plan(theme: &Theme, req: &Request, machine: &Machine, paths: &Paths) -> Result<Plan, PlanError> {
    let launch = clink_launch(paths, machine)?;
    // Built for both targets: it also checks the paths that the WT-only
    // target's `/k` line uses.
    let entry = autorun_entry(&launch, paths)?;
    let omp_text = omp::config_text(theme).map_err(|e| PlanError::Generate(e.to_string()))?;
    let mut ops = vec![
        Op::InstallRuntime { component: Component::Omp, dir: paths.omp_dir() },
        Op::InstallRuntime { component: Component::Clink, dir: launch.runtime_dir(paths) },
        Op::WriteFile { path: paths.omp_config(), contents: omp_text },
        Op::WriteFile { path: paths.generated().join("init.lua"), contents: lua::init_lua(&paths.omp_exe(), &paths.omp_config()) },
        Op::WriteFile { path: paths.generated().join("banner.lua"), contents: lua::banner_lua(theme, req.lang) },
        Op::WriteFile {
            path: paths.clink_profile().join("clink_settings"),
            contents: clink::settings(theme.prompt.transient, &paths.generated()),
        },
    ];
    let mut th = vec!["ปรับ prompt และกล่องต้อนรับตามธีม"];
    let mut en = vec!["Update the prompt and the welcome box"];
    let mut warnings = Vec::new();

    if matches!(launch, ClinkLaunch::ProgramData { .. }) {
        th.push("วาง Clink ไว้ใต้ ProgramData เพราะ path ของบัญชีนี้มีอักษรที่ Clink ใช้ไม่ได้");
        en.push("Put Clink under ProgramData, because this account's folder has letters Clink can't use");
    }

    if machine.wt_settings.is_empty() {
        warnings.push(Warning::NoWindowsTerminal);
    } else {
        let font = wt_font(theme, machine, &mut warnings);
        if font == NERD_FONT_FAMILY && installed_font(machine, NERD_FONT_FAMILY).is_none() {
            ops.push(Op::InstallFont { family: NERD_FONT_FAMILY.to_string() });
            th.push("ติดตั้งฟอนต์ 1 ตัว (เฉพาะบัญชีนี้)");
            en.push("Install 1 font for this account only");
        }
        let mut wt_theme = theme.clone();
        wt_theme.terminal.font = font;
        let scheme = wt::scheme(&wt_theme);
        let profile = wt::profile_values(&wt_theme, req.target, &launch.exe(), &paths.clink_profile());
        for settings in &machine.wt_settings {
            ops.push(Op::WtApply {
                settings: settings.clone(),
                scheme: scheme.clone(),
                profile: profile.clone(),
                set_default: req.set_wt_default_profile,
            });
        }
        th.push("สีและตัวอักษรของหน้าต่าง cmd ใน Windows Terminal");
        en.push("Colors and font of cmd in Windows Terminal");
        if req.set_wt_default_profile {
            th.push("เปิด Windows Terminal มาเป็น cmd");
            en.push("Open Windows Terminal with cmd");
        }
    }

    if machine.autorun_hklm.as_deref().is_some_and(|v| v.to_ascii_lowercase().contains("clink")) {
        warnings.push(Warning::MachineWideClink);
    }

    let (next, replaced) = next_autorun(machine.autorun_hkcu.as_deref(), req.target, &entry, paths, req.replace_foreign_clink)?;
    if replaced {
        th.push("ปิด Clink ตัวเดิมที่ตั้งไว้ใน cmd (ย้อนกลับได้)");
        en.push("Turn off the other Clink set up in cmd (can be undone)");
    }
    if next != machine.autorun_hkcu {
        match req.target {
            Target::All => {
                th.push("ให้ cmd ทุกหน้าต่างโหลดธีมตอนเปิด");
                en.push("Load the theme in every cmd window");
            }
            Target::Wt => {
                th.push("หยุดโหลดธีมในหน้าต่าง cmd นอก Windows Terminal");
                en.push("Stop loading the theme in cmd windows outside Windows Terminal");
            }
        }
        ops.push(Op::SetAutoRun { value: next });
    }

    Ok(Plan { ops, summary_th: th, summary_en: en, warnings, launch })
}
```

- [ ] **Step 4: Run the tests and confirm they pass**

Run: `cargo test -p cmdstudio-core plan`
Expected: 22 passed.

- [ ] **Step 5: Commit**

Message:
```text
feat(core): plan an apply, pick <clink-launch> and compose AutoRun

The plan lists every operation plus the plain-language summary the
wizard shows. Clink always starts from an all-ASCII path: its real
path, the data folder's 8.3 path, or the user's own ProgramData
folder, because Clink 1.9.34 cannot inject from a Thai path (G19).
AutoRun keeps other programs' parts verbatim, replaces our older
entries, asks before removing a foreign Clink, and refuses paths that
cmd cannot quote. A missing theme font falls back to the bundled
CaskaydiaCove NF.
```
```bash
git add -- crates/core/src/plan.rs crates/core/src/lib.rs
git diff --cached --name-only
git commit -F .git/CMDSTUDIO_COMMIT_MSG
```

---

### Task 12: The transactional apply engine

**Files:**
- Create: `crates/core/src/txn.rs`
- Modify: `crates/core/src/lib.rs` (add `pub mod txn;`), `docs/superpowers/specs/2026-10-10-cmd-studio-design.md` (§5.9: add `E_TXN_INTERRUPTED`)

**Interfaces:**
- Consumes: `plan::Op`
- Produces:
  - `txn::Failure { code: &'static str, message: String }`
  - `txn::Snapshot { op_index: usize, prior: Option<Vec<u8>> }`
  - `txn::Stage { Planned, Snapshotted, Applying { done }, Verifying, Committed, RollingBack { from }, RolledBack, RollbackIncomplete { failed: Vec<usize> } }`
  - `txn::Journal { id: String, ops: Vec<Op>, snapshots: Vec<Snapshot>, stage: Stage }`, with `is_finished()`.
  - Traits:
    - `txn::Executor { capture(&mut self, &Op) -> Result<Option<Vec<u8>>, Failure>; apply(&mut self, &Op) -> Result<(), Failure>; restore(&mut self, &Op, &Option<Vec<u8>>) -> Result<(), Failure> }`
    - `txn::Verifier { verify(&mut self, &[Op]) -> Result<(), Failure> }`
    - `txn::JournalStore { save(&mut self, &Journal) -> Result<(), Failure>; load_unfinished(&mut self) -> Result<Option<Journal>, Failure> }`
  - `txn::Outcome { Committed, RolledBack { code, reason }, RollbackIncomplete { reason, failed } }`
  - `txn::run(id, ops, &mut dyn Executor, &mut dyn Verifier, &mut dyn JournalStore) -> Outcome`
  - `txn::recover(&mut dyn Executor, &mut dyn JournalStore) -> Option<Outcome>`

- [ ] **Step 1: Write the failing tests** in a new `crates/core/src/txn.rs`, and add `pub mod txn;` to `lib.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::Op;
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    /// A fake machine: a key → bytes map, with optional injected failures.
    #[derive(Default)]
    struct FakeMachine {
        state: BTreeMap<String, Vec<u8>>,
        fail_apply_key: Option<String>,
        fail_restore_key: Option<String>,
    }

    fn key(op: &Op) -> String {
        match op {
            Op::WriteFile { path, .. } => path.display().to_string(),
            Op::SetAutoRun { .. } => "autorun".to_string(),
            Op::WtApply { settings, .. } => settings.display().to_string(),
            Op::InstallFont { family } => format!("font:{family}"),
            Op::InstallRuntime { dir, .. } => dir.display().to_string(),
        }
    }

    fn new_value(op: &Op) -> Option<Vec<u8>> {
        match op {
            Op::WriteFile { contents, .. } => Some(contents.as_bytes().to_vec()),
            Op::SetAutoRun { value } => value.as_ref().map(|v| v.as_bytes().to_vec()),
            Op::WtApply { profile, .. } => Some(serde_json::to_vec(profile).unwrap()),
            Op::InstallFont { .. } => Some(b"installed".to_vec()),
            Op::InstallRuntime { .. } => Some(b"runtime".to_vec()),
        }
    }

    impl Executor for FakeMachine {
        fn capture(&mut self, op: &Op) -> Result<Option<Vec<u8>>, Failure> {
            Ok(self.state.get(&key(op)).cloned())
        }
        fn apply(&mut self, op: &Op) -> Result<(), Failure> {
            let k = key(op);
            if self.fail_apply_key.as_deref() == Some(k.as_str()) {
                self.state.insert(k, b"HALF-WRITTEN".to_vec());
                return Err(Failure { code: "E_WT_WRITE", message: "disk said no".into() });
            }
            match new_value(op) {
                Some(v) => self.state.insert(k, v),
                None => self.state.remove(&k),
            };
            Ok(())
        }
        fn restore(&mut self, op: &Op, prior: &Option<Vec<u8>>) -> Result<(), Failure> {
            let k = key(op);
            if self.fail_restore_key.as_deref() == Some(k.as_str()) {
                return Err(Failure { code: "E_ROLLBACK_INCOMPLETE", message: "locked".into() });
            }
            match prior {
                Some(v) => self.state.insert(k, v.clone()),
                None => self.state.remove(&k),
            };
            Ok(())
        }
    }

    struct Verify(Result<(), Failure>);
    impl Verifier for Verify {
        fn verify(&mut self, _: &[Op]) -> Result<(), Failure> {
            self.0.clone()
        }
    }

    #[derive(Default)]
    struct MemStore {
        saved: Vec<Journal>,
    }
    impl JournalStore for MemStore {
        fn save(&mut self, j: &Journal) -> Result<(), Failure> {
            self.saved.push(j.clone());
            Ok(())
        }
        fn load_unfinished(&mut self) -> Result<Option<Journal>, Failure> {
            Ok(self.saved.last().filter(|j| !j.is_finished()).cloned())
        }
    }

    fn ops() -> Vec<Op> {
        vec![
            Op::WriteFile { path: PathBuf::from("a.lua"), contents: "new a".into() },
            Op::WtApply { settings: PathBuf::from("wt.json"), scheme: serde_json::json!({}), profile: serde_json::Map::new(), set_default: false },
            Op::SetAutoRun { value: Some("ours".into()) },
        ]
    }

    fn machine_with_old_state() -> FakeMachine {
        let mut m = FakeMachine::default();
        m.state.insert("a.lua".into(), b"old a".to_vec());
        m.state.insert("wt.json".into(), b"old wt".to_vec());
        m
    }

    #[test]
    fn commits_and_journals_every_stage_in_order() {
        let (mut m, mut store) = (machine_with_old_state(), MemStore::default());
        let out = run("t1", ops(), &mut m, &mut Verify(Ok(())), &mut store);
        assert_eq!(out, Outcome::Committed);
        assert_eq!(m.state["a.lua"], b"new a");
        assert_eq!(m.state["autorun"], b"ours");
        let stages: Vec<Stage> = store.saved.iter().map(|j| j.stage.clone()).collect();
        assert_eq!(
            stages,
            vec![
                Stage::Planned,
                Stage::Snapshotted,
                Stage::Applying { done: 1 },
                Stage::Applying { done: 2 },
                Stage::Applying { done: 3 },
                Stage::Verifying,
                Stage::Committed
            ]
        );
    }

    #[test]
    fn apply_failure_restores_everything_byte_for_byte() {
        let mut m = machine_with_old_state();
        let before = m.state.clone();
        m.fail_apply_key = Some("wt.json".into());
        let out = run("t2", ops(), &mut m, &mut Verify(Ok(())), &mut MemStore::default());
        assert_eq!(out, Outcome::RolledBack { code: "E_WT_WRITE", reason: "disk said no".into() });
        assert_eq!(m.state, before);
    }

    #[test]
    fn verify_failure_rolls_back_with_the_verifier_code() {
        let mut m = machine_with_old_state();
        let before = m.state.clone();
        let mut verify = Verify(Err(Failure { code: "E_VERIFY_OMP", message: "omp exited 1".into() }));
        let out = run("t3", ops(), &mut m, &mut verify, &mut MemStore::default());
        assert_eq!(out, Outcome::RolledBack { code: "E_VERIFY_OMP", reason: "omp exited 1".into() });
        assert_eq!(m.state, before);
    }

    #[test]
    fn a_failed_restore_is_reported_never_hidden() {
        let mut m = machine_with_old_state();
        m.fail_apply_key = Some("autorun".into());
        m.fail_restore_key = Some("a.lua".into());
        let mut store = MemStore::default();
        let out = run("t4", ops(), &mut m, &mut Verify(Ok(())), &mut store);
        assert_eq!(out, Outcome::RollbackIncomplete { reason: "disk said no".into(), failed: vec![0] });
        assert_eq!(store.saved.last().unwrap().stage, Stage::RollbackIncomplete { failed: vec![0] });
    }

    #[test]
    fn recovers_a_transaction_cut_off_mid_apply() {
        let mut m = machine_with_old_state();
        let before = m.state.clone();
        let ops = ops();
        let snapshots: Vec<Snapshot> = ops
            .iter()
            .enumerate()
            .map(|(i, op)| Snapshot { op_index: i, prior: m.capture(op).unwrap() })
            .collect();
        m.apply(&ops[0]).unwrap();
        m.state.insert("wt.json".into(), b"HALF-WRITTEN".to_vec()); // crashed while writing op 1
        let mut store = MemStore { saved: vec![Journal { id: "t5".into(), ops, snapshots, stage: Stage::Applying { done: 1 } }] };
        let out = recover(&mut m, &mut store).unwrap();
        assert_eq!(out, Outcome::RolledBack { code: "E_TXN_INTERRUPTED", reason: "the previous apply was interrupted".into() });
        assert_eq!(m.state, before);
        assert!(store.saved.last().unwrap().is_finished());
    }

    #[test]
    fn finished_journals_are_left_alone() {
        let mut store = MemStore { saved: vec![Journal { id: "t6".into(), ops: ops(), snapshots: vec![], stage: Stage::Committed }] };
        assert_eq!(recover(&mut FakeMachine::default(), &mut store), None);
    }
}
```

- [ ] **Step 2: Run the tests and confirm they fail**

Run: `cargo test -p cmdstudio-core txn`
Expected: compile errors.

- [ ] **Step 3: Implement it above the tests**
```rust
//! The transactional apply engine (spec §5.5). Side effects happen only
//! through `Executor`. The journal is saved before every stage change, so a
//! crash can always be rolled back.
use crate::plan::Op;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    /// Stable code from spec §5.9.
    pub code: &'static str,
    pub message: String,
}

/// Prior state of one operation's target, captured before anything changes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    pub op_index: usize,
    pub prior: Option<Vec<u8>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Stage {
    Planned,
    Snapshotted,
    Applying { done: usize },
    Verifying,
    Committed,
    RollingBack { from: usize },
    RolledBack,
    RollbackIncomplete { failed: Vec<usize> },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Journal {
    pub id: String,
    pub ops: Vec<Op>,
    pub snapshots: Vec<Snapshot>,
    pub stage: Stage,
}

impl Journal {
    pub fn is_finished(&self) -> bool {
        matches!(self.stage, Stage::Committed | Stage::RolledBack | Stage::RollbackIncomplete { .. })
    }
}

pub trait Executor {
    fn capture(&mut self, op: &Op) -> Result<Option<Vec<u8>>, Failure>;
    fn apply(&mut self, op: &Op) -> Result<(), Failure>;
    fn restore(&mut self, op: &Op, prior: &Option<Vec<u8>>) -> Result<(), Failure>;
}

pub trait Verifier {
    fn verify(&mut self, ops: &[Op]) -> Result<(), Failure>;
}

pub trait JournalStore {
    fn save(&mut self, journal: &Journal) -> Result<(), Failure>;
    fn load_unfinished(&mut self) -> Result<Option<Journal>, Failure>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Committed,
    RolledBack { code: &'static str, reason: String },
    /// Some targets could not be restored. The caller must tell the user
    /// which ones and how to fix them by hand (spec §5.5); never report success.
    RollbackIncomplete { reason: String, failed: Vec<usize> },
}

pub fn run(id: &str, ops: Vec<Op>, exec: &mut dyn Executor, verifier: &mut dyn Verifier, store: &mut dyn JournalStore) -> Outcome {
    let mut j = Journal { id: id.to_string(), ops, snapshots: Vec::new(), stage: Stage::Planned };
    if let Err(f) = store.save(&j) {
        return Outcome::RolledBack { code: f.code, reason: f.message };
    }
    for i in 0..j.ops.len() {
        match exec.capture(&j.ops[i]) {
            Ok(prior) => j.snapshots.push(Snapshot { op_index: i, prior }),
            Err(f) => return finish(&mut j, store, Vec::new(), f),
        }
    }
    j.stage = Stage::Snapshotted;
    if let Err(f) = store.save(&j) {
        return finish(&mut j, store, Vec::new(), f);
    }
    for i in 0..j.ops.len() {
        if let Err(f) = exec.apply(&j.ops[i]) {
            // The failed op may be half-applied, so it is restored too.
            return rollback(&mut j, i + 1, exec, store, f);
        }
        j.stage = Stage::Applying { done: i + 1 };
        if let Err(f) = store.save(&j) {
            return rollback(&mut j, i + 1, exec, store, f);
        }
    }
    let all = j.ops.len();
    j.stage = Stage::Verifying;
    if let Err(f) = store.save(&j) {
        return rollback(&mut j, all, exec, store, f);
    }
    if let Err(f) = verifier.verify(&j.ops) {
        return rollback(&mut j, all, exec, store, f);
    }
    j.stage = Stage::Committed;
    match store.save(&j) {
        Ok(()) => Outcome::Committed,
        Err(f) => rollback(&mut j, all, exec, store, f),
    }
}

/// Finishes a transaction that was cut off (the app crashed or was killed).
pub fn recover(exec: &mut dyn Executor, store: &mut dyn JournalStore) -> Option<Outcome> {
    let mut j = match store.load_unfinished() {
        Ok(Some(j)) => j,
        Ok(None) => return None,
        Err(f) => return Some(Outcome::RollbackIncomplete { reason: format!("cannot read the journal: {}", f.message), failed: Vec::new() }),
    };
    let applied = match j.stage {
        Stage::Planned | Stage::Snapshotted => 0,
        // The op after `done` may be half-applied.
        Stage::Applying { done } => (done + 1).min(j.ops.len()),
        Stage::Verifying => j.ops.len(),
        Stage::RollingBack { from } => from,
        Stage::Committed | Stage::RolledBack | Stage::RollbackIncomplete { .. } => return None,
    };
    let cause = Failure { code: "E_TXN_INTERRUPTED", message: "the previous apply was interrupted".to_string() };
    Some(rollback(&mut j, applied, exec, store, cause))
}

fn rollback(j: &mut Journal, applied: usize, exec: &mut dyn Executor, store: &mut dyn JournalStore, cause: Failure) -> Outcome {
    j.stage = Stage::RollingBack { from: applied };
    // Best effort: we are already failing. The outcome reports the real state either way.
    let _ = store.save(j);
    let mut failed = Vec::new();
    for i in (0..applied.min(j.ops.len()).min(j.snapshots.len())).rev() {
        if exec.restore(&j.ops[i], &j.snapshots[i].prior).is_err() {
            failed.push(i);
        }
    }
    finish(j, store, failed, cause)
}

fn finish(j: &mut Journal, store: &mut dyn JournalStore, failed: Vec<usize>, cause: Failure) -> Outcome {
    if failed.is_empty() {
        j.stage = Stage::RolledBack;
        let _ = store.save(j);
        Outcome::RolledBack { code: cause.code, reason: cause.message }
    } else {
        j.stage = Stage::RollbackIncomplete { failed: failed.clone() };
        let _ = store.save(j);
        Outcome::RollbackIncomplete { reason: cause.message, failed }
    }
}
```

- [ ] **Step 4: Run the tests and confirm they pass**

Run: `cargo test -p cmdstudio-core txn`
Expected: 6 passed.

- [ ] **Step 5: Add the new code to the spec** so the spec matches the shipped code. In spec §5.9, add this row to the table after `E_ROLLBACK_INCOMPLETE`:
```markdown
| `E_TXN_INTERRUPTED` | การใช้ธีมครั้งก่อนถูกตัดกลางทาง (โปรแกรมดับ) และย้อนกลับให้แล้วตอนเปิดใหม่ |
```

- [ ] **Step 6: Commit**

Message:
```text
feat(core): add the transactional apply engine

Every op's prior state is captured before anything changes, and the
journal is saved before each stage. A failed apply or verify restores
every touched target in reverse order. A crash is rolled back on the
next start (E_TXN_INTERRUPTED, now listed in spec 5.9). A restore that
fails is reported as RollbackIncomplete, never as success.
```
```bash
git add -- crates/core/src/txn.rs crates/core/src/lib.rs docs/superpowers/specs/2026-10-10-cmd-studio-design.md
git diff --cached --name-only
git commit -F .git/CMDSTUDIO_COMMIT_MSG
```

---

### Task 13: Whole-crate quality gate

**Files:**
- Modify: any file the checks below flag (only the minimal fix)

- [ ] **Step 1: Format check**

Run: `cargo fmt --all -- --check`
Expected: no output. If it reports differences, run `cargo fmt --all` and check that the diff is formatting only.

- [ ] **Step 2: Clippy with warnings as errors**

Run: `cargo clippy -p cmdstudio-core --all-targets -- -D warnings`
Expected: `Finished` with no warnings. Fix the real cause of each warning. Allow a lint only on the exact item it concerns, with a comment explaining why.

- [ ] **Step 3: Full test run, read in full**

Run: `cargo test -p cmdstudio-core 2>&1 | Tee-Object -FilePath target/phase1a-test-output.txt`
Expected: every test listed as `ok`, `0 failed`, and `0 ignored`. Open the file and confirm the property test ran 1000 cases (no `ignored` lines) and the 8 Lua tests ran (not skipped).

- [ ] **Step 4: Production-path check**

Run: `git grep -n -E "unwrap\(|expect\(|panic!" -- crates/core/src`
Expected: matches only inside `#[cfg(test)]` modules. Read each match to confirm, and fix any that is not in test code.

- [ ] **Step 5: Commit** (only if Steps 1–4 changed files). Message: `chore(core): satisfy fmt and clippy for phase 1a`. Then report to the founder in Thai: the test totals, what is proven (the pure core), what is not yet proven (everything that touches Windows, which is Phase 1b), and the link to `target/phase1a-test-output.txt`.
