//! All 47 authored G rows from docs/grammar/v0-syntax-v1.md, interpreted with
//! its EBNF. These assert syntax only; semantic rejection is not parser failure.
use NodeKind::*;
use compiler::lexer::lex;
use compiler::parser::{EntryPoint, FailureKind, NodeKind, ParseLimits, parse};
use compiler::source::Snapshot;

macro_rules! g_case {
    ($name:ident, $(($entry:ident, $text:expr, $expected:expr)),+ $(,)?) => {
        #[test]
        fn $name() {
            $(
                let source = Snapshot::capture("synthetic://G", $text.as_bytes()).unwrap();
                let entry = EntryPoint::$entry;
                let result = parse(&source, &lex(&source), entry, ParseLimits::default());
                let expected: Option<NodeKind> = $expected;
                match expected {
                    Some(kind) => {
                        let syntax = result.unwrap_or_else(|e| panic!("{}: {e:?}", $text));
                        let node = if entry == EntryPoint::Module {
                            syntax.nodes[syntax.root].children[0]
                        } else { syntax.root };
                        assert_eq!(syntax.nodes[node].kind, kind, "{}", $text);
                        assert_eq!(syntax.views.last().unwrap().span, source.eof());
                    }
                    None => assert!(matches!(result.unwrap_err().kind, FailureKind::Syntax { .. }), "{}", $text),
                }
            )+
        }
    };
}

g_case!(
    g01_grouped_import,
    (
        Module,
        "import geometry::{Point as P, origin,};",
        Some(Import)
    )
);
g_case!(
    g02_module_only_declarations,
    (Module, "function f(): void { import api; }", None),
    (
        Module,
        "function f(): void { implement f(): void {} }",
        None
    )
);
g_case!(
    g03_local_named_function,
    (
        Module,
        "function f(): void { function g(x: i32): i32 { return x; } }",
        Some(Function)
    )
);
g_case!(
    g04_comparison_against_shift,
    (Expression, "a < b >> c", Some(Binary("<")))
);
g_case!(
    g05_generic_call_commit,
    (Expression, "a < b > (c)", Some(Call))
);
g_case!(
    g06_grouped_comparison,
    (Expression, "(a < b) > (c)", Some(Binary(">")))
);
g_case!(
    g07_generic_function_value,
    (
        Module,
        "let converter = into<i64, i32>;",
        Some(Binding { mutable: false })
    )
);
g_case!(
    g08_nested_closers_and_assignment,
    (
        Module,
        "let item: Box<Box<i32>>=value;",
        Some(Binding { mutable: false })
    )
);
g_case!(
    g09_parent_applied_variant,
    (Expression, "Packet<i32>.Data(3i32)", Some(Call))
);
g_case!(
    g10_no_variant_owned_arguments,
    (Expression, "Packet.Data<i32>", None)
);
g_case!(
    g11_typed_arrow,
    (Expression, "(number: i32): i32 => number", Some(Arrow))
);
g_case!(
    g12_no_bare_arrow_head,
    (Expression, "number => number", None)
);
g_case!(
    g13_callable_and_nested_references,
    (Type, "&(() => i32)", Some(ReferenceType { mutable: false })),
    (Type, "&&string", Some(ReferenceType { mutable: false }))
);
g_case!(
    g14_expression_ampersands_do_not_split,
    (Expression, "&&value", None)
);
g_case!(
    g15_zero_and_one_tuple_types,
    (Type, "[i32]", Some(TupleType)),
    (Type, "[]", Some(TupleType))
);
g_case!(
    g16_uncontextualized_bracket_shape,
    (Module, "let value = [];", Some(Binding { mutable: false }))
);
g_case!(
    g17_empty_nominal_shapes,
    (Module, "enum Empty {}", Some(Enum)),
    (Module, "type Marker = {};", Some(Record))
);
g_case!(
    g18_no_structural_alias,
    (Module, "type Alias = Other;", None)
);
g_case!(
    g19_no_implementation_defaults,
    (
        Module,
        "implement<T = i32> f(value: T): T { return value; }",
        None
    )
);
g_case!(
    g20_nontrailing_default_is_well_formedness_not_parsing,
    (
        Module,
        "function f<T = i32, U>(value: U): U;",
        Some(Function)
    )
);
g_case!(
    g21_construction_shorthand,
    (Expression, "Point { x, y }", Some(Construction))
);
g_case!(
    g22_leading_rebuild,
    (Expression, "Point { ...base, x: 2 }", Some(Construction))
);
g_case!(
    g23_no_nonleading_spread,
    (Expression, "Point { x: 2, ...base }", None)
);
g_case!(
    g24_nested_record_pattern,
    (
        Module,
        "let { child: { value } } = record;",
        Some(Binding { mutable: false })
    )
);
g_case!(
    g25_binder_and_qualified_unit_pattern,
    (Pattern, "None", Some(NamePattern)),
    (Pattern, "Option.None", Some(NamePattern))
);
g_case!(
    g26_static_templates_only_in_patterns,
    (Pattern, "`a ${value}`", None),
    (Pattern, "`static`", Some(Template))
);
g_case!(
    g27_statement_if,
    (
        Module,
        "if (ready) { work(); } else { stop(); }",
        Some(IfStatement)
    )
);
g_case!(
    g28_grouped_expression_if_statement,
    (
        Module,
        "(if (ready) left else right);",
        Some(ExpressionStatement)
    )
);
g_case!(
    g29_statement_if_requires_blocks,
    (Module, "if (ready) left else right;", None)
);
g_case!(
    g30_control_target_is_not_parser_work,
    (Module, "return;", Some(Return))
);
g_case!(
    g31_wrap_versus_propagate_and_subtract,
    (Expression, "read()? - 1i32", Some(Wrap)),
    (Expression, "(read()?) - 1i32", Some(Binary("-")))
);
g_case!(
    g32_no_question_mark_retry,
    (Expression, "value?()", None),
    (Expression, "(value?)()", Some(Call))
);
g_case!(g33_nonassociative_wrap, (Expression, "a ? b ? c", None));
g_case!(
    g34_wrap_pipe_result,
    (Expression, "x |> f ? Failure {}", Some(Wrap))
);
g_case!(
    g35_direct_placeholder,
    (Expression, "x |> pad(10i32, _)", Some(Pipe))
);
g_case!(
    g36_no_nested_or_multiple_free_placeholders,
    (Expression, "x |> f([_])", None),
    (Expression, "x |> f(_, _)", None)
);
g_case!(
    g37_parenthesized_arrow_step,
    (Expression, "x |> ((value) => value)", Some(Pipe))
);
g_case!(
    g38_restricted_step_boundary,
    (Expression, "x |> f + y", None),
    (Expression, "(x |> f) + y", Some(Binary("+")))
);
g_case!(
    g39_annotated_guard_payload,
    (
        Module,
        "guard let item: Item = operation() else (error) { return; }",
        Some(Guard)
    )
);
g_case!(
    g40_expectation_nested_callable,
    (Module, "expect (() => { return true; })();", Some(Expect))
);
g_case!(
    g41_no_nested_expectation,
    (Module, "function f(): void { expect true; }", None)
);
g_case!(
    g42_closed_helpers_are_ordinary_call_shapes,
    (Expression, "Ok()", Some(Call)),
    (Expression, "Err<Success, ErrorType>(error)", Some(Call))
);
g_case!(
    g43_reserved_guard_arrow,
    (
        Expression,
        "match (value) { Variant if (ready) => answer }",
        Some(Match)
    )
);
g_case!(
    g44_nested_guard_arrow,
    (
        Expression,
        "match (value) { Variant if ((item) => test(item))(value) => answer }",
        Some(Match)
    )
);
g_case!(
    g45_separate_nested_placeholder_allowances,
    (Expression, "x |> f(y |> g(_), _)", Some(Pipe))
);
g_case!(
    g46_no_manufactured_assignment,
    (Expression, "a < b >= c", Some(Binary(">=")))
);
g_case!(
    g47_directly_negated_minimum_shape,
    (Pattern, "-(128i8)", Some(NegatedPattern))
);
