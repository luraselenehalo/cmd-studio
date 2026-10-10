# G9 — Edit Windows Terminal's settings.json (JSONC) and keep comments

- **Date:** 2026-10-10
- **Where it ran:** dev machine (Windows 11 Home 26300, AtlasOS); Rust 1.99.0
- **Result:** ✅ PASS

## Crate

`jsonc-parser = { version = "=0.34.0", features = ["cst"] }` (latest on crates.io on 2026-10-10, from `cargo search jsonc-parser`).

## API, read from the crate source (`src/cst/mod.rs`, `src/cst/input.rs`, `src/lib.rs`)

| Need | Signature in 0.34.0 |
|---|---|
| parse | `CstRootNode::parse(text: &str, parse_options: &ParseOptions) -> Result<CstRootNode, ParseError>` |
| root object | `CstRootNode::object_value_or_set(&self) -> CstObject` (also `object_value`, `object_value_or_create`) |
| child object / array by name | `CstObject::object_value(&self, name: &str) -> Option<CstObject>`, `CstObject::array_value(&self, name: &str) -> Option<CstArray>`, `CstObject::array_value_or_set(&self, name: &str) -> CstArray` |
| property | `CstObject::get(&self, name: &str) -> Option<CstObjectProp>`, `CstObjectProp::value(&self) -> Option<CstNode>`, `CstObjectProp::set_value(&self, CstInputValue)` |
| add | `CstObject::append(&self, prop_name: &str, value: CstInputValue) -> CstObjectProp`, `CstObject::insert(&self, index, name, value)`, `CstArray::append(&self, CstInputValue) -> CstNode` |
| iterate | `CstArray::elements(&self) -> Vec<CstNode>`, `CstNode::as_object(&self) -> Option<CstObject>` |
| read a string | `CstNode::as_string_lit(&self) -> Option<CstStringLit>`, `CstStringLit::decoded_value(&self) -> Result<String, ParseStringErrorKind>` |
| replace / remove | `replace_with(self, CstInputValue) -> Option<CstNode>` and `remove(self)` on nodes and props |
| print | `impl Display for CstRootNode` (so `root.to_string()`) |
| values | `jsonc_parser::json!` macro (`#[macro_export]`) builds `CstInputValue`; `From` impls exist for bool, &str, String, numbers and `Vec<(String, CstInputValue)>`; **there is no `From<serde_json::Value>`** |
| validate | `jsonc_parser::parse_to_value(text, &ParseOptions) -> Result<Option<JsonValue>, ParseError>` |

The method names the plan remembered all exist with the expected shapes, so no code changes were needed.

## Test 1: fixture with comments, trailing comma and user formatting

Command: `cargo test --manifest-path spikes/jsonc-cst/Cargo.toml -- --nocapture`

Edited output (exact):
```jsonc
// User's own comment at the top
{
    "$schema": "https://aka.ms/terminal-profiles-schema",
    "defaultProfile": "{61c54bbd-c2c6-5271-96e7-009a87ff44bf}",
    "profiles":
    {
        "defaults": {},
        "list":
        [
            {
                "guid": "{61c54bbd-c2c6-5271-96e7-009a87ff44bf}",
                "name": "Windows PowerShell" // inline comment
            },
            {
                "commandline": "%SystemRoot%\\System32\\cmd.exe",
                "guid": "{0caa0dad-35be-5f56-a8ff-afceeeaa6101}",
                "hidden": false,
                "name": "Command Prompt",
                "colorScheme": "CMD Studio",
            }
        ]
    },
    /* block comment the user wrote */
    "schemes": [
        {
            "name": "CMD Studio",
            "background": "#1f1e1d"
        }
    ]
}
```
`test tests::edits_keep_comments_and_other_bytes ... ok`

What this shows:
- The line comment, the inline comment and the block comment all survive.
- The user's layout is untouched: 4-space indent, and braces and brackets on their own lines.
- The new property follows the object's existing trailing-comma style.
- The empty `[]` became a properly indented block.

## Test 2: the real settings.json round-trips byte for byte (read only)

Command: `G9_REAL_SETTINGS=<WT settings.json> cargo test -- --nocapture --test-threads 1`

Output: `REAL FILE OK: 3158 bytes, starts with BOM: false` and `test tests::real_settings_file_round_trips_byte_for_byte ... ok` (it ran; it was not skipped).

The file is WT 1.24's settings on the dev machine, which already holds the prototype's "Claude" scheme. It was never written.

## Decision

Use **jsonc-parser 0.34.0 with the `cst` feature** for the Phase 1b `WtSettings` adapter. A hand-written byte editor is not needed.

## What the Phase 1b adapter must handle (not covered here)

1. **Conversion:** write `fn to_cst(&serde_json::Value) -> CstInputValue`, because our generators return `serde_json::Value` and the crate has no conversion.
2. **Missing `schemes`:** `array_value_or_set("schemes")` creates the key. That comes from the API; test it in 1b.
3. **Existing `CMD Studio` scheme:** find it by `name` and use `replace_with`. Never append a second one.
4. **Missing cmd profile** (guid `{0caa0dad-…}`): append a new profile object. Test it in 1b.
5. **Restoring removed keys:** when restoring a key that did not exist before, `remove()` the prop.
6. **Untested shapes:** a file with a UTF-8 BOM, a file that fails to parse (must give `E_WT_PARSE` without writing), and the settings files of WT Preview and the unpackaged WT.
