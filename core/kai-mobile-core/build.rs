//! Reads the repository's LMGW_PIN (three lines, `lmgw_commit`, `lmgw_tag`, `lmgw_tree`) into
//! compile-time constants: what the About page shows and what a bug report names. A build whose
//! pin is missing or malformed fails here, so no APK carries an unknown source.

use std::{env, fs, path::PathBuf};

fn main() {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let pin = manifest.join("../../LMGW_PIN");
    println!("cargo:rerun-if-changed={}", pin.display());
    let text =
        fs::read_to_string(&pin).unwrap_or_else(|e| panic!("cannot read {}: {e}", pin.display()));
    let field = |key: &str| -> String {
        text.lines()
            .filter(|l| !l.trim_start().starts_with('#'))
            .find_map(|l| {
                let mut words = l.split_whitespace();
                (words.next() == Some(key))
                    .then(|| words.next().map(str::to_owned))
                    .flatten()
            })
            .unwrap_or_else(|| panic!("LMGW_PIN has no {key}"))
    };
    for (key, var) in [
        ("lmgw_commit", "KAI_LMGW_COMMIT"),
        ("lmgw_tag", "KAI_LMGW_TAG"),
        ("lmgw_tree", "KAI_LMGW_TREE"),
    ] {
        println!("cargo:rustc-env={var}={}", field(key));
    }
}
