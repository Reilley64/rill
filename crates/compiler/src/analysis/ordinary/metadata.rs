use super::*;

#[derive(Debug)]
pub struct DefaultMetadata<'g> {
    pub binder: &'g DeclarationId,
    pub node: NodeId,
    pub span: &'g Span,
    pub ty: Option<Type<'g>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequirementTarget<'g> {
    Family(&'g DeclarationId),
    Predicate(&'g Intrinsic),
}

#[derive(Debug)]
pub struct RequirementMetadata<'g> {
    pub node: NodeId,
    pub span: &'g Span,
    pub target: Option<RequirementTarget<'g>>,
    pub routes: Vec<&'g Binding>,
    pub arguments: Vec<Option<Type<'g>>>,
}

#[derive(Debug, Default)]
pub struct HeaderMetadata<'g> {
    pub defaults: Vec<DefaultMetadata<'g>>,
    pub requirements: Vec<RequirementMetadata<'g>>,
    type_metadata: PartialFacts<'g>,
}

impl<'g> HeaderMetadata<'g> {
    pub fn type_metadata(&self) -> &PartialFacts<'g> {
        &self.type_metadata
    }
}

pub(in crate::analysis) type Headers<'g> = BTreeMap<&'g DeclarationId, HeaderMetadata<'g>>;

pub(in crate::analysis) fn capture<'g>(
    headers: &mut Headers<'g>,
    analyzer: &mut Analyzer<'g>,
    module: &'g Module,
    owner: &'g Declaration,
    signature: NodeId,
    binders: &[Binder<'g>],
) -> Work<()> {
    analyzer.reserve(1)?;
    let mut metadata = HeaderMetadata::default();
    // Metadata may have its own unresolved obligations. It is not an input
    // prerequisite unless a default is actually needed in a type position.
    let findings = std::mem::take(&mut analyzer.facts.findings);
    let obligations = std::mem::take(&mut analyzer.facts.obligations);
    let result = capture_written(&mut metadata, analyzer, module, signature, binders);
    metadata.type_metadata.findings = std::mem::replace(&mut analyzer.facts.findings, findings);
    metadata.type_metadata.obligations =
        std::mem::replace(&mut analyzer.facts.obligations, obligations);
    headers.insert(&owner.id, metadata);
    result
}

fn capture_written<'g>(
    metadata: &mut HeaderMetadata<'g>,
    analyzer: &mut Analyzer<'g>,
    module: &'g Module,
    signature: NodeId,
    binders: &[Binder<'g>],
) -> Work<()> {
    let nodes = &module.syntax.nodes;
    for binder in binders {
        analyzer.step(1)?;
        if let Some(&node) = nodes[binder.declaration.node].children.get(1) {
            let ty = analyzer.ty(module, node, &binders[..binder.position], 1)?;
            analyzer.reserve(1)?;
            metadata.defaults.push(DefaultMetadata {
                binder: &binder.declaration.id,
                node,
                span: &nodes[node].span,
                ty,
            });
        }
    }
    for &child in &nodes[signature].children {
        analyzer.step(1)?;
        if nodes[child].kind != NodeKind::Requirements {
            continue;
        }
        for &node in &nodes[child].children {
            analyzer.reserve(1)?;
            let [path, args @ ..] = nodes[node].children.as_slice() else {
                unreachable!()
            };
            let mut routes = Vec::new();
            let target = requirement_target(analyzer, module, *path, &mut routes)?;
            analyzer.reserve(args.len())?;
            let mut arguments = Vec::with_capacity(args.len());
            for &arg in args {
                arguments.push(analyzer.ty(module, arg, binders, 1)?);
            }
            metadata.requirements.push(RequirementMetadata {
                node,
                span: &nodes[node].span,
                target,
                routes,
                arguments,
            });
        }
    }
    Ok(())
}

fn requirement_target<'g>(
    analyzer: &mut Analyzer<'g>,
    module: &'g Module,
    path: NodeId,
    routes: &mut Vec<&'g Binding>,
) -> Work<Option<RequirementTarget<'g>>> {
    let nodes = &module.syntax.nodes;
    let mut at = &module.key;
    let segments = &nodes[path].children;
    for (i, &segment) in segments.iter().enumerate() {
        analyzer.step(1)?;
        let terminal = i + 1 == segments.len();
        let namespace = if terminal {
            Namespace::ValueFunction
        } else {
            Namespace::TypeModule
        };
        let name = std::str::from_utf8(nodes[segment].span.bytes()).unwrap();
        let Some(binding) = analyzer.namespace_binding(at, namespace, name)? else {
            if i == 0 && terminal {
                let prelude = analyzer.graph.prelude();
                let family = match name {
                    "copy" => Some(&prelude.copy),
                    "dispose" => Some(&prelude.dispose),
                    _ => None,
                };
                if let Some(family) = family {
                    return Ok(Some(RequirementTarget::Family(family)));
                }
                for intrinsic in &prelude.intrinsics {
                    analyzer.step(1)?;
                    if intrinsic.namespace() == namespace && intrinsic.name() == name {
                        return Ok(Some(RequirementTarget::Predicate(intrinsic)));
                    }
                }
            }
            return Ok(None);
        };
        if i != 0 && !binding.public {
            return Ok(None);
        }
        analyzer.reserve(1)?;
        routes.push(binding);
        match &binding.target {
            Target::Declaration(id) if terminal && id.kind == DeclarationKind::Family => {
                return Ok(Some(RequirementTarget::Family(id)));
            }
            Target::Module(key) if !terminal => at = key,
            _ => return Ok(None),
        }
    }
    Ok(None)
}
