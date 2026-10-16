//! The stable codes of spec §5.9 and the code agree: every `E_...` code that the production code
//! of the core, the CLI and the app says is a row of the table in §5.9, and every row is said
//! somewhere. The app's own table of those meanings (`const MEANINGS` in `error.rs`) counts for
//! the first half only: it lists every row by construction, so it cannot show that a row is used.
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

/// `source` without the app's table of meanings, the lines from one that starts `const MEANINGS`
/// to the next that is `];`, and whether it had one. The table holds every row of §5.9 by
/// construction (a test of the app compares it with the spec), so it can show that a code is a
/// row but never that a row is used.
fn without_meanings_table(source: &str) -> (String, bool) {
    let mut rest = String::new();
    let (mut found, mut in_table) = (false, false);
    for line in source.lines() {
        if line.starts_with("const MEANINGS") {
            (found, in_table) = (true, true);
        }
        if !in_table {
            rest.push_str(line);
            rest.push('\n');
        } else if line == "];" {
            in_table = false;
        }
    }
    assert!(!in_table, "`const MEANINGS` has no closing `];` line");
    (rest, found)
}

#[test]
fn the_codes_of_spec_5_9_and_the_codes_the_code_says_are_the_same() {
    let mut files = Vec::new();
    for krate in ["core", "cli"] {
        rust_files(&root().join("crates").join(krate).join("src"), &mut files);
    }
    // The app (Phase 2b) says codes too: its own E_INTERNAL, and the rows it quotes.
    rust_files(
        &root().join("app").join("src-tauri").join("src"),
        &mut files,
    );
    // The in-memory adapters are for tests only.
    files.retain(|f| !f.ends_with(Path::new("adapters").join("fake.rs")));
    assert!(files.len() > 20, "{files:?}");
    // `said`: every code any file says, the app's table of meanings included (each must be a row
    // of §5.9). `used`: the codes said outside that table (each row must be one of them).
    let (mut said, mut used) = (BTreeSet::new(), BTreeSet::new());
    let mut tables = Vec::new();
    for file in &files {
        let source = std::fs::read_to_string(file).unwrap();
        said.extend(codes_in(&source));
        let (rest, has_table) = without_meanings_table(&source);
        if has_table {
            tables.push(file.file_name().unwrap().to_string_lossy().into_owned());
        }
        used.extend(codes_in(&rest));
    }
    // If the table were renamed the exclusion would quietly stop, and the guard with it.
    assert_eq!(tables, ["error.rs"], "where `const MEANINGS` is found");
    let spec = spec_codes();
    assert!(!spec.is_empty());
    let not_in_spec: Vec<_> = said.difference(&spec).collect();
    let never_said: Vec<_> = spec.difference(&used).collect();
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

#[test]
fn a_code_only_in_the_table_of_meanings_is_said_but_not_used() {
    let source = r#"
pub const INTERNAL: &str = "E_DEFINED_OUTSIDE";
const MEANINGS: &[(&str, &str, &str)] = &[
    ("E_ONLY_IN_THE_TABLE", "th", "en"),
    ("E_DEFINED_OUTSIDE", "th", "en"),
];
fn a() -> &'static str { "E_USED_AFTER" }
#[cfg(test)]
mod tests { const X: &str = "E_IN_A_TEST"; }
"#;
    let (rest, found) = without_meanings_table(source);
    assert!(found);
    // In the table or not, each code is said: each must be a row of §5.9.
    let said: Vec<String> = codes_in(source).into_iter().collect();
    assert_eq!(
        said,
        ["E_DEFINED_OUTSIDE", "E_ONLY_IN_THE_TABLE", "E_USED_AFTER"]
    );
    // Outside the table: the one only the table names is not used.
    let used: Vec<String> = codes_in(&rest).into_iter().collect();
    assert_eq!(used, ["E_DEFINED_OUTSIDE", "E_USED_AFTER"]);
    // A file with no table is returned as it is, lines and all.
    let (same, found) = without_meanings_table("fn a() { \"E_EXACT\" }\nfn b() {}\n");
    assert!(!found);
    assert_eq!(same, "fn a() { \"E_EXACT\" }\nfn b() {}\n");
}
