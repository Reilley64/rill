use std::{
    fs,
    path::{Path, PathBuf},
};

pub(crate) const TARGET: &str = env!("RILL_HOST_TARGET");

pub(crate) struct Installation {
    pub(crate) root: PathBuf,
}

impl Installation {
    pub(crate) fn stage(test: &str) -> Self {
        let root = std::env::temp_dir().join(format!("rill-cli-{test}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let std = root.join("lib/rill/std");
        let runtime = root.join("lib/rill/runtime").join(TARGET);
        for directory in [&root.join("bin"), &root.join("app"), &std, &runtime] {
            fs::create_dir_all(directory).unwrap();
        }
        rill_test_support::stage_executable(
            Path::new(env!("CARGO_BIN_EXE_rill")),
            &root.join("bin/rill"),
        )
        .unwrap();
        for name in include_str!("../../../../tests/installed/typed/stdlib-sources.txt")
            .lines()
            .chain(include_str!("../../../../tests/installed/typed/routine-sources.txt").lines())
            .chain(include_str!("../../../../tests/installed/typed/append-sources.txt").lines())
        {
            fs::create_dir_all(std.join(name).parent().unwrap()).unwrap();
            fs::copy(repo.join("stdlib").join(name), std.join(name)).unwrap();
        }
        fs::copy(
            repo.join("runtime/error/src/lib.rs"),
            runtime.join("error.rs"),
        )
        .unwrap();
        fs::write(
            runtime.join("librill_ownership.rlib"),
            compiler::loader::pinned_ownership_archive(),
        )
        .unwrap();
        Self { root }
    }
}

impl Drop for Installation {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
