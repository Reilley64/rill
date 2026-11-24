//! RFC 0007 O2 and O8 (ri-1up, retained-store B): a `&mut &Record` parameter
//! holds retained contents like `&mut &bool` and `&mut &string` do, and a
//! scalar field read through a `&Record` is lowered. The O2 and O8 rows above
//! pin the RFC's own modules; these tests pin the forms around them, the
//! operation sequence, the old caps (metering rule 11) and the siblings.
use super::o12::{Case, check_cases};
use super::*;
use compiler::analysis::ownership::RetainedPlace;

const READS: &str = r####"
type Point = { x: i32; y: i32 };
function viaParam(view: &Point): i32 { return view.x; }
function second(view: &Point): i32 { return view.y; }
function bound(view: &Point): i32 { let n = view.x; return n; }
function grouped(view: &Point): i32 { return (view).x; }
function looped(view: &Point): i32 { while (true) { return view.x; } return 0i32; }
function named(): i32 {
  let first = Point { x: 1, y: 2 };
  let view = &first;
  return view.x;
}
function late(): i32 {
  let first = Point { x: 1, y: 2 };
  let view = &first;
  let moved = first;
  return view.x;
}
"####;

/// B1: a scalar field read through a named `&Record` (a parameter or a local
/// view), bare, bound, grouped and in a loop. At `bc9bfdd5` each root was
/// `Incomplete` at its read (1,978 / 81 / 3 for `viaParam`; whole graph
/// 191,726 / 4,469 / 5); these are the new caps, measured on the candidate.
/// `late` reads the view after its owner moved: still `Checked` here, as the
/// existing read of a `bool` view is (the move rule belongs to the owner's
/// storage, not to this read).
#[test]
fn record_view_reads_check() {
    installed(
        "rfc_cases::record_contents::record_view_reads_check",
        |request| {
            check_cases(
                &request,
                &[const {
                    Case {
                        text: READS,
                        typed_stats: (333458, 48578, 12),
                        graph: (GraphCompletion::Incomplete, (193340, 5239, 5)),
                        roots: &[
                            checked("viaParam", (2093, 137, 3)),
                            checked("second", (2093, 137, 3)),
                            checked("bound", (2206, 193, 3)),
                            checked("grouped", (2093, 137, 3)),
                            checked("looped", (2153, 170, 5)),
                            checked("named", (2699, 312, 4)),
                            checked("late", (3240, 460, 4)),
                        ],
                    }
                }],
            );
        },
    );
}

const READ_SIBLINGS: &str = r####"
type Point = { x: i32; y: i32 };
function exclusive(view: &mut Point): i32 { return view.x; }
function sum(view: &Point): i32 { return view.x + view.y; }
function compared(view: &Point): bool { return view.x == 1i32; }
function copied(view: &Point): Point { return *view; }
"####;

const NAMED_READS: &str = r####"
type Named = { name: string; id: i32 };
function text(view: &Named): string { return view.name; }
function id(view: &Named): i32 { return view.id; }
"####;

/// Rule 2: the nearest unsupported reads keep the caps `bc9bfdd5` measured
/// (typed 328,492 / 48,011 / 12 and whole graph 188,245 / 4,241 / 5): an
/// exclusive view, arithmetic and comparison on the read, and a whole copy.
/// A `string` field stays R0501 at the caps it had (1,914 / 55 / 3; graph
/// 186,447 / 4,165 / 5 at the parent, 186,564 / 4,221 / 5 now, because `id`
/// in the same module is admitted).
#[test]
fn unsupported_record_reads_keep_their_parent_caps() {
    installed(
        "rfc_cases::record_contents::unsupported_record_reads_keep_their_parent_caps",
        |request| {
            check_cases(
                &request,
                &[
                    const {
                        Case {
                            text: READ_SIBLINGS,
                            typed_stats: (328492, 48011, 12),
                            graph: (GraphCompletion::Incomplete, (188245, 4241, 5)),
                            roots: &[
                                incomplete("exclusive", "view.x", (1906, 45, 3)),
                                incomplete("sum", "view.x + view.y", (1905, 45, 3)),
                                incomplete("compared", "view.x == 1i32", (1905, 45, 3)),
                                incomplete("copied", "*view", (2262, 104, 3)),
                            ],
                        }
                    },
                    const {
                        Case {
                            text: NAMED_READS,
                            typed_stats: (325120, 47705, 12),
                            graph: (GraphCompletion::SourceRejected, (186564, 4221, 5)),
                            roots: &[
                                Root {
                                    name: "text",
                                    want: Want::Rejected {
                                        code: "R0501",
                                        site: "view.name",
                                        related: "view: &Named",
                                    },
                                    graph: None,
                                    stats: (1914, 55, 3),
                                },
                                checked("id", (1983, 82, 3)),
                            ],
                        }
                    },
                ],
            );
        },
    );
}

pub(super) const fn checked(name: &'static str, stats: Triple) -> Root {
    Root {
        name,
        want: Want::Checked,
        graph: None,
        stats,
    }
}

pub(super) const fn incomplete(name: &'static str, blocker: &'static str, stats: Triple) -> Root {
    Root {
        name,
        want: Want::Incomplete { blocker },
        graph: None,
        stats,
    }
}

pub(super) const fn rejected(
    name: &'static str,
    code: &'static str,
    site: &'static str,
    related: &'static str,
    stats: Triple,
) -> Root {
    Root {
        name,
        want: Want::Rejected {
            code,
            site,
            related,
        },
        graph: None,
        stats,
    }
}

const FORMS: &str = r####"
type Point = { x: i32; y: i32 };
function remember(slot: &mut &Point, source: &Point): void {
  *slot = source;
}
function grouped(): i32 {
  let first = Point { x: 1, y: 2 };
  let second = Point { x: 9, y: 10 };
  let mut view = &first;
  remember((&mut view), &second);
  return view.x;
}
function looped(): i32 {
  let first = Point { x: 1, y: 2 };
  let second = Point { x: 9, y: 10 };
  let mut view = &first;
  while (true) {
    remember(&mut view, &second);
    return view.x;
  }
  return 0i32;
}
function other(): i32 {
  let first = Point { x: 1, y: 2 };
  let second = Point { x: 9, y: 10 };
  let mut view = &first;
  remember(&mut view, &second);
  return view.y;
}
function groupedRead(): i32 {
  let first = Point { x: 1, y: 2 };
  let second = Point { x: 9, y: 10 };
  let mut view = &first;
  remember(&mut view, &second);
  return (view).x;
}
function early(): i32 {
  let first = Point { x: 1, y: 2 };
  let mut view = &first;
  if (true) {
    let second = Point { x: 9, y: 10 };
    remember(&mut view, &second);
  }
  return view.x;
}
function dead(): i32 {
  let first = Point { x: 1, y: 2 };
  let mut view = &first;
  if (false) {
    let second = Point { x: 9, y: 10 };
    remember(&mut view, &second);
  }
  return view.x;
}
function unread(): void {
  let first = Point { x: 1, y: 2 };
  let mut view = &first;
  if (true) {
    let second = Point { x: 9, y: 10 };
    remember(&mut view, &second);
  }
}
function released(): i32 {
  let first = Point { x: 1, y: 2 };
  let second = Point { x: 9, y: 10 };
  let mut view = &first;
  remember(&mut view, &second);
  let n = view.x;
  return n;
}
"####;

/// The caller's view takes the stored source's loan: when the source's scope
/// ends before the view is read, the scope's end releases it while needed
/// (R0203 at the block, related the borrow). A grouped destination, a loop,
/// the other field, a grouped read, a never-read view and a read bound to a
/// name check; the same store in dead code grants nothing and rejects
/// nothing (rule 6). All measured on the candidate (nothing was `Checked`
/// at `bc9bfdd5`: typed was `Incomplete` at 345,383 / 51,204 / 12).
#[test]
fn stored_source_loans_reach_the_caller_view() {
    installed(
        "rfc_cases::record_contents::stored_source_loans_reach_the_caller_view",
        |request| {
            check_cases(
                &request,
                &[const {
                    Case {
                        text: FORMS,
                        typed_stats: (346876, 51370, 12),
                        graph: (GraphCompletion::SourceRejected, (215791, 9533, 8)),
                        roots: &[
                            checked("remember", (2810, 339, 3)),
                            checked("grouped", (7066, 1199, 7)),
                            checked("looped", (7201, 1277, 8)),
                            checked("other", (7061, 1199, 6)),
                            checked("groupedRead", (7061, 1199, 6)),
                            rejected(
                                "early",
                                "R0203",
                                r####"{\n    let second = Point { x: 9, y: 10 };\n    remember(&mut view, &second);\n  }"####,
                                "&second",
                                (4196, 862, 8),
                            ),
                            checked("dead", (3049, 491, 4)),
                            checked("unread", (7018, 1190, 8)),
                            checked("released", (7195, 1271, 6)),
                        ],
                    }
                }],
            );
        },
    );
}

const POISON: &str = r####"
type Point = { x: i32; y: i32 };
function remember(slot: &mut &Point, source: &Point): void {
  *slot = source;
}
function poison(slot: &mut &Point): void {
  let local = Point { x: 1, y: 2 };
  *slot = &local;
}
function forward(slot: &mut &Point, source: &Point): void {
  remember(slot, source);
}
function forwardLocal(slot: &mut &Point): void {
  let local = Point { x: 1, y: 2 };
  remember(slot, &local);
}
function keep(slot: &mut &Point): void {
  let local = Point { x: 1, y: 2 };
  let view = &local;
}
function twice(slot: &mut &Point, a: &Point, b: &Point): void {
  *slot = a;
  *slot = b;
}
"####;

/// The callee exit check: storing the address of a body-local owner into the
/// caller's slot escapes it (R0202), directly or through another call;
/// forwarding the caller's own source, keeping a local view to oneself and
/// storing two parameters check. Measured on the candidate.
#[test]
fn callee_stores_of_locals_escape_and_of_parameters_do_not() {
    installed(
        "rfc_cases::record_contents::callee_stores_of_locals_escape_and_of_parameters_do_not",
        |request| {
            check_cases(
                &request,
                &[const {
                    Case {
                        text: POISON,
                        typed_stats: (332719, 48732, 12),
                        graph: (GraphCompletion::SourceRejected, (196249, 5587, 6)),
                        roots: &[
                            checked("remember", (2552, 210, 3)),
                            rejected(
                                "poison",
                                "R0202",
                                "*slot = &local;",
                                "&local",
                                (2873, 331, 4),
                            ),
                            checked("forward", (5245, 460, 5)),
                            rejected(
                                "forwardLocal",
                                "R0202",
                                "remember(slot, &local)",
                                "&local",
                                (3136, 420, 6),
                            ),
                            checked("keep", (2878, 360, 4)),
                            checked("twice", (2814, 295, 3)),
                        ],
                    }
                }],
            );
        },
    );
}

const EMPTY_BODY: &str = r####"
type Point = { x: i32; y: i32 };
function remember(slot: &mut &Point, source: &Point): void { }
function selected(): i32 {
  let first = Point { x: 1, y: 2 };
  let second = Point { x: 9, y: 10 };
  let mut view = &first;
  remember(&mut view, &second);
  return view.x;
}
function early(): i32 {
  let first = Point { x: 1, y: 2 };
  let mut view = &first;
  if (true) {
    let second = Point { x: 9, y: 10 };
    remember(&mut view, &second);
  }
  return view.x;
}
"####;

const FAMILY: &str = r####"
type Point = { x: i32; y: i32 };
function remember<D, S>(slot: D, source: S): void;
implement remember(slot: &mut &Point, source: &Point): void { while (true) {} }
function selected(): i32 {
  let first = Point { x: 1, y: 2 };
  let second = Point { x: 9, y: 10 };
  let mut view = &first;
  remember(&mut view, &second);
  return view.x;
}
"####;

/// Typed admissions that come with the contents (ri-1up, reported to the
/// chief of staff): a call that passes `&mut view` to a `&mut &Point`
/// formal no longer stops typed at `&mut view`. At `bc9bfdd5` the empty body
/// was typed `Incomplete` at 328,191 / 48,344 / 12 and the family call at
/// 327,695 / 48,334 / 12; both old caps now give `ResourceLimit`. The
/// ownership pass checks the plain call; the family call reads the written
/// formals, which the unmetered screen decides for `&bool` and `&string` only,
/// so it stays `Incomplete` at its call, as before for these contents. The
/// empty body checks, and `early` shows that the caller's view still carries
/// the argument's loan: the call is checked against the declared signature,
/// never the body, as for `&bool` (chief of staff ruling of 2026-11-24 11:35
/// AEDT, condition 2).
#[test]
fn empty_body_and_family_calls_with_record_contents() {
    installed(
        "rfc_cases::record_contents::empty_body_and_family_calls_with_record_contents",
        |request| {
            check_cases(
                &request,
                &[
                    const {
                        Case {
                            text: EMPTY_BODY,
                            typed_stats: (329185, 48511, 12),
                            graph: (GraphCompletion::SourceRejected, (193577, 5558, 8)),
                            roots: &[
                                checked("remember", (2322, 138, 1)),
                                checked("selected", (6269, 845, 6)),
                                rejected(
                                    "early",
                                    "R0203",
                                    r####"{\n    let second = Point { x: 9, y: 10 };\n    remember(&mut view, &second);\n  }"####,
                                    "&second",
                                    (3888, 708, 8),
                                ),
                            ],
                        }
                    },
                    const {
                        Case {
                            text: FAMILY,
                            typed_stats: (328618, 48501, 12),
                            graph: (GraphCompletion::Incomplete, (189426, 4282, 5)),
                            roots: &[incomplete(
                                "selected",
                                "remember(&mut view, &second)",
                                (2663, 181, 3),
                            )],
                        }
                    },
                ],
            );
            old_caps(
                &request,
                &[
                    (EMPTY_BODY, (328191, 48344, 12)),
                    (FAMILY, (327695, 48334, 12)),
                ],
            );
        },
    );
}

/// Each `(source, typed cap)` is `Incomplete` at that cap's measure: with the
/// same source the cap now gives `ResourceLimit` (rule 11), in both modes.
fn old_caps(request: &InstalledRequest, olds: &[(&str, Triple)]) {
    for (text, old) in olds {
        fs::write(&request.entry, format!("{IMPORTS}\n{text}")).unwrap();
        for test_mode in [false, true] {
            let request = InstalledRequest {
                test_mode,
                ..request.clone()
            };
            let graph = loader::load_installed(request).unwrap();
            let (old, _) = limits(statistics(*old));
            let typed = analyze_typed(&graph, old);
            crate::limit::assert_typed_limit(&typed);
        }
    }
}

const UNSUPPORTED: &[Source] = &[
    Source {
        text: r####"
    type Named = { name: string; id: i32 };
    function remember(slot: &mut &Named, source: &Named): void {
      *slot = source;
    }
    function selected(): i32 {
      let first = Named { name: "a", id: 1 };
      let second = Named { name: "b", id: 2 };
      let mut view = &first;
      remember(&mut view, &second);
      return view.id;
    }
    "####,
        outcome: Outcome::Typed {
            completion: TypedCompletion::Incomplete,
            signals: &[
                r####"pending Storage @ *slot = source;"####,
                r####"pending Storage @ &mut view"####,
            ],
            stats: (325631, 47914, 12),
        },
    },
    Source {
        text: r####"
    type Point = { x: i32; y: i32 };
    type Line = { start: Point; id: i32 };
    function remember(slot: &mut &Line, source: &Line): void {
      *slot = source;
    }
    function selected(): i32 {
      let first = Line { start: Point { x: 1, y: 2 }, id: 1 };
      let second = Line { start: Point { x: 3, y: 4 }, id: 2 };
      let mut view = &first;
      remember(&mut view, &second);
      return view.id;
    }
    "####,
        outcome: Outcome::Typed {
            completion: TypedCompletion::Incomplete,
            signals: &[
                r####"pending Storage @ *slot = source;"####,
                r####"pending Storage @ &mut view"####,
            ],
            stats: (327296, 48135, 12),
        },
    },
    Source {
        text: r####"
    enum Shape { Circle, Square }
    function remember(slot: &mut &Shape, source: &Shape): void {
      *slot = source;
    }
    function selected(a: &Shape, b: &Shape): i32 {
      let mut view = a;
      remember(&mut view, b);
      return 1i32;
    }
    "####,
        outcome: Outcome::Typed {
            completion: TypedCompletion::Incomplete,
            signals: &[r####"pending Storage @ *slot = source;"####],
            stats: (324334, 47682, 12),
        },
    },
    Source {
        text: r####"
    type Point = { x: i32; y: i32 };
    function remember(slot: &mut &mut Point, source: &mut Point): void {
      *slot = source;
    }
    function selected(): i32 {
      let mut first = Point { x: 1, y: 2 };
      let mut second = Point { x: 9, y: 10 };
      let mut view = &mut first;
      remember(&mut view, &mut second);
      return view.x;
    }
    "####,
        outcome: Outcome::Typed {
            completion: TypedCompletion::Incomplete,
            signals: &[
                r####"pending Storage @ *slot = source;"####,
                r####"pending Storage @ &mut view"####,
            ],
            stats: (325413, 47912, 12),
        },
    },
    Source {
        text: r####"
    type Box<T> = { value: T };
    function remember(slot: &mut &Box<i32>, source: &Box<i32>): void {
      *slot = source;
    }
    function selected(): i32 {
      let first = Box { value: 1i32 };
      let second = Box { value: 9i32 };
      let mut view = &first;
      remember(&mut view, &second);
      return view.value;
    }
    "####,
        outcome: Outcome::Typed {
            completion: TypedCompletion::Incomplete,
            signals: &[
                r####"pending Storage @ *slot = source;"####,
                r####"pending Storage @ &mut view"####,
            ],
            stats: (325398, 47894, 12),
        },
    },
];

/// Rule 1 and rule 2 (ri-1up; rule-1 exception, chief of staff, 2026-11-24
/// 11:35 AEDT): a store into `&mut &T` for a `T` that is not a flat
/// non-generic record of `i32` fields stays typed `Incomplete` at the same
/// blockers. The record screen reads the type alone first, so a generic
/// record and an exclusive `&mut Point` pay nothing (caps unchanged, measured
/// at `bc9bfdd5`). A string field, a nested record and an enum need the
/// declaration's fields, which no paid fact carries: each pays two lookups
/// (the callee's store, the caller's `&mut view`), pinned here at the new
/// caps. Their parent caps (325,487 / 47,914 / 12; 327,152 / 48,135 / 12;
/// 324,194 / 47,682 / 12) give `ResourceLimit`, with no receipt.
#[test]
fn unsupported_record_contents_stay_incomplete() {
    run(
        "rfc_cases::record_contents::unsupported_record_contents_stay_incomplete",
        UNSUPPORTED,
    );
}

#[test]
fn unsupported_record_contents_parent_caps_now_give_resource_limit() {
    installed(
        "rfc_cases::record_contents::unsupported_record_contents_parent_caps_now_give_resource_limit",
        |request| {
            let olds = [
                (0, (325487, 47914, 12)),
                (1, (327152, 48135, 12)),
                (2, (324194, 47682, 12)),
            ];
            let olds: Vec<_> = olds
                .iter()
                .map(|&(i, old)| (UNSUPPORTED[i].text, old))
                .collect();
            old_caps(&request, &olds);
        },
    );
}

/// O2's `selected` and an empty-body function with the same signature, whole
/// operation sequences (rule 9). The declared contract is the same for both:
/// the call may store `&second` into `view`, so the caller's view carries
/// its loan whatever the callee's body does (`early` in
/// `stored_source_loans_reach_the_caller_view` rejects on it).
#[test]
fn caller_sequence_of_a_record_retaining_call() {
    installed(
        "rfc_cases::record_contents::caller_sequence_of_a_record_retaining_call",
        |request| {
            for text in [REMEMBER_SELECTED, EMPTY_BODY] {
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
                    let declaration = module
                        .declarations
                        .iter()
                        .find(|d| {
                            d.id.kind == DeclarationKind::Function
                                && d.name.as_deref() == Some("selected")
                        })
                        .unwrap();
                    let report =
                        lower_body(&receipt, module, &declaration.id, AnalysisLimits::default())
                            .unwrap();
                    let mir = report.mir().expect("selected checks");
                    let ops: Vec<String> = straight_blocks(mir, mir.entry)
                        .iter()
                        .filter_map(|block| describe(&block.operation))
                        .collect();
                    assert_eq!(ops, EXPECTED_SEQUENCE, "test_mode {test_mode}");
                }
            }
        },
    );
}

const REMEMBER_SELECTED: &str = r####"
type Point = { x: i32; y: i32 };
function remember(slot: &mut &Point, source: &Point): void {
  *slot = source;
}
function selected(): i32 {
  let first = Point { x: 1, y: 2 };
  let second = Point { x: 9, y: 10 };
  let mut view = &first;
  remember(&mut view, &second);
  return view.x;
}
"####;

fn place(place: &compiler::analysis::ownership::Place<'_>) -> String {
    String::from_utf8_lossy(place.root.span.bytes()).into_owned()
}

/// One entry per operation, `None` omitted; places by their source text.
fn describe(operation: &Operation<'_>) -> Option<String> {
    Some(match operation {
        Operation::None => return None,
        Operation::Initialize(value) => format!("init {}", place(value)),
        Operation::Borrow {
            source,
            holder,
            exclusive,
        } => format!(
            "borrow{} {} -> {}",
            if *exclusive { " exclusive" } else { "" },
            place(source),
            place(holder)
        ),
        Operation::CallRetaining {
            destination: RetainedPlace::Local(destination),
            input,
            ..
        } => format!("retain {} <- {}", place(destination), place(input)),
        Operation::Dereference(value) => format!("read {}", place(value)),
        Operation::Export(value) => format!("export {}", place(value)),
        Operation::Cleanup(actions) => actions
            .iter()
            .map(|action| match action {
                Obligation::EndStorage(value) => format!("end {}", place(value)),
                other => panic!("unexpected cleanup {other:?}"),
            })
            .collect::<Vec<_>>()
            .join("; "),
        other => panic!("unexpected operation {other:?}"),
    })
}
const EXPECTED_SEQUENCE: [&str; 13] = [
    "init first",
    "init first",
    "init first",
    "init second",
    "init second",
    "init second",
    "borrow first -> view",
    "borrow exclusive view -> &mut view",
    "borrow second -> &second",
    "retain view <- &second",
    "read view",
    "export view.x",
    "end view; end second; end second; end second; end first; end first; end first",
];
