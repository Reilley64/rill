//! Exercise source-to-residence transitions with an abstract watched declaration.
//! This does not attach any first-party role or test authority admission.
use super::*;
use crate::analysis::{
    AnalysisLimits,
    ordinary::analyze_typed_inventory,
    scalars::generics::{IntegratedInputs, analyze_integrated_bodies},
};
use crate::loader::{self, Limits, Prelude, Request, Selector};
use std::fs;

#[test]
fn typed_sources_preserve_origins_across_rename_projection_replacement_and_paths() {
    let root = std::env::temp_dir().join(format!("rill-result-use-flow-{}", std::process::id()));
    let app = root.join("app");
    let standard = root.join("std");
    fs::create_dir_all(&app).unwrap();
    fs::create_dir_all(&standard).unwrap();
    let entry = app.join("main.rill");
    fs::write(&entry, "
        enum Watched { Good(bool), Bad(bool) }
        function renamed(input: Watched): void { let renamed = input; }
        function projected(left: Watched, right: Watched): Watched {
            let pair: [Watched, Watched] = [left, right];
            return pair[0];
        }
        function replaced(old: Watched, new: Watched): void {
            let mut local = old;
            local = new;
            let _ = local;
        }
        function branching(used: bool, later: Watched): void {
            if (used) { let _ = later; }
        }
        function borrowed(owned: Watched): void { let temporary = &owned; }
        function observed(outcome: Watched): void { match (outcome) { Watched.Good(_) => {}, Watched.Bad(_) => {} }; }
        function checkedBoundary(argument: Watched): void { let _ = argument; }
        function forwarding(outcome: Watched): void { checkedBoundary(outcome); }
        function generated(): Watched { return Watched.Good(true); }
        function discardCall(): void { generated(); }
        function loopBreak(run: bool): void { while (run) { let held = generated(); break; } }
        function loopContinue(run: bool): void { while (run) { let held = generated(); continue; } }
        function diverges(infinite: Watched): never { while (true) {} }
    ").unwrap();
    fs::write(
        standard.join("core.rill"),
        "export function copy<T>(value: &T): T; export function dispose<T>(value: T): void;",
    )
    .unwrap();
    let select = |name: &str| Selector {
        module: "core.rill".into(),
        name: name.into(),
    };
    let loaded = loader::load(Request {
        entry,
        standard_root: standard,
        prelude: Prelude {
            copy: select("copy"),
            dispose: select("dispose"),
        },
        target: "aarch64-apple-darwin".into(),
        options: Default::default(),
        test_mode: false,
        limits: Limits::default(),
    });
    fs::remove_dir_all(root).unwrap();
    let graph = loaded.unwrap();
    let limits = AnalysisLimits::default();
    let inventory = analyze_typed_inventory(&graph, limits);
    let budget = Budget {
        limits,
        statistics: inventory.statistics(),
    };
    let bodies = analyze_integrated_bodies(&inventory, budget, IntegratedInputs::default());
    assert!(bodies.findings().is_empty(), "{:?}", bodies.findings());
    let mut budget = Budget {
        limits,
        statistics: bodies.statistics(),
    };
    let watched = graph
        .modules()
        .iter()
        .flat_map(|m| &m.declarations)
        .find(|d| d.name.as_deref() == Some("Watched"))
        .unwrap();
    let variant = |name| {
        &graph
            .modules()
            .iter()
            .flat_map(|m| &m.declarations)
            .find(|d| d.id.kind == DeclarationKind::Variant && d.name.as_deref() == Some(name))
            .unwrap()
            .id
    };
    let mut report = ResultUseReport::default();
    check(
        &inventory,
        &bodies,
        Inputs {
            result: Container {
                declaration: &watched.id,
                success: variant("Good"),
                failure: variant("Bad"),
            },
            option: None,
            opaque_cause: None,
            owned_bytes: None,
            collections: Default::default(),
            native: &Default::default(),
            outcome_observer: None,
        },
        &mut budget,
        &mut report,
    )
    .unwrap();
    assert!(report.pending().is_empty(), "{:?}", report.pending());
    let mut origins: Vec<_> = report
        .discards()
        .iter()
        .map(|finding| {
            assert_eq!(finding.code(), "R0103");
            let module = graph
                .modules()
                .iter()
                .find(|module| &module.key == finding.origin.module)
                .unwrap();
            module
                .declarations
                .iter()
                .find(|d| d.node == finding.origin.node)
                .and_then(|d| d.name.as_deref())
                .unwrap_or_else(|| std::str::from_utf8(finding.origin.span.bytes()).unwrap())
        })
        .collect();
    origins.sort_unstable();
    assert_eq!(
        origins,
        [
            "generated()",
            "generated()",
            "generated()",
            "input",
            "later",
            "old",
            "owned",
            "right"
        ]
    );
    for site in [b"break;".as_slice(), b"continue;".as_slice()] {
        let witness = report
            .discards()
            .iter()
            .find(|w| w.discard.span.bytes() == site)
            .unwrap();
        assert_eq!(witness.kind, ResultDiscardKind::ScopeExit);
        assert!(witness.storage.is_some());
    }
    let replacement = report
        .discards()
        .iter()
        .find(|witness| witness.kind == ResultDiscardKind::Replacement)
        .unwrap();
    assert_eq!(replacement.discard.span.bytes(), b"local = new;");
}
