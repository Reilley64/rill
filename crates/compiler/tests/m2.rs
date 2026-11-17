//! Parser-only acceptance for M2 (#52); G cases come from both v1 syntax annexes.
//! Syntax success is not source checking or Rill execution.
use compiler::lexer::{LexStatus, lex};
use compiler::parser::*;
use compiler::source::Snapshot;

fn snapshot(text: impl AsRef<[u8]>) -> Snapshot {
    Snapshot::capture("synthetic://m2", text.as_ref()).unwrap()
}

fn parse_text(text: &str, entry: EntryPoint) -> Result<Syntax, ParseFailure> {
    let source = snapshot(text);
    parse(&source, &lex(&source), entry, ParseLimits::default())
}

fn accepted(text: &str, entry: EntryPoint) -> Syntax {
    parse_text(text, entry).unwrap_or_else(|error| panic!("{text:?}: {error:?}"))
}

fn rejected(text: &str, entry: EntryPoint) -> ParseFailure {
    parse_text(text, entry).unwrap_err()
}

#[test]
fn g4_g6_g9_g10_g14_g46_generic_commit_and_precedence() {
    let syntax = accepted("a < b >> c", EntryPoint::Expression);
    let root = &syntax.nodes[syntax.root];
    assert_eq!(root.kind, NodeKind::Binary("<"));
    assert_eq!(syntax.nodes[root.children[1]].kind, NodeKind::Binary(">>"));
    assert!(syntax.views.iter().any(|v| v.span.bytes() == b">>"));
    let syntax = accepted("a < b > (c)", EntryPoint::Expression);
    let root = &syntax.nodes[syntax.root];
    assert_eq!(root.kind, NodeKind::Call);
    let reference = &syntax.nodes[root.children[0]];
    assert_eq!(reference.kind, NodeKind::ValueReference);
    assert_eq!(
        syntax.nodes[reference.children[1]].kind,
        NodeKind::TypeArguments
    );
    let syntax = accepted("(a < b) > (c)", EntryPoint::Expression);
    assert_eq!(syntax.nodes[syntax.root].kind, NodeKind::Binary(">"));
    let syntax = accepted("Packet<i32>.Data(3i32)", EntryPoint::Expression);
    assert_eq!(syntax.nodes[syntax.root].kind, NodeKind::Call);
    rejected("Packet.Data<i32>", EntryPoint::Expression);
    rejected("&&value", EntryPoint::Expression);
    let syntax = accepted("a < b >= c", EntryPoint::Expression);
    assert_eq!(syntax.nodes[syntax.root].kind, NodeKind::Binary(">="));
    assert!(syntax.views.iter().any(|v| v.span.bytes() == b">="));
    for text in ["f<T>", "f < /* type */ T >", "f<Box<T>>", "f<T>(x)"] {
        accepted(text, EntryPoint::Expression);
    }
    let comparison = accepted("f<T>=x", EntryPoint::Expression);
    assert_eq!(
        comparison.nodes[comparison.root].kind,
        NodeKind::Binary(">=")
    );
    let comparison = accepted("f<T", EntryPoint::Expression);
    assert_eq!(
        comparison.nodes[comparison.root].kind,
        NodeKind::Binary("<")
    );
    for text in ["f<T>>=x", "f<>", "f<", "value other"] {
        rejected(text, EntryPoint::Expression);
    }
    let syntax = accepted("a + b * c - d", EntryPoint::Expression);
    let root = &syntax.nodes[syntax.root];
    assert_eq!(root.kind, NodeKind::Binary("-"));
    let sum = &syntax.nodes[root.children[0]];
    assert_eq!(sum.kind, NodeKind::Binary("+"));
    assert_eq!(syntax.nodes[sum.children[1]].kind, NodeKind::Binary("*"));
}

#[test]
fn g21_g23_g31_g33_construction_and_question_commitments() {
    let syntax = accepted("Point { x, y }", EntryPoint::Expression);
    let root = &syntax.nodes[syntax.root];
    assert_eq!(root.kind, NodeKind::Construction);
    assert_eq!(
        syntax.nodes[root.children[1]].kind,
        NodeKind::InitializerField { shorthand: true }
    );
    let syntax = accepted("Point { ...base, x: 2 }", EntryPoint::Expression);
    assert_eq!(
        syntax.nodes[syntax.nodes[syntax.root].children[1]].kind,
        NodeKind::Spread
    );
    rejected("Point { x: 2, ...base }", EntryPoint::Expression);
    rejected("Point { ...base, ...other }", EntryPoint::Expression);
    for text in ["read()? - 1i32", "read() ? /* wrap */\n-1i32"] {
        let syntax = accepted(text, EntryPoint::Expression);
        assert_eq!(syntax.nodes[syntax.root].kind, NodeKind::Wrap);
        assert_eq!(
            syntax.nodes[syntax.nodes[syntax.root].children[1]].kind,
            NodeKind::Prefix("-")
        );
    }
    let syntax = accepted("(read()?) - 1i32", EntryPoint::Expression);
    assert_eq!(syntax.nodes[syntax.root].kind, NodeKind::Binary("-"));
    rejected("value?()", EntryPoint::Expression);
    let syntax = accepted("(value?)()", EntryPoint::Expression);
    assert_eq!(syntax.nodes[syntax.root].kind, NodeKind::Call);
    rejected("a ? b ? c", EntryPoint::Expression);
    for text in [
        "a ? (b ? c)",
        "(a ? b) ? c",
        "value?",
        "(value?) [0]",
        "value?[0]",
    ] {
        accepted(text, EntryPoint::Expression);
    }
    let syntax = accepted("value?[0]", EntryPoint::Expression);
    assert_eq!(syntax.nodes[syntax.root].kind, NodeKind::Wrap);
    for text in [
        "a ?",
        "{}",
        "Point.X.Y {}",
        "make() {}",
        "Point { x: }",
        "Point { ... }",
    ] {
        if text == "a ?" {
            accepted(text, EntryPoint::Expression);
        } else {
            rejected(text, EntryPoint::Expression);
        }
    }
}

#[test]
fn literal_and_template_boundaries_are_m1_events() {
    let text = "`outer ${Point { value: \"}\", other: `inner ${1i32}` }} end`";
    let syntax = accepted(text, EntryPoint::Expression);
    assert_eq!(syntax.nodes[syntax.root].kind, NodeKind::Template);
    let holes: Vec<_> = syntax
        .nodes
        .iter()
        .filter(|n| n.kind == NodeKind::TemplateHole)
        .collect();
    assert_eq!(holes.len(), 2);
    assert!(holes.iter().any(|n| n.span.bytes() == b"${1i32}"));
    assert!(
        syntax
            .nodes
            .iter()
            .any(|n| n.kind == NodeKind::Literal && n.span.bytes() == b"\"}\"")
    );
    for text in [
        "``",
        "`\\${not_a_hole}`",
        "r##\"${raw}\"##",
        "'🙂'",
        "128i8",
    ] {
        accepted(text, EntryPoint::Expression);
    }
    for text in [
        "`${}`",
        "`${x y}`",
        "`${x:fmt}`",
        "`${(x}`",
        "`${x;}`",
        "`unfinished",
        "\"a\" \"b\"",
    ] {
        rejected(text, EntryPoint::Expression);
    }
}

#[test]
fn g24_g26_g47_patterns_are_syntax_not_type_queries() {
    let syntax = accepted("{ child: { value } }", EntryPoint::Pattern);
    assert_eq!(syntax.nodes[syntax.root].kind, NodeKind::RecordPattern);
    let field = &syntax.nodes[syntax.nodes[syntax.root].children[0]];
    assert_eq!(field.kind, NodeKind::PatternField { shorthand: false });
    assert_eq!(
        syntax.nodes[field.children[1]].kind,
        NodeKind::RecordPattern
    );
    let binder = accepted("None", EntryPoint::Pattern);
    assert_eq!(binder.nodes[binder.root].kind, NodeKind::NamePattern);
    let variant = accepted("Option.None", EntryPoint::Pattern);
    assert_eq!(variant.nodes[variant.root].kind, NodeKind::NamePattern);
    let syntax = accepted("-(128i8)", EntryPoint::Pattern);
    assert_eq!(syntax.nodes[syntax.root].kind, NodeKind::NegatedPattern);
    assert!(
        syntax
            .nodes
            .iter()
            .any(|n| n.kind == NodeKind::Literal && n.span.bytes() == b"128i8")
    );
    for text in [
        "_",
        "[]",
        "[x, Packet<i32>.Data(y,),]",
        "Point { x, y: _, }",
        "`static`",
        "true",
        "r#\"raw\"#",
        "'c'",
        "-((128i8))",
        "Some()",
    ] {
        accepted(text, EntryPoint::Pattern);
    }
    for text in [
        "`${value}`",
        "x + y",
        "[...tail]",
        "-name",
        "-(1i32 + 2i32)",
        "{} extra",
        "Point { ...rest }",
        "Packet.Data<i32>",
        "&x",
    ] {
        rejected(text, EntryPoint::Pattern);
    }
}

#[test]
fn g1_g2_g17_g20_declaration_shapes_and_placement() {
    let syntax = accepted(
        "import geometry::{Point as P, origin,};",
        EntryPoint::Module,
    );
    let import = &syntax.nodes[syntax.nodes[syntax.root].children[0]];
    assert_eq!(import.kind, NodeKind::Import);
    let group = &syntax.nodes[import.children[1]];
    assert_eq!(group.kind, NodeKind::ImportGroup);
    assert_eq!(group.children.len(), 2);
    assert_eq!(syntax.nodes[group.children[0]].span.bytes(), b"Point as P");
    for text in [
        "enum Empty {} type Marker = {};",
        "function f<T = i32, U>(value: U): U;", // G20: well-formedness is NOT parsing.
        "export function f(_: i32,): bool where eq(i32),;",
        "export import path as alias;",
        "export type R<T,> = { private hidden: &mut T; shown: [T]; };",
        "export enum E<T> { Unit, Tuple(T, bool,), Record { x: T; }, Empty {}, }",
        "implement<T,> api::f<T>(x: T): T where eq(T), {}",
        "function outer(): void { function inner(x: i32): i32 {} type Local = {} enum E {} }",
    ] {
        accepted(text, EntryPoint::Module);
    }
    for text in [
        "type Alias = Other;",
        "implement<T = i32> f(value: T): T {}",
        "function f(): void { import api; }",
        "function f(): void { implement f(): void {} }",
        "function f(): void { function local(): void; }",
        "export implement f(): void {}",
        "export let x = 1i32;",
        "function f(x): void;",
        "function f(): void {};",
        "{};",
        ";",
        "type R = { x: T, y: U }",
        "enum E { V() }",
        "import api::{};",
        "function f<>(): T;",
        "function f(x: T = value): T;",
    ] {
        rejected(text, EntryPoint::Module);
    }
}

#[test]
fn g3_g7_g8_g16_g24_g27_g30_g39_g41_g42_statements() {
    for text in [
        "function outer(): void { function inner(x: i32): i32 { return x; } }",
        "let converter = into<i64, i32>;",
        "let value = [];", // G16: missing element type is a later semantic error.
        "let { child: { value } } = record;",
        "if (ready) { work(); } else { stop(); }",
        "(if (ready) left else right);",
        "return; break; continue;", // Shapes parse independently of control targets.
        "guard let item: Item = operation() else (error) { return; }",
        "Ok(); Err<Success, ErrorType>(error);",
        "while (ready) { continue; } for (let [x, y] of source) { break; }",
        "if (a) {} else if (b) {} else {}",
        "let mut x: i32 = 1i32; x += 2i32; *ptr = value;",
        "guard ready else {} guard operation() else (error) {}",
        "expect 1i32;", // Parser does NOT enforce bool or return-context rules.
    ] {
        accepted(text, EntryPoint::Module);
    }
    let syntax = accepted("let item: Box<Box<i32>>=value;", EntryPoint::Module);
    let split: Vec<_> = syntax
        .views
        .iter()
        .filter(|v| v.span.start() >= 21 && v.span.end() <= 24)
        .collect();
    assert_eq!(
        split.iter().map(|v| v.span.bytes()).collect::<Vec<_>>(),
        vec![b">".as_slice(), b">", b"="]
    );
    assert_eq!(split[0].token, split[1].token);
    assert_eq!(split[1].token, split[2].token);
    for text in [
        "if (ready) left else right;",
        "if (ready) {};",
        "function f(): void { expect true; }",
        "{ expect true; }",
        "for (let mut x of source) {}",
        "let value;",
        "let x = 1i32",
        "return",
        "a = b = c;",
        "while (ready) work();",
        "guard let mut x = source else {}",
        "guard x else (_) {}",
        "if (ready) { value }",
    ] {
        rejected(text, EntryPoint::Module);
    }
}

#[test]
fn g11_g12_g40_g43_g44_arrows_and_reserved_match_delimiters() {
    let syntax = accepted("(number: i32): i32 => number", EntryPoint::Expression);
    assert_eq!(syntax.nodes[syntax.root].kind, NodeKind::Arrow);
    let head = &syntax.nodes[syntax.nodes[syntax.root].children[0]];
    assert_eq!(head.kind, NodeKind::ArrowHead);
    assert_eq!(head.span.bytes(), b"(number: i32): i32");
    for text in [
        "number => number",
        "mut (x) => x",
        "<T>(x: T) => x",
        "()",
        "(x, y)",
    ] {
        rejected(text, EntryPoint::Expression);
    }
    for text in [
        "() => true",
        "(_, x: i32,) => x",
        "(x) => (y) => x + y",
        "(() => true)()",
        "(x): (i32) => bool => x",
        "(x) => { return x; }",
    ] {
        accepted(text, EntryPoint::Expression);
    }
    accepted("expect (() => { return true; })();", EntryPoint::Module);
    let syntax = accepted(
        "match (value) { Variant if (ready) => answer, }",
        EntryPoint::Expression,
    );
    assert_eq!(syntax.nodes[syntax.root].kind, NodeKind::Match);
    let arm = &syntax.nodes[syntax.nodes[syntax.root].children[1]];
    assert_eq!(arm.kind, NodeKind::MatchArm);
    assert_eq!(syntax.nodes[arm.children[1]].kind, NodeKind::MatchGuard);
    assert!(!syntax.nodes.iter().any(|n| n.kind == NodeKind::Arrow));
    let syntax = accepted(
        "match (value) { Variant if ((item) => test(item))(value) => answer }",
        EntryPoint::Expression,
    );
    assert_eq!(
        syntax
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Arrow)
            .count(),
        1
    );
    for text in [
        "match (x) {}",
        "match (x) { _ => {}, V(y) => y, }",
        "match (x) { V if test((y) => y) => Record {}, _ => 0i32 }",
        "if (a) if (b) x else y else z",
        "if (a) (x) => x else (y) => y",
        "(x) => x(y)", // call belongs to the expression body, not the arrow.
    ] {
        accepted(text, EntryPoint::Expression);
    }
    for text in [
        "match (x) { _ if () => y }",
        "match (x) { _ => }",
        "match (x) { _ => x; }",
        "if (a) x",
        "if (a) {} else x",
        "() => { x }",
        "(x): T =>",
    ] {
        rejected(text, EntryPoint::Expression);
    }
}

#[test]
fn g34_g38_g45_pipe_steps_and_placeholder_ownership() {
    let syntax = accepted("x |> f ? Failure {}", EntryPoint::Expression);
    assert_eq!(syntax.nodes[syntax.root].kind, NodeKind::Wrap);
    let pipe = &syntax.nodes[syntax.nodes[syntax.root].children[0]];
    assert_eq!(pipe.kind, NodeKind::Pipe);
    assert_eq!(
        syntax.nodes[pipe.children[1]].kind,
        NodeKind::PipeStep { propagate: false }
    );
    let syntax = accepted("x |> pad(10i32, _)", EntryPoint::Expression);
    assert_eq!(syntax.nodes[syntax.root].kind, NodeKind::Pipe);
    assert_eq!(
        syntax
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::PipePlaceholder)
            .count(),
        1
    );
    let syntax = accepted("x |> f(y |> g(_), _)", EntryPoint::Expression);
    assert_eq!(
        syntax
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::PipePlaceholder)
            .count(),
        2
    );
    accepted("x |> ((value) => value)", EntryPoint::Expression);
    rejected("x |> f + y", EntryPoint::Expression);
    let syntax = accepted("(x |> f) + y", EntryPoint::Expression);
    assert_eq!(syntax.nodes[syntax.root].kind, NodeKind::Binary("+"));
    let syntax = accepted("x |> f? |> api::g<T>.call(extra)?", EntryPoint::Expression);
    assert_eq!(
        syntax
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::PipeStep { propagate: true })
            .count(),
        2
    );
    for text in [
        "x |> f([_])",
        "x |> f(_, _)",
        "x |> f(_ + y)",
        "x |> f((_))",
        "x |> f(g(_))",
        "x |> f[0]",
        "x |> (f)",
        "x |> [f]",
        "_",
        "f(_)",
        "x |> f?()",
    ] {
        rejected(text, EntryPoint::Expression);
    }
    for text in [
        "x |> f(y |> g(_, 2i32), _)",
        "x |> f(_) |> g(_)",
        "x |> f(_,) ? failure",
        "x |> f ? (e) => e",
        "x |> f?",
        "x + y |> object.call",
    ] {
        accepted(text, EntryPoint::Expression);
    }
}

#[test]
fn g13_g15_types_and_exact_token_views() {
    let syntax = accepted("&&Box<Box<i32>>", EntryPoint::Type);
    assert_eq!(
        syntax.nodes[syntax.root].kind,
        NodeKind::ReferenceType { mutable: false }
    );
    let inner = syntax.nodes[syntax.root].children[0];
    assert_eq!(
        syntax.nodes[inner].kind,
        NodeKind::ReferenceType { mutable: false }
    );
    let views: Vec<_> = syntax
        .views
        .iter()
        .map(|v| (v.span.start(), v.span.end(), v.span.bytes()))
        .collect();
    assert_eq!(views[0], (0, 1, b"&".as_slice()));
    assert_eq!(views[1], (1, 2, b"&".as_slice()));
    assert_eq!(views[7], (13, 14, b">".as_slice()));
    assert_eq!(views[8], (14, 15, b">".as_slice()));
    assert_eq!(syntax.views[0].token, syntax.views[1].token);
    assert_eq!(syntax.views[7].token, syntax.views[8].token);
    let callable = accepted("&(() => i32)", EntryPoint::Type);
    assert_eq!(
        callable.nodes[callable.root].kind,
        NodeKind::ReferenceType { mutable: false }
    );
    for (text, arity) in [("[]", 0), ("[i32]", 1), ("[i32, bool,]", 2)] {
        let syntax = accepted(text, EntryPoint::Type);
        assert_eq!(syntax.nodes[syntax.root].kind, NodeKind::TupleType);
        assert_eq!(syntax.nodes[syntax.root].children.len(), arity);
    }
    for text in [
        "_", "Box<>", "Box<T", "T extra", "(T,)", "()", "mut (T)", "T | U",
    ] {
        rejected(text, EntryPoint::Type);
    }
}

#[test]
fn all_question_expression_starts_ignore_trivia() {
    let starts = [
        "right",
        "1i32",
        "'c'",
        "\"text\"",
        "`text`",
        "true",
        "false",
        "(right)",
        "[right]",
        "if (c) a else b",
        "match (x) { _ => y }",
        "!right",
        "-right",
        "~right",
        "&right",
        "*right",
    ];
    assert_eq!(starts.len(), 16);
    for rhs in starts {
        for gap in ["", " ", "/* comment */", "\r\n// comment\r\n"] {
            let text = format!("left?{gap}{rhs}");
            let syntax = accepted(&text, EntryPoint::Expression);
            assert_eq!(syntax.nodes[syntax.root].kind, NodeKind::Wrap);
            assert_eq!(syntax.nodes[syntax.root].children.len(), 2);
            assert_preserved(&syntax);
        }
    }
    for text in [
        "value ? /* no RHS */",
        "value? // EOF",
        "value? /* gap */ && other",
        "value? == other",
    ] {
        let syntax = accepted(text, EntryPoint::Expression);
        assert!(syntax.nodes.iter().any(|n| n.kind == NodeKind::Propagate));
    }
    for text in [
        "value? /* gap */ ()",
        "value? !",
        "value? if (x) a",
        "value? match",
        "value? &",
    ] {
        rejected(text, EntryPoint::Expression);
    }
}

#[test]
fn generic_follow_set_and_nondestructive_views_with_trivia() {
    for text in [
        "f<T>(x)",
        "f<T> {}",
        "f<T>.V",
        "[f<T>, x]",
        "g(f<T>)",
        "[f<T>]",
        "Point{x:f<T>}",
        "() => f<T>",
        "f<T>?",
        "f<T> |> g",
        "if (x) f<T> else g",
        "`${f<T>}`",
        "match (x) { _ if f<T> => y }",
        "f /*a*/ < /*b*/ Box < T > /*c*/ > /*d*/ (x)",
    ] {
        let syntax = accepted(text, EntryPoint::Expression);
        assert!(
            syntax
                .nodes
                .iter()
                .any(|n| n.kind == NodeKind::TypeArguments)
        );
        assert_preserved(&syntax);
    }
    for text in ["f<T>;", "f<T> = x;"] {
        let syntax = accepted(text, EntryPoint::Module);
        assert!(
            syntax
                .nodes
                .iter()
                .any(|n| n.kind == NodeKind::TypeArguments)
        );
        assert_preserved(&syntax);
    }
    let error = rejected("f<T>:x", EntryPoint::Expression);
    assert_eq!(error.span.bytes(), b":"); // ':' follows a committed generic root.
    for text in [
        "a < b >> c",
        "a < b >= c",
        "a < /*c*/ b /*d*/ >> c",
        "f<T> -x",
    ] {
        let syntax = accepted(text, EntryPoint::Expression);
        assert!(
            !syntax
                .nodes
                .iter()
                .any(|n| n.kind == NodeKind::TypeArguments)
        );
        assert_preserved(&syntax);
    }
    for text in ["(f<T>) + x", "(f<T>) > x", "f<Unknown>.Unknown(x)"] {
        accepted(text, EntryPoint::Expression); // No resolution or expected types select a retry.
    }
    for text in [
        "let x: Box<T>=v;",
        "let x: Box<Box<T>>=v;",
        "let x: A<B<C<T>>>=v;",
        "let x: A<B<T> /*gap*/ > =v;",
    ] {
        assert_preserved(&accepted(text, EntryPoint::Module));
    }
    for text in [
        "a > /* not a shift */ > b",
        "a & /* not Boolean-and */ &",
        "f< T > + x",
    ] {
        rejected(text, EntryPoint::Expression);
    }
}

#[test]
fn complete_operator_and_list_shapes() {
    for operator in [
        "=", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "<<=", ">>=",
    ] {
        let text = format!("target {operator} value;");
        let syntax = accepted(&text, EntryPoint::Module);
        let statement = &syntax.nodes[syntax.nodes[syntax.root].children[0]];
        assert_eq!(statement.kind, NodeKind::Assignment(operator));
    }
    for operator in [
        "||", "&&", "|", "^", "&", "==", "!=", "<", "<=", ">", ">=", "<<", ">>", "+", "-", "*",
        "/", "%",
    ] {
        let text = format!("a {operator} b {operator} c");
        let syntax = accepted(&text, EntryPoint::Expression);
        let root = &syntax.nodes[syntax.root];
        assert_eq!(root.kind, NodeKind::Binary(operator));
        assert_eq!(
            syntax.nodes[root.children[0]].kind,
            NodeKind::Binary(operator)
        );
    }
    let syntax = accepted(
        "a || b && c | d ^ e & f == g < h << i + j * k",
        EntryPoint::Expression,
    );
    let mut node = syntax.root;
    for operator in ["||", "&&", "|", "^", "&", "==", "<", "<<", "+", "*"] {
        assert_eq!(syntax.nodes[node].kind, NodeKind::Binary(operator));
        node = syntax.nodes[node].children[1];
    }
    for text in [
        "& &value",
        "&mut *value",
        "!~-value",
        "functionValue(a, b,)[index].field?",
        "if (a) if (b) x else y else z",
    ] {
        assert_preserved(&accepted(text, EntryPoint::Expression));
    }
    for text in [
        "once () => mut (T,) => &U",
        "(T)",
        "(T) => (U) => V",
        "[T,]",
        "Box<(T) => U,>",
        "& mut &T",
    ] {
        assert_preserved(&accepted(text, EntryPoint::Type));
    }
    for text in [
        "f(,)",
        "[x,,]",
        "T<,>",
        "type T = {;}",
        "enum E {,}",
        "function f<T,,>(): U;",
    ] {
        rejected(text, EntryPoint::Module);
    }
}

#[test]
fn resource_limits_progress_and_determinism() {
    let source = snapshot("f<Box<Box<T>>>(x)");
    let lexed = lex(&source);
    let result = parse(
        &source,
        &lexed,
        EntryPoint::Expression,
        ParseLimits::default(),
    );
    assert_eq!(
        result,
        parse(
            &source,
            &lexed,
            EntryPoint::Expression,
            ParseLimits::default()
        )
    );
    for max_work in [0, 1, 4, 16] {
        assert_eq!(
            parse(
                &source,
                &lexed,
                EntryPoint::Expression,
                ParseLimits {
                    max_work,
                    ..ParseLimits::default()
                }
            )
            .unwrap_err()
            .kind,
            FailureKind::ResourceLimit
        );
    }
    let mut exhausted = false;
    for max_work in 0..256 {
        match parse(
            &source,
            &lexed,
            EntryPoint::Expression,
            ParseLimits {
                max_work,
                ..ParseLimits::default()
            },
        ) {
            Err(error) => {
                assert!(!exhausted);
                assert_eq!(error.kind, FailureKind::ResourceLimit);
            }
            Ok(syntax) => {
                exhausted = true;
                assert_eq!(syntax, result.clone().unwrap());
            }
        }
    }
    assert!(exhausted);
    for (entry, text) in [
        (EntryPoint::Type, format!("{}T", "&".repeat(10_000))),
        (
            EntryPoint::Expression,
            format!("{}x{}", "(".repeat(10_000), ")".repeat(10_000)),
        ),
        (
            EntryPoint::Pattern,
            format!("{}x{}", "[".repeat(10_000), "]".repeat(10_000)),
        ),
        (
            EntryPoint::Module,
            format!("{}{}", "{".repeat(10_000), "}".repeat(10_000)),
        ),
        (EntryPoint::Expression, format!("{}x", "!".repeat(10_000))),
        (
            EntryPoint::Expression,
            format!("{}x", "() => ".repeat(10_000)),
        ),
    ] {
        let source = snapshot(text);
        let error = parse(
            &source,
            &lex(&source),
            entry,
            ParseLimits {
                max_nesting: usize::MAX,
                ..ParseLimits::default()
            },
        )
        .unwrap_err();
        assert_eq!(error.kind, FailureKind::ResourceLimit);
    }
    let long = std::iter::repeat_n("x", 10_000)
        .collect::<Vec<_>>()
        .join(" + ");
    let syntax = accepted(&long, EntryPoint::Expression);
    assert_eq!(syntax.nodes[syntax.root].span.end(), long.len());
    drop(syntax); // Flat arena destruction must not recurse along the left chain.
}

fn assert_preserved(syntax: &Syntax) {
    let id = syntax.snapshot.id();
    assert_eq!(syntax.lexed, lex(&syntax.snapshot));
    let mut view = 0;
    for (index, token) in syntax.lexed.tokens.iter().enumerate() {
        let mut end = token.span.start();
        assert_eq!(syntax.views[view].token, index);
        while view < syntax.views.len() && syntax.views[view].token == index {
            let span = &syntax.views[view].span;
            assert_eq!(span.snapshot().id(), id);
            assert_eq!(span.start(), end);
            assert!(span.end() <= token.span.end());
            end = span.end();
            view += 1;
        }
        assert_eq!(end, token.span.end());
    }
    assert_eq!(view, syntax.views.len());
    let mut parents = vec![0; syntax.nodes.len()];
    for (index, node) in syntax.nodes.iter().enumerate() {
        assert_eq!(node.span.snapshot().id(), id);
        for &child in &node.children {
            assert!(child < index);
            assert!(node.span.start() <= syntax.nodes[child].span.start());
            assert!(node.span.end() >= syntax.nodes[child].span.end());
            parents[child] += 1;
        }
    }
    for (index, parents) in parents.into_iter().enumerate() {
        assert_eq!(
            parents,
            usize::from(index != syntax.root),
            "orphaned or shared syntax node {index}"
        );
    }
}

#[test]
fn all_truncations_and_generated_inputs_terminate_truthfully() {
    for (entry, text) in [
        (
            EntryPoint::Module,
            "function f<T>(x: T): T { let y = Point { x }; return y; }",
        ),
        (
            EntryPoint::Expression,
            "(match (x) { V if ((a) => test(a))(x) => `v ${x |> f(_)}`, _ => y })",
        ),
        (EntryPoint::Type, "(&mut (Box<[T, U]>) => once () => R)"),
        (
            EntryPoint::Pattern,
            "(Packet<Box<T>>.Data({ child: [_, -(128i8)] }))",
        ),
    ] {
        assert_preserved(&accepted(text, entry));
        for end in 1..text.len() {
            assert!(
                parse_text(&text[..end], entry).is_err(),
                "accepted truncation {end}: {:?}",
                &text[..end]
            );
        }
    }
    let alphabet = [
        "x", "T", "_", "function", "let", "if", "else", "match", "return", "expect", "(", ")", "{",
        "}", "[", "]", "<", ">", ">>=", "&&", "&", "=>", ":", ";", ",", "?", "|>", "=", "1i32",
        "\"}\"", "`x ${", "//", "/*", "-", ".", "...", " ", "\n",
    ];
    let mut seed = 0x52_u64;
    for length in 0..48 {
        for _ in 0..64 {
            let mut input = String::new();
            for _ in 0..length {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                input.push_str(alphabet[(seed >> 32) as usize % alphabet.len()]);
                input.push(' ');
            }
            let source = snapshot(&input);
            let lexed = lex(&source);
            for entry in [
                EntryPoint::Module,
                EntryPoint::Expression,
                EntryPoint::Type,
                EntryPoint::Pattern,
            ] {
                let limits = ParseLimits {
                    max_work: 4000,
                    ..ParseLimits::default()
                };
                let first = parse(&source, &lexed, entry, limits);
                assert_eq!(first, parse(&source, &lexed, entry, limits));
                if let Ok(syntax) = first {
                    assert_preserved(&syntax);
                }
            }
        }
    }
}

#[test]
fn exact_eof_errors_and_no_successful_prefixes() {
    for (text, entry, expected) in [
        ("(value", EntryPoint::Expression, ")"),
        ("Box<T", EntryPoint::Type, "type punctuation"),
        ("[value", EntryPoint::Pattern, "]"),
        ("let x = value", EntryPoint::Module, ";"),
        ("function f(): T {", EntryPoint::Module, "}"),
        ("(x) =>", EntryPoint::Expression, "expression"),
    ] {
        let source = snapshot(text);
        let error = parse(&source, &lex(&source), entry, ParseLimits::default()).unwrap_err();
        assert_eq!(error.kind, FailureKind::Syntax { expected });
        assert_eq!(error.span, source.eof());
    }
    for (text, entry) in [
        ("x y", EntryPoint::Expression),
        ("T U", EntryPoint::Type),
        ("_ other", EntryPoint::Pattern),
        ("let x = y; ;", EntryPoint::Module),
    ] {
        let error = rejected(text, entry);
        assert!(error.span.start() > 0);
        assert!(matches!(error.kind, FailureKind::Syntax { .. }));
    }
}

#[test]
fn resource_failure_before_input_validation_uses_supplied_snapshot() {
    let source = snapshot("this source is different");
    let other = snapshot("value");
    let lexed = lex(&other);
    let error = parse(
        &source,
        &lexed,
        EntryPoint::Expression,
        ParseLimits {
            max_work: 0,
            ..ParseLimits::default()
        },
    )
    .unwrap_err();
    assert_eq!(error.kind, FailureKind::ResourceLimit);
    assert_eq!(error.span, source.eof());
}

#[test]
fn mismatched_and_unavailable_lexical_input_cannot_succeed() {
    use compiler::lexer::{TokenKind, TriviaKind};
    let source = snapshot("/* trivia */ value");
    let original = lex(&source);
    let other = snapshot(source.bytes());
    let mut cases = Vec::new();
    let mut broken = original.clone();
    broken.tokens.clear();
    cases.push(broken);
    let mut broken = original.clone();
    broken.tokens.pop();
    cases.push(broken);
    let mut broken = original.clone();
    broken.tokens[0].span = other.span(13, 18).unwrap();
    cases.push(broken);
    let mut broken = original.clone();
    broken.trivia[0].span = other.span(0, 12).unwrap();
    cases.push(broken);
    let mut broken = original.clone();
    broken.tokens[0].kind = TokenKind::Eof;
    cases.push(broken);
    let mut broken = original.clone();
    broken.trivia.push(compiler::lexer::Trivia {
        kind: TriviaKind::Space,
        span: source.span(0, 1).unwrap(),
    });
    cases.push(broken);
    for lexed in cases {
        assert_eq!(
            parse(
                &source,
                &lexed,
                EntryPoint::Expression,
                ParseLimits::default()
            )
            .unwrap_err()
            .kind,
            FailureKind::InputMismatch
        );
    }
    let mut incomplete = original.clone();
    incomplete.status = LexStatus::Incomplete;
    assert_eq!(
        parse(
            &source,
            &incomplete,
            EntryPoint::Expression,
            ParseLimits::default()
        )
        .unwrap_err()
        .kind,
        FailureKind::Lexical
    );
    for bytes in [
        b"let x = y; \xff".as_slice(),
        b"//\xff",
        b"\"\xff\"",
        b"`${\xff}`",
    ] {
        let source = snapshot(bytes);
        assert_eq!(
            parse(
                &source,
                &lex(&source),
                EntryPoint::Module,
                ParseLimits::default()
            )
            .unwrap_err()
            .kind,
            FailureKind::Lexical
        );
    }
}

#[test]
fn original_unicode_spelling_trivia_and_decoded_provenance_survive() {
    let text = "\u{feff}let é = Point { e\u{301}: r#\"a\r\nb\"#, text: `\\${é} ${é}` };\r\n";
    let syntax = accepted(text, EntryPoint::Module);
    assert_preserved(&syntax);
    assert_eq!(syntax.nodes[syntax.root].span.bytes(), text.as_bytes());
    assert!(
        syntax
            .nodes
            .iter()
            .any(|n| n.kind == NodeKind::Name && n.span.bytes() == "é".as_bytes())
    );
    assert!(
        syntax
            .nodes
            .iter()
            .any(|n| n.kind == NodeKind::Name && n.span.bytes() == "e\u{301}".as_bytes())
    );
    let raw = syntax
        .lexed
        .tokens
        .iter()
        .find_map(|t| match &t.kind {
            compiler::lexer::TokenKind::Literal(value)
                if matches!(value.mode, compiler::lexer::LiteralMode::Raw { .. }) =>
            {
                Some(value)
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(raw.decoded[1].value, '\n');
    assert_eq!(raw.decoded[1].source.bytes(), b"\r\n");
    let template_text = syntax
        .lexed
        .tokens
        .iter()
        .find_map(|t| match &t.kind {
            compiler::lexer::TokenKind::Literal(value)
                if value.mode == compiler::lexer::LiteralMode::TemplateText =>
            {
                Some(value)
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(
        template_text.decoded[0].source,
        template_text.decoded[1].source
    );
    assert_eq!(template_text.decoded[0].source.bytes(), b"\\${");
}

#[test]
fn strict_source_and_completion_contract() {
    let source = snapshot("/* retained */\r\n");
    let lexed = lex(&source);
    let syntax = parse(&source, &lexed, EntryPoint::Module, ParseLimits::default()).unwrap();
    assert_eq!(syntax.snapshot.id(), source.id());
    assert_eq!(syntax.lexed, lexed);
    assert_eq!(syntax.nodes[syntax.root].kind, NodeKind::Module);
    assert_eq!(syntax.nodes[syntax.root].span.bytes(), source.bytes());
    assert!(syntax.nodes[syntax.root].children.is_empty());
    let other = snapshot(source.bytes());
    assert_eq!(
        parse(&other, &lexed, EntryPoint::Module, ParseLimits::default())
            .unwrap_err()
            .kind,
        FailureKind::InputMismatch
    );
    for text in ["\"unfinished", "123name", "/*"] {
        let source = snapshot(text);
        assert_ne!(lex(&source).status, LexStatus::Complete);
        assert_eq!(
            parse(
                &source,
                &lex(&source),
                EntryPoint::Module,
                ParseLimits::default()
            )
            .unwrap_err()
            .kind,
            FailureKind::Lexical
        );
    }
    for entry in [
        EntryPoint::Expression,
        EntryPoint::Type,
        EntryPoint::Pattern,
    ] {
        let error = parse_text("", entry).unwrap_err();
        assert!(matches!(error.kind, FailureKind::Syntax { .. }));
        assert_eq!(error.span.start(), 0);
        assert_eq!(error.span.end(), 0);
    }
}
