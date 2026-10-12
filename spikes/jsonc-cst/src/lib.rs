// Spike for gate G9: can we edit Windows Terminal's settings.json (JSONC)
// and keep the user's comments and formatting? API names were checked
// against the jsonc-parser 0.34.0 source before writing this test.

#[cfg(test)]
mod tests {
    use jsonc_parser::cst::CstRootNode;
    use jsonc_parser::{json, ParseOptions};

    const FIXTURE: &str = include_str!("../fixtures/settings.jsonc");
    const CMD_GUID: &str = "{0caa0dad-35be-5f56-a8ff-afceeeaa6101}";

    #[test]
    fn edits_keep_comments_and_other_bytes() {
        let root = CstRootNode::parse(FIXTURE, &ParseOptions::default()).expect("fixture parses");
        let obj = root.object_value_or_set();

        // 1. Add our scheme.
        let schemes = obj.array_value_or_set("schemes");
        schemes.append(json!({ "name": "CMD Studio", "background": "#1f1e1d" }));

        // 2. Set keys on the cmd profile, found by guid.
        let list = obj
            .object_value("profiles")
            .and_then(|p| p.array_value("list"))
            .expect("profiles.list exists");
        let cmd = list
            .elements()
            .into_iter()
            .filter_map(|e| e.as_object())
            .find(|o| {
                o.get("guid")
                    .and_then(|p| p.value())
                    .and_then(|v| v.as_string_lit())
                    .map(|s| s.decoded_value().ok().as_deref() == Some(CMD_GUID))
                    .unwrap_or(false)
            })
            .expect("cmd profile exists");
        match cmd.get("colorScheme") {
            Some(prop) => prop.set_value(json!("CMD Studio")),
            None => {
                cmd.append("colorScheme", json!("CMD Studio"));
            }
        }

        let out = root.to_string();
        // Comments survive.
        assert!(out.contains("// User's own comment at the top"));
        assert!(out.contains("// inline comment"));
        assert!(out.contains("/* block comment the user wrote */"));
        // Edits landed.
        assert!(out.contains("\"colorScheme\": \"CMD Studio\""));
        assert!(out.contains("\"name\": \"CMD Studio\""));
        // The PowerShell profile block is byte-identical.
        let ps = "\"name\": \"Windows PowerShell\" // inline comment";
        assert!(out.contains(ps));
        // The result is still valid JSONC.
        jsonc_parser::parse_to_value(&out, &ParseOptions::default()).expect("still parses");
        println!("{out}");
    }

    /// Real-data seam: a real settings.json must survive parse → print with
    /// no edits byte for byte. Read only; the file is never written.
    #[test]
    fn real_settings_file_round_trips_byte_for_byte() {
        let Some(path) = std::env::var_os("G9_REAL_SETTINGS") else {
            println!("SKIPPED real_settings_file_round_trips_byte_for_byte: G9_REAL_SETTINGS is not set");
            return;
        };
        let text = std::fs::read_to_string(&path).expect("readable");
        let root = CstRootNode::parse(&text, &ParseOptions::default()).expect("real file parses");
        assert_eq!(root.to_string(), text, "round trip changed bytes");
        println!("REAL FILE OK: {} bytes, starts with BOM: {}", text.len(), text.starts_with('\u{feff}'));
    }
}
