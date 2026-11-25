use std::{env, path::PathBuf, process::Command};

const NATIVE_TARGETS: [&str; 4] = [
    "x86_64-unknown-linux-gnu",
    "aarch64-unknown-linux-gnu",
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
];

fn main() {
    let source = "../../runtime/ownership";
    println!("cargo:rerun-if-changed={source}");
    let target = env::var("TARGET").unwrap();
    assert!(
        NATIVE_TARGETS.contains(&target.as_str()),
        "the installed analysis profile covers {NATIVE_TARGETS:?}, not {target}"
    );
    let archive = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("librill_ownership.rlib");
    let rustc = env::var("RUSTC").unwrap();
    let result = Command::new(&rustc)
        .args([
            "--edition=2024",
            "--crate-name=rill_ownership",
            "--crate-type=rlib",
            "--target",
            &target,
            "-Dwarnings",
            "-o",
        ])
        .arg(&archive)
        .arg(format!("{source}/lib.rs"))
        .status()
        .expect("run selected rustc for the first-party ownership archive");
    assert!(result.success(), "build the first-party ownership archive");
    println!(
        "cargo:rustc-env=RILL_OWNERSHIP_ARCHIVE={}",
        archive.display()
    );
    println!("cargo:rustc-env=RILL_ANALYSIS_TARGET={target}");
    println!("cargo:rustc-env=RILL_OWNERSHIP_RUSTC={rustc}");
}
