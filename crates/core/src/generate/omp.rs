//! Theme → Oh My Posh config, format version 4 (spec §3.5). Built only from
//! whitelisted segment types; `text` carries only our own frame glyphs.
use crate::model::{Layout, Segment, Theme};
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

/// The Oh My Posh config for `theme`, format version 4. It reads the theme's colors, layout
/// and segment list and nothing else, so no text from the theme file ends up in it. The
/// transient prompt is always described; whether Clink uses it is Clink's own
/// `prompt.transient` setting.
pub fn config(theme: &Theme) -> Value {
    let blocks = match theme.prompt.layout {
        Layout::Box => box_blocks(theme),
        Layout::Line => single_line(theme, "\u{276f}"),
        Layout::Powerline => single_line(theme, " \u{276f}"),
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

/// [`config`] as the text of the file: pretty-printed, with one final newline.
pub fn config_text(theme: &Theme) -> Result<String, serde_json::Error> {
    let mut text = serde_json::to_string_pretty(&config(theme))?;
    text.push('\n');
    Ok(text)
}

fn palette(theme: &Theme) -> Value {
    let c = &theme.colors;
    // ansi 1 to 4 are red, green, yellow and blue; destructuring cannot index out of range
    let [_, red, green, yellow, blue, ..] = c.ansi;
    json!({
        "bg": c.background.to_string(),
        "fg": c.foreground.to_string(),
        "accent": c.accent.to_string(),
        "muted": c.muted.to_string(),
        "frame": c.frame.to_string(),
        "red": red.to_string(),
        "ok": green.to_string(),
        "dirty": yellow.to_string(),
        "blue": blue.to_string()
    })
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

    /// Text in `fg`, with nothing behind it.
    fn plain(kind: &str, fg: &str, template: &str) -> Self {
        Seg::default()
            .set("type", kind)
            .set("style", "plain")
            .set("foreground", fg)
            .set("template", template)
    }

    /// Text in `fg` on a block of `bg`, joined to the next segment by an arrow (a Nerd Font
    /// glyph).
    fn powerline(kind: &str, bg: &str, fg: &str, template: &str) -> Self {
        Seg::default()
            .set("type", kind)
            .set("style", "powerline")
            .set("powerline_symbol", "\u{e0b0}")
            .set("background", bg)
            .set("foreground", fg)
            .set("template", template)
    }
}

impl From<Seg> for Value {
    fn from(seg: Seg) -> Value {
        Value::Object(seg.0)
    }
}

fn text(fg: &str, template: &str) -> Value {
    Seg::plain("text", fg, template).into()
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

fn path(layout: Layout) -> Value {
    let seg = match layout {
        Layout::Box => Seg::plain("path", "p:muted", "{{ .Path }} "),
        Layout::Line => Seg::plain("path", "p:blue", "{{ .Path }} "),
        Layout::Powerline => Seg::powerline("path", "p:frame", "p:fg", " {{ .Path }} "),
    };
    seg.set("options", json!({ "style": "full", "home_icon": "~" }))
        .into()
}

fn git(layout: Layout) -> Value {
    // The branch, then `*` when something changed or `✓` when nothing did (spec §3.5).
    let state = ["{{ .HEAD }}", IF_CHANGED, " *{{ else }} \u{2713}{{ end }}"].concat();
    // Oh My Posh uses the first color template that gives a non-empty result, and the plain
    // color when none does. So the segment is `p:ok` while nothing has changed, and this
    // template makes it `p:dirty` once something has.
    let dirty_color = [IF_CHANGED, "p:dirty{{ end }}"].concat();
    let seg = match layout {
        Layout::Box => Seg::plain("git", "p:ok", &format!("<p:muted>\u{b7} </>{state} "))
            .set("foreground_templates", json!([dirty_color])),
        Layout::Line => Seg::plain("git", "p:ok", &format!("{state} "))
            .set("foreground_templates", json!([dirty_color])),
        // Here the block behind the text changes color. The text stays `p:bg`, which reads
        // on `p:ok` and on `p:dirty` alike.
        Layout::Powerline => Seg::powerline("git", "p:ok", "p:bg", &format!(" {state} "))
            .set("background_templates", json!([dirty_color])),
    };
    seg.set("options", git_options(layout)).into()
}

fn time(layout: Layout) -> Value {
    let seg = match layout {
        Layout::Box => Seg::plain("time", "p:muted", " {{ .CurrentDate | date .Format }}"),
        Layout::Line => Seg::plain("time", "p:muted", "{{ .CurrentDate | date .Format }} "),
        Layout::Powerline => Seg::powerline(
            "time",
            "p:frame",
            "p:fg",
            " {{ .CurrentDate | date .Format }} ",
        ),
    };
    seg.set("options", json!({ "time_format": TIME_FORMAT }))
        .into()
}

fn session(layout: Layout, user: bool, host: bool) -> Value {
    // Only called when the theme lists `user` or `host`, so `(false, _)` is host alone.
    let body = match (user, host) {
        (true, true) => "{{ .UserName }}@{{ .HostName }}",
        (true, false) => "{{ .UserName }}",
        (false, _) => "{{ .HostName }}",
    };
    match layout {
        Layout::Powerline => Seg::powerline("session", "p:accent", "p:bg", &format!(" {body} ")),
        Layout::Box | Layout::Line => Seg::plain("session", "p:fg", &format!("{body} ")),
    }
    .into()
}

fn status(layout: Layout) -> Value {
    match layout {
        Layout::Powerline => Seg::powerline("status", "p:red", "p:bg", " \u{2717} {{ .Code }} "),
        Layout::Box | Layout::Line => Seg::plain("status", "p:red", "\u{2717} {{ .Code }} "),
    }
    .into()
}

fn duration(layout: Layout) -> Value {
    let seg = match layout {
        Layout::Powerline => {
            Seg::powerline("executiontime", "p:dirty", "p:bg", " {{ .FormattedMs }} ")
        }
        Layout::Box | Layout::Line => Seg::plain("executiontime", "p:dirty", "{{ .FormattedMs }} "),
    };
    seg.set("options", json!({ "threshold": 2000, "style": "austin" }))
        .into()
}

/// The segments drawn on the left side, one per theme segment, in theme order. `user` and
/// `host` share one `session` segment, placed where the first of them is listed. Time is not
/// drawn in list order: the box layout puts it on the right side, and the other two put it
/// last on the left, whatever its place in the list (spec §3.5).
fn left_segments(theme: &Theme) -> Vec<Value> {
    let layout = theme.prompt.layout;
    let list = &theme.prompt.segments;
    let user = list.contains(&Segment::User);
    let host = list.contains(&Segment::Host);
    let mut session_done = false;
    let mut out = Vec::new();
    for seg in list {
        let v = match seg {
            Segment::Time => continue,
            Segment::User | Segment::Host => {
                if session_done {
                    continue;
                }
                session_done = true;
                session(layout, user, host)
            }
            Segment::Path => path(layout),
            Segment::Git => git(layout),
            Segment::Status => status(layout),
            Segment::Duration => duration(layout),
        };
        out.push(v);
    }
    if has_time(theme) {
        match layout {
            Layout::Box => {} // drawn on the right side, by `box_blocks`
            Layout::Line | Layout::Powerline => out.push(time(layout)),
        }
    }
    out
}

fn has_time(theme: &Theme) -> bool {
    theme.prompt.segments.contains(&Segment::Time)
}

fn box_blocks(theme: &Theme) -> Vec<Value> {
    let mut left = vec![text("p:frame", "\u{256d}\u{2500} ")];
    left.extend(left_segments(theme));
    // With time, a space sets the closing corner off from it. Without time only the filler
    // comes before the corner, and a space there would show as a gap.
    let right = if has_time(theme) {
        vec![time(Layout::Box), text("p:frame", " \u{2500}\u{256e}")]
    } else {
        vec![text("p:frame", "\u{2500}\u{256e}")]
    };
    vec![
        json!({ "type": "prompt", "alignment": "left", "segments": left }),
        json!({ "type": "prompt", "alignment": "right", "filler": "<p:frame>\u{2500}</>", "segments": right }),
        json!({ "type": "prompt", "alignment": "left", "newline": true, "segments": [text("p:frame", "\u{2502} "), text("p:accent", ">")] }),
        json!({ "type": "rprompt", "segments": [text("p:frame", "\u{2502}")] }),
    ]
}

/// The one-line layouts: the left segments, then the prompt character.
fn single_line(theme: &Theme, prompt_char: &str) -> Vec<Value> {
    let mut list = left_segments(theme);
    list.push(text("p:accent", prompt_char));
    vec![json!({ "type": "prompt", "alignment": "left", "segments": list })]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::{contrast_ratio, Rgb};
    use crate::palette::MIN_TEXT_CONTRAST;
    use crate::presets::terracotta;

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

    /// Every subset of the seven segments, each in the order of `ALL`.
    fn subsets() -> impl Iterator<Item = Vec<Segment>> {
        (0u8..128).map(|mask| {
            ALL.iter()
                .enumerate()
                .filter(|(i, _)| mask & (1 << i) != 0)
                .map(|(_, s)| *s)
                .collect()
        })
    }

    /// Every segment found anywhere under `v`: the items of each `segments` list, however
    /// deep the list sits. The guard walks the whole config this way and not only the
    /// blocks we expect, so a segment added anywhere is seen.
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

    /// The segment types in `config` that are not on the whitelist.
    fn outside_the_whitelist(config: &Value) -> Vec<String> {
        let mut all = Vec::new();
        all_segments(config, &mut all);
        all.iter()
            .map(|s| s["type"].as_str().unwrap_or("<missing>").to_string())
            .filter(|ty| !ALLOWED_TYPES.contains(&ty.as_str()))
            .collect()
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
        // spec §7 doctrine guard (ข): every layout × every subset of segments, every block
        for layout in LAYOUTS {
            for segments in subsets() {
                let c = config(&themed(layout, &segments));
                let outside = outside_the_whitelist(&c);
                assert!(outside.is_empty(), "{layout:?} {segments:?}: {outside:?}");
                // the walk reached every segment of every block, so it did not pass by
                // looking at nothing
                let mut found = Vec::new();
                all_segments(&c, &mut found);
                let listed: usize = c["blocks"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|b| b["segments"].as_array().unwrap().len())
                    .sum();
                assert!(listed > 0, "{layout:?} {segments:?}");
                assert_eq!(found.len(), listed, "{layout:?} {segments:?}");
            }
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
        assert_eq!(outside_the_whitelist(&planted), ["command"]);
        let untyped = json!({ "blocks": [{ "segments": [{ "style": "plain" }] }] });
        assert_eq!(outside_the_whitelist(&untyped), ["<missing>"]);
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
}
