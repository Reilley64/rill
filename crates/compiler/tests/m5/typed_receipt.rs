use super::*;

fn receipts_in_both_modes(test: &str, source: &str) {
    installed(test, |request| {
        fs::write(&request.entry, source).unwrap();
        for test_mode in [false, true] {
            let request = InstalledRequest {
                test_mode,
                ..request.clone()
            };
            let graph = loader::load_installed(request).unwrap();
            let typed = analyze_typed(&graph, AnalysisLimits::default());
            assert!(
                typed.receipt().is_some(),
                "test_mode {test_mode}: {:?}, core {:?}",
                typed.completion(),
                typed.core_blockers()
            );
            assert_eq!(typed.core_blockers(), []);
        }
    });
}

#[test]
fn an_empty_program_gets_a_receipt() {
    receipts_in_both_modes("typed_receipt::an_empty_program_gets_a_receipt", "");
}

#[test]
fn a_program_with_no_imports_gets_a_receipt() {
    receipts_in_both_modes(
        "typed_receipt::a_program_with_no_imports_gets_a_receipt",
        "function f(): i32 { return 1i32; }\n",
    );
}
