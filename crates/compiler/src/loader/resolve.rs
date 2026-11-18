use super::collect::{self, Import};
use super::filesystem::{self, Kind};
use super::*;
use crate::lexer::{LexStatus, lex};
use crate::parser::{EntryPoint, NodeKind, parse};
use std::collections::{BTreeSet, VecDeque};
use std::path::{Component, Path};

mod cycles;
use cycles::{CYCLIC, MISSING, PRIVATE, alias_causes};

type Slot = (ModuleKey, Namespace, String);
type Facts = BTreeMap<Slot, BTreeSet<Target>>;
type Privacy = BTreeMap<Slot, Vec<(Span, Target)>>;

struct Engine {
    id: GraphId,
    request: Request,
    host: Host,
    roots: BTreeMap<Origin, PathBuf>,
    modules: BTreeMap<ModuleKey, Module>,
    imports: Vec<Import>,
    outcomes: Vec<ImportOutcome>,
    findings: Vec<Finding>,
    pending: VecDeque<ModuleKey>,
    installed: Option<InstalledProfile>,
    error_storage: Option<(PathBuf, Vec<u8>)>,
}

pub(super) fn load(request: Request, installed: Option<InstalledProfile>) -> Result<Graph> {
    let mut engine = Engine {
        id: GraphId(Arc::new(())),
        host: Host::new(request.limits),
        request,
        roots: BTreeMap::new(),
        modules: BTreeMap::new(),
        imports: Vec::new(),
        outcomes: Vec::new(),
        findings: Vec::new(),
        pending: VecDeque::new(),
        installed,
        error_storage: None,
    };
    match engine.run() {
        Ok((bindings, implementations, prelude)) => Ok(Graph {
            id: engine.id,
            request: engine.request,
            modules: engine.modules.into_values().collect(),
            bindings,
            imports: engine.outcomes,
            implementations,
            prelude,
            host: engine.host,
            installed,
            error_storage: engine.error_storage,
        }),
        Err(mut error) => {
            error.findings.append(&mut engine.findings);
            error.findings.sort_by(|a, b| {
                let key = |finding: &Finding| {
                    finding.span.as_ref().map(|span| {
                        let module = engine
                            .modules
                            .values()
                            .find(|m| m.snapshot.id() == span.snapshot().id())
                            .map(|m| m.key.clone());
                        (module, span.start())
                    })
                };
                key(a).cmp(&key(b)).then(a.code.cmp(&b.code))
            });
            error.partial.modules = engine.modules.into_values().collect();
            error.partial.imports = engine.outcomes;
            Err(error)
        }
    }
}

impl Engine {
    fn run(&mut self) -> Result<(Vec<Binding>, Vec<Implementation>, PreludeIdentities)> {
        let parent = self
            .request
            .entry
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        for (origin, requested) in [
            (Origin::Application, parent.to_path_buf()),
            (Origin::Standard, self.request.standard_root.clone()),
        ] {
            let root = self
                .host
                .probe(&requested, None)?
                .filter(|p| p.kind == Kind::Directory)
                .ok_or_else(|| {
                    filesystem::host_error(&requested, "selected root is unavailable")
                })?;
            self.roots.insert(origin, root.path);
        }
        let filename = self
            .request
            .entry
            .file_name()
            .ok_or_else(|| failure(FailureKind::InvalidRequest, "entry requires a filename"))?;
        self.require_source(
            Origin::Application,
            self.roots[&Origin::Application].join(filename),
        )?;
        let selectors = [
            self.request.prelude.copy.clone(),
            self.request.prelude.dispose.clone(),
        ];
        let mut prelude_keys = Vec::new();
        for selector in &selectors {
            let path = Path::new(&selector.module);
            if !selector.module.ends_with(".rill")
                || path
                    .components()
                    .any(|c| !matches!(c, Component::Normal(_)))
                || path.components().next().is_none()
            {
                return Err(failure(
                    FailureKind::InvalidRequest,
                    "prelude needs a root-relative .rill module",
                ));
            }
            let key =
                self.require_source(Origin::Standard, self.roots[&Origin::Standard].join(path))?;
            if self.installed.is_some() && key.path != selector.module {
                return Err(failure(
                    FailureKind::InvalidRequest,
                    "installed prelude does not have its pinned module key",
                ));
            }
            prelude_keys.push(key);
        }
        while let Some(key) = self.pending.pop_front() {
            self.host.charge(1)?;
            let mut imports =
                collect::collect(self.modules.get_mut(&key).unwrap(), &mut self.host)?;
            for import in &mut imports {
                self.discover(import)?;
            }
            self.imports.extend(imports);
        }
        self.capture_installed_metadata()?;
        self.imports
            .sort_by(|a, b| (&a.module, a.span.start()).cmp(&(&b.module, b.span.start())));
        let (facts, names, public_names) = self.seeds()?;
        let mut facts = facts;
        let no_privacy = Privacy::new();
        loop {
            let mut changed = false;
            for import in &self.imports {
                self.host.charge(1)?;
                let results = resolve_import(
                    import,
                    &facts,
                    &names,
                    &public_names,
                    &no_privacy,
                    &mut self.host,
                )?;
                if import.public {
                    for (name, _, resolution) in results {
                        for (ns, targets) in resolution.targets {
                            for target in targets {
                                self.host.charge(1)?;
                                changed |= facts
                                    .entry((import.module.clone(), ns, name.clone()))
                                    .or_default()
                                    .insert(target);
                            }
                        }
                    }
                }
            }
            if !changed {
                break;
            }
        }
        let privacy = self.private_bindings(&facts, &names, &public_names)?;
        let mut bindings = self.declaration_bindings()?;
        let findings = &mut self.findings;
        let mut analyses = Vec::new();
        for import in &self.imports {
            analyses.push(resolve_import(
                import,
                &facts,
                &names,
                &public_names,
                &privacy,
                &mut self.host,
            )?);
        }
        let causes = alias_causes(&self.imports, &analyses, &mut self.host)?;
        let mut obligation = 0;
        for (import, results) in self.imports.iter().zip(analyses) {
            if import.items.is_some() {
                let base = resolve_path(
                    import,
                    &facts,
                    &names,
                    &public_names,
                    &privacy,
                    &mut self.host,
                )?;
                let types = &base.targets[&Namespace::TypeModule];
                if !types.is_empty()
                    && (types.len() != 1 || !matches!(types.first(), Some(Target::Module(_))))
                {
                    findings.push(finding(
                        if types.len() > 1 {
                            "R0402"
                        } else if base.private {
                            "R0401"
                        } else {
                            "R0400"
                        },
                        &import.span,
                        "group requires one module",
                        &base,
                        &mut self.host,
                    )?);
                }
            }
            for (name, span, resolution) in results {
                let candidates = resolution.candidates(&mut self.host)?;
                self.outcomes.push(ImportOutcome {
                    module: import.module.clone(),
                    span: span.clone(),
                    name: name.clone(),
                    candidates,
                });
                if resolution.empty() {
                    for (bit, code) in [(MISSING, "R0400"), (PRIVATE, "R0401"), (CYCLIC, "R0405")] {
                        if causes[obligation] & bit != 0 {
                            findings.push(finding(
                                code,
                                &span,
                                "import has no public target",
                                &resolution,
                                &mut self.host,
                            )?);
                        }
                    }
                }
                obligation += 1;
                for (namespace, targets) in &resolution.targets {
                    if targets.len() > 1 {
                        findings.push(finding(
                            "R0402",
                            &span,
                            "distinct canonical import targets",
                            &resolution,
                            &mut self.host,
                        )?);
                    }
                    for target in targets {
                        self.host.charge(1 + resolution.routes.len())?;
                        bindings.push(Binding {
                            module: import.module.clone(),
                            name: name.clone(),
                            namespace: *namespace,
                            target: target.clone(),
                            public: import.public,
                            span: span.clone(),
                            import: Some(import.span.clone()),
                            routes: resolution.routes.iter().cloned().collect(),
                        });
                    }
                }
            }
        }
        let mut by_slot: BTreeMap<Slot, BTreeSet<Target>> = BTreeMap::new();
        for binding in &bindings {
            self.host.charge(1)?;
            by_slot
                .entry((
                    binding.module.clone(),
                    binding.namespace,
                    binding.name.clone(),
                ))
                .or_default()
                .insert(binding.target.clone());
        }
        for binding in &bindings {
            if by_slot[&(
                binding.module.clone(),
                binding.namespace,
                binding.name.clone(),
            )]
                .len()
                > 1
            {
                findings.push(Finding {
                    code: Some("R0403"),
                    span: Some(binding.span.clone()),
                    message: "same-scope binding collision".into(),
                    candidates: Vec::new(),
                    witnesses: Vec::new(),
                });
            }
        }
        if !findings.is_empty() {
            return Err(rejected(std::mem::take(findings)));
        }
        bindings.sort_by(|a, b| {
            (&a.module, a.span.start(), a.namespace, &a.name, &a.target).cmp(&(
                &b.module,
                b.span.start(),
                b.namespace,
                &b.name,
                &b.target,
            ))
        });
        // Repeated routes are idempotent; retain public visibility on any route.
        let mut installed: BTreeMap<(ModuleKey, Namespace, String), Binding> = BTreeMap::new();
        for binding in bindings {
            let slot = (
                binding.module.clone(),
                binding.namespace,
                binding.name.clone(),
            );
            if let Some(old) = installed.get_mut(&slot) {
                old.public |= binding.public;
                old.routes.extend(binding.routes);
                old.routes.sort();
                old.routes.dedup();
            } else {
                installed.insert(slot, binding);
            }
        }
        let bindings: Vec<_> = installed.into_values().collect();
        let select = |key: &ModuleKey, selector: &Selector| -> Result<DeclarationId> {
            bindings
                .iter()
                .find(|b| {
                    &b.module == key
                        && b.name == selector.name
                        && b.namespace == Namespace::ValueFunction
                        && b.public
                })
                .and_then(|b| match &b.target {
                    Target::Declaration(id) if id.kind == DeclarationKind::Family => {
                        Some(id.clone())
                    }
                    _ => None,
                })
                .ok_or_else(|| {
                    failure(
                        FailureKind::InvalidRequest,
                        "prelude selector requires one exported family",
                    )
                })
        };
        let prelude = PreludeIdentities {
            intrinsics: intrinsics(&self.id),
            copy: select(&prelude_keys[0], &selectors[0])?,
            dispose: select(&prelude_keys[1], &selectors[1])?,
        };
        let implementations = self.attach_implementations(&bindings, &prelude)?;
        self.capture_error_storage()?;
        self.host.replay(
            self.request.limits,
            self.modules
                .values()
                .map(|m| (m.path.as_path(), m.snapshot.bytes()))
                .chain(
                    self.error_storage
                        .iter()
                        .map(|(path, bytes)| (path.as_path(), bytes.as_slice())),
                ),
            true,
        )?;
        if let Some((path, bytes)) = &self.error_storage
            && Some(bytes.as_slice()) != self.installed.map(|profile| profile.error_storage().1)
        {
            let mut error = failure(
                FailureKind::InvalidRequest,
                "installed error storage differs from toolchain pin",
            );
            error.path = Some(path.clone());
            return Err(error);
        }
        Ok((bindings, implementations, prelude))
    }

    fn capture_error_storage(&mut self) -> Result<()> {
        let Some(profile) = self.installed else {
            return Ok(());
        };
        self.host.charge(1)?;
        if !self.modules.contains_key(&ModuleKey {
            origin: Origin::Standard,
            path: "error.rill".into(),
        }) {
            return Ok(());
        }
        // Installed requests derive this parent from the canonical executable path.
        let root = self
            .request
            .standard_root
            .parent()
            .expect("installed lib/rill");
        let path = root.join(profile.error_storage().0);
        let Some(probe) = self.host.probe(&path, Some(root))? else {
            return Ok(());
        };
        if probe.kind != Kind::File || probe.path != path {
            return Err(failure(
                FailureKind::InvalidRequest,
                "installed error storage does not have its pinned artifact key",
            ));
        }
        let bytes = self.host.read(&path, true)?;
        self.host.charge(bytes.len())?;
        self.error_storage = Some((path, bytes));
        Ok(())
    }

    fn attach_implementations(
        &mut self,
        bindings: &[Binding],
        prelude: &PreludeIdentities,
    ) -> Result<Vec<Implementation>> {
        let mut implementations = Vec::new();
        let findings = &mut self.findings;
        for module in self.modules.values() {
            self.host.charge(module.declarations.len())?;
            for declaration in &module.declarations {
                if declaration.id.kind != DeclarationKind::Implementation {
                    continue;
                }
                self.host.charge(1)?;
                let node = &module.syntax.nodes[declaration.node];
                let signature = node.children[0];
                let path = collect::path(
                    &module.syntax,
                    collect::signature_name(&module.syntax, signature),
                );
                let mut at = module.key.clone();
                let mut target = None;
                let mut inaccessible = false;
                for (index, name) in path.iter().enumerate() {
                    self.host.charge(1)?;
                    let terminal = index + 1 == path.len();
                    let ns = if terminal {
                        Namespace::ValueFunction
                    } else {
                        Namespace::TypeModule
                    };
                    self.host.charge(bindings.len())?;
                    let binding = bindings
                        .iter()
                        .find(|b| b.module == at && b.namespace == ns && &b.name == name);
                    if let Some(binding) = binding {
                        if index > 0 && !binding.public {
                            self.host.charge(1)?;
                            inaccessible = true;
                            findings.push(Finding {
                                code: Some("R0401"),
                                span: Some(declaration.span.clone()),
                                message: "private implementation target".into(),
                                candidates: Vec::new(),
                                witnesses: vec![PrivacyWitness {
                                    module: binding.module.clone(),
                                    name: name.clone(),
                                    namespace: ns,
                                    segment: index,
                                    binding: binding.span.clone(),
                                    target: binding.target.clone(),
                                }],
                            });
                            break;
                        }
                        if terminal {
                            target = Some(binding.target.clone());
                        } else if let Target::Module(module) = &binding.target {
                            at = module.clone();
                        } else {
                            break;
                        }
                    } else {
                        if index == 0 && terminal {
                            target = match name.as_str() {
                                "copy" => Some(Target::Declaration(prelude.copy.clone())),
                                "dispose" => Some(Target::Declaration(prelude.dispose.clone())),
                                _ => None,
                            };
                        }
                        break;
                    }
                }
                if inaccessible {
                    continue;
                }
                match target {
                    Some(Target::Declaration(family)) if family.kind == DeclarationKind::Family => {
                        implementations.push(Implementation {
                            declaration: declaration.id.clone(),
                            family,
                            signature_node: signature,
                            body_node: node.children[1],
                        })
                    }
                    Some(_) => findings.push(Finding {
                        code: None,
                        span: Some(declaration.span.clone()),
                        message: "implementation target is not a body-less family".into(),
                        candidates: Vec::new(),
                        witnesses: Vec::new(),
                    }),
                    None => findings.push(Finding {
                        code: Some("R0400"),
                        span: Some(declaration.span.clone()),
                        message: "implementation family target is unavailable".into(),
                        candidates: Vec::new(),
                        witnesses: Vec::new(),
                    }),
                }
            }
        }
        if findings.is_empty() {
            Ok(implementations)
        } else {
            Err(rejected(std::mem::take(findings)))
        }
    }

    fn private_bindings(
        &mut self,
        facts: &Facts,
        names: &BTreeSet<Slot>,
        public_names: &BTreeSet<Slot>,
    ) -> Result<Privacy> {
        let mut privacy = Privacy::new();
        for binding in self
            .declaration_bindings()?
            .into_iter()
            .filter(|b| !b.public)
        {
            self.host.charge(1)?;
            privacy
                .entry((binding.module, binding.namespace, binding.name))
                .or_default()
                .push((binding.span, binding.target));
        }
        for import in self.imports.iter().filter(|i| !i.public) {
            for (name, span, result) in
                resolve_import(import, facts, names, public_names, &privacy, &mut self.host)?
            {
                for (ns, targets) in result.targets {
                    for target in targets {
                        self.host.charge(1)?;
                        privacy
                            .entry((import.module.clone(), ns, name.clone()))
                            .or_default()
                            .push((span.clone(), target));
                    }
                }
            }
        }
        Ok(privacy)
    }

    fn capture_installed_metadata(&mut self) -> Result<()> {
        let Some(profile) = self.installed else {
            return Ok(());
        };
        let (mut question_mark, mut interpolation) = (false, false);
        'sources: for module in self.modules.values() {
            for node in &module.syntax.nodes {
                self.host.charge(1)?;
                question_mark |= matches!(
                    node.kind,
                    NodeKind::Propagate | NodeKind::Wrap | NodeKind::PipeStep { propagate: true }
                );
                interpolation |= node.kind == NodeKind::TemplateHole;
                if question_mark && interpolation {
                    break 'sources;
                }
            }
        }
        if question_mark {
            self.capture_metadata_sources(profile.question_mark_dependencies())?;
        }
        if interpolation {
            self.capture_metadata_sources(profile.interpolation_dependencies())?;
        }
        Ok(())
    }

    fn capture_metadata_sources(&mut self, paths: &[&str]) -> Result<()> {
        let root = self.roots[&Origin::Standard].clone();
        for path in paths {
            let key = ModuleKey {
                origin: Origin::Standard,
                path: (*path).into(),
            };
            self.host.charge(1)?;
            if self.modules.contains_key(&key) {
                continue; // An explicit source edge is already in the graph.
            }
            let path = root.join(path);
            let Some(probe) = self.host.probe(&path, Some(&root))? else {
                continue;
            };
            if probe.kind != Kind::File || probe.path != path {
                return Err(failure(
                    FailureKind::InvalidRequest,
                    "installed metadata does not have its pinned module key",
                ));
            }
            let loaded = self.add_module(Origin::Standard, probe.path)?;
            let pending = self.pending.pop_front();
            debug_assert_eq!(pending.as_ref(), Some(&loaded));
            let module = self.modules.get_mut(&loaded).unwrap();
            let imports = collect::collect(module, &mut self.host)?;
            self.host.charge(module.declarations.len())?;
            // This edge has no imports and admits no registrations. Do not let it
            // change the source resolver's import, dispatch or execution roots.
            if !imports.is_empty()
                || module.declarations.iter().any(|d| {
                    matches!(
                        d.id.kind,
                        DeclarationKind::Startup
                            | DeclarationKind::Expectation
                            | DeclarationKind::Implementation
                    )
                })
            {
                return Err(failure(
                    FailureKind::InvalidRequest,
                    "installed metadata exceeds its declared scope",
                ));
            }
        }
        Ok(())
    }

    fn require_source(&mut self, origin: Origin, path: PathBuf) -> Result<ModuleKey> {
        let probe = self
            .host
            .probe(&path, Some(&self.roots[&origin]))?
            .ok_or_else(|| filesystem::host_error(&path, "required source is missing"))?;
        if probe.kind != Kind::File {
            return Err(filesystem::host_error(
                &path,
                "source is not a regular file",
            ));
        }
        self.add_module(origin, probe.path)
    }

    fn add_module(&mut self, origin: Origin, path: PathBuf) -> Result<ModuleKey> {
        let relative = path
            .strip_prefix(&self.roots[&origin])
            .unwrap()
            .to_str()
            .ok_or_else(|| filesystem::host_error(&path, "canonical module path is not UTF-8"))?
            .to_owned();
        if !relative.ends_with(".rill") {
            return Err(filesystem::host_error(
                &path,
                "source requires exact .rill suffix",
            ));
        }
        let key = ModuleKey {
            origin,
            path: relative,
        };
        if self.modules.contains_key(&key) {
            return Ok(key);
        }
        if self.modules.len() >= self.request.limits.max_modules {
            return Err(filesystem::resource());
        }
        self.host.charge(1)?;
        let bytes = self.host.read(&path, true)?;
        if origin == Origin::Standard
            && let Some(pinned) = self.installed.and_then(|profile| profile.source(&key.path))
        {
            self.host.charge(bytes.len())?;
            if bytes != pinned {
                // Preserve SourceChanged if the mismatch is not a stable observation.
                self.host.replay(
                    self.request.limits,
                    self.modules
                        .values()
                        .map(|m| (m.path.as_path(), m.snapshot.bytes()))
                        .chain(std::iter::once((path.as_path(), bytes.as_slice()))),
                    true,
                )?;
                let mut error = failure(
                    FailureKind::InvalidRequest,
                    "installed source differs from toolchain pin",
                );
                error.path = Some(path);
                return Err(error);
            }
        }
        let identity = path
            .to_str()
            .ok_or_else(|| filesystem::host_error(&path, "source identity is not UTF-8"))?;
        let snapshot = Snapshot::capture(identity, &bytes).map_err(|_| filesystem::resource())?;
        let lexical = lex(&snapshot);
        if lexical.status == LexStatus::Incomplete {
            return Err(filesystem::resource());
        }
        if !lexical.diagnostics.is_empty() {
            return Err(rejected(
                lexical
                    .diagnostics
                    .iter()
                    .map(|d| Finding {
                        code: Some(d.kind.code()),
                        span: Some(d.span.clone()),
                        message: format!("{:?}", d.kind),
                        candidates: Vec::new(),
                        witnesses: Vec::new(),
                    })
                    .collect(),
            ));
        }
        let mut parser_limits = self.request.limits.parser;
        parser_limits.max_work = parser_limits
            .max_work
            .min(self.request.limits.max_work - self.host.work);
        let syntax =
            parse(&snapshot, &lexical, EntryPoint::Module, parser_limits).map_err(|e| {
                if e.kind == crate::parser::FailureKind::ResourceLimit {
                    filesystem::resource()
                } else {
                    rejected(vec![Finding {
                        code: None,
                        span: Some(e.span),
                        message: format!("{:?}", e.kind),
                        candidates: Vec::new(),
                        witnesses: Vec::new(),
                    }])
                }
            })?;
        self.host.charge(syntax.work)?;
        self.host.charge(1)?;
        self.pending.push_back(key.clone());
        self.modules.insert(
            key.clone(),
            Module {
                context: self.id.clone(),
                key: key.clone(),
                path,
                snapshot,
                syntax,
                declarations: Vec::new(),
            },
        );
        Ok(key)
    }

    fn discover(&mut self, import: &mut Import) -> Result<()> {
        let mut origin = import.module.origin;
        let mut directory = PathBuf::new();
        let mut start = 0;
        match import.path[0].as_str() {
            "std" => {
                origin = Origin::Standard;
                start = 1;
            }
            "self" | "super" => {
                directory = Path::new(&import.module.path)
                    .parent()
                    .unwrap()
                    .to_path_buf();
                if import.path[0] == "self" {
                    start = 1;
                }
                while import.path.get(start).is_some_and(|s| s == "super") {
                    if !directory.pop() {
                        let mut e = filesystem::escape(&self.roots[&origin]);
                        e.findings[0].span = Some(import.span.clone());
                        return Err(e);
                    }
                    start += 1;
                }
            }
            _ => (),
        }
        self.host.charge(import.path.len() - start)?;
        let segments = import.path[start..].to_vec();
        if segments.is_empty() {
            if import.path == ["std"] {
                let candidate = self.roots[&origin].join("std.rill");
                if let Some(probe) = self.host.probe(&candidate, Some(&self.roots[&origin]))? {
                    match probe.kind {
                        Kind::File => import
                            .starts
                            .push((self.add_module(origin, probe.path)?, 0)),
                        Kind::Other => {
                            return Err(filesystem::host_error(
                                &candidate,
                                "standard facade is not a regular file",
                            ));
                        }
                        Kind::Directory => (),
                    }
                }
            }
        } else {
            for (index, segment) in segments.iter().enumerate() {
                self.host.charge(1)?;
                directory.push(segment);
                let mut candidate = self.roots[&origin].join(&directory);
                candidate.set_extension("rill");
                let probe = self
                    .host
                    .probe(&candidate, Some(&self.roots[&origin]))
                    .map_err(|mut e| {
                        if let Some(d) = e.findings.first_mut() {
                            d.span = Some(import.span.clone());
                        }
                        e
                    })?;
                if let Some(probe) = probe {
                    if probe.kind == Kind::Other {
                        return Err(filesystem::host_error(
                            &candidate,
                            "candidate source is not a regular file",
                        ));
                    }
                    if probe.kind == Kind::File {
                        import
                            .starts
                            .push((self.add_module(origin, probe.path)?, index + 1));
                    }
                }
            }
        }
        import.path = segments;
        Ok(())
    }

    fn declaration_bindings(&mut self) -> Result<Vec<Binding>> {
        let count = self.modules.values().try_fold(0usize, |count, module| {
            count
                .checked_add(module.declarations.len())
                .ok_or_else(filesystem::resource)
        })?;
        self.host.charge(count)?;
        Ok(self
            .modules
            .values()
            .flat_map(|module| {
                module.declarations.iter().filter_map(|d| {
                    if d.scope.is_some() {
                        return None;
                    }
                    let namespace = match d.id.kind {
                        DeclarationKind::Record | DeclarationKind::Enum => Namespace::TypeModule,
                        DeclarationKind::Function
                        | DeclarationKind::Family
                        | DeclarationKind::Binding => Namespace::ValueFunction,
                        _ => return None,
                    };
                    Some(Binding {
                        module: module.key.clone(),
                        name: d.name.clone()?,
                        namespace,
                        target: Target::Declaration(d.id.clone()),
                        public: d.public,
                        span: d.span.clone(),
                        import: None,
                        routes: Vec::new(),
                    })
                })
            })
            .collect())
    }

    fn seeds(&mut self) -> Result<(Facts, BTreeSet<Slot>, BTreeSet<Slot>)> {
        let mut facts = Facts::new();
        let mut names = BTreeSet::new();
        let mut public_names = BTreeSet::new();
        for b in self.declaration_bindings()? {
            self.host.charge(1)?;
            let slot = (b.module, b.namespace, b.name);
            names.insert(slot.clone());
            if b.public {
                public_names.insert(slot.clone());
                facts.entry(slot).or_default().insert(b.target);
            }
        }
        for import in &self.imports {
            let aliases: Vec<_> = import
                .items
                .as_ref()
                .map(|items| items.iter().map(|i| i.alias.clone()).collect())
                .unwrap_or_else(|| vec![import.alias.clone()]);
            for alias in aliases {
                for ns in [Namespace::TypeModule, Namespace::ValueFunction] {
                    let slot = (import.module.clone(), ns, alias.clone());
                    names.insert(slot.clone());
                    if import.public {
                        public_names.insert(slot);
                    }
                }
            }
        }
        Ok((facts, names, public_names))
    }
}

#[derive(Default)]
struct Resolution {
    targets: BTreeMap<Namespace, BTreeSet<Target>>,
    private: bool,
    witnesses: Vec<PrivacyWitness>,
    missing: bool,
    dependencies: BTreeSet<(ModuleKey, String)>,
    routes: BTreeSet<RouteEdge>,
}

impl Resolution {
    fn new() -> Self {
        Self {
            targets: [
                (Namespace::TypeModule, BTreeSet::new()),
                (Namespace::ValueFunction, BTreeSet::new()),
            ]
            .into(),
            ..Self::default()
        }
    }
    fn empty(&self) -> bool {
        self.targets.values().all(BTreeSet::is_empty)
    }
    fn candidates(&self, host: &mut Host) -> Result<Vec<Candidate>> {
        let count: usize = self.targets.values().map(BTreeSet::len).sum();
        host.charge(
            count
                .checked_mul(
                    self.routes
                        .len()
                        .checked_add(1)
                        .ok_or_else(filesystem::resource)?,
                )
                .ok_or_else(filesystem::resource)?,
        )?;
        Ok(self
            .targets
            .iter()
            .flat_map(|(ns, targets)| {
                targets.iter().map(|target| Candidate {
                    namespace: *ns,
                    target: target.clone(),
                    routes: self.routes.iter().cloned().collect(),
                })
            })
            .collect())
    }
}

struct Tables<'a> {
    facts: &'a Facts,
    public_names: &'a BTreeSet<Slot>,
    privacy: &'a Privacy,
}

fn lookup(
    module: &ModuleKey,
    name: &str,
    ns: Namespace,
    segment: usize,
    tables: &Tables<'_>,
    result: &mut Resolution,
    host: &mut Host,
) -> Result<BTreeSet<Target>> {
    let Tables {
        facts,
        public_names,
        privacy,
    } = tables;
    host.charge(1)?;
    let slot = (module.clone(), ns, name.to_owned());
    if facts.get(&slot).is_none_or(BTreeSet::is_empty)
        && let Some(bindings) = privacy.get(&slot)
    {
        host.charge(bindings.len())?;
        result.private = true;
        for (binding, target) in bindings {
            result.witnesses.push(PrivacyWitness {
                module: module.clone(),
                name: name.to_owned(),
                namespace: ns,
                segment,
                binding: binding.clone(),
                target: target.clone(),
            });
        }
    }
    if public_names.contains(&slot) {
        result
            .dependencies
            .insert((module.clone(), name.to_owned()));
    }
    host.charge(facts.get(&slot).map_or(0, BTreeSet::len))?;
    let targets = facts.get(&slot).cloned().unwrap_or_default();
    for target in &targets {
        result.routes.insert(RouteEdge {
            module: module.clone(),
            name: name.to_owned(),
            target: target.clone(),
        });
    }
    Ok(targets)
}

fn resolve_path(
    import: &Import,
    facts: &Facts,
    names: &BTreeSet<Slot>,
    public_names: &BTreeSet<Slot>,
    privacy: &Privacy,
    host: &mut Host,
) -> Result<Resolution> {
    let mut result = Resolution::new();
    result.missing = import.starts.is_empty();
    for (module, consumed) in &import.starts {
        host.charge(1)?;
        if *consumed == import.path.len() {
            result
                .targets
                .get_mut(&Namespace::TypeModule)
                .unwrap()
                .insert(Target::Module(module.clone()));
            result.routes.insert(RouteEdge {
                module: module.clone(),
                name: module.path.clone(),
                target: Target::Module(module.clone()),
            });
            continue;
        }
        let mut frontier = BTreeSet::from([module.clone()]);
        for (index, name) in import.path.iter().enumerate().skip(*consumed) {
            let terminal = index + 1 == import.path.len();
            let mut next = BTreeSet::new();
            for module in frontier {
                let namespaces = if terminal {
                    &[Namespace::TypeModule, Namespace::ValueFunction][..]
                } else {
                    &[Namespace::TypeModule][..]
                };
                if !namespaces
                    .iter()
                    .any(|ns| names.contains(&(module.clone(), *ns, name.clone())))
                {
                    result.missing = true;
                }
                for ns in [Namespace::TypeModule, Namespace::ValueFunction] {
                    if !terminal && ns == Namespace::ValueFunction {
                        continue;
                    }
                    let targets = lookup(
                        &module,
                        name,
                        ns,
                        index,
                        &Tables {
                            facts,
                            public_names,
                            privacy,
                        },
                        &mut result,
                        host,
                    )?;
                    if terminal {
                        result.targets.get_mut(&ns).unwrap().extend(targets);
                    } else {
                        for target in targets {
                            if let Target::Module(module) = target {
                                next.insert(module);
                            }
                        }
                    }
                }
            }
            frontier = next;
        }
    }
    Ok(result)
}

fn resolve_import(
    import: &Import,
    facts: &Facts,
    names: &BTreeSet<Slot>,
    public_names: &BTreeSet<Slot>,
    privacy: &Privacy,
    host: &mut Host,
) -> Result<Vec<(String, Span, Resolution)>> {
    let base = resolve_path(import, facts, names, public_names, privacy, host)?;
    let Some(items) = &import.items else {
        return Ok(vec![(import.alias.clone(), import.span.clone(), base)]);
    };
    let mut results = Vec::new();
    for item in items {
        let copies = base
            .dependencies
            .len()
            .checked_add(base.routes.len())
            .and_then(|n| n.checked_add(base.witnesses.len()))
            .and_then(|n| n.checked_add(1))
            .ok_or_else(filesystem::resource)?;
        host.charge(copies)?;
        let mut result = Resolution::new();
        let public_module = base.targets[&Namespace::TypeModule]
            .iter()
            .any(|t| matches!(t, Target::Module(_)));
        if !public_module {
            result.private = base.private;
            result.witnesses = base.witnesses.clone();
        }
        result.missing = base.missing;
        result.dependencies = base.dependencies.clone();
        result.routes = base.routes.clone();
        for target in &base.targets[&Namespace::TypeModule] {
            if let Target::Module(module) = target {
                if ![Namespace::TypeModule, Namespace::ValueFunction]
                    .iter()
                    .any(|ns| names.contains(&(module.clone(), *ns, item.name.clone())))
                {
                    result.missing = true;
                }
                for ns in [Namespace::TypeModule, Namespace::ValueFunction] {
                    let targets = lookup(
                        module,
                        &item.name,
                        ns,
                        import.path.len(),
                        &Tables {
                            facts,
                            public_names,
                            privacy,
                        },
                        &mut result,
                        host,
                    )?;
                    result.targets.get_mut(&ns).unwrap().extend(targets);
                }
            }
        }
        results.push((item.alias.clone(), item.span.clone(), result));
    }
    Ok(results)
}

fn finding(
    code: &'static str,
    span: &Span,
    message: &str,
    resolution: &Resolution,
    host: &mut Host,
) -> Result<Finding> {
    host.charge(
        resolution
            .witnesses
            .len()
            .checked_add(1)
            .ok_or_else(filesystem::resource)?,
    )?;
    Ok(Finding {
        code: Some(code),
        span: Some(span.clone()),
        message: message.into(),
        candidates: resolution.candidates(host)?,
        witnesses: resolution.witnesses.clone(),
    })
}

fn rejected(mut findings: Vec<Finding>) -> Failure {
    fn location(f: &Finding) -> Option<(&str, usize)> {
        f.span
            .as_ref()
            .map(|s| (s.snapshot().identity(), s.start()))
    }
    findings.sort_by(|a, b| location(a).cmp(&location(b)).then(a.code.cmp(&b.code)));
    let mut error = failure(
        FailureKind::SourceRejected,
        "captured source rejected by M3 or its source/syntax prerequisites",
    );
    error.findings = findings;
    error
}
