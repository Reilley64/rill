//! RFC 0007 O9 (ri-ema, rule R1): a `&mut List<&R>` destination holds the
//! element contents `&R`, like a `&mut &R` destination holds `&R`. The O9 row
//! in `rfc_cases.rs` pins the RFC's own module; these tests pin the forms
//! around it, the old caps (metering rule 11), the siblings that keep their
//! outcomes (rule 1) and the known conservative rejection.
use super::o12::{Case, check_cases};
use super::record_contents::{checked, incomplete, rejected};
use super::*;

const CONSERVATIVE: &str = r####"
type User = { id: i32 };
function retain<D, S>(dst: D, source: S): void;
implement retain(dst: &mut List<&User>, source: &User): void { }
function local(dst: &mut List<&User>): void {
  let user = User { id: 1 };
  retain(dst, &user);
}
"####;

const DIRECT: &str = r####"
type User = { id: i32 };
function retain(dst: &mut List<&User>, source: &User): void { }
function forwarded(dst: &mut List<&User>, source: &User): void { retain(dst, source); }
function local(dst: &mut List<&User>): void {
  let user = User { id: 1 };
  retain(dst, &user);
}
function keep(dst: &mut List<&User>): void {
  let user = User { id: 1 };
  let view = &user;
}
function twice(dst: &mut List<&User>, a: &User, b: &User): void {
  retain(dst, a);
  retain(dst, b);
}
"####;

const FAMILY_FORMS: &str = r####"
type User = { id: i32 };
function retain<D, S>(dst: D, source: S): void;
implement retain(dst: &mut List<&User>, source: &User): void { while (true) {} }
function twice(dst: &mut List<&User>, a: &User, b: &User): void {
  retain(dst, a);
  retain(dst, b);
}
function grouped(dst: &mut List<&User>, source: &User): void { retain((dst), source); }
function looped(dst: &mut List<&User>, source: &User): void {
  while (true) { retain(dst, source); }
}
function keep(dst: &mut List<&User>): void {
  let user = User { id: 1 };
  let view = &user;
}
"####;

const PAIR: &str = r####"
type User = { id: i32 };
function retain<D, S, T>(dst: D, a: S, b: T): void;
implement retain(dst: &mut List<&User>, a: &User, b: &User): void { while (true) {} }
function both(dst: &mut List<&User>, a: &User, b: &User): void { retain(dst, a, b); }
"####;

const ALONE: &str = r####"
type User = { id: i32 };
function retain<D>(dst: D): void;
implement retain(dst: &mut List<&User>): void { while (true) {} }
function forwarded(dst: &mut List<&User>): void { retain(dst); }
"####;

const ESCAPES: &str = r####"
type User = { id: i32 };
function retain<D, S>(dst: D, source: S): void;
implement retain(dst: &mut List<&User>, source: &User): void { while (true) {} }
function aliased(dst: &mut List<&User>): void {
  let user = User { id: 1 };
  let alias = dst;
  retain(alias, &user);
}
function viaView(dst: &mut List<&User>): void {
  let user = User { id: 1 };
  let view = &user;
  retain(dst, view);
}
function mixed(dst: &mut List<&User>, a: &User): void {
  let user = User { id: 1 };
  retain(dst, a);
  retain(dst, &user);
}
function branch(dst: &mut List<&User>, flag: bool): void {
  let user = User { id: 1 };
  if (flag) { retain(dst, &user); }
}
function dead(dst: &mut List<&User>): void {
  let user = User { id: 1 };
  if (false) { retain(dst, &user); }
}
function loopLocal(dst: &mut List<&User>): void {
  while (true) {
    let user = User { id: 1 };
    retain(dst, &user);
  }
}
function viaCall(dst: &mut List<&User>): void {
  let user = User { id: 1 };
  forward(dst, &user);
}
function forward(dst: &mut List<&User>, source: &User): void { retain(dst, source); }
"####;

const AFTER_RETAIN: &str = r####"
type User = { id: i32 };
function retain<D, S>(dst: D, source: S): void;
implement retain(dst: &mut List<&User>, source: &User): void { while (true) {} }
function reuse(dst: &mut List<&User>, source: &User): i32 {
  retain(dst, source);
  return source.id;
}
function reuseDst(dst: &mut List<&User>, source: &User): void {
  retain(dst, source);
  retain(dst, source);
}
"####;

const NAMED: &str = r####"
type Named = { name: string; id: i32 };
function retain<D, S>(dst: D, source: S): void;
implement retain(dst: &mut List<&Named>, source: &Named): void { while (true) {} }
function forwarded(dst: &mut List<&Named>, source: &Named): void { retain(dst, source); }
"####;

const NESTED: &str = r####"
type Inner = { id: i32 };
type Outer = { inner: Inner; id: i32 };
function retain<D, S>(dst: D, source: S): void;
implement retain(dst: &mut List<&Outer>, source: &Outer): void { while (true) {} }
function forwarded(dst: &mut List<&Outer>, source: &Outer): void { retain(dst, source); }
"####;

const ENUM: &str = r####"
enum Kind { A, B }
function retain<D, S>(dst: D, source: S): void;
implement retain(dst: &mut List<&Kind>, source: &Kind): void { while (true) {} }
function forwarded(dst: &mut List<&Kind>, source: &Kind): void { retain(dst, source); }
"####;

const GENERIC: &str = r####"
type Box<T> = { value: T };
function retain<D, S>(dst: D, source: S): void;
implement retain(dst: &mut List<&Box<i32>>, source: &Box<i32>): void { while (true) {} }
function forwarded(dst: &mut List<&Box<i32>>, source: &Box<i32>): void { retain(dst, source); }
"####;

const OWNED: &str = r####"
type User = { id: i32 };
function retain<D, S>(dst: D, source: S): void;
implement retain(dst: &mut List<User>, source: &User): void { while (true) {} }
function forwarded(dst: &mut List<User>, source: &User): void { retain(dst, source); }
"####;

const EXCLUSIVE: &str = r####"
type User = { id: i32 };
function retain<D, S>(dst: D, source: S): void;
implement retain(dst: &mut List<&mut User>, source: &mut User): void { while (true) {} }
function forwarded(dst: &mut List<&mut User>, source: &mut User): void { retain(dst, source); }
"####;

const SHARED_DESTINATION: &str = r####"
type User = { id: i32 };
function retain<D, S>(dst: D, source: S): void;
implement retain(dst: &List<&User>, source: &User): void { while (true) {} }
function forwarded(dst: &List<&User>, source: &User): void { retain(dst, source); }
"####;

const FLAG: &str = r####"
type User = { id: i32 };
function retain<D, S, F>(dst: D, source: S, flag: F): void;
implement retain(dst: &mut List<&User>, source: &User, flag: bool): void { while (true) {} }
function forwarded(dst: &mut List<&User>, source: &User, flag: bool): void { retain(dst, source, flag); }
"####;

const LOCAL_DESTINATION: &str = r####"
type User = { id: i32 };
function retain<D, S>(dst: D, source: S): void;
implement retain(dst: &mut List<&User>, source: &User): void { while (true) {} }
function fresh(source: &User): void {
  let mut users: List<&User> = [source];
  retain(&mut users, source);
}
"####;

const R0202_LOCAL: (&str, &str, &str) = ("R0202", "retain(dst, &user)", "&user");

/// R1's known conservative rejection (the chief's condition, ri-ema). The
/// implementation stores nothing, so the program is valid: `user` ends with
/// the call's caller and `dst` never holds it. Rule R1 reads only the
/// declared signature, assumes the `&User` input can flow into `dst`'s
/// elements and rejects the caller's body-local owner as an escape (R0202).
/// A later refinement (R2, an explicit storage contract) can turn this into
/// `Checked`; this test is its witness.
#[test]
fn r1_known_conservative_rejection_of_a_callee_that_stores_nothing() {
    installed(
        "rfc_cases::list_contents::r1_known_conservative_rejection_of_a_callee_that_stores_nothing",
        |request| {
            check_cases(
                &request,
                &[const {
                    Case {
                        text: CONSERVATIVE,
                        typed_stats: (327058, 48201, 12),
                        graph: (GraphCompletion::SourceRejected, (189799, 4419, 6)),
                        roots: &[rejected(
                            "local",
                            R0202_LOCAL.0,
                            R0202_LOCAL.1,
                            R0202_LOCAL.2,
                            (3696, 317, 6),
                        )],
                    }
                }],
            );
            old_root_caps(
                &request,
                &[(
                    CONSERVATIVE,
                    &[("local", (2262, 103, 3))],
                    (188363, 4204, 5),
                )],
            );
        },
    );
}

/// The direct-function form of O9: the contract is the declared signature,
/// the destination is the caller's `&mut List<&User>` parameter. Forwarding a
/// parameter, twice, and a view that is never stored check; a body-local
/// owner is R0202.
#[test]
fn direct_list_destination_checks_parameters_and_rejects_locals() {
    installed(
        "rfc_cases::list_contents::direct_list_destination_checks_parameters_and_rejects_locals",
        |request| {
            check_cases(
                &request,
                &[const {
                    Case {
                        text: DIRECT,
                        typed_stats: (330828, 48520, 12),
                        graph: (GraphCompletion::SourceRejected, (194154, 5367, 6)),
                        roots: &[
                            checked("retain", (2196, 149, 1)),
                            checked("forwarded", (4715, 386, 5)),
                            rejected(
                                "local",
                                R0202_LOCAL.0,
                                R0202_LOCAL.1,
                                R0202_LOCAL.2,
                                (3011, 360, 6),
                            ),
                            checked("keep", (2760, 305, 4)),
                            checked("twice", (5284, 590, 5)),
                        ],
                    }
                }],
            );
            old_root_caps(
                &request,
                &[(
                    DIRECT,
                    &[
                        ("retain", (2066, 121, 1)),
                        ("forwarded", (1995, 86, 3)),
                        ("local", (2356, 149, 3)),
                        ("keep", (2584, 260, 4)),
                        ("twice", (2001, 90, 3)),
                    ],
                    (191296, 4541, 5),
                )],
            );
        },
    );
}

/// Family calls with a list destination: two calls, a call in a loop, a pair
/// of sources, a lone destination and a source read after the call check. A
/// grouped destination keeps its parent blocker. Old caps give `ResourceLimit`.
#[test]
fn family_list_destination_forms() {
    installed(
        "rfc_cases::list_contents::family_list_destination_forms",
        |request| {
            check_cases(
                &request,
                &[
                    const {
                        Case {
                            text: FAMILY_FORMS,
                            typed_stats: (334497, 49369, 12),
                            graph: (GraphCompletion::Incomplete, (196629, 5148, 7)),
                            roots: &[
                                checked("twice", (4763, 488, 5)),
                                incomplete("grouped", "retain((dst), source)", (2069, 90, 3)),
                                checked("looped", (3562, 363, 7)),
                                checked("keep", (2769, 309, 4)),
                            ],
                        }
                    },
                    const {
                        Case {
                            text: PAIR,
                            typed_stats: (326861, 48231, 12),
                            graph: (GraphCompletion::Incomplete, (190024, 4414, 5)),
                            roots: &[checked("both", (3490, 312, 5))],
                        }
                    },
                    const {
                        Case {
                            text: ALONE,
                            typed_stats: (325974, 47893, 12),
                            graph: (GraphCompletion::Incomplete, (188935, 4258, 5)),
                            roots: &[checked("forwarded", (3133, 156, 5))],
                        }
                    },
                    const {
                        Case {
                            text: AFTER_RETAIN,
                            typed_stats: (329745, 48653, 12),
                            graph: (GraphCompletion::Incomplete, (193669, 4807, 5)),
                            roots: &[
                                checked("reuse", (3489, 322, 5)),
                                checked("reuseDst", (4630, 423, 5)),
                            ],
                        }
                    },
                ],
            );
            old_root_caps(
                &request,
                &[
                    (
                        FAMILY_FORMS,
                        &[
                            ("twice", (2000, 91, 3)),
                            ("grouped", (1995, 87, 3)),
                            ("looped", (1998, 88, 5)),
                            ("keep", (2593, 264, 4)),
                        ],
                        (192037, 4424, 5),
                    ),
                    (PAIR, &[("both", (1921, 51, 3))], (188446, 4152, 5)),
                    (ALONE, &[("forwarded", (1882, 31, 3))], (187679, 4132, 5)),
                ],
            );
        },
    );
}

/// Rule R1 never accepts an invalid program: a body-local owner stored
/// through an alias, a local view, a branch, a later call or a callee that
/// forwards it is R0202 at the call (a loop body that ends first is R0203);
/// the same store in dead code checks and rejects nothing (rule 6), and
/// forwarding a parameter checks.
#[test]
fn local_sources_escape_through_every_route_to_the_list() {
    installed(
        "rfc_cases::list_contents::local_sources_escape_through_every_route_to_the_list",
        |request| {
            check_cases(
                &request,
                &[const {
                    Case {
                        text: ESCAPES,
                        typed_stats: (347513, 51623, 12),
                        graph: (GraphCompletion::SourceRejected, (210865, 7021, 8)),
                        roots: &[
                            rejected(
                                "aliased",
                                "R0202",
                                "retain(alias, &user)",
                                "&user",
                                (4042, 485, 6),
                            ),
                            rejected(
                                "viaView",
                                "R0202",
                                "retain(dst, view)",
                                "&user",
                                (4034, 483, 5),
                            ),
                            rejected(
                                "mixed",
                                "R0202",
                                "retain(dst, &user)",
                                "&user",
                                (5290, 652, 6),
                            ),
                            rejected(
                                "branch",
                                "R0202",
                                "retain(dst, &user)",
                                "&user",
                                (4269, 606, 8),
                            ),
                            checked("dead", (2868, 371, 4)),
                            rejected(
                                "loopLocal",
                                "R0203",
                                r####"{\n    let user = User { id: 1 };\n    retain(dst, &user);\n  }"####,
                                "&user",
                                (4122, 525, 8),
                            ),
                            rejected(
                                "viaCall",
                                "R0202",
                                "forward(dst, &user)",
                                "&user",
                                (3166, 437, 6),
                            ),
                            checked("forward", (3540, 348, 5)),
                        ],
                    }
                }],
            );
        },
    );
}

/// Rule 1 for the siblings that pass the free type screen but not the record
/// lookup (a `string` field, a nested record, an enum) or the contract (an
/// extra `bool`), and the shapes the screen refuses on the type alone. Each
/// keeps its outcome and its blocker. The first four pay one lookup in the
/// parameter and one at the call (+148, +148, +144 work and +3 entries; `Flag`
/// pays the parameter's, +74); the rest pay nothing and keep their parent caps.
#[test]
fn list_destination_siblings_keep_their_outcomes_and_blockers() {
    installed(
        "rfc_cases::list_contents::list_destination_siblings_keep_their_outcomes_and_blockers",
        |request| {
            check_cases(
                &request,
                &[
                    const {
                        Case {
                            text: NAMED,
                            typed_stats: (326651, 48080, 12),
                            graph: (GraphCompletion::Incomplete, (188274, 4146, 5)),
                            roots: &[incomplete(
                                "forwarded",
                                "retain(dst, source)",
                                (2052, 45, 3),
                            )],
                        }
                    },
                    const {
                        Case {
                            text: NESTED,
                            typed_stats: (327653, 48126, 12),
                            graph: (GraphCompletion::Incomplete, (188340, 4147, 5)),
                            roots: &[incomplete(
                                "forwarded",
                                "retain(dst, source)",
                                (2054, 46, 3),
                            )],
                        }
                    },
                    const {
                        Case {
                            text: ENUM,
                            typed_stats: (326303, 48119, 12),
                            graph: (GraphCompletion::Incomplete, (188146, 4144, 5)),
                            roots: &[incomplete(
                                "forwarded",
                                "retain(dst, source)",
                                (2044, 43, 3),
                            )],
                        }
                    },
                    const {
                        Case {
                            text: FLAG,
                            typed_stats: (327151, 48195, 12),
                            graph: (GraphCompletion::Incomplete, (188396, 4153, 5)),
                            roots: &[incomplete(
                                "forwarded",
                                "retain(dst, source, flag)",
                                (1991, 52, 3),
                            )],
                        }
                    },
                    const {
                        Case {
                            text: GENERIC,
                            typed_stats: (327170, 48156, 12),
                            graph: (GraphCompletion::Incomplete, (188312, 4146, 5)),
                            roots: &[incomplete(
                                "forwarded",
                                "retain(dst, source)",
                                (1910, 45, 3),
                            )],
                        }
                    },
                    const {
                        Case {
                            text: OWNED,
                            typed_stats: (326361, 48033, 12),
                            graph: (GraphCompletion::Incomplete, (187939, 4140, 5)),
                            roots: &[incomplete(
                                "forwarded",
                                "retain(dst, source)",
                                (1898, 39, 3),
                            )],
                        }
                    },
                    const {
                        Case {
                            text: EXCLUSIVE,
                            typed_stats: (326414, 48062, 12),
                            graph: (GraphCompletion::Incomplete, (188063, 4142, 5)),
                            roots: &[incomplete(
                                "forwarded",
                                "retain(dst, source)",
                                (1902, 41, 3),
                            )],
                        }
                    },
                    const {
                        Case {
                            text: SHARED_DESTINATION,
                            typed_stats: (326414, 48062, 12),
                            graph: (GraphCompletion::Incomplete, (188063, 4142, 5)),
                            roots: &[incomplete(
                                "forwarded",
                                "retain(dst, source)",
                                (1902, 41, 3),
                            )],
                        }
                    },
                    const {
                        Case {
                            text: LOCAL_DESTINATION,
                            typed_stats: (327114, 48212, 12),
                            graph: (GraphCompletion::Incomplete, (188230, 4139, 5)),
                            roots: &[incomplete("fresh", "users", (1889, 38, 1))],
                        }
                    },
                ],
            );
            old_root_caps(
                &request,
                &[
                    (NAMED, &[("forwarded", (1904, 42, 3))], (188126, 4143, 5)),
                    (NESTED, &[("forwarded", (1906, 43, 3))], (188192, 4144, 5)),
                    (ENUM, &[("forwarded", (1900, 40, 3))], (188002, 4141, 5)),
                    (FLAG, &[("forwarded", (1917, 49, 3))], (188322, 4150, 5)),
                ],
            );
        },
    );
}

/// A source, the parent's cap of each named root and of the whole graph.
pub(super) type OldCaps<'a> = (&'a str, &'a [(&'a str, Triple)], Triple);

/// Metering rule 11: each `(source, roots, graph)` is the parent's exact cap
/// (`b6c93f30`). With the same source, every old cap now gives
/// `ResourceLimit` (no MIR), in both modes.
pub(super) fn old_root_caps(request: &InstalledRequest, olds: &[OldCaps<'_>]) {
    for (text, roots, whole) in olds {
        fs::write(&request.entry, format!("{IMPORTS}\n{text}")).unwrap();
        for test_mode in [false, true] {
            let request = InstalledRequest {
                test_mode,
                ..request.clone()
            };
            let graph = loader::load_installed(request).unwrap();
            let typed = analyze_typed(&graph, AnalysisLimits::default());
            let receipt = typed.receipt().unwrap();
            let module = super::o12::application(&graph);
            let (old, _) = limits(statistics(*whole));
            assert_eq!(
                analyze_ownership(&receipt, old).completion(),
                GraphCompletion::ResourceLimit
            );
            for (name, old) in *roots {
                let declaration = module
                    .declarations
                    .iter()
                    .find(|d| {
                        d.id.kind == DeclarationKind::Function && d.name.as_deref() == Some(name)
                    })
                    .unwrap();
                let (old, _) = limits(statistics(*old));
                let limited = lower_body(&receipt, module, &declaration.id, old).unwrap();
                assert_eq!(limited.completion(), Completion::ResourceLimit, "{name}");
                assert!(limited.mir().is_none() && limited.findings().is_empty());
            }
        }
    }
}
