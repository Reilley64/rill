//! Declaration inventory over M2 nodes, not a second parser or typed checker.
use super::*;
use crate::parser::NodeId;

#[derive(Clone, Debug)]
pub(super) struct ImportItem {
    pub name: String,
    pub alias: String,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub(super) struct Import {
    pub module: ModuleKey,
    pub path: Vec<String>,
    pub items: Option<Vec<ImportItem>>,
    pub alias: String,
    pub public: bool,
    pub span: Span,
    pub starts: Vec<(ModuleKey, usize)>,
}

pub(super) fn spelling(syntax: &Syntax, id: NodeId) -> String {
    // M2 admits names only after strict UTF-8 validation.
    std::str::from_utf8(syntax.nodes[id].span.bytes())
        .unwrap()
        .to_owned()
}

pub(super) fn path(syntax: &Syntax, id: NodeId) -> Vec<String> {
    syntax.nodes[id]
        .children
        .iter()
        .map(|&name| spelling(syntax, name))
        .collect()
}

pub(super) fn signature_name(syntax: &Syntax, signature: NodeId) -> NodeId {
    *syntax.nodes[signature]
        .children
        .iter()
        .find(|&&id| {
            matches!(
                syntax.nodes[id].kind,
                NodeKind::Name | NodeKind::QualifiedName
            )
        })
        .unwrap()
}

pub(super) fn collect(module: &mut Module, host: &mut Host) -> Result<Vec<Import>> {
    let syntax = &module.syntax;
    let mut stack = vec![(syntax.root, None, false, false)];
    let mut imports = Vec::new();
    while let Some((id, scope, public, top)) = stack.pop() {
        host.charge(1)?;
        let n = &syntax.nodes[id];
        if n.kind == NodeKind::Export {
            stack.push((n.children[0], scope, true, top));
            continue;
        }
        if n.kind == NodeKind::Import {
            let names = path(syntax, n.children[0]);
            let group = n
                .children
                .get(1)
                .filter(|&&child| syntax.nodes[child].kind == NodeKind::ImportGroup);
            let items = group.map(|&group| {
                syntax.nodes[group]
                    .children
                    .iter()
                    .map(|&item| {
                        let item = &syntax.nodes[item];
                        ImportItem {
                            name: spelling(syntax, item.children[0]),
                            alias: spelling(syntax, *item.children.last().unwrap()),
                            span: item.span.clone(),
                        }
                    })
                    .collect()
            });
            let alias = if group.is_none() && n.children.len() > 1 {
                spelling(syntax, n.children[1])
            } else {
                names.last().unwrap().clone()
            };
            imports.push(Import {
                module: module.key.clone(),
                path: names,
                items,
                alias,
                public,
                span: n.span.clone(),
                starts: Vec::new(),
            });
            continue;
        }
        let declaration = match n.kind {
            NodeKind::Record => Some((DeclarationKind::Record, Some(n.children[0]), public)),
            NodeKind::Enum => Some((DeclarationKind::Enum, Some(n.children[0]), public)),
            NodeKind::Function => Some((
                if n.children.len() == 1 {
                    DeclarationKind::Family
                } else {
                    DeclarationKind::Function
                },
                Some(signature_name(syntax, n.children[0])),
                public,
            )),
            NodeKind::Implementation => Some((DeclarationKind::Implementation, None, false)),
            NodeKind::EnumVariant => Some((DeclarationKind::Variant, Some(n.children[0]), public)),
            NodeKind::RecordField { private } => {
                Some((DeclarationKind::Field, Some(n.children[0]), !private))
            }
            NodeKind::GenericParameter => {
                Some((DeclarationKind::TypeParameter, Some(n.children[0]), false))
            }
            NodeKind::Parameter | NodeKind::ArrowParameter => Some((
                DeclarationKind::Parameter,
                (syntax.nodes[n.children[0]].kind == NodeKind::Name).then_some(n.children[0]),
                false,
            )),
            NodeKind::Expect => Some((DeclarationKind::Expectation, None, false)),
            _ if top => Some((DeclarationKind::Startup, None, false)),
            _ => None,
        };
        if let Some((kind, name, public)) = declaration {
            module.declarations.push(Declaration {
                id: DeclarationId {
                    context: module.context.clone(),
                    module: module.key.clone(),
                    start: n.span.start(),
                    kind,
                },
                name: name.map(|id| spelling(syntax, id)),
                public,
                scope,
                node: id,
                span: n.span.clone(),
            });
        }
        let pattern_scope = matches!(n.kind, NodeKind::For | NodeKind::MatchArm)
            .then_some(n.span.start())
            .or(scope);
        if matches!(
            n.kind,
            NodeKind::Binding { .. } | NodeKind::GuardBinding | NodeKind::For | NodeKind::MatchArm
        ) {
            for name in binders(syntax, n.children[0], host)? {
                let span = syntax.nodes[name].span.clone();
                module.declarations.push(Declaration {
                    id: DeclarationId {
                        context: module.context.clone(),
                        module: module.key.clone(),
                        start: span.start(),
                        kind: DeclarationKind::Binding,
                    },
                    name: Some(spelling(syntax, name)),
                    public: false,
                    scope: pattern_scope,
                    node: name,
                    span,
                });
            }
        }
        if n.kind == NodeKind::Guard && n.children.len() == 3 {
            let name = n.children[1];
            let span = syntax.nodes[name].span.clone();
            host.charge(1)?;
            module.declarations.push(Declaration {
                id: DeclarationId {
                    context: module.context.clone(),
                    module: module.key.clone(),
                    start: span.start(),
                    kind: DeclarationKind::Binding,
                },
                name: Some(spelling(syntax, name)),
                public: false,
                scope: Some(syntax.nodes[n.children[2]].span.start()),
                node: name,
                span,
            });
        }
        let child_scope = if matches!(
            n.kind,
            NodeKind::Block
                | NodeKind::Function
                | NodeKind::Implementation
                | NodeKind::Record
                | NodeKind::Enum
                | NodeKind::Arrow
                | NodeKind::For
                | NodeKind::MatchArm
        ) {
            Some(n.span.start())
        } else {
            scope
        };
        host.charge(n.children.len())?;
        for &child in n.children.iter().rev() {
            // Enum members inherit the enum's visibility, not an import binding.
            let child_public = n.kind == NodeKind::Enum && public;
            stack.push((child, child_scope, child_public, n.kind == NodeKind::Module));
        }
    }
    module.declarations.sort_by(|a, b| a.id.cmp(&b.id));
    imports.sort_by_key(|i| i.span.start());
    Ok(imports)
}

fn binders(syntax: &Syntax, pattern: NodeId, host: &mut Host) -> Result<Vec<NodeId>> {
    let mut result = Vec::new();
    let mut stack = vec![pattern];
    while let Some(id) = stack.pop() {
        host.charge(1)?;
        let n = &syntax.nodes[id];
        match n.kind {
            NodeKind::NamePattern => {
                let p = &syntax.nodes[n.children[0]];
                let qualified = &syntax.nodes[p.children[0]];
                if p.children.len() == 1 && qualified.children.len() == 1 {
                    result.push(qualified.children[0]);
                }
            }
            NodeKind::PatternField { shorthand: true } => result.push(n.children[0]),
            NodeKind::PatternField { shorthand: false } => stack.push(n.children[1]),
            NodeKind::BracketPattern
            | NodeKind::RecordPattern
            | NodeKind::PayloadPattern
            | NodeKind::GroupPattern => stack.extend(n.children.iter().copied()),
            _ => (),
        }
    }
    Ok(result)
}
