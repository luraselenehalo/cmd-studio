//! The stable codes of spec §5.9 and the code agree: every `E_...` code that the production code
//! of the core and the CLI says is a row of the table in §5.9, and every row is said somewhere.
//! The table also holds each code's Thai meaning, for the app's own UI; the CLI prints the code
//! and English text only.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

/// The codes in the table of spec §5.9: its rows start with "| `E_".
fn spec_codes() -> BTreeSet<String> {
    let spec = root()
        .join("docs")
        .join("superpowers")
        .join("specs")
        .join("2026-10-10-cmd-studio-design.md");
    let spec = std::fs::read_to_string(spec).unwrap();
    let rows: Vec<String> = spec
        .lines()
        .filter_map(|l| l.strip_prefix("| `E_"))
        .filter_map(|rest| rest.split('`').next())
        .map(|name| format!("E_{name}"))
        .collect();
    let codes: BTreeSet<String> = rows.iter().cloned().collect();
    assert_eq!(codes.len(), rows.len(), "a code has two rows: {rows:?}");
    codes
}

/// Every `.rs` file under `dir`, in order.
fn rust_files(dir: &Path, found: &mut Vec<PathBuf>) {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            rust_files(&path, found);
        } else if path.extension().is_some_and(|e| e == "rs") {
            found.push(path);
        }
    }
}

/// The codes a file's production code says: a string literal that starts with `E_` and capital
/// letters, such as `"E_BUSY"` or `"E_THEME_INVALID: ..."`. The file's tests (from its first
/// `#[cfg(test)]` on) and comment lines do not count.
fn codes_in(source: &str) -> BTreeSet<String> {
    let mut codes = BTreeSet::new();
    for line in source.lines() {
        if line.trim_start() == "#[cfg(test)]" {
            break;
        }
        if line.trim_start().starts_with("//") {
            continue;
        }
        for (at, _) in line.match_indices("\"E_") {
            let code: String = line
                .get(at + 1..)
                .unwrap_or_default()
                .chars()
                .take_while(|c| c.is_ascii_uppercase() || *c == '_')
                .collect();
            let code = code.trim_end_matches('_');
            if code.len() > "E_".len() {
                codes.insert(code.to_string());
            }
        }
    }
    codes
}

#[test]
fn the_codes_of_spec_5_9_and_the_codes_the_code_says_are_the_same() {
    let mut files = Vec::new();
    for krate in ["core", "cli"] {
        rust_files(&root().join("crates").join(krate).join("src"), &mut files);
    }
    // The in-memory adapters are for tests only.
    files.retain(|f| !f.ends_with(Path::new("adapters").join("fake.rs")));
    assert!(files.len() > 20, "{files:?}");
    let mut said = BTreeSet::new();
    for file in &files {
        said.extend(codes_in(&std::fs::read_to_string(file).unwrap()));
    }
    let spec = spec_codes();
    assert!(!spec.is_empty());
    let not_in_spec: Vec<_> = said.difference(&spec).collect();
    let never_said: Vec<_> = spec.difference(&said).collect();
    assert!(
        not_in_spec.is_empty() && never_said.is_empty(),
        "codes the code says that §5.9 does not list: {not_in_spec:?}; \
         codes §5.9 lists that the code never says: {never_said:?}"
    );
}

#[test]
fn the_reader_finds_codes_in_literals_and_not_in_comments_or_tests() {
    let source = r#"
// "E_IN_A_COMMENT" is not said
fn a() -> &'static str { "E_EXACT" }
fn b() -> String { format!("E_PREFIX: {}", 1) }
fn c() -> &'static str { "ME_NOT_A_CODE E_" }
fn d() -> &'static str { "E_" }
#[cfg(test)]
mod tests { const X: &str = "E_IN_A_TEST"; }
"#;
    let codes: Vec<String> = codes_in(source).into_iter().collect();
    assert_eq!(codes, ["E_EXACT", "E_PREFIX"]);
}
