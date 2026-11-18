//! Source-level Result-use witnesses from the installed production capture.
use compiler::analysis::{AnalysisLimits, typed::analyze_typed};
use compiler::loader::{self, InstalledRequest, Limits, Origin};
use std::{fs, path::PathBuf, process::Command};

#[test]
fn destructuring_commits_only_written_components() {
    installed(
        "destructuring_commits_only_written_components",
        "
        import std::result::Result;
        type Pair = { left: Result<bool>; right: Result<bool>; };
        function make(flag: bool): Result<bool> { return Result.Ok(flag); }
        function tuple(): void {
            let [_, owed]: [Result<bool>, Result<bool>] = [make(true), make(false)];
        }
        function record(): void {
            let Pair { left: _ } = Pair { left: make(true), right: make(false) };
        }
        ",
        &[("make(false)", None), ("make(false)", Some("owed"))],
    );
}

#[test]
fn match_observes_only_tested_discriminants_and_commits_on_guard_true() {
    installed(
        "match_observes_only_tested_discriminants_and_commits_on_guard_true",
        "
        import std::result::Result;
        function make(flag: bool): Result<bool> { return Result.Ok(flag); }
        function sibling(): void {
            let pair: [Result<bool>, Result<bool>] = [make(true), make(false)];
            match (pair) { [Result.Ok(_), kept] => {}, [Result.Err(_), kept] => {} };
        }
        function guardFalse(): void {
            let wrapped: Result<Result<bool>> = Result.Ok(make(true));
            match (wrapped) { Result.Ok(_) if false => {}, Result.Ok(owed) => {}, Result.Err(_) => {} };
        }
        function guardTrue(): void {
            let wrapped: Result<Result<bool>> = Result.Ok(make(false));
            match (wrapped) { Result.Ok(_) if true => {}, Result.Ok(fallback) => {}, Result.Err(_) => {} };
        }
        function forwarded(input: Result<Result<bool>>): Result<bool> {
            return match (input) { Result.Ok(payload) => payload, Result.Err(_) => Result.Ok(false) };
        }
        ",
        &[("make(false)", Some("kept")), ("make(false)", Some("kept")), ("make(true)", Some("owed"))],
    );
}

#[test]
fn guards_and_propagation_keep_payload_and_early_exit_edges_separate() {
    installed_with(
        "guards_and_propagation_keep_payload_and_early_exit_edges_separate",
        "
        import std::result::Result;
        import std::option::Option;
        function branch(input: Result<Result<bool>>, other: Result<bool>): void {
            guard let owed = input else { return; }
            let _ = other;
        }
        function direct(ignored: Result<Result<bool>>): void {
            guard ignored else { return; }
        }
        function boolean(owed: Result<bool>, ready: bool): void {
            guard ready else { return; }
            let _ = owed;
        }
        function option(input: Option<Result<bool>>): void {
            guard let owed = input else { return; }
        }
        function forward(input: Result<Result<bool>>): Result<bool> {
            return input?;
        }
        ",
        &[
            ("ignored: Result<Result<bool>>", None),
            ("input: Option<Result<bool>>", Some("owed")),
            ("input: Result<Result<bool>>", Some("owed")),
            ("other: Result<bool>", Some("other")),
            ("owed: Result<bool>", Some("owed")),
        ],
        |report| {
            use compiler::analysis::typed::ResultComponent;
            let role = report.result_role().unwrap();
            let uses = report.result_use().unwrap();
            let ignored = uses
                .discards()
                .iter()
                .find(|w| w.origin.span.bytes().starts_with(b"ignored:"))
                .unwrap();
            assert_eq!(
                ignored.component.as_ref(),
                &[ResultComponent::VariantPayload {
                    variant: &role.ok.id,
                    index: 0
                }]
            );
            assert_eq!(
                ignored.discard.span.bytes(),
                b"guard ignored else { return; }"
            );
            assert!(
                uses.discards()
                    .iter()
                    .any(|w| w.discard.span.bytes() == b"return;"
                        && w.origin.span.bytes().starts_with(b"other:"))
            );
        },
    );
}

#[test]
fn storage_projection_and_call_boundaries_preserve_independent_obligations() {
    installed(
        "storage_projection_and_call_boundaries_preserve_independent_obligations",
        "
        import std::result::{Result, isOk, ok};
        type Pair = { left: Result<bool>; right: Result<bool>; };
        function make(flag: bool): Result<bool> { return Result.Ok(flag); }
        function consume(input: Result<bool>): void { let _ = input; }
        function forwarded(value: Result<bool>): void { consume(value); }
        function retain(read: Result<bool>): void { let _ = ok(&read); }
        function aliases(handled: Result<bool>, owed: Result<bool>): void {
            let first = &handled;
            let second = first;
            isOk(second);
        }
        function projection(): Result<bool> {
            return (Pair { left: make(true), right: make(false) }).left;
        }
        function rebuild(original: Pair): Pair {
            return Pair { ...original, left: make(true) };
        }
        ",
        &[
            ("make(false)", None),
            ("original: Pair", Some("original")),
            ("owed: Result<bool>", Some("owed")),
            ("read: Result<bool>", Some("read")),
        ],
    );
}

#[test]
fn lexical_dead_source_and_conditional_paths_do_not_merge() {
    installed(
        "lexical_dead_source_and_conditional_paths_do_not_merge",
        "
        import std::result::{Result, isOk};
        function afterReturn(x: Result<bool>): Result<bool> {
            return x;
            x;
        }
        function deadBranch(y: Result<bool>): void {
            if (false) { y; }
            let _ = y;
        }
        function shortCircuit(flag: bool, owed: Result<bool>): void {
            flag && isOk(&owed);
        }
        function correlated(flag: bool, accounted: Result<bool>): void {
            if (flag) { let _ = accounted; }
            if (!flag) { let _ = accounted; }
        }
        function known(accounted: Result<bool>): void {
            let definite: Result<bool> = Result.Ok(true);
            if (isOk(&definite)) { let _ = accounted; }
        }
        ",
        &[
            ("owed: Result<bool>", Some("owed")),
            ("x: Result<bool>", None),
            ("y: Result<bool>", None),
        ],
    );
}

#[test]
fn argument_staging_and_unresolved_aliases_preserve_only_provable_witnesses() {
    installed(
        "argument_staging_and_unresolved_aliases_preserve_only_provable_witnesses",
        "
        import std::result::Result;
        function make(flag: bool): Result<bool> { return Result.Ok(flag); }
        function take(first: Result<bool>, flag: bool): void { let _ = first; }
        function early(input: Result<bool>): Result<bool> {
            take(make(true), input?);
            return Result.Ok(false);
        }
        function edit(place: &mut Result<bool>): void { *place = Result.Ok(true); }
        function caller(affected: Result<bool>, untouched: Result<bool>): void {
            let mut slot = affected;
            edit(&mut slot);
        }
        ",
        &[
            ("make(true)", None),
            ("untouched: Result<bool>", Some("untouched")),
        ],
    );
}

#[test]
fn loop_iterations_keep_coexisting_occurrences_at_the_same_origin() {
    installed(
        "loop_iterations_keep_coexisting_occurrences_at_the_same_origin",
        "
        import std::result::Result;
        function make(flag: bool): Result<bool> { return Result.Ok(flag); }
        function again(flag: bool): bool { return flag; }
        function looped(flag: bool): void {
            let mut previous = make(false);
            let mut retained = make(false);
            while (again(flag)) {
                let _ = retained;
                retained = previous;
                previous = make(true);
                continue;
            }
        }
        ",
        &[
            ("make(false)", Some("previous")),
            ("make(false)", Some("retained")),
            ("make(false)", Some("retained")),
            ("make(true)", Some("previous")),
            ("make(true)", Some("retained")),
        ],
    );
}

#[test]
fn opaque_storage_does_not_erase_an_independent_witness() {
    installed(
        "opaque_storage_does_not_erase_an_independent_witness",
        "
        import std::collection::List;
        import std::result::{Result, isOk};
        function opaque(owed: Result<bool>, items: List<Result<bool>>, index: u64): void {
            let _ = &items[index];
        }
        function identity(value: &Result<bool>): &Result<bool> { return value; }
        function alias(first: Result<bool>, second: Result<bool>): void {
            isOk(identity(&first));
        }
        ",
        &[
            ("owed: Result<bool>", Some("owed")),
            ("second: Result<bool>", Some("second")),
        ],
    );
}

#[test]
fn refined_projection_can_forward_a_payload_without_acknowledging_other_paths() {
    installed(
        "refined_projection_can_forward_a_payload_without_acknowledging_other_paths",
        "
        import std::result::Result;
        enum Packet { Payload { value: Result<bool> }, Empty }
        function forward(input: Packet): Result<bool> {
            return match (input) { Packet.Payload {} => input.value, Packet.Empty => Result.Ok(false) };
        }
        ",
        &[],
    );
}

#[test]
fn implementation_parameters_use_the_captured_header_and_canonical_role() {
    installed(
        "implementation_parameters_use_the_captured_header_and_canonical_role",
        "
        import std::result::Result as Outcome;
        enum Result { Ok(bool), Err(bool) }
        function dropInput<T>(value: T): void;
        implement dropInput(value: Outcome<bool>): void {}
        function namesake(unrelated: Result): void {}
        ",
        &[("value: Outcome<bool>", Some("value"))],
    );
}

#[test]
fn typed_arrow_entries_check_parameters_without_assuming_a_capture_layout() {
    installed(
        "typed_arrow_entries_check_parameters_without_assuming_a_capture_layout",
        "
        import std::result::{Result, isOk};
        function callbacks(borrowed: Result<bool>, untouched: Result<bool>): void {
            let handler = (argument: Result<bool>) => {};
            handler(Result.Ok(true));
            let observer = () => isOk(&borrowed);
            observer();
        }
        ",
        &[
            ("argument: Result<bool>", Some("argument")),
            ("untouched: Result<bool>", Some("untouched")),
        ],
    );
}

#[test]
fn list_literal_stores_each_result_and_borrowing_does_not_acknowledge_it() {
    installed(
        "list_literal_stores_each_result_and_borrowing_does_not_acknowledge_it",
        "
        import std::collection::List;
        import std::result::Result;
        function make(flag: bool): Result<bool> { return Result.Ok(flag); }
        function remember(index: u64): void {
            let items: List<Result<bool>> = [make(true), make(false)];
            let _ = &items[index];
        }
        function acknowledge(): void {
            let items: List<Result<bool>> = [make(true)];
            let _ = items;
        }
        ",
        &[
            ("make(false)", Some("items")),
            ("make(true)", Some("items")),
        ],
    );
}

#[test]
fn collection_representations_distinguish_retained_outcomes_from_borrowed_storage() {
    installed_with(
        "collection_representations_distinguish_retained_outcomes_from_borrowed_storage",
        "
        import std::collection::List;
        import std::iter::{OwnedListIter, ListIter, ListIterMut};
        import std::result::Result;
        function check(
            scalars: List<i32>,
            remaining: OwnedListIter<Result<bool>>,
            shared: ListIter<Result<bool>>,
            exclusive: ListIterMut<Result<bool>>,
        ): void {}
        ",
        &[],
        |report| {
            use compiler::analysis::typed::ResultUseBlocker;
            let pending: Vec<_> = report
                .result_use()
                .unwrap()
                .pending()
                .iter()
                .filter(|p| {
                    p.site.module.origin == Origin::Application
                        && p.blocker == ResultUseBlocker::Containment
                })
                .map(|p| p.site.span.bytes())
                .collect();
            assert_eq!(pending, [b"remaining: OwnedListIter<Result<bool>>"]);
            assert!(
                !report
                    .result_use()
                    .unwrap()
                    .pending()
                    .iter()
                    .any(|p| p.site.module.path == "iter.rill"
                        && p.site.span.bytes() == b"intoOwned(source)")
            );
        },
    );
}

#[test]
fn scalar_list_iteration_keeps_the_empty_exit() {
    installed_with(
        "scalar_list_iteration_keeps_the_empty_exit",
        "
        import std::collection::List;
        import std::result::Result;
        function visit(owed: Result<bool>, source: List<i32>): void {
            for (let item of source) {
                if (item == 0i32) { let _ = owed; break; }
            }
        }
        ",
        &[("owed: Result<bool>", Some("owed"))],
        |report| {
            assert!(
                !report.result_use().unwrap().pending().iter().any(|p| p
                    .site
                    .span
                    .bytes()
                    .starts_with(b"for (let item of source)"))
            )
        },
    );
}

#[test]
fn conditional_iteration_does_not_prove_a_yield_or_a_retained_discard() {
    installed_with(
        "conditional_iteration_does_not_prove_a_yield_or_a_retained_discard",
        "
        import std::collection::List;
        import std::result::Result;
        function make(flag: bool): Result<bool> { return Result.Ok(flag); }
        function unresolved(source: List<Result<bool>>, unsettled: Result<bool>): void {
            for (let item of source) { make(false); break; }
        }
        ",
        &[("make(false)", None)],
        |report| {
            use compiler::analysis::{scalars::ScalarFailureKind, typed::ResultUseBlocker};
            assert_eq!(report.completion(), ScalarFailureKind::Incomplete);
            assert!(report.result_use().unwrap().pending().iter().any(|p| {
                p.site.span.bytes().starts_with(b"for (let item of source)")
                    && matches!(
                        p.blocker,
                        ResultUseBlocker::Storage | ResultUseBlocker::Control
                    )
            }));
        },
    );
}

#[test]
fn unclassified_generic_storage_can_be_forwarded_or_acknowledged_parametrically() {
    installed_with(
        "unclassified_generic_storage_can_be_forwarded_or_acknowledged_parametrically",
        "
        import std::result::Result;
        function forward<T>(returned: T): T { return returned; }
        function acknowledge<T>(explicit: T): void { let _ = explicit; }
        function implicit<T>(unknown: T): void {}
        ",
        &[],
        |report| {
            use compiler::analysis::typed::ResultUseBlocker;
            let unknown: Vec<_> = report
                .result_use()
                .unwrap()
                .pending()
                .iter()
                .filter(|p| {
                    p.site.module.origin == Origin::Application
                        && p.blocker == ResultUseBlocker::Containment
                })
                .map(|p| p.site.span.bytes())
                .collect();
            assert_eq!(unknown, [b"unknown: T"]);
        },
    );
}

fn installed(test: &str, source: &str, expected: &[(&str, Option<&str>)]) {
    installed_with(test, source, expected, |_| {});
}

fn installed_with(
    test: &str,
    source: &str,
    expected: &[(&str, Option<&str>)],
    verify: impl Fn(&compiler::analysis::typed::TypedReport<'_>),
) {
    let mut root = std::env::current_exe().unwrap();
    if root.file_name().unwrap() == "result-use-probe" {
        root.pop();
        root.pop();
        let entry = root.join("app/main.rill");
        fs::write(&entry, source).unwrap();
        for test_mode in [false, true] {
            let graph = loader::load_installed(InstalledRequest {
                entry: entry.clone(),
                target: "aarch64-apple-darwin".into(),
                options: Default::default(),
                test_mode,
                limits: Limits::default(),
            })
            .unwrap();
            let report = analyze_typed(&graph, AnalysisLimits::default());
            assert!(report.result_role().is_some());
            let uses = report.result_use().unwrap();
            let mut actual: Vec<_> = uses
                .discards()
                .iter()
                .filter(|w| w.origin.module.origin == Origin::Application)
                .map(|w| {
                    assert_eq!(w.code(), "R0103");
                    (
                        std::str::from_utf8(w.origin.span.bytes()).unwrap(),
                        w.storage.map(|id| {
                            let module =
                                graph.modules().iter().find(|m| m.key == id.module).unwrap();
                            module
                                .declarations
                                .iter()
                                .find(|d| d.id == *id)
                                .unwrap()
                                .name
                                .as_deref()
                                .unwrap()
                        }),
                    )
                })
                .collect();
            actual.sort_unstable();
            let findings: Vec<_> = report
                .bodies()
                .findings()
                .iter()
                .filter(|f| f.site.module.origin == Origin::Application)
                .map(|f| (f.kind, f.site.span.bytes().escape_ascii().to_string()))
                .collect();
            assert!(findings.is_empty(), "{findings:?}");
            let pending: Vec<_> = uses
                .pending()
                .iter()
                .filter(|p| p.site.module.origin == Origin::Application)
                .map(|p| (p.blocker, p.site.span.bytes().escape_ascii().to_string()))
                .collect();
            assert_eq!(actual, expected, "pending Result use: {pending:?}");
            verify(&report);
        }
        return;
    }
    let root = std::env::temp_dir().join(format!("rill-{test}-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let _cleanup = Cleanup(root.clone());
    for path in [
        "bin",
        "app",
        "lib/rill/std",
        "lib/rill/runtime/aarch64-apple-darwin",
    ] {
        fs::create_dir_all(root.join(path)).unwrap();
    }
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    for module in [
        "core",
        "option",
        "result",
        "error",
        "format",
        "collection",
        "iter",
    ] {
        let path = format!("{module}.rill");
        fs::copy(
            repo.join("stdlib").join(&path),
            root.join("lib/rill/std").join(path),
        )
        .unwrap();
    }
    fs::copy(
        repo.join("runtime/error/src/lib.rs"),
        root.join("lib/rill/runtime/aarch64-apple-darwin/error.rs"),
    )
    .unwrap();
    let probe = root.join("bin/result-use-probe");
    fs::copy(std::env::current_exe().unwrap(), &probe).unwrap();
    let output = Command::new(probe)
        .args(["--exact", test, "--nocapture"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

struct Cleanup(PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
