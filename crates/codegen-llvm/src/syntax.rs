use crate::error::BackendError;
use compiler::loader::{Declaration, DeclarationId, Graph, Module, ModuleKey};
use compiler::parser::{Node, NodeId, NodeKind};

pub(crate) fn module<'g>(graph: &'g Graph, key: &ModuleKey) -> Result<&'g Module, BackendError> {
    graph
        .modules()
        .iter()
        .find(|module| module.key == *key)
        .ok_or_else(|| BackendError::internal(format!("module {} is not in the graph", key.path)))
}

pub(crate) fn node(module: &Module, id: NodeId) -> Result<&Node, BackendError> {
    module
        .syntax
        .nodes
        .get(id)
        .ok_or_else(|| BackendError::internal(format!("node {id} is not in {}", module.key.path)))
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
            BackendError::internal(format!(
                "declaration at {} in {} is not in its module",
                id.start, id.module.path
            ))
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
