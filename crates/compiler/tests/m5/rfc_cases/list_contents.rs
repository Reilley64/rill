//! RFC 0007 O9 (ri-ema, rule R1): a `&mut List<&R>` destination holds the
//! element contents `&R`, like a `&mut &R` destination holds `&R`. The O9 row
//! in `rfc_cases.rs` pins the RFC's own module; these tests pin the forms
//! around it, the old caps (metering rule 11), the siblings that keep their
//! outcomes (rule 1) and the known conservative rejection.
use super::o12::{Case, check_cases};
use super::record_contents::{checked, incomplete, rejected};
use super::*;
use compiler::analysis::ownership::{Place, RetainedPlace};
use compiler::analysis::typed::ResultUseBlocker;

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

const DIRECT_CALL: &str = r####"
type User = { id: i32 };
function retain(dst: &mut List<&User>, source: &User): void { }
function forwarded(dst: &mut List<&User>, source: &User): void { retain(dst, source); }
"####;

const TWO_CALLS: &str = r####"
type User = { id: i32 };
function retain<D, S>(dst: D, source: S): void;
implement retain(dst: &mut List<&User>, source: &User): void { while (true) {} }
function twice(dst: &mut List<&User>, a: &User, b: &User): void {
  retain(dst, a);
  retain(dst, b);
}
"####;

const LOOPED: &str = r####"
type User = { id: i32 };
function retain<D, S>(dst: D, source: S): void;
implement retain(dst: &mut List<&User>, source: &User): void { while (true) {} }
function looped(dst: &mut List<&User>, source: &User): void {
  while (true) { retain(dst, source); }
}
"####;

const AFTER_READ: &str = r####"
type User = { id: i32 };
function retain<D, S>(dst: D, source: S): void;
implement retain(dst: &mut List<&User>, source: &User): void { while (true) {} }
function reuse(dst: &mut List<&User>, source: &User): i32 {
  retain(dst, source);
  return source.id;
}
"####;

const LOOP_LOCAL: &str = r####"
type User = { id: i32 };
function retain<D, S>(dst: D, source: S): void;
implement retain(dst: &mut List<&User>, source: &User): void { while (true) {} }
function looped(dst: &mut List<&User>, source: &User): void {
  while (true) { retain(dst, source); }
}
function loopLocal(dst: &mut List<&User>): void {
  while (true) {
    let user = User { id: 1 };
    retain(dst, &user);
  }
}
function loopLocalReturn(dst: &mut List<&User>): void {
  while (true) {
    let user = User { id: 1 };
    retain(dst, &user);
    return;
  }
}
"####;

const OUTLIVES: &str = r####"
type User = { id: i32 };
function retain<D, S>(dst: D, source: S): void;
implement retain(dst: &mut List<&User>, source: &User): void { while (true) {} }
function writeAfter(dst: &mut List<&User>): void {
  let mut user = User { id: 1 };
  retain(dst, &user);
  user.id = 2;
}
function readAfter(dst: &mut List<&User>): void {
  let user = User { id: 1 };
  retain(dst, &user);
  let n = user.id;
}
function writeBefore(dst: &mut List<&User>): void {
  let mut user = User { id: 1 };
  user.id = 2;
  retain(dst, &user);
}
function viewReadThenWrite(): void {
  let mut user = User { id: 1 };
  let view = &user;
  let n = view.id;
  user.id = 2;
}
"####;

const EXCLUSIVE_SOURCE: &str = r####"
type User = { id: i32 };
function retain<D, S>(dst: D, source: S): void;
implement retain(dst: &mut List<&User>, source: &User): void { while (true) {} }
function viaReborrow(dst: &mut List<&User>, source: &mut User): void {
  retain(dst, &*source);
  source.id = 2;
}
"####;

const EXCLUSIVE_PASSED: &str = r####"
type User = { id: i32 };
function retain<D, S>(dst: D, source: S): void;
implement retain(dst: &mut List<&User>, source: &User): void { while (true) {} }
function passed(dst: &mut List<&User>, source: &mut User): void {
  retain(dst, source);
  source.id = 2;
}
"####;

const GROUPED: &str = r####"
type User = { id: i32 };
function retain<D, S>(dst: D, source: S): void;
implement retain(dst: &mut List<&User>, source: &User): void { while (true) {} }
function grouped(dst: &mut List<&User>, source: &User): void { retain((dst), source); }
"####;

const OLD_CONTENTS: &str = r####"
type User = { id: i32 };
function retain<D, S>(dst: D, source: S): void;
implement retain(dst: &mut List<&User>, source: &User): void { while (true) {} }
function forward(dst: &mut List<&User>, source: &User): void { retain(dst, source); }
function reversed(dst: &mut List<&User>, a: &User): void {
  let user = User { id: 1 };
  retain(dst, &user);
  retain(dst, a);
}
function reversedViaCall(dst: &mut List<&User>, a: &User): void {
  let user = User { id: 1 };
  retain(dst, &user);
  forward(dst, a);
}
"####;

const ORDER: &str = r####"
type User = { id: i32 };
function retain<D, S>(dst: D, source: S): void;
implement retain(dst: &mut List<&User>, source: &User): void { while (true) {} }
function paramThenLocal(dst: &mut List<&User>, a: &User): void {
  let user = User { id: 1 };
  retain(dst, a);
  retain(dst, &user);
}
function localThenParam(dst: &mut List<&User>, a: &User): void {
  let user = User { id: 1 };
  retain(dst, &user);
  retain(dst, a);
}
"####;

const OWNED_SOURCES: &str = r####"
type User = { id: i32 };
function retain<D, S>(dst: D, source: S): void;
implement retain(dst: &mut List<&User>, source: &User): void { while (true) {} }
function byValue(dst: &mut List<&User>, owned: User): void { retain(dst, &owned); }
function hop(dst: &mut List<&User>, owned: User): void {
  let view = &owned;
  retain(dst, view);
}
function oneBranch(dst: &mut List<&User>, owned: User, flag: bool): void {
  if (flag) { retain(dst, &owned); }
}
function loopOwned(dst: &mut List<&User>, owned: User, flag: bool): void {
  while (flag) { retain(dst, &owned); }
}
function loopBreak(dst: &mut List<&User>, owned: User): void {
  while (true) { retain(dst, &owned); break; }
}
"####;

const TEMPORARY_SOURCES: &str = r####"
type User = { id: i32 };
function retain<D, S>(dst: D, source: S): void;
implement retain(dst: &mut List<&User>, source: &User): void { while (true) {} }
function make(): User { return User { id: 1 }; }
function call(dst: &mut List<&User>): void { retain(dst, &make()); }
function literal(dst: &mut List<&User>): void { retain(dst, &User { id: 1 }); }
"####;

const PAIR_LOCAL: &str = r####"
type User = { id: i32 };
function retain<D, S, T>(dst: D, a: S, b: T): void;
implement retain(dst: &mut List<&User>, a: &User, b: &User): void { while (true) {} }
function localSecond(dst: &mut List<&User>, a: &User): void {
  let user = User { id: 1 };
  retain(dst, a, &user);
}
function localFirst(dst: &mut List<&User>, a: &User): void {
  let user = User { id: 1 };
  retain(dst, &user, a);
}
"####;

const LONE_LOCAL: &str = r####"
type User = { id: i32 };
function retain<D, S>(dst: D, source: S): void;
implement retain(dst: &mut List<&User>, source: &User): void { while (true) {} }
function touch<D>(dst: D): void;
implement touch(dst: &mut List<&User>): void { while (true) {} }
function loneAfterLocal(dst: &mut List<&User>): void {
  let user = User { id: 1 };
  retain(dst, &user);
  touch(dst);
}
function loneBeforeLocal(dst: &mut List<&User>): void {
  let user = User { id: 1 };
  touch(dst);
  retain(dst, &user);
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
/// keeps its outcome and its blocker. The first three pay one lookup in the
/// parameter and one at the call (+148, +148, +144 work and +3 entries); `Flag`
/// and `grouped` (`retain((dst), source)`, the fifth use of the standing rule-1
/// exception, review ci-0mdr ST-1) pay the parameter's only (+74 work, +3
/// entries); the rest pay nothing and keep their parent caps.
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
                            text: GROUPED,
                            typed_stats: (327397, 48238, 12),
                            graph: (GraphCompletion::Incomplete, (188200, 4146, 5)),
                            roots: &[incomplete(
                                "grouped",
                                "retain((dst), source)",
                                (1979, 45, 3),
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
                    (GROUPED, &[("grouped", (1905, 42, 3))], (188126, 4143, 5)),
                ],
            );
        },
    );
}

/// Condition 1 (chief of staff, 2026-11-24 12:35): the direct-function form,
/// no family. `forwarded` passes its own `dst` and `source` to `retain`: a
/// mutable reborrow of `dst`, a copy of the source, the retention into the
/// destination's contents, then the parameters end. An empty implementation
/// body checks (the call reads the declared signature).
#[test]
fn r1_direct_function_retain_checks_with_its_sequence() {
    pin_form(
        "rfc_cases::list_contents::r1_direct_function_retain_checks_with_its_sequence",
        const {
            Case {
                text: DIRECT_CALL,
                typed_stats: (324989, 47779, 12),
                graph: (GraphCompletion::Incomplete, (187650, 4387, 5)),
                roots: &[
                    checked("retain", (2102, 102, 1)),
                    checked("forwarded", (4527, 292, 5)),
                ],
            }
        },
        &[
            (
                "retain",
                &[
                    "0: goto 1",
                    "1: end source: &User; end dst: &mut List<&User>; goto 2",
                    "2: return",
                ],
            ),
            (
                "forwarded",
                &[
                    "0: reborrow mut dst: &mut List<&User> -> dst; goto 1",
                    "1: copy source: &User -> source; goto 2",
                    "2: retain through dst: &mut List<&User> <- source; goto 3",
                    "3: goto 4",
                    "4: goto 5",
                    "5: end source: &User; end dst: &mut List<&User>; goto 6",
                    "6: return",
                ],
            ),
        ],
    );
}

/// Condition 1: two calls into one destination. Each call reborrows `dst`
/// and retains its own source; both sources reach the contents in call order.
#[test]
fn r1_two_calls_check_with_their_sequence() {
    pin_form(
        "rfc_cases::list_contents::r1_two_calls_check_with_their_sequence",
        const {
            Case {
                text: TWO_CALLS,
                typed_stats: (327343, 48308, 12),
                graph: (GraphCompletion::Incomplete, (191217, 4551, 5)),
                roots: &[checked("twice", (4683, 448, 5))],
            }
        },
        &[(
            "twice",
            &[
                "0: reborrow mut dst: &mut List<&User> -> dst; goto 1",
                "1: copy a: &User -> a; goto 2",
                "2: retain through dst: &mut List<&User> <- a; goto 3",
                "3: goto 4",
                "4: reborrow mut dst: &mut List<&User> -> dst; goto 5",
                "5: copy b: &User -> b; goto 6",
                "6: retain through dst: &mut List<&User> <- b; goto 7",
                "7: goto 8",
                "8: goto 9",
                "9: end b: &User; end a: &User; end dst: &mut List<&User>; goto 10",
                "10: return",
            ],
        )],
    );
}

/// Condition 1: a call in a loop. The block graph holds the loop's backedge
/// (the last retain jumps to block 5) and no return: the loop never exits.
#[test]
fn r1_call_in_a_loop_checks_with_its_sequence() {
    pin_form(
        "rfc_cases::list_contents::r1_call_in_a_loop_checks_with_its_sequence",
        const {
            Case {
                text: LOOPED,
                typed_stats: (326532, 48095, 12),
                graph: (GraphCompletion::Incomplete, (189820, 4423, 7)),
                roots: &[checked("looped", (3476, 320, 7))],
            }
        },
        &[(
            "looped",
            &[
                "0: goto 1",
                "1: goto 2",
                "2: reborrow mut dst: &mut List<&User> -> dst; goto 3",
                "3: copy source: &User -> source; goto 4",
                "4: retain through dst: &mut List<&User> <- source; goto 5",
                "5: goto 6",
                "6: goto 7",
                "7: goto 8",
                "8: goto 9",
                "9: reborrow mut dst: &mut List<&User> -> dst; goto 10",
                "10: copy source: &User -> source; goto 11",
                "11: retain through dst: &mut List<&User> <- source; goto 5",
            ],
        )],
    );
}

/// Condition 1: a pair of `&User` sources, both staged before the one
/// retention, in parameter order.
#[test]
fn r1_pair_of_user_sources_checks_with_its_sequence() {
    pin_form(
        "rfc_cases::list_contents::r1_pair_of_user_sources_checks_with_its_sequence",
        const {
            Case {
                text: PAIR,
                typed_stats: (326861, 48231, 12),
                graph: (GraphCompletion::Incomplete, (190024, 4414, 5)),
                roots: &[checked("both", (3490, 312, 5))],
            }
        },
        &[(
            "both",
            &[
                "0: reborrow mut dst: &mut List<&User> -> dst; goto 1",
                "1: copy a: &User -> a; goto 2",
                "2: copy b: &User -> b; goto 3",
                "3: retain through dst: &mut List<&User> <- a, b; goto 4",
                "4: goto 5",
                "5: goto 6",
                "6: end b: &User; end a: &User; end dst: &mut List<&User>; goto 7",
                "7: return",
            ],
        )],
    );
}

/// Condition 1: a lone destination. `retain(dst)` adds no source and ends
/// none ("the contents keep every source they already held").
#[test]
fn r1_lone_destination_checks_with_its_sequence() {
    pin_form(
        "rfc_cases::list_contents::r1_lone_destination_checks_with_its_sequence",
        const {
            Case {
                text: ALONE,
                typed_stats: (325974, 47893, 12),
                graph: (GraphCompletion::Incomplete, (188935, 4258, 5)),
                roots: &[checked("forwarded", (3133, 156, 5))],
            }
        },
        &[(
            "forwarded",
            &[
                "0: reborrow mut dst: &mut List<&User> -> dst; goto 1",
                "1: retain through dst: &mut List<&User> <- nothing; goto 2",
                "2: goto 3",
                "3: goto 4",
                "4: end dst: &mut List<&User>; goto 5",
                "5: return",
            ],
        )],
    );
}

/// Condition 1: the source is read after the call. The retention does not
/// end the caller's `source`: the read (`deref`, then `export`) follows it.
#[test]
fn r1_source_read_after_the_call_checks_with_its_sequence() {
    pin_form(
        "rfc_cases::list_contents::r1_source_read_after_the_call_checks_with_its_sequence",
        const {
            Case {
                text: AFTER_READ,
                typed_stats: (326688, 48106, 12),
                graph: (GraphCompletion::Incomplete, (189806, 4408, 5)),
                roots: &[checked("reuse", (3457, 306, 5))],
            }
        },
        &[(
            "reuse",
            &[
                "0: reborrow mut dst: &mut List<&User> -> dst; goto 1",
                "1: copy source: &User -> source; goto 2",
                "2: retain through dst: &mut List<&User> <- source; goto 3",
                "3: goto 4",
                "4: deref source: &User; goto 5",
                "5: export source.id; goto 6",
                "6: goto 7",
                "7: end source: &User; end dst: &mut List<&User>; goto 8",
                "8: return",
            ],
        )],
    );
}

/// Condition 1: `keep` (a `&mut List<&User>` parameter and no call) is pinned
/// by name at 2,760 / 305 / 4 in the module of the direct-function test
/// (`DIRECT`, whose `local` rejects, so the graph is `SourceRejected`). The parent's 2,584 / 260 / 4 was never pinned; the move is a
/// contents slot and one record lookup (`ri-ema.md`).
#[test]
fn r1_keep_list_parameter_without_a_call_is_pinned() {
    pin_form(
        "rfc_cases::list_contents::r1_keep_list_parameter_without_a_call_is_pinned",
        const {
            Case {
                text: DIRECT,
                typed_stats: (330828, 48520, 12),
                graph: (GraphCompletion::SourceRejected, (194154, 5367, 6)),
                roots: &[
                    rejected(
                        "local",
                        R0202_LOCAL.0,
                        R0202_LOCAL.1,
                        R0202_LOCAL.2,
                        (3011, 360, 6),
                    ),
                    checked("keep", (2760, 305, 4)),
                ],
            }
        },
        &[(
            "keep",
            &[
                "0: init user.id; goto 1",
                "1: init user; goto 2",
                "2: borrow user -> view; goto 3",
                "3: goto 4",
                "4: end view; end user.id; end user; goto 5",
                "5: end dst: &mut List<&User>; goto 6",
                "6: return",
            ],
        )],
    );
}

/// Condition 2: the loop form's rejecting twin. A `User` created inside the
/// loop body and retained into `dst` cannot outlive the iteration. When the
/// body ends first the finding is R0203 at the body block (as for
/// `stored_source_loans_reach_the_caller_view`'s `early`: the scope's end
/// releases a loan the destination still needs); when the body leaves the
/// function by `return` while the destination holds it, the finding is R0202
/// at the call, related the local's borrow. `looped` (a parameter source)
/// checks beside them.
#[test]
fn r1_loop_body_local_source_retained_into_dst_rejects() {
    installed(
        "rfc_cases::list_contents::r1_loop_body_local_source_retained_into_dst_rejects",
        |request| {
            check_cases(
                &request,
                &[const {
                    Case {
                        text: LOOP_LOCAL,
                        typed_stats: (332728, 49168, 12),
                        graph: (GraphCompletion::SourceRejected, (196274, 5223, 8)),
                        roots: &[
                            checked("looped", (3546, 355, 7)),
                            rejected(
                                "loopLocal",
                                "R0203",
                                r####"{\n    let user = User { id: 1 };\n    retain(dst, &user);\n  }"####,
                                "&user",
                                (3960, 444, 8),
                            ),
                            rejected(
                                "loopLocalReturn",
                                R0202_LOCAL.0,
                                R0202_LOCAL.1,
                                R0202_LOCAL.2,
                                (3936, 441, 8),
                            ),
                        ],
                    }
                }],
            );
        },
    );
}

/// Condition 3 (RFC 0007 line 483, "Ending the call loan does not erase the
/// stored source"): after `retain(dst, &user)` returns, the call's own loan
/// is over but the source is still held by `dst`, which lives to the end of
/// the function. A write to the source in the next statement needs it
/// exclusively and rejects, R0203 at the write, related the borrow; an
/// ordinary view's loan ends at its last use, so the same write after the
/// view's last read checks (`viewReadThenWrite`). The write before the call
/// and a read after it reject at the call (R0202, a body-local owner stored
/// into a parameter's list): only the write after the call reaches the
/// loan-outlives-the-call judgment. Stated limit: a use that needs a
/// caller-provided source released or exclusively borrowed is not
/// expressible today (an `&mut User` source is typed `Incomplete`, a
/// caller-owned list is `Incomplete` at its call), so this judgment is
/// reached through a body-local owner only; see `ri-ema.md`.
#[test]
fn r1_stored_source_outlives_the_call_loan() {
    installed(
        "rfc_cases::list_contents::r1_stored_source_outlives_the_call_loan",
        |request| {
            check_cases(
                &request,
                &[const {
                    Case {
                        text: OUTLIVES,
                        typed_stats: (336144, 49622, 12),
                        graph: (GraphCompletion::SourceRejected, (198597, 5538, 6)),
                        roots: &[
                            rejected(
                                "writeAfter",
                                "R0203",
                                "user.id = 2;",
                                "&user",
                                (3924, 431, 6),
                            ),
                            rejected(
                                "readAfter",
                                R0202_LOCAL.0,
                                R0202_LOCAL.1,
                                R0202_LOCAL.2,
                                (3909, 422, 6),
                            ),
                            rejected(
                                "writeBefore",
                                R0202_LOCAL.0,
                                R0202_LOCAL.1,
                                R0202_LOCAL.2,
                                (4006, 472, 6),
                            ),
                            checked("viewReadThenWrite", (2761, 346, 4)),
                        ],
                    }
                }],
            );
        },
    );
}

/// RFC 0007 R1: "After the call, the contents of `dst` carry the sources of
/// that input and their old contents" (review ci-0mdr SP-1). A body-local
/// source stored first is still in the contents after a later retain of a
/// parameter, directly or through a forwarding function, so each is R0202; a
/// strong update (the second call replacing the first source) would check
/// both. `forward` (a parameter alone) checks beside them.
#[test]
fn r1_old_contents_survive_a_later_retain() {
    installed(
        "rfc_cases::list_contents::r1_old_contents_survive_a_later_retain",
        |request| {
            check_cases(
                &request,
                &[const {
                    Case {
                        text: OLD_CONTENTS,
                        typed_stats: (333755, 49416, 12),
                        graph: (GraphCompletion::SourceRejected, (197880, 5390, 6)),
                        roots: &[
                            checked("forward", (3384, 270, 5)),
                            rejected(
                                "reversed",
                                "R0202",
                                "retain(dst, a)",
                                "&user",
                                (5130, 572, 6),
                            ),
                            rejected(
                                "reversedViaCall",
                                "R0202",
                                "forward(dst, a)",
                                "&user",
                                (4360, 572, 6),
                            ),
                        ],
                    }
                }],
            );
        },
    );
}

/// Review ci-0mdr SP-6. The R0202 for a body-local source stored into `dst` is
/// reported at the last call that retains into `dst` before the exit, with the
/// body-local borrow as the related site, not at the call that stored the
/// local. `paramThenLocal` stores the local last (site `retain(dst, &user)`);
/// `localThenParam` stores it first and a valid parameter call follows (site
/// `retain(dst, a)`, the valid call). Both still reject; the rule gives the
/// exit's last retention as the site, and this pins it.
#[test]
fn r1_local_then_parameter_reports_the_storing_call() {
    installed(
        "rfc_cases::list_contents::r1_local_then_parameter_reports_the_storing_call",
        |request| {
            check_cases(
                &request,
                &[const {
                    Case {
                        text: ORDER,
                        typed_stats: (332006, 49227, 12),
                        graph: (GraphCompletion::SourceRejected, (196271, 5172, 6)),
                        roots: &[
                            rejected(
                                "paramThenLocal",
                                "R0202",
                                "retain(dst, &user)",
                                "&user",
                                (5108, 561, 6),
                            ),
                            rejected(
                                "localThenParam",
                                "R0202",
                                "retain(dst, a)",
                                "&user",
                                (5104, 559, 6),
                            ),
                        ],
                    }
                }],
            );
        },
    );
}

/// RFC 0007 R1: exits "must not retain body-local owners, temporary storage or
/// destroyed owned-parameter storage" (review ci-0mdr SP-2). A by-value
/// parameter's storage ends with the call, so retaining a view of it into
/// `dst` is R0202 directly, through a local view, on one branch, in a loop
/// whose condition can fail and in a loop that breaks. A temporary source (a
/// call result or a literal) is not expressible today: the targeted query is
/// `Incomplete` at the temporary, so no unchecked shape reaches `dst` (a stated
/// limit, the O12 call-result rule has no list destination).
#[test]
fn r1_owned_parameter_and_temporary_sources_do_not_escape() {
    installed(
        "rfc_cases::list_contents::r1_owned_parameter_and_temporary_sources_do_not_escape",
        |request| {
            check_cases(
                &request,
                &[const {
                    Case {
                        text: OWNED_SOURCES,
                        typed_stats: (339046, 50101, 12),
                        graph: (GraphCompletion::SourceRejected, (201987, 5871, 8)),
                        roots: &[
                            rejected(
                                "byValue",
                                "R0202",
                                "retain(dst, &owned)",
                                "&owned",
                                (3689, 314, 6),
                            ),
                            rejected(
                                "hop",
                                "R0202",
                                "retain(dst, view)",
                                "&owned",
                                (3811, 372, 5),
                            ),
                            rejected(
                                "oneBranch",
                                "R0202",
                                "retain(dst, &owned)",
                                "&owned",
                                (3974, 459, 8),
                            ),
                            rejected(
                                "loopOwned",
                                "R0202",
                                "retain(dst, &owned)",
                                "&owned",
                                (4234, 584, 8),
                            ),
                            rejected(
                                "loopBreak",
                                "R0202",
                                "retain(dst, &owned)",
                                "&owned",
                                (3809, 376, 8),
                            ),
                        ],
                    }
                }],
            );
            check_cases(
                &request,
                &[const {
                    Case {
                        text: TEMPORARY_SOURCES,
                        typed_stats: (331121, 48738, 12),
                        graph: (GraphCompletion::Incomplete, (191890, 4301, 6)),
                        roots: &[
                            checked("make", (2363, 154, 4)),
                            incomplete("call", "make()", (2941, 63, 6)),
                            incomplete("literal", "User { id: 1 }", (2941, 63, 6)),
                        ],
                    }
                }],
            );
        },
    );
}

/// Rejecting twin of `r1_pair_of_user_sources_checks_with_its_sequence` (review
/// ci-0mdr SP-5, RFC 0007: a rule "never accepts an invalid program"): one
/// body-local source in a pair is R0202 at the call, whichever position holds it.
#[test]
fn r1_pair_with_one_body_local_source_rejects() {
    installed(
        "rfc_cases::list_contents::r1_pair_with_one_body_local_source_rejects",
        |request| {
            check_cases(
                &request,
                &[const {
                    Case {
                        text: PAIR_LOCAL,
                        typed_stats: (331050, 49095, 12),
                        graph: (GraphCompletion::SourceRejected, (193688, 4884, 6)),
                        roots: &[
                            rejected(
                                "localSecond",
                                "R0202",
                                "retain(dst, a, &user)",
                                "&user",
                                (3901, 415, 6),
                            ),
                            rejected(
                                "localFirst",
                                "R0202",
                                "retain(dst, &user, a)",
                                "&user",
                                (3899, 414, 6),
                            ),
                        ],
                    }
                }],
            );
        },
    );
}

/// Rejecting twin of `r1_lone_destination_checks_with_its_sequence` (review
/// ci-0mdr SP-5): a lone-destination call adds no source but keeps "every
/// source they already held", so a body-local source stored before or after
/// it still reaches the exit. R0202 is at the last retaining call: `touch(dst)`
/// when the local was stored first, the storing call when it came last.
#[test]
fn r1_lone_destination_keeps_a_stored_local_rejected() {
    installed(
        "rfc_cases::list_contents::r1_lone_destination_keeps_a_stored_local_rejected",
        |request| {
            check_cases(
                &request,
                &[const {
                    Case {
                        text: LONE_LOCAL,
                        typed_stats: (333673, 49207, 12),
                        graph: (GraphCompletion::SourceRejected, (197402, 5006, 6)),
                        roots: &[
                            rejected(
                                "loneAfterLocal",
                                "R0202",
                                "touch(dst)",
                                "&user",
                                (4951, 475, 6),
                            ),
                            rejected(
                                "loneBeforeLocal",
                                "R0202",
                                "retain(dst, &user)",
                                "&user",
                                (4960, 478, 6),
                            ),
                        ],
                    }
                }],
            );
        },
    );
}

/// The stated limit of condition 3, pinned: a caller-provided `&mut User`
/// source cannot reach `retain` today. The typed pass stops (`Incomplete`, no
/// receipt, so the ownership pass never sees the call), for the reborrow
/// `&*source` and for passing `source` itself; a use that needs a
/// caller-provided source exclusively while `dst` is live is not expressible.
/// Rule 8: the outcome is `Incomplete` at exact typed caps, no ownership
/// receipt is published, and one short on work gives `ResourceLimit`. The
/// typed report names the stop in its Result-use pending list (review
/// ci-0mdr ST-10): the write `source.id = 2;` (`Storage`) for the reborrow,
/// and for passing `source` itself the call (`TypedInput`, with body finding
/// R0300 at the call: the `&mut User` argument does not meet the `&User`
/// parameter) and the write. It has no body obligation and no core blocker.
#[test]
fn r1_caller_provided_exclusive_source_stops_typed() {
    installed(
        "rfc_cases::list_contents::r1_caller_provided_exclusive_source_stops_typed",
        |request| {
            for (module, stats, pending, findings) in [
                (
                    EXCLUSIVE_SOURCE,
                    (326581, 48099, 12),
                    &[(ResultUseBlocker::Storage, "source.id = 2;")][..],
                    &[][..],
                ),
                (
                    EXCLUSIVE_PASSED,
                    (326114, 47888, 12),
                    &[
                        (ResultUseBlocker::TypedInput, "retain(dst, source)"),
                        (ResultUseBlocker::Storage, "source.id = 2;"),
                    ][..],
                    &[("R0300", "retain(dst, source)")][..],
                ),
            ] {
                fs::write(&request.entry, format!("{IMPORTS}\n{module}")).unwrap();
                for test_mode in [false, true] {
                    let request = InstalledRequest {
                        test_mode,
                        ..request.clone()
                    };
                    let graph = loader::load_installed(request).unwrap();
                    let typed = analyze_typed(&graph, AnalysisLimits::default());
                    assert_eq!(typed.completion(), TypedCompletion::Incomplete);
                    assert_eq!(typed.statistics(), statistics(stats));
                    assert!(typed.receipt().is_none());
                    let blockers: Vec<_> = typed
                        .result_use()
                        .unwrap()
                        .pending()
                        .iter()
                        .map(|p| (p.blocker, text(&p.site)))
                        .collect();
                    let pending: Vec<_> =
                        pending.iter().map(|(b, s)| (*b, s.to_string())).collect();
                    assert_eq!(blockers, pending);
                    let body_findings: Vec<_> = typed
                        .bodies()
                        .findings()
                        .iter()
                        .map(|f| (f.kind.code().unwrap(), text(&f.site)))
                        .collect();
                    let findings: Vec<_> =
                        findings.iter().map(|(c, s)| (*c, s.to_string())).collect();
                    assert_eq!(body_findings, findings);
                    let (exact, short) = limits(statistics(stats));
                    let replay = analyze_typed(&graph, exact);
                    assert_eq!(replay.completion(), TypedCompletion::Incomplete);
                    assert_eq!(replay.statistics(), typed.statistics());
                    for short in short {
                        let limited = analyze_typed(&graph, short);
                        crate::limit::assert_typed_limit_keeping_body_findings(&limited);
                    }
                }
            }
        },
    );
}

/// One module, its caps (exact and one short, both modes, the targeted query
/// and the ordinary graph root: `check_cases`) and, for each named `Checked`
/// root, the whole MIR sequence of the targeted query and of the graph
/// root's ordinary MIR (rule 9).
fn pin_form(test: &'static str, case: Case, sequences: &[(&str, &[&str])]) {
    installed(test, |request| {
        check_cases(&request, std::slice::from_ref(&case));
        fs::write(&request.entry, format!("{IMPORTS}\n{}", case.text)).unwrap();
        for test_mode in [false, true] {
            let request = InstalledRequest {
                test_mode,
                ..request.clone()
            };
            let graph = loader::load_installed(request).unwrap();
            let typed = analyze_typed(&graph, AnalysisLimits::default());
            let receipt = typed.receipt().unwrap();
            let module = super::o12::application(&graph);
            let whole = analyze_ownership(&receipt, AnalysisLimits::default());
            for (name, expected) in sequences {
                let declaration = module
                    .declarations
                    .iter()
                    .find(|d| {
                        d.id.kind == DeclarationKind::Function && d.name.as_deref() == Some(*name)
                    })
                    .unwrap();
                let targeted =
                    lower_body(&receipt, module, &declaration.id, AnalysisLimits::default())
                        .unwrap();
                assert_eq!(targeted.completion(), Completion::Checked, "{name}");
                assert_eq!(&sequence(targeted.mir().unwrap()), expected, "{name}");
                let root = whole
                    .roots()
                    .iter()
                    .find(|r| r.site().module == &module.key && r.site().node == declaration.node)
                    .unwrap();
                assert_eq!(root.ordinary_completion(), Completion::Checked, "{name}");
                assert_eq!(&sequence(root.ordinary_mir().unwrap()), expected, "{name}");
            }
        }
    });
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
            crate::limit::assert_graph_limit(&analyze_ownership(&receipt, old));
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
                crate::limit::assert_body_limit(&limited);
            }
        }
    }
}

fn place(place: &Place<'_>) -> String {
    let root = String::from_utf8_lossy(place.root.span.bytes()).into_owned();
    match place.field() {
        Some(field) => format!("{root}.{}", field.name.as_deref().unwrap()),
        None => root,
    }
}

fn destination(destination: &RetainedPlace<'_>) -> String {
    match destination {
        RetainedPlace::Local(value) => format!("local {}", place(value)),
        RetainedPlace::ThroughParameter(value) => format!("through {}", place(value)),
        other => panic!("unexpected destination {other:?}"),
    }
}

fn terminator(terminator: &Terminator) -> String {
    match terminator {
        Terminator::Goto(next) => format!("goto {next}"),
        Terminator::Branch { yes, no } => format!("branch {yes} {no}"),
        Terminator::Return => "return".to_string(),
        other => panic!("unexpected terminator {other:?}"),
    }
}

/// The whole checked MIR, one entry per block that does something (its index,
/// its action and its terminator; a block that only jumps is its terminator),
/// in block order: straight lines and loops alike (rule 9).
pub(super) fn sequence(mir: &Mir<'_>) -> Vec<String> {
    let mut events = Vec::new();
    for (index, block) in mir.blocks.iter().enumerate() {
        let action = match &block.operation {
            Operation::None => String::new(),
            Operation::Initialize(value) => format!("init {}", place(value)),
            Operation::Copy {
                from,
                into: Some(into),
            } => format!("copy {} -> {}", place(from), place(into)),
            Operation::Reborrow {
                parent,
                holder,
                exclusive,
            } => format!(
                "reborrow{} {} -> {}",
                if *exclusive { " mut" } else { "" },
                place(parent),
                place(holder)
            ),
            Operation::Borrow {
                source,
                holder,
                exclusive,
            } => format!(
                "borrow{} {} -> {}",
                if *exclusive { " mut" } else { "" },
                place(source),
                place(holder)
            ),
            Operation::CallRetaining {
                destination: to,
                input,
                ..
            } => format!("retain {} <- {}", destination(to), place(input)),
            Operation::CallRetainingNoInput {
                destination: to, ..
            } => format!("retain {} <- nothing", destination(to)),
            Operation::CallRetainingPair {
                destination: to,
                inputs,
                ..
            } => format!(
                "retain {} <- {}, {}",
                destination(to),
                place(&inputs[0]),
                place(&inputs[1])
            ),
            Operation::Dereference(value) => format!("deref {}", place(value)),
            Operation::Export(value) => format!("export {}", place(value)),
            Operation::Cleanup(actions) => {
                let ends: Vec<_> = actions
                    .iter()
                    .map(|action| match action {
                        Obligation::EndStorage(value) => format!("end {}", place(value)),
                        other => panic!("unexpected cleanup {other:?}"),
                    })
                    .collect();
                ends.join("; ")
            }
            other => panic!("unexpected operation {}", &format!("{other:?}")[..40]),
        };
        let line = if action.is_empty() {
            terminator(&block.terminator)
        } else {
            format!("{action}; {}", terminator(&block.terminator))
        };
        events.push(format!("{index}: {line}"));
    }
    events
}
