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
