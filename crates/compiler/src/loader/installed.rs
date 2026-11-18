//! Capture the first-party installation without taking authority from a request.
use super::*;
use std::path::Path;

#[derive(Clone, Debug)]
pub struct InstalledRequest {
    pub entry: PathBuf,
    pub target: String,
    pub options: BTreeMap<String, String>,
    pub test_mode: bool,
    pub limits: Limits,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum InstalledProfile {
    DarwinArm64,
}

impl InstalledProfile {
    fn select(request: &InstalledRequest) -> Option<Self> {
        (request.target == "aarch64-apple-darwin" && request.options.is_empty())
            .then_some(Self::DarwinArm64)
    }

    pub(super) fn question_mark_dependencies(self) -> &'static [&'static str] {
        match self {
            Self::DarwinArm64 => &["option.rill"],
        }
    }

    pub(super) fn source(self, path: &str) -> Option<&'static [u8]> {
        match (self, path) {
            (Self::DarwinArm64, "core.rill") => {
                Some(include_bytes!("../../../../stdlib/core.rill"))
            }
            (Self::DarwinArm64, "option.rill") => {
                Some(include_bytes!("../../../../stdlib/option.rill"))
            }
            _ => None,
        }
    }
}

/// Load using the standard root beside the running toolchain executable.
/// Unsupported profiles can load source but receive no first-party evidence.
pub fn load_installed(input: InstalledRequest) -> Result<Graph> {
    let executable = std::env::current_exe()
        .and_then(std::fs::canonicalize)
        .map_err(|_| {
            failure(
                FailureKind::InputFailure,
                "cannot locate the running toolchain executable",
            )
        })?;
    let prefix = executable
        .parent()
        .filter(|directory| directory.file_name().is_some_and(|name| name == "bin"))
        .and_then(Path::parent)
        .ok_or_else(|| {
            filesystem::host_error(
                &executable,
                "toolchain executable is outside an installed bin directory",
            )
        })?;
    let profile = InstalledProfile::select(&input);
    let selector = |name: &str| Selector {
        module: "core.rill".into(),
        name: name.into(),
    };
    resolve::load(
        Request {
            entry: input.entry,
            standard_root: prefix.join("lib/rill/std"),
            prelude: Prelude {
                copy: selector("copy"),
                dispose: selector("dispose"),
            },
            target: input.target,
            options: input.options,
            test_mode: input.test_mode,
            limits: input.limits,
        },
        profile,
    )
}
