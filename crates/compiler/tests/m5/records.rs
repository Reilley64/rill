use super::*;

#[test]
fn owned_record_projections() {
    probe(
        "records::owned_record_projections",
        r#"
            import std::core::dispose as release;
            type Pack = { first: string; second: string; third: string; }
            function splitField(flag: bool, pack: Pack): string {
                if (flag) { let taken = pack.first; }
                return pack.second;
            }
            function movedField(flag: bool, pack: Pack): string {
                if (flag) { let taken = pack.first; }
                return pack.first;
            }
            function movedWhole(flag: bool, pack: Pack): Pack {
                if (flag) { let taken = pack.first; }
                return pack;
            }
            function construct(one: string, two: string, three: string): void {
                let pack = Pack { third: three, first: one, second: two };
            }
            function restoreField(seed: Pack): string {
                let mut pack = seed;
                let taken = pack.first;
                pack.first = "new";
                return taken;
            }
            function replaceField(seed: Pack): void {
                let mut pack = seed;
                pack.second = "new";
            }
            function disjointFields(seed: Pack): string {
                let mut pack = seed;
                let access = &mut pack.first;
                let taken = pack.second;
                let last = access;
                return taken;
            }
            function sameField(seed: Pack): void {
                let mut pack = seed;
                let access = &mut pack.first;
                pack.first = "new";
                let last = access;
            }
            function temporaryField(): string {
                return (Pack { first: "a", second: "b", third: "c" }).first;
            }
            function overlappingWhole(pack: Pack): string {
                let access = &pack;
                let taken = pack.second;
                let last = access;
                return taken;
            }
            function escapingField(pack: Pack): &string {
                return &pack.first;
            }
            function replacePartial(seed: Pack, next: Pack): string {
                let mut pack = seed;
                let taken = pack.first;
                pack = next;
                return pack.second;
            }
            type CustomPack = { text: string; remaining: string; }
            implement release(value: CustomPack): void { }
            function customPartial(pack: CustomPack): string { return pack.text; }
            function customConditional(flag: bool, pack: CustomPack): void {
                if (flag) { let before = true; let taken = pack.text; }
            }
            function customWhole(pack: CustomPack): CustomPack { return pack; }
            function customCleanup(pack: CustomPack): void { }
            function borrowedFieldMove(pack: &Pack): string { return pack.first; }
            type FlagPack = { bit: bool; }
            function copyRecord(pack: FlagPack): bool {
                let other = pack;
                let original = pack.bit;
                return other.bit;
            }
        "#,
        |request, module, receipt| {
            let lower = |name| {
                let id = &module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap()
                    .id;
                lower_body(receipt, module, id, AnalysisLimits::default()).unwrap()
            };
            let field_split = lower("splitField");
            let mir = field_split.mir().unwrap_or_else(|| {
                panic!(
                    "{:?}: {:?}",
                    field_split.completion(),
                    field_split
                        .blocked_at()
                        .map(|s| s.span.bytes().escape_ascii().to_string())
                )
            });
            let (yes, no) = branch_targets(mir);
            let drop_names = |start| {
                path_drops(mir, start)
                    .iter()
                    .map(|drop| match drop {
                        Obligation::Drop(place) => (
                            place.root.span.bytes(),
                            place.field().map(|field| field.name.as_deref().unwrap()),
                        ),
                        other => panic!("unexpected cleanup {other:?}"),
                    })
                    .collect::<Vec<_>>()
            };
            assert_eq!(
                drop_names(yes),
                [
                    (b"taken".as_slice(), None),
                    (b"pack: Pack".as_slice(), Some("third"))
                ]
            );
            assert_eq!(
                drop_names(no),
                [
                    (b"pack: Pack".as_slice(), Some("third")),
                    (b"pack: Pack".as_slice(), Some("first"))
                ]
            );
            for (name, use_site) in [
                ("movedField", b"pack.first".as_slice()),
                ("movedWhole", b"pack".as_slice()),
            ] {
                let rejected = lower(name);
                assert_eq!(rejected.completion(), Completion::SourceRejected);
                assert!(rejected.mir().is_none());
                let [finding] = rejected.findings() else {
                    panic!("expected one finding")
                };
                assert_eq!(finding.code(), "R0200");
                assert_eq!(finding.site.span.bytes(), use_site);
                assert_eq!(finding.related.span.bytes(), b"pack.first");
            }
            let constructed = lower("construct");
            let mir = constructed
                .mir()
                .unwrap_or_else(|| panic!("{:?}", constructed.completion()));
            let fields: Vec<_> = mir
                .blocks
                .iter()
                .filter_map(|b| match b.operation {
                    Operation::Move {
                        into: Some(place), ..
                    } => place.field().and_then(|field| field.name.as_deref()),
                    _ => None,
                })
                .collect();
            assert_eq!(
                fields,
                ["third", "first", "second"],
                "evaluate fields as written"
            );
            assert_eq!(
                dropped_fields(mir),
                ["third", "second", "first"],
                "clean in reverse declaration order"
            );
            let restored = lower("restoreField");
            let mir = restored
                .mir()
                .unwrap_or_else(|| panic!("{:?}", restored.completion()));
            assert_eq!(
                dropped_fields(mir),
                ["third", "second", "first"],
                "do not drop the moved old field again"
            );
            let replaced = lower("replaceField");
            let mir = replaced
                .mir()
                .unwrap_or_else(|| panic!("{:?}", replaced.completion()));
            assert_eq!(dropped_fields(mir), ["second", "third", "second", "first"]);
            assert!(mir.blocks.iter().any(|block| matches!(( &block.operation, block.terminator), (Operation::Drop(place), Terminator::Goto(next))
            if place.field().is_some_and(|f| f.name.as_deref() == Some("second"))
                && matches!(mir.blocks[next].operation, Operation::Install { target, .. } if target.field().is_some_and(|f| f.name.as_deref() == Some("second"))))));
            let disjoint = lower("disjointFields");
            let mir = disjoint
                .mir()
                .unwrap_or_else(|| panic!("{:?}", disjoint.completion()));
            assert_eq!(dropped_fields(mir), ["third", "first"]);
            let replaced = lower("replacePartial");
            let mir = replaced
                .mir()
                .unwrap_or_else(|| panic!("{:?}", replaced.completion()));
            assert_eq!(dropped_fields(mir), ["third", "second", "third", "first"]);
            let temporary = lower("temporaryField");
            let mir = temporary
                .mir()
                .unwrap_or_else(|| panic!("{:?}", temporary.completion()));
            assert_eq!(dropped_fields(mir), ["third", "second"]);
            assert!(mir.blocks.iter().any(
                |b| matches!(b.operation, Operation::Move { from, into: Some(_) }
                if from.field().is_some_and(|field| field.name.as_deref() == Some("first")))
            ));
            let copy = lower("copyRecord");
            let mir = copy
                .mir()
                .unwrap_or_else(|| panic!("{:?}", copy.completion()));
            assert!(mir.blocks.iter().any(
                |b| matches!(b.operation, Operation::Copy { from, into: Some(into) }
                if from.projection.is_none() && from.root.span.bytes() == b"pack: FlagPack"
                    && into.root.span.bytes() == b"other")
            ));
            assert!(path_drops(mir, mir.entry).is_empty());
            let same = lower("sameField");
            assert_eq!(same.completion(), Completion::SourceRejected);
            assert!(same.mir().is_none());
            let [finding] = same.findings() else {
                panic!("expected one finding")
            };
            assert_eq!(finding.code(), "R0203");
            assert_eq!(finding.related.span.bytes(), b"&mut pack.first");
            let transferred = lower("customWhole");
            let mir = transferred
                .mir()
                .unwrap_or_else(|| panic!("{:?}", transferred.completion()));
            assert!(path_drops(mir, mir.entry).is_empty());
            let checked = lower("customCleanup");
            let mir = checked
                .mir()
                .expect("checked outer disposer and default field cleanup");
            let operations = path_drops(mir, mir.entry);
            let [Obligation::InvokeDisposer { implementation, .. }] = operations.as_slice() else {
                panic!("expected only an outer disposer at the caller boundary: {operations:?}");
            };
            let active = mir.disposer(implementation).unwrap();
            assert_eq!(dropped_fields(active), ["remaining", "text"]);

            for (name, code) in [
                ("overlappingWhole", "R0203"),
                ("escapingField", "R0202"),
                ("customPartial", "R0501"),
                ("customConditional", "R0501"),
                ("borrowedFieldMove", "R0501"),
            ] {
                let report = lower(name);
                assert_eq!(report.completion(), Completion::SourceRejected, "{name}");
                assert_eq!(report.findings()[0].code(), code, "{name}");
                assert!(report.mir().is_none());
            }

            let duplicate = request.entry.with_file_name("duplicate.rill");
            let mut source = fs::read_to_string(&request.entry).unwrap();
            source.push_str("\nimplement dispose(other: CustomPack): void { }\n");
            fs::write(&duplicate, source).unwrap();
            let graph = loader::load_installed(InstalledRequest {
                entry: duplicate,
                ..request.clone()
            })
            .unwrap();
            let typed = analyze_typed(&graph, AnalysisLimits::default());
            assert!(typed.receipt().is_none());
            assert!(
                typed
                    .bodies()
                    .findings()
                    .iter()
                    .any(|f| f.kind.code() == Some("R0304"))
            );
        },
    );
}

fn dropped_fields<'g>(mir: &Mir<'g>) -> Vec<&'g str> {
    path_drops(mir, mir.entry)
        .into_iter()
        .filter_map(|drop| match drop {
            Obligation::Drop(place) => place.field().and_then(|field| field.name.as_deref()),
            _ => None,
        })
        .collect()
}
