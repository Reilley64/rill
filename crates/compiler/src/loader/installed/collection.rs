use crate::loader::GraphId;
use rill_collection::link::{self, ProviderLink, RepresentationLink};
use std::{
    fmt,
    path::{Path, PathBuf},
    ptr,
};

#[derive(Debug)]
pub(in crate::loader) struct CollectionLinks {
    executable: PathBuf,
    provider: Link,
    representation: Link,
}

impl CollectionLinks {
    pub(in crate::loader) fn capture(executable: PathBuf) -> Self {
        Self {
            executable,
            provider: Link::Provider(link::providers()),
            representation: Link::Representation(link::representations()),
        }
    }

    pub(in crate::loader) fn artifacts<'g>(
        &'g self,
        context: &'g GraphId,
    ) -> (CollectionArtifact<'g>, CollectionArtifact<'g>) {
        let artifact = |link| CollectionArtifact {
            context,
            executable: &self.executable,
            link,
        };
        (artifact(self.provider), artifact(self.representation))
    }
}

// The typed statics live in the linked image. The path is its captured location,
// not a claim about whichever file might later occupy that path.
#[derive(Clone, Copy)]
enum Link {
    Provider(&'static ProviderLink<'static, u8>),
    Representation(&'static RepresentationLink<'static, u8>),
}

impl fmt::Debug for Link {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Provider(_) => "Provider",
            Self::Representation(_) => "Representation",
        })
    }
}

impl PartialEq for Link {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Provider(a), Self::Provider(b)) => ptr::eq(*a, *b),
            (Self::Representation(a), Self::Representation(b)) => ptr::eq(*a, *b),
            _ => false,
        }
    }
}
impl Eq for Link {}

/// Capture-local identity of compiler-linked collection code or storage.
/// This is neither a persistent binary digest nor an installed native ABI pin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CollectionArtifact<'g> {
    context: &'g GraphId,
    executable: &'g Path,
    link: Link,
}

impl<'g> CollectionArtifact<'g> {
    pub(crate) fn same_capture(self, other: Self) -> bool {
        ptr::eq(self.context, other.context) && ptr::eq(self.executable, other.executable)
    }

    pub(crate) fn same_link(self, other: Self) -> bool {
        self.same_capture(other) && self.link == other.link
    }

    pub fn context(self) -> &'g GraphId {
        self.context
    }

    pub fn executable(self) -> &'g Path {
        self.executable
    }
}
