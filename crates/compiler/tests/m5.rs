//! Source → installed typed receipt → ownership judgments and cleanup MIR.
use compiler::analysis::AnalysisLimits;
use compiler::analysis::ownership::{
    Cleanup as Obligation, Completion, Operation, Terminator, lower_body,
};
use compiler::analysis::typed::analyze_typed;
use compiler::loader::{self, InstalledRequest, Origin};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

mod support;

#[test]
fn ownership_from_installed_source() {
    installed(|request| {
        fs::write(
            &request.entry,
            r#"
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
        "#,
        )
        .unwrap();
        for test_mode in [false, true] {
            let graph = loader::load_installed(InstalledRequest {
                test_mode,
                ..request.clone()
            })
            .unwrap();
            let typed = analyze_typed(&graph, AnalysisLimits::default());
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
            let module = graph
                .modules()
                .iter()
                .find(|m| m.key.origin == Origin::Application)
                .unwrap();
            let lower = |name| {
                let id = &module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap()
                    .id;
                lower_body(&receipt, module, id, AnalysisLimits::default()).unwrap()
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
                lower_body(&receipt, module, id, limits)
                    .unwrap()
                    .mir()
                    .is_some()
            );
            let limited = lower_body(
                &receipt,
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
            let other = loader::load_installed(InstalledRequest {
                test_mode,
                ..request.clone()
            })
            .unwrap();
            let foreign = other
                .modules()
                .iter()
                .find(|m| m.key == module.key)
                .unwrap();
            assert!(lower_body(&receipt, foreign, id, limits).is_err());
            let unsupported = lower("unsupported");
            assert_eq!(unsupported.completion(), Completion::Incomplete);
            assert!(unsupported.mir().is_none());
            assert_eq!(
                unsupported.blocked_at().unwrap().span.bytes(),
                b"value: List<bool>"
            );
            let mir = split.mir().unwrap_or_else(|| panic!("{split:?}"));
            let mut block = mir.entry;
            let (taken, not_taken) = loop {
                match mir.blocks[block].terminator {
                    Terminator::Goto(next) => block = next,
                    Terminator::Branch { yes, no } => break (yes, no),
                    other => panic!("expected a branch, found {other:?}"),
                }
            };
            let drops = |mir: &compiler::analysis::ownership::Mir<'_>, mut block: usize| {
                let mut drops = Vec::new();
                loop {
                    let step = &mir.blocks[block];
                    match &step.operation {
                        Operation::Drop(local) => drops.push(local.span.bytes()),
                        Operation::ScopeExit(obligations) => {
                            drops.extend(obligations.iter().filter_map(|cleanup| {
                                if let Obligation::Drop(local) = cleanup {
                                    Some(local.span.bytes())
                                } else {
                                    None
                                }
                            }))
                        }
                        _ => {}
                    }
                    match step.terminator {
                        Terminator::Goto(next) => block = next,
                        Terminator::Return => {
                            return drops
                                .into_iter()
                                .map(|bytes| bytes.to_vec())
                                .collect::<Vec<_>>();
                        }
                        other => panic!("unexpected {other:?}"),
                    }
                }
            };
            assert_eq!(drops(mir, taken), [b"taken".as_slice()]);
            assert_eq!(drops(mir, not_taken), [b"text: string".as_slice()]);
            let leave = lower("leave");
            let mir = leave
                .mir()
                .unwrap_or_else(|| panic!("{:?}", leave.completion()));
            assert_eq!(
                drops(mir, mir.entry),
                [b"second".as_slice(), b"first".as_slice()]
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
            for block in &mir.blocks {
                if matches!(block.operation, Operation::Drop(place) if place.span.bytes() == b"state")
                {
                    let Terminator::Goto(next) = block.terminator else {
                        panic!("drop must continue to install");
                    };
                    assert!(
                        matches!(mir.blocks[next].operation, Operation::Install { target, .. } if target.span.bytes() == b"state")
                    );
                }
            }

            let ordered = lower("ordered");
            let mir = ordered
                .mir()
                .unwrap_or_else(|| panic!("{:?}", ordered.completion()));
            assert!(mir.blocks.iter().any(|b| {
                let Operation::ScopeExit(order) = &b.operation else { return false; };
                matches!(order.as_slice(), [Obligation::EndStorage(dependent), Obligation::Drop(source)]
                    if dependent.span.bytes() == b"dependent" && source.span.bytes() == b"source")
            }));

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
            assert!(
                !mir.blocks
                    .iter()
                    .any(|b| matches!(b.operation, Operation::Drop(_)))
            );

            let captured = lower("ownedCapture");
            let mir = captured
                .mir()
                .unwrap_or_else(|| panic!("{:?}", captured.completion()));
            assert!(mir.blocks.iter().any(|block| matches!(&block.operation, Operation::ScopeExit(order) if order.iter().any(|cleanup| matches!(cleanup, Obligation::DropCapture { closure, .. } if closure.span.bytes() == b"job")))));
            assert!(
                !mir.blocks
                    .iter()
                    .any(|b| matches!(b.operation, Operation::Drop(_)))
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
        }
    });
}

fn installed(check: impl FnOnce(InstalledRequest)) {
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
    let root = std::env::temp_dir().join(format!("rill-m5-{}", std::process::id()));
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
            .args(["--exact", "ownership_from_installed_source", "--nocapture"])
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
