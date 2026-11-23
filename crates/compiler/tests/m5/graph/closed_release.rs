use super::*;
use compiler::analysis::ownership::lower_root;

const TEXT: &str = r#"export function ready(): bool { return true; }
function consumes(value: string): bool { dispose(value); return true; }
expect consumes("expectation");
let startup = "startup";
dispose(startup);
"#;

fn imported_roots<'a, 'g>(report: &'a GraphReport<'g>) -> [&'a RootReport<'g>; 3] {
    let startup = report
        .roots()
        .iter()
        .find(|root| {
            root.site().module.origin == Origin::Application
                && root.site().module.path == "helper.rill"
                && root.owner() == BodyOwner::Startup
        })
        .unwrap();
    [
        startup,
        root(report, b"function consumes"),
        root(report, b"expect consumes"),
    ]
}

#[test]
fn duplicate_disposal_keeps_the_first_release_as_related_evidence() {
    let startup = format!("{TEXT}dispose(startup);\n");
    let function = TEXT.replace("dispose(value);", "dispose(value); dispose(value);");
    let sources = [startup.as_str(), function.as_str()];
    fixture(
        "graph::closed_release::duplicate_disposal_keeps_the_first_release_as_related_evidence",
        &sources,
        |case, _, graph, receipt, _| {
            let report = analyze_ownership(receipt, AnalysisLimits::default());
            assert_eq!(report.completion(), GraphCompletion::SourceRejected);
            let [finding] = report.findings() else {
                panic!("{:?}", report.findings())
            };
            assert_eq!(finding.code(), "R0200");
            let written = ["dispose(startup)", "dispose(value)"][case];
            let first = sources[case].find(written).unwrap();
            let second = sources[case].rfind(written).unwrap();
            for (site, start) in [(finding.site, second), (finding.related, first)] {
                assert_eq!(
                    (site.module.origin, site.module.path.as_str()),
                    (Origin::Application, "helper.rill")
                );
                assert_eq!(
                    site.span.start()..site.span.end(),
                    start..start + written.len()
                );
            }
            let callers = imported_roots(&report);
            for (index, caller) in callers.into_iter().enumerate() {
                let rejected = (index == 0) == (case == 0);
                assert_eq!(
                    caller.ordinary_completion(),
                    if rejected {
                        Completion::SourceRejected
                    } else {
                        Completion::Checked
                    }
                );
                assert_eq!(caller.ordinary_mir().is_none(), rejected);
            }
            let helper = graph
                .modules()
                .iter()
                .find(|m| m.key.path == "helper.rill")
                .unwrap();
            let targeted = lower_root(
                receipt,
                helper,
                callers[case].owner(),
                AnalysisLimits::default(),
            )
            .unwrap();
            assert_eq!(targeted.completion(), Completion::SourceRejected);
            assert!(targeted.mir().is_none());
            let [direct] = targeted.findings() else {
                panic!("expected targeted duplicate disposal")
            };
            assert_eq!(direct.code(), finding.code());
            for (direct, graph) in [
                (direct.site, finding.site),
                (direct.related, finding.related),
            ] {
                assert_eq!((direct.module, direct.node), (graph.module, graph.node));
            }
            assert!(
                root(&report, b"function entryControl")
                    .ordinary_mir()
                    .is_some()
            );
        },
    );
}

#[test]
fn wrong_text_wrapper_or_missing_guard_archive_cannot_publish_mir() {
    use compiler::analysis::typed::{NativeOwnershipAction, OwnershipRule};

    installed(
        "graph::closed_release::wrong_text_wrapper_or_missing_guard_archive_cannot_publish_mir",
        |request| {
            let prefix = request.entry.parent().unwrap().parent().unwrap();
            let core = prefix.join("lib/rill/std/core.rill");
            let archive =
                prefix.join("lib/rill/runtime/aarch64-apple-darwin/librill_ownership.rlib");
            let saved = archive.with_extension("saved");
            let original = fs::read_to_string(&core).unwrap();
            let wrong = original.replace("{ disposeText(value); }", "{}");
            assert_ne!(original, wrong);
            fs::write(
                &request.entry,
                format!(
                    "{MAIN}\nfunction releases(lock: LockGuard<i32>): void {{ dispose(lock); }}\n"
                ),
            )
            .unwrap();
            fs::write(request.entry.with_file_name("helper.rill"), TEXT).unwrap();
            for test_mode in [false, true] {
                let request = InstalledRequest {
                    test_mode,
                    ..request.clone()
                };
                fs::write(&core, &wrong).unwrap();
                let failure = loader::load_installed(request.clone()).unwrap_err();
                assert_eq!(failure.kind, loader::FailureKind::InvalidRequest);
                assert_eq!(
                    failure.message,
                    "installed source differs from toolchain pin"
                );
                fs::write(&core, &original).unwrap();

                fs::rename(&archive, &saved).unwrap();
                let graph = loader::load_installed(request.clone()).unwrap();
                let typed = analyze_typed(&graph, AnalysisLimits::default());
                assert!(typed.receipt().is_none());
                let guard = typed
                    .ownership_protection()
                    .iter()
                    .find(|scope| {
                        scope.rule == OwnershipRule::Required(NativeOwnershipAction::GuardRelease)
                    })
                    .unwrap();
                assert!(guard.provider.is_none());
                fs::rename(&saved, &archive).unwrap();
            }
        },
    );
}

fn standard_root<'a, 'g>(
    report: &'a GraphReport<'g>,
    module: &str,
    prefix: &[u8],
) -> &'a RootReport<'g> {
    report
        .roots()
        .iter()
        .find(|root| {
            root.site().module.origin == Origin::Standard
                && root.site().module.path == module
                && root.site().span.bytes().starts_with(prefix)
                && matches!(root.owner(), BodyOwner::Declaration(_))
        })
        .unwrap()
}

#[test]
fn imported_text_disposal_closes_only_the_ordinary_caller_edges() {
    fixture(
        "graph::closed_release::imported_text_disposal_closes_only_the_ordinary_caller_edges",
        &[TEXT],
        |_, _, graph, receipt, _| {
            let report = analyze_ownership(receipt, AnalysisLimits::default());
            let helper = graph
                .modules()
                .iter()
                .find(|m| m.key.path == "helper.rill")
                .unwrap();
            let [startup, function, expectation] = imported_roots(&report);
            for caller in [startup, function, expectation] {
                let targeted =
                    lower_root(receipt, helper, caller.owner(), AnalysisLimits::default()).unwrap();
                assert_eq!(targeted.completion(), Completion::Checked);
                assert_eq!(
                    caller.ordinary_completion(),
                    Completion::Checked,
                    "{:?}",
                    caller.blocked_at()
                );
                assert!(targeted.mir().is_some());
                assert!(caller.ordinary_mir().is_some());
                assert!(matches!(caller.enabled_frame(), Delivery::Unresolved));
            }
            for (caller, name) in [(startup, b"startup".as_slice()), (function, b"value")] {
                assert!(caller.ordinary_mir().unwrap().blocks.iter().any(|block| matches!(
                block.operation, Operation::Dispose { operand, .. }
                    if operand.root.span.bytes().starts_with(name) && operand.projection.is_none()
            )));
                let [edge] = caller.dependencies() else {
                    panic!("{:?}", caller.dependencies())
                };
                assert_eq!(edge.contract, DependencyContract::BuiltinDispose);
                assert_eq!(edge.declaration.module.origin, Origin::Standard);
                assert_eq!(edge.declaration.module.path, "core.rill");
            }
            let provider = standard_root(&report, "core.rill", b"implement dispose(value: string)");
            assert_eq!(provider.ordinary_completion(), Completion::Incomplete);
            assert!(provider.ordinary_mir().is_none());
            assert!(report.active_disposers().is_empty());
            assert!(report.findings().is_empty());
            assert_eq!(report.completion(), GraphCompletion::Incomplete);
        },
    );
}

/// The whole ordered cleanup of a straight path: one `(kind, place)` per action.
fn cleanup_sequence(mir: &Mir<'_>) -> Vec<(&'static str, String)> {
    let blocks = straight_blocks(mir, mir.entry);
    assert!(matches!(
        blocks.last().unwrap().terminator,
        Terminator::Return
    ));
    let mut events = Vec::new();
    for block in blocks {
        match &block.operation {
            Operation::None => {}
            Operation::Cleanup(actions) => {
                for action in actions {
                    let (kind, place) = match action {
                        Obligation::EndStorage(place) => ("end", place),
                        Obligation::Drop(place) => ("drop", place),
                        Obligation::ReleaseGuard { operand, .. } => ("release", operand),
                        other => panic!("unexpected cleanup {other:?}"),
                    };
                    let root = String::from_utf8_lossy(place.root.span.bytes()).into_owned();
                    events.push((
                        kind,
                        match place.field() {
                            Some(field) => format!("{root}.{}", field.name.as_deref().unwrap()),
                            None => root,
                        },
                    ));
                }
            }
            other => panic!("unexpected operation {other:?}"),
        }
    }
    events
}

#[test]
fn implicit_guard_release_closes_the_ordinary_root() {
    probe(
        "graph::closed_release::implicit_guard_release_closes_the_ordinary_root",
        "function implicit(lock: LockGuard<i32>): void {}",
        |_, module, receipt| {
            let report = analyze_ownership(receipt, AnalysisLimits::default());
            let implicit = root(&report, b"function implicit");
            let targeted =
                lower_root(receipt, module, implicit.owner(), AnalysisLimits::default()).unwrap();
            assert_eq!(targeted.completion(), Completion::Checked);
            let expected = vec![("release", "lock: LockGuard<i32>".to_string())];
            assert_eq!(cleanup_sequence(targeted.mir().unwrap()), expected);
            assert_eq!(
                implicit.ordinary_completion(),
                Completion::Checked,
                "{:?}",
                implicit.blocked_at()
            );
            assert_eq!(cleanup_sequence(implicit.ordinary_mir().unwrap()), expected);
            let [edge] = implicit.dependencies() else {
                panic!("{:?}", implicit.dependencies())
            };
            let provider = standard_root(&report, "mutex.rill", b"implement<T> dispose");
            assert_eq!(BodyOwner::Declaration(edge.declaration), provider.owner());
            let DependencyContract::ProtectedRelease { entry } = edge.contract else {
                panic!("{edge:?}")
            };
            assert!(implicit.ordinary_mir().unwrap().blocks.iter().any(|block| matches!(
                &block.operation, Operation::Cleanup(actions)
                    if matches!(actions.as_slice(), [Obligation::ReleaseGuard { implementation, entry: selected, .. }]
                        if *implementation == edge.declaration && *selected == entry)
            )));
            assert!(report.findings().is_empty());
            assert_eq!(report.completion(), GraphCompletion::Incomplete);

            // The targeted query is unchanged from the parent (36f1cf15 with
            // D1's branch); only the whole graph pays for the new root.
            assert_eq!(
                targeted.statistics(),
                Statistics {
                    work: 1890,
                    entries: 31,
                    max_depth: 1
                }
            );
            // Rule 11: 185,222 / 3,993 / 3 (Incomplete, pinned by c668ad4)
            // becomes 185,246 / 3,993 / 3 (Checked root). All 24 work is the
            // `implicit` root's scope-exit verification; see the ledger.
            // Rule 11 again (ri-785.9): the i32 equality source root adds 316
            // work and 134 entries and sets depth 5, so 185,246 / 3,993 / 3
            // becomes 185,562 / 4,127 / 5 and gives ResourceLimit below.
            let exact = AnalysisLimits {
                max_work: 185562,
                max_entries: 4127,
                max_depth: 5,
            };
            assert_eq!(
                report.statistics(),
                Statistics {
                    work: exact.max_work,
                    entries: exact.max_entries,
                    max_depth: exact.max_depth
                }
            );
            let replay = analyze_ownership(receipt, exact);
            assert_eq!(replay.completion(), GraphCompletion::Incomplete);
            assert_eq!(
                root(&replay, b"function implicit").ordinary_completion(),
                Completion::Checked
            );
            for short in [
                AnalysisLimits {
                    max_work: 185222,
                    max_entries: 3993,
                    max_depth: 3,
                },
                AnalysisLimits {
                    max_work: 185246,
                    max_entries: 3993,
                    max_depth: 3,
                },
                AnalysisLimits {
                    max_work: exact.max_work - 1,
                    ..exact
                },
                AnalysisLimits {
                    max_entries: exact.max_entries - 1,
                    ..exact
                },
                AnalysisLimits {
                    max_depth: exact.max_depth - 1,
                    ..exact
                },
            ] {
                let limited = analyze_ownership(receipt, short);
                assert_eq!(limited.completion(), GraphCompletion::ResourceLimit);
                assert!(limited.findings().is_empty());
                assert!(
                    limited
                        .roots()
                        .iter()
                        .all(|root| root.ordinary_mir().is_none())
                );
            }
        },
    );
}

/// Review ci-uaa1 (ri-px1.8 #4): two scope-exit releases of the same provider
/// in one block. Each `ReleaseGuard` edge re-verifies every release of the
/// block, so the cost grows with the square of the releases. Against the
/// parent 36f1cf15 (roots `Incomplete`, graphs 185,387 / 4,008 / 3 and
/// 185,814 / 4,068 / 3), the i32 equality root adds 316 work and 134 entries
/// and sets depth 5 (ri-785.9), and the verification adds 100 work for `two`
/// and 386 for `pair`, against 24 for the single `implicit` release.
#[test]
fn two_guard_parameters_release_in_reverse_and_close_the_root() {
    assert_two_releases(
        "graph::closed_release::two_guard_parameters_release_in_reverse_and_close_the_root",
        "function two(a: LockGuard<i32>, b: LockGuard<i32>): void {}",
        b"function two",
        &[
            ("release", "b: LockGuard<i32>"),
            ("release", "a: LockGuard<i32>"),
        ],
        (1_933, 45, 1),
        (185_803, 4_142, 5),
    );
}

/// See `two_guard_parameters_release_in_reverse_and_close_the_root`.
#[test]
fn two_guard_fields_release_in_reverse_and_close_the_root() {
    assert_two_releases(
        "graph::closed_release::two_guard_fields_release_in_reverse_and_close_the_root",
        "type Pair = { first: LockGuard<i32>; second: LockGuard<i32>; }\nfunction pair(value: Pair): void {}",
        b"function pair",
        &[
            ("release", "value: Pair.second"),
            ("release", "value: Pair.first"),
            ("end", "value: Pair"),
        ],
        (2_296, 105, 1),
        (186_516, 4_202, 5),
    );
}

fn assert_two_releases(
    test: &str,
    source: &str,
    name: &[u8],
    expected: &[(&str, &str)],
    (work, entries, max_depth): (usize, usize, usize),
    (graph_work, graph_entries, graph_depth): (usize, usize, usize),
) {
    probe(test, source, |_, module, receipt| {
        let expected: Vec<_> = expected.iter().map(|(k, p)| (*k, p.to_string())).collect();
        let short = |exact: AnalysisLimits| {
            [
                AnalysisLimits {
                    max_work: exact.max_work - 1,
                    ..exact
                },
                AnalysisLimits {
                    max_entries: exact.max_entries - 1,
                    ..exact
                },
                AnalysisLimits {
                    max_depth: exact.max_depth - 1,
                    ..exact
                },
            ]
        };
        let exact = AnalysisLimits {
            max_work: graph_work,
            max_entries: graph_entries,
            max_depth: graph_depth,
        };
        let report = analyze_ownership(receipt, exact);
        assert_eq!(
            report.statistics(),
            Statistics {
                work: graph_work,
                entries: graph_entries,
                max_depth: graph_depth
            }
        );
        assert_eq!(report.completion(), GraphCompletion::Incomplete);
        assert!(report.findings().is_empty());
        let root = root(&report, name);
        assert_eq!(
            root.ordinary_completion(),
            Completion::Checked,
            "{:?}",
            root.blocked_at()
        );
        assert_eq!(cleanup_sequence(root.ordinary_mir().unwrap()), expected);
        let provider = standard_root(&report, "mutex.rill", b"implement<T> dispose");
        assert!(!root.dependencies().is_empty());
        for edge in root.dependencies() {
            assert_eq!(BodyOwner::Declaration(edge.declaration), provider.owner());
            assert!(matches!(
                edge.contract,
                DependencyContract::ProtectedRelease { .. }
            ));
        }
        for cap in short(exact) {
            let limited = analyze_ownership(receipt, cap);
            assert_eq!(limited.completion(), GraphCompletion::ResourceLimit);
            assert!(limited.findings().is_empty());
            assert!(
                limited
                    .roots()
                    .iter()
                    .all(|root| root.ordinary_mir().is_none())
            );
        }

        let exact = AnalysisLimits {
            max_work: work,
            max_entries: entries,
            max_depth,
        };
        let targeted = lower_root(receipt, module, root.owner(), exact).unwrap();
        assert_eq!(targeted.completion(), Completion::Checked);
        assert_eq!(
            targeted.statistics(),
            Statistics {
                work,
                entries,
                max_depth
            }
        );
        assert_eq!(cleanup_sequence(targeted.mir().unwrap()), expected);
        for cap in short(exact) {
            let limited = lower_root(receipt, module, root.owner(), cap).unwrap();
            assert_eq!(limited.completion(), Completion::ResourceLimit);
            assert!(limited.mir().is_none() && limited.findings().is_empty());
        }
    });
}

/// Rule 2 (review ci-uaa1, ri-px1.8 #3b): a non-`i32` guard and a guard field
/// of a generic record stop at their parameter, so no scope-exit release is
/// verified and nothing is charged for one. Caps measured at the parent
/// 36f1cf15; the graph was 185,689 / 3,979 / 3 there and the i32 equality
/// source root (ri-785.9) adds 316 work and 134 entries and sets depth 5.
#[test]
fn implicit_guard_siblings_keep_their_parent_caps() {
    probe(
        "graph::closed_release::implicit_guard_siblings_keep_their_parent_caps",
        "type Holder<T> = { lock: LockGuard<T>; }\nfunction wide(lock: LockGuard<i64>): void {}\nfunction generic(value: Holder<i32>): void {}",
        |_, module, receipt| {
            let short = |exact: AnalysisLimits| {
                [
                    AnalysisLimits {
                        max_work: exact.max_work - 1,
                        ..exact
                    },
                    AnalysisLimits {
                        max_entries: exact.max_entries - 1,
                        ..exact
                    },
                    AnalysisLimits {
                        max_depth: exact.max_depth - 1,
                        ..exact
                    },
                ]
            };
            let old = AnalysisLimits {
                max_work: 185_689,
                max_entries: 3_979,
                max_depth: 3,
            };
            assert_eq!(
                analyze_ownership(receipt, old).completion(),
                GraphCompletion::ResourceLimit
            );
            let exact = AnalysisLimits {
                max_work: 186_005,
                max_entries: 4_113,
                max_depth: 5,
            };
            let report = analyze_ownership(receipt, exact);
            assert_eq!(
                report.statistics(),
                Statistics {
                    work: 186_005,
                    entries: 4_113,
                    max_depth: 5
                }
            );
            assert_eq!(report.completion(), GraphCompletion::Incomplete);
            assert!(report.findings().is_empty());
            for cap in short(exact) {
                let limited = analyze_ownership(receipt, cap);
                assert_eq!(limited.completion(), GraphCompletion::ResourceLimit);
                assert!(
                    limited
                        .roots()
                        .iter()
                        .all(|root| root.ordinary_mir().is_none())
                );
            }
            for (name, (work, entries), site) in [
                (
                    &b"function wide"[..],
                    (1_841, 12),
                    &b"lock: LockGuard<i64>"[..],
                ),
                (b"function generic", (1_849, 12), b"value: Holder<i32>"),
            ] {
                let root = root(&report, name);
                assert_eq!(root.ordinary_completion(), Completion::Incomplete);
                assert!(root.ordinary_mir().is_none());
                assert_eq!(root.blocked_at().unwrap().span.bytes(), site);
                let BodyOwner::Declaration(id) = root.owner() else {
                    panic!("a function root")
                };
                // Depth 0: no one-short depth cap.
                let exact = AnalysisLimits {
                    max_work: work,
                    max_entries: entries,
                    max_depth: 0,
                };
                let target = lower_body(receipt, module, id, exact).unwrap();
                assert_eq!(target.completion(), Completion::Incomplete);
                assert_eq!(
                    target.statistics(),
                    Statistics {
                        work,
                        entries,
                        max_depth: 0
                    }
                );
                assert_eq!(target.blocked_at().unwrap().span.bytes(), site);
                assert!(target.mir().is_none() && target.findings().is_empty());
                for cap in &short(AnalysisLimits {
                    max_depth: 1,
                    ..exact
                })[..2]
                {
                    let limited = lower_body(
                        receipt,
                        module,
                        id,
                        AnalysisLimits {
                            max_depth: 0,
                            ..*cap
                        },
                    )
                    .unwrap();
                    assert_eq!(limited.completion(), Completion::ResourceLimit);
                    assert!(limited.mir().is_none() && limited.findings().is_empty());
                }
            }
        },
    );
}

/// Rule 2: the nearest unsupported sibling, a guard moved into a local
/// binding and released at scope exit, keeps the caps measured at the parent
/// (36f1cf15 with D1's branch), not re-measured on the candidate.
#[test]
fn implicit_guard_binding_release_keeps_its_parent_caps() {
    probe(
        "graph::closed_release::implicit_guard_binding_release_keeps_its_parent_caps",
        "function local(lock: LockGuard<i32>): void { let held = lock; }",
        |_, module, receipt| {
            let report = analyze_ownership(receipt, AnalysisLimits::default());
            let local = root(&report, b"function local");
            let targeted =
                lower_root(receipt, module, local.owner(), AnalysisLimits::default()).unwrap();
            assert_eq!(targeted.completion(), Completion::Checked);
            assert_eq!(
                targeted.statistics(),
                Statistics {
                    work: 1988,
                    entries: 76,
                    max_depth: 3
                }
            );
            // Rule 11 (ri-785.9): the parent graph cap 185,441 / 4,038 / 3 gives
            // ResourceLimit; the i32 equality source root adds 316 work and
            // 134 entries and sets depth 5.
            let exact = AnalysisLimits {
                max_work: 185757,
                max_entries: 4172,
                max_depth: 5,
            };
            assert_eq!(
                report.statistics(),
                Statistics {
                    work: exact.max_work,
                    entries: exact.max_entries,
                    max_depth: exact.max_depth
                }
            );
            let replay = analyze_ownership(receipt, exact);
            assert_eq!(replay.statistics(), report.statistics());
            for report in [&report, &replay] {
                assert_eq!(report.completion(), GraphCompletion::Incomplete);
                assert!(report.findings().is_empty());
                let local = root(report, b"function local");
                assert_eq!(local.ordinary_completion(), Completion::Incomplete);
                assert!(local.ordinary_mir().is_none());
                assert_eq!(
                    local.blocked_at().unwrap().span.bytes(),
                    b"{ let held = lock; }"
                );
                assert!(
                    matches!(local.dependencies(), [edge] if matches!(edge.contract, DependencyContract::ProtectedRelease { .. }))
                );
            }
            for short in [
                AnalysisLimits {
                    max_work: 185441,
                    max_entries: 4038,
                    max_depth: 3,
                },
                AnalysisLimits {
                    max_work: exact.max_work - 1,
                    ..exact
                },
                AnalysisLimits {
                    max_entries: exact.max_entries - 1,
                    ..exact
                },
                AnalysisLimits {
                    max_depth: exact.max_depth - 1,
                    ..exact
                },
            ] {
                let limited = analyze_ownership(receipt, short);
                assert_eq!(limited.completion(), GraphCompletion::ResourceLimit);
                assert!(limited.findings().is_empty());
                assert!(
                    limited
                        .roots()
                        .iter()
                        .all(|root| root.ordinary_mir().is_none())
                );
            }
        },
    );
}

#[test]
fn written_guard_release_closes_only_the_ordinary_caller_edge() {
    let source = format!(
        "{MAIN}
function releases(lock: LockGuard<i32>): void {{ dispose(lock); }}
function implicit(lock: LockGuard<i32>): void {{}}
function unsupported(lock: LockGuard<i64>): void {{ dispose(lock); }}
type Parcel = {{ text: string; }}
function projected(parcel: Parcel): void {{ dispose(parcel.text); }}
"
    );
    fixture_with_main(
        "graph::closed_release::written_guard_release_closes_only_the_ordinary_caller_edge",
        &source,
        &[HELPER],
        |_, _, graph, receipt, _| {
            let report = analyze_ownership(receipt, AnalysisLimits::default());
            let main = graph
                .modules()
                .iter()
                .find(|m| m.key.path == "main.rill")
                .unwrap();
            let caller = root(&report, b"function releases");
            let targeted =
                lower_root(receipt, main, caller.owner(), AnalysisLimits::default()).unwrap();
            assert_eq!(targeted.completion(), Completion::Checked);
            assert!(targeted.mir().is_some());
            assert_eq!(caller.ordinary_completion(), Completion::Checked);
            assert!(caller.ordinary_mir().is_some());
            assert!(matches!(caller.enabled_frame(), Delivery::Unresolved));
            let [edge] = caller.dependencies() else {
                panic!("{:?}", caller.dependencies())
            };
            let provider = standard_root(&report, "mutex.rill", b"implement<T> dispose");
            assert_eq!(BodyOwner::Declaration(edge.declaration), provider.owner());
            let DependencyContract::ProtectedRelease { entry } = edge.contract else {
                panic!("{edge:?}")
            };
            assert_eq!(entry.module, edge.declaration.module);
            assert_eq!(entry.kind, loader::DeclarationKind::Family);
            assert!(caller.ordinary_mir().unwrap().blocks.iter().any(|block| matches!(
            &block.operation, Operation::Cleanup(actions)
                if matches!(actions.as_slice(), [Obligation::ReleaseGuard { operand, implementation, entry: selected }]
                    if operand.root.span.bytes() == b"lock: LockGuard<i32>"
                        && *implementation == edge.declaration && *selected == entry)
        )));
            let projected = root(&report, b"function projected");
            let targeted =
                lower_root(receipt, main, projected.owner(), AnalysisLimits::default()).unwrap();
            assert!(targeted.mir().is_some());
            assert!(
                projected
                    .dependencies()
                    .iter()
                    .any(|edge| edge.contract == DependencyContract::BuiltinDispose)
            );
            // ri-px1.8 (D3): the scope-exit release now closes its root; see
            // `implicit_guard_release_closes_the_ordinary_root`.
            let implicit = root(&report, b"function implicit");
            assert_eq!(implicit.ordinary_completion(), Completion::Checked);
            assert!(implicit.ordinary_mir().is_some());
            for pending in [provider, root(&report, b"function unsupported"), projected] {
                assert_eq!(pending.ordinary_completion(), Completion::Incomplete);
                assert!(pending.ordinary_mir().is_none());
            }
            assert!(report.active_disposers().is_empty());
            assert!(report.findings().is_empty());
            assert_eq!(report.completion(), GraphCompletion::Incomplete);
        },
    );
}
