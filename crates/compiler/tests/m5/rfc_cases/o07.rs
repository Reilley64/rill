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
            crate::limit::assert_stats(typed.statistics(), statistics(case.typed_stats), &context);
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
    typed_stats: (327130, 48724, 12),
    graph: (GraphCompletion::SourceRejected, (191331, 4475, 5)),
    roots: &[
        Root {
            name: "overlapConstruct",
            want: Want::Rejected {
                code: "R0201",
                site: r####"&value"####,
                related: r####"&mut value"####,
            },
            graph: None,
            stats: (2612, 203, 5),
        },
        Root {
            name: "overlapReversed",
            want: Want::Rejected {
                code: "R0201",
                site: r####"&mut value"####,
                related: r####"&value"####,
            },
            graph: None,
            stats: (2612, 203, 5),
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
    typed_stats: (336761, 50174, 12),
    graph: (GraphCompletion::SourceRejected, (200387, 5724, 6)),
    roots: &[
        Root {
            name: "overlapNoUse",
            want: Want::Rejected {
                code: "R0201",
                site: r####"&mut a"####,
                related: r####"&mut a"####,
            },
            graph: None,
            stats: (2979, 385, 5),
        },
        Root {
            name: "replacesFirst",
            want: Want::Rejected {
                code: "R0201",
                site: r####"&mut b"####,
                related: r####"&mut b"####,
            },
            graph: None,
            stats: (2974, 384, 5),
        },
        Root {
            name: "sharedMember",
            want: Want::Rejected {
                code: "R0201",
                site: r####"&a"####,
                related: r####"&mut a"####,
            },
            graph: None,
            stats: (2979, 385, 5),
        },
        Root {
            name: "sharedFirst",
            want: Want::Rejected {
                code: "R0201",
                site: r####"&a"####,
                related: r####"&mut a"####,
            },
            graph: None,
            stats: (2974, 384, 5),
        },
        Root {
            name: "looped",
            want: Want::Rejected {
                code: "R0201",
                site: r####"&mut a"####,
                related: r####"&mut a"####,
            },
            graph: None,
            stats: (3225, 514, 6),
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
        (201866, 6418, 6),
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
    typed_stats: (355117, 52878, 12),
    graph: (GraphCompletion::SourceRejected, (219492, 8614, 6)),
    roots: &[
        Root {
            name: "reborrowDirect",
            want: Want::Rejected {
                code: "R0201",
                site: r####"holder.y = &mut *holder.x;"####,
                related: r####"&mut *holder.x"####,
            },
            graph: None,
            stats: (3381, 577, 5),
        },
        Root {
            name: "reborrowThenWrite",
            want: Want::Rejected {
                code: "R0201",
                site: r####"holder.y = &mut *holder.x;"####,
                related: r####"&mut *holder.x"####,
            },
            graph: None,
            stats: (3597, 684, 5),
        },
        Root {
            name: "reborrowLocal",
            want: Want::Rejected {
                code: "R0201",
                site: r####"holder.y = r;"####,
                related: r####"&mut *holder.x"####,
            },
            graph: None,
            stats: (3494, 625, 5),
        },
        Root {
            name: "reborrowCall",
            want: Want::Rejected {
                code: "R0201",
                site: r####"holder.y = same(&mut *holder.x);"####,
                related: r####"&mut *holder.x"####,
            },
            graph: None,
            stats: (3476, 629, 6),
        },
        Root {
            name: "reborrowParam",
            want: Want::Rejected {
                code: "R0201",
                site: r####"k.y = &mut *k.x;"####,
                related: r####"&mut *k.x"####,
            },
            graph: None,
            stats: (3611, 545, 4),
        },
        Root {
            name: "reborrowShared",
            want: Want::Rejected {
                code: "R0201",
                site: r####"holder.second = &*holder.first;"####,
                related: r####"&*holder.first"####,
            },
            graph: None,
            stats: (3381, 577, 5),
        },
        Root {
            name: "siblingReplacedAfter",
            want: Want::Rejected {
                code: "R0201",
                site: r####"holder.y = &mut *holder.x;"####,
                related: r####"&mut *holder.x"####,
            },
            graph: None,
            stats: (4050, 908, 5),
        },
        Root {
            name: "reborrowKept",
            want: Want::Checked,
            graph: None,
            stats: (3468, 611, 5),
        },
        Root {
            name: "reborrowRead",
            want: Want::Checked,
            graph: None,
            stats: (3483, 613, 5),
        },
        Root {
            name: "reborrowPassed",
            want: Want::Checked,
            graph: None,
            stats: (6024, 933, 6),
        },
        Root {
            name: "reborrowSharedKept",
            want: Want::Checked,
            graph: None,
            stats: (3484, 613, 5),
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
    typed_stats: (326133, 48663, 12),
    graph: (GraphCompletion::SourceRejected, (192112, 4708, 5)),
    roots: &[
        Root {
            name: "take",
            want: Want::Checked,
            graph: None,
            stats: (2537, 168, 3),
        },
        Root {
            name: "takeAfterReborrow",
            want: Want::Rejected {
                code: "R0201",
                site: r####"holder.y = &mut *holder.x;"####,
                related: r####"&mut *holder.x"####,
            },
            graph: None,
            stats: (3475, 472, 5),
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

/// Re-review ci-9qga 2C-F1, pinned as a conservative stated limit (chief
/// ruling 21 (3)): a value read from a
/// member of another record carries the loans of that whole record
/// (`Action::Reborrow` builds them through `carried_loan_root`), so when
/// `other` also holds a reborrow of `holder.x` the store into `holder.y`
/// rejects R0201 although `holder.x` and `holder.y` end up pointing at
/// different places (`a` and `c`). Each source below is sound and was
/// `Checked` at `d87e5dd2`; the member-replacement check over-rejects it.
/// Narrowing needs a member-only loan set per record member, which is the
/// per-member loan tracking ruling 21 excludes from M5; ri-qlh holds the
/// follow-up. The pins are exact and one short on each axis in both load
/// modes. A change that makes any of them `Checked` is the fix landing: edit
/// the pin then, with that bead.
const OVER_REJECTION: Case = Case {
    text: r#"
type Pair = { x: &mut i32; y: &mut i32 };
type Holder = { first: &mut i32; second: &i32 };
function storeFromBuilt(): i32 {
  let mut a = 1i32;
  let mut b = 2i32;
  let mut c = 3i32;
  let mut holder = Pair { x: &mut a, y: &mut b };
  let other = Pair { x: &mut c, y: &mut *holder.x };
  holder.y = other.x;
  return 0i32;
}
function storeFromBuiltThenWrite(): i32 {
  let mut a = 1i32;
  let mut b = 2i32;
  let mut c = 3i32;
  let mut holder = Pair { x: &mut a, y: &mut b };
  let other = Pair { x: &mut c, y: &mut *holder.x };
  holder.y = other.x;
  *holder.y += 1i32;
  return 0i32;
}
function storeFromSetMember(): i32 {
  let mut a = 1i32;
  let mut b = 2i32;
  let mut c = 3i32;
  let mut d = 4i32;
  let mut holder = Pair { x: &mut a, y: &mut b };
  let mut other = Pair { x: &mut c, y: &mut d };
  other.y = &mut *holder.x;
  holder.y = other.x;
  return 0i32;
}
function storeReborrowOfRead(): i32 {
  let mut a = 1i32;
  let mut b = 2i32;
  let mut c = 3i32;
  let mut d = 4i32;
  let mut holder = Pair { x: &mut a, y: &mut b };
  let mut other = Pair { x: &mut c, y: &mut d };
  other.y = &mut *holder.x;
  holder.y = &mut *other.x;
  return 0i32;
}
function storeReborrowViaLocal(): i32 {
  let mut a = 1i32;
  let mut b = 2i32;
  let mut c = 3i32;
  let mut d = 4i32;
  let mut holder = Pair { x: &mut a, y: &mut b };
  let mut other = Pair { x: &mut c, y: &mut d };
  other.y = &mut *holder.x;
  let r = &mut *other.x;
  holder.y = r;
  return 0i32;
}
function storeSharedFromShared(): i32 {
  let mut a = 1i32;
  let e = 5i32;
  let f = 6i32;
  let mut c = 3i32;
  let mut h = Holder { first: &mut a, second: &e };
  let mut o = Holder { first: &mut c, second: &f };
  o.second = &*h.first;
  let k = &*o.first;
  h.second = k;
  return 0i32;
}
"#,
    typed_stats: (346508, 52562, 12),
    graph: (GraphCompletion::SourceRejected, (217129, 9075, 5)),
    roots: &[
        Root {
            name: "storeFromBuilt",
            want: Want::Rejected {
                code: "R0201",
                site: r####"holder.y = other.x;"####,
                related: r####"&mut *holder.x"####,
            },
            graph: None,
            stats: (4281, 820, 5),
        },
        Root {
            name: "storeFromBuiltThenWrite",
            want: Want::Rejected {
                code: "R0201",
                site: r####"holder.y = other.x;"####,
                related: r####"&mut *holder.x"####,
            },
            graph: None,
            stats: (4543, 953, 5),
        },
        Root {
            name: "storeFromSetMember",
            want: Want::Rejected {
                code: "R0201",
                site: r####"holder.y = other.x;"####,
                related: r####"&mut *holder.x"####,
            },
            graph: None,
            stats: (4735, 1037, 5),
        },
        Root {
            name: "storeReborrowOfRead",
            want: Want::Rejected {
                code: "R0201",
                site: r####"holder.y = &mut *other.x;"####,
                related: r####"&mut *holder.x"####,
            },
            graph: None,
            stats: (4722, 1039, 5),
        },
        Root {
            name: "storeReborrowViaLocal",
            want: Want::Rejected {
                code: "R0201",
                site: r####"holder.y = r;"####,
                related: r####"&mut *holder.x"####,
            },
            graph: None,
            stats: (4898, 1115, 5),
        },
        Root {
            name: "storeSharedFromShared",
            want: Want::Rejected {
                code: "R0201",
                site: r####"h.second = k;"####,
                related: r####"&*h.first"####,
            },
            graph: None,
            stats: (4889, 1115, 5),
        },
    ],
};

#[test]
fn member_replacement_pins_an_over_rejection_of_sound_stores_from_another_record() {
    check(
        "rfc_cases::o07::member_replacement_pins_an_over_rejection_of_sound_stores_from_another_record",
        &OVER_REJECTION,
    );
}

/// Re-review ci-9qga 2C-F3: a call result carries the loan of every
/// argument (RFC 0007:28), so `pickFirst(&mut c, &mut *holder.x)` carries the
/// reborrow of `holder.x` although it returns its first argument, `&mut c`.
/// Storing it into `holder.y` rejects R0201: conservative, and
/// the same limit as `OVER_REJECTION` (the call does not say which argument
/// the result keeps). `pickFirst` itself checks.
const CALL_RESULT_CARRIES_EVERY_LOAN: Case = Case {
    text: r#"
type Pair = { x: &mut i32; y: &mut i32 };
function pickFirst(a: &mut i32, b: &mut i32): &mut i32 { return a; }
function storePickedFirst(): i32 {
  let mut a = 1i32;
  let mut b = 2i32;
  let mut c = 3i32;
  let mut holder = Pair { x: &mut a, y: &mut b };
  holder.y = pickFirst(&mut c, &mut *holder.x);
  return 0i32;
}
"#,
    typed_stats: (326919, 48770, 12),
    graph: (GraphCompletion::SourceRejected, (191918, 4707, 6)),
    roots: &[
        Root {
            name: "pickFirst",
            want: Want::Checked,
            graph: None,
            stats: (2092, 128, 3),
        },
        Root {
            name: "storePickedFirst",
            want: Want::Rejected {
                code: "R0201",
                site: r####"holder.y = pickFirst(&mut c, &mut *holder.x);"####,
                related: r####"&mut *holder.x"####,
            },
            graph: None,
            stats: (3257, 519, 6),
        },
    ],
};

#[test]
fn member_replacement_pins_a_call_result_that_carries_every_argument_loan() {
    check(
        "rfc_cases::o07::member_replacement_pins_a_call_result_that_carries_every_argument_loan",
        &CALL_RESULT_CARRIES_EVERY_LOAN,
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
    typed_stats: (330189, 48960, 12),
    graph: (GraphCompletion::SourceRejected, (194926, 4990, 7)),
    roots: &[
        Root {
            name: "refFirst",
            want: Want::Checked,
            graph: None,
            stats: (2580, 190, 3),
        },
        Root {
            name: "movedTwice",
            want: Want::Rejected {
                code: "R0200",
                site: r####"h"####,
                related: r####"h"####,
            },
            graph: None,
            stats: (3634, 409, 5),
        },
        Root {
            name: "memberAndReferent",
            want: Want::Rejected {
                code: "R0201",
                site: r####"&mut a"####,
                related: r####"&a"####,
            },
            graph: None,
            stats: (2578, 223, 7),
        },
        Root {
            name: "dupParam",
            want: Want::Rejected {
                code: "R0200",
                site: r####"p"####,
                related: r####"p"####,
            },
            graph: None,
            stats: (2611, 205, 4),
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
    typed_stats: (364769, 54767, 12),
    graph: (GraphCompletion::SourceRejected, (233286, 11538, 7)),
    roots: &[
        Root {
            name: "disjoint",
            want: Want::Checked,
            graph: None,
            stats: (4037, 886, 5),
        },
        Root {
            name: "twice",
            want: Want::Checked,
            graph: None,
            stats: (4661, 1198, 5),
        },
        Root {
            name: "endsBeforeSource",
            want: Want::Checked,
            graph: None,
            stats: (4009, 882, 5),
        },
        Root {
            name: "twoOwners",
            want: Want::Checked,
            graph: None,
            stats: (4724, 1214, 5),
        },
        Root {
            name: "groupedBorrows",
            want: Want::Checked,
            graph: None,
            stats: (3971, 872, 6),
        },
        Root {
            name: "looped",
            want: Want::Checked,
            graph: None,
            stats: (4402, 1074, 5),
        },
        Root {
            name: "replaced",
            want: Want::Checked,
            graph: None,
            stats: (4951, 1209, 5),
        },
        Root {
            name: "withText",
            want: Want::Checked,
            graph: None,
            stats: (3815, 804, 5),
        },
        Root {
            name: "sameOwnerTwice",
            want: Want::Rejected {
                code: "R0201",
                site: r####"&mut a"####,
                related: r####"&mut a"####,
            },
            graph: None,
            stats: (3415, 600, 5),
        },
        Root {
            name: "readOwnerWhileHeld",
            want: Want::Rejected {
                code: "R0201",
                site: r####"a"####,
                related: r####"&mut a"####,
            },
            graph: None,
            stats: (3656, 720, 5),
        },
        Root {
            name: "secondHeldOverFirst",
            want: Want::Rejected {
                code: "R0201",
                site: r####"&mut b"####,
                related: r####"&b"####,
            },
            graph: None,
            stats: (4239, 821, 5),
        },
        Root {
            name: "scopedOverlap",
            want: Want::Rejected {
                code: "R0201",
                site: r####"&a"####,
                related: r####"&mut a"####,
            },
            graph: None,
            stats: (3514, 662, 7),
        },
        Root {
            name: "assignThrough",
            want: Want::Incomplete {
                blocker: r####"*holder.first"####,
            },
            graph: None,
            stats: (3077, 438, 3),
        },
        Root {
            name: "groupedTarget",
            want: Want::Incomplete {
                blocker: r####"*(holder.first) += 1i32;"####,
            },
            graph: None,
            stats: (3075, 438, 3),
        },
        Root {
            name: "movedOut",
            want: Want::Incomplete {
                blocker: r####"*moved += 1i32;"####,
            },
            graph: None,
            stats: (3082, 443, 3),
        },
        Root {
            name: "reborrowed",
            want: Want::Incomplete {
                blocker: r####"*view += 1i32;"####,
            },
            graph: None,
            stats: (3082, 443, 3),
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
    typed_stats: (334310, 49203, 12),
    graph: (GraphCompletion::Incomplete, (194233, 4432, 5)),
    roots: &[
        Root {
            name: "boolMember",
            want: Want::Incomplete {
                blocker: r####"h"####,
            },
            graph: None,
            stats: (2452, 120, 1),
        },
        Root {
            name: "textMember",
            want: Want::Incomplete {
                blocker: r####"h"####,
            },
            graph: None,
            stats: (2450, 119, 1),
        },
        Root {
            name: "wideMember",
            want: Want::Incomplete {
                blocker: r####"h"####,
            },
            graph: None,
            stats: (2450, 119, 1),
        },
        Root {
            name: "guarded",
            want: Want::Incomplete {
                blocker: r####"h"####,
            },
            graph: None,
            stats: (2657, 121, 1),
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
