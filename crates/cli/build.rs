fn main() {
    println!(
        "cargo:rustc-env=RILL_HOST_TARGET={}",
        std::env::var("TARGET").expect("cargo sets TARGET for build scripts")
    );
}
