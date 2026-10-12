use cmdstudio_core::color::{contrast_ratio, Rgb};
use cmdstudio_core::palette::{checked_slots, generate, Tone};
use proptest::prelude::*;
use proptest::test_runner::{Config, FileFailurePersistence, TestRunner};
use std::sync::atomic::{AtomicU32, Ordering};

/// Spec §7 asks for at least 1000 random palettes.
const CASES: u32 = 1000;

// libtest never prints how many cases a property test ran, so this test drives the runner itself
// and counts them. It avoids the `proptest!` macro because the macro lets the PROPTEST_CASES
// variable replace the configured case count without a word.
#[test]
fn generated_palettes_meet_every_threshold() {
    let executed = AtomicU32::new(0);
    let mut runner = TestRunner::new(Config {
        cases: CASES,
        // The seed of a failing case is saved in palette_props.proptest-regressions, next to this
        // file, and replayed first on every later run. Nothing is written unless a case fails.
        source_file: Some(file!()),
        ..Config::with_failure_persistence(FileFailurePersistence::WithSource(
            "proptest-regressions",
        ))
    });
    let result = runner.run(
        &(0u8..=255, 0u8..=255, 0u8..=255, any::<bool>(), 0usize..3),
        |(r, g, b, dark, tint_index)| {
            executed.fetch_add(1, Ordering::Relaxed);
            let tint = [0.0, 0.15, 0.30][tint_index];
            let tone = if dark { Tone::Dark } else { Tone::Light };
            let p = generate(tone, Rgb { r, g, b }, tint);
            for (slot, min) in checked_slots() {
                let ratio = contrast_ratio(p.get(slot), p.background);
                prop_assert!(
                    ratio >= min,
                    "{slot:?} {ratio:.2} < {min} for #{r:02x}{g:02x}{b:02x} {tone:?} tint {tint}"
                );
            }
            Ok(())
        },
    );
    if let Err(e) = result {
        panic!("{e}\n{runner}");
    }
    // `run` returns Ok only after CASES passing cases. It also replays any seeds saved from
    // earlier failures first, and those reach the closure too, so the count can only be higher.
    let executed = executed.load(Ordering::Relaxed);
    println!("palette property test: {executed} cases executed");
    assert!(executed >= CASES, "only {executed} of {CASES} cases ran");
}
