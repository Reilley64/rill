use super::*;

#[test]
fn owned_tuple_binding_transfers_and_cleans_initialized_elements() {
    probe(
        "patterns::owned_tuple_binding_transfers_and_cleans_initialized_elements",
        r#"
            function pass(value: [string, string, string, string]): [string, string, string, string] {
                return value;
            }
            function unpack(value: [string, string, string, string]): void {
                let [first, _, third, _] = pass(value);
            }
            function reuse(value: [string, string]): string {
                let [first, _] = value;
                return value[0];
            }
            function alreadyPartial(value: [string, string]): string {
                let earlier = value[1];
                let [first, _] = value;
                return first;
            }
            function copyPair(value: [bool, bool]): bool {
                let [first, _] = value;
                if (first) { return value[0]; }
                return value[1];
            }
            function forward(value: [&bool, string]): &bool {
                let [saved, _] = value;
                return saved;
            }
            function expires(flag: bool): &bool {
                let [saved, _]: [&bool, string] = [&flag, "tag"];
                return saved;
            }
        "#,
        |_, module, receipt| {
            let declaration = |name| {
                module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap()
            };
            let lower = |name| {
                lower_body(
                    receipt,
                    module,
                    &declaration(name).id,
                    AnalysisLimits::default(),
                )
                .unwrap()
            };
            let report = lower("unpack");
            let mir = report
                .mir()
                .unwrap_or_else(|| panic!("unpack: {:?}", report.completion()));
            let events: Vec<_> = mir
                .blocks
                .iter()
                .flat_map(|block| match &block.operation {
                    Operation::Call(_) => vec![(b"call".as_slice(), None)],
                    Operation::Move {
                        from,
                        into: Some(to),
                    } if from.tuple_element().is_some() => {
                        vec![(to.root.span.bytes(), from.tuple_element())]
                    }
                    Operation::Cleanup(cleanups) => cleanups
                        .iter()
                        .filter_map(|op| match op {
                            Obligation::Drop(place) => {
                                Some((place.root.span.bytes(), place.tuple_element()))
                            }
                            _ => None,
                        })
                        .collect(),
                    _ => Vec::new(),
                })
                .collect();
            assert_eq!(
                events,
                [
                    (b"call".as_slice(), None),
                    (b"first", Some(0)),
                    (b"third", Some(2)),
                    (b"pass(value)", Some(3)),
                    (b"pass(value)", Some(1)),
                    (b"third", None),
                    (b"first", None),
                ]
            );

            let limited = lower_body(
                receipt,
                module,
                &declaration("unpack").id,
                AnalysisLimits {
                    max_work: report.statistics().work - 1,
                    ..AnalysisLimits::default()
                },
            )
            .unwrap();
            assert_eq!(limited.completion(), Completion::ResourceLimit);
            assert!(limited.mir().is_none());

            for (name, site, source, code) in [
                (
                    "reuse",
                    b"value[0]".as_slice(),
                    b"value".as_slice(),
                    "R0200",
                ),
                (
                    "alreadyPartial",
                    b"value".as_slice(),
                    b"value[1]".as_slice(),
                    "R0200",
                ),
                (
                    "expires",
                    b"return saved;".as_slice(),
                    b"flag: bool".as_slice(),
                    "R0202",
                ),
            ] {
                let report = lower(name);
                assert_eq!(report.completion(), Completion::SourceRejected, "{name}");
                assert!(report.mir().is_none());
                let [finding] = report.findings() else {
                    panic!("{name}: expected one finding")
                };
                assert_eq!(finding.code(), code, "{name}");
                assert_eq!(finding.site.span.bytes(), site, "{name}");
                assert_eq!(finding.related.span.bytes(), source, "{name}");
            }

            let report = lower("copyPair");
            let mir = report.mir().expect("copyable tuple stays initialized");
            assert!(mir.blocks.iter().any(
                |b| matches!(b.operation, Operation::Copy { from, into: Some(to) }
                if from.tuple_element() == Some(0) && to.root.span.bytes() == b"first")
            ));
            let report = lower("forward");
            let mir = report
                .mir()
                .expect("return caller backing, not a loan of the scratch tuple");
            assert!(
                matches!(path_drops(mir, mir.entry).as_slice(), [Obligation::Drop(place)]
                if place.tuple_element() == Some(1))
            );
        },
    );
}
