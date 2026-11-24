//! RFC 0007 O7, ri-px1.11.16 (C19b, chief ruling 25): a record whose reference
//! member borrows the record's own storage (`h.r = &h.v;`), copied or moved.
//!
//! A record of copyable members (`Own2 = { v: i32; r: &i32 }`) copies, and the
//! copy keeps the original's dependency (RFC 0007 lines 107, 113 and 190). A
//! record with a `string` member (`OwnS`) moves. Every outcome here is what the
//! base gives; the copy is not rejected for being a copy (C19's copy rejection
//! was dropped after probe ci-0xlg). A `Checked` root pins its whole MIR
//! sequence, so `copy` against `move` is asserted. A root that stops
//! `Incomplete` is an unanalysed route, not evidence of soundness. These are
//! compiler judgments over MIR, not Rill execution.
use super::o07_forms::{Form, check};
use super::*;
use compiler::analysis::typed::TypedCompletion;

const WRITES: Form = Form {
    text: r#"
type Own2 = { v: i32; r: &i32 };
function keep(o: Own2): &i32 { return o.r; }
function p1WriteAfterCopy(): i32 {
  let e = 5i32; let mut h = Own2 { v: 1i32, r: &e }; h.r = &h.v; let g = h; h.v = 3i32; return *g.r;
}
function p2ReplaceAfterCopy(): i32 {
  let e = 5i32; let mut h = Own2 { v: 1i32, r: &e }; h.r = &h.v; let g = h; h = Own2 { v: 9i32, r: &e }; return *g.r;
}
function p6WriteAfterCallCopy(): i32 {
  let e = 5i32; let mut h = Own2 { v: 1i32, r: &e }; h.r = &h.v; let p = keep(h); h.v = 3i32; return *p;
}
function p18InnerScopeDies(): i32 {
  let e = 5i32; let mut h = Own2 { v: 1i32, r: &e }; h.r = &h.v; let mut g = Own2 { v: 0i32, r: &e }; { let mut h2 = Own2 { v: 1i32, r: &e }; h2.r = &h2.v; g = h2; } return *g.r;
}
function writeWhileMemberLive(): i32 {
  let e = 5i32; let mut h = Own2 { v: 1i32, r: &e }; h.r = &h.v; h.v = 2i32; let n = *h.r; return n;
}
"#,
    typed_stats: (340266, 50285, 12),
    graph: (GraphCompletion::SourceRejected, (205171, 7678, 6)),
    roots: &[
        Root {
            name: "p1WriteAfterCopy",
            want: Want::Rejected {
                code: "R0203",
                site: r####"h.v = 3i32;"####,
                related: r####"&h.v"####,
            },
            graph: None,
            stats: (3584, 647, 5),
        },
        Root {
            name: "p2ReplaceAfterCopy",
            want: Want::Rejected {
                code: "R0203",
                site: r####"h = Own2 { v: 9i32, r: &e };"####,
                related: r####"&h.v"####,
            },
            graph: None,
            stats: (4202, 837, 5),
        },
        Root {
            name: "p6WriteAfterCallCopy",
            want: Want::Rejected {
                code: "R0203",
                site: r####"h.v = 3i32;"####,
                related: r####"&h.v"####,
            },
            graph: None,
            stats: (3668, 698, 5),
        },
        Root {
            name: "p18InnerScopeDies",
            want: Want::Rejected {
                code: "R0203",
                site: r####"{ let mut h2 = Own2 { v: 1i32, r: &e }; h2.r = &h2.v; g = h2; }"####,
                related: r####"&h2.v"####,
            },
            graph: None,
            stats: (5332, 1324, 6),
        },
        Root {
            name: "writeWhileMemberLive",
            want: Want::Rejected {
                code: "R0203",
                site: r####"h.v = 2i32;"####,
                related: r####"&h.v"####,
            },
            graph: None,
            stats: (3097, 521, 5),
        },
    ],
    sequences: &[],
};

/// Probe ci-0xlg P1, P2, P6 and P18, and ruling 21 (2) twin (i). `Own2` copies and the copy
/// keeps its dependency on `h.v`: a write to `h.v` (P1), a replacement of `h` (P2), a write
/// after `keep(h)` returned the copied reference (P6) and an inner record that ends while the
/// copy outside holds its reference (P18) each reject R0203. `writeWhileMemberLive` is twin (i)
/// with no copy: writing `h.v` while `h.r` is live rejects.
#[test]
fn a_copy_keeps_its_dependency_on_the_original_so_a_write_or_scope_end_rejects_r0203() {
    check(
        "rfc_cases::o07_self_borrow::a_copy_keeps_its_dependency_on_the_original_so_a_write_or_scope_end_rejects_r0203",
        &WRITES,
    );
}

const ESCAPES: Form = Form {
    text: r#"
type Own2 = { v: i32; r: &i32 };
function keepR(o: Own2): &i32 { return o.r; }
function p3ReturnTheCopy(): Own2 {
  let e = 5i32; let mut h = Own2 { v: 1i32, r: &e }; h.r = &h.v; let g = h; return g;
}
function p15ReturnTheCopysReference(): &i32 {
  let e = 5i32; let mut h = Own2 { v: 1i32, r: &e }; h.r = &h.v; let g = h; return keepR(g);
}
function returnTheRecord(): Own2 {
  let e = 5i32; let mut h = Own2 { v: 1i32, r: &e }; h.r = &h.v; return h;
}
"#,
    typed_stats: (330741, 48662, 12),
    graph: (GraphCompletion::SourceRejected, (195358, 5863, 5)),
    roots: &[
        Root {
            name: "p3ReturnTheCopy",
            want: Want::Rejected {
                code: "R0202",
                site: r####"return g;"####,
                related: r####"&h.v"####,
            },
            graph: None,
            stats: (3748, 615, 5),
        },
        Root {
            name: "p15ReturnTheCopysReference",
            want: Want::Rejected {
                code: "R0202",
                site: r####"return keepR(g);"####,
                related: r####"&h.v"####,
            },
            graph: None,
            stats: (3855, 674, 5),
        },
        Root {
            name: "returnTheRecord",
            want: Want::Rejected {
                code: "R0202",
                site: r####"return h;"####,
                related: r####"&h.v"####,
            },
            graph: None,
            stats: (3215, 461, 5),
        },
    ],
    sequences: &[],
};

/// Probe ci-0xlg P3 and P15, and ruling 21 (2) twin (ii) as corrected by ruling 25: returning
/// the copy (P3), returning the reference the copy gives (P15) or returning `h` itself
/// (`returnTheRecord`) each reject R0202, related `&h.v`.
#[test]
fn a_copy_keeps_its_dependency_on_the_original_so_an_escape_rejects_r0202() {
    check(
        "rfc_cases::o07_self_borrow::a_copy_keeps_its_dependency_on_the_original_so_an_escape_rejects_r0202",
        &ESCAPES,
    );
}

const MOVES: Form = Form {
    text: r#"
type OwnS = { v: i32; s: string; r: &i32 };
function takeS(o: OwnS): i32 { return 0i32; }
function p11MoveIntoCall(): i32 {
  let e = 5i32; let mut h = OwnS { v: 1i32, s: "a", r: &e }; h.r = &h.v; return takeS(h);
}
function p12MoveIntoLet(): i32 {
  let e = 5i32; let mut h = OwnS { v: 1i32, s: "a", r: &e }; h.r = &h.v; let g = h; return *g.r;
}
function p13MoveWhileBorrowed(): i32 {
  let e = 5i32; let mut h = OwnS { v: 1i32, s: "a", r: &e }; let k = &h.v; let g = h; return *k;
}
"#,
    typed_stats: (331503, 48782, 12),
    graph: (GraphCompletion::SourceRejected, (194670, 5698, 5)),
    roots: &[
        Root {
            name: "p11MoveIntoCall",
            want: Want::Rejected {
                code: "R0203",
                site: r####"h"####,
                related: r####"&h.v"####,
            },
            graph: None,
            stats: (3375, 547, 5),
        },
        Root {
            name: "p12MoveIntoLet",
            want: Want::Rejected {
                code: "R0203",
                site: r####"h"####,
                related: r####"&h.v"####,
            },
            graph: None,
            stats: (3389, 549, 5),
        },
        Root {
            name: "p13MoveWhileBorrowed",
            want: Want::Rejected {
                code: "R0203",
                site: r####"h"####,
                related: r####"&h.v"####,
            },
            graph: None,
            stats: (3248, 485, 5),
        },
    ],
    sequences: &[],
};

/// Probe ci-0xlg P11, P12 and P13. `OwnS` has a `string` member, so a whole-record use moves
/// it, and a move of a record whose member borrows its own storage rejects R0203 at `h`,
/// related `&h.v`. P13 has a separate live `k = &h.v`. (`p11` and `p12` are the moves; the
/// same shapes with no self-borrow are Checked, probe7 N1 and N2.)
#[test]
fn a_move_of_a_self_borrowing_record_rejects_r0203() {
    check(
        "rfc_cases::o07_self_borrow::a_move_of_a_self_borrowing_record_rejects_r0203",
        &MOVES,
    );
}

const COPIES: Form = Form {
    text: r#"
type Own2 = { v: i32; r: &i32 };
function take2(o: Own2): i32 { return 0i32; }
function keepR(o: Own2): &i32 { return o.r; }
function p7CopyIntoLet(): i32 {
  let e = 5i32; let mut h = Own2 { v: 1i32, r: &e }; h.r = &h.v; let g = h; return *g.r;
}
function p8CopyIntoCall(): i32 {
  let e = 5i32; let mut h = Own2 { v: 1i32, r: &e }; h.r = &h.v; return take2(h);
}
function p9CopyDeadBeforeWrite(): i32 {
  let e = 5i32; let mut h = Own2 { v: 1i32, r: &e }; h.r = &h.v; { let g = h; } h.v = 3i32; return 0i32;
}
function p10CopyWhileSharedBorrowed(): i32 {
  let e = 5i32; let mut h = Own2 { v: 1i32, r: &e }; h.r = &h.v; let k = &h.v; let g = h; return *k;
}
function p17CopyIntoReferenceReturningCall(): i32 {
  let e = 5i32; let mut h = Own2 { v: 1i32, r: &e }; h.r = &h.v; let p = keepR(h); return *p;
}
function deadLoanBeforeCopy(): i32 {
  let e = 5i32; let mut h = Own2 { v: 1i32, r: &e }; let k = &h.v; return take2(h);
}
"#,
    typed_stats: (340859, 49993, 12),
    graph: (GraphCompletion::Incomplete, (205562, 7723, 5)),
    roots: &[
        Root {
            name: "p7CopyIntoLet",
            want: Want::Checked,
            graph: None,
            stats: (3607, 638, 5),
        },
        Root {
            name: "p8CopyIntoCall",
            want: Want::Checked,
            graph: None,
            stats: (6181, 910, 5),
        },
        Root {
            name: "p9CopyDeadBeforeWrite",
            want: Want::Checked,
            graph: None,
            stats: (3836, 780, 5),
        },
        Root {
            name: "p10CopyWhileSharedBorrowed",
            want: Want::Checked,
            graph: None,
            stats: (3737, 703, 5),
        },
        Root {
            name: "p17CopyIntoReferenceReturningCall",
            want: Want::Checked,
            graph: None,
            stats: (6333, 977, 5),
        },
        Root {
            name: "deadLoanBeforeCopy",
            want: Want::Checked,
            graph: None,
            stats: (6085, 861, 5),
        },
    ],
    sequences: &[
        (
            "p7CopyIntoLet",
            &[
                "init e",
                "init h.v",
                "borrow e -> h.r",
                "init h",
                "borrow h.v -> &h.v",
                "replace h.r <- &h.v",
                "copy h -> g",
                "deref g.r",
                "export *g.r",
                "cleanup [end g.r, end g.v, end g, end h.r, end h.v, end h, end e]",
            ],
        ),
        (
            "p8CopyIntoCall",
            &[
                "init e",
                "init h.v",
                "borrow e -> h.r",
                "init h",
                "borrow h.v -> &h.v",
                "replace h.r <- &h.v",
                "copy h -> h",
                "call take2",
                "export take2(h)",
                "cleanup [end h.r, end h.v, end h, end e]",
            ],
        ),
        (
            "p9CopyDeadBeforeWrite",
            &[
                "init e",
                "init h.v",
                "borrow e -> h.r",
                "init h",
                "borrow h.v -> &h.v",
                "replace h.r <- &h.v",
                "copy h -> g",
                "cleanup [end g.r, end g.v, end g]",
                "init 3i32",
                "replace h.v <- 3i32",
                "init 0i32",
                "export 0i32",
                "cleanup [end h.r, end h.v, end h, end e]",
            ],
        ),
        (
            "p10CopyWhileSharedBorrowed",
            &[
                "init e",
                "init h.v",
                "borrow e -> h.r",
                "init h",
                "borrow h.v -> &h.v",
                "replace h.r <- &h.v",
                "borrow h.v -> k",
                "copy h -> g",
                "deref k",
                "export *k",
                "cleanup [end g.r, end g.v, end g, end k, end h.r, end h.v, end h, end e]",
            ],
        ),
        (
            "p17CopyIntoReferenceReturningCall",
            &[
                "init e",
                "init h.v",
                "borrow e -> h.r",
                "init h",
                "borrow h.v -> &h.v",
                "replace h.r <- &h.v",
                "copy h -> h",
                "call keepR",
                "deref p",
                "export *p",
                "cleanup [end p, end h.r, end h.v, end h, end e]",
            ],
        ),
        (
            "deadLoanBeforeCopy",
            &[
                "init e",
                "init h.v",
                "borrow e -> h.r",
                "init h",
                "borrow h.v -> k",
                "copy h -> h",
                "call take2",
                "export take2(h)",
                "cleanup [end k, end h.r, end h.v, end h, end e]",
            ],
        ),
    ],
};

/// Probe ci-0xlg P7, P8, P9, P10 and P17, and the C19 valid twin `deadLoanBeforeCopy`. `Own2`
/// copies, and a copy that is only read, or whose original is not written while the copy is
/// live, is valid: `Checked`. The sequences assert `copy h`, never `move h`. C19 rejected all
/// of these (R0203) and was wrong to (RFC 0007 lines 107, 113 and 190). Checked is not a
/// soundness claim for the unanalysed routes in `calls_returning_a_reference_bearing_record`.
#[test]
fn a_copy_of_a_self_borrowing_record_is_valid_when_nothing_writes_or_escapes() {
    check(
        "rfc_cases::o07_self_borrow::a_copy_of_a_self_borrowing_record_is_valid_when_nothing_writes_or_escapes",
        &COPIES,
    );
}

const MOVE_OK: Form = Form {
    text: r#"
type OwnS = { v: i32; s: string; r: &i32 };
function takeS(o: OwnS): i32 { return 0i32; }
function p14MoveAfterLoanDied(): i32 {
  let e = 5i32; let mut h = OwnS { v: 1i32, s: "a", r: &e }; let k = &h.v; return takeS(h);
}
"#,
    typed_stats: (326036, 47952, 12),
    graph: (GraphCompletion::Incomplete, (189159, 4795, 5)),
    roots: &[Root {
        name: "p14MoveAfterLoanDied",
        want: Want::Checked,
        graph: None,
        stats: (5793, 722, 5),
    }],
    sequences: &[(
        "p14MoveAfterLoanDied",
        &[
            "init e",
            "init h.v",
            "init h.s",
            "borrow e -> h.r",
            "init h",
            "borrow h.v -> k",
            "move h -> h",
            "call takeS",
            "export takeS(h)",
            "cleanup [end k, end e]",
        ],
    )],
};

/// Probe ci-0xlg P14: `OwnS` moves into `takeS(h)` after the only separate loan `k = &h.v`
/// is dead. `Checked`, and the sequence asserts `move h`.
#[test]
fn a_move_of_a_self_borrowing_record_is_valid_when_the_other_loan_is_dead() {
    check(
        "rfc_cases::o07_self_borrow::a_move_of_a_self_borrowing_record_is_valid_when_the_other_loan_is_dead",
        &MOVE_OK,
    );
}

const UNANALYSED: Form = Form {
    text: r#"
type Own2 = { v: i32; r: &i32 };
type Box1 = { r: &i32 };
function pass(o: Own2): Own2 { return o; }
function store(o: Own2): Box1 { return Box1 { r: o.r }; }
function p4ReturnThroughPass(): Own2 {
  let e = 5i32; let mut h = Own2 { v: 1i32, r: &e }; h.r = &h.v; return pass(h);
}
function p5WriteAfterPass(): i32 {
  let e = 5i32; let mut h = Own2 { v: 1i32, r: &e }; h.r = &h.v; let g = pass(h); h.v = 3i32; return *g.r;
}
function p16StoreIntoBox(): Box1 {
  let e = 5i32; let mut h = Own2 { v: 1i32, r: &e }; h.r = &h.v; return store(h);
}
"#,
    typed_stats: (333980, 48973, 12),
    graph: (GraphCompletion::Incomplete, (195006, 5054, 5)),
    roots: &[
        Root {
            name: "p4ReturnThroughPass",
            want: Want::Incomplete {
                blocker: r####"pass(h)"####,
            },
            graph: None,
            stats: (2727, 218, 3),
        },
        Root {
            name: "p5WriteAfterPass",
            want: Want::Incomplete {
                blocker: r####"pass(h)"####,
            },
            graph: None,
            stats: (2764, 228, 3),
        },
        Root {
            name: "p16StoreIntoBox",
            want: Want::Incomplete {
                blocker: r####"store(h)"####,
            },
            graph: None,
            stats: (2724, 217, 3),
        },
    ],
    sequences: &[],
};

/// Probe ci-0xlg P4, P5 and P16. A call that returns a reference-bearing record (`pass(h)`,
/// `store(h)`) stops `Incomplete` at the call, in the targeted query and in the ordinary graph
/// root. These routes are **unanalysed**. They are not accepted and not rejected, and are not
/// evidence of soundness. Found bead: "Calls that return a reference-bearing record stop
/// Incomplete (pass(h), store(h))".
#[test]
fn calls_returning_a_reference_bearing_record_stop_incomplete_and_are_unanalysed() {
    check(
        "rfc_cases::o07_self_borrow::calls_returning_a_reference_bearing_record_stop_incomplete_and_are_unanalysed",
        &UNANALYSED,
    );
}

const MEMBERS: Form = Form {
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
    sequences: &[
        (
            "replaceOwn",
            &[
                "init a",
                "init e",
                "init h.v",
                "borrow e -> h.r",
                "borrow mut a -> h.m",
                "init h",
                "borrow h.v -> &h.v",
                "replace h.r <- &h.v",
                "init 0i32",
                "export 0i32",
                "cleanup [end h.m, end h.r, end h.v, end h, end e, end a]",
            ],
        ),
        (
            "replaceOwn2",
            &[
                "init e",
                "init h.v",
                "borrow e -> h.r",
                "init h",
                "borrow h.v -> &h.v",
                "replace h.r <- &h.v",
                "init 0i32",
                "export 0i32",
                "cleanup [end h.r, end h.v, end h, end e]",
            ],
        ),
    ],
};

/// Ruling 21 (2), 2C-F2. The member-replacement check counts a loan only when it goes through
/// a sibling **reference** member. `Own` (`h.r = &h.v;` next to an `&mut` member) and `Own2`
/// are `Checked`; `OwnM` (`h.m = &mut h.v;`) is `Incomplete` at its body, its `d87e5dd2`
/// outcome. The twins reject while `h.r` is live: writing `h.v` (R0203 at the write), and
/// moving `h` by `return h;`, a call or a `let` (R0203 at `h`, related `&h.v`; `Own` moves
/// because of its `&mut` member). The `&mut` self-borrow followed by a move, `ownMTake` and
/// `ownMutMember`, rejects R0203 at the move. With the `reference_member` condition removed,
/// `replaceOwn` and `replaceOwnM` (and every root after them) give R0201 at the replacement.
#[test]
fn the_member_replacement_check_covers_only_sibling_reference_members() {
    check(
        "rfc_cases::o07_self_borrow::the_member_replacement_check_covers_only_sibling_reference_members",
        &MEMBERS,
    );
}

const PATHS: Form = Form {
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
    sequences: &[],
};

/// C19's fifteen move-path pins, re-measured at the base: each path stops `Incomplete` before
/// the whole-record use can be judged, at the same blocker and with the same caps as at
/// C19's fix. They are unanalysed routes, not acceptances and not evidence of soundness.
#[test]
fn other_whole_value_paths_of_a_self_borrowing_record_stay_incomplete() {
    check(
        "rfc_cases::o07_self_borrow::other_whole_value_paths_of_a_self_borrowing_record_stay_incomplete",
        &PATHS,
    );
}

/// A source the typed analysis stops on, before any ownership judgment: no receipt,
/// so no ownership query can run.
struct TypedForm {
    text: &'static str,
    signals: &'static [&'static str],
    stats: Triple,
}

fn check_typed_incomplete(test: &str, forms: &[&TypedForm]) {
    installed(test, |request| {
        for form in forms {
            fs::write(&request.entry, format!("{IMPORTS}\n{}", form.text)).unwrap();
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
                    form.signals,
                    form.stats,
                );
            }
        }
    });
}

const STORE_INTO_CALLER_BOX: TypedForm = TypedForm {
    text: r#"
type Own2 = { v: i32; r: &i32 };
type Box1 = { r: &i32 };
function put(slot: &mut Box1, o: Own2): void { slot.r = o.r; }
function q1StoreIntoCallerBox(): i32 {
  let e = 5i32; let mut b = Box1 { r: &e }; let mut h = Own2 { v: 1i32, r: &e }; h.r = &h.v; put(&mut b, h); h.v = 3i32; return *b.r;
}
"#,
    signals: &[
        "pending Storage @ slot.r = o.r;",
        "pending Storage @ &mut b",
    ],
    stats: (328018, 48155, 12),
};

const STORE_INTO_CALLER_REFERENCE: TypedForm = TypedForm {
    text: r#"
type Own2 = { v: i32; r: &i32 };
function putRef(slot: &mut &i32, o: Own2): void { *slot = o.r; }
function q2StoreIntoCallerReference(): i32 {
  let e = 5i32; let mut b = &e; let mut h = Own2 { v: 1i32, r: &e }; h.r = &h.v; putRef(&mut b, h); h.v = 3i32; return *b;
}
"#,
    signals: &["pending Storage @ *slot = o.r;", "pending Storage @ &mut b"],
    stats: (326434, 48022, 12),
};

/// Ruling 25 condition 3, the probe ci-0xlg lacks: the copy `g` outliving `h`
/// through a callee that stores `g.r` into the caller's storage. Neither form
/// reaches ownership. The typed analysis stops on the store through the
/// `&mut` parameter and on `&mut b` (`pending Storage`), so there is no receipt
/// and no judgment: the route is **unanalysed**, not accepted and not rejected,
/// and no evidence of soundness. Both forms are listed under the found bead
/// "Calls that return a reference-bearing record stop Incomplete (pass(h),
/// store(h))" with P4, P5 and P16. (Storing through a `List<&i32>` is not
/// expressible: `push` is a `MissingField`.)
#[test]
fn a_callee_storing_a_copys_reference_into_caller_storage_stops_typed_incomplete() {
    check_typed_incomplete(
        "rfc_cases::o07_self_borrow::a_callee_storing_a_copys_reference_into_caller_storage_stops_typed_incomplete",
        &[&STORE_INTO_CALLER_BOX, &STORE_INTO_CALLER_REFERENCE],
    );
}

const DISPOSE_SELF_BORROW: TypedForm = TypedForm {
    text: r#"
type Own2 = { v: i32; r: &i32 };
function p19DisposeTheOriginal(): i32 {
  let e = 5i32; let mut h = Own2 { v: 1i32, r: &e }; h.r = &h.v; let g = h; dispose(h); return *g.r;
}
"#,
    signals: &[
        "obligation FamilySelection @ dispose(h)",
        "pending TypedInput @ dispose(h)",
    ],
    stats: (325474, 47894, 12),
};

const DISPOSE_PLAIN: TypedForm = TypedForm {
    text: r#"
type Plain = { v: i32 };
function disposePlain(): i32 {
  let h = Plain { v: 1i32 }; dispose(h); return 0i32;
}
"#,
    signals: &[
        "obligation FamilySelection @ dispose(h)",
        "pending TypedInput @ dispose(h)",
    ],
    stats: (323897, 47604, 12),
};

/// Probe ci-0xlg P19. `dispose(h)` of any record, with or without a self-borrow
/// (`Plain = { v: i32 }` is the control), stops the typed analysis at the call
/// (`FamilySelection`, `TypedInput`): no receipt, so the ownership analysis
/// never sees the explicit dispose of a record. Unanalysed, and no evidence of
/// soundness. Found bead: "dispose of a record has no typed receipt".
#[test]
fn dispose_of_a_record_has_no_typed_receipt() {
    check_typed_incomplete(
        "rfc_cases::o07_self_borrow::dispose_of_a_record_has_no_typed_receipt",
        &[&DISPOSE_SELF_BORROW, &DISPOSE_PLAIN],
    );
}

const ARROW_CAPTURE: TypedForm = TypedForm {
    text: r#"
type Own2 = { v: i32; r: &i32 };
function captured(): i32 {
  let e = 5i32;
  let mut h = Own2 { v: 1i32, r: &e };
  h.r = &h.v;
  let f: () => Own2 = () => h;
  return 0i32;
}
"#,
    signals: &["pending Containment @ () => h"],
    stats: (324232, 47806, 12),
};

/// An arrow that captures the record stops in the typed analysis, before any
/// ownership judgment (C19's move path "arrow"). Unanalysed, not an acceptance.
#[test]
fn an_arrow_capturing_a_self_borrowing_record_stays_typed_incomplete() {
    check_typed_incomplete(
        "rfc_cases::o07_self_borrow::an_arrow_capturing_a_self_borrowing_record_stays_typed_incomplete",
        &[&ARROW_CAPTURE],
    );
}

/// Metering rule 11: the member-replacement roots of `MEMBERS` that rejected
/// R0201 at the replacement at the parent (`b082e6ac`) now run on (`Checked`,
/// `Incomplete`, or to the move or write that rejects), which costs more, so
/// the parent's caps give `ResourceLimit`. `ruling 21 (2)` approved the move.
#[test]
fn member_replacement_parent_caps_now_give_resource_limit() {
    super::o07::at_parent_caps(
        "rfc_cases::o07_self_borrow::member_replacement_parent_caps_now_give_resource_limit",
        MEMBERS.text,
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
        super::o07::Parent::Exceeds,
    );
}
