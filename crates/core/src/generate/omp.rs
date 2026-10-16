//! Theme → Oh My Posh config, format version 4 (spec §3.5). Built only from
//! whitelisted segment types; `text` carries only our own frame glyphs.
//!
//! What each segment looks like in each layout (`drawn`), which segments the left side
//! holds (`left_parts`), the palette (`Swatch`) and the frame characters are shared with
//! the preview emulator (`crate::preview`), which draws the same prompt without running Oh
//! My Posh. The parity test (`tests/parity.rs`) holds the two together.
use crate::color::Rgb;
use crate::model::{Layout, Segment, Theme};
use crate::palette::Colors;
use serde_json::{json, Map, Value};

/// Segment types this generator may emit (doctrine guard, spec §3.7 #2).
pub const ALLOWED_TYPES: &[&str] = &[
    "path",
    "git",
    "time",
    "session",
    "status",
    "executiontime",
    "text",
];

const TIME_FORMAT: &str = "15:04";
/// Opens a Go template `if` that is true when the working tree or the index has changes. The
/// text that uses it closes the `if` with `{{ end }}`.
const IF_CHANGED: &str = "{{ if or .Working.Changed .Staging.Changed }}";

/// The top left corner of the box and the line that starts it.
pub(crate) const BOX_TOP: &str = "\u{256d}\u{2500} ";
/// What closes the box's top line when nothing comes before it but the filler.
pub(crate) const BOX_CLOSE: &str = "\u{2500}\u{256e}";
/// What closes the box's top line after the time: a space sets the corner off from it.
pub(crate) const BOX_CLOSE_AFTER_TIME: &str = " \u{2500}\u{256e}";
/// The character the box's top line is filled with, up to the right side.
pub(crate) const BOX_FILLER: &str = "\u{2500}";
/// The start of the box's second line, the line the command is typed on.
pub(crate) const BOX_SIDE: &str = "\u{2502} ";
/// The prompt character of the box, after [`BOX_SIDE`].
pub(crate) const BOX_PROMPT: &str = ">";
/// The right prompt of the box: its right side, drawn by Clink at the right edge.
pub(crate) const BOX_RIGHT: &str = "\u{2502}";
/// The prompt character of the line layout.
pub(crate) const LINE_PROMPT: &str = "\u{276f}";
/// The prompt character of the powerline layout; the space sets it off from the last arrow.
pub(crate) const POWERLINE_PROMPT: &str = " \u{276f}";
/// The arrow between powerline segments (a Nerd Font glyph).
pub(crate) const POWERLINE_SYMBOL: &str = "\u{e0b0}";
/// What an earlier prompt shrinks to when the theme turns the transient prompt on.
pub(crate) const TRANSIENT_PROMPT: &str = "> ";
/// Git's separator from the path in the box layout, in the muted color.
pub(crate) const GIT_SEPARATOR: &str = "\u{b7} ";
/// After the branch: something changed.
pub(crate) const GIT_DIRTY_MARK: &str = " *";
/// After the branch: nothing changed.
pub(crate) const GIT_CLEAN_MARK: &str = " \u{2713}";
/// Before the exit code of the status segment.
pub(crate) const STATUS_MARK: &str = "\u{2717} ";
/// The duration segment shows from this many milliseconds on (spec §3.5).
pub(crate) const DURATION_THRESHOLD_MS: u64 = 2000;

/// A color of the config's palette, which the segments name as `p:<name>`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Swatch {
    Bg,
    Fg,
    Accent,
    Muted,
    Frame,
    Red,
    Ok,
    Dirty,
    Blue,
}

impl Swatch {
    const ALL: [Swatch; 9] = [
        Swatch::Bg,
        Swatch::Fg,
        Swatch::Accent,
        Swatch::Muted,
        Swatch::Frame,
        Swatch::Red,
        Swatch::Ok,
        Swatch::Dirty,
        Swatch::Blue,
    ];

    /// The name in the palette.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Swatch::Bg => "bg",
            Swatch::Fg => "fg",
            Swatch::Accent => "accent",
            Swatch::Muted => "muted",
            Swatch::Frame => "frame",
            Swatch::Red => "red",
            Swatch::Ok => "ok",
            Swatch::Dirty => "dirty",
            Swatch::Blue => "blue",
        }
    }

    /// The theme color the palette gives this name.
    pub(crate) fn color(self, colors: &Colors) -> Rgb {
        // ansi 1 to 4 are red, green, yellow and blue; destructuring cannot index out of range
        let [_, red, green, yellow, blue, ..] = colors.ansi;
        match self {
            Swatch::Bg => colors.background,
            Swatch::Fg => colors.foreground,
            Swatch::Accent => colors.accent,
            Swatch::Muted => colors.muted,
            Swatch::Frame => colors.frame,
            Swatch::Red => red,
            Swatch::Ok => green,
            Swatch::Dirty => yellow,
            Swatch::Blue => blue,
        }
    }

    /// How a segment names the color: `p:accent`.
    fn reference(self) -> String {
        format!("p:{}", self.name())
    }
}

/// One Oh My Posh segment of the left side, or the box's right side: one per theme segment,
/// except that `user` and `host` share one `Session`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Part {
    Path,
    Git,
    Time,
    Session { user: bool, host: bool },
    Status,
    Duration,
}

/// How a segment is colored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Look {
    /// Text in `fg`, with nothing behind it.
    Plain { fg: Swatch },
    /// Text in `fg` on a block of `bg`, joined to the next segment by an arrow.
    Powerline { bg: Swatch, fg: Swatch },
}

impl Look {
    /// The look git switches to once something has changed: the color its color template
    /// gives (`foreground_templates` in box and line, `background_templates` in powerline).
    pub(crate) fn changed(self) -> Look {
        match self {
            Look::Plain { .. } => Look::Plain { fg: Swatch::Dirty },
            Look::Powerline { fg, .. } => Look::Powerline {
                bg: Swatch::Dirty,
                fg,
            },
        }
    }
}

/// A segment as a layout draws it: its colors, and the text on either side of its data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Drawn {
    pub(crate) look: Look,
    /// Text in a color of its own at the very start (`<p:muted>· </>` before git in the box).
    pub(crate) lead: Option<(Swatch, &'static str)>,
    pub(crate) before: &'static str,
    pub(crate) after: &'static str,
}

/// How `layout` draws `part`. Powerline puts a space on each side of the data, inside the
/// block of color. Box and line put one space after it, except the time on the box's right
/// side, which has its space before.
pub(crate) fn drawn(layout: Layout, part: Part) -> Drawn {
    match layout {
        Layout::Powerline => {
            let (bg, fg) = match part {
                Part::Path | Part::Time => (Swatch::Frame, Swatch::Fg),
                Part::Git => (Swatch::Ok, Swatch::Bg),
                Part::Session { .. } => (Swatch::Accent, Swatch::Bg),
                Part::Status => (Swatch::Red, Swatch::Bg),
                Part::Duration => (Swatch::Dirty, Swatch::Bg),
            };
            Drawn {
                look: Look::Powerline { bg, fg },
                lead: None,
                before: " ",
                after: " ",
            }
        }
        Layout::Box | Layout::Line => {
            let in_box = layout == Layout::Box;
            let fg = match part {
                Part::Path if in_box => Swatch::Muted,
                Part::Path => Swatch::Blue,
                Part::Git => Swatch::Ok,
                Part::Time => Swatch::Muted,
                Part::Session { .. } => Swatch::Fg,
                Part::Status => Swatch::Red,
                Part::Duration => Swatch::Dirty,
            };
            let lead = (in_box && part == Part::Git).then_some((Swatch::Muted, GIT_SEPARATOR));
            let (before, after) = if in_box && part == Part::Time {
                (" ", "")
            } else {
                ("", " ")
            };
            Drawn {
                look: Look::Plain { fg },
                lead,
                before,
                after,
            }
        }
    }
}

/// The character the user types after, in the accent color: on the box's second line, or at
/// the end of the one line of the other layouts.
pub(crate) fn prompt_char(layout: Layout) -> &'static str {
    match layout {
        Layout::Box => BOX_PROMPT,
        Layout::Line => LINE_PROMPT,
        Layout::Powerline => POWERLINE_PROMPT,
    }
}

/// The Oh My Posh config for `theme`, format version 4. It reads the theme's colors, layout
/// and segment list and nothing else, so no text from the theme file ends up in it. The
/// transient prompt is always described; whether Clink uses it is Clink's own
/// `prompt.transient` setting.
pub fn config(theme: &Theme) -> Value {
    let layout = theme.prompt.layout;
    let blocks = match layout {
        Layout::Box => box_blocks(theme),
        Layout::Line | Layout::Powerline => single_line(theme),
    };
    json!({
        "$schema": "https://raw.githubusercontent.com/JanDeDobbeleer/oh-my-posh/main/themes/schema.json",
        "version": 4,
        "final_space": true,
        "console_title_template": "{{ .Folder }}",
        "palette": palette(&theme.colors),
        "blocks": blocks,
        "transient_prompt": {
            "foreground": Swatch::Accent.reference(),
            "background": "transparent",
            "template": TRANSIENT_PROMPT
        }
    })
}

/// [`config`] as the text of the file: pretty-printed, with one final newline.
pub fn config_text(theme: &Theme) -> Result<String, serde_json::Error> {
    let mut text = serde_json::to_string_pretty(&config(theme))?;
    text.push('\n');
    Ok(text)
}

fn palette(colors: &Colors) -> Value {
    let names: Map<String, Value> = Swatch::ALL
        .iter()
        .map(|swatch| {
            (
                swatch.name().to_string(),
                Value::String(swatch.color(colors).to_string()),
            )
        })
        .collect();
    Value::Object(names)
}

/// One Oh My Posh segment under construction. It holds a JSON object, so setting a key
/// cannot fail the way writing `v["key"]` into a `Value` that is not an object would.
#[derive(Default)]
struct Seg(Map<String, Value>);

impl Seg {
    fn set(mut self, key: &str, value: impl Into<Value>) -> Self {
        self.0.insert(key.to_string(), value.into());
        self
    }

    /// A segment of `kind` with `look` and `template`.
    fn new(kind: &str, look: Look, template: &str) -> Self {
        let seg = Seg::default().set("type", kind).set("template", template);
        match look {
            Look::Plain { fg } => seg.set("style", "plain").set("foreground", fg.reference()),
            // the arrow to the next segment is a Nerd Font glyph
            Look::Powerline { bg, fg } => seg
                .set("style", "powerline")
                .set("powerline_symbol", POWERLINE_SYMBOL)
                .set("background", bg.reference())
                .set("foreground", fg.reference()),
        }
    }
}

impl From<Seg> for Value {
    fn from(seg: Seg) -> Value {
        Value::Object(seg.0)
    }
}

/// A `text` segment: one of our own frame characters, in `swatch`.
fn text(swatch: Swatch, template: &str) -> Value {
    Seg::new("text", Look::Plain { fg: swatch }, template).into()
}

/// Git's icons are Nerd Font glyphs unless they are replaced; only powerline keeps them.
fn git_options(layout: Layout) -> Value {
    match layout {
        Layout::Powerline => json!({ "fetch_status": true }),
        Layout::Box | Layout::Line => json!({
            "branch_icon": "", "commit_icon": "@", "tag_icon": "#", "rebase_icon": "rebase ",
            "cherry_pick_icon": "pick ", "revert_icon": "revert ", "merge_icon": "merge ",
            "no_commits_icon": "", "fetch_status": true
        }),
    }
}

/// The Oh My Posh type of a part, and the Go template of its data.
fn kind_and_body(part: Part) -> (&'static str, String) {
    match part {
        Part::Path => ("path", "{{ .Path }}".to_string()),
        // the branch, then `*` when something changed or `✓` when nothing did (spec §3.5)
        Part::Git => (
            "git",
            [
                "{{ .HEAD }}",
                IF_CHANGED,
                GIT_DIRTY_MARK,
                "{{ else }}",
                GIT_CLEAN_MARK,
                "{{ end }}",
            ]
            .concat(),
        ),
        Part::Time => ("time", "{{ .CurrentDate | date .Format }}".to_string()),
        // only built when the theme lists `user` or `host`, so `(false, _)` is host alone
        Part::Session { user, host } => (
            "session",
            match (user, host) {
                (true, true) => "{{ .UserName }}@{{ .HostName }}",
                (true, false) => "{{ .UserName }}",
                (false, _) => "{{ .HostName }}",
            }
            .to_string(),
        ),
        Part::Status => ("status", format!("{STATUS_MARK}{{{{ .Code }}}}")),
        Part::Duration => ("executiontime", "{{ .FormattedMs }}".to_string()),
    }
}

/// The segment of `part` in `layout`, with the options it needs.
fn segment(layout: Layout, part: Part) -> Value {
    let drawn = drawn(layout, part);
    let (kind, body) = kind_and_body(part);
    let lead = drawn
        .lead
        .map(|(swatch, text)| format!("<{}>{text}</>", swatch.reference()))
        .unwrap_or_default();
    let template = [lead.as_str(), drawn.before, body.as_str(), drawn.after].concat();
    let seg = Seg::new(kind, drawn.look, &template);
    let seg = match part {
        Part::Path => seg.set("options", json!({ "style": "full", "home_icon": "~" })),
        Part::Git => {
            // Oh My Posh uses the first color template that gives a non-empty result, and the
            // plain color when none does. So git is `p:ok` while nothing has changed, and this
            // template makes it `p:dirty` once something has: the text in box and line, the
            // block behind the text in powerline (whose text stays `p:bg`, readable on both).
            let when_changed = [IF_CHANGED, "p:dirty{{ end }}"].concat();
            let dirty = json!([when_changed]);
            let seg = match drawn.look {
                Look::Plain { .. } => seg.set("foreground_templates", dirty),
                Look::Powerline { .. } => seg.set("background_templates", dirty),
            };
            seg.set("options", git_options(layout))
        }
        Part::Time => seg.set("options", json!({ "time_format": TIME_FORMAT })),
        Part::Session { .. } | Part::Status => seg,
        Part::Duration => seg.set(
            "options",
            json!({ "threshold": DURATION_THRESHOLD_MS, "style": "austin" }),
        ),
    };
    seg.into()
}

/// The parts drawn on the left side, one per theme segment, in theme order. `user` and `host`
/// share one `session` segment, placed where the first of them is listed. Time is not drawn in
/// list order: the box layout puts it on the right side, and the other two put it last on the
/// left, whatever its place in the list (spec §3.5).
pub(crate) fn left_parts(theme: &Theme) -> Vec<Part> {
    let list = &theme.prompt.segments;
    let user = list.contains(&Segment::User);
    let host = list.contains(&Segment::Host);
    let mut session_done = false;
    let mut out = Vec::new();
    for seg in list {
        let part = match seg {
            Segment::Time => continue,
            Segment::User | Segment::Host => {
                if session_done {
                    continue;
                }
                session_done = true;
                Part::Session { user, host }
            }
            Segment::Path => Part::Path,
            Segment::Git => Part::Git,
            Segment::Status => Part::Status,
            Segment::Duration => Part::Duration,
        };
        out.push(part);
    }
    if has_time(theme) {
        match theme.prompt.layout {
            Layout::Box => {} // drawn on the right side, by `box_blocks`
            Layout::Line | Layout::Powerline => out.push(Part::Time),
        }
    }
    out
}

/// Whether the theme lists the time.
pub(crate) fn has_time(theme: &Theme) -> bool {
    theme.prompt.segments.contains(&Segment::Time)
}

fn left_segments(theme: &Theme) -> Vec<Value> {
    let layout = theme.prompt.layout;
    left_parts(theme)
        .into_iter()
        .map(|part| segment(layout, part))
        .collect()
}

fn box_blocks(theme: &Theme) -> Vec<Value> {
    let mut left = vec![text(Swatch::Frame, BOX_TOP)];
    left.extend(left_segments(theme));
    // With time, a space sets the closing corner off from it. Without time only the filler
    // comes before the corner, and a space there would show as a gap.
    let right = if has_time(theme) {
        vec![
            segment(Layout::Box, Part::Time),
            text(Swatch::Frame, BOX_CLOSE_AFTER_TIME),
        ]
    } else {
        vec![text(Swatch::Frame, BOX_CLOSE)]
    };
    let filler = format!("<{}>{BOX_FILLER}</>", Swatch::Frame.reference());
    vec![
        json!({ "type": "prompt", "alignment": "left", "segments": left }),
        json!({ "type": "prompt", "alignment": "right", "filler": filler, "segments": right }),
        json!({ "type": "prompt", "alignment": "left", "newline": true, "segments": [text(Swatch::Frame, BOX_SIDE), text(Swatch::Accent, BOX_PROMPT)] }),
        json!({ "type": "rprompt", "segments": [text(Swatch::Frame, BOX_RIGHT)] }),
    ]
}

/// The one-line layouts: the left segments, then the prompt character.
fn single_line(theme: &Theme) -> Vec<Value> {
    let mut list = left_segments(theme);
    list.push(text(Swatch::Accent, prompt_char(theme.prompt.layout)));
    vec![json!({ "type": "prompt", "alignment": "left", "segments": list })]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::{contrast_ratio, Rgb};
    use crate::palette::MIN_TEXT_CONTRAST;
    use crate::presets::terracotta;
    use std::collections::{BTreeSet, HashSet};

    /// Every segment a theme can list, once.
    const ALL: [Segment; 7] = [
        Segment::Path,
        Segment::Git,
        Segment::Time,
        Segment::User,
        Segment::Host,
        Segment::Status,
        Segment::Duration,
    ];

    const LAYOUTS: [Layout; 3] = [Layout::Box, Layout::Line, Layout::Powerline];

    /// The types of block this generator draws (spec §3.5). A block is never of a type in
    /// `ALLOWED_TYPES`, and a segment is never of one of these.
    const BLOCK_TYPES: [&str; 2] = ["prompt", "rprompt"];

    /// The keys of the top level of the config, and no others. Oh My Posh reads more keys than
    /// these, and one of them, `tooltips`, holds segments that it renders when a word is typed.
    /// A key added to the config has to be added here by someone who has looked at what it
    /// holds, so that nothing gets into the config where the guard does not look.
    const TOP_LEVEL_KEYS: [&str; 7] = [
        "$schema",
        "blocks",
        "console_title_template",
        "final_space",
        "palette",
        "transient_prompt",
        "version",
    ];

    /// The only strings a `text` segment may hold: the frame and prompt characters the
    /// generator draws itself (spec §3.5).
    const FRAME_GLYPHS: [&str; 8] = [
        "\u{256d}\u{2500} ",
        " \u{2500}\u{256e}",
        "\u{2500}\u{256e}",
        "\u{2502} ",
        ">",
        "\u{2502}",
        "\u{276f}",
        " \u{276f}",
    ];

    /// Terracotta with this layout and these segments, in this order.
    fn themed(layout: Layout, segments: &[Segment]) -> Theme {
        let mut t = terracotta();
        t.prompt.layout = layout;
        t.prompt.segments = segments.to_vec();
        t
    }

    /// Every subset of the segments in `ALL`, each in the order of `ALL`.
    fn subsets() -> impl Iterator<Item = Vec<Segment>> {
        (0u32..1 << ALL.len()).map(|mask| {
            ALL.iter()
                .enumerate()
                .filter(|(i, _)| mask & (1 << i) != 0)
                .map(|(_, s)| *s)
                .collect()
        })
    }

    /// Every segment found in a `segments` list anywhere under `v`, however deep the list
    /// sits. The tests below that read the template or the options of a segment use it. The
    /// whitelist guard does not: `typed_objects` also sees a segment that is not in a list
    /// named `segments`, and a block.
    fn all_segments(v: &Value, out: &mut Vec<Value>) {
        if let Some(segs) = v.get("segments").and_then(Value::as_array) {
            out.extend(segs.iter().cloned());
        }
        match v {
            Value::Array(a) => a.iter().for_each(|x| all_segments(x, out)),
            Value::Object(o) => o.values().for_each(|x| all_segments(x, out)),
            _ => {}
        }
    }

    /// The first segment of this type anywhere in the config.
    fn segment_of(config: &Value, kind: &str) -> Value {
        let mut all = Vec::new();
        all_segments(config, &mut all);
        all.into_iter().find(|s| s["type"] == kind).unwrap()
    }

    /// What an object of the config stands for: a block, which is an item of the top-level
    /// `blocks`, or a segment, which is anything else that has a type.
    #[derive(Clone, Copy, Debug, PartialEq)]
    enum Role {
        Block,
        Segment,
    }

    /// An object of the config that has a `type`, or that sits where one is needed.
    struct Typed {
        /// Where it is, as a JSON pointer.
        place: String,
        role: Role,
        /// Its `type`; `None` when there is none, or when it is not a string.
        ty: Option<String>,
    }

    /// Every object under `v`, however deep, that has a `type` or is an item of a list where
    /// each item needs one: the top-level `blocks` (its items are blocks) and any list named
    /// `segments` (its items are segments). An object with a type anywhere else is taken for a
    /// segment, so a list that Oh My Posh renders and this generator never writes, such as
    /// `tooltips`, is held to the whitelist as well. `role` is what `v` is, when it is an item of
    /// such a list.
    fn typed_objects(v: &Value, place: &str, role: Option<Role>, out: &mut Vec<Typed>) {
        match v {
            Value::Object(map) => {
                if role.is_some() || map.contains_key("type") {
                    out.push(Typed {
                        place: place.to_string(),
                        role: role.unwrap_or(Role::Segment),
                        ty: map.get("type").and_then(Value::as_str).map(String::from),
                    });
                }
                for (key, child) in map {
                    let items = match key.as_str() {
                        "blocks" if place.is_empty() => Some(Role::Block),
                        "segments" => Some(Role::Segment),
                        _ => None,
                    };
                    let place = format!("{place}/{key}");
                    match child {
                        Value::Array(list) => {
                            for (i, item) in list.iter().enumerate() {
                                typed_objects(item, &format!("{place}/{i}"), items, out);
                            }
                        }
                        other => typed_objects(other, &place, None, out),
                    }
                }
            }
            Value::Array(list) => {
                for (i, item) in list.iter().enumerate() {
                    typed_objects(item, &format!("{place}/{i}"), None, out);
                }
            }
            _ => {}
        }
    }

    /// The types in `config` that are not allowed, each with its place (`command at
    /// /tooltips/0`): a block that is not of a type in `BLOCK_TYPES`, a segment that is not of
    /// one in `ALLOWED_TYPES`, and an object that needs a type and has none (`<missing>`).
    fn outside_the_whitelist(config: &Value) -> Vec<String> {
        let mut found = Vec::new();
        typed_objects(config, "", None, &mut found);
        found
            .iter()
            .filter(|typed| {
                let allowed: &[&str] = match typed.role {
                    Role::Block => &BLOCK_TYPES,
                    Role::Segment => ALLOWED_TYPES,
                };
                !typed.ty.as_deref().is_some_and(|ty| allowed.contains(&ty))
            })
            .map(|typed| {
                let ty = typed.ty.as_deref().unwrap_or("<missing>");
                format!("{ty} at {}", typed.place)
            })
            .collect()
    }

    /// The keys of the top level of `config`.
    fn top_level_keys(config: &Value) -> BTreeSet<String> {
        config.as_object().unwrap().keys().cloned().collect()
    }

    /// The `type` of each segment in a block, in order.
    fn types(block: &Value) -> Vec<&str> {
        block["segments"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["type"].as_str().unwrap())
            .collect()
    }

    /// Every string under `v`.
    fn collect_strings(v: &Value, out: &mut Vec<String>) {
        match v {
            Value::String(s) => out.push(s.clone()),
            Value::Array(a) => a.iter().for_each(|x| collect_strings(x, out)),
            Value::Object(o) => o.values().for_each(|x| collect_strings(x, out)),
            _ => {}
        }
    }

    /// The names after each `p:` in `s`: `p:accent`, `<p:frame>`, `}}p:dirty{{`.
    fn palette_names(s: &str) -> Vec<String> {
        s.split("p:")
            .skip(1)
            .map(|rest| rest.chars().take_while(char::is_ascii_lowercase).collect())
            .collect()
    }

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
        // what is in each block: the left side without time, time on the right
        assert_eq!(types(&blocks[0]), ["text", "path", "git"]);
        assert_eq!(types(&blocks[1]), ["time", "text"]);
        assert_eq!(blocks[1]["segments"][1]["template"], " \u{2500}\u{256e}");
        assert_eq!(types(&blocks[2]), ["text", "text"]);
        assert_eq!(types(&blocks[3]), ["text"]);
    }

    #[test]
    fn never_emits_a_segment_type_outside_the_whitelist() {
        // spec §7 doctrine guard (ข): every layout × every subset of segments, every object
        // of the config that has a type
        for layout in LAYOUTS {
            for segments in subsets() {
                let c = config(&themed(layout, &segments));
                let outside = outside_the_whitelist(&c);
                assert!(outside.is_empty(), "{layout:?} {segments:?}: {outside:?}");
                // the walk reached every segment of every block, so it did not pass by
                // looking at nothing
                let mut found = Vec::new();
                all_segments(&c, &mut found);
                let blocks = c["blocks"].as_array().unwrap();
                let listed: usize = blocks
                    .iter()
                    .map(|b| b["segments"].as_array().unwrap().len())
                    .sum();
                assert!(listed > 0, "{layout:?} {segments:?}");
                assert_eq!(found.len(), listed, "{layout:?} {segments:?}");
                // and the guard sees the blocks and those segments, and no other object with
                // a type, anywhere in the config
                let mut typed = Vec::new();
                typed_objects(&c, "", None, &mut typed);
                let block_count = typed.iter().filter(|t| t.role == Role::Block).count();
                assert_eq!(block_count, blocks.len(), "{layout:?} {segments:?}");
                assert_eq!(
                    typed.len(),
                    blocks.len() + listed,
                    "{layout:?} {segments:?}"
                );
            }
        }
    }

    #[test]
    fn the_top_level_of_the_config_has_exactly_these_keys() {
        // the other half of the guard: a key that nobody has looked at is refused as a key,
        // whatever it holds
        let want: BTreeSet<String> = TOP_LEVEL_KEYS.iter().map(|k| k.to_string()).collect();
        for layout in LAYOUTS {
            for segments in subsets() {
                let keys = top_level_keys(&config(&themed(layout, &segments)));
                assert_eq!(keys, want, "{layout:?} {segments:?}");
            }
        }
        // it can fail: add `tooltips`, or any other key
        for extra in ["tooltips", "anything"] {
            let mut planted = config(&terracotta());
            planted[extra] = json!([]);
            assert_ne!(top_level_keys(&planted), want, "{extra}");
        }
        // and a key that is taken away
        let mut planted = config(&terracotta());
        planted.as_object_mut().unwrap().remove("palette");
        assert_ne!(top_level_keys(&planted), want);
    }

    #[test]
    fn subsets_lists_every_combination_of_all_once() {
        // the guards loop over it, so a range that fell short would leave out the
        // combinations with the last segment in them, and nothing would show it
        let all: Vec<Vec<Segment>> = subsets().collect();
        let distinct: HashSet<&Vec<Segment>> = all.iter().collect();
        assert_eq!(distinct.len(), all.len(), "a combination comes twice");
        assert!(all.contains(&Vec::new()), "the empty list");
        assert!(all.contains(&ALL.to_vec()), "the list of all");
        for segment in ALL {
            let with = all.iter().filter(|s| s.contains(&segment)).count();
            assert_eq!(with * 2, all.len(), "{segment:?} is in half of them");
        }
    }

    #[test]
    fn the_guard_catches_a_forbidden_type_in_any_block() {
        // the guard above is only worth something if it can fail: plant `command` in the
        // last block, and a segment with no type at all
        let planted = json!({
            "blocks": [
                { "type": "prompt", "segments": [{ "type": "path" }] },
                { "type": "rprompt", "segments": [{ "type": "command" }] }
            ]
        });
        assert_eq!(
            outside_the_whitelist(&planted),
            ["command at /blocks/1/segments/0"]
        );
        let untyped =
            json!({ "blocks": [{ "type": "prompt", "segments": [{ "style": "plain" }] }] });
        assert_eq!(
            outside_the_whitelist(&untyped),
            ["<missing> at /blocks/0/segments/0"]
        );
    }

    #[test]
    fn the_guard_catches_a_forbidden_type_wherever_the_config_puts_it() {
        // The first version of the guard read only the lists named `segments`. Oh My Posh also
        // renders the segments of a top-level `tooltips` list, so a `command` in one of them
        // passed. The guard reads every object that has a type, and every item of `blocks`
        // and of `segments`.
        for (what, planted, want) in [
            (
                "a tooltip",
                json!({ "blocks": [], "tooltips": [{ "type": "command", "tips": ["git"] }] }),
                "command at /tooltips/0",
            ),
            (
                "the transient prompt",
                json!({ "blocks": [], "transient_prompt": { "type": "command" } }),
                "command at /transient_prompt",
            ),
            (
                "inside the options of a segment",
                json!({ "blocks": [{ "type": "prompt", "segments": [
                    { "type": "path", "options": { "more": { "type": "command" } } }
                ] }] }),
                "command at /blocks/0/segments/0/options/more",
            ),
            (
                "a list of segments inside a segment",
                json!({ "blocks": [{ "type": "prompt", "segments": [
                    { "type": "path", "segments": [{ "type": "command" }] }
                ] }] }),
                "command at /blocks/0/segments/0/segments/0",
            ),
            (
                "a block of the wrong type",
                json!({ "blocks": [{ "type": "command", "segments": [] }] }),
                "command at /blocks/0",
            ),
            (
                "a block of a segment type",
                json!({ "blocks": [{ "type": "text", "segments": [] }] }),
                "text at /blocks/0",
            ),
            (
                "a segment of a block type",
                json!({ "blocks": [{ "type": "prompt", "segments": [{ "type": "rprompt" }] }] }),
                "rprompt at /blocks/0/segments/0",
            ),
            (
                "a block with no type",
                json!({ "blocks": [{ "segments": [] }] }),
                "<missing> at /blocks/0",
            ),
            (
                "a type that is not a string",
                json!({ "blocks": [{ "type": "prompt", "segments": [{ "type": 5 }] }] }),
                "<missing> at /blocks/0/segments/0",
            ),
        ] {
            assert_eq!(outside_the_whitelist(&planted), [want], "{what}");
        }
        // and the guard lets the shapes of the config through
        let fine = json!({
            "blocks": [
                { "type": "prompt", "segments": [{ "type": "path" }, { "type": "text" }] },
                { "type": "rprompt", "segments": [{ "type": "time" }] }
            ],
            "transient_prompt": { "template": "> " }
        });
        assert!(outside_the_whitelist(&fine).is_empty());
    }

    #[test]
    fn the_whitelist_is_the_spec_list_without_command() {
        // spec §3.5 and §3.7 #2, written out again so that changing the constant fails here
        assert_eq!(
            ALLOWED_TYPES,
            [
                "path",
                "git",
                "time",
                "session",
                "status",
                "executiontime",
                "text"
            ]
        );
        assert!(!ALLOWED_TYPES.contains(&"command"));
    }

    #[test]
    fn text_segments_hold_only_frame_glyphs_never_text_from_the_theme() {
        // spec §3.5: `text` is for the frame characters the generator sets itself
        for layout in LAYOUTS {
            for segments in subsets() {
                let c = config(&themed(layout, &segments));
                let mut all = Vec::new();
                all_segments(&c, &mut all);
                for s in all.iter().filter(|s| s["type"] == "text") {
                    let template = s["template"].as_str().unwrap();
                    assert!(
                        FRAME_GLYPHS.contains(&template),
                        "{layout:?} {segments:?}: {template:?}"
                    );
                }
            }
        }
        // and no text of the theme file gets into the config at all
        let marker = "ZZ-text-from-the-theme-file-ZZ";
        let mut t = terracotta();
        t.name = marker.to_string();
        t.author = marker.to_string();
        t.terminal.font = marker.to_string();
        t.banner.title = marker.to_string();
        t.banner.lines = vec![marker.to_string()];
        t.prompt.segments = ALL.to_vec();
        for layout in LAYOUTS {
            t.prompt.layout = layout;
            assert!(!config_text(&t).unwrap().contains(marker), "{layout:?}");
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
        // line and powerline are left with the prompt character
        for layout in [Layout::Line, Layout::Powerline] {
            let c = config(&themed(layout, &[]));
            assert_eq!(types(&c["blocks"][0]), ["text"], "{layout:?}");
        }
    }

    #[test]
    fn user_and_host_share_one_session_segment() {
        let mut t = terracotta();
        t.prompt.segments = vec![Segment::User, Segment::Path, Segment::Host];
        let c = config(&t);
        let mut all = Vec::new();
        all_segments(&c, &mut all);
        assert_eq!(all.iter().filter(|s| s["type"] == "session").count(), 1);
        assert_eq!(
            c["blocks"][0]["segments"][1]["template"],
            "{{ .UserName }}@{{ .HostName }} "
        );
    }

    #[test]
    fn user_alone_and_host_alone_each_get_their_own_session_template() {
        for (layout, user, host) in [
            (Layout::Box, "{{ .UserName }} ", "{{ .HostName }} "),
            (Layout::Line, "{{ .UserName }} ", "{{ .HostName }} "),
            (Layout::Powerline, " {{ .UserName }} ", " {{ .HostName }} "),
        ] {
            for (segment, want) in [(Segment::User, user), (Segment::Host, host)] {
                let c = config(&themed(layout, &[segment]));
                assert_eq!(
                    segment_of(&c, "session")["template"],
                    want,
                    "{layout:?} {segment:?}"
                );
            }
        }
    }

    #[test]
    fn status_and_duration_use_the_values_of_the_spec_table() {
        // spec §3.5: status shows when the exit code is not 0, which Oh My Posh does unless
        // `always_enabled` is set; executiontime shows past 2 seconds
        let c = config(&themed(Layout::Line, &[Segment::Status, Segment::Duration]));
        let status = segment_of(&c, "status");
        assert_eq!(status["template"], "\u{2717} {{ .Code }} ");
        assert!(status.get("options").is_none());
        let duration = segment_of(&c, "executiontime");
        assert_eq!(duration["template"], "{{ .FormattedMs }} ");
        assert_eq!(duration["options"]["threshold"], 2000);
    }

    #[test]
    fn the_box_closes_with_a_space_before_the_corner_only_when_time_comes_before_it() {
        // with time the line ends `── 16:47 ─╮`, exactly as the prototype draws it; without
        // time the filler runs straight into `─╮` and leaves no gap before the corner
        let c = config(&themed(Layout::Box, &[Segment::Path, Segment::Time]));
        let right = &c["blocks"][1];
        assert_eq!(types(right), ["time", "text"]);
        assert_eq!(
            right["segments"][0]["template"],
            " {{ .CurrentDate | date .Format }}"
        );
        assert_eq!(right["segments"][1]["template"], " \u{2500}\u{256e}");
        let without_time: [&[Segment]; 3] = [
            &[Segment::Path],
            &[Segment::Path, Segment::Git, Segment::Status],
            &[],
        ];
        for list in without_time {
            let c = config(&themed(Layout::Box, list));
            let right = &c["blocks"][1];
            assert_eq!(types(right), ["text"], "{list:?}");
            assert_eq!(
                right["segments"][0]["template"], "\u{2500}\u{256e}",
                "{list:?}"
            );
        }
    }

    #[test]
    fn in_the_box_layout_time_is_on_the_right_side() {
        // the right block holds time and then the closing frame, wherever time is listed
        for list in [
            [Segment::Time, Segment::Path, Segment::Git],
            [Segment::Path, Segment::Time, Segment::Git],
            [Segment::Path, Segment::Git, Segment::Time],
        ] {
            let c = config(&themed(Layout::Box, &list));
            let blocks = c["blocks"].as_array().unwrap();
            assert_eq!(types(&blocks[0]), ["text", "path", "git"], "{list:?}");
            assert_eq!(blocks[1]["alignment"], "right", "{list:?}");
            assert_eq!(types(&blocks[1]), ["time", "text"], "{list:?}");
        }
        // not listed: the right block is the closing frame alone
        let c = config(&themed(Layout::Box, &[Segment::Path]));
        assert_eq!(types(&c["blocks"][1]), ["text"]);
    }

    #[test]
    fn in_line_and_powerline_time_comes_last_on_the_left_wherever_it_is_listed() {
        // spec §3.5: time ends the left side. The last `text` segment of the block is the
        // prompt character the generator draws, not a segment of the theme, so time is the
        // segment right before it, and the whole list of types is pinned.
        let cases: [(&[Segment], &[&str]); 5] = [
            (
                &[Segment::Time, Segment::Path, Segment::Git],
                &["path", "git", "time", "text"],
            ),
            (
                &[Segment::Path, Segment::Time, Segment::Git],
                &["path", "git", "time", "text"],
            ),
            (
                &[Segment::Path, Segment::Git, Segment::Time],
                &["path", "git", "time", "text"],
            ),
            (&[Segment::Time], &["time", "text"]),
            // the others keep their order, and user and host stay one session where the
            // first of them was listed
            (
                &[
                    Segment::Time,
                    Segment::Status,
                    Segment::User,
                    Segment::Path,
                    Segment::Duration,
                    Segment::Host,
                    Segment::Git,
                ],
                &[
                    "status",
                    "session",
                    "path",
                    "executiontime",
                    "git",
                    "time",
                    "text",
                ],
            ),
        ];
        for layout in [Layout::Line, Layout::Powerline] {
            for (list, want) in cases {
                let c = config(&themed(layout, list));
                let blocks = c["blocks"].as_array().unwrap();
                assert_eq!(blocks.len(), 1, "{layout:?}: one block, no right side");
                assert_eq!(types(&blocks[0]), want, "{layout:?} {list:?}");
                // the text segment after time is the prompt character
                let last = blocks[0]["segments"].as_array().unwrap().last().unwrap();
                assert_eq!(last["template"].as_str().unwrap().trim(), "\u{276f}");
            }
            // not listed: no time anywhere
            let c = config(&themed(layout, &[Segment::Path, Segment::Git]));
            assert_eq!(types(&c["blocks"][0]), ["path", "git", "text"]);
        }
    }

    #[test]
    fn git_shows_the_branch_then_a_star_when_dirty_or_a_check_mark_when_clean() {
        // spec §3.5, in every layout. Only the real Oh My Posh renders these templates
        // (the Phase 1b parity tests), so the text is pinned whole.
        let state =
            "{{ .HEAD }}{{ if or .Working.Changed .Staging.Changed }} *{{ else }} \u{2713}{{ end }}";
        for (layout, want) in [
            (Layout::Box, format!("<p:muted>\u{b7} </>{state} ")),
            (Layout::Line, format!("{state} ")),
            (Layout::Powerline, format!(" {state} ")),
        ] {
            let c = config(&themed(layout, &[Segment::Git]));
            let git = segment_of(&c, "git");
            assert_eq!(git["template"], want.as_str(), "{layout:?}");
            // without `fetch_status` Oh My Posh never reads the state, and `*` never shows
            assert_eq!(git["options"]["fetch_status"], true, "{layout:?}");
        }
    }

    #[test]
    fn git_is_ok_colored_while_clean_and_dirty_colored_while_changed_in_every_layout() {
        // Oh My Posh takes the first color template that is not empty, and the plain color
        // otherwise: the plain color is `p:ok`, and the template gives `p:dirty` when changed
        let when_changed = json!(["{{ if or .Working.Changed .Staging.Changed }}p:dirty{{ end }}"]);
        for layout in LAYOUTS {
            let c = config(&themed(layout, &[Segment::Git]));
            let git = segment_of(&c, "git");
            match layout {
                // the text is colored
                Layout::Box | Layout::Line => {
                    assert_eq!(git["foreground"], "p:ok", "{layout:?}");
                    assert_eq!(git["foreground_templates"], when_changed, "{layout:?}");
                    assert!(git.get("background").is_none(), "{layout:?}");
                    assert!(git.get("background_templates").is_none(), "{layout:?}");
                }
                // the block behind the text is colored, and the text stays `p:bg`
                Layout::Powerline => {
                    assert_eq!(git["background"], "p:ok");
                    assert_eq!(git["background_templates"], when_changed);
                    assert_eq!(git["foreground"], "p:bg");
                    assert!(git.get("foreground_templates").is_none());
                    // `p:bg` is readable text on both of the backgrounds
                    let color =
                        |name: &str| c["palette"][name].as_str().unwrap().parse::<Rgb>().unwrap();
                    for background in ["ok", "dirty"] {
                        let ratio = contrast_ratio(color("bg"), color(background));
                        assert!(
                            ratio >= MIN_TEXT_CONTRAST,
                            "p:bg on p:{background}: {ratio}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn powerline_segments_use_the_powerline_symbol() {
        let c = config(&themed(Layout::Powerline, &ALL));
        let mut all = Vec::new();
        all_segments(&c, &mut all);
        let (frame, data): (Vec<_>, Vec<_>) = all.iter().partition(|s| s["type"] == "text");
        // path, git, time, session (user and host share one), status and executiontime
        assert_eq!(data.len(), 6);
        for s in data {
            assert_eq!(s["style"], "powerline", "{s}");
            assert_eq!(s["powerline_symbol"], "\u{e0b0}", "{s}");
            assert!(s["background"].is_string(), "{s}");
        }
        // the prompt character is plain text
        for s in frame {
            assert_eq!(s["style"], "plain", "{s}");
        }
    }

    #[test]
    fn nerd_font_glyphs_and_icons_appear_only_in_the_powerline_layout() {
        // spec §3.5: outside powerline, git's icons are plain text and no glyph needs a Nerd Font
        let git_icons = [
            "branch_icon",
            "commit_icon",
            "tag_icon",
            "rebase_icon",
            "cherry_pick_icon",
            "revert_icon",
            "merge_icon",
            "no_commits_icon",
        ];
        for layout in LAYOUTS {
            let c = config(&themed(layout, &ALL));
            let git = segment_of(&c, "git");
            // the three private use areas, where Nerd Font keeps its icons
            let private_use = serde_json::to_string(&c).unwrap().chars().any(|ch| {
                matches!(ch, '\u{e000}'..='\u{f8ff}' | '\u{f0000}'..='\u{ffffd}' | '\u{100000}'..='\u{10fffd}')
            });
            if layout == Layout::Powerline {
                assert!(
                    private_use,
                    "the arrow between segments is a Nerd Font glyph"
                );
                assert!(git_icons.iter().all(|k| git["options"].get(*k).is_none()));
            } else {
                assert!(!private_use, "{layout:?}");
                for key in git_icons {
                    let icon = git["options"][key].as_str().unwrap();
                    assert!(icon.is_ascii(), "{layout:?} {key}: {icon:?}");
                }
            }
        }
    }

    #[test]
    fn every_palette_reference_names_a_color_in_the_palette() {
        // `p:name` is used for colors, inside templates and in the filler; a typo in one of
        // them would paint nothing, and only a real Oh My Posh would show it
        for layout in LAYOUTS {
            let c = config(&themed(layout, &ALL));
            let palette = c["palette"].as_object().unwrap();
            let mut strings = Vec::new();
            collect_strings(&c["blocks"], &mut strings);
            collect_strings(&c["transient_prompt"], &mut strings);
            let names: Vec<String> = strings.iter().flat_map(|s| palette_names(s)).collect();
            assert!(!names.is_empty(), "{layout:?}");
            for name in names {
                assert!(palette.contains_key(&name), "{layout:?}: p:{name}");
            }
        }
        // the scan finds a name as a color, inside `<...>` and straight after `}}`
        assert_eq!(palette_names("p:accent"), ["accent"]);
        assert_eq!(palette_names("<p:frame>\u{2500}</>"), ["frame"]);
        assert_eq!(palette_names("{{ if x }}p:dirty{{ end }}"), ["dirty"]);
        assert!(palette_names("{{ .Path }} ").is_empty());
    }

    #[test]
    fn config_text_is_the_config_pretty_printed_with_one_final_newline() {
        let t = terracotta();
        let text = config_text(&t).unwrap();
        assert_eq!(serde_json::from_str::<Value>(&text).unwrap(), config(&t));
        assert!(text.starts_with("{\n  \""), "{text}");
        assert!(text.ends_with("}\n") && !text.ends_with("\n\n"));
    }

    // ---- the parts the preview shares (crate::preview draws the same prompt) ----

    /// Every part, `Session` with both names.
    const PARTS: [Part; 6] = [
        Part::Path,
        Part::Git,
        Part::Time,
        Part::Session {
            user: true,
            host: true,
        },
        Part::Status,
        Part::Duration,
    ];

    #[test]
    fn every_swatch_is_in_the_palette_with_its_theme_color() {
        let t = terracotta();
        let c = config(&t);
        let palette = c["palette"].as_object().unwrap();
        assert_eq!(palette.len(), Swatch::ALL.len());
        for swatch in Swatch::ALL {
            assert_eq!(
                palette[swatch.name()],
                swatch.color(&t.colors).to_string(),
                "{swatch:?}"
            );
        }
        // the four ansi colors the prompt uses: red, green, yellow and blue
        assert_eq!(Swatch::Red.color(&t.colors), t.colors.ansi[1]);
        assert_eq!(Swatch::Ok.color(&t.colors), t.colors.ansi[2]);
        assert_eq!(Swatch::Dirty.color(&t.colors), t.colors.ansi[3]);
        assert_eq!(Swatch::Blue.color(&t.colors), t.colors.ansi[4]);
    }

    #[test]
    fn the_looks_are_the_ones_spec_3_5_was_built_with() {
        use Swatch as S;
        let plain = |fg| Look::Plain { fg };
        let block = |bg, fg| Look::Powerline { bg, fg };
        let session = Part::Session {
            user: true,
            host: false,
        };
        for (layout, part, look) in [
            (Layout::Box, Part::Path, plain(S::Muted)),
            (Layout::Box, Part::Git, plain(S::Ok)),
            (Layout::Box, Part::Time, plain(S::Muted)),
            (Layout::Box, session, plain(S::Fg)),
            (Layout::Box, Part::Status, plain(S::Red)),
            (Layout::Box, Part::Duration, plain(S::Dirty)),
            (Layout::Line, Part::Path, plain(S::Blue)),
            (Layout::Line, Part::Git, plain(S::Ok)),
            (Layout::Line, Part::Time, plain(S::Muted)),
            (Layout::Line, session, plain(S::Fg)),
            (Layout::Line, Part::Status, plain(S::Red)),
            (Layout::Line, Part::Duration, plain(S::Dirty)),
            (Layout::Powerline, Part::Path, block(S::Frame, S::Fg)),
            (Layout::Powerline, Part::Git, block(S::Ok, S::Bg)),
            (Layout::Powerline, Part::Time, block(S::Frame, S::Fg)),
            (Layout::Powerline, session, block(S::Accent, S::Bg)),
            (Layout::Powerline, Part::Status, block(S::Red, S::Bg)),
            (Layout::Powerline, Part::Duration, block(S::Dirty, S::Bg)),
        ] {
            assert_eq!(drawn(layout, part).look, look, "{layout:?} {part:?}");
        }
        // only git in the box has a lead, and only the box's time has its space before
        assert_eq!(
            drawn(Layout::Box, Part::Git).lead,
            Some((S::Muted, GIT_SEPARATOR))
        );
        assert_eq!(
            (
                drawn(Layout::Box, Part::Time).before,
                drawn(Layout::Box, Part::Time).after
            ),
            (" ", "")
        );
    }

    #[test]
    fn every_segment_is_drawn_as_the_shared_table_says() {
        for layout in LAYOUTS {
            for part in PARTS {
                let d = drawn(layout, part);
                let seg = segment(layout, part);
                let template = seg["template"].as_str().unwrap();
                let lead = d
                    .lead
                    .map(|(swatch, text)| format!("<p:{}>{text}</>", swatch.name()))
                    .unwrap_or_default();
                let why = format!("{layout:?} {part:?}: {template:?}");
                assert!(
                    template.starts_with(&format!("{lead}{}", d.before)),
                    "{why}"
                );
                assert!(template.ends_with(d.after), "{why}");
                match d.look {
                    Look::Plain { fg } => {
                        assert_eq!(seg["style"], "plain", "{why}");
                        assert_eq!(seg["foreground"], format!("p:{}", fg.name()), "{why}");
                        assert!(seg.get("background").is_none(), "{why}");
                    }
                    Look::Powerline { bg, fg } => {
                        assert_eq!(seg["style"], "powerline", "{why}");
                        assert_eq!(seg["background"], format!("p:{}", bg.name()), "{why}");
                        assert_eq!(seg["foreground"], format!("p:{}", fg.name()), "{why}");
                    }
                }
            }
        }
    }

    #[test]
    fn left_parts_follow_the_list_with_one_session_and_time_last() {
        let list = [
            Segment::Time,
            Segment::Host,
            Segment::Path,
            Segment::User,
            Segment::Git,
        ];
        let session = Part::Session {
            user: true,
            host: true,
        };
        assert_eq!(
            left_parts(&themed(Layout::Line, &list)),
            [session, Part::Path, Part::Git, Part::Time]
        );
        // the box keeps time for its right side
        assert_eq!(
            left_parts(&themed(Layout::Box, &list)),
            [session, Part::Path, Part::Git]
        );
        assert!(has_time(&themed(Layout::Box, &list)));
        assert!(left_parts(&themed(Layout::Powerline, &[])).is_empty());
    }

    #[test]
    fn look_changed_is_the_color_the_git_template_switches() {
        // the preview colors a changed repository with `Look::changed`; Oh My Posh with the
        // color template the config gives git
        for layout in LAYOUTS {
            let c = config(&themed(layout, &[Segment::Git]));
            let git = segment_of(&c, "git");
            let look = drawn(layout, Part::Git).look;
            match (look, look.changed()) {
                (Look::Plain { .. }, Look::Plain { fg }) => {
                    assert_eq!(fg, Swatch::Dirty);
                    let t = git["foreground_templates"][0].as_str().unwrap();
                    assert!(t.ends_with("p:dirty{{ end }}"), "{t}");
                }
                (Look::Powerline { fg: before, .. }, Look::Powerline { bg, fg }) => {
                    assert_eq!((bg, fg), (Swatch::Dirty, before));
                    let t = git["background_templates"][0].as_str().unwrap();
                    assert!(t.ends_with("p:dirty{{ end }}"), "{t}");
                }
                other => panic!("{layout:?}: {other:?}"),
            }
        }
    }
}
