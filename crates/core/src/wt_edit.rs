//! Edits Windows Terminal's settings.json in place (spec §5.4, G9). Only the
//! keys CMD Studio owns change; comments, order, spacing, newlines and a
//! UTF-8 BOM stay byte-for-byte. Pure: text in, text out.
use crate::generate::wt::{CMD_PROFILE_GUID, SCHEME_NAME};
use jsonc_parser::cst::{CstArray, CstInputValue, CstNode, CstObject, CstRootNode};
use jsonc_parser::ParseOptions;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};

/// What Windows Terminal itself accepts: comments and trailing commas, nothing looser.
pub fn wt_parse_options() -> ParseOptions {
    ParseOptions {
        allow_comments: true,
        allow_trailing_commas: true,
        allow_loose_object_property_names: false,
        allow_missing_commas: false,
        allow_single_quoted_strings: false,
        allow_hexadecimal_numbers: false,
        allow_unary_plus_numbers: false,
        allow_bare_decimal_point_numbers: false,
        allow_non_finite_numbers: false,
        allow_extended_string_escapes: false,
    }
}

/// One setting CMD Studio owns in a settings.json.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "at", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum WtKey {
    /// The color scheme named `CMD Studio`, as a whole.
    Scheme,
    /// The top-level `defaultProfile`.
    DefaultProfile,
    /// A key of the cmd profile (guid `{0caa0dad-…}`).
    Profile { key: String },
    /// A key inside an object-valued key of the cmd profile, such as `font.face`.
    ProfileSub { key: String, sub: String },
}

/// A key's value before and after one edit. `original: None` means the key was absent.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WtChange {
    pub key: WtKey,
    pub original: Option<Value>,
    pub written: Value,
}

/// A container the file lacked, so the edit added it. Revert removes it again
/// once nothing of the user's is in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WtCreated {
    /// The cmd profile object.
    CmdProfile,
    /// `profiles.list`.
    ProfileList,
    /// `profiles`.
    Profiles,
    /// `schemes`.
    Schemes,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WtEdited {
    pub text: String,
    pub changes: Vec<WtChange>,
    pub created: Vec<WtCreated>,
}

/// What `apply_edit` writes: the values from `plan::Op::WtApply`.
#[derive(Clone, Copy, Debug)]
pub struct WtEdit<'a> {
    pub scheme: &'a Value,
    pub profile: &'a Map<String, Value>,
    pub set_default: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WtError {
    #[error("settings.json is not valid JSON with comments: {0}")]
    Parse(String),
    #[error("settings.json has an unexpected shape: {0}")]
    Shape(String),
}

impl WtError {
    /// Stable code shown to users (spec §5.9).
    pub fn code(&self) -> &'static str {
        "E_WT_PARSE"
    }
}

/// Converts a generator value into the CST input type (G9: the crate has no
/// `From<serde_json::Value>`). Object keys keep `guid` and `name` first, then
/// the map's own order.
pub fn to_cst(value: &Value) -> CstInputValue {
    match value {
        Value::Null => CstInputValue::Null,
        Value::Bool(b) => CstInputValue::Bool(*b),
        Value::Number(n) => CstInputValue::Number(n.to_string()),
        Value::String(s) => CstInputValue::String(s.clone()),
        Value::Array(items) => CstInputValue::Array(items.iter().map(to_cst).collect()),
        Value::Object(map) => {
            let first = ["guid", "name"];
            let mut props: Vec<(String, CstInputValue)> = first
                .iter()
                .filter_map(|k| map.get(*k).map(|v| ((*k).to_string(), to_cst(v))))
                .collect();
            props.extend(
                map.iter()
                    .filter(|(k, _)| !first.contains(&k.as_str()))
                    .map(|(k, v)| (k.clone(), to_cst(v))),
            );
            CstInputValue::Object(props)
        }
    }
}

/// Fails with `E_WT_PARSE` when Windows Terminal could not read the text.
pub fn check_parses(text: &str) -> Result<(), WtError> {
    let (_, body) = split_bom(text);
    let root = parse(body)?;
    root_object(&root).map(|_| ())
}

/// Writes CMD Studio's scheme, cmd profile keys and (when asked) the default
/// profile. Records every touched key's value before and after. `edit.scheme`
/// is `generate::wt::scheme`'s object; anything else is refused as `Shape`.
pub fn apply_edit(text: &str, edit: &WtEdit<'_>) -> Result<WtEdited, WtError> {
    let (bom, body) = split_bom(text);
    let root = parse(body)?;
    let obj = root_object(&root)?;
    let mut changes = Vec::new();
    let mut created = Vec::new();

    // 1. The scheme: every scheme named "CMD Studio" becomes ours, or one is appended.
    let scheme = edit
        .scheme
        .as_object()
        .ok_or_else(|| WtError::Shape("the scheme to write is not an object".into()))?;
    if obj.get("schemes").is_none() {
        created.push(WtCreated::Schemes);
    }
    let schemes = obj
        .array_value_or_create("schemes")
        .ok_or_else(|| WtError::Shape("\"schemes\" is not a list".into()))?;
    let ours: Vec<CstObject> = schemes
        .elements()
        .into_iter()
        .filter_map(|e| our_scheme(&e))
        .collect();
    let original = ours.first().and_then(CstObject::to_serde_value);
    if ours.is_empty() {
        schemes.append(to_cst(edit.scheme));
    }
    for existing in &ours {
        set_object(existing, scheme);
    }
    changes.push(WtChange {
        key: WtKey::Scheme,
        original,
        written: edit.scheme.clone(),
    });

    // 2. The cmd profile: create it, or set each key on the existing one.
    let list = profiles_list(&obj, &mut created)?;
    match find_cmd_profile(&list) {
        Some(profile) => set_profile_keys(&profile, edit.profile, &mut changes)?,
        None => {
            let mut map = Map::new();
            map.insert("guid".into(), Value::String(CMD_PROFILE_GUID.into()));
            map.insert("name".into(), Value::String("Command Prompt".into()));
            for (k, v) in edit.profile {
                map.insert(k.clone(), v.clone());
                changes.push(WtChange {
                    key: WtKey::Profile { key: k.clone() },
                    original: None,
                    written: v.clone(),
                });
            }
            list.append(to_cst(&Value::Object(map)));
            created.push(WtCreated::CmdProfile);
        }
    }

    // 3. The default profile, only when the user asked for it.
    if edit.set_default {
        let new = Value::String(CMD_PROFILE_GUID.into());
        let original = obj
            .get("defaultProfile")
            .and_then(|p| p.value())
            .and_then(|v| v.to_serde_value());
        let already = original
            .as_ref()
            .and_then(Value::as_str)
            .is_some_and(|s| s.eq_ignore_ascii_case(CMD_PROFILE_GUID));
        if !already {
            set_prop(&obj, "defaultProfile", &new);
        }
        changes.push(WtChange {
            key: WtKey::DefaultProfile,
            original,
            written: new,
        });
    }

    let text = format!("{bom}{root}");
    check_parses(&text)?;
    created.sort();
    Ok(WtEdited {
        text,
        changes,
        created,
    })
}

/// The current value of one owned key; `None` when it is absent.
pub fn read_value(text: &str, key: &WtKey) -> Result<Option<Value>, WtError> {
    let (_, body) = split_bom(text);
    let root = parse(body)?;
    let obj = root_object(&root)?;
    Ok(match key {
        WtKey::Scheme => obj
            .array_value("schemes")
            .and_then(|s| s.elements().into_iter().find_map(|e| our_scheme(&e)))
            .and_then(|o| o.to_serde_value()),
        WtKey::DefaultProfile => obj
            .get("defaultProfile")
            .and_then(|p| p.value())
            .and_then(|v| v.to_serde_value()),
        WtKey::Profile { key } => existing_profile(&obj)
            .and_then(|p| p.get(key))
            .and_then(|p| p.value())
            .and_then(|v| v.to_serde_value()),
        WtKey::ProfileSub { key, sub } => existing_profile(&obj)
            .and_then(|p| p.object_value(key))
            .and_then(|o| o.get(sub))
            .and_then(|p| p.value())
            .and_then(|v| v.to_serde_value()),
    })
}

/// Undoes CMD Studio's keys one by one (spec §6.3 step 2): every scheme named
/// `CMD Studio` goes; a key still holding what CMD Studio last wrote gets its
/// original back (or is removed when it was absent); a key the user changed
/// since keeps the user's value; containers CMD Studio created go when
/// nothing of the user's is left in them. The maps may cover several applies:
/// each key's first original and last write.
pub fn revert(
    text: &str,
    originals: &BTreeMap<WtKey, Option<Value>>,
    written: &BTreeMap<WtKey, Value>,
    created: &[WtCreated],
) -> Result<String, WtError> {
    let (bom, body) = split_bom(text);
    let root = parse(body)?;
    let obj = root_object(&root)?;

    if let Some(schemes) = obj.array_value("schemes") {
        for scheme in schemes
            .elements()
            .into_iter()
            .filter_map(|e| our_scheme(&e))
        {
            scheme.remove();
        }
    }

    // BTreeMap order puts a parent key (`font`) before its sub-keys (`font.face`);
    // this walks it backwards, sub-keys first. Over several applies one may write
    // `font` whole and a later one its keys one by one; once those are back, `font`
    // holds what was first written and goes too.
    for (key, original) in originals.iter().rev() {
        let Some(last) = written.get(key) else {
            continue;
        };
        match key {
            WtKey::Scheme => {}
            WtKey::DefaultProfile => restore_prop(&obj, "defaultProfile", original, last),
            WtKey::Profile { key } => {
                if let Some(profile) = existing_profile(&obj) {
                    restore_prop(&profile, key, original, last);
                }
            }
            WtKey::ProfileSub { key, sub } => {
                if let Some(parent) = existing_profile(&obj).and_then(|p| p.object_value(key)) {
                    restore_prop(&parent, sub, original, last);
                }
            }
        }
    }

    if created.contains(&WtCreated::CmdProfile) {
        if let Some(profile) = existing_profile(&obj) {
            let only_ours = profile
                .properties()
                .iter()
                .all(|p| p.decoded_name().is_some_and(|n| n == "guid" || n == "name"));
            if only_ours {
                profile.remove();
            }
        }
    }
    if created.contains(&WtCreated::ProfileList) {
        if let Some(profiles) = obj.object_value("profiles") {
            remove_if_empty(&profiles, "list");
        }
    }
    if created.contains(&WtCreated::Profiles) {
        remove_if_empty(&obj, "profiles");
    }
    if created.contains(&WtCreated::Schemes) {
        remove_if_empty(&obj, "schemes");
    }

    let text = format!("{bom}{root}");
    check_parses(&text)?;
    Ok(text)
}

fn split_bom(text: &str) -> (&'static str, &str) {
    match text.strip_prefix('\u{feff}') {
        Some(rest) => ("\u{feff}", rest),
        None => ("", text),
    }
}

fn parse(text: &str) -> Result<CstRootNode, WtError> {
    CstRootNode::parse(text, &wt_parse_options()).map_err(|e| WtError::Parse(e.to_string()))
}

fn root_object(root: &CstRootNode) -> Result<CstObject, WtError> {
    root.object_value()
        .ok_or_else(|| WtError::Shape("the top level is not an object".into()))
}

fn our_scheme(node: &CstNode) -> Option<CstObject> {
    let obj = node.as_object()?;
    let name = obj
        .get("name")?
        .value()?
        .as_string_lit()?
        .decoded_value()
        .ok()?;
    (name == SCHEME_NAME).then_some(obj)
}

/// `profiles.list`, or the legacy form where `profiles` is the list itself.
/// Adds whichever of the two is missing and records it in `created`.
fn profiles_list(obj: &CstObject, created: &mut Vec<WtCreated>) -> Result<CstArray, WtError> {
    let shape =
        || WtError::Shape("\"profiles\" is neither an object with a \"list\" nor a list".into());
    match obj.get("profiles").and_then(|p| p.value()) {
        Some(node) => match (node.as_array(), node.as_object()) {
            (Some(list), _) => Ok(list),
            (None, Some(profiles)) => {
                if profiles.get("list").is_none() {
                    created.push(WtCreated::ProfileList);
                }
                profiles.array_value_or_create("list").ok_or_else(shape)
            }
            (None, None) => Err(shape()),
        },
        None => {
            created.push(WtCreated::Profiles);
            created.push(WtCreated::ProfileList);
            obj.object_value_or_create("profiles")
                .and_then(|p| p.array_value_or_create("list"))
                .ok_or_else(shape)
        }
    }
}

/// Removes `name` when it holds an empty list or an empty object.
fn remove_if_empty(obj: &CstObject, name: &str) {
    let empty = match current(obj, name) {
        Some(Value::Array(items)) => items.is_empty(),
        Some(Value::Object(map)) => map.is_empty(),
        _ => false,
    };
    if empty {
        if let Some(prop) = obj.get(name) {
            prop.remove();
        }
    }
}

fn find_cmd_profile(list: &CstArray) -> Option<CstObject> {
    list.elements()
        .into_iter()
        .filter_map(|e| e.as_object())
        .find(|o| {
            o.get("guid")
                .and_then(|p| p.value())
                .and_then(|v| v.as_string_lit())
                .and_then(|s| s.decoded_value().ok())
                .is_some_and(|g| g.eq_ignore_ascii_case(CMD_PROFILE_GUID))
        })
}

fn existing_profile(obj: &CstObject) -> Option<CstObject> {
    let node = obj.get("profiles")?.value()?;
    let list = match node.as_array() {
        Some(list) => list,
        None => node.as_object()?.array_value("list")?,
    };
    find_cmd_profile(&list)
}

fn current(obj: &CstObject, name: &str) -> Option<Value> {
    obj.get(name)
        .and_then(|p| p.value())
        .and_then(|v| v.to_serde_value())
}

/// Sets `name` to `value`, leaving the text alone when it already holds it.
fn set_prop(obj: &CstObject, name: &str, value: &Value) {
    match obj.get(name) {
        Some(prop) => {
            if current(obj, name).as_ref() != Some(value) {
                prop.set_value(to_cst(value));
            }
        }
        None => {
            obj.append(name, to_cst(value));
        }
    }
}

/// Makes `obj` hold exactly the keys and values of `value`, key by key, so it
/// keeps its place, its layout and the bytes of every key that already matches.
/// (jsonc-parser 0.34.0's `replace_with` on an array element lays the new
/// object out one level too shallow.) Keys that are not in `value`, and second
/// copies of a key, go.
fn set_object(obj: &CstObject, value: &Map<String, Value>) {
    let mut seen = BTreeSet::new();
    for prop in obj.properties() {
        let keep = prop
            .decoded_name()
            .is_some_and(|name| value.contains_key(&name) && seen.insert(name));
        if !keep {
            prop.remove();
        }
    }
    for (name, v) in value {
        set_prop(obj, name, v);
    }
}

fn set_profile_keys(
    profile: &CstObject,
    values: &Map<String, Value>,
    changes: &mut Vec<WtChange>,
) -> Result<(), WtError> {
    for (key, value) in values {
        let existing = current(profile, key);
        match (value.as_object(), existing.as_ref()) {
            // An object value (font) merges key by key into an existing object, so
            // the user's other font keys (weight, features) survive.
            (Some(subs), Some(Value::Object(_))) => {
                let parent = profile.object_value(key).ok_or_else(|| {
                    WtError::Shape(format!("cmd profile key {key:?} changed while editing"))
                })?;
                for (sub, sub_value) in subs {
                    changes.push(WtChange {
                        key: WtKey::ProfileSub {
                            key: key.clone(),
                            sub: sub.clone(),
                        },
                        original: current(&parent, sub),
                        written: sub_value.clone(),
                    });
                    set_prop(&parent, sub, sub_value);
                }
            }
            _ => {
                changes.push(WtChange {
                    key: WtKey::Profile { key: key.clone() },
                    original: existing.clone(),
                    written: value.clone(),
                });
                set_prop(profile, key, value);
            }
        }
    }
    Ok(())
}

/// Puts `original` back when the key still holds what CMD Studio wrote.
fn restore_prop(obj: &CstObject, name: &str, original: &Option<Value>, last_written: &Value) {
    if current(obj, name).as_ref() != Some(last_written) {
        return;
    }
    match original {
        Some(value) => set_prop(obj, name, value),
        None => {
            if let Some(prop) = obj.get(name) {
                prop.remove();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const COMMENTED: &str = include_str!("../tests/fixtures/wt/commented.jsonc");
    const DEFAULT: &str = include_str!("../tests/fixtures/wt/default.json");
    const NO_CMD: &str = include_str!("../tests/fixtures/wt/no-cmd-profile.jsonc");
    const LEGACY: &str = include_str!("../tests/fixtures/wt/legacy-profiles-array.jsonc");
    const USER_CMD: &str = include_str!("../tests/fixtures/wt/user-cmd-profile.jsonc");
    const BROKEN: &str = include_str!("../tests/fixtures/wt/broken.jsonc");

    fn scheme() -> Value {
        json!({ "name": "CMD Studio", "background": "#1f1e1d", "foreground": "#faf9f5" })
    }

    fn profile() -> Map<String, Value> {
        let mut m = Map::new();
        m.insert("colorScheme".into(), json!("CMD Studio"));
        m.insert(
            "commandline".into(),
            json!(r"%SystemRoot%\System32\cmd.exe"),
        );
        m.insert(
            "font".into(),
            json!({ "face": "Cascadia Mono", "size": 12 }),
        );
        m.insert("opacity".into(), json!(96));
        m
    }

    fn edit<'a>(
        scheme: &'a Value,
        profile: &'a Map<String, Value>,
        set_default: bool,
    ) -> WtEdit<'a> {
        WtEdit {
            scheme,
            profile,
            set_default,
        }
    }

    fn originals(e: &WtEdited) -> BTreeMap<WtKey, Option<Value>> {
        e.changes
            .iter()
            .map(|c| (c.key.clone(), c.original.clone()))
            .collect()
    }

    fn written(e: &WtEdited) -> BTreeMap<WtKey, Value> {
        e.changes
            .iter()
            .map(|c| (c.key.clone(), c.written.clone()))
            .collect()
    }

    #[test]
    fn comments_and_untouched_bytes_survive() {
        let (s, p) = (scheme(), profile());
        let out = apply_edit(COMMENTED, &edit(&s, &p, false)).unwrap().text;
        assert!(out.contains("// User's own comment at the top"));
        assert!(out.contains("\"name\": \"Windows PowerShell\" // inline comment"));
        assert!(out.contains("/* block comment the user wrote */"));
        assert!(out.starts_with("// User's own comment at the top\n{\n    \"$schema\""));
        assert_eq!(
            read_value(
                &out,
                &WtKey::Profile {
                    key: "colorScheme".into()
                }
            )
            .unwrap(),
            Some(json!("CMD Studio"))
        );
        assert_eq!(read_value(&out, &WtKey::Scheme).unwrap(), Some(s));
    }

    #[test]
    fn reapplying_the_same_values_changes_nothing() {
        let (s, p) = (scheme(), profile());
        let once = apply_edit(DEFAULT, &edit(&s, &p, true)).unwrap().text;
        let twice = apply_edit(&once, &edit(&s, &p, true)).unwrap().text;
        assert_eq!(once, twice);
    }

    #[test]
    fn an_existing_cmd_studio_scheme_is_replaced_not_duplicated() {
        let (s, p) = (scheme(), profile());
        let first = apply_edit(DEFAULT, &edit(&s, &p, false)).unwrap().text;
        let s2 = json!({ "name": "CMD Studio", "background": "#000000", "foreground": "#ffffff" });
        let second = apply_edit(&first, &edit(&s2, &p, false)).unwrap();
        assert_eq!(
            second.text.matches("\"CMD Studio\"").count(),
            first.matches("\"CMD Studio\"").count()
        );
        assert_eq!(read_value(&second.text, &WtKey::Scheme).unwrap(), Some(s2));
        assert_eq!(second.changes.first().unwrap().original, Some(s));
    }

    #[test]
    fn a_missing_cmd_profile_is_created_and_reverted_away() {
        let (s, p) = (scheme(), profile());
        let e = apply_edit(NO_CMD, &edit(&s, &p, false)).unwrap();
        assert!(e.created.contains(&WtCreated::CmdProfile));
        assert!(e.text.contains("\"schemes\""));
        assert_eq!(
            read_value(&e.text, &WtKey::Profile { key: "name".into() }).unwrap(),
            Some(json!("Command Prompt"))
        );
        let back = revert(&e.text, &originals(&e), &written(&e), &e.created).unwrap();
        assert!(!back.contains("0caa0dad"));
        assert!(!back.contains("CMD Studio"));
        jsonc_parser::parse_to_value(&back, &wt_parse_options()).unwrap();
    }

    #[test]
    fn legacy_profiles_array_is_edited_in_place() {
        let (s, p) = (scheme(), profile());
        let e = apply_edit(LEGACY, &edit(&s, &p, false)).unwrap();
        assert!(!e.created.contains(&WtCreated::CmdProfile));
        assert_eq!(
            read_value(
                &e.text,
                &WtKey::Profile {
                    key: "opacity".into()
                }
            )
            .unwrap(),
            Some(json!(96))
        );
    }

    #[test]
    fn font_merges_into_the_users_font_object() {
        let (s, p) = (scheme(), profile());
        let e = apply_edit(USER_CMD, &edit(&s, &p, false)).unwrap();
        assert_eq!(
            read_value(
                &e.text,
                &WtKey::ProfileSub {
                    key: "font".into(),
                    sub: "weight".into()
                }
            )
            .unwrap(),
            Some(json!("bold"))
        );
        assert_eq!(
            read_value(
                &e.text,
                &WtKey::ProfileSub {
                    key: "font".into(),
                    sub: "face".into()
                }
            )
            .unwrap(),
            Some(json!("Cascadia Mono"))
        );
        assert!(e.changes.iter().any(|c| c.key
            == WtKey::ProfileSub {
                key: "font".into(),
                sub: "face".into()
            }
            && c.original == Some(json!("Consolas"))));
    }

    #[test]
    fn uppercase_guid_finds_the_users_profile() {
        let (s, p) = (scheme(), profile());
        let e = apply_edit(USER_CMD, &edit(&s, &p, false)).unwrap();
        assert!(!e.created.contains(&WtCreated::CmdProfile));
        assert_eq!(
            read_value(&e.text, &WtKey::Profile { key: "name".into() }).unwrap(),
            Some(json!("My cmd"))
        );
    }

    #[test]
    fn bom_and_crlf_are_kept() {
        // Review Focus 3.
        let (s, p) = (scheme(), profile());
        let crlf = format!("\u{feff}{}", COMMENTED.replace('\n', "\r\n"));
        let out = apply_edit(&crlf, &edit(&s, &p, true)).unwrap().text;
        assert!(out.starts_with('\u{feff}'));
        assert!(
            !out.replace("\r\n", "").contains('\n'),
            "a bare LF slipped in"
        );
    }

    #[test]
    fn unparseable_files_are_refused() {
        let (s, p) = (scheme(), profile());
        let err = apply_edit(BROKEN, &edit(&s, &p, false)).unwrap_err();
        assert_eq!(err.code(), "E_WT_PARSE");
        assert!(matches!(
            apply_edit("[]", &edit(&s, &p, false)),
            Err(WtError::Shape(_))
        ));
        assert!(matches!(
            apply_edit("{'a': 1}", &edit(&s, &p, false)),
            Err(WtError::Parse(_))
        ));
        assert!(matches!(
            apply_edit(r#"{"profiles": 3}"#, &edit(&s, &p, false)),
            Err(WtError::Shape(_))
        ));
    }

    #[test]
    fn default_profile_is_set_only_on_request_and_recorded() {
        let (s, p) = (scheme(), profile());
        let off = apply_edit(DEFAULT, &edit(&s, &p, false)).unwrap();
        assert!(off
            .text
            .contains("\"defaultProfile\": \"{61c54bbd-c2c6-5271-96e7-009a87ff44bf}\""));
        let on = apply_edit(DEFAULT, &edit(&s, &p, true)).unwrap();
        assert_eq!(
            read_value(&on.text, &WtKey::DefaultProfile).unwrap(),
            Some(json!(CMD_PROFILE_GUID))
        );
        assert!(on.changes.iter().any(|c| c.key == WtKey::DefaultProfile
            && c.original == Some(json!("{61c54bbd-c2c6-5271-96e7-009a87ff44bf}"))));
    }

    #[test]
    fn revert_of_an_untouched_edit_gives_back_the_original_bytes() {
        let (s, p) = (scheme(), profile());
        for (name, fixture) in [
            ("default", DEFAULT),
            ("commented", COMMENTED),
            ("legacy", LEGACY),
            ("nocmd", NO_CMD),
        ] {
            let e = apply_edit(fixture, &edit(&s, &p, true)).unwrap();
            let back = revert(&e.text, &originals(&e), &written(&e), &e.created).unwrap();
            assert_eq!(back, fixture, "{name}");
        }
    }

    #[test]
    fn a_file_with_no_profiles_or_schemes_gets_them_and_loses_them_again() {
        // Review Focus 3.
        let (s, p) = (scheme(), profile());
        for fixture in ["{}\n", "{\n    \"copyOnSelect\": true\n}\n"] {
            let e = apply_edit(fixture, &edit(&s, &p, true)).unwrap();
            assert_eq!(
                e.created,
                vec![
                    WtCreated::CmdProfile,
                    WtCreated::ProfileList,
                    WtCreated::Profiles,
                    WtCreated::Schemes
                ]
            );
            let back = revert(&e.text, &originals(&e), &written(&e), &e.created).unwrap();
            assert_eq!(back, fixture);
        }
    }

    #[test]
    fn revert_after_growing_a_one_line_object_restores_values_not_layout() {
        // Adding `size` to `"font": { "face": …, "weight": … }` makes the
        // library lay the object out over several lines; revert brings the
        // values back but keeps that layout. Undo (snapshot restore) is the
        // byte-exact path.
        let (s, p) = (scheme(), profile());
        let e = apply_edit(USER_CMD, &edit(&s, &p, true)).unwrap();
        let back = revert(&e.text, &originals(&e), &written(&e), &e.created).unwrap();
        let value = |t: &str| {
            CstRootNode::parse(t, &wt_parse_options())
                .unwrap()
                .value()
                .unwrap()
                .to_serde_value()
                .unwrap()
        };
        assert_eq!(value(&back), value(USER_CMD));
    }

    #[test]
    fn revert_keeps_a_value_the_user_changed_after_us() {
        let (s, p) = (scheme(), profile());
        let e = apply_edit(DEFAULT, &edit(&s, &p, false)).unwrap();
        let user_edit = e.text.replace("\"opacity\": 96", "\"opacity\": 70");
        assert_ne!(user_edit, e.text);
        let back = revert(&user_edit, &originals(&e), &written(&e), &e.created).unwrap();
        assert_eq!(
            read_value(
                &back,
                &WtKey::Profile {
                    key: "opacity".into()
                }
            )
            .unwrap(),
            Some(json!(70))
        );
        assert_eq!(
            read_value(
                &back,
                &WtKey::Profile {
                    key: "colorScheme".into()
                }
            )
            .unwrap(),
            None
        );
        assert_eq!(read_value(&back, &WtKey::Scheme).unwrap(), None);
    }

    #[test]
    fn to_cst_round_trips_every_json_kind() {
        let v = json!({ "name": "n", "a": [1, 2.5, -3], "b": { "c": null, "d": true }, "e": "x\"y\\z" });
        let root = CstRootNode::parse("{}", &wt_parse_options()).unwrap();
        root.object_value_or_set().append("v", to_cst(&v));
        let text = root.to_string();
        let back = CstRootNode::parse(&text, &wt_parse_options())
            .unwrap()
            .value()
            .unwrap()
            .to_serde_value()
            .unwrap();
        assert_eq!(back["v"], v);
        assert!(text.find("\"name\"").unwrap() < text.find("\"a\"").unwrap());
    }

    #[test]
    fn every_written_value_reads_back() {
        // The one exception: a defaultProfile that already names the cmd profile in
        // other letter case stays as the user wrote it. The recorded write, our
        // lowercase guid, never matches it, so revert leaves the user's text alone.
        let (s, p) = (scheme(), profile());
        for (name, fixture) in [
            ("default", DEFAULT),
            ("commented", COMMENTED),
            ("nocmd", NO_CMD),
            ("legacy", LEGACY),
            ("usercmd", USER_CMD),
        ] {
            let e = apply_edit(fixture, &edit(&s, &p, true)).unwrap();
            for c in &e.changes {
                let got = read_value(&e.text, &c.key).unwrap();
                if name == "usercmd" && c.key == WtKey::DefaultProfile {
                    assert_eq!(got, Some(json!("{0CAA0DAD-35BE-5F56-A8FF-AFCEEEAA6101}")));
                    assert_eq!(c.written, json!(CMD_PROFILE_GUID));
                } else {
                    assert_eq!(got.as_ref(), Some(&c.written), "{name} {:?}", c.key);
                }
            }
        }
        let e = apply_edit(USER_CMD, &edit(&s, &p, true)).unwrap();
        assert!(e
            .text
            .contains("\"defaultProfile\": \"{0CAA0DAD-35BE-5F56-A8FF-AFCEEEAA6101}\","));
    }

    #[test]
    fn a_new_theme_rewrites_the_scheme_in_place() {
        // jsonc-parser 0.34.0's `replace_with` lays a replaced array element out
        // one level too shallow, so the scheme is updated key by key: after a
        // theme change the file reads as if that theme had been applied first.
        let (s, p) = (scheme(), profile());
        let s2 = json!({ "name": "CMD Studio", "background": "#000000", "foreground": "#ffffff" });
        for (name, fixture) in [
            ("default", DEFAULT),
            ("legacy", LEGACY),
            ("usercmd", USER_CMD),
        ] {
            let first = apply_edit(fixture, &edit(&s, &p, false)).unwrap().text;
            let second = apply_edit(&first, &edit(&s2, &p, false)).unwrap().text;
            assert_eq!(
                second,
                apply_edit(fixture, &edit(&s2, &p, false)).unwrap().text,
                "{name}"
            );
        }
    }

    #[test]
    fn every_cmd_studio_scheme_becomes_ours_and_every_one_goes_on_revert() {
        // Windows Terminal uses the last of two schemes with one name. A key that
        // is not ours, and a second copy of a key, go too.
        let (s, p) = (scheme(), profile());
        let text = concat!(
            "{\n    \"schemes\": [\n",
            "        { \"name\": \"CMD Studio\", \"background\": \"#000000\", \"cursorColor\": \"#ffffff\" },\n",
            "        { \"name\": \"Campbell\" },\n",
            "        { \"name\": \"CMD Studio\", \"foreground\": \"#111111\", \"foreground\": \"#222222\" }\n",
            "    ]\n}\n"
        );
        let schemes = |t: &str| {
            CstRootNode::parse(t, &wt_parse_options())
                .unwrap()
                .to_serde_value()
                .unwrap()["schemes"]
                .clone()
        };
        let e = apply_edit(text, &edit(&s, &p, false)).unwrap();
        assert_eq!(
            schemes(&e.text),
            json!([s.clone(), { "name": "Campbell" }, s.clone()])
        );
        assert_eq!(e.text.matches("\"foreground\"").count(), 2);
        assert_eq!(
            apply_edit(&e.text, &edit(&s, &p, false)).unwrap().text,
            e.text
        );
        let back = revert(&e.text, &originals(&e), &written(&e), &e.created).unwrap();
        assert_eq!(schemes(&back), json!([{ "name": "Campbell" }]));
    }

    #[test]
    fn revert_gives_back_crlf_and_bom_bytes_too() {
        let (s, p) = (scheme(), profile());
        for (name, fixture) in [
            ("default", DEFAULT),
            ("commented", COMMENTED),
            ("legacy", LEGACY),
            ("nocmd", NO_CMD),
        ] {
            let crlf = format!("\u{feff}{}", fixture.replace('\n', "\r\n"));
            let e = apply_edit(&crlf, &edit(&s, &p, true)).unwrap();
            assert!(
                !e.text.replace("\r\n", "").contains('\n'),
                "{name}: a bare LF slipped in"
            );
            let back = revert(&e.text, &originals(&e), &written(&e), &e.created).unwrap();
            assert_eq!(back, crlf, "{name}");
        }
    }

    #[test]
    fn reapplying_changes_nothing_on_any_fixture() {
        let (s, p) = (scheme(), profile());
        for (name, fixture) in [
            ("commented", COMMENTED),
            ("nocmd", NO_CMD),
            ("legacy", LEGACY),
            ("usercmd", USER_CMD),
            ("empty", "{}\n"),
        ] {
            let once = apply_edit(fixture, &edit(&s, &p, true)).unwrap().text;
            let twice = apply_edit(&once, &edit(&s, &p, true)).unwrap();
            assert_eq!(twice.text, once, "{name}");
            assert!(twice.created.is_empty(), "{name}");
        }
    }

    #[test]
    fn revert_keeps_a_font_key_the_user_changed_after_us() {
        let (s, p) = (scheme(), profile());
        let e = apply_edit(USER_CMD, &edit(&s, &p, false)).unwrap();
        let user_edit = e
            .text
            .replace("\"face\": \"Cascadia Mono\"", "\"face\": \"Fira Code\"");
        assert_ne!(user_edit, e.text);
        let back = revert(&user_edit, &originals(&e), &written(&e), &e.created).unwrap();
        let font = |sub: &str| {
            read_value(
                &back,
                &WtKey::ProfileSub {
                    key: "font".into(),
                    sub: sub.into(),
                },
            )
            .unwrap()
        };
        assert_eq!(font("face"), Some(json!("Fira Code")));
        assert_eq!(font("weight"), Some(json!("bold")));
        assert_eq!(font("size"), None);
    }

    #[test]
    fn revert_everything_after_two_themes_gives_back_the_original() {
        // "Revert everything" passes the first original and the last write of each
        // key over several applies. The first apply writes `font` whole; the next,
        // finding that object, records `font.face` and `font.size`. The sub-keys
        // must go back first, so that `font` then holds what was written first.
        let s = scheme();
        let p1 = profile();
        let mut p2 = profile();
        p2.insert(
            "font".into(),
            json!({ "face": "CaskaydiaCove NF", "size": 14 }),
        );
        for (name, fixture) in [("default", DEFAULT), ("nocmd", NO_CMD)] {
            let e1 = apply_edit(fixture, &edit(&s, &p1, true)).unwrap();
            let e2 = apply_edit(&e1.text, &edit(&s, &p2, true)).unwrap();
            let mut first = BTreeMap::new();
            let mut last = BTreeMap::new();
            for c in e1.changes.iter().chain(&e2.changes) {
                first
                    .entry(c.key.clone())
                    .or_insert_with(|| c.original.clone());
                last.insert(c.key.clone(), c.written.clone());
            }
            let back = revert(&e2.text, &first, &last, &e1.created).unwrap();
            assert_eq!(back, fixture, "{name}");
        }
    }

    #[test]
    fn a_scheme_that_is_not_an_object_is_refused() {
        let p = profile();
        let not_an_object = json!("CMD Studio");
        assert!(matches!(
            apply_edit(DEFAULT, &edit(&not_an_object, &p, false)),
            Err(WtError::Shape(_))
        ));
    }
}
