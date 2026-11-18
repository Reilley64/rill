use super::*;
use compiler::analysis::ownership::Place;

#[test]
fn caller_owned_descriptor_cannot_retain_body_local_sources() {
    probe(
        "retained::caller_owned_descriptor_cannot_retain_body_local_sources",
        r#"
            function remember(slot: &mut &bool, input: &bool, text: string): void {
                *slot = input;
            }
            function restored(slot: &mut &bool, input: &bool): void {
                {
                    let flag = true;
                    *slot = &flag;
                    *slot = input;
                }
            }
            function escaped(slot: &mut &bool): void {
                let flag = true;
                *slot = &flag;
            }
            function movedSlot(slot: &mut &bool, input: &bool): void {
                let moved = slot;
                *slot = input;
            }
            function split(slot: &mut &bool, input: &bool, pick: bool): void {
                let flag = true;
                if (pick) { *slot = &flag; }
                else { *slot = input; }
            }
        "#,
        |_, module, receipt| {
            let lower = |name| {
                let decl = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                lower_body(receipt, module, &decl.id, AnalysisLimits::default()).unwrap()
            };
            let remembered = lower("remember");
            let mir = remembered
                .mir()
                .unwrap_or_else(|| panic!("remember: {:?}", remembered.completion()));
            let writes = writes_on_path(mir);
            assert!(matches!(writes.as_slice(), [(target, value)]
                if target.root.span.bytes() == b"slot: &mut &bool" && value.root.span.bytes() == b"input"));
            assert!(
                matches!(path_drops(mir, mir.entry).as_slice(), [Obligation::Drop(text)]
                if text.root.span.bytes() == b"text: string")
            );

            let restored = lower("restored");
            let mir = restored
                .mir()
                .expect("exact overwrite retires the local backing before it ends");
            let writes = writes_on_path(mir);
            assert!(
                matches!(writes.as_slice(), [(first, local), (second, input)]
                if first.root.node == second.root.node && local.root.span.bytes() == b"&flag" && input.root.span.bytes() == b"input")
            );

            for (name, code, source) in [
                ("escaped", "R0203", b"&flag".as_slice()),
                ("split", "R0203", b"&flag".as_slice()),
                ("movedSlot", "R0200", b"slot".as_slice()),
            ] {
                let rejected = lower(name);
                assert_eq!(rejected.completion(), Completion::SourceRejected, "{name}");
                assert!(rejected.mir().is_none());
                let [finding] = rejected.findings() else {
                    panic!("{name}: expected one finding")
                };
                assert_eq!(finding.code(), code, "{name}");
                assert_eq!(finding.related.span.bytes(), source, "{name}");
            }
        },
    );
}

fn writes_on_path<'g>(mir: &Mir<'g>) -> Vec<(Place<'g>, Place<'g>)> {
    let mut writes = Vec::new();
    let mut pc = mir.entry;
    for _ in 0..mir.blocks.len() {
        let block = &mir.blocks[pc];
        if let Operation::InstallThrough {
            descriptor,
            incoming,
        } = block.operation
        {
            writes.push((descriptor, incoming));
        }
        match block.terminator {
            Terminator::Goto(next) => pc = next,
            Terminator::Return => return writes,
            other => panic!("expected straight-line MIR, got {other:?}"),
        }
    }
    panic!("path did not return");
}
