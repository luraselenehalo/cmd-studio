//! JSON Schema for `.cmdtheme` files, generated from the Rust types so the
//! published file can never drift from the parser (spec §3.3).
use schemars::generate::SchemaSettings;
use schemars::transform::RestrictFormats;

/// Pretty-printed schema text, ending with a newline.
///
/// The schema is draft 2020-12, named here so that a new default of `schemars` cannot change
/// it, and only the `format` values that standard defines are kept (`uuid`, for the id).
/// `schemars` also writes `uint8`, `uint32` and `double` for the number types. JSON Schema does
/// not define those, and a validator that does not know a format may refuse the whole schema
/// (Ajv does by default). The `minimum` and `maximum` of the numbers stay.
///
/// `schemars` marks the optional `$schema` line `writeOnly`, because CMD Studio reads it and
/// does not write it back. In JSON Schema `writeOnly` speaks of a server and its clients, and an
/// editor may hide or flag such a property; in a file a person writes it means nothing, so it is
/// taken out.
///
/// The text is the same on every run, so `schema/cmdtheme-1.schema.json` can be compared with it
/// byte for byte. That relies on `serde_json` being built without its `preserve_order` feature:
/// with it on, `schemars` writes properties in declaration order instead of alphabetical order.
/// The schema would say the same, but the committed file would no longer match its text.
/// `tests/schema_file.rs` checks for that.
pub fn theme_schema_json() -> Result<String, serde_json::Error> {
    let mut schema = SchemaSettings::draft2020_12()
        .with_transform(RestrictFormats::default())
        .into_generator()
        .into_root_schema_for::<crate::model::Theme>();
    // Edited on the `Schema` itself: it writes its keywords in an order of its own, which a
    // trip through `serde_json::Value` would sort.
    if let Some(line) = schema
        .pointer_mut("/properties/$schema")
        .and_then(serde_json::Value::as_object_mut)
    {
        line.remove("writeOnly");
    }
    let mut text = serde_json::to_string_pretty(&schema)?;
    text.push('\n');
    Ok(text)
}
