//! The two controls of the bounded `copy(&closure)` and two-job join approvals,
//! pinned at their current outcome and site (ri-785.8, row B4).
//!
//! Both controls stop in the typed (M4) stage: no receipt is issued, so the M5
//! target query and the ordinary graph root have nothing to judge. The typed
//! outcome is what is pinned, in both load modes, with the exact cap and each
//! independently one-short work, entries and depth. These are compiler
//! judgments, not Rill execution.
use super::*;
use compiler::analysis::{
    Statistics,
    scalars::ScalarUnsupported::{self, FamilySelection, Requirements},
    typed::TypedCompletion,
};

struct Pin<'a> {
    source: &'a str,
    completion: TypedCompletion,
    used: Statistics,
    /// Every unsupported-feature obligation, in report order, with its site text.
    obligations: &'a [(ScalarUnsupported, &'a str)],
}

fn stats(work: usize, entries: usize, max_depth: usize) -> Statistics {
    Statistics {
        work,
        entries,
        max_depth,
    }
}

fn assert_pinned(test: &str, pin: &Pin) {
    probe_report(test, pin.source, |request, module, typed| {
        assert_eq!(typed.completion(), pin.completion, "{}", pin.source);
        assert_ne!(typed.completion(), TypedCompletion::SourceRejected);
        assert_eq!(typed.statistics(), pin.used, "{}", pin.source);
        assert_eq!(
            typed.receipt().is_some(),
            pin.completion == TypedCompletion::Checked
        );
        assert!(typed.bodies().findings().is_empty());
        assert!(typed.core_blockers().is_empty());
        let obligations: Vec<_> = typed
            .bodies()
            .obligations()
            .iter()
            .map(|o| {
                assert_eq!(o.site.module, &module.key);
                (o.reason, o.site)
            })
            .collect();
        assert_eq!(
            obligations
                .iter()
                .map(|(reason, site)| (*reason, site.span.bytes().escape_ascii().to_string()))
                .collect::<Vec<_>>(),
            pin.obligations
                .iter()
                .map(|(reason, site)| (*reason, site.escape_default().to_string()))
                .collect::<Vec<_>>(),
        );
        for (reason, site) in obligations {
            if matches!(reason, FamilySelection | Requirements) {
                assert!(typed.bodies().node(module, site.node).unwrap().is_none());
            }
        }

        let graph = loader::load_installed(request.clone()).unwrap();
        let exact = AnalysisLimits {
            max_work: pin.used.work,
            max_entries: pin.used.entries,
            max_depth: pin.used.max_depth,
        };
        let bounded = analyze_typed(&graph, exact);
        assert_eq!(bounded.completion(), pin.completion);
        assert_eq!(bounded.statistics(), pin.used);
        for short in short_capture_limits(exact) {
            let limited = analyze_typed(&graph, short);
            crate::limit::assert_typed_limit(&limited);
        }
    });
}

/// A fresh nominal wrapper whose `copy` builds independent state is not
/// forbidden by containment: it stays `Incomplete` at the `copy` call, with no
/// finding, in both modes.
#[test]
fn fresh_wrapper_copy_is_incomplete_not_rejected() {
    const TEST: &str = "captures::approval_controls::fresh_wrapper_copy_is_incomplete_not_rejected";
    const DECLARATION: &str = "implement copy(value: &Holder): Holder {\n    let fresh: once () => bool = () => true;\n    return Holder { task: fresh };\n}";
    for (source, used) in [
        (
            r#"
type Holder = { task: once () => bool; };
implement copy(value: &Holder): Holder {
    let fresh: once () => bool = () => true;
    return Holder { task: fresh };
}
function probe(seed: bool): void {
    let mut flag = seed;
    let task: once () => bool = () => { let _ = &mut flag; return seed; };
    let packed = Holder { task: task };
    let _ = copy(&packed);
    let _ = packed;
}
"#,
            stats(432_230, 53_229, 12),
        ),
        (
            r#"
type Holder = { task: once () => bool; };
implement copy(value: &Holder): Holder {
    let fresh: once () => bool = () => true;
    return Holder { task: fresh };
}
function probe(seed: bool): void {
    let task: once () => bool = () => seed;
    let packed = Holder { task: task };
    let _ = copy(&packed);
    let _ = packed;
}
"#,
            stats(432_010, 53_132, 12),
        ),
    ] {
        assert_pinned(
            TEST,
            &Pin {
                source,
                completion: TypedCompletion::Incomplete,
                used,
                obligations: &[
                    (ScalarUnsupported::DeclarationPrerequisite, DECLARATION),
                    (
                        ScalarUnsupported::DeclarationPrerequisite,
                        "Holder { task: task }",
                    ),
                    (ScalarUnsupported::BindingType, "packed"),
                    (FamilySelection, "copy(&packed)"),
                    (ScalarUnsupported::BindingType, "packed"),
                ],
            },
        );
    }
}

/// A `LockGuard` is shareable but not sendable. A job that captures one never
/// enters the join: selection stops at the `join` call, while the same join
/// without the guard checks and issues a receipt.
#[test]
fn guard_job_does_not_enter_the_join() {
    const TEST: &str = "captures::approval_controls::guard_job_does_not_enter_the_join";
    const JOIN: &str = "import std::routine::join;";
    let list_join = |parameters: &str, second: &str| {
        format!(
            "{JOIN}\nfunction joined(input: List<i32>{parameters}): [i32, i32] {{
    let numbers = input;
    let [first, second] = join([() => (&numbers)[0u64], {second}]);
    dispose(numbers);
    return [first, second];
}}\n"
        )
    };
    let control = list_join("", "() => (&numbers)[1u64]");
    let guarded = list_join(
        ", lease: LockGuard<i32>",
        "() => { let _ = lease; return (&numbers)[1u64]; }",
    );
    assert_pinned(
        TEST,
        &Pin {
            source: &control,
            completion: TypedCompletion::Checked,
            used: stats(440_989, 55_101, 12),
            obligations: &[],
        },
    );
    assert_pinned(
        TEST,
        &Pin {
            source: &guarded,
            completion: TypedCompletion::Incomplete,
            used: stats(441_006, 54_809, 13),
            obligations: &[
                (
                    FamilySelection,
                    "join([() => (&numbers)[0u64], () => { let _ = lease; return (&numbers)[1u64]; }])",
                ),
                (ScalarUnsupported::BindingType, "second"),
                (ScalarUnsupported::BindingType, "first"),
            ],
        },
    );
    let direct = "function joined(lease: LockGuard<i32>): [bool, bool] {
    return join([() => true, () => { let _ = lease; return true; }]);
}";
    assert_pinned(
        TEST,
        &Pin {
            source: &format!("{JOIN}\n{direct}\n"),
            completion: TypedCompletion::Incomplete,
            used: stats(437_855, 54_206, 12),
            obligations: &[(
                FamilySelection,
                "join([() => true, () => { let _ = lease; return true; }])",
            )],
        },
    );
}

/// The same guard capture against a bare `sendable` or `shareable` requirement
/// is `Incomplete` at the requirement, not rejected with `R0302`.
#[test]
fn guard_capture_requirements_are_incomplete_at_the_call() {
    const TEST: &str =
        "captures::approval_controls::guard_capture_requirements_are_incomplete_at_the_call";
    for (predicate, used) in [
        ("sendable", stats(428_925, 52_913, 12)),
        ("shareable", stats(428_926, 52_913, 12)),
    ] {
        let source = format!(
            "function need<T>(value: T): bool where {predicate}(T) {{ let _ = value; return true; }}
function probe(lease: LockGuard<i32>): bool {{ return need(() => {{ let _ = lease; return true; }}); }}\n"
        );
        assert_pinned(
            TEST,
            &Pin {
                source: &source,
                completion: TypedCompletion::Incomplete,
                used,
                obligations: &[(Requirements, "need(() => { let _ = lease; return true; })")],
            },
        );
    }
}
