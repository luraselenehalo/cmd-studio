//! JSON Schema for `.cmdtheme` files, generated from the Rust types so the
//! published file can never drift from the parser (spec §3.3).

/// Pretty-printed schema text, ending with a newline.
///
/// The text is the same on every run, so `schema/cmdtheme-1.schema.json` can be compared with it
/// byte for byte. That relies on `serde_json` being built without its `preserve_order` feature:
/// with it on, `schemars` writes properties in declaration order instead of alphabetical order.
/// The schema would say the same, but the committed file would no longer match its text.
pub fn theme_schema_json() -> Result<String, serde_json::Error> {
    let schema = schemars::schema_for!(crate::model::Theme);
    let mut text = serde_json::to_string_pretty(&schema)?;
    text.push('\n');
    Ok(text)
}
