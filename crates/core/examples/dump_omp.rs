//! Prints the Terracotta Oh My Posh config, so that the real oh-my-posh binary can render it.
//!
//! With one argument it writes the config to that path instead; the path is the only place
//! it ever writes to, and it picks none of its own:
//!
//! ```text
//! cargo run -p cmdstudio-core --example dump_omp -- target/terracotta.omp.json
//! ```
use std::error::Error;
use std::io::Write;

fn main() -> Result<(), Box<dyn Error>> {
    let (_, theme) = cmdstudio_core::presets::builtin()?
        .into_iter()
        .find(|(key, _)| *key == "terracotta")
        .ok_or("the terracotta preset is not built in")?;
    let text = cmdstudio_core::generate::omp::config_text(&theme)?;

    let mut args = std::env::args_os().skip(1);
    match (args.next(), args.next()) {
        (None, _) => std::io::stdout().write_all(text.as_bytes())?,
        (Some(path), None) => std::fs::write(path, text)?,
        (Some(_), Some(_)) => return Err("usage: dump_omp [output-path]".into()),
    }
    Ok(())
}
