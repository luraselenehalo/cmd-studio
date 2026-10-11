//! Windows implementations of the adapters (spec §5.2). Each one takes the
//! places it touches as parameters, so tests can aim it at a throwaway
//! registry key and temp folders; `real()` constructors use the user's own.
use super::AdapterError;
use std::path::Path;

pub mod fs;

pub(crate) fn io_err(op: &str, path: &Path, e: &std::io::Error) -> AdapterError {
    AdapterError::new(format!("{op} {}", path.display()), e.to_string())
}
