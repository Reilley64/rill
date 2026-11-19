//! Source → installed typed receipt → ownership judgments and cleanup MIR.
// The installed M4 profile that can issue a receipt is macOS AArch64.
#![cfg(all(target_arch = "aarch64", target_os = "macos"))]

use compiler::analysis::AnalysisLimits;
use compiler::analysis::ownership::{
    Cleanup as Obligation, Completion, Mir, Operation, Terminator, lower_body,
};
use compiler::analysis::typed::{TypedReceipt, TypedReport, analyze_typed};
use compiler::loader::{self, InstalledRequest, Origin};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[path = "m5/captures.rs"]
mod captures;
#[path = "m5/disposers.rs"]
mod disposers;
#[path = "m5/field_routes.rs"]
mod field_routes;
#[path = "m5/patterns.rs"]
mod patterns;
#[path = "m5/propagation.rs"]
mod propagation;
#[path = "m5/records.rs"]
mod records;
#[path = "m5/retained.rs"]
mod retained;
mod support;
#[path = "m5/tuples.rs"]
mod tuples;

#[test]
fn ownership_from_installed_source() {
    probe(
        "ownership_from_installed_source",
        r#"
            function split(flag: bool, text: string): void {
                if (flag) { let taken = text; }
            }
            function bad(flag: bool, text: string): string {
                if (flag) { let taken = text; }
                return text;
            }
            function moved(text: string): string {
                let taken = text;
                return taken;
            }
            function lastUse(text: string): string {
                let view = &text;
                let duplicate = view;
                let last = duplicate;
                return text;
            }
            function liveAlias(text: string): &string {
                let view = &text;
                let duplicate = view;
                let taken = text;
                return duplicate;
            }
            function ownSlot(text: string): &string {
                let view = &text;
                return view;
            }
            function forward(view: &string): &string {
                let duplicate = view;
                return duplicate;
            }
            function exclusive(): void {
                let mut flag = true;
                let access = &mut flag;
                let copy = flag;
                let last = access;
            }
            function chooseView(flag: bool, left: &string, right: &string): &string {
                if (flag) { return left; }
                return right;
            }
            function callRetainsBoth(flag: bool, left: string, right: string): &string {
                let view = chooseView(flag, &left, &right);
                let gone = right;
                return view;
            }
            function check(flag: &bool): bool { return *flag; }
            function callNoBorrow(): bool {
                let mut flag = true;
                let value = check(&flag);
                flag = false;
                return value;
            }
            function releaseScalar(flag: bool): bool {
                dispose(flag);
                return flag;
            }
            function repeatMove(flag: bool, text: string): void {
                while (flag) { let taken = text; continue; }
            }
            function restore(flag: bool, text: string, next: string): string {
                let mut state = text;
                while (flag) {
                    let previous = state;
                    state = "new";
                    continue;
                }
                state = next;
                return state;
            }
            function leave(text: string, other: string): void {
                while (true) {
                    let first = text;
                    let second = other;
                    break;
                }
            }
            function ownedCapture(text: string): void {
                let job = () => text;
            }
            function consumedCapture(text: string): string {
                let job = () => text;
                return text;
            }
            function escapingCapture(text: string): () => &string {
                return () => &text;
            }
            function copiedCapture(flag: bool): bool {
                let job = () => flag;
                return flag;
            }
            function ordered(seed: &string): void {
                let mut dependent = seed;
                let source = "later";
                dependent = &source;
            }
            function leavesScope(seed: &string): &string {
                let mut view = seed;
                if (true) {
                    let owner = "inner";
                    view = &owner;
                }
                return view;
            }
            function readMut(flag: &mut bool): bool { return *flag; }
            function resumeParent(): bool {
                let mut flag = true;
                let parent = &mut flag;
                let first = readMut(parent);
                return *parent;
            }
            function freeze(flag: &mut bool): &bool { return &*flag; }
            function suspendParent(): bool {
                let mut flag = true;
                let parent = &mut flag;
                let child = freeze(parent);
                let tooSoon = *parent;
                return *child;
            }
            function unsupported(value: List<bool>): void { }
            function discardOwned(text: string): void { let _ = text; }
            function descriptorSlot(input: &bool): &&bool { return &input; }
            function keepBacking(input: &mut bool): &bool {
                let parent = input;
                return freeze(parent);
            }
            function usePair(first: &mut bool, second: &bool): bool { return *second; }
            function conflictingArgs(): bool {
                let mut flag = true;
                return usePair(&mut flag, &flag);
            }
            function nestedView(text: string): void {
                let view = &text;
                let slot = &view;
                let taken = text;
                let last = slot;
            }
            function reinitialized(seed: &string): void {
                let mut view = seed;
                let mut owner = "old";
                view = &owner;
                owner = "new";
            }
            function groupedBorrow(): bool {
                let mut flag = true;
                return readMut((&mut flag));
            }
            function localEscape(): &bool { let invalid = true; return &invalid; }
            function transitivelyBad(): bool { let result = localEscape(); return *result; }
            function recursive(flag: bool): bool {
                if (flag) { return true; }
                return recursive(true);
            }
        "#,
        |request, module, receipt| {
            let lower = |name| {
                let id = &module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap()
                    .id;
                lower_body(receipt, module, id, AnalysisLimits::default()).unwrap()
            };
            let split = lower("split");
            let id = &module
                .declarations
                .iter()
                .find(|d| d.name.as_deref() == Some("split"))
                .unwrap()
                .id;
            let limits = AnalysisLimits {
                max_work: split.statistics().work,
                ..AnalysisLimits::default()
            };
            assert!(
                lower_body(receipt, module, id, limits)
                    .unwrap()
                    .mir()
                    .is_some()
            );
            let limited = lower_body(
                receipt,
                module,
                id,
                AnalysisLimits {
                    max_work: limits.max_work - 1,
                    ..limits
                },
            )
            .unwrap();
            assert_eq!(limited.completion(), Completion::ResourceLimit);
            assert!(limited.mir().is_none());
            let entries = split.statistics().entries;
            assert!(
                lower_body(
                    receipt,
                    module,
                    id,
                    AnalysisLimits {
                        max_entries: entries,
                        ..limits
                    }
                )
                .unwrap()
                .mir()
                .is_some()
            );
            let limited = lower_body(
                receipt,
                module,
                id,
                AnalysisLimits {
                    max_entries: entries - 1,
                    ..limits
                },
            )
            .unwrap();
            assert_eq!(limited.completion(), Completion::ResourceLimit);
            assert!(limited.mir().is_none());
            let other = loader::load_installed(request.clone()).unwrap();
            let foreign = other
                .modules()
                .iter()
                .find(|m| m.key == module.key)
                .unwrap();
            assert!(lower_body(receipt, foreign, id, limits).is_err());
            let unsupported = lower("unsupported");
            assert_eq!(unsupported.completion(), Completion::Incomplete);
            assert!(unsupported.mir().is_none());
            assert_eq!(
                unsupported.blocked_at().unwrap().span.bytes(),
                b"value: List<bool>"
            );
            let mir = split.mir().unwrap_or_else(|| panic!("{split:?}"));
            let (taken, not_taken) = branch_targets(mir);
            assert!(
                matches!(path_drops(mir, taken).as_slice(), [Obligation::Drop(taken)] if taken.root.span.bytes() == b"taken")
            );
            assert!(
                matches!(path_drops(mir, not_taken).as_slice(), [Obligation::Drop(text)] if text.root.span.bytes() == b"text: string")
            );
            let leave = lower("leave");
            let mir = leave
                .mir()
                .unwrap_or_else(|| panic!("{:?}", leave.completion()));
            assert!(
                matches!(path_drops(mir, mir.entry).as_slice(), [Obligation::Drop(second), Obligation::Drop(first)] if second.root.span.bytes() == b"second" && first.root.span.bytes() == b"first")
            );
            let restore = lower("restore");
            let mir = restore
                .mir()
                .unwrap_or_else(|| panic!("{:?}", restore.completion()));
            assert!(
                mir.blocks.iter().enumerate().any(
                    |(i, block)| matches!(block.terminator, Terminator::Goto(next) if next < i)
                )
            );
            let drops: Vec<_> = mir.blocks.iter().filter(|b| matches!(b.operation, Operation::Drop(place) if place.root.span.bytes() == b"state")).collect();
            assert_eq!(
                drops.len(),
                1,
                "replace the initialized state after exiting the loop"
            );
            let Terminator::Goto(next) = drops[0].terminator else {
                panic!("drop must continue to install");
            };
            assert!(
                matches!(mir.blocks[next].operation, Operation::Install { target, .. } if target.root.span.bytes() == b"state")
            );

            let ordered = lower("ordered");
            let mir = ordered
                .mir()
                .unwrap_or_else(|| panic!("{:?}", ordered.completion()));
            assert!(mir.blocks.iter().any(|b| {
            let Operation::Cleanup(order) = &b.operation else { return false; };
            matches!(order.as_slice(), [Obligation::EndStorage(dependent), Obligation::Drop(source)]
                if dependent.root.span.bytes() == b"dependent" && source.root.span.bytes() == b"source")
        }));

            let replaced = lower("reinitialized");
            let mir = replaced.mir().unwrap();
            assert!(
                mir.blocks
                    .iter()
                    .any(|b| matches!(&b.operation, Operation::Cleanup(order)
            if matches!(order.as_slice(), [Obligation::Drop(owner), Obligation::EndStorage(view)]
                if owner.root.span.bytes() == b"owner" && view.root.span.bytes() == b"view")))
            );
            let caller = lower("transitivelyBad");
            assert_eq!(caller.completion(), Completion::SourceRejected);
            assert!(caller.mir().is_none());
            assert_eq!(caller.findings()[0].code(), "R0202");
            assert_eq!(caller.findings()[0].related.span.bytes(), b"invalid");

            let rejected = lower("bad");
            assert_eq!(rejected.completion(), Completion::SourceRejected);
            assert!(rejected.mir().is_none());
            let [finding] = rejected.findings() else {
                panic!("{rejected:?}")
            };
            assert_eq!(finding.code(), "R0200");
            assert_eq!(finding.site.span.bytes(), b"text");
            assert_eq!(finding.related.span.bytes(), b"text");
            assert!(finding.related.span.start() < finding.site.span.start());

            let returned = lower("moved");
            let mir = returned.mir().unwrap();
            assert!(path_drops(mir, mir.entry).is_empty());

            let wildcard = lower("discardOwned");
            let mir = wildcard
                .mir()
                .unwrap_or_else(|| panic!("{:?}", wildcard.completion()));
            assert!(
                matches!(path_drops(mir, mir.entry).as_slice(), [Obligation::Drop(value)] if value.root.span.bytes() == b"text")
            );

            let captured = lower("ownedCapture");
            let mir = captured
                .mir()
                .unwrap_or_else(|| panic!("{:?}", captured.completion()));
            assert!(
                matches!(path_drops(mir, mir.entry).as_slice(), [Obligation::DropCapture { closure, field }] if closure.span.bytes() == b"job" && field.span.bytes() == b"text")
            );
            for name in [
                "lastUse",
                "forward",
                "chooseView",
                "check",
                "callNoBorrow",
                "copiedCapture",
                "readMut",
                "resumeParent",
                "freeze",
                "keepBacking",
                "groupedBorrow",
                "recursive",
            ] {
                let report = lower(name);
                assert_eq!(
                    report.completion(),
                    Completion::Checked,
                    "{name}: {report:?}"
                );
            }
            for (name, code) in [
                ("liveAlias", "R0203"),
                ("ownSlot", "R0202"),
                ("exclusive", "R0201"),
                ("callRetainsBoth", "R0203"),
                ("releaseScalar", "R0200"),
                ("repeatMove", "R0200"),
                ("consumedCapture", "R0200"),
                ("escapingCapture", "R0202"),
                ("leavesScope", "R0203"),
                ("suspendParent", "R0201"),
                ("descriptorSlot", "R0202"),
                ("conflictingArgs", "R0201"),
                ("nestedView", "R0203"),
            ] {
                let report = lower(name);
                assert_eq!(
                    report.completion(),
                    Completion::SourceRejected,
                    "{name}: {report:?}"
                );
                assert_eq!(report.findings()[0].code(), code, "{name}: {report:?}");
                assert!(report.mir().is_none());
            }
        },
    );
}

fn probe(
    test: &str,
    source: &str,
    mut verify: impl for<'g> FnMut(&InstalledRequest, &'g loader::Module, &TypedReceipt<'_, 'g>),
) {
    probe_with_typed(test, source, |request, module, receipt, _| {
        verify(request, module, receipt);
    });
}

fn probe_with_typed(
    test: &str,
    source: &str,
    mut verify: impl for<'g> FnMut(
        &InstalledRequest,
        &'g loader::Module,
        &TypedReceipt<'_, 'g>,
        &TypedReport<'g>,
    ),
) {
    probe_report(test, source, |request, module, typed| {
        let receipt = typed.receipt().unwrap_or_else(|| {
            panic!(
                "{:?}: {:?}; {:?}; {:?}; pending Result uses {:?}",
                typed.completion(),
                typed.core_blockers(),
                typed
                    .bodies()
                    .findings()
                    .iter()
                    .map(|f| f.kind)
                    .collect::<Vec<_>>(),
                typed
                    .bodies()
                    .obligations()
                    .iter()
                    .map(|o| o.reason)
                    .collect::<Vec<_>>(),
                typed.result_use().map(|r| r
                    .pending()
                    .iter()
                    .map(|p| (p.blocker, p.site.span.bytes().escape_ascii().to_string()))
                    .collect::<Vec<_>>())
            )
        });
        verify(request, module, &receipt, typed);
    });
}

fn probe_report(
    test: &str,
    source: &str,
    mut verify: impl for<'g> FnMut(&InstalledRequest, &'g loader::Module, &TypedReport<'g>),
) {
    const IMPORTS: &str = r#"
            import std::{List, Result, Err};
            import std::channel::{Receiver, Sender};
            import std::convert::{into, tryInto};
            import std::error::formatError;
            import std::iter::iterate;
            import std::mutex::LockGuard;
            import std::number::ParseError;
            import std::option::{Option, map as mapOption};
            import std::result::{isOk, map as mapResult};
            import std::shared::Shared;
    "#;
    installed(test, |request| {
        fs::write(&request.entry, format!("{IMPORTS}\n{source}")).unwrap();
        for test_mode in [false, true] {
            let request = InstalledRequest {
                test_mode,
                ..request.clone()
            };
            let graph = loader::load_installed(request.clone()).unwrap();
            let typed = analyze_typed(&graph, AnalysisLimits::default());
            let module = graph
                .modules()
                .iter()
                .find(|m| m.key.origin == Origin::Application)
                .unwrap();

            verify(&request, module, &typed);
        }
    });
}

fn linear<'a, 'g>(mir: &'a Mir<'g>, mut pc: usize) -> Vec<&'a Operation<'g>> {
    let mut ops = Vec::new();
    for _ in 0..mir.blocks.len() {
        let block = &mir.blocks[pc];
        ops.push(&block.operation);
        match block.terminator {
            Terminator::Goto(next) => pc = next,
            Terminator::Return => return ops,
            other => panic!("expected linear path, got {other:?}"),
        }
    }
    panic!("path did not reach a return");
}

fn straight_blocks<'a, 'g>(
    mir: &'a Mir<'g>,
    mut pc: usize,
) -> Vec<&'a compiler::analysis::ownership::Block<'g>> {
    let mut path = Vec::new();
    for _ in 0..mir.blocks.len() {
        let block = &mir.blocks[pc];
        path.push(block);
        match block.terminator {
            Terminator::Goto(next) => pc = next,
            _ => return path,
        }
    }
    panic!("expected a finite path to a branch or return");
}

fn branch_targets(mir: &Mir<'_>) -> (usize, usize) {
    let mut block = mir.entry;
    for _ in 0..mir.blocks.len() {
        match mir.blocks[block].terminator {
            Terminator::Goto(next) => block = next,
            Terminator::Branch { yes, no } => return (yes, no),
            other => panic!("expected a branch, found {other:?}"),
        }
    }
    panic!("expected a reachable branch");
}

fn path_drops<'g>(mir: &Mir<'g>, mut block: usize) -> Vec<Obligation<'g>> {
    let mut drops = Vec::new();
    for _ in 0..mir.blocks.len() {
        let step = &mir.blocks[block];
        match &step.operation {
            Operation::Drop(local) => drops.push(Obligation::Drop(*local)),
            Operation::DropCapture { closure, field } => drops.push(Obligation::DropCapture {
                closure: *closure,
                field: *field,
            }),
            Operation::Cleanup(obligations) => {
                drops.extend(obligations.iter().copied().filter(|op| {
                    !matches!(
                        op,
                        Obligation::EndStorage(_) | Obligation::DeferDisposerFields { .. }
                    )
                }))
            }
            _ => {}
        }
        match step.terminator {
            Terminator::Goto(next) => block = next,
            Terminator::Return => return drops,
            other => panic!("unexpected {other:?}"),
        }
    }
    panic!("cleanup path did not reach its return");
}

fn installed(test: &str, check: impl FnOnce(InstalledRequest)) {
    let target = "aarch64-apple-darwin";
    if let Some(root) = std::env::var_os("RILL_M5_PROBE_ROOT") {
        check(InstalledRequest {
            entry: PathBuf::from(root).join("app/main.rill"),
            target: target.into(),
            options: Default::default(),
            test_mode: false,
            limits: Default::default(),
        });
        return;
    }
    let root = std::env::temp_dir().join(format!("rill-m5-{test}-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let _cleanup = Cleanup(&root);
    let runtime = root.join("lib/rill/runtime").join(target);
    let stdlib = root.join("lib/rill/std");
    for path in [&root.join("bin"), &root.join("app"), &stdlib, &runtime] {
        fs::create_dir_all(path).unwrap();
    }
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for name in include_str!("../../../tests/installed/typed/stdlib-sources.txt").lines() {
        fs::copy(repo.join("stdlib").join(name), stdlib.join(name)).unwrap();
    }
    fs::copy(
        env!("RILL_OWNERSHIP_ARCHIVE"),
        runtime.join("librill_ownership.rlib"),
    )
    .unwrap();
    fs::copy(
        repo.join("runtime/error/src/lib.rs"),
        runtime.join("error.rs"),
    )
    .unwrap();
    let output = support::run_copied_probe(
        Command::new(root.join("bin/m5-probe"))
            .args(["--exact", test, "--nocapture"])
            .env("RILL_M5_PROBE_ROOT", &root)
            .current_dir(root.join("app")),
    )
    .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

struct Cleanup<'a>(&'a Path);
impl Drop for Cleanup<'_> {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(self.0);
    }
}
