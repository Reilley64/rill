//! Capture the first-party installation without taking authority from a request.
use super::*;
use std::path::Path;

mod collection;
pub use collection::CollectionArtifact;
pub(super) use collection::CollectionLinks;

pub(super) const OWNERSHIP_ARCHIVE: Option<&[u8]> =
    Some(include_bytes!(env!("RILL_OWNERSHIP_ARCHIVE")));

pub(super) fn is_ownership_source(path: &str) -> bool {
    matches!(path, "shared.rill" | "mutex.rill" | "channel.rill")
}

pub(super) fn observe_ownership_archive(
    host: &mut Host,
    prefix: &Path,
) -> Result<Option<(PathBuf, Vec<u8>)>> {
    if OWNERSHIP_ARCHIVE.is_none() {
        return Ok(None);
    }
    let path = prefix.join("lib/rill/runtime/aarch64-apple-darwin/librill_ownership.rlib");
    let Some(probe) = host.probe(&path, Some(prefix))? else {
        return Ok(None);
    };
    if probe.kind != filesystem::Kind::File || probe.path != path {
        let mut error = failure(
            FailureKind::InvalidRequest,
            "ownership archive requires its exact installed path",
        );
        error.path = Some(path);
        return Err(error);
    }
    let bytes = host.read(&path, true)?;
    host.charge(bytes.len())?;
    Ok(Some((path, bytes)))
}

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

    pub(super) fn metadata(self) -> &'static [MetadataEdge] {
        match self {
            Self::DarwinArm64 => &[
                MetadataEdge {
                    path: "option.rill",
                    dependencies: &[],
                    registrations: &[],
                },
                MetadataEdge {
                    path: "collection.rill",
                    dependencies: &[("option", "option.rill")],
                    registrations: &[
                        ("collection.rill", &["get", "first"]),
                        ("core.rill", &["copy", "dispose"]),
                    ],
                },
                MetadataEdge {
                    path: "iter.rill",
                    dependencies: &[("option", "option.rill"), ("collection", "collection.rill")],
                    registrations: &[
                        ("iter.rill", &["iterate", "next"]),
                        ("core.rill", &["dispose"]),
                    ],
                },
                MetadataEdge {
                    path: "format.rill",
                    dependencies: &[],
                    registrations: &[("format.rill", &["show"])],
                },
                MetadataEdge {
                    path: "string.rill",
                    dependencies: &[("option", "option.rill"), ("error", "error.rill")],
                    registrations: &[("error.rill", &["display"])],
                },
                MetadataEdge {
                    path: "bytes.rill",
                    dependencies: &[
                        ("collection", "collection.rill"),
                        ("convert", "convert.rill"),
                        ("result", "result.rill"),
                        ("string", "string.rill"),
                    ],
                    registrations: &[("convert.rill", &["into", "tryInto"])],
                },
            ],
        }
    }

    pub(super) fn error_storage(self) -> (&'static str, &'static [u8]) {
        match self {
            Self::DarwinArm64 => (
                "runtime/aarch64-apple-darwin/error.rs",
                include_bytes!("../../../../runtime/error/src/lib.rs"),
            ),
        }
    }

    pub(super) fn source(self, path: &str) -> Option<&'static [u8]> {
        // Explicit import discovery can capture this extension. It is not a
        // member of the baseline required by every admitted role.
        if path == "routine.rill" {
            return Some(include_bytes!("../../../../stdlib/routine.rill"));
        }
        self.sources()
            .iter()
            .find_map(|&(key, bytes)| (path == key).then_some(bytes))
    }

    pub(super) fn sources(self) -> &'static [(&'static str, &'static [u8])] {
        macro_rules! pins {
            ($($name:literal),* $(,)?) => {
                &[$(($name, include_bytes!(concat!("../../../../stdlib/", $name)))),*]
            };
        }
        match self {
            Self::DarwinArm64 => pins![
                "std.rill",
                "core.rill",
                "convert.rill",
                "number.rill",
                "char.rill",
                "bytes.rill",
                "string.rill",
                "format.rill",
                "compare.rill",
                "option.rill",
                "result.rill",
                "error.rill",
                "shared.rill",
                "mutex.rill",
                "channel.rill",
                "collection.rill",
                "iter.rill",
            ],
        }
    }
}

pub(super) struct MetadataEdge {
    pub path: &'static str,
    pub dependencies: &'static [(&'static str, &'static str)],
    pub registrations: &'static [(&'static str, &'static [&'static str])],
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
        Some(executable),
    )
}
