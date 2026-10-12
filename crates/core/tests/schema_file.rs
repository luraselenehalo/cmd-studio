//! The published JSON Schema (spec §3.3). `schema/cmdtheme-1.schema.json` is generated from the
//! Rust types and never written by hand. The first test keeps that file equal to what the types
//! generate. The others read the generated schema back and hold it to what the spec says the
//! format is, so a change to a limit, a field or the strictness of the format cannot pass as a
//! change to the generated file alone.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use cmdstudio_core::model::{limits, SCHEMA_VERSION};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::PathBuf;

fn schema_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../schema/cmdtheme-1.schema.json")
}

fn schema_text() -> String {
    cmdstudio_core::schema::theme_schema_json().unwrap()
}

fn schema() -> Value {
    serde_json::from_str(&schema_text()).unwrap()
}

#[test]
fn schema_file_matches_the_rust_types() {
    let generated = schema_text();
    let path = schema_path();
    // A plain run only reads the committed file. It is rewritten when UPDATE_SCHEMA is set, and
    // then only to hold exactly what the types generate.
    if std::env::var_os("UPDATE_SCHEMA").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &generated).unwrap();
    }
    let on_disk =
        std::fs::read_to_string(&path).expect("missing schema file: run with UPDATE_SCHEMA=1 once");
    assert_eq!(
        on_disk.replace("\r\n", "\n"),
        generated,
        "schema file is stale: rerun with UPDATE_SCHEMA=1 and commit it"
    );
}

#[test]
fn serde_json_keeps_its_keys_sorted() {
    // `schemars` writes the properties of the schema in the order of serde_json's maps. That is
    // alphabetical until some dependency turns serde_json's `preserve_order` feature on for the
    // whole build. Then the properties come out in the order the fields are declared, the file
    // above no longer matches, and rerunning with UPDATE_SCHEMA=1 would only move the problem
    // to the builds that do not have the feature. This test says what is wrong before that.
    let mut keys = serde_json::Map::new();
    keys.insert("b".to_string(), Value::Null);
    keys.insert("a".to_string(), Value::Null);
    assert_eq!(
        keys.keys().next().map(String::as_str),
        Some("a"),
        "serde_json preserve_order is on: the schema file's key order would change. Rerunning \
         with UPDATE_SCHEMA=1 does not fix that; find what turns the feature on, or sort the keys \
         in schema::theme_schema_json"
    );
}

#[test]
fn schema_is_strict_and_requires_the_version() {
    let schema = schema();
    assert_eq!(schema["additionalProperties"], json!(false));
    assert!(schema["required"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v == "schemaVersion"));
}

// ---- the contract of spec §3.3, read back out of the generated schema ----

/// The JSON at `pointer` (RFC 6901, e.g. `/properties/name`), or a panic that says where it is
/// missing. Plain `schema["a"]["b"]` would answer `null` and hide where the path went wrong.
fn at<'a>(schema: &'a Value, pointer: &str) -> &'a Value {
    schema
        .pointer(pointer)
        .unwrap_or_else(|| panic!("the schema says nothing at {pointer}"))
}

fn text_at<'a>(schema: &'a Value, pointer: &str) -> &'a str {
    at(schema, pointer)
        .as_str()
        .unwrap_or_else(|| panic!("{pointer} is not a string"))
}

/// A length, a count or a bound of a whole-number field. The schema has to write it as `64`,
/// not `64.0`, and `as_u64` refuses the latter.
fn whole(schema: &Value, pointer: &str) -> u64 {
    at(schema, pointer)
        .as_u64()
        .unwrap_or_else(|| panic!("{pointer} is not a whole number"))
}

fn real(schema: &Value, pointer: &str) -> f64 {
    at(schema, pointer)
        .as_f64()
        .unwrap_or_else(|| panic!("{pointer} is not a number"))
}

/// A limit of the code, of whatever integer type, as the whole number the schema states.
fn wide<T: TryInto<u64>>(limit: T) -> u64 {
    limit.try_into().ok().unwrap()
}

fn words(list: &[&str]) -> BTreeSet<String> {
    list.iter().map(|word| String::from(*word)).collect()
}

fn strings_at(schema: &Value, pointer: &str) -> BTreeSet<String> {
    at(schema, pointer)
        .as_array()
        .unwrap_or_else(|| panic!("{pointer} is not a list"))
        .iter()
        .map(|item| String::from(item.as_str().unwrap()))
        .collect()
}

fn keys_at(schema: &Value, pointer: &str) -> BTreeSet<String> {
    at(schema, pointer)
        .as_object()
        .unwrap_or_else(|| panic!("{pointer} is not an object"))
        .keys()
        .cloned()
        .collect()
}

/// Every schema in `node`, at any depth, that describes an object, with its place as a pointer
/// (the root is the empty one). It looks at every JSON object in the file, so a type that is
/// nested inside another one is found as well.
fn object_types<'a>(node: &'a Value, place: &str, found: &mut Vec<(String, &'a Value)>) {
    match node {
        Value::Object(map) => {
            let describes_an_object = match map.get("type") {
                Some(Value::String(kind)) => kind == "object",
                Some(Value::Array(kinds)) => kinds.iter().any(|kind| kind == "object"),
                _ => false,
            };
            if describes_an_object || map.contains_key("properties") {
                found.push((place.to_string(), node));
            }
            for (key, child) in map {
                object_types(child, &format!("{place}/{key}"), found);
            }
        }
        Value::Array(items) => {
            for (i, child) in items.iter().enumerate() {
                object_types(child, &format!("{place}/{i}"), found);
            }
        }
        _ => {}
    }
}

/// The text of every `keyword` in `node`, at any depth: the targets of `$ref`, say, or the names
/// of the `format`s. A property that is called like the keyword has a schema for its value, not
/// a string, so it is walked into and not taken for the keyword.
fn string_values<'a>(node: &'a Value, keyword: &str, found: &mut Vec<&'a str>) {
    match node {
        Value::Object(map) => {
            for (key, child) in map {
                match child {
                    Value::String(text) if key.as_str() == keyword => found.push(text),
                    _ => string_values(child, keyword, found),
                }
            }
        }
        Value::Array(items) => {
            for child in items {
                string_values(child, keyword, found);
            }
        }
        _ => {}
    }
}

#[test]
fn the_text_is_a_pretty_printed_draft_2020_12_schema_with_one_final_newline() {
    let text = schema_text();
    assert!(
        text.starts_with("{\n  \""),
        "pretty-printed: two spaces, one key per line"
    );
    assert!(
        text.ends_with("}\n"),
        "one newline, after the closing brace"
    );
    assert!(
        !text.contains('\r'),
        "LF only, which is how .gitattributes keeps the file"
    );
    // The dialect the keywords are read in (`$defs`, `uniqueItems`, `$ref` next to siblings).
    // This is the schema's own `$schema`; the theme's optional `$schema` field is another thing.
    assert_eq!(
        text_at(&schema(), "/$schema"),
        "https://json-schema.org/draft/2020-12/schema"
    );
}

#[test]
fn every_object_type_refuses_unknown_properties() {
    let schema = schema();
    let mut objects = Vec::new();
    object_types(&schema, "", &mut objects);
    // The check below must not pass for lack of anything to check: the file itself and the five
    // parts it is made of are all objects.
    let places: BTreeSet<&str> = objects.iter().map(|(place, _)| place.as_str()).collect();
    for part in [
        "",
        "/$defs/SimpleRecipe",
        "/$defs/Colors",
        "/$defs/TerminalSettings",
        "/$defs/PromptSettings",
        "/$defs/BannerSettings",
    ] {
        assert!(places.contains(part), "no object type found at {part:?}");
    }
    for (place, object) in objects {
        assert_eq!(
            object.get("additionalProperties"),
            Some(&json!(false)),
            "the object type at {place:?} accepts properties the format does not define"
        );
    }
}

#[test]
fn the_schema_names_exactly_the_fields_of_the_format() {
    // The fields of each part, as the spec's example lists them. A part has no optional field:
    // a file that leaves one out is refused, so the fields it must have are all of them.
    const RECIPE: &[&str] = &["accent", "tint"];
    const COLORS: &[&str] = &[
        "accent",
        "ansi",
        "background",
        "cursor",
        "foreground",
        "frame",
        "muted",
        "selection",
    ];
    const TERMINAL: &[&str] = &[
        "acrylic",
        "cursorShape",
        "font",
        "fontSize",
        "opacity",
        "padding",
    ];
    const PROMPT: &[&str] = &["layout", "segments", "transient"];
    const BANNER: &[&str] = &["clearLogo", "enabled", "lines", "tips", "title"];
    let schema = schema();
    // (the object, all its fields, the fields a file must have.) The top level has three that
    // may be left out: `author` (which defaults to empty), `simple` (which may be null) and the
    // optional `$schema` line.
    let objects: [(&str, &[&str], &[&str]); 6] = [
        (
            "",
            &[
                "$schema",
                "author",
                "banner",
                "colors",
                "id",
                "name",
                "prompt",
                "schemaVersion",
                "simple",
                "terminal",
                "tone",
            ],
            &[
                "banner",
                "colors",
                "id",
                "name",
                "prompt",
                "schemaVersion",
                "terminal",
                "tone",
            ],
        ),
        ("/$defs/SimpleRecipe", RECIPE, RECIPE),
        ("/$defs/Colors", COLORS, COLORS),
        ("/$defs/TerminalSettings", TERMINAL, TERMINAL),
        ("/$defs/PromptSettings", PROMPT, PROMPT),
        ("/$defs/BannerSettings", BANNER, BANNER),
    ];
    for (object, fields, required) in objects {
        assert_eq!(
            keys_at(&schema, &format!("{object}/properties")),
            words(fields),
            "the fields of the object at {object:?}"
        );
        assert_eq!(
            strings_at(&schema, &format!("{object}/required")),
            words(required),
            "the fields that the object at {object:?} must have"
        );
    }
    // `$schema` is a string, nothing more: its text is dropped unread.
    assert_eq!(
        text_at(&schema, "/properties/$schema/type"),
        "string",
        "the optional $schema line"
    );
    // `simple` is the recipe or null.
    let simple = at(&schema, "/properties/simple/anyOf").as_array().unwrap();
    assert_eq!(simple.len(), 2, "{simple:?}");
    assert!(
        simple.contains(&json!({ "$ref": "#/$defs/SimpleRecipe" })),
        "simple may be a recipe: {simple:?}"
    );
    assert!(
        simple.contains(&json!({ "type": "null" })),
        "simple may be null: {simple:?}"
    );
}

#[test]
fn the_schema_states_every_limit_of_the_format() {
    let schema = schema();
    // (the field in the schema, the keyword that limits it, what `model::limits` says, and what
    // spec §3.3 says.) The spec's number is written out on purpose. The schema is generated from
    // `limits`, so it cannot differ from it; the literal is what makes a changed constant show
    // up here, because a limit is part of a public contract: changing one means changing the
    // spec, this table and the schema file together.
    let from_the_code = [
        (
            "/properties/schemaVersion",
            "minimum",
            wide(SCHEMA_VERSION),
            1,
        ),
        (
            "/properties/schemaVersion",
            "maximum",
            wide(SCHEMA_VERSION),
            1,
        ),
        (
            "/properties/name",
            "minLength",
            wide(limits::NAME_MIN_CHARS),
            1,
        ),
        (
            "/properties/name",
            "maxLength",
            wide(limits::NAME_MAX_CHARS),
            64,
        ),
        (
            "/properties/author",
            "maxLength",
            wide(limits::AUTHOR_MAX_CHARS),
            64,
        ),
        (
            "/$defs/TerminalSettings/properties/font",
            "minLength",
            wide(limits::FONT_MIN_CHARS),
            1,
        ),
        (
            "/$defs/TerminalSettings/properties/font",
            "maxLength",
            wide(limits::FONT_MAX_CHARS),
            64,
        ),
        (
            "/$defs/TerminalSettings/properties/fontSize",
            "minimum",
            wide(limits::FONT_SIZE_MIN),
            8,
        ),
        (
            "/$defs/TerminalSettings/properties/fontSize",
            "maximum",
            wide(limits::FONT_SIZE_MAX),
            32,
        ),
        (
            "/$defs/TerminalSettings/properties/opacity",
            "minimum",
            wide(limits::OPACITY_MIN),
            30,
        ),
        (
            "/$defs/TerminalSettings/properties/opacity",
            "maximum",
            wide(limits::OPACITY_MAX),
            100,
        ),
        (
            "/$defs/TerminalSettings/properties/padding/items",
            "maximum",
            wide(limits::PADDING_MAX),
            40,
        ),
        (
            "/$defs/BannerSettings/properties/title",
            "maxLength",
            wide(limits::TITLE_MAX_CHARS),
            80,
        ),
        (
            "/$defs/BannerSettings/properties/lines",
            "maxItems",
            wide(limits::LINES_MAX),
            4,
        ),
        (
            "/$defs/BannerSettings/properties/lines/items",
            "maxLength",
            wide(limits::LINE_MAX_CHARS),
            80,
        ),
    ];
    for (field, keyword, in_the_code, in_the_spec) in from_the_code {
        assert_eq!(
            in_the_code, in_the_spec,
            "{field} {keyword}: model::limits no longer says what spec §3.3 says"
        );
        assert_eq!(
            whole(&schema, &format!("{field}/{keyword}")),
            in_the_code,
            "{field} {keyword}: the schema does not state the limit in model::limits"
        );
    }
    // Limits that come from the type itself (a pair, sixteen colors) and have no constant.
    let from_the_types = [
        (
            "/$defs/TerminalSettings/properties/padding/items",
            "minimum",
            0,
        ),
        ("/$defs/TerminalSettings/properties/padding", "minItems", 2),
        ("/$defs/TerminalSettings/properties/padding", "maxItems", 2),
        ("/$defs/Colors/properties/ansi", "minItems", 16),
        ("/$defs/Colors/properties/ansi", "maxItems", 16),
    ];
    for (field, keyword, number) in from_the_types {
        assert_eq!(
            whole(&schema, &format!("{field}/{keyword}")),
            number,
            "{field} {keyword}"
        );
    }
    // Where the spec gives only a maximum, the schema gives no minimum at all. `author` is 0-64,
    // so the spec's own example, which has `"author": ""`, has to pass. A banner may have no
    // lines, and its title and each of its lines may be empty.
    for (field, keyword) in [
        ("/properties/author", "minLength"),
        ("/$defs/BannerSettings/properties/title", "minLength"),
        ("/$defs/BannerSettings/properties/lines", "minItems"),
        ("/$defs/BannerSettings/properties/lines/items", "minLength"),
    ] {
        assert_eq!(
            schema.pointer(&format!("{field}/{keyword}")),
            None,
            "{field} has a {keyword}, but spec §3.3 gives it only a maximum"
        );
    }
    // The tint is the one limit that is not a whole number.
    let tint = "/$defs/SimpleRecipe/properties/tint";
    assert_eq!(
        limits::TINT_MIN,
        0.0,
        "tint minimum: model::limits no longer says what spec §3.3 says"
    );
    assert_eq!(
        limits::TINT_MAX,
        0.30,
        "tint maximum: model::limits no longer says what spec §3.3 says"
    );
    assert_eq!(
        real(&schema, &format!("{tint}/minimum")),
        limits::TINT_MIN,
        "tint minimum: the schema does not state the limit in model::limits"
    );
    assert_eq!(
        real(&schema, &format!("{tint}/maximum")),
        limits::TINT_MAX,
        "tint maximum: the schema does not state the limit in model::limits"
    );
    // Each segment once.
    assert_eq!(
        at(
            &schema,
            "/$defs/PromptSettings/properties/segments/uniqueItems"
        ),
        &json!(true)
    );
    // And a limit means nothing on the wrong type: a whole number that the schema let be a
    // fraction would take 30.5, which the parser refuses.
    let kinds = [
        ("/properties/schemaVersion", "integer"),
        ("/properties/name", "string"),
        ("/properties/author", "string"),
        ("/$defs/SimpleRecipe/properties/tint", "number"),
        ("/$defs/TerminalSettings/properties/font", "string"),
        ("/$defs/TerminalSettings/properties/fontSize", "integer"),
        ("/$defs/TerminalSettings/properties/opacity", "integer"),
        ("/$defs/TerminalSettings/properties/padding", "array"),
        (
            "/$defs/TerminalSettings/properties/padding/items",
            "integer",
        ),
        ("/$defs/PromptSettings/properties/segments", "array"),
        ("/$defs/BannerSettings/properties/title", "string"),
        ("/$defs/BannerSettings/properties/lines", "array"),
        ("/$defs/BannerSettings/properties/lines/items", "string"),
    ];
    for (field, kind) in kinds {
        assert_eq!(text_at(&schema, &format!("{field}/type")), kind, "{field}");
    }
}

#[test]
fn the_schema_states_the_color_the_id_and_the_allowed_words() {
    let schema = schema();
    // One definition of a color, `#rrggbb`, and every color of a theme is one.
    assert_eq!(text_at(&schema, "/$defs/Color/type"), "string");
    assert_eq!(
        text_at(&schema, "/$defs/Color/pattern"),
        "^#[0-9a-fA-F]{6}$",
        "a color is #rrggbb (spec §3.3)"
    );
    let colors = [
        "/$defs/SimpleRecipe/properties/accent",
        "/$defs/Colors/properties/background",
        "/$defs/Colors/properties/foreground",
        "/$defs/Colors/properties/cursor",
        "/$defs/Colors/properties/selection",
        "/$defs/Colors/properties/accent",
        "/$defs/Colors/properties/muted",
        "/$defs/Colors/properties/frame",
        "/$defs/Colors/properties/ansi/items",
    ];
    for color in colors {
        assert_eq!(
            text_at(&schema, &format!("{color}/$ref")),
            "#/$defs/Color",
            "{color}"
        );
    }
    // An id is a version 4 UUID in its one written form (spec §3.3).
    assert_eq!(text_at(&schema, "/properties/id/type"), "string");
    assert_eq!(text_at(&schema, "/properties/id/format"), "uuid");
    assert_eq!(
        text_at(&schema, "/properties/id/pattern"),
        r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-4[0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$",
        "an id is a version 4 UUID in its 8-4-4-4-12 form (spec §3.3)"
    );
    // The words a field may hold, and the one definition each field points at. The prompt
    // segments are the whitelist of spec §3.5: anything else, `command` included, is refused.
    let enums: [(&str, &str, &[&str]); 4] = [
        ("/properties/tone", "Tone", &["dark", "light"]),
        (
            "/$defs/TerminalSettings/properties/cursorShape",
            "CursorShape",
            &["filledBox", "bar", "underscore"],
        ),
        (
            "/$defs/PromptSettings/properties/layout",
            "Layout",
            &["box", "line", "powerline"],
        ),
        (
            "/$defs/PromptSettings/properties/segments/items",
            "Segment",
            &["path", "git", "time", "user", "host", "status", "duration"],
        ),
    ];
    for (field, definition, allowed) in enums {
        assert_eq!(
            text_at(&schema, &format!("{field}/$ref")),
            format!("#/$defs/{definition}"),
            "{field}"
        );
        assert_eq!(
            strings_at(&schema, &format!("/$defs/{definition}/enum")),
            words(allowed),
            "{definition}"
        );
    }
}

#[test]
fn every_reference_resolves_inside_the_file() {
    let schema = schema();
    let mut targets = Vec::new();
    string_values(&schema, "$ref", &mut targets);
    assert!(!targets.is_empty());
    for target in targets {
        let pointer = target
            .strip_prefix('#')
            .unwrap_or_else(|| panic!("{target} points outside the file"));
        assert!(
            schema.pointer(pointer).is_some(),
            "{target} points at nothing"
        );
    }
}

#[test]
fn the_only_format_in_the_schema_is_uuid() {
    // `schemars` also writes `uint8`, `uint32` and `double` for the number types. Those are not
    // formats of JSON Schema, and a validator that does not know a format can refuse the whole
    // schema (Ajv does by default), so the published file keeps only what the standard defines.
    let schema = schema();
    let mut formats = Vec::new();
    string_values(&schema, "format", &mut formats);
    assert!(!formats.is_empty(), "the id should keep its uuid format");
    assert!(
        formats.iter().all(|format| *format == "uuid"),
        "the schema has formats other than uuid: {formats:?}"
    );
}
