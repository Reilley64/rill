//! Canonical loaded graphs. Resolution is not type checking or program execution.

use crate::parser::{NodeKind, ParseLimits, Syntax};
use crate::source::{Snapshot, Span};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

mod collect;
mod filesystem;
mod installed;
mod resolve;
use filesystem::Host;
use installed::InstalledProfile;
pub use installed::{
    CollectionArtifact, InstalledRequest, load_installed, pinned_ownership_archive,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Origin {
    Application,
    Standard,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ModuleKey {
    pub origin: Origin,
    pub path: String,
}

#[derive(Clone, Debug)]
pub struct Selector {
    pub module: String,
    pub name: String,
}

#[derive(Clone, Debug)]
pub struct Prelude {
    pub copy: Selector,
    pub dispose: Selector,
}

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub max_modules: usize,
    pub max_source_bytes: usize,
    pub max_total_bytes: usize,
    pub max_work: usize,
    pub max_probes: usize,
    pub max_directory_entries: usize,
    pub parser: ParseLimits,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_modules: 4096,
            max_source_bytes: 64 * 1024 * 1024,
            max_total_bytes: 256 * 1024 * 1024,
            max_work: 10_000_000,
            max_probes: 100_000,
            max_directory_entries: 1_000_000,
            parser: ParseLimits::default(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Request {
    pub entry: PathBuf,
    pub standard_root: PathBuf,
    pub prelude: Prelude,
    pub target: String,
    pub options: BTreeMap<String, String>,
    pub test_mode: bool,
    pub limits: Limits,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Namespace {
    TypeModule,
    ValueFunction,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DeclarationKind {
    Record,
    Enum,
    Family,
    Function,
    Implementation,
    Variant,
    Field,
    Parameter,
    TypeParameter,
    Binding,
    Expectation,
    Startup,
}

/// Position of a declaration among every declaration of one loaded graph, from
/// zero with no gaps. Assigned in `loader/collect.rs`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DeclarationOrdinal(usize);

impl DeclarationOrdinal {
    pub fn index(self) -> usize {
        self.0
    }
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct DeclarationId {
    pub context: GraphId,
    pub module: ModuleKey,
    pub start: usize,
    pub kind: DeclarationKind,
    pub ordinal: DeclarationOrdinal,
}

impl std::fmt::Debug for DeclarationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeclarationId")
            .field("context", &self.context)
            .field("module", &self.module)
            .field("start", &self.start)
            .field("kind", &self.kind)
            .finish()
    }
}

#[derive(Clone, Debug)]
pub struct Declaration {
    pub id: DeclarationId,
    pub name: Option<String>,
    pub public: bool,
    /// The containing syntax scope's original start, absent at module scope.
    pub scope: Option<usize>,
    pub node: usize,
    pub span: Span,
}

#[derive(Debug)]
pub struct Module {
    context: GraphId,
    pub key: ModuleKey,
    pub path: PathBuf,
    pub snapshot: Snapshot,
    pub syntax: Syntax,
    pub declarations: Vec<Declaration>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Target {
    Module(ModuleKey),
    Declaration(DeclarationId),
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RouteEdge {
    pub module: ModuleKey,
    pub name: String,
    pub target: Target,
}

#[derive(Clone, Debug)]
pub struct Candidate {
    pub namespace: Namespace,
    pub target: Target,
    /// Finite predecessor edges, never expanded cyclic route histories.
    pub routes: Vec<RouteEdge>,
}

#[derive(Clone, Debug)]
pub struct Binding {
    pub module: ModuleKey,
    pub name: String,
    pub namespace: Namespace,
    pub target: Target,
    pub public: bool,
    pub span: Span,
    pub import: Option<Span>,
    pub routes: Vec<RouteEdge>,
}

#[derive(Clone, Debug)]
pub struct ImportOutcome {
    pub module: ModuleKey,
    pub span: Span,
    pub name: String,
    pub candidates: Vec<Candidate>,
}

#[derive(Clone, Debug)]
pub struct Implementation {
    pub declaration: DeclarationId,
    pub family: DeclarationId,
    pub signature_node: usize,
    pub body_node: usize,
}

/// A compiler-defined prelude identity, not an implementation or runtime trust grant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Intrinsic {
    context: GraphId,
    name: &'static str,
    namespace: Namespace,
}
impl Intrinsic {
    pub fn name(&self) -> &'static str {
        self.name
    }
    pub fn namespace(&self) -> Namespace {
        self.namespace
    }
}

fn intrinsics(context: &GraphId) -> Vec<Intrinsic> {
    let primitives = [
        "i8", "i16", "i32", "i64", "i128", "u8", "u16", "u32", "u64", "u128", "f32", "f64", "bool",
        "char", "string", "void", "never",
    ];
    let predicates = [
        "noBorrow",
        "sendable",
        "shareable",
        "callable",
        "callableMut",
        "callableOnce",
    ];
    primitives
        .into_iter()
        .map(|name| (name, Namespace::TypeModule))
        .chain(
            predicates
                .into_iter()
                .map(|name| (name, Namespace::ValueFunction)),
        )
        .map(|(name, namespace)| Intrinsic {
            context: context.clone(),
            name,
            namespace,
        })
        .collect()
}

#[derive(Debug)]
pub struct PreludeIdentities {
    pub intrinsics: Vec<Intrinsic>,
    pub copy: DeclarationId,
    pub dispose: DeclarationId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureKind {
    InvalidRequest,
    SourceRejected,
    InputFailure,
    SourceChanged,
    ResourceLimit,
}

/// Diagnostic-only evidence at a private path barrier. It does not create a
/// source binding or permit traversal of the private target.
#[derive(Clone, Debug)]
pub struct PrivacyWitness {
    pub module: ModuleKey,
    pub name: String,
    pub namespace: Namespace,
    /// Zero-based segment after the origin prefix; a group member follows it.
    pub segment: usize,
    pub binding: Span,
    pub target: Target,
}

#[derive(Clone, Debug)]
pub struct Finding {
    pub code: Option<&'static str>,
    pub span: Option<Span>,
    pub message: String,
    pub candidates: Vec<Candidate>,
    pub witnesses: Vec<PrivacyWitness>,
}

#[derive(Debug, Default)]
pub struct PartialGraph {
    pub modules: Vec<Module>,
    pub imports: Vec<ImportOutcome>,
    /// False: secondary diagnostics can be suppressed by an earlier failure.
    pub complete_diagnostics: bool,
}

#[derive(Debug)]
pub struct Failure {
    pub kind: FailureKind,
    pub path: Option<PathBuf>,
    pub span: Option<Span>,
    pub message: String,
    pub findings: Vec<Finding>,
    /// Provisional facts only. There is deliberately no installed-binding or
    /// successful export-lookup interface on rejected/incomplete attempts.
    pub partial: Box<PartialGraph>,
}

type Result<T> = std::result::Result<T, Failure>;

fn failure(kind: FailureKind, message: impl Into<String>) -> Failure {
    Failure {
        kind,
        path: None,
        span: None,
        message: message.into(),
        findings: Vec::new(),
        partial: Box::default(),
    }
}

#[derive(Clone, Debug)]
pub struct GraphId(Arc<()>);
impl PartialEq for GraphId {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for GraphId {}
impl PartialOrd for GraphId {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for GraphId {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        Arc::as_ptr(&self.0).cmp(&Arc::as_ptr(&other.0))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Lookup {
    Found(Target),
    Missing,
    Private,
    Ambiguous(Vec<Target>),
}

#[derive(Debug)]
pub struct Graph {
    id: GraphId,
    request: Request,
    modules: Vec<Module>,
    routine_index: Option<usize>,
    declaration_count: usize,
    prelude: PreludeIdentities,
    bindings: Vec<Binding>,
    imports: Vec<ImportOutcome>,
    implementations: Vec<Implementation>,
    host: Host,
    installed: Option<InstalledProfile>,
    error_storage: Option<(PathBuf, Vec<u8>)>,
    ownership_archive: Option<PathBuf>,
    collection_links: Option<installed::CollectionLinks>,
}

impl Graph {
    pub(crate) fn collection_artifacts(
        &self,
    ) -> Option<(CollectionArtifact<'_>, CollectionArtifact<'_>)> {
        Some(self.collection_links.as_ref()?.artifacts(&self.id))
    }

    pub(crate) fn ownership_archive(&self) -> Option<&std::path::Path> {
        self.ownership_archive.as_deref()
    }

    /// Modules carry the loader's private capture identity, even for empty startup.
    pub(crate) fn owns_module(&self, module: &Module) -> bool {
        module.context == self.id
    }

    /// This does not admit roles, implementations or representations.
    pub(crate) fn is_pinned_standard_source(&self, module: &Module) -> bool {
        module.context == self.id
            && module.key.origin == Origin::Standard
            && self
                .installed
                .and_then(|p| p.source(&module.key.path))
                .is_some()
    }

    /// This group is required only when captured by ordinary import discovery.
    pub(crate) fn installed_routine_source(&self) -> Option<&Module> {
        self.routine_index.map(|index| &self.modules[index])
    }

    /// The baseline source keys, not a discovery request.
    pub(crate) fn installed_source_keys(&self) -> Option<impl Iterator<Item = &'static str>> {
        Some(self.installed?.sources().iter().map(|&(key, _)| key))
    }

    pub(crate) fn error_storage_source(&self) -> Option<&std::path::Path> {
        self.error_storage.as_ref().map(|(path, _)| path.as_path())
    }

    pub fn id(&self) -> GraphId {
        self.id.clone()
    }
    pub fn request(&self) -> &Request {
        &self.request
    }
    pub fn modules(&self) -> &[Module] {
        &self.modules
    }
    /// One more than the largest `DeclarationOrdinal` of the graph.
    pub fn declaration_count(&self) -> usize {
        self.declaration_count
    }
    pub fn prelude(&self) -> &PreludeIdentities {
        &self.prelude
    }
    pub fn bindings(&self) -> &[Binding] {
        &self.bindings
    }
    pub fn imports(&self) -> &[ImportOutcome] {
        &self.imports
    }
    pub fn implementations(&self) -> &[Implementation] {
        &self.implementations
    }
    pub fn expectations(&self) -> Vec<&Declaration> {
        self.modules
            .iter()
            .flat_map(|m| &m.declarations)
            .filter(|d| d.id.kind == DeclarationKind::Expectation)
            .collect()
    }

    /// Select exports only. Qualification never probes the filesystem.
    pub fn lookup_export(
        &self,
        module: &ModuleKey,
        namespace: Namespace,
        segments: &[&str],
    ) -> Lookup {
        let mut at = module.clone();
        for (index, name) in segments.iter().enumerate() {
            let ns = if index + 1 == segments.len() {
                namespace
            } else {
                Namespace::TypeModule
            };
            let candidates: Vec<_> = self
                .bindings
                .iter()
                .filter(|b| b.module == at && b.namespace == ns && b.name == *name && b.public)
                .map(|b| b.target.clone())
                .collect();
            let target = match candidates.as_slice() {
                [] => {
                    return if self
                        .bindings
                        .iter()
                        .any(|b| b.module == at && b.namespace == ns && b.name == *name)
                    {
                        Lookup::Private
                    } else {
                        Lookup::Missing
                    };
                }
                [target] => target,
                _ => return Lookup::Ambiguous(candidates),
            };
            if index + 1 == segments.len() {
                return Lookup::Found(target.clone());
            }
            let Target::Module(module) = target else {
                return Lookup::Missing;
            };
            at = module.clone();
        }
        Lookup::Missing
    }
}

/// Load through M1 snapshots and M2 syntax. Success resolves only M3 bindings;
/// types, bodies, ownership, protected operations and execution remain unchecked.
pub fn load(request: Request) -> Result<Graph> {
    resolve::load(request, None, None)
}

/// Recheck a retained capture without changing its snapshots or graph identity.
/// Unchanged observations do not establish a simultaneous filesystem transaction.
pub fn validate_inputs(graph: &Graph, limits: Limits) -> Result<()> {
    graph.host.replay(
        limits,
        graph
            .modules
            .iter()
            .map(|m| (m.path.as_path(), m.snapshot.bytes()))
            .chain(
                graph
                    .error_storage
                    .iter()
                    .map(|(path, bytes)| (path.as_path(), bytes.as_slice())),
            )
            .chain(graph.ownership_archive().zip(installed::OWNERSHIP_ARCHIVE)),
        false,
    )
}
