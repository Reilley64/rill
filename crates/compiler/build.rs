use std::{env, path::PathBuf, process::Command};

fn main() {
    let source = "../../runtime/ownership";
    println!("cargo:rerun-if-changed={source}");
    let target = env::var("TARGET").unwrap();
    if target != "aarch64-apple-darwin" {
        return;
    }

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
    println!("cargo:rustc-env=RILL_OWNERSHIP_RUSTC={rustc}");
}
