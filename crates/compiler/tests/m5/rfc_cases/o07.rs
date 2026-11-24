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
