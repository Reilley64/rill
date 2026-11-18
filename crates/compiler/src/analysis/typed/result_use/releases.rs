//! Parametric release proofs for the exact checked delegates to linked storage.
use super::*;
use crate::analysis::typed::{CollectionOperation, NativeOwnershipAction, OwnershipRule};

pub(super) type Parameters<'g> = BTreeMap<(&'g ModuleKey, NodeId), NodeId>;

pub(super) fn check<'g>(
    inventory: &OrdinaryAnalysis<'g>,
    core: Inputs<'_, 'g>,
    nodes: &Nodes<'_, 'g>,
    checked: &BTreeSet<&'g DeclarationId>,
    headers: &BTreeMap<&'g DeclarationId, &DeclarationFacts<'g>>,
    budget: &mut Budget,
) -> Work<Parameters<'g>> {
    let mut parameters = Parameters::new();
    budget.step(core.collections.protection.len() + core.native.protection.len())?;
    let collection = core
        .collections
        .protection
        .into_iter()
        .flatten()
        .filter_map(|scope| {
            use CollectionOperation::{DisposeIterator, DisposeList};
            let provider = scope.provider?;
            matches!(scope.operation, DisposeList | DisposeIterator(_))
                .then_some((provider.implementation, provider.bridge))
        });
    let native = core.native.protection.iter().filter_map(|scope| {
        let OwnershipRule::Required(action) = scope.rule else {
            return None;
        };
        release_blocker(action)?;
        let provider = scope.provider.as_ref()?;
        Some((provider.implementation, provider.entry))
    });
    for (wrapper, bridge) in collection.chain(native) {
        budget.step(1)?;
        if !checked.contains(&wrapper.id) {
            continue;
        }
        let Some((module, _)) = crate::analysis::ordinary::selection::declaration(
            inventory.graph(),
            &wrapper.id,
            budget,
        )?
        else {
            continue;
        };
        let Some(header) = headers.get(&wrapper.id) else {
            continue;
        };
        let DeclarationShape::Function {
            parameters: input, ..
        } = &header.shape
        else {
            continue;
        };
        let [parameter] = input.as_slice() else {
            continue;
        };
        let Some(&block) = module.syntax.nodes[wrapper.node].children.get(1) else {
            continue;
        };
        let [statement] = module.syntax.nodes[block].children.as_slice() else {
            continue;
        };
        if module.syntax.nodes[*statement].kind != NodeKind::ExpressionStatement {
            continue;
        }
        let [node] = module.syntax.nodes[*statement].children.as_slice() else {
            continue;
        };
        let expression = |node| match &nodes.get(&(&module.key, node))?.kind {
            NodeFactKind::Expression(value) if value.ordinary_context.is_none() => Some(value),
            _ => None,
        };
        let Some(call) = expression(*node).and_then(|value| value.call.as_ref()) else {
            continue;
        };
        let [argument] = call.arguments.as_slice() else {
            continue;
        };
        if *call.callee == bridge.id
            && expression(*argument).is_some_and(|value| {
                value.access == Access::Owned
                    && value.path.as_ref().is_some_and(|path| {
                        *path.root == parameter.declaration.id && path.projections.is_empty()
                    })
            })
        {
            // At this exact body edge the opaque input is the formal parameter.
            // The delegate's effect is checked at its concrete application; it
            // is not an acknowledgement of the parameter or of known contents.
            budget.reserve(1)?;
            parameters.insert((&module.key, *node), parameter.declaration.node);
        }
    }
    Ok(parameters)
}

/// Neither preservation nor possible destruction authorizes forgetting an
/// outstanding Result. The current pass lacks a precise native residence.
pub(super) fn release_blocker(action: NativeOwnershipAction) -> Option<ResultUseBlocker> {
    use NativeOwnershipAction::*;
    match action {
        GuardRelease | SenderRelease => Some(ResultUseBlocker::Storage),
        SharedRelease | ReceiverRelease => Some(ResultUseBlocker::Containment),
        SharedRetain | SenderRetain => None,
    }
}
