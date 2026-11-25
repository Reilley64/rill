#[cfg(target_os = "macos")]
pub const NATIVE_LIBS: &[&str] = &["-lSystem", "-lc", "-lm"];

#[cfg(target_os = "linux")]
pub const NATIVE_LIBS: &[&str] = &[
    "-lgcc_s",
    "-lutil",
    "-lrt",
    "-lpthread",
    "-lm",
    "-ldl",
    "-lc",
];
