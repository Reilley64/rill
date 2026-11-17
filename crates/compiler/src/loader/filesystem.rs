//! Exact, bounded local filesystem observations. This is not a sandbox.
use super::{Failure, FailureKind, Finding, Limits, Result, failure};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::ffi::OsString;
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    File,
    Directory,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Probe {
    pub path: PathBuf,
    pub kind: Kind,
}

#[derive(Debug)]
pub(super) struct Host {
    pub limits: Limits,
    pub work: usize,
    probes: usize,
    entries: usize,
    bytes: usize,
    retained: usize,
    observations: BTreeMap<(PathBuf, Option<PathBuf>), Option<Probe>>,
}

impl Host {
    pub fn new(limits: Limits) -> Self {
        Self {
            limits,
            work: 0,
            probes: 0,
            entries: 0,
            bytes: 0,
            retained: 0,
            observations: BTreeMap::new(),
        }
    }

    pub fn charge(&mut self, amount: usize) -> Result<()> {
        add(&mut self.work, amount, self.limits.max_work)
    }

    pub fn probe(&mut self, path: &Path, root: Option<&Path>) -> Result<Option<Probe>> {
        self.charge(path.as_os_str().len())?;
        let path = absolute(path)?;
        let key = (path.clone(), root.map(Path::to_path_buf));
        if let Some(probe) = self.observations.get(&key) {
            return Ok(probe.clone());
        }
        let probe = self.inspect(&path, root)?;
        self.charge(1)?;
        self.observations.insert(key, probe.clone());
        Ok(probe)
    }

    fn inspect(&mut self, path: &Path, root: Option<&Path>) -> Result<Option<Probe>> {
        add(&mut self.probes, 1, self.limits.max_probes)?;
        self.charge(path.as_os_str().len())?;
        let mut pending: VecDeque<_> = components(path)?
            .into_iter()
            .map(|name| (name, false))
            .collect();
        let mut at = PathBuf::from("/");
        let mut links = 0;
        let mut link_states = BTreeSet::new();
        while let Some((name, link_target)) = pending.pop_front() {
            self.charge(1)?;
            let mut exact = false;
            let directory = match fs::read_dir(&at) {
                Ok(directory) => directory,
                Err(error) if error.kind() == std::io::ErrorKind::NotADirectory => return Ok(None),
                Err(error) => return Err(io_error(&at, error)),
            };
            // Do not stop at the match: budget consumption cannot depend on
            // host directory enumeration order.
            for entry in directory {
                add(&mut self.entries, 1, self.limits.max_directory_entries)?;
                self.charge(1)?;
                if entry.map_err(|e| io_error(&at, e))?.file_name() == name {
                    exact = true;
                }
            }
            at.push(&name);
            if !exact {
                if link_target {
                    return Err(host_error(&at, "broken symbolic link"));
                }
                return match fs::symlink_metadata(&at) {
                    Ok(_) => Err(host_error(
                        &at,
                        "host cannot establish exact case/Unicode spelling",
                    )),
                    Err(error)
                        if matches!(
                            error.kind(),
                            std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                        ) =>
                    {
                        Ok(None)
                    }
                    Err(error) => Err(io_error(&at, error)),
                };
            }
            let metadata = fs::symlink_metadata(&at).map_err(|e| io_error(&at, e))?;
            if metadata.file_type().is_symlink() {
                self.charge(pending.len().checked_add(1).ok_or_else(resource)?)?;
                if !link_states.insert((at.clone(), pending.clone())) {
                    return Err(host_error(&at, "filesystem symbolic-link loop"));
                }
                links += 1;
                if links > 64 {
                    return Err(resource());
                }
                let target = fs::read_link(&at).map_err(|e| io_error(&at, e))?;
                let target = if target.is_absolute() {
                    target
                } else {
                    at.parent().unwrap().join(target)
                };
                let target = normalize(&target)?;
                if root.is_some_and(|root| !target.starts_with(root)) {
                    return Err(escape(&at));
                }
                let mut next: VecDeque<_> = components(&target)?
                    .into_iter()
                    .map(|name| (name, true))
                    .collect();
                next.append(&mut pending);
                pending = next;
                at = PathBuf::from("/");
                continue;
            }
            if !pending.is_empty() && !metadata.is_dir() {
                if pending.front().is_some_and(|(_, required)| *required) {
                    return Err(host_error(&at, "broken symbolic-link target"));
                }
                return Ok(None);
            }
        }
        if root.is_some_and(|root| !at.starts_with(root)) {
            return Err(escape(&at));
        }
        let metadata = fs::metadata(&at).map_err(|e| io_error(&at, e))?;
        let kind = if metadata.is_file() {
            Kind::File
        } else if metadata.is_dir() {
            Kind::Directory
        } else {
            Kind::Other
        };
        Ok(Some(Probe { path: at, kind }))
    }

    pub fn read(&mut self, path: &Path, retain: bool) -> Result<Vec<u8>> {
        let metadata = fs::metadata(path).map_err(|e| io_error(path, e))?;
        if !metadata.is_file() {
            return Err(host_error(path, "source is not a regular file"));
        }
        let length = usize::try_from(metadata.len()).map_err(|_| resource())?;
        if length > self.limits.max_source_bytes {
            return Err(resource());
        }
        if retain
            && self
                .retained
                .checked_add(length)
                .is_none_or(|n| n > self.limits.max_total_bytes)
        {
            return Err(resource());
        }
        let cap = self
            .limits
            .max_source_bytes
            .checked_add(1)
            .ok_or_else(resource)?;
        let remaining = self
            .limits
            .max_total_bytes
            .checked_mul(2)
            .and_then(|n| n.checked_sub(self.bytes))
            .ok_or_else(resource)?;
        let limit = cap
            .min(remaining.saturating_add(1))
            .min((self.limits.max_work - self.work).saturating_add(1));
        let file = fs::File::open(path).map_err(|e| io_error(path, e))?;
        if !file.metadata().map_err(|e| io_error(path, e))?.is_file() {
            return Err(host_error(path, "opened source is not a regular file"));
        }
        let mut bytes = Vec::new();
        file.take(u64::try_from(limit).map_err(|_| resource())?)
            .read_to_end(&mut bytes)
            .map_err(|e| io_error(path, e))?;
        if bytes.len() > self.limits.max_source_bytes {
            return Err(resource());
        }
        add(
            &mut self.bytes,
            bytes.len(),
            self.limits
                .max_total_bytes
                .checked_mul(2)
                .ok_or_else(resource)?,
        )?;
        if retain {
            add(&mut self.retained, bytes.len(), self.limits.max_total_bytes)?;
        }
        self.charge(bytes.len())?;
        Ok(bytes)
    }

    pub fn replay<'a>(
        &self,
        limits: Limits,
        sources: impl Iterator<Item = (&'a Path, &'a [u8])>,
        continuing: bool,
    ) -> Result<()> {
        let mut replay = Self::new(limits);
        if continuing {
            replay.work = self.work;
            replay.probes = self.probes;
            replay.entries = self.entries;
            replay.bytes = self.bytes;
            replay.retained = self.retained;
        }
        replay.charge(self.observations.len())?;
        for ((path, root), expected) in &self.observations {
            let actual = replay.inspect(path, root.as_deref()).map_err(|mut error| {
                if error.kind == FailureKind::SourceRejected {
                    error.kind = FailureKind::SourceChanged;
                    error.message = "a previously confined path now escapes its origin".into();
                    error.findings.clear();
                }
                error
            })?;
            if &actual != expected {
                return Err(failure(
                    FailureKind::SourceChanged,
                    "path observation changed",
                ));
            }
        }
        for (path, expected) in sources {
            if replay.read(path, false)? != expected {
                return Err(failure(FailureKind::SourceChanged, "source bytes changed"));
            }
        }
        Ok(())
    }
}

pub(super) fn resource() -> Failure {
    failure(
        FailureKind::ResourceLimit,
        "loader work or storage budget exhausted",
    )
}

pub(super) fn add(counter: &mut usize, amount: usize, maximum: usize) -> Result<()> {
    *counter = counter
        .checked_add(amount)
        .filter(|&n| n <= maximum)
        .ok_or_else(resource)?;
    Ok(())
}

pub(super) fn host_error(path: &Path, message: &str) -> Failure {
    let mut error = failure(FailureKind::InputFailure, message);
    error.path = Some(path.to_path_buf());
    error
}

fn io_error(path: &Path, error: std::io::Error) -> Failure {
    host_error(path, &error.to_string())
}

pub(super) fn escape(path: &Path) -> Failure {
    let mut error = failure(FailureKind::SourceRejected, "path escapes selected origin");
    error.path = Some(path.to_path_buf());
    error.findings.push(Finding {
        code: Some("R0404"),
        span: None,
        message: error.message.clone(),
        candidates: Vec::new(),
    });
    error
}

fn absolute(path: &Path) -> Result<PathBuf> {
    normalize(&if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| host_error(path, &e.to_string()))?
            .join(path)
    })
}

fn normalize(path: &Path) -> Result<PathBuf> {
    let mut out = PathBuf::from("/");
    for c in path.components() {
        match c {
            Component::RootDir | Component::CurDir => (),
            Component::Normal(name) => out.push(name),
            Component::ParentDir => {
                out.pop();
            }
            Component::Prefix(_) => return Err(host_error(path, "unsupported host path prefix")),
        }
    }
    Ok(out)
}

fn components(path: &Path) -> Result<VecDeque<OsString>> {
    path.components()
        .filter_map(|c| match c {
            Component::Normal(name) => Some(Ok(name.to_os_string())),
            Component::RootDir => None,
            _ => Some(Err(host_error(path, "non-canonical path component"))),
        })
        .collect()
}
