use super::*;

#[test]
fn active_disposer_cleans_only_remaining_fields() {
    probe(
        "disposers::active_disposer_cleans_only_remaining_fields",
        r#"
            import std::core::dispose as release;
            type Bag = { head: string; transferred: string; manual: string; tail: string; }
            implement release(value: Bag): void {
                let saved = value.transferred;
                dispose(value.manual);
            }
            function normal(value: Bag, later: string): void { let inner = "inner"; }
            function replace(old: Bag, incoming: Bag): void {
                let mut current = old;
                current = incoming;
            }
            function liveFieldLoan(value: Bag, seed: &string): &string {
                let mut view = seed;
                if (true) {
                    let local = value;
                    view = &local.head;
                }
                return view;
            }

            type Split = { flag: bool; first: string; rest: string; }
            implement release(value: Split): void {
                let active = value;
                if (active.flag) { let saved = active.first; return; }
            }
            function split(value: Split): void { }

            type Escape = { text: string; }
            function take(value: Escape): void { }
            implement release(value: Escape): void { let active = value; take(active); }
            function triesEscape(value: Escape): void { }

            type Invalid = { text: string; }
            implement release(value: Invalid): void {
                let taken = value.text;
                let again = value.text;
            }
            function triesInvalid(value: Invalid): void { }

            type Protected = { lock: LockGuard<i32>; }
            implement release(value: Protected): void { }
            function protectedCleanup(value: Protected): void { }
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
            let checked = lower("normal");
            let mir = checked
                .mir()
                .unwrap_or_else(|| panic!("{:?}", checked.completion()));
            let cleanup = path_drops(mir, mir.entry);
            let [
                Obligation::Drop(inner),
                Obligation::Drop(later),
                Obligation::InvokeDisposer {
                    operand,
                    implementation,
                },
            ] = cleanup.as_slice()
            else {
                panic!("expected inner cleanup, then reverse parameter order: {cleanup:?}");
            };
            assert_eq!(inner.root.span.bytes(), b"inner");
            assert_eq!(later.root.span.bytes(), b"later: string");
            assert_eq!(operand.root.span.bytes(), b"value: Bag");
            let implementation = module
                .declarations
                .iter()
                .find(|decl| &decl.id == *implementation)
                .unwrap();
            assert!(
                implementation
                    .span
                    .bytes()
                    .starts_with(b"implement release(value: Bag)")
            );

            let active = mir
                .disposer(&implementation.id)
                .expect("selected disposer MIR");
            assert_eq!(
                active
                    .blocks
                    .iter()
                    .filter(|b| matches!(b.operation, Operation::Dispose { operand, .. }
                if operand.field.is_some_and(|field| field.name.as_deref() == Some("manual"))))
                    .count(),
                1
            );
            let cleanup = path_drops(active, active.entry);
            let [
                Obligation::Drop(saved),
                Obligation::Drop(tail),
                Obligation::Drop(head),
            ] = cleanup.as_slice()
            else {
                panic!(
                    "expected the moved value and only the remaining initialized fields: {cleanup:?}"
                );
            };
            assert_eq!(saved.root.span.bytes(), b"saved");
            assert_eq!(
                tail.field.and_then(|field| field.name.as_deref()),
                Some("tail")
            );
            assert_eq!(
                head.field.and_then(|field| field.name.as_deref()),
                Some("head")
            );

            let replacement = lower("replace");
            let replacement = replacement.mir().expect("replacement MIR");
            let retire = path_drops(replacement, replacement.entry);
            assert_eq!(
                retire.len(),
                2,
                "retire the old and installed values once each"
            );
            assert!(retire.iter().all(|obligation| matches!(obligation,
                Obligation::InvokeDisposer { operand, implementation: selected }
                if *selected == &implementation.id && operand.root.span.bytes() == b"current"
            )));
            assert!(replacement.blocks.iter().any(|b| matches!((&b.operation, b.terminator),
                (Operation::Cleanup(actions), Terminator::Goto(next))
                    if actions.iter().any(|action| matches!(action, Obligation::InvokeDisposer { .. }))
                        && matches!(replacement.blocks[next].operation, Operation::Install { target, .. }
                           if target.root.span.bytes() == b"current")
            )), "disposer invocation must precede installation");

            let split = lower("split");
            let split = split.mir().expect("branching disposer MIR");
            let retire = path_drops(split, split.entry);
            let [Obligation::InvokeDisposer { implementation, .. }] = retire.as_slice() else {
                panic!("expected one outer disposer: {retire:?}");
            };
            let active = split.disposer(implementation).unwrap();
            let (yes, no) = branch_targets(active);
            assert!(
                matches!(path_drops(active, yes).as_slice(), [Obligation::Drop(saved), Obligation::Drop(rest)]
                if saved.root.span.bytes() == b"saved" && rest.field.and_then(|f| f.name.as_deref()) == Some("rest"))
            );
            assert!(
                matches!(path_drops(active, no).as_slice(), [Obligation::Drop(rest), Obligation::Drop(first)]
                if rest.field.and_then(|f| f.name.as_deref()) == Some("rest")
                    && first.field.and_then(|f| f.name.as_deref()) == Some("first"))
            );

            let protected = lower("protectedCleanup");
            assert_eq!(protected.completion(), Completion::Incomplete);
            assert!(
                protected.mir().is_none(),
                "an empty wrapper disposer cannot replace protected cleanup"
            );

            let normal = module
                .declarations
                .iter()
                .find(|d| d.name.as_deref() == Some("normal"))
                .unwrap();
            let limits = AnalysisLimits {
                max_work: checked.statistics().work - 1,
                ..AnalysisLimits::default()
            };
            let short = lower_body(receipt, module, &normal.id, limits).unwrap();
            assert_eq!(short.completion(), Completion::ResourceLimit);
            assert!(
                short.mir().is_none(),
                "do not publish an unchecked disposer graph"
            );

            for (name, code, witness) in [
                ("triesEscape", "R0501", b"value: Escape".as_slice()),
                ("triesInvalid", "R0200", b"value.text".as_slice()),
                ("liveFieldLoan", "R0203", b"&local.head".as_slice()),
            ] {
                let report = lower(name);
                assert_eq!(report.completion(), Completion::SourceRejected, "{name}");
                assert!(report.mir().is_none());
                let [finding] = report.findings() else {
                    panic!("{name}: expected one finding")
                };
                assert_eq!(finding.code(), code);
                assert_eq!(finding.related.span.bytes(), witness);
            }
        },
    );
}
