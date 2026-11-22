use super::*;

pub(super) fn has_leaf_checkpoint(
    work: &[WorkItem<'_, '_>],
    report: &GraphReport<'_>,
    leaf: usize,
) -> bool {
    // Use the already-metered enabled judgment, not a CFG walk or the
    // leaf's ordinary (possibly infeasible) backedge.
    let item = &work[leaf];
    item.retaining_parameters.is_some()
        && (matches!(report.bodies[leaf].enabled_frame, Delivery::Rejected { .. })
            || item.enabled_proof.as_ref().is_some_and(|proof| {
                proof
                    .sole_backedge_site(item.ordinary.as_ref().unwrap())
                    .is_some()
            }))
}

pub(super) fn check<'g>(
    work: &mut [WorkItem<'_, 'g>],
    report: &mut GraphReport<'g>,
    candidates: Vec<(usize, usize, usize)>,
    budget: &mut Budget,
) -> Result<(), ()> {
    // Both orders of source declarations work. Consult only the already
    // validated retaining context of the selected middle root, not a root's
    // ordinary completion or a rejection in some other delivery context.
    for (index, block, callee_root) in candidates {
        budget.step(1)?;
        let middle = &work[callee_root];
        let Some(parameter) = middle.selected_gate else {
            continue;
        };
        let (leaf_checkpoint, rejection) =
            match &report.bodies[callee_root].selected_call {
                Delivery::Rejected { finding_id, .. } => {
                    let Some(site) = report.findings[*finding_id].cancellation_delivery else {
                        continue;
                    };
                    (site, Some(*finding_id))
                }
                _ => {
                    let Some(proof) = &middle.selected_proof else {
                        continue;
                    };
                    let leaf = &work[proof.call.callee_root];
                    let Some(site) = leaf.enabled_proof.as_ref().and_then(|proof| {
                        proof.sole_backedge_site(leaf.ordinary.as_ref().unwrap())
                    }) else {
                        continue;
                    };
                    (site, None)
                }
            };
        let trace = work[index].trace.take().unwrap();
        let mir = work[index].ordinary.as_ref().unwrap();
        let (entry, suspended) = match trace.finish_forwarding_caller(mir, block, parameter, budget)
        {
            Ok(Some(proof)) => proof,
            Err(Stop::Limit) => return Err(()),
            Ok(None) | Err(Stop::Pending(_) | Stop::Rejected { .. }) => continue,
        };
        budget.reserve(1)?;
        let call = RetainingCallSite {
            block,
            site: mir.blocks[block].site,
            callee_root,
        };
        if let Some(finding_id) = rejection {
            report.bodies[index].forwarded_call = Delivery::Rejected {
                finding_id,
                via_call: Some(call),
            };
        } else {
            work[index].forwarded_proof = Some(Box::new(GraphForwardedRetainingCall {
                call,
                caller_entry_site: entry.entry_site,
                on_caller_entry_delivery: entry.on_entry_delivery,
                leaf_checkpoint,
                suspended,
            }));
        }
    }
    Ok(())
}
