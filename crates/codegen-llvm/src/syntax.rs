use crate::error::BackendError;
use compiler::lexer::{LiteralMode, TokenKind};
use compiler::loader::{Declaration, DeclarationId, Graph, Module, ModuleKey};
use compiler::parser::{Node, NodeId, NodeKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Type {
    Owned,
    View,
    Exclusive,
}

#[derive(Clone, Debug)]
pub(crate) struct Signature {
    pub parameters: Vec<(NodeId, Type)>,
    pub never: bool,
}

pub(crate) fn module<'g>(graph: &'g Graph, key: &ModuleKey) -> Result<&'g Module, BackendError> {
    graph
        .modules()
        .iter()
        .find(|module| module.key == *key)
        .ok_or_else(|| BackendError::internal(format!("module {key:?} is not in the graph")))
}

pub(crate) fn node(module: &Module, id: NodeId) -> Result<&Node, BackendError> {
    module
        .syntax
        .nodes
        .get(id)
        .ok_or_else(|| BackendError::internal(format!("node {id} is not in {:?}", module.key)))
}

pub(crate) fn declaration<'g>(
    graph: &'g Graph,
    id: &DeclarationId,
) -> Result<(&'g Module, &'g Declaration), BackendError> {
    let module = module(graph, &id.module)?;
    let declaration = module
        .declarations
        .iter()
        .find(|declaration| declaration.id == *id)
        .ok_or_else(|| {
            BackendError::internal(format!("declaration {id:?} is not in its module"))
        })?;
    Ok((module, declaration))
}

pub(crate) fn unwrap_groups(module: &Module, mut id: NodeId) -> Result<NodeId, BackendError> {
    loop {
        let node = node(module, id)?;
        match (node.kind, node.children.as_slice()) {
            (NodeKind::Group, [inner]) => id = *inner,
            _ => return Ok(id),
        }
    }
}

pub(crate) fn string_literal(module: &Module, id: NodeId) -> Result<Option<Vec<u8>>, BackendError> {
    let id = unwrap_groups(module, id)?;
    let node = node(module, id)?;
    if node.kind != NodeKind::Literal {
        return Ok(None);
    }
    let tokens = &module.syntax.lexed.tokens;
    let Ok(at) = tokens.binary_search_by_key(&node.span.start(), |token| token.span.start()) else {
        return Ok(None);
    };
    let token = &tokens[at];
    match &token.kind {
        TokenKind::Literal(literal)
            if token.span.end() == node.span.end()
                && matches!(literal.mode, LiteralMode::String | LiteralMode::Raw { .. }) =>
        {
            let text: String = literal.decoded.iter().map(|scalar| scalar.value).collect();
            Ok(Some(text.into_bytes()))
        }
        _ => Ok(None),
    }
}

pub(crate) fn signature(graph: &Graph, id: &DeclarationId) -> Result<Signature, BackendError> {
    let (module, declaration) = declaration(graph, id)?;
    let unsupported = |what: &str| {
        BackendError::unsupported_at(&module.key.path, &declaration.span, what.to_owned())
    };
    let routine = node(module, declaration.node)?;
    let [signature, ..] = routine.children.as_slice() else {
        return Err(BackendError::internal("a routine without a signature"));
    };
    let signature = node(module, *signature)?;
    let mut parameters = Vec::new();
    let mut never = None;
    for &child in &signature.children {
        let child_node = node(module, child)?;
        match child_node.kind {
            NodeKind::GenericParameters { .. } => return Err(unsupported("a generic body")),
            NodeKind::ValueParameters => {
                for &parameter in &child_node.children {
                    let [_, annotation] = node(module, parameter)?.children.as_slice() else {
                        return Err(unsupported("a parameter without a type"));
                    };
                    let ty = parameter_type(module, *annotation)?
                        .ok_or_else(|| unsupported("a parameter that is not a string"))?;
                    parameters.push((parameter, ty));
                }
            }
            NodeKind::NamedType => {
                never = Some(match child_node.span.bytes() {
                    b"void" => false,
                    b"never" => true,
                    _ => return Err(unsupported("a result other than void or never")),
                });
            }
            _ => {}
        }
    }
    Ok(Signature {
        parameters,
        never: never.ok_or_else(|| unsupported("a routine without a result type"))?,
    })
}

fn parameter_type(module: &Module, annotation: NodeId) -> Result<Option<Type>, BackendError> {
    let node = node(module, annotation)?;
    Ok(match (node.kind, node.children.as_slice()) {
        (NodeKind::NamedType, _) if node.span.bytes() == b"string" => Some(Type::Owned),
        (NodeKind::ReferenceType { mutable: false }, [referent])
            if self::node(module, *referent)?.span.bytes() == b"string" =>
        {
            Some(Type::View)
        }
        (NodeKind::ReferenceType { mutable: true }, [referent])
            if self::node(module, *referent)?.span.bytes() == b"string" =>
        {
            Some(Type::Exclusive)
        }
        _ => None,
    })
}
