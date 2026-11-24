//! RFC 0007 O7 (ri-pfxi): two reference fields of one record may point to the
//! same owner, so loans staged into one construction conflict (R0201) unless
//! the owners are distinct. The O7 row above pins the RFC's module; these
//! tests pin the construction conflict, the loans that stay disjoint and the
//! reference members the proof still refuses at their parent caps
//! (metering rule 2).
use super::*;

struct Case {
    text: &'static str,
    typed_stats: Triple,
    graph: (GraphCompletion, Triple),
    roots: &'static [Root],
}

fn check(test: &str, case: &Case) {
    installed(test, |request| {
        fs::write(&request.entry, format!("{IMPORTS}\n{}", case.text)).unwrap();
        for test_mode in [false, true] {
            let request = InstalledRequest {
                test_mode,
                ..request.clone()
            };
            let graph = loader::load_installed(request).unwrap();
            let typed = analyze_typed(&graph, AnalysisLimits::default());
            let context = format!("test_mode {test_mode}");
            assert_eq!(typed.completion(), TypedCompletion::Checked, "{context}");
            assert_eq!(
                typed.statistics(),
                statistics(case.typed_stats),
                "{context}"
            );
            let module = graph
                .modules()
                .iter()
                .find(|m| m.key.origin == Origin::Application && m.key.path == "main.rill")
                .unwrap();
            check_receipt(&context, &typed, module, case.graph, case.roots);
        }
    });
}

/// Done when 1: the record under construction holds both loans, so the
/// second borrow conflicts with the first whichever is exclusive. The site is
/// the second borrow and the related site the first. No later use is needed.
const CONSTRUCTION: Case = Case {
    text: r#"
type Holder = { first: &mut i32; second: &i32 };
type Reversed = { first: &i32; second: &mut i32 };
function overlapConstruct(): i32 {
  let mut value = 1i32;
  let holder = Holder { first: &mut value, second: &value };
  return 0i32;
}
function overlapReversed(): i32 {
  let mut value = 1i32;
  let holder = Reversed { first: &value, second: &mut value };
  return 0i32;
}
"#,
    typed_stats: (327083, 48063, 12),
    graph: (GraphCompletion::SourceRejected, (188526, 4475, 5)),
    roots: &[
        Root {
            name: "overlapConstruct",
            want: Want::Rejected {
                code: "R0201",
                site: r####"&value"####,
                related: r####"&mut value"####,
            },
            graph: None,
            stats: (2462, 203, 5),
        },
        Root {
            name: "overlapReversed",
            want: Want::Rejected {
                code: "R0201",
                site: r####"&mut value"####,
                related: r####"&value"####,
            },
            graph: None,
            stats: (2462, 203, 5),
        },
    ],
};

#[test]
fn overlapping_construction_rejects_r0201() {
    check(
        "rfc_cases::o07::overlapping_construction_rejects_r0201",
        &CONSTRUCTION,
    );
}

/// Done when 3 (review ci-0mdr Q3, SP-4; chief ruling 11): a member
/// replacement that creates an overlap rejects R0201 at the replacement with
/// no later use, as construction does. The store uses the record's other
/// members, so their loans are held while the new loan is staged; the replaced
/// member's own old loan is released by the store (`sameOwnerAgain` in
/// `o07_forms`). The site is the new borrow, the related site the loan it
/// meets, the construction's `&mut a` or `&mut b` (the two have the same text
/// in `overlapNoUse`, `replacesFirst` and `looped`; `sharedMember` and
/// `sharedFirst` tell the sites apart). `looped` repeats the replacement in a
/// loop.
const REPLACEMENT: Case = Case {
    text: r#"
type Pair = { x: &mut i32; y: &mut i32 };
type Holder = { first: &mut i32; second: &i32 };
type Reversed = { first: &i32; second: &mut i32 };
function overlapNoUse(): i32 {
  let mut a = 1i32;
  let mut b = 2i32;
  let mut holder = Pair { x: &mut a, y: &mut b };
  holder.y = &mut a;
  return 0i32;
}
function replacesFirst(): i32 {
  let mut a = 1i32;
  let mut b = 2i32;
  let mut holder = Pair { x: &mut a, y: &mut b };
  holder.x = &mut b;
  return 0i32;
}
function sharedMember(): i32 {
  let mut a = 1i32;
  let b = 2i32;
  let mut holder = Holder { first: &mut a, second: &b };
  holder.second = &a;
  return 0i32;
}
function sharedFirst(): i32 {
  let b = 2i32;
  let mut a = 1i32;
  let mut holder = Reversed { first: &b, second: &mut a };
  holder.first = &a;
  return 0i32;
}
function looped(flag: bool): i32 {
  let mut a = 1i32;
  let mut b = 2i32;
  let mut holder = Pair { x: &mut a, y: &mut b };
  while (flag) { holder.y = &mut a; }
  return 0i32;
}
"#,
    typed_stats: (336765, 49495, 12),
    graph: (GraphCompletion::SourceRejected, (196876, 5724, 6)),
    roots: &[
        Root {
            name: "overlapNoUse",
            want: Want::Rejected {
                code: "R0201",
                site: r####"&mut a"####,
                related: r####"&mut a"####,
            },
            graph: None,
            stats: (2828, 385, 5),
        },
        Root {
            name: "replacesFirst",
            want: Want::Rejected {
                code: "R0201",
                site: r####"&mut b"####,
                related: r####"&mut b"####,
            },
            graph: None,
            stats: (2823, 384, 5),
        },
        Root {
            name: "sharedMember",
            want: Want::Rejected {
                code: "R0201",
                site: r####"&a"####,
                related: r####"&mut a"####,
            },
            graph: None,
            stats: (2828, 385, 5),
        },
        Root {
            name: "sharedFirst",
            want: Want::Rejected {
                code: "R0201",
                site: r####"&a"####,
                related: r####"&mut a"####,
            },
            graph: None,
            stats: (2823, 384, 5),
        },
        Root {
            name: "looped",
            want: Want::Rejected {
                code: "R0201",
                site: r####"&mut a"####,
                related: r####"&mut a"####,
            },
            graph: None,
            stats: (3073, 514, 6),
        },
    ],
};

#[test]
fn member_replacement_overlap_without_later_use() {
    check(
        "rfc_cases::o07::member_replacement_overlap_without_later_use",
        &REPLACEMENT,
    );
}

/// Metering rule 11, the mandated flip of Q3: the parent `da5bec50` checked
/// every `REPLACEMENT` root at the caps below (the graph was `Incomplete` at
/// its first loop), and the new check costs less, because the replacement
/// rejects before the rest of the body is lowered. The old caps therefore still
/// hold and give the new outcome, R0201, not `ResourceLimit`.
#[test]
fn member_replacement_overlap_gives_r0201_at_its_parent_caps() {
    at_parent_caps(
        "rfc_cases::o07::member_replacement_overlap_gives_r0201_at_its_parent_caps",
        REPLACEMENT.text,
        (198404, 6418, 6),
        &[
            ("overlapNoUse", (3061, 491, 5)),
            ("replacesFirst", (3056, 490, 5)),
            ("sharedMember", (3061, 491, 5)),
            ("sharedFirst", (3056, 490, 5)),
            ("looped", (3679, 789, 6)),
        ],
        Parent::Rejects("R0201"),
    );
}

/// Review ci-b4b2 D-F1, D-F2 (chief rulings 11 Q3 and 18): a replacement that
/// stores a reborrow of a sibling member into another member rejects R0201 at
/// the replacement, as construction rejects `Pair { x: &mut *r, y: &mut *r }`.
/// The reborrow is a child of the sibling's loan, so the borrow check at its
/// creation sees no overlap; the store does. The site is the replacement and
/// the related site the reborrow. It holds directly (`reborrowDirect`, and
/// `reborrowThenWrite` with a later write), through a local (`reborrowLocal`),
/// through a call's result (`reborrowCall`), in a record parameter
/// (`reborrowParam`) and for a shared member (`reborrowShared`).
/// `siblingReplacedAfter` is D-F2: the later replacement of `holder.x` does not
/// make the overlap go away, so it rejects at the first replacement (it was
/// R0203 at `holder.x = &mut c;`, conservatively, before). The twins reborrow
/// a sibling without storing it into a member (held, read, passed to a call
/// that does not return it, and the shared reborrow) and check; a write
/// through the reborrow, `*r += 1i32`, is `Incomplete` and not pinned.
const REBORROW: Case = Case {
    text: r#"
type Pair = { x: &mut i32; y: &mut i32 };
type Holder = { first: &mut i32; second: &i32 };
function same(r: &mut i32): &mut i32 { return r; }
function peek(r: &mut i32): i32 { return *r; }
function reborrowDirect(): i32 {
  let mut a = 1i32;
  let mut b = 2i32;
  let mut holder = Pair { x: &mut a, y: &mut b };
  holder.y = &mut *holder.x;
  return 0i32;
}
function reborrowThenWrite(): i32 {
  let mut a = 1i32;
  let mut b = 2i32;
  let mut holder = Pair { x: &mut a, y: &mut b };
  holder.y = &mut *holder.x;
  *holder.y += 1i32;
  return 0i32;
}
function reborrowLocal(): i32 {
  let mut a = 1i32;
  let mut b = 2i32;
  let mut holder = Pair { x: &mut a, y: &mut b };
  let r = &mut *holder.x;
  holder.y = r;
  return 0i32;
}
function reborrowCall(): i32 {
  let mut a = 1i32;
  let mut b = 2i32;
  let mut holder = Pair { x: &mut a, y: &mut b };
  holder.y = same(&mut *holder.x);
  return 0i32;
}
function reborrowParam(h: Pair): i32 {
  let mut k = h;
  k.y = &mut *k.x;
  return 0i32;
}
function reborrowShared(): i32 {
  let mut a = 1i32;
  let b = 2i32;
  let mut holder = Holder { first: &mut a, second: &b };
  holder.second = &*holder.first;
  return 0i32;
}
function siblingReplacedAfter(): i32 {
  let mut a = 1i32;
  let mut b = 2i32;
  let mut holder = Pair { x: &mut a, y: &mut b };
  let mut c = 3i32;
  holder.y = &mut *holder.x;
  holder.x = &mut c;
  *holder.x += 1i32;
  *holder.y += 1i32;
  return 0i32;
}
function reborrowKept(): i32 {
  let mut a = 1i32;
  let mut b = 2i32;
  let mut holder = Pair { x: &mut a, y: &mut b };
  let r = &mut *holder.x;
  return 0i32;
}
function reborrowRead(): i32 {
  let mut a = 1i32;
  let mut b = 2i32;
  let mut holder = Pair { x: &mut a, y: &mut b };
  let r = &mut *holder.x;
  return *r;
}
function reborrowPassed(): i32 {
  let mut a = 1i32;
  let mut b = 2i32;
  let mut holder = Pair { x: &mut a, y: &mut b };
  return peek(&mut *holder.x);
}
function reborrowSharedKept(): i32 {
  let mut a = 1i32;
  let b = 2i32;
  let mut holder = Holder { first: &mut a, second: &b };
  let r = &*holder.first;
  return *r;
}
"#,
    typed_stats: (355194, 52171, 12),
    graph: (GraphCompletion::SourceRejected, (215061, 8614, 6)),
    roots: &[
        Root {
            name: "reborrowDirect",
            want: Want::Rejected {
                code: "R0201",
                site: r####"holder.y = &mut *holder.x;"####,
                related: r####"&mut *holder.x"####,
            },
            graph: None,
            stats: (3230, 577, 5),
        },
        Root {
            name: "reborrowThenWrite",
            want: Want::Rejected {
                code: "R0201",
                site: r####"holder.y = &mut *holder.x;"####,
                related: r####"&mut *holder.x"####,
            },
            graph: None,
            stats: (3446, 684, 5),
        },
        Root {
            name: "reborrowLocal",
            want: Want::Rejected {
                code: "R0201",
                site: r####"holder.y = r;"####,
                related: r####"&mut *holder.x"####,
            },
            graph: None,
            stats: (3342, 625, 5),
        },
        Root {
            name: "reborrowCall",
            want: Want::Rejected {
                code: "R0201",
                site: r####"holder.y = same(&mut *holder.x);"####,
                related: r####"&mut *holder.x"####,
            },
            graph: None,
            stats: (3325, 629, 6),
        },
        Root {
            name: "reborrowParam",
            want: Want::Rejected {
                code: "R0201",
                site: r####"k.y = &mut *k.x;"####,
                related: r####"&mut *k.x"####,
            },
            graph: None,
            stats: (3392, 545, 4),
        },
        Root {
            name: "reborrowShared",
            want: Want::Rejected {
                code: "R0201",
                site: r####"holder.second = &*holder.first;"####,
                related: r####"&*holder.first"####,
            },
            graph: None,
            stats: (3230, 577, 5),
        },
        Root {
            name: "siblingReplacedAfter",
            want: Want::Rejected {
                code: "R0201",
                site: r####"holder.y = &mut *holder.x;"####,
                related: r####"&mut *holder.x"####,
            },
            graph: None,
            stats: (3898, 908, 5),
        },
        Root {
            name: "reborrowKept",
            want: Want::Checked,
            graph: None,
            stats: (3316, 611, 5),
        },
        Root {
            name: "reborrowRead",
            want: Want::Checked,
            graph: None,
            stats: (3331, 613, 5),
        },
        Root {
            name: "reborrowPassed",
            want: Want::Checked,
            graph: None,
            stats: (5762, 933, 6),
        },
        Root {
            name: "reborrowSharedKept",
            want: Want::Checked,
            graph: None,
            stats: (3332, 613, 5),
        },
    ],
};

#[test]
fn member_replacement_through_a_sibling_reborrow_rejects_r0201() {
    check(
        "rfc_cases::o07::member_replacement_through_a_sibling_reborrow_rejects_r0201",
        &REBORROW,
    );
}

/// Chief ruling 20 (re-review ci-b4b2, C13's stricter neighbour): a record
/// holding a reborrow of its own sibling member, passed whole to a call. It was
/// R0203 at `holder` (the move of a record whose member a loan still covers);
/// the replacement is now the first rejection, R0201 at the replacement,
/// related the reborrow, as for every other neighbour in `REBORROW`.
const REBORROW_TAKE: Case = Case {
    text: r#"
type Pair = { x: &mut i32; y: &mut i32 };
function take(h: Pair): i32 { return 0i32; }
function takeAfterReborrow(): i32 {
  let mut a = 1i32;
  let mut b = 2i32;
  let mut holder = Pair { x: &mut a, y: &mut b };
  holder.y = &mut *holder.x;
  return take(holder);
}
"#,
    typed_stats: (326089, 48005, 12),
    graph: (GraphCompletion::SourceRejected, (189238, 4708, 5)),
    roots: &[
        Root {
            name: "take",
            want: Want::Checked,
            graph: None,
            stats: (2388, 168, 3),
        },
        Root {
            name: "takeAfterReborrow",
            want: Want::Rejected {
                code: "R0201",
                site: r####"holder.y = &mut *holder.x;"####,
                related: r####"&mut *holder.x"####,
            },
            graph: None,
            stats: (3255, 472, 5),
        },
    ],
};

#[test]
fn a_record_holding_a_sibling_reborrow_rejects_at_the_replacement_before_it_is_taken() {
    check(
        "rfc_cases::o07::a_record_holding_a_sibling_reborrow_rejects_at_the_replacement_before_it_is_taken",
        &REBORROW_TAKE,
    );
}

/// What a root or graph does at the caps the parent revision measured.
pub(super) enum Parent {
    /// The check now costs more: the old caps give `ResourceLimit`.
    Exceeds,
    /// The check now costs less: the old caps still hold and reject with this code.
    Rejects(&'static str),
}

/// Review ci-0mdr Q3 (metering rule 11): the targeted roots and the whole
/// graph of `text` at the caps `da5bec50` measured.
pub(super) fn at_parent_caps(
    test: &str,
    text: &str,
    graph_caps: Triple,
    roots: &[(&str, Triple)],
    parent: Parent,
) {
    installed(test, |request| {
        fs::write(&request.entry, format!("{IMPORTS}\n{text}")).unwrap();
        for test_mode in [false, true] {
            let request = InstalledRequest {
                test_mode,
                ..request.clone()
            };
            let graph = loader::load_installed(request).unwrap();
            let typed = analyze_typed(&graph, AnalysisLimits::default());
            assert_eq!(typed.completion(), TypedCompletion::Checked);
            let module = graph
                .modules()
                .iter()
                .find(|m| m.key.origin == Origin::Application && m.key.path == "main.rill")
                .unwrap();
            let receipt = typed.receipt().unwrap();
            let (old, _) = limits(statistics(graph_caps));
            let whole = analyze_ownership(&receipt, old);
            match parent {
                Parent::Exceeds => crate::limit::assert_graph_limit(&whole),
                Parent::Rejects(_) => {
                    assert_eq!(whole.completion(), GraphCompletion::SourceRejected)
                }
            }
            for &(name, caps) in roots {
                let declaration = module
                    .declarations
                    .iter()
                    .find(|d| {
                        d.id.kind == DeclarationKind::Function && d.name.as_deref() == Some(name)
                    })
                    .unwrap();
                let (old, _) = limits(statistics(caps));
                let report = lower_body(&receipt, module, &declaration.id, old).unwrap();
                match parent {
                    Parent::Exceeds => crate::limit::assert_body_limit(&report),
                    Parent::Rejects(code) => {
                        assert_eq!(report.completion(), Completion::SourceRejected, "{name}");
                        assert!(report.mir().is_none() && report.blocked_at().is_none());
                        let codes: Vec<_> = report.findings().iter().map(|f| f.code()).collect();
                        assert_eq!(codes, [code], "{name}");
                    }
                }
            }
        }
    });
}

/// Review ci-0mdr SP-3, the rejecting twins of construction that the second
/// review found correct but unpinned (probe 2 of ci-0mdr): a record moved
/// twice is R0200 at the second move (related the first); a member and its
/// referent in one call, `refFirst(&a, One { first: &mut a })`, is R0201 at the
/// exclusive borrow (related the shared one); a `&mut` parameter stored in two
/// members is R0200 at the second use (related the first). `refFirst` is the
/// call's callee and checks.
const TWINS: Case = Case {
    text: r#"
type Pair = { x: &mut i32; y: &mut i32 };
type One = { first: &mut i32 };
function refFirst(r: &i32, o: One): i32 { return 0i32; }
function movedTwice(): i32 {
  let mut a = 1i32;
  let h = One { first: &mut a };
  let h2 = h;
  let h3 = h;
  return 0i32;
}
function memberAndReferent(): i32 {
  let mut a = 1i32;
  return refFirst(&a, One { first: &mut a });
}
function dupParam(p: &mut i32): i32 {
  let h = Pair { x: p, y: p };
  return 0i32;
}
"#,
    typed_stats: (330147, 48293, 12),
    graph: (GraphCompletion::SourceRejected, (191771, 4990, 7)),
    roots: &[
        Root {
            name: "refFirst",
            want: Want::Checked,
            graph: None,
            stats: (2430, 190, 3),
        },
        Root {
            name: "movedTwice",
            want: Want::Rejected {
                code: "R0200",
                site: r####"h"####,
                related: r####"h"####,
            },
            graph: None,
            stats: (3344, 409, 5),
        },
        Root {
            name: "memberAndReferent",
            want: Want::Rejected {
                code: "R0201",
                site: r####"&mut a"####,
                related: r####"&a"####,
            },
            graph: None,
            stats: (2498, 223, 7),
        },
        Root {
            name: "dupParam",
            want: Want::Rejected {
                code: "R0200",
                site: r####"p"####,
                related: r####"p"####,
            },
            graph: None,
            stats: (2461, 205, 4),
        },
    ],
};

#[test]
fn construction_twins_the_review_found_unpinned() {
    check(
        "rfc_cases::o07::construction_twins_the_review_found_unpinned",
        &TWINS,
    );
}

/// Done when 2: loans staged into one record conflict unless the owners are
/// distinct. `disjoint` is the RFC's disjoint form (`&mut a`, `&b`), written
/// through and read back. Rejected sites are the later borrow or access, the
/// related site the loan it conflicts with. The `Incomplete` roots are writes
/// and reads through the member that the proof does not admit; they stop at
/// the use and pin the caps measured with this change.
const LOANS: Case = Case {
    text: r#"
type Holder = { first: &mut i32; second: &i32 };
type Pair = { x: &mut i32; y: &mut i32 };
type Named = { first: &mut i32; name: string };
function disjoint(): i32 {
  let mut a = 1i32;
  let b = 2i32;
  let mut holder = Holder { first: &mut a, second: &b };
  *holder.first += 1i32;
  return *holder.second;
}
function twice(): i32 {
  let mut a = 1i32;
  let b = 2i32;
  let mut holder = Holder { first: &mut a, second: &b };
  *holder.first += 1i32;
  *holder.first += 2i32;
  return *holder.second;
}
function endsBeforeSource(): i32 {
  let mut a = 1i32;
  let b = 2i32;
  let mut holder = Holder { first: &mut a, second: &b };
  *holder.first += 1i32;
  return a;
}
function twoOwners(): i32 {
  let mut a = 1i32;
  let mut b = 1i32;
  let mut holder = Pair { x: &mut a, y: &mut b };
  *holder.x += 1i32;
  *holder.y += 1i32;
  return a + b;
}
function groupedBorrows(): i32 {
  let mut a = 1i32;
  let b = 2i32;
  let mut holder = Holder { first: (&mut a), second: (&b) };
  *holder.first += 1i32;
  return 0i32;
}
function looped(flag: bool): i32 {
  let mut a = 1i32;
  let b = 2i32;
  let mut holder = Holder { first: &mut a, second: &b };
  while (flag) { *holder.first += 1i32; }
  return 0i32;
}
function replaced(): i32 {
  let mut a = 1i32;
  let mut b = 1i32;
  let mut holder = Holder { first: &mut a, second: &b };
  holder = Holder { first: &mut b, second: &a };
  *holder.first += 1i32;
  return 0i32;
}
function withText(): i32 {
  let mut a = 1i32;
  let mut holder = Named { first: &mut a, name: "x" };
  *holder.first += 1i32;
  return 0i32;
}
function sameOwnerTwice(): i32 {
  let mut a = 1i32;
  let mut holder = Pair { x: &mut a, y: &mut a };
  *holder.x += 1i32;
  return 0i32;
}
function readOwnerWhileHeld(): i32 {
  let mut a = 1i32;
  let b = 2i32;
  let mut holder = Holder { first: &mut a, second: &b };
  let seen = a;
  *holder.first += 1i32;
  return seen;
}
function secondHeldOverFirst(): i32 {
  let mut a = 1i32;
  let mut b = 2i32;
  let mut holder = Holder { first: &mut a, second: &b };
  let mut other = Holder { first: &mut b, second: &a };
  *holder.first += 1i32;
  return 0i32;
}
function scopedOverlap(): i32 {
  let mut a = 1i32;
  if (true) {
    let mut holder = Holder { first: &mut a, second: &a };
    *holder.first += 1i32;
  }
  return 0i32;
}
function assignThrough(): i32 {
  let mut a = 1i32;
  let b = 2i32;
  let mut holder = Holder { first: &mut a, second: &b };
  *holder.first = 5i32;
  return 0i32;
}
function groupedTarget(): i32 {
  let mut a = 1i32;
  let b = 2i32;
  let mut holder = Holder { first: &mut a, second: &b };
  *(holder.first) += 1i32;
  return 0i32;
}
function movedOut(): i32 {
  let mut a = 1i32;
  let b = 2i32;
  let mut holder = Holder { first: &mut a, second: &b };
  let moved = holder.first;
  *moved += 1i32;
  return 0i32;
}
function reborrowed(): i32 {
  let mut a = 1i32;
  let b = 2i32;
  let mut holder = Holder { first: &mut a, second: &b };
  let view = &mut *holder.first;
  *view += 1i32;
  return 0i32;
}
"#,
    typed_stats: (364913, 54043, 12),
    graph: (GraphCompletion::SourceRejected, (228054, 11538, 7)),
    roots: &[
        Root {
            name: "disjoint",
            want: Want::Checked,
            graph: None,
            stats: (3886, 886, 5),
        },
        Root {
            name: "twice",
            want: Want::Checked,
            graph: None,
            stats: (4510, 1198, 5),
        },
        Root {
            name: "endsBeforeSource",
            want: Want::Checked,
            graph: None,
            stats: (3858, 882, 5),
        },
        Root {
            name: "twoOwners",
            want: Want::Checked,
            graph: None,
            stats: (4611, 1214, 5),
        },
        Root {
            name: "groupedBorrows",
            want: Want::Checked,
            graph: None,
            stats: (3820, 872, 6),
        },
        Root {
            name: "looped",
            want: Want::Checked,
            graph: None,
            stats: (4250, 1074, 5),
        },
        Root {
            name: "replaced",
            want: Want::Checked,
            graph: None,
            stats: (4731, 1209, 5),
        },
        Root {
            name: "withText",
            want: Want::Checked,
            graph: None,
            stats: (3665, 804, 5),
        },
        Root {
            name: "sameOwnerTwice",
            want: Want::Rejected {
                code: "R0201",
                site: r####"&mut a"####,
                related: r####"&mut a"####,
            },
            graph: None,
            stats: (3265, 600, 5),
        },
        Root {
            name: "readOwnerWhileHeld",
            want: Want::Rejected {
                code: "R0201",
                site: r####"a"####,
                related: r####"&mut a"####,
            },
            graph: None,
            stats: (3504, 720, 5),
        },
        Root {
            name: "secondHeldOverFirst",
            want: Want::Rejected {
                code: "R0201",
                site: r####"&mut b"####,
                related: r####"&b"####,
            },
            graph: None,
            stats: (3949, 821, 5),
        },
        Root {
            name: "scopedOverlap",
            want: Want::Rejected {
                code: "R0201",
                site: r####"&a"####,
                related: r####"&mut a"####,
            },
            graph: None,
            stats: (3364, 662, 7),
        },
        Root {
            name: "assignThrough",
            want: Want::Incomplete {
                blocker: r####"*holder.first"####,
            },
            graph: None,
            stats: (2926, 438, 3),
        },
        Root {
            name: "groupedTarget",
            want: Want::Incomplete {
                blocker: r####"*(holder.first) += 1i32;"####,
            },
            graph: None,
            stats: (2924, 438, 3),
        },
        Root {
            name: "movedOut",
            want: Want::Incomplete {
                blocker: r####"*moved += 1i32;"####,
            },
            graph: None,
            stats: (2930, 443, 3),
        },
        Root {
            name: "reborrowed",
            want: Want::Incomplete {
                blocker: r####"*view += 1i32;"####,
            },
            graph: None,
            stats: (2930, 443, 3),
        },
    ],
};

#[test]
fn staged_loans_conflict_unless_the_owners_are_distinct() {
    check(
        "rfc_cases::o07::staged_loans_conflict_unless_the_owners_are_distinct",
        &LOANS,
    );
}

/// Rules 1 and 2: the records the admission still refuses keep the caps
/// measured at the parent `b6c93f30` (in a `git worktree add` with its own
/// target directory), so the admission spends nothing before it rejects. Each
/// stays `Incomplete` at the construction: an `&mut bool` or `&mut i64`
/// member, an `&mut string` member, and an `&mut i32` member of a record with
/// a custom disposer, which could read the member when it runs.
const SIBLINGS: Case = Case {
    text: r#"
import std::core::dispose as release;
type Bools = { flag: &mut bool; n: i32 };
type Texts = { view: &mut string };
type Wide = { wide: &mut i64 };
type Guarded = { view: &mut i32 };
implement release(value: Guarded): void {}
function boolMember(): i32 {
  let mut f = true;
  let h = Bools { flag: &mut f, n: 1 };
  return 0i32;
}
function textMember(): i32 {
  let mut s = "a";
  let h = Texts { view: &mut s };
  return 0i32;
}
function wideMember(): i32 {
  let mut w = 1i64;
  let h = Wide { wide: &mut w };
  return 0i32;
}
function guarded(): i32 {
  let mut a = 1i32;
  let h = Guarded { view: &mut a };
  return 0i32;
}
"#,
    typed_stats: (334264, 48531, 12),
    graph: (GraphCompletion::Incomplete, (190674, 4432, 5)),
    roots: &[
        Root {
            name: "boolMember",
            want: Want::Incomplete {
                blocker: r####"h"####,
            },
            graph: None,
            stats: (2303, 120, 1),
        },
        Root {
            name: "textMember",
            want: Want::Incomplete {
                blocker: r####"h"####,
            },
            graph: None,
            stats: (2301, 119, 1),
        },
        Root {
            name: "wideMember",
            want: Want::Incomplete {
                blocker: r####"h"####,
            },
            graph: None,
            stats: (2301, 119, 1),
        },
        Root {
            name: "guarded",
            want: Want::Incomplete {
                blocker: r####"h"####,
            },
            graph: None,
            stats: (2310, 121, 1),
        },
    ],
};

#[test]
fn unsupported_reference_members_keep_their_parent_caps() {
    check(
        "rfc_cases::o07::unsupported_reference_members_keep_their_parent_caps",
        &SIBLINGS,
    );
}

// ---------------------------------------------------------------------------
// ri-px1.11.16 (C19, chief rulings 21 and 22): a whole-record use whose
// reference member holds a loan on the record's own storage.
// ---------------------------------------------------------------------------

/// C19, the wrong acceptance. `h.r = &h.v;` leaves a member of `h` borrowing
/// another member of `h`. A whole-record use of such a record, whether `h`
/// moves or copies, rejects R0203 at the use (the moved operand `h`), related
/// the borrow `&h.v`, as `sharedWholeMove` in `records.rs` rejects moving a
/// record whose member a local loan covers. The forms: a call argument
/// (`moveCall`), a `let` binding (`moveLet`, and `moveLetRead` that reads
/// through the copy), a branch, a reassignment, a loop and a call in a loop.
/// `moveReturn` is the existing neighbour: the returned value escapes, R0202 at
/// `return h;`, and stays so. `moveWrite` is the writing twin: a write to `h.v`
/// while `h.r` is live, R0203 at the write. `Own2` has only copyable members, so
/// its whole-record use is a copy, not a move; the rule is the same.
const SELF_BORROW_COPY: Case = Case {
    text: r#"
type Own2 = { v: i32; r: &i32 };
type Wrap = { o: Own2 };
function take2(o: Own2): i32 { return 0i32; }

function moveCall(): i32 {
  let e = 5i32; let mut h = Own2 { v: 1i32, r: &e }; h.r = &h.v;
  return take2(h);
}
function moveLet(): i32 {
  let e = 5i32; let mut h = Own2 { v: 1i32, r: &e }; h.r = &h.v;
  let g = h; return 0i32;
}
function moveLetRead(): i32 {
  let e = 5i32; let mut h = Own2 { v: 1i32, r: &e }; h.r = &h.v;
  let g = h; return *g.r;
}
function moveBranch(): i32 {
  let e = 5i32; let mut h = Own2 { v: 1i32, r: &e }; h.r = &h.v;
  if (true) { let g = h; } return 0i32;
}
function moveAssign(): i32 {
  let e = 5i32; let mut h = Own2 { v: 1i32, r: &e }; h.r = &h.v;
  let mut g = Own2 { v: 3i32, r: &e }; g = h; return 0i32;
}
function moveLoop(): i32 {
  let e = 5i32; let mut h = Own2 { v: 1i32, r: &e }; h.r = &h.v;
  let mut again = true; while (again) { let g = h; again = false; } return 0i32;
}
function moveLoopCall(): i32 {
  let e = 5i32; let mut h = Own2 { v: 1i32, r: &e }; h.r = &h.v;
  let mut again = true; while (again) { let n = take2(h); again = false; } return 0i32;
}
function moveReturn(): Own2 {
  let e = 5i32; let mut h = Own2 { v: 1i32, r: &e }; h.r = &h.v;
  return h;
}
function moveWrite(): i32 {
  let e = 5i32; let mut h = Own2 { v: 1i32, r: &e }; h.r = &h.v;
  h.v = 2i32; let n = *h.r; return n;
}
"#,
    typed_stats: (348956, 51350, 12),
    graph: (GraphCompletion::SourceRejected, (213177, 8919, 7)),
    roots: &[
        Root {
            name: "take2",
            want: Want::Checked,
            graph: None,
            stats: (2750, 349, 3),
        },
        Root {
            name: "moveCall",
            want: Want::Rejected {
                code: "R0203",
                site: r####"h"####,
                related: r####"&h.v"####,
            },
            graph: None,
            stats: (3519, 615, 5),
        },
        Root {
            name: "moveLet",
            want: Want::Rejected {
                code: "R0203",
                site: r####"h"####,
                related: r####"&h.v"####,
            },
            graph: None,
            stats: (3509, 613, 5),
        },
        Root {
            name: "moveLetRead",
            want: Want::Rejected {
                code: "R0203",
                site: r####"h"####,
                related: r####"&h.v"####,
            },
            graph: None,
            stats: (3532, 617, 5),
        },
        Root {
            name: "moveBranch",
            want: Want::Rejected {
                code: "R0203",
                site: r####"h"####,
                related: r####"&h.v"####,
            },
            graph: None,
            stats: (3628, 683, 5),
        },
        Root {
            name: "moveAssign",
            want: Want::Rejected {
                code: "R0203",
                site: r####"h"####,
                related: r####"&h.v"####,
            },
            graph: None,
            stats: (4223, 855, 5),
        },
        Root {
            name: "moveLoop",
            want: Want::Rejected {
                code: "R0203",
                site: r####"h"####,
                related: r####"&h.v"####,
            },
            graph: None,
            stats: (3959, 867, 5),
        },
        Root {
            name: "moveLoopCall",
            want: Want::Rejected {
                code: "R0203",
                site: r####"h"####,
                related: r####"&h.v"####,
            },
            graph: None,
            stats: (4044, 909, 7),
        },
        Root {
            name: "moveReturn",
            want: Want::Rejected {
                code: "R0202",
                site: r####"return h;"####,
                related: r####"&h.v"####,
            },
            graph: None,
            stats: (3505, 606, 5),
        },
        Root {
            name: "moveWrite",
            want: Want::Rejected {
                code: "R0203",
                site: r####"h.v = 2i32;"####,
                related: r####"&h.v"####,
            },
            graph: None,
            stats: (3219, 582, 5),
        },
    ],
};

#[test]
fn a_whole_record_use_with_a_self_borrowing_member_rejects_r0203() {
    check(
        "rfc_cases::o07::a_whole_record_use_with_a_self_borrowing_member_rejects_r0203",
        &SELF_BORROW_COPY,
    );
}

/// The old caps of the seven forms that were `Checked` at `2e13bc6e`: the
/// rejection is cheaper than the acceptance was, so those caps still hold and
/// now reject (`take2`, `moveReturn` and `moveWrite` did not change).
#[test]
fn self_borrowing_record_forms_keep_their_old_caps_and_now_reject() {
    at_parent_caps(
        "rfc_cases::o07::self_borrowing_record_forms_keep_their_old_caps_and_now_reject",
        SELF_BORROW_COPY.text,
        (215532, 9926, 7),
        &[
            ("moveCall", (6465, 1052, 5)),
            ("moveLet", (3722, 705, 5)),
            ("moveLetRead", (3749, 709, 5)),
            ("moveBranch", (3883, 795, 5)),
            ("moveAssign", (4612, 1028, 5)),
            ("moveLoop", (4416, 1074, 5)),
            ("moveLoopCall", (7312, 1506, 7)),
        ],
        Parent::Rejects("R0203"),
    );
}

/// C19 valid twins, `Checked`. `deadLoan` is the chief's twin: `k` borrows
/// `h.v` but is dead before `take2(h)`, and `h.r` borrows nothing of `h`.
/// `plainCopy` and `plainLet` copy a record whose reference member borrows a
/// local outside it. The rule costs one work unit on each whole-record read of a
/// record whose reference member holds a loan (`h.r` holds the loan on `e`), so
/// each root and the whole graph cost 1 and 3 more than at `2e13bc6e`.
const SELF_BORROW_TWINS: Case = Case {
    text: r#"
type Own2 = { v: i32; r: &i32 };
type Wrap = { o: Own2 };
function take2(o: Own2): i32 { return 0i32; }

function deadLoan(): i32 {
  let e = 5i32; let mut h = Own2 { v: 1i32, r: &e };
  let k = &h.v; return take2(h);
}
function plainCopy(): i32 {
  let e = 5i32; let mut h = Own2 { v: 1i32, r: &e };
  return take2(h);
}
function plainLet(): i32 {
  let e = 5i32; let mut h = Own2 { v: 1i32, r: &e };
  let g = h; return *g.r;
}
"#,
    typed_stats: (330595, 48481, 12),
    graph: (GraphCompletion::Incomplete, (193546, 5512, 5)),
    roots: &[
        Root {
            name: "take2",
            want: Want::Checked,
            graph: None,
            stats: (2432, 190, 3),
        },
        Root {
            name: "deadLoan",
            want: Want::Checked,
            graph: None,
            stats: (5734, 685, 5),
        },
        Root {
            name: "plainCopy",
            want: Want::Checked,
            graph: None,
            stats: (5608, 627, 5),
        },
        Root {
            name: "plainLet",
            want: Want::Checked,
            graph: None,
            stats: (3210, 443, 5),
        },
    ],
};

#[test]
fn copying_a_record_that_borrows_nothing_of_itself_stays_checked() {
    check(
        "rfc_cases::o07::copying_a_record_that_borrows_nothing_of_itself_stays_checked",
        &SELF_BORROW_TWINS,
    );
}

/// Metering rule 11, third case of chief ruling 22: the one work unit above.
/// The caps `2e13bc6e` measured now give `ResourceLimit`.
#[test]
fn self_borrow_twin_parent_caps_now_give_resource_limit() {
    at_parent_caps(
        "rfc_cases::o07::self_borrow_twin_parent_caps_now_give_resource_limit",
        SELF_BORROW_TWINS.text,
        (193543, 5512, 5),
        &[
            ("deadLoan", (5733, 685, 5)),
            ("plainCopy", (5607, 627, 5)),
            ("plainLet", (3209, 443, 5)),
        ],
        Parent::Exceeds,
    );
}

/// C19, the other whole-value move paths. Each stops before a move that could
/// be accepted: a struct-literal field, a member store, a record update, a
/// tuple element, a list element, a return through a tuple, list, `Option` or
/// `Result`, a `let` of `Option.Some(h)`, a `match` scrutinee (`h`, `take2(h)` or
/// `Option.Some(h)`) and an arrow body. Each is `Incomplete` at the
/// blocker named, not an acceptance. They were the same at `2e13bc6e` (same
/// caps, same blockers).
const SELF_BORROW_UNREACHED: Case = Case {
    text: r#"
type Own2 = { v: i32; r: &i32 };
type Wrap = { o: Own2 };
function take2(o: Own2): i32 { return 0i32; }

function moveStructField(): i32 {
  let e = 5i32; let mut h = Own2 { v: 1i32, r: &e }; h.r = &h.v;
  let w = Wrap { o: h }; return 0i32;
}
function moveStructReturn(): Wrap {
  let e = 5i32; let mut h = Own2 { v: 1i32, r: &e }; h.r = &h.v;
  return Wrap { o: h };
}
function moveMemberStore(): i32 {
  let e = 5i32; let mut h = Own2 { v: 1i32, r: &e }; h.r = &h.v;
  let mut w = Wrap { o: Own2 { v: 3i32, r: &e } }; w.o = h; return 0i32;
}
function moveRecordUpdate(): i32 {
  let e = 5i32; let mut h = Own2 { v: 1i32, r: &e }; h.r = &h.v;
  let g = Own2 { ...h, v: 2i32 }; return 0i32;
}
function moveTuple(): i32 {
  let e = 5i32; let mut h = Own2 { v: 1i32, r: &e }; h.r = &h.v;
  let t: [Own2, i32] = [h, 1i32]; return 0i32;
}
function moveTupleReturn(): [Own2, i32] {
  let e = 5i32; let mut h = Own2 { v: 1i32, r: &e }; h.r = &h.v;
  return [h, 1i32];
}
function moveList(): i32 {
  let e = 5i32; let mut h = Own2 { v: 1i32, r: &e }; h.r = &h.v;
  let l: List<Own2> = [h]; return 0i32;
}
function moveListReturn(): List<Own2> {
  let e = 5i32; let mut h = Own2 { v: 1i32, r: &e }; h.r = &h.v;
  return [h];
}
function moveWrapperReturn(): Option<Own2> {
  let e = 5i32; let mut h = Own2 { v: 1i32, r: &e }; h.r = &h.v;
  return Option.Some(h);
}
function moveResultReturn(): Result<Own2, i32> {
  let e = 5i32; let mut h = Own2 { v: 1i32, r: &e }; h.r = &h.v;
  return Result.Ok(h);
}
function moveMatch(): i32 {
  let e = 5i32; let mut h = Own2 { v: 1i32, r: &e }; h.r = &h.v;
  let n = match (h) { x => 0i32 }; return n;
}
function moveMatchCall(): i32 {
  let e = 5i32; let mut h = Own2 { v: 1i32, r: &e }; h.r = &h.v;
  let n = match (take2(h)) { 0i32 => 0i32, _ => 1i32 }; return n;
}
function moveArrowCall(): i32 {
  let e = 5i32; let mut h = Own2 { v: 1i32, r: &e }; h.r = &h.v;
  let f: () => i32 = () => h.v; return f();
}
function moveWrapperLet(): i32 {
  let e = 5i32; let mut h = Own2 { v: 1i32, r: &e }; h.r = &h.v;
  let o = Option.Some(h); return 0i32;
}
function moveMatchWrapper(): i32 {
  let e = 5i32; let mut h = Own2 { v: 1i32, r: &e }; h.r = &h.v;
  let n = match (Option.Some(h)) { Option.Some(x) => 0i32, Option.None => 1i32 }; return n;
}
"#,
    typed_stats: (365714, 54164, 12),
    graph: (GraphCompletion::Incomplete, (216715, 6116, 5)),
    roots: &[
        Root {
            name: "take2",
            want: Want::Checked,
            graph: None,
            stats: (2998, 473, 3),
        },
        Root {
            name: "moveStructField",
            want: Want::Incomplete {
                blocker: r####"w"####,
            },
            graph: None,
            stats: (3226, 462, 1),
        },
        Root {
            name: "moveStructReturn",
            want: Want::Incomplete {
                blocker: r####"Wrap { o: h }"####,
            },
            graph: None,
            stats: (3229, 471, 2),
        },
        Root {
            name: "moveMemberStore",
            want: Want::Incomplete {
                blocker: r####"w"####,
            },
            graph: None,
            stats: (3226, 462, 1),
        },
        Root {
            name: "moveRecordUpdate",
            want: Want::Incomplete {
                blocker: r####"Own2 { ...h, v: 2i32 }"####,
            },
            graph: None,
            stats: (3262, 479, 3),
        },
        Root {
            name: "moveTuple",
            want: Want::Incomplete {
                blocker: r####"t"####,
            },
            graph: None,
            stats: (3230, 469, 2),
        },
        Root {
            name: "moveTupleReturn",
            want: Want::Incomplete {
                blocker: r####"[h, 1i32]"####,
            },
            graph: None,
            stats: (3239, 477, 2),
        },
        Root {
            name: "moveList",
            want: Want::Incomplete {
                blocker: r####"l"####,
            },
            graph: None,
            stats: (2866, 404, 1),
        },
        Root {
            name: "moveListReturn",
            want: Want::Incomplete {
                blocker: r####"[h]"####,
            },
            graph: None,
            stats: (2875, 412, 2),
        },
        Root {
            name: "moveWrapperReturn",
            want: Want::Incomplete {
                blocker: r####"Option.Some(h)"####,
            },
            graph: None,
            stats: (2875, 412, 2),
        },
        Root {
            name: "moveResultReturn",
            want: Want::Incomplete {
                blocker: r####"Result.Ok(h)"####,
            },
            graph: None,
            stats: (2875, 412, 2),
        },
        Root {
            name: "moveMatch",
            want: Want::Incomplete {
                blocker: r####"match (h) { x => 0i32 }"####,
            },
            graph: None,
            stats: (2897, 420, 3),
        },
        Root {
            name: "moveMatchCall",
            want: Want::Incomplete {
                blocker: r####"match (take2(h)) { 0i32 => 0i32, _ => 1i32 }"####,
            },
            graph: None,
            stats: (2897, 420, 3),
        },
        Root {
            name: "moveArrowCall",
            want: Want::Incomplete {
                blocker: r####"f()"####,
            },
            graph: None,
            stats: (2887, 419, 3),
        },
        Root {
            name: "moveWrapperLet",
            want: Want::Incomplete {
                blocker: r####"o"####,
            },
            graph: None,
            stats: (2866, 404, 1),
        },
        Root {
            name: "moveMatchWrapper",
            want: Want::Incomplete {
                blocker: r####"match (Option.Some(h)) { Option.Some(x) => 0i32, Option.None => 1i32 }"####,
            },
            graph: None,
            stats: (2897, 420, 3),
        },
    ],
};

#[test]
fn other_whole_value_paths_of_a_self_borrowing_record_stay_incomplete() {
    check(
        "rfc_cases::o07::other_whole_value_paths_of_a_self_borrowing_record_stay_incomplete",
        &SELF_BORROW_UNREACHED,
    );
}

/// Ruling 21 (2), 2C-F2. The member-replacement check covers only a loan
/// through a sibling reference member. `Own` (`h.r = &h.v`, with an `&mut`
/// member beside) and `Own2` are `Checked` again, and `OwnM` (`h.m = &mut h.v`)
/// is `Incomplete` at its block, as at `d87e5dd2`. The twins reject while
/// `h.r` is live: writing `h.v` (R0203 at the write), moving `h` into a call,
/// returning it and copying it to a local (R0203 at `h`, related `&h.v`). The
/// `&mut` self-borrow `ownMTake` and `ownMutMember` rejects at the move too.
const SELF_BORROW_MEMBERS: Case = Case {
    text: r#"
type Own2 = { v: i32; r: &i32 };
type Own = { v: i32; r: &i32; m: &mut i32 };
type OwnM = { v: i32; m: &mut i32 };

function take2(o: Own2): i32 { return 0i32; }
function takeOwn(o: Own): i32 { return 0i32; }
function takeOwnM(o: OwnM): i32 { return 0i32; }
function replaceOwn(): i32 {
  let mut a = 1i32; let e = 5i32; let mut h = Own { v: 1i32, r: &e, m: &mut a }; h.r = &h.v;
  return 0i32;
}
function replaceOwn2(): i32 {
  let e = 5i32; let mut h = Own2 { v: 1i32, r: &e }; h.r = &h.v;
  return 0i32;
}
function replaceOwnM(): i32 {
  let mut a = 1i32; let mut h = OwnM { v: 1i32, m: &mut a }; h.m = &mut h.v;
  return 0i32;
}
function ownWrite(): i32 {
  let mut a = 1i32; let e = 5i32; let mut h = Own { v: 1i32, r: &e, m: &mut a }; h.r = &h.v;
  h.v = 2i32; let n = *h.r; return n;
}
function ownReturn(): Own {
  let mut a = 1i32; let e = 5i32; let mut h = Own { v: 1i32, r: &e, m: &mut a }; h.r = &h.v;
  return h;
}
function ownTake(): i32 {
  let mut a = 1i32; let e = 5i32; let mut h = Own { v: 1i32, r: &e, m: &mut a }; h.r = &h.v;
  return takeOwn(h);
}
function ownLet(): i32 {
  let mut a = 1i32; let e = 5i32; let mut h = Own { v: 1i32, r: &e, m: &mut a }; h.r = &h.v;
  let g = h; return *g.r;
}
function ownMTake(): i32 {
  let mut a = 1i32; let mut h = OwnM { v: 1i32, m: &mut a }; h.m = &mut h.v;
  return takeOwnM(h);
}
function ownMutMember(): i32 {
  let mut a = 1i32; let e = 5i32; let mut h = Own { v: 1i32, r: &e, m: &mut a };
  h.m = &mut h.v; return takeOwn(h);
}
"#,
    typed_stats: (353793, 51619, 12),
    graph: (GraphCompletion::SourceRejected, (213613, 8865, 5)),
    roots: &[
        Root {
            name: "take2",
            want: Want::Checked,
            graph: None,
            stats: (2766, 357, 3),
        },
        Root {
            name: "takeOwn",
            want: Want::Checked,
            graph: None,
            stats: (2790, 368, 3),
        },
        Root {
            name: "takeOwnM",
            want: Want::Checked,
            graph: None,
            stats: (2766, 357, 3),
        },
        Root {
            name: "replaceOwn",
            want: Want::Checked,
            graph: None,
            stats: (3411, 658, 5),
        },
        Root {
            name: "replaceOwn2",
            want: Want::Checked,
            graph: None,
            stats: (3147, 541, 5),
        },
        Root {
            name: "replaceOwnM",
            want: Want::Incomplete {
                blocker: r####"{\n  let mut a = 1i32; let mut h = OwnM { v: 1i32, m: &mut a }; h.m = &mut h.v;\n  return 0i32;\n}"####,
            },
            graph: None,
            stats: (3107, 527, 5),
        },
        Root {
            name: "ownWrite",
            want: Want::Rejected {
                code: "R0203",
                site: r####"h.v = 2i32;"####,
                related: r####"&h.v"####,
            },
            graph: None,
            stats: (3494, 708, 5),
        },
        Root {
            name: "ownReturn",
            want: Want::Rejected {
                code: "R0203",
                site: r####"h"####,
                related: r####"&h.v"####,
            },
            graph: None,
            stats: (3756, 729, 5),
        },
        Root {
            name: "ownTake",
            want: Want::Rejected {
                code: "R0203",
                site: r####"h"####,
                related: r####"&h.v"####,
            },
            graph: None,
            stats: (3812, 760, 5),
        },
        Root {
            name: "ownLet",
            want: Want::Rejected {
                code: "R0203",
                site: r####"h"####,
                related: r####"&h.v"####,
            },
            graph: None,
            stats: (3825, 762, 5),
        },
        Root {
            name: "ownMTake",
            want: Want::Rejected {
                code: "R0203",
                site: r####"h"####,
                related: r####"&mut h.v"####,
            },
            graph: None,
            stats: (3541, 624, 5),
        },
        Root {
            name: "ownMutMember",
            want: Want::Rejected {
                code: "R0203",
                site: r####"h"####,
                related: r####"&mut h.v"####,
            },
            graph: None,
            stats: (3815, 760, 5),
        },
    ],
};

#[test]
fn the_member_replacement_check_covers_only_sibling_reference_members() {
    check(
        "rfc_cases::o07::the_member_replacement_check_covers_only_sibling_reference_members",
        &SELF_BORROW_MEMBERS,
    );
}

/// Metering rule 11, third case of chief ruling 22: the replacements that
/// rejected R0201 at `2e13bc6e` now run on (`Checked`, `Incomplete`, or to the
/// move that rejects), which costs more, so the old caps give `ResourceLimit`.
#[test]
fn self_borrow_member_parent_caps_now_give_resource_limit() {
    at_parent_caps(
        "rfc_cases::o07::self_borrow_member_parent_caps_now_give_resource_limit",
        SELF_BORROW_MEMBERS.text,
        (213012, 8571, 5),
        &[
            ("replaceOwn", (3212, 575, 5)),
            ("replaceOwnM", (3014, 480, 5)),
            ("ownWrite", (3419, 664, 5)),
            ("ownReturn", (3709, 705, 5)),
            ("ownTake", (3762, 734, 5)),
            ("ownLet", (3775, 736, 5)),
            ("ownMTake", (3502, 604, 5)),
            ("ownMutMember", (3763, 734, 5)),
        ],
        Parent::Exceeds,
    );
}

/// C19 move path that stops in the typed analysis, before any ownership
/// judgment: an arrow that captures the record. It is `Incomplete` with no
/// receipt at `2e13bc6e` as now, so it is not an acceptance. The pinned signal
/// is what the typed report names.
#[test]
fn an_arrow_capturing_a_self_borrowing_record_stays_typed_incomplete() {
    const SOURCE: &str = "type Own2 = { v: i32; r: &i32 };
function captured(): i32 {
  let e = 5i32;
  let mut h = Own2 { v: 1i32, r: &e };
  h.r = &h.v;
  let f: () => Own2 = () => h;
  return 0i32;
}
";
    installed(
        "rfc_cases::o07::an_arrow_capturing_a_self_borrowing_record_stays_typed_incomplete",
        |request| {
            fs::write(&request.entry, format!("{IMPORTS}\n{SOURCE}")).unwrap();
            for test_mode in [false, true] {
                let request = InstalledRequest {
                    test_mode,
                    ..request.clone()
                };
                let graph = loader::load_installed(request).unwrap();
                let typed = analyze_typed(&graph, AnalysisLimits::default());
                let module = graph
                    .modules()
                    .iter()
                    .find(|m| m.key.origin == Origin::Application && m.key.path == "main.rill")
                    .unwrap();
                check_typed(
                    &format!("test_mode {test_mode}"),
                    &graph,
                    &typed,
                    module,
                    TypedCompletion::Incomplete,
                    &["pending Containment @ () => h"],
                    (324232, 47806, 12),
                );
            }
        },
    );
}
