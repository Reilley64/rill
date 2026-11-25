//! RFC 0007 O7 (ri-pfxi, finish pass ci-81k1): the chief's conditions on the
//! `&mut i32` record-member admission (`chief-rulings-9.md`, 2026-11-24).
//! Each admitted form is pinned by name with its whole MIR sequence, and
//! gets a rejecting twin where one is expressible. The tuple and the
//! nested-reference shapes are pinned `Incomplete`, so a later change
//! cannot admit them silently.
use super::*;
use compiler::analysis::ownership::{Cleanup, Mir, Operation};

pub(super) struct Form {
    pub(super) text: &'static str,
    pub(super) typed_stats: Triple,
    pub(super) graph: (GraphCompletion, Triple),
    pub(super) roots: &'static [Root],
    /// The whole MIR of a `Checked` root, in order (rule 9).
    pub(super) sequences: &'static [(&'static str, &'static [&'static str])],
}

fn place(place: &Place<'_>) -> String {
    let mut name = text(&place.root);
    match place.projection {
        None => {}
        Some(_) => match place.field() {
            Some(field) => name = format!("{name}.{}", field.name.as_deref().unwrap_or("?")),
            None => panic!("a projection this test does not pin: {place:?}"),
        },
    }
    name
}

fn cleanup(order: &[Cleanup<'_>]) -> String {
    let order: Vec<_> = order
        .iter()
        .map(|obligation| match obligation {
            Cleanup::EndStorage(at) => format!("end {}", place(at)),
            Cleanup::Drop(at) => format!("drop {}", place(at)),
            other => panic!("a cleanup this test does not pin: {other:?}"),
        })
        .collect();
    format!("cleanup [{}]", order.join(", "))
}

/// Every operation of a straight-line body in order. `Operation::None` is the
/// join filler between statements and carries no action, so it is left out.
fn sequence(module: &loader::Module, mir: &Mir<'_>) -> Vec<String> {
    let mut found = Vec::new();
    for operation in linear(mir, mir.entry) {
        found.push(match operation {
            Operation::None => continue,
            Operation::Initialize(at) => format!("init {}", place(at)),
            Operation::Copy { from, into } => {
                format!("copy {} -> {}", place(from), place(&into.unwrap()))
            }
            Operation::Move { from, into } => {
                format!("move {} -> {}", place(from), place(&into.unwrap()))
            }
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
            Operation::Dereference(at) => format!("deref {}", place(at)),
            Operation::CheckedI32Add { left, right, into } => {
                format!("add {} {} -> {}", place(left), place(right), place(into))
            }
            Operation::InstallThrough {
                descriptor,
                incoming,
            } => format!("install {} <- {}", place(descriptor), place(incoming)),
            Operation::Install { target, incoming } => {
                format!("replace {} <- {}", place(target), place(incoming))
            }
            Operation::Call(callee) => {
                let callee = module.declarations.iter().find(|d| &d.id == *callee);
                format!("call {}", callee.unwrap().name.as_deref().unwrap())
            }
            Operation::Export(at) => format!("export {}", place(at)),
            Operation::Cleanup(order) => cleanup(order),
            other => panic!("an operation this test does not pin: {other:?}"),
        });
    }
    found
}

pub(super) fn check(test: &str, form: &Form) {
    installed(test, |request| {
        fs::write(&request.entry, format!("{IMPORTS}\n{}", form.text)).unwrap();
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
                statistics(form.typed_stats),
                "{context}"
            );
            let module = graph
                .modules()
                .iter()
                .find(|m| m.key.origin == Origin::Application && m.key.path == "main.rill")
                .unwrap();
            check_receipt(&context, &typed, module, form.graph, form.roots);
            let receipt = typed.receipt().unwrap();
            for (name, expected) in form.sequences {
                let declaration = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                let body = lower_body(&receipt, module, &declaration.id, AnalysisLimits::default())
                    .unwrap();
                assert_eq!(
                    sequence(module, body.mir().unwrap()),
                    *expected,
                    "{context}, {name}"
                );
            }
            // Every `Checked` root has its sequence pinned.
            let checked = form
                .roots
                .iter()
                .filter(|r| matches!(r.want, Want::Checked))
                .count();
            assert_eq!(checked, form.sequences.len(), "{context}");
        }
    });
}

/// Condition 1, `withText`: a record with a `string` member next to the
/// `&mut i32` member. The write goes through the member's descriptor; the
/// record drops its text and ends the member's loan before `a` ends. Condition
/// 2: with a shared member borrowing the same owner, the construction is
/// R0201 at the second borrow, related the first.
const WITH_TEXT: Form = Form {
    text: r#"
type Named = { first: &mut i32; name: string };
type Named2 = { first: &mut i32; second: &i32; name: string };
function withText(): i32 {
  let mut a = 1i32;
  let mut holder = Named { first: &mut a, name: "x" };
  *holder.first += 1i32;
  return 0i32;
}
function withTextOverlap(): i32 {
  let mut a = 1i32;
  let holder = Named2 { first: &mut a, second: &a, name: "x" };
  return 0i32;
}
"#,
    typed_stats: (450377, 48836, 12),
    graph: (GraphCompletion::SourceRejected, (192235, 4774, 5)),
    roots: &[
        Root {
            name: "withText",
            want: Want::Checked,
            graph: None,
            stats: (3179, 486, 5),
        },
        Root {
            name: "withTextOverlap",
            want: Want::Rejected {
                code: "R0201",
                site: r####"&a"####,
                related: r####"&mut a"####,
            },
            graph: None,
            stats: (2660, 225, 5),
        },
    ],
    sequences: &[(
        "withText",
        &[
            "init a",
            "borrow mut a -> holder.first",
            "init holder.name",
            "init holder",
            "deref holder.first",
            "init 1i32",
            "add *holder.first 1i32 -> *holder.first += 1i32;",
            "install holder.first <- *holder.first += 1i32;",
            "init 0i32",
            "export 0i32",
            "cleanup [drop holder.name, end holder.first, end holder, end a]",
        ],
    )],
};

#[test]
fn with_text_record_pins_its_sequence_and_the_overlap_twin() {
    check(
        "rfc_cases::o07_forms::with_text_record_pins_its_sequence_and_the_overlap_twin",
        &WITH_TEXT,
    );
}

/// Condition 1, the record parameter: `readFirst` and `readSecond` read the
/// exclusive and the shared member through the parameter; `forward` and
/// `callRead` pass the record to a call, which moves it. Condition 2: a use of
/// the exclusive member's source while the record is live rejects. A read or a
/// borrow of `a` conflicts with the exclusive loan (R0201, the access rule);
/// an assignment to `a` is R0203, the rule for replacing a borrowed source, as
/// for any live loan. The site is the use, the related site the loan.
const PARAMETER: Form = Form {
    text: r#"
type Holder = { first: &mut i32; second: &i32 };
function readFirst(h: Holder): i32 { return *h.first; }
function readSecond(h: Holder): i32 { return *h.second; }
function forward(h: Holder): i32 { return readFirst(h); }
function callRead(): i32 {
  let mut a = 1i32;
  let b = 2i32;
  let h = Holder { first: &mut a, second: &b };
  return readFirst(h);
}
function readOwnerWhileLive(): i32 {
  let mut a = 1i32;
  let b = 2i32;
  let h = Holder { first: &mut a, second: &b };
  let seen = a;
  return readFirst(h);
}
function borrowOwnerWhileLive(): i32 {
  let mut a = 1i32;
  let b = 2i32;
  let h = Holder { first: &mut a, second: &b };
  let view = &a;
  return readFirst(h);
}
function writeOwnerWhileLive(): i32 {
  let mut a = 1i32;
  let b = 2i32;
  let h = Holder { first: &mut a, second: &b };
  a = 5i32;
  return readFirst(h);
}
"#,
    typed_stats: (460489, 49871, 12),
    graph: (GraphCompletion::SourceRejected, (203479, 6361, 5)),
    roots: &[
        Root {
            name: "readFirst",
            want: Want::Checked,
            graph: None,
            stats: (2681, 236, 3),
        },
        Root {
            name: "readSecond",
            want: Want::Checked,
            graph: None,
            stats: (2682, 236, 3),
        },
        Root {
            name: "forward",
            want: Want::Checked,
            graph: None,
            stats: (5991, 585, 5),
        },
        Root {
            name: "callRead",
            want: Want::Checked,
            graph: None,
            stats: (6375, 771, 5),
        },
        Root {
            name: "readOwnerWhileLive",
            want: Want::Rejected {
                code: "R0201",
                site: r####"a"####,
                related: r####"&mut a"####,
            },
            graph: None,
            stats: (3460, 476, 5),
        },
        Root {
            name: "borrowOwnerWhileLive",
            want: Want::Rejected {
                code: "R0201",
                site: r####"&a"####,
                related: r####"&mut a"####,
            },
            graph: None,
            stats: (3462, 476, 5),
        },
        Root {
            name: "writeOwnerWhileLive",
            want: Want::Rejected {
                code: "R0203",
                site: r####"a = 5i32;"####,
                related: r####"&mut a"####,
            },
            graph: None,
            stats: (3519, 511, 5),
        },
    ],
    sequences: &[
        (
            "readFirst",
            &[
                "deref h: Holder.first",
                "export *h.first",
                "cleanup [end h: Holder.second, end h: Holder.first, end h: Holder]",
            ],
        ),
        (
            "readSecond",
            &[
                "deref h: Holder.second",
                "export *h.second",
                "cleanup [end h: Holder.second, end h: Holder.first, end h: Holder]",
            ],
        ),
        (
            "forward",
            &[
                "move h: Holder -> h",
                "call readFirst",
                "export readFirst(h)",
                "cleanup []",
            ],
        ),
        (
            "callRead",
            &[
                "init a",
                "init b",
                "borrow mut a -> h.first",
                "borrow b -> h.second",
                "init h",
                "move h -> h",
                "call readFirst",
                "export readFirst(h)",
                "cleanup [end b, end a]",
            ],
        ),
    ],
};

#[test]
fn record_parameter_pins_its_sequences_and_the_source_use_twins() {
    check(
        "rfc_cases::o07_forms::record_parameter_pins_its_sequences_and_the_source_use_twins",
        &PARAMETER,
    );
}

/// Condition 1, the returning function: `make` moves the exclusive parameter
/// into the member and copies the shared one. Its caller `useMake` stays
/// `Incomplete` at the call. Condition 2: returning a record whose `&mut`
/// member borrows a body-local is R0202 (the loan would outlive the body), as
/// a direct return and through a binding. The site is the return, the related
/// site the local.
const RETURNING: Form = Form {
    text: r#"
type Holder = { first: &mut i32; second: &i32 };
function make(p: &mut i32, q: &i32): Holder { return Holder { first: p, second: q }; }
function useMake(): i32 {
  let mut a = 1i32;
  let b = 2i32;
  let h = make(&mut a, &b);
  return 0i32;
}
function makeLocalMut(q: &i32): Holder {
  let mut a = 1i32;
  return Holder { first: &mut a, second: q };
}
function makeLocalBound(q: &i32): Holder {
  let mut a = 1i32;
  let h = Holder { first: &mut a, second: q };
  return h;
}
"#,
    typed_stats: (452657, 49054, 12),
    graph: (GraphCompletion::SourceRejected, (194681, 4961, 5)),
    roots: &[
        Root {
            name: "make",
            want: Want::Checked,
            graph: None,
            stats: (2637, 247, 4),
        },
        Root {
            name: "useMake",
            want: Want::Incomplete {
                blocker: r####"make(&mut a, &b)"####,
            },
            graph: None,
            stats: (2412, 136, 3),
        },
        Root {
            name: "makeLocalMut",
            want: Want::Rejected {
                code: "R0202",
                site: r####"return Holder { first: &mut a, second: q };"####,
                related: r####"a"####,
            },
            graph: None,
            stats: (2631, 252, 5),
        },
        Root {
            name: "makeLocalBound",
            want: Want::Rejected {
                code: "R0202",
                site: r####"return h;"####,
                related: r####"a"####,
            },
            graph: None,
            stats: (3267, 376, 5),
        },
    ],
    sequences: &[(
        "make",
        &[
            "move p: &mut i32 -> Holder { first: p, second: q }.first",
            "copy q: &i32 -> Holder { first: p, second: q }.second",
            "init Holder { first: p, second: q }",
            "export Holder { first: p, second: q }",
            "cleanup [end q: &i32]",
        ],
    )],
};

#[test]
fn returning_function_pins_its_sequence_its_caller_and_the_local_twins() {
    check(
        "rfc_cases::o07_forms::returning_function_pins_its_sequence_its_caller_and_the_local_twins",
        &RETURNING,
    );
}

/// Condition 3: the nested shapes stay `Incomplete` at the construction. A
/// reference to a reference member (`&&i32`, `&mut &mut i32`) and a record
/// whose member is a record with an `&mut` member, bound or written in place.
/// Measured with the admission in place.
const NESTED: Form = Form {
    text: r#"
type DeepShared = { top: &&i32 };
type Deep = { top: &mut &mut i32 };
type Holder = { first: &mut i32; second: &i32 };
type Wrap = { inner: Holder; n: i32 };
function refToShared(): i32 {
  let a = 1i32;
  let r = &a;
  let rr = &r;
  let h = DeepShared { top: rr };
  return 0i32;
}
function refToRef(): i32 {
  let mut a = 1i32;
  let mut r = &mut a;
  let rr = &mut r;
  let h = Deep { top: rr };
  return 0i32;
}
function nestedHolder(): i32 {
  let mut a = 1i32;
  let b = 2i32;
  let inner = Holder { first: &mut a, second: &b };
  let outer = Wrap { inner: inner, n: 1i32 };
  return 0i32;
}
function nestedLiteral(): i32 {
  let mut a = 1i32;
  let b = 2i32;
  let outer = Wrap { inner: Holder { first: &mut a, second: &b }, n: 1i32 };
  return 0i32;
}
"#,
    typed_stats: (456831, 49631, 12),
    graph: (GraphCompletion::Incomplete, (195889, 4517, 5)),
    roots: &[
        Root {
            name: "refToShared",
            want: Want::Incomplete {
                blocker: r####"h"####,
            },
            graph: None,
            stats: (2496, 146, 1),
        },
        Root {
            name: "refToRef",
            want: Want::Incomplete {
                blocker: r####"h"####,
            },
            graph: None,
            stats: (2496, 146, 1),
        },
        Root {
            name: "nestedHolder",
            want: Want::Incomplete {
                blocker: r####"outer"####,
            },
            graph: None,
            stats: (3065, 204, 1),
        },
        Root {
            name: "nestedLiteral",
            want: Want::Incomplete {
                blocker: r####"outer"####,
            },
            graph: None,
            stats: (2562, 142, 1),
        },
    ],
    sequences: &[],
};

#[test]
fn nested_reference_shapes_stay_incomplete() {
    check(
        "rfc_cases::o07_forms::nested_reference_shapes_stay_incomplete",
        &NESTED,
    );
}

/// Condition 3, the tuple: a tuple with an `&mut` element is typed
/// `Incomplete` (R0100 on the shared element, the pending input and storage at
/// the tuple), so ownership never receives it. The typed caps are exact and
/// one short.
#[test]
fn tuple_with_an_exclusive_element_stays_typed_incomplete() {
    installed(
        "rfc_cases::o07_forms::tuple_with_an_exclusive_element_stays_typed_incomplete",
        |request| {
            fs::write(
                &request.entry,
                format!(
                    "{IMPORTS}\n{}",
                    r#"
function tupleForm(): i32 {
  let mut a = 1i32;
  let b = 2i32;
  let t = [&mut a, &b];
  return 0i32;
}
"#
                ),
            )
            .unwrap();
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
                    &[
                        r####"finding R0100 @ &b"####,
                        r####"pending TypedInput @ [&mut a, &b]"####,
                        r####"pending Storage @ let t = [&mut a, &b];"####,
                    ],
                    (444142, 48149, 12),
                );
            }
        },
    );
}

/// Review ci-0mdr SP-3 and Q3: member replacement, `holder.y = &mut c;`. The
/// store stages the new loan, replaces the member and leaves the others as
/// they were. `replaceMember` and `replaceNoUse` replace with a loan of a third
/// owner (`replaceNoUse` is the valid twin of the overlap that
/// `o07::member_replacement_overlap_without_later_use` rejects, with no later
/// use). `sameOwnerAgain` replaces a member with a loan of the owner it
/// already borrows: the store releases the old loan, so it does not overlap
/// itself. `sharedReplace` replaces a shared member. Rejecting twins:
/// `overlapThenUse` is R0201 at the new `&mut a`, related the loan of `a` in
/// the construction; `localReferent` is R0203 at the block that ends `c` while
/// `holder.y` still reads it, related the borrow `&mut c`.
const MEMBER_REPLACEMENT: Form = Form {
    text: r#"
type Pair = { x: &mut i32; y: &mut i32 };
type Holder = { first: &mut i32; second: &i32 };
function replaceMember(): i32 {
  let mut a = 1i32;
  let mut b = 2i32;
  let mut c = 3i32;
  let mut holder = Pair { x: &mut a, y: &mut b };
  holder.y = &mut c;
  *holder.x += 1i32;
  return 0i32;
}
function replaceNoUse(): i32 {
  let mut a = 1i32;
  let mut b = 2i32;
  let mut c = 3i32;
  let mut holder = Pair { x: &mut a, y: &mut b };
  holder.y = &mut c;
  return 0i32;
}
function sameOwnerAgain(): i32 {
  let mut a = 1i32;
  let mut b = 2i32;
  let mut holder = Pair { x: &mut a, y: &mut b };
  holder.y = &mut b;
  *holder.y += 1i32;
  return 0i32;
}
function sharedReplace(): i32 {
  let mut a = 1i32;
  let b = 2i32;
  let c = 3i32;
  let mut holder = Holder { first: &mut a, second: &b };
  holder.second = &c;
  *holder.first += 1i32;
  return *holder.second;
}
function overlapThenUse(): i32 {
  let mut a = 1i32;
  let mut b = 2i32;
  let mut holder = Pair { x: &mut a, y: &mut b };
  holder.y = &mut a;
  *holder.x += 1i32;
  return 0i32;
}
function localReferent(): i32 {
  let mut a = 1i32;
  let mut b = 2i32;
  let mut holder = Pair { x: &mut a, y: &mut b };
  if (true) { let mut c = 3i32; holder.y = &mut c; }
  *holder.y += 1i32;
  return 0i32;
}
"#,
    typed_stats: (463534, 50955, 12),
    graph: (GraphCompletion::SourceRejected, (208031, 7771, 6)),
    roots: &[
        Root {
            name: "replaceMember",
            want: Want::Checked,
            graph: None,
            stats: (3987, 884, 5),
        },
        Root {
            name: "replaceNoUse",
            want: Want::Checked,
            graph: None,
            stats: (3436, 602, 5),
        },
        Root {
            name: "sameOwnerAgain",
            want: Want::Checked,
            graph: None,
            stats: (3843, 805, 5),
        },
        Root {
            name: "sharedReplace",
            want: Want::Checked,
            graph: None,
            stats: (4059, 898, 5),
        },
        Root {
            name: "overlapThenUse",
            want: Want::Rejected {
                code: "R0201",
                site: r####"&mut a"####,
                related: r####"&mut a"####,
            },
            graph: None,
            stats: (3273, 530, 5),
        },
        Root {
            name: "localReferent",
            want: Want::Rejected {
                code: "R0203",
                site: r####"{ let mut c = 3i32; holder.y = &mut c; }"####,
                related: r####"&mut c"####,
            },
            graph: None,
            stats: (3644, 735, 6),
        },
    ],
    sequences: &[
        (
            "replaceMember",
            &[
                "init a",
                "init b",
                "init c",
                "borrow mut a -> holder.x",
                "borrow mut b -> holder.y",
                "init holder",
                "borrow mut c -> &mut c",
                "replace holder.y <- &mut c",
                "deref holder.x",
                "init 1i32",
                "add *holder.x 1i32 -> *holder.x += 1i32;",
                "install holder.x <- *holder.x += 1i32;",
                "init 0i32",
                "export 0i32",
                "cleanup [end holder.y, end holder.x, end holder, end c, end b, end a]",
            ],
        ),
        (
            "replaceNoUse",
            &[
                "init a",
                "init b",
                "init c",
                "borrow mut a -> holder.x",
                "borrow mut b -> holder.y",
                "init holder",
                "borrow mut c -> &mut c",
                "replace holder.y <- &mut c",
                "init 0i32",
                "export 0i32",
                "cleanup [end holder.y, end holder.x, end holder, end c, end b, end a]",
            ],
        ),
        (
            "sameOwnerAgain",
            &[
                "init a",
                "init b",
                "borrow mut a -> holder.x",
                "borrow mut b -> holder.y",
                "init holder",
                "borrow mut b -> &mut b",
                "replace holder.y <- &mut b",
                "deref holder.y",
                "init 1i32",
                "add *holder.y 1i32 -> *holder.y += 1i32;",
                "install holder.y <- *holder.y += 1i32;",
                "init 0i32",
                "export 0i32",
                "cleanup [end holder.y, end holder.x, end holder, end b, end a]",
            ],
        ),
        (
            "sharedReplace",
            &[
                "init a",
                "init b",
                "init c",
                "borrow mut a -> holder.first",
                "borrow b -> holder.second",
                "init holder",
                "borrow c -> &c",
                "replace holder.second <- &c",
                "deref holder.first",
                "init 1i32",
                "add *holder.first 1i32 -> *holder.first += 1i32;",
                "install holder.first <- *holder.first += 1i32;",
                "deref holder.second",
                "export *holder.second",
                "cleanup [end holder.second, end holder.first, end holder, end c, end b, end a]",
            ],
        ),
    ],
};

/// Review ci-b4b2 C-4 (ruling 9 condition 1), the admitted forms the first
/// fix pass measured and did not pin. `exclusiveOverlap` replaces the
/// exclusive member with a loan of the owner it already borrows while the
/// shared member borrows another: the store releases the old loan, so it checks.
/// `blockLocalNoUse` replaces a member with a loan of a block-local owner that
/// ends with the block while nothing reads the member again, so it checks
/// (`localReferent` reads it and is R0203). `recordUpdateOverlap` is the
/// record-update root of the same overlap, `Pair { ...holder, y: &mut a }`
/// over `x: &mut a`: the update is not lowered, so the root is `Incomplete` at
/// the update and the graph with it, never `Checked`.
const MEMBER_REPLACEMENT_SIBLINGS: Form = Form {
    text: r#"
type Pair = { x: &mut i32; y: &mut i32 };
type Holder = { first: &mut i32; second: &i32 };
function exclusiveOverlap(): i32 {
  let mut a = 1i32;
  let b = 2i32;
  let mut holder = Holder { first: &mut a, second: &b };
  holder.first = &mut a;
  return 0i32;
}
function blockLocalNoUse(): i32 {
  let mut a = 1i32;
  let mut b = 2i32;
  let mut holder = Pair { x: &mut a, y: &mut b };
  if (true) { let mut c = 3i32; holder.y = &mut c; }
  return 0i32;
}
function recordUpdateOverlap(): i32 {
  let mut a = 1i32;
  let mut b = 2i32;
  let mut holder = Pair { x: &mut a, y: &mut b };
  let next = Pair { ...holder, y: &mut a };
  return 0i32;
}
"#,
    typed_stats: (453351, 49419, 12),
    graph: (GraphCompletion::Incomplete, (196034, 5221, 6)),
    roots: &[
        Root {
            name: "exclusiveOverlap",
            want: Want::Checked,
            graph: None,
            stats: (3142, 452, 5),
        },
        Root {
            name: "blockLocalNoUse",
            want: Want::Checked,
            graph: None,
            stats: (3385, 589, 6),
        },
        Root {
            name: "recordUpdateOverlap",
            want: Want::Incomplete {
                blocker: r####"Pair { ...holder, y: &mut a }"####,
            },
            graph: None,
            stats: (3023, 217, 3),
        },
    ],
    sequences: &[
        (
            "exclusiveOverlap",
            &[
                "init a",
                "init b",
                "borrow mut a -> holder.first",
                "borrow b -> holder.second",
                "init holder",
                "borrow mut a -> &mut a",
                "replace holder.first <- &mut a",
                "init 0i32",
                "export 0i32",
                "cleanup [end holder.second, end holder.first, end holder, end b, end a]",
            ],
        ),
        (
            "blockLocalNoUse",
            &[
                "init a",
                "init b",
                "borrow mut a -> holder.x",
                "borrow mut b -> holder.y",
                "init holder",
                "init c",
                "borrow mut c -> &mut c",
                "replace holder.y <- &mut c",
                "cleanup [end c]",
                "init 0i32",
                "export 0i32",
                "cleanup [end holder.y, end holder.x, end holder, end b, end a]",
            ],
        ),
    ],
};

#[test]
fn member_replacement_pins_its_sequence_and_twins() {
    check(
        "rfc_cases::o07_forms::member_replacement_pins_its_sequence_and_twins",
        &MEMBER_REPLACEMENT,
    );
    check(
        "rfc_cases::o07_forms::member_replacement_pins_its_sequence_and_twins",
        &MEMBER_REPLACEMENT_SIBLINGS,
    );
}

/// Metering rule 11 (review ci-b4b2 D-F1, chief rulings 11 Q3 and 18): the
/// replacement now checks that the incoming value holds no loan on a sibling
/// member (`member_replacement_overlap` in `flow.rs`), one work unit for the
/// one loan a stored reference holds. Each root that stores a member and
/// reaches the check costs one more than at `d87e5dd2`, and the whole graph of
/// `MEMBER_REPLACEMENT` five more; the old caps below now give
/// `ResourceLimit`. `overlapThenUse` rejects before the store and keeps its
/// cap; `recordUpdateOverlap` is not lowered and keeps it too.
#[test]
fn member_replacement_pre_sibling_check_caps_now_give_resource_limit() {
    let test =
        "rfc_cases::o07_forms::member_replacement_pre_sibling_check_caps_now_give_resource_limit";
    super::o07::at_parent_caps(
        test,
        MEMBER_REPLACEMENT.text,
        (204371, 7771, 6),
        &[
            ("replaceMember", (3834, 884, 5)),
            ("replaceNoUse", (3283, 602, 5)),
            ("sameOwnerAgain", (3691, 805, 5)),
            ("sharedReplace", (3906, 898, 5)),
            ("localReferent", (3491, 735, 6)),
        ],
        super::o07::Parent::Exceeds,
    );
    super::o07::at_parent_caps(
        test,
        MEMBER_REPLACEMENT_SIBLINGS.text,
        (192944, 5221, 6),
        &[
            ("exclusiveOverlap", (2990, 452, 5)),
            ("blockLocalNoUse", (3232, 589, 6)),
        ],
        super::o07::Parent::Exceeds,
    );
}

/// Metering rule 11 (review ci-0mdr Q3): the replacement keeps the record's
/// other members live, so each `MEMBER_REPLACEMENT` root costs a little more
/// than at the parent `da5bec50`, and the whole graph does too. The parent
/// caps below now give `ResourceLimit`.
#[test]
fn member_replacement_parent_caps_now_give_resource_limit() {
    super::o07::at_parent_caps(
        "rfc_cases::o07_forms::member_replacement_parent_caps_now_give_resource_limit",
        MEMBER_REPLACEMENT.text,
        (204349, 7769, 6),
        &[
            ("replaceMember", (3832, 884, 5)),
            ("replaceNoUse", (3271, 600, 5)),
            ("sameOwnerAgain", (3689, 805, 5)),
            ("sharedReplace", (3904, 898, 5)),
            ("overlapThenUse", (3120, 530, 5)),
            ("localReferent", (3489, 735, 6)),
        ],
        super::o07::Parent::Exceeds,
    );
}
