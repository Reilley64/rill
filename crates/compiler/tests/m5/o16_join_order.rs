//! RFC 0007 case O16's `joinedOrder` and its expectation (ri-e9t8): `&&` on two
//! i32 equalities, tuple-component operands (`pair[0]`) and an annotated
//! `List<i32>` literal of integer literals. These are compiler judgments over
//! MIR, not Rill execution.
//!
//! Every source loads in both installed modes. Each pinned cap is replayed at
//! its exact value and one short on each axis (`ResourceLimit`, no MIR, no
//! finding). The siblings keep the caps measured at the parent `7002855e`.
use super::*;
use compiler::analysis::Statistics;
use compiler::analysis::ownership::{
    GraphCompletion, GraphReport, Mir, Operation, Report, Terminator, analyze_ownership, lower_root,
};
use compiler::analysis::scalars::BodyOwner;
use compiler::loader::DeclarationKind;

const JOINED: &str = r#"
import std::routine::join;
function joined(input: List<i32>): [i32, i32] {
    let numbers = input;
    let [first, second] = join([() => (&numbers)[0u64], () => (&numbers)[1u64]]);
    dispose(numbers);
    return [first, second];
}
"#;

/// The approved RFC source after `joined`.
const ORDER: &str = r#"
function joinedOrder(): bool {
    let input: List<i32> = [13i32, 29i32];
    let pair = joined(input);
    return pair[0] == 13i32 && pair[1] == 29i32;
}
expect joinedOrder();
"#;

/// Metering rule 11, third case (ri-e9t8): these were the exact caps while the
/// roots stopped at the `&&`. They now give `ResourceLimit`.
const OLD_JOINED_ORDER: (usize, usize, usize) = (2392, 139, 3);
const OLD_EXPECT: (usize, usize, usize) = (4431, 219, 3);
const OLD_GRAPH: (usize, usize, usize) = (225554, 25180, 10);

struct Sibling {
    label: &'static str,
    source: &'static str,
    blocker: &'static str,
    /// Measured at 7002855e, not on this branch.
    targeted: (usize, usize, usize),
    graph: (usize, usize, usize),
}

struct Admitted {
    label: &'static str,
    source: &'static str,
    completion: &'static str,
    targeted: (usize, usize, usize),
    graph: (usize, usize, usize),
    /// The whole MIR of `f`, targeted and graph root alike: every block in
    /// index order as (operation and next block, site). Empty when rejected.
    sequence: &'static [(&'static str, &'static str)],
}

const SIBLINGS: &[Sibling] = &[
    Sibling {
        label: "or",
        source: r####"function f(): bool { let input: List<i32> = [13i32, 29i32]; let pair = joined(input); return pair[0] == 13i32 || pair[1] == 29i32; }"####,
        blocker: r####"pair[0] == 13i32 || pair[1] == 29i32"####,
        targeted: (2390, 137, 3),
        graph: (225281, 25149, 10),
    },
    Sibling {
        label: "and-true",
        source: r####"function f(): bool { let input: List<i32> = [13i32, 29i32]; let pair = joined(input); return pair[0] == 13i32 && true; }"####,
        blocker: r####"pair[0] == 13i32 && true"####,
        targeted: (2384, 134, 3),
        graph: (225095, 25146, 10),
    },
    Sibling {
        label: "not-equal",
        source: r####"function f(): bool { let input: List<i32> = [13i32, 29i32]; let pair = joined(input); return pair[0] != 13i32; }"####,
        blocker: r####"pair[0] != 13i32"####,
        targeted: (2380, 132, 3),
        graph: (224971, 25144, 10),
    },
    Sibling {
        label: "chain",
        source: r####"function f(): bool { let input: List<i32> = [13i32, 29i32]; let pair = joined(input); return pair[0] == 13i32 && pair[1] == 29i32 && pair[0] == 1i32; }"####,
        blocker: r####"pair[0] == 13i32 && pair[1] == 29i32 && pair[0] == 1i32"####,
        targeted: (2400, 142, 3),
        graph: (225591, 25154, 10),
    },
    Sibling {
        label: "list-index",
        source: r####"function f(input: List<i32>): bool { return (&input)[0u64] == 13i32; }"####,
        blocker: r####"(&input)[0u64] == 13i32"####,
        targeted: (1994, 55, 3),
        graph: (224284, 25067, 10),
    },
    Sibling {
        label: "list-index-and",
        source: r####"function f(input: List<i32>): bool { return (&input)[0u64] == 13i32 && (&input)[1u64] == 29i32; }"####,
        blocker: r####"(&input)[0u64] == 13i32 && (&input)[1u64] == 29i32"####,
        targeted: (2010, 63, 3),
        graph: (224780, 25075, 10),
    },
    Sibling {
        label: "while",
        source: r####"function f(pair: [i32, i32]): bool { while (pair[0] == 13i32 && pair[1] == 29i32) { } return true; }"####,
        blocker: r####"pair[0] == 13i32 && pair[1] == 29i32"####,
        targeted: (2362, 128, 4),
        graph: (225012, 25140, 10),
    },
    Sibling {
        label: "if",
        source: r####"function f(pair: [i32, i32]): bool { if (pair[0] == 13i32 && pair[1] == 29i32) { return true; } return false; }"####,
        blocker: r####"pair[0] == 13i32 && pair[1] == 29i32"####,
        targeted: (2373, 132, 5),
        graph: (225143, 25144, 10),
    },
    Sibling {
        label: "ref-pair",
        source: r####"function f(pair: &[i32, i32]): bool { return pair[0] == 13i32; }"####,
        blocker: r####"pair[0] == 13i32"####,
        targeted: (1973, 54, 3),
        graph: (224203, 25066, 10),
    },
    Sibling {
        label: "nested",
        source: r####"function f(pair: [[i32, i32], i32]): bool { return pair[0][1] == 13i32; }"####,
        blocker: r####"pair[0][1] == 13i32"####,
        targeted: (2727, 189, 3),
        graph: (225077, 25201, 10),
    },
    Sibling {
        label: "lit-empty",
        source: r####"function f(): bool { let input: List<i32> = []; dispose(input); return true; }"####,
        blocker: r####"[]"####,
        targeted: (2043, 63, 5),
        graph: (224333, 25075, 10),
    },
    Sibling {
        label: "lit-local",
        source: r####"function f(a: i32): bool { let input: List<i32> = [13i32, a]; dispose(input); return true; }"####,
        blocker: r####"[13i32, a]"####,
        targeted: (2056, 72, 5),
        graph: (224527, 25084, 10),
    },
    Sibling {
        label: "lit-sum",
        source: r####"function f(a: i32): bool { let input: List<i32> = [13i32, a + 1i32]; dispose(input); return true; }"####,
        blocker: r####"[13i32, a + 1i32]"####,
        targeted: (2060, 74, 5),
        graph: (224651, 25086, 10),
    },
    Sibling {
        label: "lit-grouped",
        source: r####"function f(): bool { let input: List<i32> = [(13i32), 29i32]; dispose(input); return true; }"####,
        blocker: r####"[(13i32), 29i32]"####,
        targeted: (2049, 66, 5),
        graph: (224519, 25078, 10),
    },
    Sibling {
        label: "lit-neg",
        source: r####"function f(): bool { let input: List<i32> = [-13i32, 29i32]; dispose(input); return true; }"####,
        blocker: r####"[-13i32, 29i32]"####,
        targeted: (2049, 66, 5),
        graph: (224519, 25078, 10),
    },
    Sibling {
        label: "lit-i64",
        source: r####"function f(): bool { let input: List<i64> = [13i64, 29i64]; dispose(input); return true; }"####,
        blocker: r####"input"####,
        targeted: (1966, 49, 1),
        graph: (224376, 25061, 10),
    },
    Sibling {
        label: "lit-bool",
        source: r####"function f(): bool { let input: List<bool> = [true, false]; dispose(input); return true; }"####,
        blocker: r####"input"####,
        targeted: (1966, 49, 1),
        graph: (224376, 25061, 10),
    },
    Sibling {
        label: "lit-text",
        source: r####"function f(): bool { let input: List<string> = ["a", "b"]; dispose(input); return true; }"####,
        blocker: r####"input"####,
        targeted: (1966, 49, 1),
        graph: (224376, 25061, 10),
    },
    Sibling {
        label: "lit-return",
        source: r####"function f(): List<i32> { return [13i32, 29i32]; }"####,
        blocker: r####"[13i32, 29i32]"####,
        targeted: (1953, 42, 2),
        graph: (223942, 25054, 10),
    },
    Sibling {
        label: "lit-unannotated",
        source: r####"function f(): bool { let input = [13i32, 29i32]; return true; }"####,
        blocker: r####"[13i32, 29i32]"####,
        targeted: (2000, 55, 3),
        graph: (224050, 25067, 10),
    },
];

const ADMITTED: &[Admitted] = &[
    Admitted {
        label: "scoped",
        source: r####"function f(pair: [i32, i32]): bool { { let ok = pair[0] == 13i32 && pair[1] == 29i32; } return true; }"####,
        completion: "Checked",
        targeted: (3207, 594, 6),
        graph: (225858, 25606, 10),
        sequence: &[
            ("Copy goto 1", "pair[0]"),
            ("Initialize goto 2", "13i32"),
            ("EqualI32 goto 3", "pair[0] == 13i32"),
            ("None branch 4 5", "pair[0] == 13i32"),
            ("None goto 6", "pair[0] == 13i32 && pair[1] == 29i32"),
            ("None goto 7", "pair[0] == 13i32 && pair[1] == 29i32"),
            ("Copy goto 8", "pair[1]"),
            ("Initialize goto 9", "pair[0] == 13i32 && pair[1] == 29i32"),
            ("Initialize goto 10", "29i32"),
            (
                "Cleanup goto 11",
                "{ let ok = pair[0] == 13i32 && pair[1] == 29i32; }",
            ),
            ("EqualI32 goto 9", "pair[1] == 29i32"),
            ("Initialize goto 12", "true"),
            ("Export goto 13", "return true;"),
            (
                "Cleanup goto 14",
                "{ { let ok = pair[0] == 13i32 && pair[1] == 29i32; } return true; }",
            ),
            (
                "None return",
                "{ { let ok = pair[0] == 13i32 && pair[1] == 29i32; } return true; }",
            ),
        ],
    },
    Admitted {
        label: "let-and",
        source: r####"function f(pair: [i32, i32]): bool { let ok = pair[0] == 13i32 && pair[1] == 29i32; return ok; }"####,
        completion: "Checked",
        targeted: (3220, 596, 5),
        graph: (225811, 25608, 10),
        sequence: &[
            ("Copy goto 1", "pair[0]"),
            ("Initialize goto 2", "13i32"),
            ("EqualI32 goto 3", "pair[0] == 13i32"),
            ("None branch 4 5", "pair[0] == 13i32"),
            ("None goto 6", "pair[0] == 13i32 && pair[1] == 29i32"),
            ("None goto 7", "pair[0] == 13i32 && pair[1] == 29i32"),
            ("Copy goto 8", "pair[1]"),
            ("Initialize goto 9", "pair[0] == 13i32 && pair[1] == 29i32"),
            ("Initialize goto 10", "29i32"),
            ("Copy goto 11", "ok"),
            ("EqualI32 goto 9", "pair[1] == 29i32"),
            ("Export goto 12", "return ok;"),
            (
                "Cleanup goto 13",
                "{ let ok = pair[0] == 13i32 && pair[1] == 29i32; return ok; }",
            ),
            (
                "Cleanup goto 14",
                "{ let ok = pair[0] == 13i32 && pair[1] == 29i32; return ok; }",
            ),
            (
                "None return",
                "{ let ok = pair[0] == 13i32 && pair[1] == 29i32; return ok; }",
            ),
        ],
    },
    Admitted {
        label: "param-pair",
        source: r####"function f(pair: [i32, i32]): bool { return pair[0] == 13i32 && pair[1] == 29i32; }"####,
        completion: "Checked",
        targeted: (3032, 495, 5),
        graph: (225502, 25507, 10),
        sequence: &[
            ("Copy goto 1", "pair[0]"),
            ("Initialize goto 2", "13i32"),
            ("EqualI32 goto 3", "pair[0] == 13i32"),
            ("None branch 4 5", "pair[0] == 13i32"),
            ("None goto 6", "pair[0] == 13i32 && pair[1] == 29i32"),
            ("None goto 7", "pair[0] == 13i32 && pair[1] == 29i32"),
            ("Copy goto 8", "pair[1]"),
            ("Initialize goto 9", "pair[0] == 13i32 && pair[1] == 29i32"),
            ("Initialize goto 10", "29i32"),
            (
                "Export goto 11",
                "return pair[0] == 13i32 && pair[1] == 29i32;",
            ),
            ("EqualI32 goto 9", "pair[1] == 29i32"),
            (
                "Cleanup goto 12",
                "{ return pair[0] == 13i32 && pair[1] == 29i32; }",
            ),
            (
                "None return",
                "{ return pair[0] == 13i32 && pair[1] == 29i32; }",
            ),
        ],
    },
    Admitted {
        label: "param-pair-eq",
        source: r####"function f(pair: [i32, i32]): bool { return pair[0] == 13i32; }"####,
        completion: "Checked",
        targeted: (2590, 248, 4),
        graph: (224760, 25260, 10),
        sequence: &[
            ("Copy goto 1", "pair[0]"),
            ("Initialize goto 2", "13i32"),
            ("EqualI32 goto 3", "pair[0] == 13i32"),
            ("Export goto 4", "return pair[0] == 13i32;"),
            ("Cleanup goto 5", "{ return pair[0] == 13i32; }"),
            ("None return", "{ return pair[0] == 13i32; }"),
        ],
    },
    Admitted {
        label: "reversed",
        source: r####"function f(pair: [i32, i32]): bool { return 13i32 == pair[0] && 29i32 == pair[1]; }"####,
        completion: "Checked",
        targeted: (3040, 497, 5),
        graph: (225510, 25509, 10),
        sequence: &[
            ("Initialize goto 1", "13i32"),
            ("Copy goto 2", "pair[0]"),
            ("EqualI32 goto 3", "13i32 == pair[0]"),
            ("None branch 4 5", "13i32 == pair[0]"),
            ("None goto 6", "13i32 == pair[0] && 29i32 == pair[1]"),
            ("None goto 7", "13i32 == pair[0] && 29i32 == pair[1]"),
            ("Initialize goto 8", "29i32"),
            ("Initialize goto 9", "13i32 == pair[0] && 29i32 == pair[1]"),
            ("Copy goto 10", "pair[1]"),
            (
                "Export goto 11",
                "return 13i32 == pair[0] && 29i32 == pair[1];",
            ),
            ("EqualI32 goto 9", "29i32 == pair[1]"),
            (
                "Cleanup goto 12",
                "{ return 13i32 == pair[0] && 29i32 == pair[1]; }",
            ),
            (
                "None return",
                "{ return 13i32 == pair[0] && 29i32 == pair[1]; }",
            ),
        ],
    },
    Admitted {
        label: "grouped",
        source: r####"function f(pair: [i32, i32]): bool { return ((pair[0] == 13i32)) && ((pair[1] == 29i32)); }"####,
        completion: "Checked",
        targeted: (3048, 499, 7),
        graph: (225758, 25511, 10),
        sequence: &[
            ("Copy goto 1", "pair[0]"),
            ("Initialize goto 2", "13i32"),
            ("EqualI32 goto 3", "pair[0] == 13i32"),
            ("None branch 4 5", "((pair[0] == 13i32))"),
            (
                "None goto 6",
                "((pair[0] == 13i32)) && ((pair[1] == 29i32))",
            ),
            (
                "None goto 7",
                "((pair[0] == 13i32)) && ((pair[1] == 29i32))",
            ),
            ("Copy goto 8", "pair[1]"),
            (
                "Initialize goto 9",
                "((pair[0] == 13i32)) && ((pair[1] == 29i32))",
            ),
            ("Initialize goto 10", "29i32"),
            (
                "Export goto 11",
                "return ((pair[0] == 13i32)) && ((pair[1] == 29i32));",
            ),
            ("EqualI32 goto 9", "pair[1] == 29i32"),
            (
                "Cleanup goto 12",
                "{ return ((pair[0] == 13i32)) && ((pair[1] == 29i32)); }",
            ),
            (
                "None return",
                "{ return ((pair[0] == 13i32)) && ((pair[1] == 29i32)); }",
            ),
        ],
    },
    Admitted {
        label: "grouped-op",
        source: r####"function f(pair: [i32, i32]): bool { return (pair[0]) == (13i32) && (pair[1]) == (29i32); }"####,
        completion: "Checked",
        targeted: (3048, 499, 6),
        graph: (225758, 25511, 10),
        sequence: &[
            ("Copy goto 1", "pair[0]"),
            ("Initialize goto 2", "13i32"),
            ("EqualI32 goto 3", "(pair[0]) == (13i32)"),
            ("None branch 4 5", "(pair[0]) == (13i32)"),
            (
                "None goto 6",
                "(pair[0]) == (13i32) && (pair[1]) == (29i32)",
            ),
            (
                "None goto 7",
                "(pair[0]) == (13i32) && (pair[1]) == (29i32)",
            ),
            ("Copy goto 8", "pair[1]"),
            (
                "Initialize goto 9",
                "(pair[0]) == (13i32) && (pair[1]) == (29i32)",
            ),
            ("Initialize goto 10", "29i32"),
            (
                "Export goto 11",
                "return (pair[0]) == (13i32) && (pair[1]) == (29i32);",
            ),
            ("EqualI32 goto 9", "(pair[1]) == (29i32)"),
            (
                "Cleanup goto 12",
                "{ return (pair[0]) == (13i32) && (pair[1]) == (29i32); }",
            ),
            (
                "None return",
                "{ return (pair[0]) == (13i32) && (pair[1]) == (29i32); }",
            ),
        ],
    },
    Admitted {
        label: "mut-pair",
        source: r####"function f(): bool { let mut pair: [i32, i32] = [1i32, 2i32]; pair[0] = 5i32; return pair[0] == 13i32; }"####,
        completion: "Checked",
        targeted: (2879, 408, 4),
        graph: (225529, 25420, 10),
        sequence: &[
            ("Initialize goto 1", "1i32"),
            ("Initialize goto 2", "2i32"),
            ("Initialize goto 3", "[1i32, 2i32]"),
            ("Initialize goto 4", "5i32"),
            ("None goto 5", "pair[0] = 5i32;"),
            ("Install goto 6", "pair[0] = 5i32;"),
            ("Copy goto 7", "pair[0]"),
            ("Initialize goto 8", "13i32"),
            ("EqualI32 goto 9", "pair[0] == 13i32"),
            ("Export goto 10", "return pair[0] == 13i32;"),
            (
                "Cleanup goto 11",
                "{ let mut pair: [i32, i32] = [1i32, 2i32]; pair[0] = 5i32; return pair[0] == 13i32; }",
            ),
            (
                "None return",
                "{ let mut pair: [i32, i32] = [1i32, 2i32]; pair[0] = 5i32; return pair[0] == 13i32; }",
            ),
        ],
    },
    Admitted {
        label: "sum-and",
        source: r####"function f(a: i32, b: i32): bool { return a + b == 13i32 && a * 2i32 == b; }"####,
        completion: "Checked",
        targeted: (2666, 415, 6),
        graph: (225197, 25427, 10),
        sequence: &[
            ("CheckedI32Add goto 1", "a + b"),
            ("Initialize goto 2", "13i32"),
            ("EqualI32 goto 3", "a + b == 13i32"),
            ("None branch 4 5", "a + b == 13i32"),
            ("None goto 6", "a + b == 13i32 && a * 2i32 == b"),
            ("None goto 7", "a + b == 13i32 && a * 2i32 == b"),
            ("CheckedI32Multiply goto 8", "a * 2i32"),
            ("Initialize goto 9", "a + b == 13i32 && a * 2i32 == b"),
            ("Copy goto 10", "b"),
            ("Export goto 11", "return a + b == 13i32 && a * 2i32 == b;"),
            ("EqualI32 goto 9", "a * 2i32 == b"),
            (
                "Cleanup goto 12",
                "{ return a + b == 13i32 && a * 2i32 == b; }",
            ),
            ("None return", "{ return a + b == 13i32 && a * 2i32 == b; }"),
        ],
    },
    Admitted {
        label: "calls-and",
        source: r####"function seven(): i32 { return 7i32; } function f(): bool { return seven() == 7i32 && seven() == 8i32; }"####,
        completion: "Checked",
        targeted: (4456, 392, 5),
        graph: (225295, 25366, 10),
        sequence: &[
            ("Call goto 1", "seven()"),
            ("Initialize goto 2", "7i32"),
            ("EqualI32 goto 3", "seven() == 7i32"),
            ("None branch 4 5", "seven() == 7i32"),
            ("None goto 6", "seven() == 7i32 && seven() == 8i32"),
            ("None goto 7", "seven() == 7i32 && seven() == 8i32"),
            ("Call goto 8", "seven()"),
            ("Initialize goto 9", "seven() == 7i32 && seven() == 8i32"),
            ("Initialize goto 10", "8i32"),
            (
                "Export goto 11",
                "return seven() == 7i32 && seven() == 8i32;",
            ),
            ("EqualI32 goto 9", "seven() == 8i32"),
            (
                "None return",
                "{ return seven() == 7i32 && seven() == 8i32; }",
            ),
        ],
    },
    Admitted {
        label: "lit-one",
        source: r####"function f(): bool { let input: List<i32> = [13i32]; dispose(input); return true; }"####,
        completion: "Checked",
        targeted: (2308, 212, 5),
        graph: (224659, 25225, 10),
        sequence: &[
            ("Initialize goto 1", "13i32"),
            ("CommitListI32 goto 2", "[13i32]"),
            ("Cleanup goto 3", "dispose(input)"),
            ("Initialize goto 4", "dispose(input)"),
            ("None goto 5", "dispose(input);"),
            ("Initialize goto 6", "true"),
            ("Export goto 7", "return true;"),
            (
                "Cleanup goto 8",
                "{ let input: List<i32> = [13i32]; dispose(input); return true; }",
            ),
            (
                "None return",
                "{ let input: List<i32> = [13i32]; dispose(input); return true; }",
            ),
        ],
    },
    Admitted {
        label: "lit-two",
        source: r####"function f(): bool { let input: List<i32> = [13i32, 29i32]; dispose(input); return true; }"####,
        completion: "Checked",
        targeted: (2385, 255, 5),
        graph: (224796, 25268, 10),
        sequence: &[
            ("Initialize goto 1", "13i32"),
            ("Initialize goto 2", "29i32"),
            ("CommitListI32 goto 3", "[13i32, 29i32]"),
            ("Cleanup goto 4", "dispose(input)"),
            ("Initialize goto 5", "dispose(input)"),
            ("None goto 6", "dispose(input);"),
            ("Initialize goto 7", "true"),
            ("Export goto 8", "return true;"),
            (
                "Cleanup goto 9",
                "{ let input: List<i32> = [13i32, 29i32]; dispose(input); return true; }",
            ),
            (
                "None return",
                "{ let input: List<i32> = [13i32, 29i32]; dispose(input); return true; }",
            ),
        ],
    },
    Admitted {
        label: "lit-leak",
        source: r####"function f(): bool { let input: List<i32> = [13i32, 29i32]; return true; }"####,
        completion: "Checked",
        targeted: (2226, 181, 4),
        graph: (224397, 25194, 10),
        sequence: &[
            ("Initialize goto 1", "13i32"),
            ("Initialize goto 2", "29i32"),
            ("CommitListI32 goto 3", "[13i32, 29i32]"),
            ("Initialize goto 4", "true"),
            ("Export goto 5", "return true;"),
            (
                "Cleanup goto 6",
                "{ let input: List<i32> = [13i32, 29i32]; return true; }",
            ),
            (
                "None return",
                "{ let input: List<i32> = [13i32, 29i32]; return true; }",
            ),
        ],
    },
    Admitted {
        label: "lit-mut",
        source: r####"function f(): bool { let mut input: List<i32> = [13i32, 29i32]; dispose(input); return true; }"####,
        completion: "Checked",
        targeted: (2385, 255, 5),
        graph: (224796, 25268, 10),
        sequence: &[
            ("Initialize goto 1", "13i32"),
            ("Initialize goto 2", "29i32"),
            ("CommitListI32 goto 3", "[13i32, 29i32]"),
            ("Cleanup goto 4", "dispose(input)"),
            ("Initialize goto 5", "dispose(input)"),
            ("None goto 6", "dispose(input);"),
            ("Initialize goto 7", "true"),
            ("Export goto 8", "return true;"),
            (
                "Cleanup goto 9",
                "{ let mut input: List<i32> = [13i32, 29i32]; dispose(input); return true; }",
            ),
            (
                "None return",
                "{ let mut input: List<i32> = [13i32, 29i32]; dispose(input); return true; }",
            ),
        ],
    },
    Admitted {
        label: "lit-scoped",
        source: r####"function f(): bool { { let input: List<i32> = [13i32, 29i32]; dispose(input); } return true; }"####,
        completion: "Checked",
        targeted: (2389, 256, 6),
        graph: (224860, 25269, 10),
        sequence: &[
            ("Initialize goto 1", "13i32"),
            ("Initialize goto 2", "29i32"),
            ("CommitListI32 goto 3", "[13i32, 29i32]"),
            ("Cleanup goto 4", "dispose(input)"),
            ("Initialize goto 5", "dispose(input)"),
            ("None goto 6", "dispose(input);"),
            (
                "Cleanup goto 7",
                "{ let input: List<i32> = [13i32, 29i32]; dispose(input); }",
            ),
            ("Initialize goto 8", "true"),
            ("Export goto 9", "return true;"),
            (
                "None return",
                "{ { let input: List<i32> = [13i32, 29i32]; dispose(input); } return true; }",
            ),
        ],
    },
    Admitted {
        label: "lit-loop",
        source: r####"function f(again: bool): bool { while (again) { let input: List<i32> = [13i32, 29i32]; dispose(input); } return true; }"####,
        completion: "Checked",
        targeted: (2663, 407, 7),
        graph: (225315, 25420, 10),
        sequence: &[
            ("Copy goto 1", "again"),
            ("None branch 2 3", "again"),
            ("Initialize goto 4", "13i32"),
            ("Initialize goto 5", "true"),
            ("Initialize goto 6", "29i32"),
            ("Export goto 7", "return true;"),
            ("CommitListI32 goto 8", "[13i32, 29i32]"),
            (
                "Cleanup goto 9",
                "{ while (again) { let input: List<i32> = [13i32, 29i32]; dispose(input); } return true; }",
            ),
            ("Cleanup goto 10", "dispose(input)"),
            (
                "None return",
                "{ while (again) { let input: List<i32> = [13i32, 29i32]; dispose(input); } return true; }",
            ),
            ("Initialize goto 11", "dispose(input)"),
            ("None goto 12", "dispose(input);"),
            (
                "Cleanup goto 13",
                "{ let input: List<i32> = [13i32, 29i32]; dispose(input); }",
            ),
            (
                "None goto 0",
                "while (again) { let input: List<i32> = [13i32, 29i32]; dispose(input); }",
            ),
        ],
    },
    Admitted {
        label: "lit-arg",
        source: r####"function g(input: List<i32>): void { dispose(input); } function f(): void { g([13i32, 29i32]); }"####,
        completion: "Checked",
        targeted: (4327, 278, 6),
        graph: (225093, 25247, 10),
        sequence: &[
            ("Initialize goto 1", "13i32"),
            ("Initialize goto 2", "29i32"),
            ("CommitListI32 goto 3", "[13i32, 29i32]"),
            ("Call goto 4", "g([13i32, 29i32])"),
            ("None goto 5", "g([13i32, 29i32]);"),
            ("None return", "{ g([13i32, 29i32]); }"),
        ],
    },
    Admitted {
        label: "lit-moved-twice",
        source: r####"function f(): bool { let input: List<i32> = [13i32, 29i32]; dispose(input); dispose(input); return true; }"####,
        completion: "SourceRejected",
        targeted: (2411, 258, 5),
        graph: (225063, 25271, 10),
        sequence: &[],
    },
];

fn stats((work, entries, max_depth): (usize, usize, usize)) -> Statistics {
    Statistics {
        work,
        entries,
        max_depth,
    }
}

fn limits(s: Statistics) -> (AnalysisLimits, [AnalysisLimits; 3]) {
    let exact = AnalysisLimits {
        max_work: s.work,
        max_entries: s.entries,
        max_depth: s.max_depth,
    };
    (
        exact,
        [
            AnalysisLimits {
                max_work: s.work - 1,
                ..exact
            },
            AnalysisLimits {
                max_entries: s.entries - 1,
                ..exact
            },
            AnalysisLimits {
                max_depth: s.max_depth - 1,
                ..exact
            },
        ],
    )
}

fn decl<'g>(module: &'g loader::Module, name: &str) -> &'g loader::Declaration {
    module
        .declarations
        .iter()
        .find(|d| {
            if name == "expect" {
                d.id.kind == DeclarationKind::Expectation
            } else {
                d.id.kind == DeclarationKind::Function && d.name.as_deref() == Some(name)
            }
        })
        .unwrap()
}

fn root_query<'g>(
    receipt: &TypedReceipt<'_, 'g>,
    module: &'g loader::Module,
    name: &str,
    limits: AnalysisLimits,
) -> Report<'g> {
    let declaration = decl(module, name);
    if name == "expect" {
        lower_root(
            receipt,
            module,
            BodyOwner::Declaration(&declaration.id),
            limits,
        )
    } else {
        lower_body(receipt, module, &declaration.id, limits)
    }
    .unwrap()
}

fn graph_of<'g>(receipt: &TypedReceipt<'_, 'g>) -> GraphReport<'g> {
    analyze_ownership(receipt, AnalysisLimits::default())
}

fn kind(operation: &Operation<'_>) -> String {
    format!("{operation:?}")
        .split(|c: char| !c.is_alphanumeric())
        .next()
        .unwrap()
        .to_string()
}

/// `(operation name and where control goes next, site text)` for every block
/// in index order: the whole MIR, branches and loops included (metering rule 9).
fn blocks(mir: &Mir<'_>) -> Vec<(String, String)> {
    mir.blocks
        .iter()
        .map(|step| {
            let next = match step.terminator {
                Terminator::Goto(next) => format!("goto {next}"),
                Terminator::Branch { yes, no } => format!("branch {yes} {no}"),
                Terminator::Return => "return".to_string(),
                other => panic!("unexpected terminator {other:?}"),
            };
            (
                format!("{} {next}", kind(&step.operation)),
                step.site.span.bytes().escape_ascii().to_string(),
            )
        })
        .collect()
}

/// `(operation name, site text)` for each block along the straight path from
/// `block` to a branch or return, with `None` operations kept (they end scratch).
fn walk(mir: &Mir<'_>, mut block: usize) -> (Vec<(String, String)>, Terminator) {
    let mut path = Vec::new();
    for _ in 0..=mir.blocks.len() {
        let step = &mir.blocks[block];
        path.push((
            kind(&step.operation),
            step.site.span.bytes().escape_ascii().to_string(),
        ));
        match step.terminator {
            Terminator::Goto(next) => block = next,
            terminator => return (path, terminator),
        }
    }
    panic!("path did not end");
}

fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
    list.iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect()
}

/// The ordered MIR of `joinedOrder`: one list per edge (metering rule 9). The
/// false edge never reads `pair[1]` or compares it.
fn check_order_mir(mir: &Mir<'_>) {
    const WHOLE: &str = "pair[0] == 13i32 && pair[1] == 29i32";
    const RETURN: &str = "return pair[0] == 13i32 && pair[1] == 29i32;";
    const BODY: &str = "{ let input: List<i32> = [13i32, 29i32]; let pair = joined(input); return pair[0] == 13i32 && pair[1] == 29i32; }";
    // The source is written across lines; compare on the nodes' own spans.
    let squash = |list: Vec<(String, String)>| -> Vec<(String, String)> {
        list.into_iter()
            .map(|(a, b)| {
                (
                    a,
                    b.replace("\\n", " ")
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" "),
                )
            })
            .collect()
    };
    let (prefix, terminator) = walk(mir, mir.entry);
    assert_eq!(
        squash(prefix),
        pairs(&[
            ("Initialize", "13i32"),
            ("Initialize", "29i32"),
            ("CommitListI32", "[13i32, 29i32]"),
            ("Move", "input"),
            ("Call", "joined(input)"),
            ("Copy", "pair[0]"),
            ("Initialize", "13i32"),
            ("EqualI32", "pair[0] == 13i32"),
            ("None", "pair[0] == 13i32"),
        ])
    );
    let Terminator::Branch { yes, no } = terminator else {
        panic!("expected the short-circuit branch: {terminator:?}");
    };
    let tail = [("Export", RETURN), ("Cleanup", BODY), ("None", BODY)];
    let (taken, end) = walk(mir, yes);
    assert!(matches!(end, Terminator::Return));
    assert_eq!(
        squash(taken),
        pairs(
            &[
                &[
                    ("None", WHOLE),
                    ("Copy", "pair[1]"),
                    ("Initialize", "29i32"),
                    ("EqualI32", "pair[1] == 29i32"),
                ][..],
                &tail,
            ]
            .concat()
        )
    );
    let (skipped, end) = walk(mir, no);
    assert!(matches!(end, Terminator::Return));
    assert_eq!(
        squash(skipped),
        pairs(&[&[("None", WHOLE), ("Initialize", WHOLE)][..], &tail].concat())
    );
}

fn each(
    test: &str,
    sources: &[(&str, String)],
    mut check: impl for<'g> FnMut(&str, &'g loader::Module, &TypedReceipt<'_, 'g>),
) {
    installed(test, |request| {
        for (label, source) in sources {
            fs::write(&request.entry, format!("{IMPORTS}\n{source}")).unwrap();
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
                let receipt = typed.receipt().unwrap();
                check(&format!("{label}, test_mode {test_mode}"), module, &receipt);
            }
        }
    });
}

/// Exact and one-short replays of one targeted query and the whole graph.
fn replay_caps<'g>(
    context: &str,
    receipt: &TypedReceipt<'_, 'g>,
    module: &'g loader::Module,
    name: &str,
    targeted: (usize, usize, usize),
    graph: (usize, usize, usize),
) {
    let target = root_query(receipt, module, name, AnalysisLimits::default());
    assert_eq!(target.statistics(), stats(targeted), "{context}: targeted");
    let (exact, short) = limits(target.statistics());
    let replay = root_query(receipt, module, name, exact);
    assert_eq!(replay.completion(), target.completion(), "{context}");
    assert_eq!(replay.statistics(), target.statistics(), "{context}");
    for short in short {
        let limited = root_query(receipt, module, name, short);
        crate::limit::assert_body_limit(&limited);
    }
    let whole = graph_of(receipt);
    assert_eq!(whole.statistics(), stats(graph), "{context}: graph");
    let (exact, short) = limits(whole.statistics());
    assert_eq!(
        analyze_ownership(receipt, exact).statistics(),
        whole.statistics(),
        "{context}"
    );
    for short in short {
        crate::limit::assert_graph_limit(&analyze_ownership(receipt, short));
    }
}

#[test]
fn joined_order_and_its_expectation_check_with_ordered_mir() {
    each(
        "o16_join_order::joined_order_and_its_expectation_check_with_ordered_mir",
        &[("O16", format!("{JOINED}{ORDER}"))],
        |context, module, receipt| {
            let order = root_query(receipt, module, "joinedOrder", AnalysisLimits::default());
            assert_eq!(order.completion(), Completion::Checked, "{context}");
            assert!(order.findings().is_empty() && order.blocked_at().is_none());
            check_order_mir(order.mir().unwrap());
            let whole = graph_of(receipt);
            let declaration = decl(module, "joinedOrder");
            let root = whole
                .roots()
                .iter()
                .find(|r| r.site().module == &module.key && r.site().node == declaration.node)
                .unwrap();
            assert_eq!(root.ordinary_completion(), Completion::Checked, "{context}");
            check_order_mir(root.ordinary_mir().unwrap());

            // The expectation calls `joinedOrder`; it adds no comparison of its own.
            let expect = root_query(receipt, module, "expect", AnalysisLimits::default());
            assert_eq!(expect.completion(), Completion::Checked, "{context}");
            let mir = expect.mir().unwrap();
            let (path, end) = walk(mir, mir.entry);
            assert!(
                matches!(end, Terminator::FinishExpectation),
                "{context}: {end:?}"
            );
            let calls: Vec<_> = path.iter().filter(|(name, _)| name == "Call").collect();
            assert_eq!(
                calls
                    .iter()
                    .map(|(_, site)| site.as_str())
                    .collect::<Vec<_>>(),
                ["joinedOrder()"],
                "{context}"
            );
            // Graph attribution (rule 11, D2 (b)): `joinedOrder` adds 1,313 work and
            // 678 entries to every graph that holds it; the expectation adds none.
            // Depth 10 stays with `joined`.
            assert_eq!(whole.statistics(), stats((226906, 25858, 10)), "{context}");
            assert_eq!(whole.completion(), GraphCompletion::Incomplete);
        },
    );
}

#[test]
fn old_stop_caps_give_resource_limit() {
    each(
        "o16_join_order::old_stop_caps_give_resource_limit",
        &[("O16", format!("{JOINED}{ORDER}"))],
        |_, module, receipt| {
            for (name, (max_work, max_entries, max_depth)) in
                [("joinedOrder", OLD_JOINED_ORDER), ("expect", OLD_EXPECT)]
            {
                let old = AnalysisLimits {
                    max_work,
                    max_entries,
                    max_depth,
                };
                let limited = root_query(receipt, module, name, old);
                crate::limit::assert_body_limit(&limited);
            }
            let (max_work, max_entries, max_depth) = OLD_GRAPH;
            let old = AnalysisLimits {
                max_work,
                max_entries,
                max_depth,
            };
            crate::limit::assert_graph_limit(&analyze_ownership(receipt, old));
        },
    );
}

/// The fixtures whose graph caps moved by the same 1,313 / 678.
#[test]
fn old_fixture_graph_caps_give_resource_limit() {
    installed(
        "o16_join_order::old_fixture_graph_caps_give_resource_limit",
        |request| {
            let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/m5/legacy-caps");
            let app = request.entry.parent().unwrap();
            for (fixture, (max_work, max_entries, max_depth), new) in [
                ("shared", (243522, 26094, 10), (244886, 26772, 10)),
                ("alias", (243522, 26094, 10), (244886, 26772, 10)),
                ("competing", (214539, 5705, 10), (215903, 6383, 10)),
            ] {
                for stale in fs::read_dir(app).unwrap() {
                    fs::remove_file(stale.unwrap().path()).unwrap();
                }
                for file in fs::read_dir(fixtures.join(fixture)).unwrap() {
                    let file = file.unwrap();
                    fs::copy(file.path(), app.join(file.file_name())).unwrap();
                }
                for test_mode in [false, true] {
                    let graph = loader::load_installed(InstalledRequest {
                        test_mode,
                        ..request.clone()
                    })
                    .unwrap();
                    let typed = analyze_typed(&graph, AnalysisLimits::default());
                    let receipt = typed.receipt().unwrap();
                    let old = AnalysisLimits {
                        max_work,
                        max_entries,
                        max_depth,
                    };
                    crate::limit::assert_graph_limit(&analyze_ownership(&receipt, old));
                    assert_eq!(
                        graph_of(&receipt).statistics(),
                        stats(new),
                        "{fixture} test_mode={test_mode}"
                    );
                }
            }
        },
    );
}

/// Metering rule 2: shapes next to the admitted ones keep the targeted and
/// graph caps and blockers that `7002855e` measured, and give `Incomplete`.
#[test]
fn unsupported_siblings_keep_their_parent_caps_and_blockers() {
    let sources: Vec<_> = SIBLINGS
        .iter()
        .map(|s| (s.label, format!("{JOINED}{}", s.source)))
        .collect();
    each(
        "o16_join_order::unsupported_siblings_keep_their_parent_caps_and_blockers",
        &sources,
        |context, module, receipt| {
            let label = context.split(',').next().unwrap();
            let sibling = SIBLINGS.iter().find(|s| s.label == label).unwrap();
            let target = root_query(receipt, module, "f", AnalysisLimits::default());
            assert_eq!(target.completion(), Completion::Incomplete, "{context}");
            assert!(target.mir().is_none() && target.findings().is_empty());
            assert_eq!(
                target
                    .blocked_at()
                    .unwrap()
                    .span
                    .bytes()
                    .escape_ascii()
                    .to_string(),
                sibling.blocker,
                "{context}"
            );
            replay_caps(
                context,
                receipt,
                module,
                "f",
                sibling.targeted,
                sibling.graph,
            );
        },
    );
}

/// The shapes this ticket admits, each at its exact caps and one short.
#[test]
fn admitted_shapes_check_at_exact_caps() {
    let sources: Vec<_> = ADMITTED
        .iter()
        .map(|s| (s.label, format!("{JOINED}{}", s.source)))
        .collect();
    each(
        "o16_join_order::admitted_shapes_check_at_exact_caps",
        &sources,
        |context, module, receipt| {
            let label = context.split(',').next().unwrap();
            let shape = ADMITTED.iter().find(|s| s.label == label).unwrap();
            let target = root_query(receipt, module, "f", AnalysisLimits::default());
            match shape.completion {
                "Checked" => {
                    assert_eq!(target.completion(), Completion::Checked, "{context}");
                    assert!(target.findings().is_empty());
                    let expected = pairs(shape.sequence);
                    assert_eq!(blocks(target.mir().unwrap()), expected, "{context}");
                    let whole = graph_of(receipt);
                    let declaration = decl(module, "f");
                    let root = whole
                        .roots()
                        .iter()
                        .find(|r| {
                            r.site().module == &module.key && r.site().node == declaration.node
                        })
                        .unwrap();
                    assert_eq!(root.ordinary_completion(), Completion::Checked, "{context}");
                    assert_eq!(blocks(root.ordinary_mir().unwrap()), expected, "{context}");
                }
                _ => {
                    // A use after the list is disposed is the ordinary finding.
                    assert_eq!(target.completion(), Completion::SourceRejected, "{context}");
                    assert!(target.mir().is_none());
                    let [finding] = target.findings() else {
                        panic!("{context}: one finding expected");
                    };
                    assert_eq!(finding.code(), "R0200", "{context}");
                    assert_eq!(finding.site.span.bytes(), b"dispose(input)", "{context}");
                }
            }
            replay_caps(context, receipt, module, "f", shape.targeted, shape.graph);
        },
    );
}

/// Metering rule 5: grouped operands add depth and no operations, and one
/// short of the grouped depth gives `ResourceLimit`.
#[test]
fn grouped_operands_add_depth_not_operations() {
    let bare =
        "function f(pair: [i32, i32]): bool { return pair[0] == 13i32 && pair[1] == 29i32; }";
    let grouped = "function f(pair: [i32, i32]): bool { return ((pair[0] == 13i32)) && ((pair[1] == 29i32)); }";
    let sources = [
        ("bare", format!("{JOINED}{bare}")),
        ("grouped", format!("{JOINED}{grouped}")),
    ];
    each(
        "o16_join_order::grouped_operands_add_depth_not_operations",
        &sources,
        |context, module, receipt| {
            let target = root_query(receipt, module, "f", AnalysisLimits::default());
            assert_eq!(target.completion(), Completion::Checked, "{context}");
            let names: Vec<_> = target
                .mir()
                .unwrap()
                .blocks
                .iter()
                .map(|b| {
                    format!("{:?}", b.operation)
                        .split(|c: char| !c.is_alphanumeric())
                        .next()
                        .unwrap()
                        .to_string()
                })
                .collect();
            let depth = if context.starts_with("bare") { 5 } else { 7 };
            let admitted = if depth == 5 { "param-pair" } else { "grouped" };
            let shape = ADMITTED.iter().find(|s| s.label == admitted).unwrap();
            assert_eq!(
                blocks(target.mir().unwrap()),
                pairs(shape.sequence),
                "{context}"
            );
            assert_eq!(target.statistics().max_depth, depth, "{context}");
            assert_eq!(
                names,
                [
                    "Copy",
                    "Initialize",
                    "EqualI32",
                    "None",
                    "None",
                    "None",
                    "Copy",
                    "Initialize",
                    "Initialize",
                    "Export",
                    "EqualI32",
                    "Cleanup",
                    "None"
                ]
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>(),
                "{context}"
            );
            let (targeted, graph) = if depth == 5 {
                ((3032, 495, 5), (225502, 25507, 10))
            } else {
                ((3048, 499, 7), (225758, 25511, 10))
            };
            replay_caps(context, receipt, module, "f", targeted, graph);
        },
    );
}
