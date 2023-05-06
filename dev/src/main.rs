#[allow(clippy::redundant_closure)]
fn main() -> io::Result<()> {
    match &*env::args().nth(1).unwrap_or_else(|| help()) {
        "test-miri" => test_miri(),
        _ => help(),
    }
}

fn test_miri() -> io::Result<()> {
    const RUSTFLAGS: &str = "-Zrandomize-layout";
    const MIRIFLAGS: &str = concat!(
        "-Zmiri-symbolic-alignment-check ",
        "-Zmiri-strict-provenance"
    );

    // Don't bother using the CARGO env var, since we need to enable Nightly
    let status = process::Command::new("cargo")
        .arg("+nightly")
        .arg("miri")
        .arg("test")
        .env("RUSTFLAGS", RUSTFLAGS)
        .env("MIRIFLAGS", MIRIFLAGS)
        .status()?;

    process::exit(status.code().unwrap_or(1))
}

fn help() -> ! {
    eprintln!("Helper for developing rofi-mode.rs");
    eprintln!();
    eprintln!("SUBCOMMANDS:");
    eprintln!("    test-miri    Runs all the tests using Miri");
    process::exit(1)
}

use std::env;
use std::io;
use std::process;
