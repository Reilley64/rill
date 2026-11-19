use super::*;

#[test]
fn owned_record_binding_transfers_fields_and_cleans_the_remainder() {
    probe(
        "patterns::owned_record_binding_transfers_fields_and_cleans_the_remainder",
        r#"
            type Packet = { left: string; skip: string; right: string; omit: string; };
            function pass(value: Packet): Packet { return value; }
            function unpack(value: Packet): string {
                let (Packet { right: (last), skip: _, left }) = pass(value);
                return last;
            }
            function reuse(value: Packet): string {
                let Packet { left: first } = value;
                return value.left;
            }
            function alreadyPartial(value: Packet): string {
                let old = value.omit;
                let Packet { left: first } = value;
                return first;
            }
            type Ref = { carried: &bool; tag: string; };
            function pointsOutside(value: Ref): &bool {
                let Ref { carried } = value;
                return carried;
            }
            function pointsInside(flag: bool): &bool {
                let Ref { carried: saved } = Ref { carried: &flag, tag: "tag" };
                return saved;
            }
            function liveBacking(seed: &bool): bool {
                let mut out = seed;
                {
                    let flag = true;
                    let Ref { carried: saved } = Ref { carried: &flag, tag: "tag" };
                    out = saved;
                }
                return *out;
            }
            type Flags = { selected: bool; answer: bool; };
            function copyable(value: Flags): bool {
                let Flags { selected: bit } = value;
                return value.answer;
            }
            type Nested = { coordinates: [bool, bool]; };
            function nested(value: Nested): bool {
                let Nested { coordinates: [x, _] } = value;
                return x;
            }
            function borrowed(value: &Packet): void {
                let Packet { left: first } = value;
            }
            type Custom = { text: string; };
            implement dispose(value: Custom): void {}
            function custom(value: Custom): string {
                let Custom { text } = value;
                return text;
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
            let check =
                |name, limits| lower_body(receipt, module, &declaration(name).id, limits).unwrap();
            let lower = |name| check(name, AnalysisLimits::default());
            let field = |name| {
                module
                    .declarations
                    .iter()
                    .find(|d| {
                        d.id.kind == loader::DeclarationKind::Field
                            && d.name.as_deref() == Some(name)
                    })
                    .unwrap()
            };
            let report = lower("unpack");
            let mir = report.mir().expect("flat record binding is checked");
            let mut events = Vec::new();
            let mut scratch = None;
            let mut result = None;
            let mut bound = None;
            let mut returned = false;
            let mut pc = mir.entry;
            for _ in 0..mir.blocks.len() {
                let block = &mir.blocks[pc];
                match &block.operation {
                    Operation::Call(callee) => {
                        assert_eq!(*callee, &declaration("pass").id);
                        scratch = Some(block.site.node);
                        events.push(("initialize", None));
                    }
                    Operation::Move {
                        from,
                        into: Some(to),
                    } if let Some(member) = from.field() => {
                        assert_eq!(Some(from.root.node), scratch);
                        let name = std::str::from_utf8(to.root.span.bytes()).unwrap();
                        if name == "last" {
                            bound = Some(to.root.node);
                        }
                        events.push((name, Some(&member.id)));
                    }
                    Operation::Move {
                        from,
                        into: Some(to),
                    } if Some(from.root.node) == bound && bound.is_some() => {
                        result = Some(to.root.node);
                        events.push(("stage return", None));
                    }
                    Operation::Export(output) => {
                        assert_eq!(Some(output.root.node), result);
                        events.push(("export", None));
                    }
                    Operation::Cleanup(obligations) => {
                        for obligation in obligations {
                            if let Obligation::Drop(place) = obligation {
                                if let Some(member) = place.field() {
                                    assert_eq!(Some(place.root.node), scratch);
                                    events.push(("remaining", Some(&member.id)));
                                } else {
                                    assert_eq!(place.root.span.bytes(), b"left");
                                    events.push(("local", None));
                                }
                            }
                        }
                    }
                    _ => {}
                }
                match block.terminator {
                    Terminator::Goto(next) => pc = next,
                    Terminator::Return => {
                        returned = true;
                        break;
                    }
                    _ => panic!("expected a straight binding path"),
                }
            }
            assert!(returned, "the path must reach Return");
            assert_eq!(
                events,
                [
                    ("initialize", None),
                    ("last", Some(&field("right").id)),
                    ("left", Some(&field("left").id)),
                    ("remaining", Some(&field("omit").id)),
                    ("remaining", Some(&field("skip").id)),
                    ("stage return", None),
                    ("export", None),
                    ("local", None),
                ]
            );
            let limits = AnalysisLimits {
                max_work: report.statistics().work,
                max_entries: report.statistics().entries,
                ..AnalysisLimits::default()
            };
            assert_eq!(check("unpack", limits).completion(), Completion::Checked);
            for short in [
                AnalysisLimits {
                    max_work: limits.max_work - 1,
                    ..limits
                },
                AnalysisLimits {
                    max_entries: limits.max_entries - 1,
                    ..limits
                },
            ] {
                let limited = check("unpack", short);
                assert_eq!(limited.completion(), Completion::ResourceLimit);
                assert!(limited.mir().is_none());
            }

            let carried = lower("pointsOutside");
            let mir = carried
                .mir()
                .expect("copying a carried field does not borrow the pattern temporary");
            let ops = linear(mir, mir.entry);
            let route: Vec<_> = ops
                .iter()
                .copied()
                .filter(|op| matches!(op, Operation::Copy { .. } | Operation::Export(_)))
                .collect();
            assert!(matches!(route.as_slice(), [
                Operation::Copy { from: member, into: Some(bound) },
                Operation::Copy { from: read, into: Some(output) },
                Operation::Export(exported),
            ] if member.field().is_some_and(|member| member.id == field("carried").id)
                && bound.root.span.bytes() == b"carried" && bound.root.node == read.root.node
                && output.root.node == exported.root.node));
            assert!(
                matches!(path_drops(mir, mir.entry).as_slice(), [Obligation::Drop(place)]
                if place.field().is_some_and(|member| member.id == field("tag").id))
            );

            let copyable = lower("copyable");
            let mir = copyable
                .mir()
                .expect("an owned copyable record remains readable");
            let route: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::Copy { .. } | Operation::Move { .. } | Operation::Export(_)
                    )
                })
                .collect();
            assert!(matches!(route.as_slice(), [
                Operation::Copy { from: original, into: Some(scratch) },
                Operation::Copy { from: selected, into: Some(bit) },
                Operation::Copy { from: answer, into: Some(output) },
                Operation::Export(exported),
            ] if original.root.span.bytes() == b"value: Flags" && selected.root.node == scratch.root.node
                && selected.field().is_some_and(|member| member.id == field("selected").id)
                && bit.root.span.bytes() == b"bit" && answer.root.node == original.root.node
                && answer.field().is_some_and(|member| member.id == field("answer").id)
                && output.root.node == exported.root.node));

            let live = lower("liveBacking");
            assert_eq!(live.completion(), Completion::SourceRejected);
            assert!(live.mir().is_none());
            let [finding] = live.findings() else {
                panic!("expected live carried backing conflict")
            };
            assert_eq!(finding.code(), "R0203");
            assert_eq!(finding.related.span.bytes(), b"&flag");
            assert!(
                finding
                    .site
                    .span
                    .bytes()
                    .starts_with(b"{\n                    let flag")
            );
            for (name, code, at, cause) in [
                ("reuse", "R0200", "value.left", "value"),
                ("alreadyPartial", "R0200", "value", "value.omit"),
                ("pointsInside", "R0202", "return saved;", "flag: bool"),
            ] {
                let report = lower(name);
                assert_eq!(report.completion(), Completion::SourceRejected, "{name}");
                assert!(report.mir().is_none(), "{name}");
                let [finding] = report.findings() else {
                    panic!("{name}: expected one finding")
                };
                assert_eq!(finding.code(), code, "{name}");
                assert_eq!(
                    std::str::from_utf8(finding.site.span.bytes()),
                    Ok(at),
                    "{name} primary"
                );
                assert_eq!(
                    std::str::from_utf8(finding.related.span.bytes()),
                    Ok(cause),
                    "{name} related"
                );
            }
            for name in ["borrowed", "custom", "nested"] {
                let incomplete = lower(name);
                assert_eq!(incomplete.completion(), Completion::Incomplete, "{name}");
                assert!(incomplete.mir().is_none(), "{name}");
            }
        },
    );
}

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
            function grouped(value: [string, string]): string {
                let (([((first)), (_)])) = value;
                return first;
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
            function liveAlias(p: &bool): bool {
                let mut out = p;
                { let x = true; let [r, _]: [&bool, string] = [&x, "s"]; out = r; }
                return *out;
            }
            function deadAlias(p: &bool): bool {
                let mut out = p;
                { let x = true; let [r, _]: [&bool, string] = [&x, "s"]; out = r; }
                return false;
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
            let mut events = Vec::new();
            let mut block = mir.entry;
            for _ in 0..mir.blocks.len() {
                let step = &mir.blocks[block];
                match &step.operation {
                    Operation::Call(_) => events.push((b"call".as_slice(), None)),
                    Operation::Move {
                        from,
                        into: Some(to),
                    } if from.tuple_element().is_some() => {
                        events.push((to.root.span.bytes(), from.tuple_element()));
                    }
                    Operation::Cleanup(cleanups) => {
                        events.extend(cleanups.iter().filter_map(|op| match op {
                            Obligation::Drop(place) => {
                                Some((place.root.span.bytes(), place.tuple_element()))
                            }
                            _ => None,
                        }))
                    }
                    _ => {}
                }
                match step.terminator {
                    Terminator::Goto(next) => block = next,
                    Terminator::Return => break,
                    other => panic!("unexpected {other:?}"),
                }
            }
            assert!(matches!(mir.blocks[block].terminator, Terminator::Return));
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

            let limits = AnalysisLimits {
                max_work: report.statistics().work,
                max_entries: report.statistics().entries,
                ..AnalysisLimits::default()
            };
            let check =
                |limits| lower_body(receipt, module, &declaration("unpack").id, limits).unwrap();
            assert!(check(limits).mir().is_some());
            for short in [
                AnalysisLimits {
                    max_work: limits.max_work - 1,
                    ..limits
                },
                AnalysisLimits {
                    max_entries: limits.max_entries - 1,
                    ..limits
                },
            ] {
                let limited = check(short);
                assert_eq!(limited.completion(), Completion::ResourceLimit);
                assert!(limited.mir().is_none());
            }

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

            let grouped = lower("grouped");
            let mir = grouped
                .mir()
                .unwrap_or_else(|| panic!("grouped: {:?}", grouped.completion()));
            assert!(mir.blocks.iter().any(
                |b| matches!(b.operation, Operation::Move { from, into: Some(to) }
                if from.tuple_element() == Some(0) && to.root.span.bytes() == b"first")
            ));
            assert!(
                matches!(path_drops(mir, mir.entry).as_slice(), [Obligation::Drop(place)]
                if place.tuple_element() == Some(1))
            );

            let live = lower("liveAlias");
            assert_eq!(live.completion(), Completion::SourceRejected);
            assert!(live.mir().is_none());
            let [finding] = live.findings() else {
                panic!("expected one live-alias finding")
            };
            assert_eq!(finding.code(), "R0203");
            assert_eq!(
                finding.site.span.bytes(),
                b"{ let x = true; let [r, _]: [&bool, string] = [&x, \"s\"]; out = r; }"
            );
            assert_eq!(finding.related.span.bytes(), b"&x");
            let dead = lower("deadAlias");
            let mir = dead.mir().expect("dead alias does not block cleanup");
            assert!(
                matches!(path_drops(mir, mir.entry).as_slice(), [Obligation::Drop(place)]
                if place.tuple_element() == Some(1))
            );

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

#[test]
fn shared_tuple_binding_copies_scalars_and_borrows_string_members() {
    probe(
        "patterns::shared_tuple_binding_copies_scalars_and_borrows_string_members",
        r#"
            function view(value: &[bool, string, string]): &string {
                let ([flag, (text), _]) = value;
                let _ = flag;
                return text;
            }
            function copyOut(value: [bool, string]): bool {
                let [flag, _] = &value;
                let moved = value;
                return flag;
            }
            function escapes(value: [bool, string]): &string {
                let [_, text] = &value;
                return text;
            }
            function premature(value: [bool, string]): void {
                let whole = &value;
                let [_, text] = whole;
                let moved = value;
                let _ = text;
            }
            function partial(value: [bool, string]): bool {
                let moved = value[1];
                let [flag, _] = &value;
                return flag;
            }
            function lastUse(value: [bool, string]): void {
                let [_, text] = &value;
                let _ = text;
                let moved = value;
            }
            function expires(seed: &string, flag: bool): &string {
                let mut out = seed;
                if (flag) { let pair: [bool, string] = [true, "local"]; let [_, view] = &pair; out = view; }
                return out;
            }
            function exclusive(value: &mut [bool, bool]): bool {
                let [bit, _] = value;
                return bit;
            }
            function nested(value: &[[bool, bool], bool]): bool {
                let [[bit, _], _] = value;
                return bit;
            }
            function carried(value: &[&bool, bool]): &bool {
                let [saved, _] = value;
                return saved;
            }
        "#,
        |_, module, receipt| {
            let lower = |name, limits| {
                let decl = module
                    .declarations
                    .iter()
                    .find(|d| d.name.as_deref() == Some(name))
                    .unwrap();
                lower_body(receipt, module, &decl.id, limits).unwrap()
            };
            let check = |name| lower(name, AnalysisLimits::default());
            let view = check("view");
            let mir = view
                .mir()
                .unwrap_or_else(|| panic!("view: {:?}", view.completion()));
            assert!(path_drops(mir, mir.entry).is_empty());
            let route: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::Copy { .. }
                            | Operation::BindSharedTupleMember { .. }
                            | Operation::Export(_)
                    )
                })
                .collect();
            assert!(matches!(route.as_slice(), [
                Operation::Copy { from: incoming, into: Some(staged) },
                Operation::BindSharedTupleMember { descriptor: first, position: 0, into: flag, projected: false },
                Operation::BindSharedTupleMember { descriptor: second, position: 1, into: text, projected: true },
                Operation::Copy { from: bit, .. },
                Operation::Copy { from: view, into: Some(result) },
                Operation::Export(output),
            ] if incoming.root.span.bytes() == b"value: &[bool, string, string]"
                && staged.root.node == first.root.node && staged.root.node == second.root.node
                && flag.root.span.bytes() == b"flag" && flag.root.node == bit.root.node
                && text.root.span.bytes() == b"text" && text.root.node == view.root.node
                && result.root.node == output.root.node));
            let limits = AnalysisLimits {
                max_work: view.statistics().work,
                max_entries: view.statistics().entries,
                ..AnalysisLimits::default()
            };
            assert!(lower("view", limits).mir().is_some());
            for short in [
                AnalysisLimits {
                    max_work: limits.max_work - 1,
                    ..limits
                },
                AnalysisLimits {
                    max_entries: limits.max_entries - 1,
                    ..limits
                },
            ] {
                let limited = lower("view", short);
                assert_eq!(limited.completion(), Completion::ResourceLimit);
                assert!(limited.mir().is_none());
            }
            for name in ["copyOut", "lastUse"] {
                let report = check(name);
                let mir = report
                    .mir()
                    .unwrap_or_else(|| panic!("{name}: {:?}", report.completion()));
                assert!(
                    matches!(path_drops(mir, mir.entry).as_slice(), [Obligation::Drop(place)]
                    if place.root.span.bytes() == b"moved" && place.tuple_element() == Some(1))
                );
            }
            for (name, code, site, related) in [
                ("escapes", "R0202", "return text;", "value: [bool, string]"),
                ("premature", "R0203", "value", "&value"),
                ("partial", "R0200", "&value", "value[1]"),
                (
                    "expires",
                    "R0203",
                    "{ let pair: [bool, string] = [true, \"local\"]; let [_, view] = &pair; out = view; }",
                    "&pair",
                ),
            ] {
                let report = check(name);
                assert_eq!(report.completion(), Completion::SourceRejected, "{name}");
                assert!(report.mir().is_none());
                let [finding] = report.findings() else {
                    panic!("{name}: expected a single finding")
                };
                assert_eq!(finding.code(), code, "{name}");
                assert_eq!(finding.site.span.bytes(), site.as_bytes(), "{name}");
                assert_eq!(finding.related.span.bytes(), related.as_bytes(), "{name}");
            }
            for (name, pattern) in [
                ("exclusive", "[bit, _]"),
                ("nested", "[[bit, _], _]"),
                ("carried", "[saved, _]"),
            ] {
                let report = check(name);
                assert_eq!(report.completion(), Completion::Incomplete, "{name}");
                assert!(report.mir().is_none());
                assert_eq!(
                    report.blocked_at().unwrap().span.bytes(),
                    pattern.as_bytes(),
                    "{name}"
                );
            }
        },
    );
}
