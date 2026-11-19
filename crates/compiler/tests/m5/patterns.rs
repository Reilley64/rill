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
            for name in ["custom", "nested"] {
                let incomplete = lower(name);
                assert_eq!(incomplete.completion(), Completion::Incomplete, "{name}");
                assert!(incomplete.mir().is_none(), "{name}");
            }
        },
    );
}

#[test]
fn shared_record_binding_uses_canonical_fields_and_retains_backing() {
    use compiler::analysis::ownership::{
        BorrowedMemberBinding::{CopyScalar, SharedView},
        Projection,
    };

    probe(
        "patterns::shared_record_binding_uses_canonical_fields_and_retains_backing",
        r#"
            type Spare = { text: bool; flag: string; };
            type Packet = { flag: bool; text: string; skip: string; omit: string; };
            function forward(value: &Packet): &string {
                let (Packet { text: (label), skip: _, flag }) = value;
                let _ = flag;
                return label;
            }
            function ignores(value: &Packet): void { let Packet { skip: _, text: _ } = value; }
            function copied(value: Packet): bool {
                let Packet { flag } = &value;
                let moved = value;
                return flag;
            }
            function escapes(value: Packet): &string {
                let Packet { text: label } = &value;
                return label;
            }
            function premature(value: Packet): void {
                let mut owner = value;
                let whole = &owner;
                let Packet { text: label } = whole;
                owner.skip = "new";
                let _ = label;
            }
            function lastUse(value: Packet): void {
                let mut owner = value;
                let Packet { text: label } = &owner;
                let _ = label;
                owner.skip = "new";
            }
            function expires(seed: &string, flag: bool, value: Packet): &string {
                let mut result = seed;
                if (flag) { let local = value; let Packet { text: label } = &local; result = label; }
                return result;
            }
            type Custom = { text: string; };
            implement dispose(value: Custom): void {}
            function custom(value: &Custom): &string {
                let Custom { text } = value;
                return text;
            }
            type Nested = { inner: [bool, bool]; };
            function nested(value: &Nested): bool {
                let Nested { inner: [bit, _] } = value;
                return bit;
            }
            type Protected = { lock: LockGuard<i32>; bit: bool; };
            function protected(value: &Protected): bool {
                let Protected { bit } = value;
                return bit;
            }
            function exclusive(value: &mut Packet): bool {
                let Packet { flag } = value;
                return flag;
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
            let forwarded = check("forward");
            let mir = forwarded
                .mir()
                .unwrap_or_else(|| panic!("forward: {:?}", forwarded.completion()));
            assert!(path_drops(mir, mir.entry).is_empty());
            let packet = module
                .declarations
                .iter()
                .find(|d| d.name.as_deref() == Some("Packet"))
                .unwrap();
            let field = |name| {
                module
                    .declarations
                    .iter()
                    .find(|d| {
                        d.id.kind == loader::DeclarationKind::Field
                            && d.name.as_deref() == Some(name)
                            && packet.span.start() < d.span.start()
                            && d.span.end() < packet.span.end()
                    })
                    .unwrap()
            };
            let ops = linear(mir, mir.entry);
            assert!(
                !ops.iter()
                    .any(|op| matches!(op, Operation::Move { .. } | Operation::Dispose { .. }))
            );
            let route: Vec<_> = ops
                .into_iter()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::Copy { .. }
                            | Operation::BindBorrowedMember { .. }
                            | Operation::Export(_)
                    )
                })
                .collect();
            assert!(matches!(route.as_slice(), [
                Operation::Copy { from: incoming, into: Some(staged) },
                Operation::BindBorrowedMember { descriptor: first, projection: Projection::Field(text), into: label, binding: SharedView },
                Operation::BindBorrowedMember { descriptor: second, projection: Projection::Field(bit), into: flag, binding: CopyScalar },
                Operation::Copy { from: used, .. },
                Operation::Copy { from: view, into: Some(result) },
                Operation::Export(output),
            ] if incoming.root.span.bytes() == b"value: &Packet"
                && staged.root.node == first.root.node && staged.root.node == second.root.node
                && text.id == field("text").id && bit.id == field("flag").id
                && label.root.span.bytes() == b"label" && label.root.node == view.root.node
                && flag.root.span.bytes() == b"flag" && flag.root.node == used.root.node
                && result.root.node == output.root.node));
            let limits = AnalysisLimits {
                max_work: forwarded.statistics().work,
                max_entries: forwarded.statistics().entries,
                ..AnalysisLimits::default()
            };
            assert!(lower("forward", limits).mir().is_some());
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
                let limited = lower("forward", short);
                assert_eq!(limited.completion(), Completion::ResourceLimit);
                assert!(limited.mir().is_none());
            }
            let ignored = check("ignores");
            let mir = ignored
                .mir()
                .expect("wildcards and omissions observe no member");
            let route: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::Copy { .. }
                            | Operation::Move { .. }
                            | Operation::Dispose { .. }
                            | Operation::BindBorrowedMember { .. }
                    )
                })
                .collect();
            assert!(
                matches!(route.as_slice(), [Operation::Copy { from, into: Some(_) }]
                if from.root.span.bytes() == b"value: &Packet")
            );
            assert!(path_drops(mir, mir.entry).is_empty());

            for (name, owner, expected) in [
                ("copied", b"moved".as_slice(), &["omit", "skip", "text"][..]),
                (
                    "lastUse",
                    b"owner".as_slice(),
                    &["skip", "omit", "skip", "text"][..],
                ),
            ] {
                let report = check(name);
                let mir = report
                    .mir()
                    .unwrap_or_else(|| panic!("{name}: {:?}", report.completion()));
                let fields: Vec<_> = path_drops(mir, mir.entry)
                    .into_iter()
                    .map(|drop| match drop {
                        Obligation::Drop(place) => {
                            assert_eq!(place.root.span.bytes(), owner, "{name}");
                            place.field().unwrap().name.as_deref().unwrap()
                        }
                        _ => panic!("unexpected cleanup"),
                    })
                    .collect();
                assert_eq!(fields, expected, "{name}");
            }
            for (name, code, site, related) in [
                ("escapes", "R0202", "return label;", "value: Packet"),
                ("premature", "R0203", "owner.skip = \"new\";", "&owner"),
                (
                    "expires",
                    "R0203",
                    "{ let local = value; let Packet { text: label } = &local; result = label; }",
                    "&local",
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
                ("custom", "Custom { text }"),
                ("nested", "[bit, _]"),
                ("exclusive", "Packet { flag }"),
                ("protected", "Protected { bit }"),
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
fn fresh_exclusive_tuple_pattern_keeps_the_original_restriction() {
    use compiler::analysis::ownership::{
        BorrowedMemberBinding::{CopyScalar, ExclusiveView},
        Projection,
    };

    probe(
        "patterns::fresh_exclusive_tuple_pattern_keeps_the_original_restriction",
        r#"
            function copies(seed: [bool, bool, string]): bool {
                let mut owner = seed;
                let ([(first), second, _]) = (&mut owner);
                let _ = first;
                let moved = owner;
                return second;
            }
            function touch(view: &mut string): void {}
            function resumes(seed: [bool, string]): bool {
                let mut owner = seed;
                let [_, view] = &mut owner;
                touch(view);
                owner[1] = "new";
                return owner[0];
            }
            function conflicts(seed: [bool, string]): bool {
                let mut owner = seed;
                let [_, view] = &mut owner;
                let moved = view;
                let bit = owner[0];
                let _ = moved;
                return bit;
            }
            function acquisition(seed: [bool, string]): bool {
                let mut owner = seed;
                let shared = &owner;
                let [bit, _] = &mut owner;
                let _ = shared;
                return bit;
            }
            function replaceEarly(seed: [bool, string]): void {
                let mut owner = seed;
                let [_, view] = &mut owner;
                owner[0] = true;
                let _ = view;
            }
            function escapes(seed: [bool, string]): &mut string {
                let mut owner = seed;
                let [_, view] = &mut owner;
                return view;
            }
            function expires(seed: &mut string, input: [bool, string], branch: bool): void {
                let mut out = seed;
                if (branch) { let mut owner = input; let [_, view] = &mut owner; out = view; }
                let _ = out;
            }
            function named(seed: [bool, string]): void {
                let mut owner = seed;
                let parent = &mut owner;
                let [_, view] = parent;
                let _ = view;
            }
            function both(seed: [string, string]): void {
                let mut owner = seed;
                let [first, second] = &mut owner;
                let _ = first;
                let _ = second;
            }
            function mixed(seed: [bool, string]): void {
                let mut owner = seed;
                let [flag, view] = &mut owner;
                let _ = flag;
                let _ = view;
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
            let copies = check("copies");
            let mir = copies
                .mir()
                .unwrap_or_else(|| panic!("copies: {:?}", copies.completion()));
            assert!(
                matches!(path_drops(mir, mir.entry).as_slice(), [Obligation::Drop(place)]
                if place.root.span.bytes() == b"moved" && place.tuple_element() == Some(2))
            );
            let route: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| match op {
                    Operation::Borrow { .. }
                    | Operation::BindBorrowedMember { .. }
                    | Operation::Export(_) => true,
                    Operation::Move { into: Some(to), .. } => to.root.span.bytes() == b"moved",
                    Operation::Copy { from, .. } => from.root.span.bytes() == b"second",
                    _ => false,
                })
                .collect();
            assert!(matches!(route.as_slice(), [
                Operation::Borrow { source, holder, exclusive: true },
                Operation::BindBorrowedMember { descriptor: a, projection: Projection::TupleElement(0), into: first, binding: CopyScalar },
                Operation::BindBorrowedMember { descriptor: b, projection: Projection::TupleElement(1), into: second, binding: CopyScalar },
                Operation::Move { from: owner, .. },
                Operation::Copy { from: read, into: Some(result) },
                Operation::Export(output),
            ] if source.root.node == owner.root.node && holder.root.node == a.root.node && holder.root.node == b.root.node
                && first.root.span.bytes() == b"first" && second.root.span.bytes() == b"second"
                && read.root.node == second.root.node && output.root.node == result.root.node));
            let resumes = check("resumes");
            let mir = resumes
                .mir()
                .expect("owner assignment resumes after the view's last use");
            let route: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::Borrow { .. }
                            | Operation::BindBorrowedMember { .. }
                            | Operation::Reborrow { .. }
                            | Operation::Call(_)
                            | Operation::Install { .. }
                    )
                })
                .collect();
            let touch = module
                .declarations
                .iter()
                .find(|d| d.name.as_deref() == Some("touch"))
                .unwrap();
            assert!(matches!(route.as_slice(), [
                Operation::Borrow { source, holder, exclusive: true },
                Operation::BindBorrowedMember { descriptor, projection: Projection::TupleElement(1), into: view, binding: ExclusiveView },
                Operation::Reborrow { parent, exclusive: true, .. },
                Operation::Call(callee),
                Operation::Install { target, .. },
            ] if source.root.node == target.root.node && target.tuple_element() == Some(1)
                && holder.root.node == descriptor.root.node && parent.root.node == view.root.node && **callee == touch.id));
            let limits = AnalysisLimits {
                max_work: resumes.statistics().work,
                max_entries: resumes.statistics().entries,
                ..AnalysisLimits::default()
            };
            assert!(lower("resumes", limits).mir().is_some());
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
                let limited = lower("resumes", short);
                assert_eq!(limited.completion(), Completion::ResourceLimit);
                assert!(limited.mir().is_none());
            }
            assert!(
                matches!(path_drops(mir, mir.entry).as_slice(), [Obligation::Drop(old), Obligation::Drop(replacement)]
                if old.root.node == replacement.root.node && old.root.span.bytes() == b"owner"
                    && old.tuple_element() == Some(1) && replacement.tuple_element() == Some(1))
            );
            for (name, code, at, cause) in [
                ("conflicts", "R0201", "owner[0]", "&mut owner"),
                ("acquisition", "R0201", "&mut owner", "&owner"),
                ("replaceEarly", "R0203", "owner[0] = true;", "&mut owner"),
                ("escapes", "R0202", "return view;", "owner"),
                (
                    "expires",
                    "R0203",
                    "{ let mut owner = input; let [_, view] = &mut owner; out = view; }",
                    "&mut owner",
                ),
            ] {
                let report = check(name);
                assert_eq!(report.completion(), Completion::SourceRejected, "{name}");
                assert!(report.mir().is_none());
                let [finding] = report.findings() else {
                    panic!("{name}: expected one finding")
                };
                assert_eq!(finding.code(), code, "{name}");
                assert_eq!(finding.site.span.bytes(), at.as_bytes(), "{name}");
                assert_eq!(finding.related.span.bytes(), cause.as_bytes(), "{name}");
            }
            for (name, pattern) in [
                ("named", "[_, view]"),
                ("both", "[first, second]"),
                ("mixed", "[flag, view]"),
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

#[test]
fn shared_product_binding_copies_carried_references_not_container_access() {
    use compiler::analysis::ownership::{BorrowedMemberBinding::CopyReference, Projection};

    probe(
        "patterns::shared_product_binding_copies_carried_references_not_container_access",
        r#"
            type Carrier = { saved: &bool; other: &bool; bit: bool; tag: string; };
            function tupleForward(seed: &bool, tag: string): &bool {
                let pair: [&bool, string] = [seed, tag];
                let alias = &pair;
                let [(view), _] = alias;
                let moved = pair;
                return view;
            }
            function recordForward(seed: &bool, tag: string): &bool {
                let value = Carrier { saved: seed, other: seed, bit: true, tag: tag };
                let Carrier { saved: view } = &value;
                let moved = value;
                return view;
            }
            function throughParameter(value: &Carrier): &bool {
                let Carrier { saved } = value;
                return saved;
            }
            function siblingBacking(seed: &bool, flag: bool): &bool {
                let value = Carrier { saved: seed, other: &flag, bit: false, tag: "other" };
                let Carrier { saved: view } = &value;
                let moved = value;
                return view;
            }
            function pointsIntoContainer(seed: &bool): &bool {
                let mut value = Carrier { saved: seed, other: seed, bit: true, tag: "local" };
                value.saved = &value.bit;
                let Carrier { saved: view } = &value;
                return view;
            }
            function sourceExpires(seed: &bool, branch: bool): bool {
                let mut out = seed;
                if (branch) {
                    let flag = true;
                    let pair: [&bool, string] = [&flag, "local"];
                    let [view, _] = &pair;
                    out = view;
                }
                return *out;
            }
            function survivesMove(flag: bool, tag: string): bool {
                let pair: [&bool, string] = [&flag, tag];
                let [view, _] = &pair;
                let moved = pair;
                return *view;
            }
            function carry(seed: &bool): [&bool, string] { return [seed, "callee"]; }
            function rootBacking(flag: bool): &bool {
                let pair = carry(&flag);
                let [view, _] = &pair;
                return view;
            }
            function suspended(seed: bool): bool {
                let mut source = seed;
                let access = &mut source;
                let first = &*access;
                let pair: [&bool, string] = [first, "retained"];
                let [view, _] = &pair;
                let _ = *access;
                return *view;
            }
            function exclusiveBacking(seed: bool): bool {
                let mut source = seed;
                let access = &mut source;
                let first = &*access;
                let pair: [&bool, string] = [first, "retained"];
                let [view, _] = &pair;
                let _ = source;
                return *view;
            }
            function projectedStringStillBorrowsOwner(seed: &bool): &string {
                let pair: [&bool, string] = [seed, "local"];
                let [view, text] = &pair;
                let _ = view;
                return text;
            }
            function exclusiveMember(value: &[&mut bool, bool]): bool {
                let [_, flag] = value;
                return flag;
            }
            function nestedReference(value: &[&&bool, bool]): bool {
                let [_, flag] = value;
                return flag;
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
            for name in ["tupleForward", "recordForward", "throughParameter"] {
                let report = check(name);
                let mir = report.mir().unwrap_or_else(|| {
                    panic!(
                        "{name}: {:?} at {:?}",
                        report.completion(),
                        report
                            .blocked_at()
                            .map(|site| std::str::from_utf8(site.span.bytes()).unwrap())
                    )
                });
                let route: Vec<_> = linear(mir, mir.entry)
                    .into_iter()
                    .filter(|op| match op {
                        Operation::Borrow { .. }
                        | Operation::BindBorrowedMember { .. }
                        | Operation::Export(_) => true,
                        Operation::Move {
                            into: Some(into), ..
                        } => into.root.span.bytes() == b"moved",
                        Operation::Copy { from, .. } => matches!(
                            from.root.span.bytes(),
                            b"alias" | b"value: &Carrier" | b"view" | b"saved"
                        ),
                        _ => false,
                    })
                    .collect();
                let bind = route
                    .iter()
                    .position(|op| matches!(op, Operation::BindBorrowedMember { .. }))
                    .unwrap();
                let (stage, uses) = route.split_at(bind);
                let [
                    Operation::BindBorrowedMember {
                        descriptor,
                        projection,
                        into: bound,
                        binding: CopyReference,
                    },
                    remaining @ ..,
                ] = uses
                else {
                    panic!("{name}: expected a stored reference copy")
                };
                let owner = match (name, stage) {
                    (
                        "tupleForward",
                        [
                            Operation::Borrow {
                                source,
                                holder,
                                exclusive: false,
                            },
                            Operation::Copy {
                                from: alias,
                                into: Some(staged),
                            },
                        ],
                    ) => {
                        assert!(matches!(projection, Projection::TupleElement(0)));
                        assert_eq!(source.root.span.bytes(), b"pair");
                        assert_eq!(alias.root.node, holder.root.node);
                        assert_eq!(staged.root.node, descriptor.root.node);
                        Some(source)
                    }
                    (
                        "recordForward",
                        [
                            Operation::Borrow {
                                source,
                                holder,
                                exclusive: false,
                            },
                        ],
                    ) => {
                        assert_eq!(source.root.span.bytes(), b"value");
                        assert_eq!(holder.root.node, descriptor.root.node);
                        Some(source)
                    }
                    (
                        "throughParameter",
                        [
                            Operation::Copy {
                                into: Some(staged), ..
                            },
                        ],
                    ) => {
                        assert_eq!(staged.root.node, descriptor.root.node);
                        None
                    }
                    _ => panic!("{name}: expected the original descriptor stage"),
                };
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
                if name != "tupleForward" {
                    assert!(
                        matches!(projection, Projection::Field(declaration) if declaration.id == field("saved").id)
                    );
                }
                let rest = if let Some(owner) = owner {
                    let [
                        Operation::Move {
                            from,
                            into: Some(destination),
                        },
                        rest @ ..,
                    ] = remaining
                    else {
                        panic!("{name}: expected a move after binding")
                    };
                    assert_eq!(from.root.node, owner.root.node);
                    let drops = path_drops(mir, mir.entry);
                    let [Obligation::Drop(place)] = drops.as_slice() else {
                        panic!("{name}: expected one cleanup")
                    };
                    assert_eq!(place.root.node, destination.root.node);
                    if name == "tupleForward" {
                        assert_eq!(place.tuple_element(), Some(1));
                    } else {
                        assert!(
                            place
                                .field()
                                .is_some_and(|member| member.id == field("tag").id)
                        );
                    }
                    rest
                } else {
                    assert!(path_drops(mir, mir.entry).is_empty());
                    remaining
                };
                assert!(
                    matches!(rest, [Operation::Copy { from, into: Some(result) }, Operation::Export(output)]
                    if from.root.node == bound.root.node && result.root.node == output.root.node)
                );
            }
            let moved = check("survivesMove");
            let mir = moved
                .mir()
                .expect("the stored local reference survives the container move");
            let route: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| match op {
                    Operation::BindBorrowedMember { .. }
                    | Operation::Dereference(_)
                    | Operation::Export(_) => true,
                    Operation::Move {
                        into: Some(into), ..
                    } => into.root.span.bytes() == b"moved",
                    _ => false,
                })
                .collect();
            assert!(matches!(route.as_slice(), [
                Operation::BindBorrowedMember { into: view, binding: CopyReference, .. },
                Operation::Move { from, into: Some(owner) },
                Operation::Dereference(read),
                Operation::Export(_),
            ] if view.root.node == read.root.node && from.root.span.bytes() == b"pair" && owner.root.span.bytes() == b"moved"));
            let forward = check("tupleForward");
            let limits = AnalysisLimits {
                max_work: forward.statistics().work,
                max_entries: forward.statistics().entries,
                ..AnalysisLimits::default()
            };
            assert!(lower("tupleForward", limits).mir().is_some());
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
                let limited = lower("tupleForward", short);
                assert_eq!(limited.completion(), Completion::ResourceLimit);
                assert!(limited.mir().is_none());
            }
            for (name, code, site, related) in [
                ("siblingBacking", "R0202", "return view;", "flag: bool"),
                ("pointsIntoContainer", "R0202", "return view;", "&value.bit"),
                (
                    "sourceExpires",
                    "R0203",
                    "{\n                    let flag = true;",
                    "&flag",
                ),
                ("rootBacking", "R0202", "return view;", "flag: bool"),
                ("suspended", "R0201", "*access", "&*access"),
                ("exclusiveBacking", "R0201", "source", "&mut source"),
                (
                    "projectedStringStillBorrowsOwner",
                    "R0202",
                    "return text;",
                    "pair",
                ),
            ] {
                let report = check(name);
                assert_eq!(report.completion(), Completion::SourceRejected, "{name}");
                assert!(report.mir().is_none());
                let [finding] = report.findings() else {
                    panic!("{name}: expected one finding")
                };
                assert_eq!(finding.code(), code, "{name}");
                if name == "sourceExpires" {
                    assert!(
                        finding.site.span.bytes().starts_with(site.as_bytes()),
                        "{name}"
                    );
                } else {
                    assert_eq!(finding.site.span.bytes(), site.as_bytes(), "{name}");
                }
                assert_eq!(finding.related.span.bytes(), related.as_bytes(), "{name}");
            }
            for name in ["exclusiveMember", "nestedReference"] {
                let report = check(name);
                assert_eq!(report.completion(), Completion::Incomplete, "{name}");
                assert!(report.mir().is_none());
                assert_eq!(report.blocked_at().unwrap().span.bytes(), b"[_, flag]");
            }
        },
    );
}

#[test]
fn shared_tuple_binding_copies_scalars_and_borrows_string_members() {
    use compiler::analysis::ownership::{
        BorrowedMemberBinding::{CopyScalar, SharedView},
        Projection,
    };

    probe(
        "patterns::shared_tuple_binding_copies_scalars_and_borrows_string_members",
        r#"
            function view(value: &[bool, string, string]): &string {
                let ([flag, (text), _]) = value;
                let _ = flag;
                return text;
            }
            function ignores(value: &[bool, string]): void { let [_, _] = value; }
            function pass(value: [bool, string]): [bool, string] { return value; }
            function temporary(value: [bool, string]): &string {
                let [_, text] = &pass(value);
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
            let view_ops = linear(mir, mir.entry);
            assert!(
                !view_ops
                    .iter()
                    .any(|op| matches!(op, Operation::Move { .. } | Operation::Dispose { .. }))
            );
            let route: Vec<_> = view_ops
                .into_iter()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::Copy { .. }
                            | Operation::BindBorrowedMember { .. }
                            | Operation::Export(_)
                    )
                })
                .collect();
            assert!(matches!(route.as_slice(), [
                Operation::Copy { from: incoming, into: Some(staged) },
                Operation::BindBorrowedMember { descriptor: first, projection: Projection::TupleElement(0), into: flag, binding: CopyScalar },
                Operation::BindBorrowedMember { descriptor: second, projection: Projection::TupleElement(1), into: text, binding: SharedView },
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
            let ignored = check("ignores");
            let mir = ignored.mir().expect("wildcards observe no member");
            let significant: Vec<_> = linear(mir, mir.entry)
                .into_iter()
                .filter(|op| {
                    matches!(
                        op,
                        Operation::Copy { .. }
                            | Operation::Move { .. }
                            | Operation::Dispose { .. }
                            | Operation::BindBorrowedMember { .. }
                    )
                })
                .collect();
            assert!(
                matches!(significant.as_slice(), [Operation::Copy { from, into: Some(_) }]
                if from.root.span.bytes() == b"value: &[bool, string]")
            );
            assert!(path_drops(mir, mir.entry).is_empty());
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
                ("temporary", "pass(value)"),
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
