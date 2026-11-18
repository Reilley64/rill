//! Installed conversion dispatch, not Rill execution or error wrapping.
use compiler::analysis::conversions::{
    ErrorAgreement, PairBlocker, PairOutcome, RegistrationIssueKind,
};
use compiler::analysis::scalars::BodyOwner;
use compiler::analysis::typed::{
    ConversionProtection, ConversionProvider, DecimalParseError, NumericConversionError,
    PrimitiveValue, TypedCompletion, analyze_typed, conversion_values,
};
use compiler::analysis::{AnalysisLimits, QueryError, TypeTerm};
use compiler::loader::{
    Graph, InstalledRequest, Limits, Origin, Prelude, Request, Selector, load, load_installed,
};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[test]
fn installed_conversions() {
    if let Some(root) = std::env::var_os("RILL_CONVERSION_TEST_ROOT") {
        let root = PathBuf::from(root);
        for mode in [false, true] {
            numeric(&root, mode);
            parsing(&root, mode);
            text_and_bytes(&root, mode);
            owned_bytes_has_no_result_storage(&root, mode);
            user_dispatch(&root, mode);
            rejected_registrations(&root, mode);
            authority(&root, mode);
        }
        return;
    }
    let root = std::env::temp_dir().join(format!("rill-m48-conversions-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let _cleanup = Cleanup(root.clone());
    for dir in ["bin", "lib/rill/std", "app"] {
        fs::create_dir_all(root.join(dir)).unwrap();
    }
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../stdlib");
    for file in [
        "core.rill",
        "convert.rill",
        "number.rill",
        "char.rill",
        "bytes.rill",
        "string.rill",
        "collection.rill",
        "option.rill",
        "result.rill",
        "error.rill",
        "format.rill",
        "compare.rill",
    ] {
        fs::copy(source.join(file), root.join("lib/rill/std").join(file)).unwrap();
    }
    let executable = root.join("bin/conversion-probe");
    fs::copy(std::env::current_exe().unwrap(), &executable).unwrap();
    let output = Command::new(executable)
        .args(["--exact", "installed_conversions", "--nocapture"])
        .env("RILL_CONVERSION_TEST_ROOT", &root)
        .current_dir(root.join("app"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn capture(root: &Path, mode: bool, source: &str) -> Graph {
    let entry = root.join("app/main.rill");
    fs::write(&entry, source).unwrap();
    load_installed(InstalledRequest {
        entry,
        target: "aarch64-apple-darwin".into(),
        options: BTreeMap::new(),
        test_mode: mode,
        limits: Limits::default(),
    })
    .unwrap_or_else(|e| {
        panic!(
            "{:?}: {}; {:?}",
            e.kind,
            e.message,
            e.findings
                .iter()
                .map(|f| (
                    f.code,
                    &f.message,
                    f.span.as_ref().map(|s| String::from_utf8_lossy(s.bytes()))
                ))
                .collect::<Vec<_>>()
        )
    })
}

fn owned_bytes_has_no_result_storage(root: &Path, mode: bool) {
    let graph = capture(
        root,
        mode,
        "
        import std::bytes::Bytes as OwnedBytes;
        import std::result::Result;
        function lost(pair: [OwnedBytes, Result<bool>.Ok]): void {}
    ",
    );
    let report = analyze_typed(&graph, AnalysisLimits::default());
    let uses = report.result_use().unwrap();
    let app = graph
        .modules()
        .iter()
        .find(|module| module.key.origin == Origin::Application)
        .unwrap();
    let relevant: Vec<_> = uses
        .discards()
        .iter()
        .filter(|w| w.origin.module == &app.key)
        .collect();
    let [witness] = relevant.as_slice() else {
        panic!("the opaque Bytes seal must not erase the Result sibling");
    };
    assert_eq!(
        witness.origin.span.bytes(),
        b"pair: [OwnedBytes, Result<bool>.Ok]"
    );
    assert_eq!(
        witness.component.as_ref(),
        &[compiler::analysis::typed::ResultComponent::TupleElement(1)]
    );
    assert!(
        !uses
            .pending()
            .iter()
            .any(|pending| pending.site.module == &app.key)
    );
}

fn numeric(root: &Path, mode: bool) {
    let graph = capture(
        root,
        mode,
        "
        import std::convert::{tryFrom as narrow, from};
        import std::result::Result;
        import std::number::ConversionError;
        import std::compare::eq;
        import std::error::display;
        function equal(left: &ConversionError, right: &ConversionError): bool { return eq(left, right); }
        function describe(error: &ConversionError): string { return display(error); }
        function checked(value: f64): Result<u8, ConversionError> { return narrow(value); }
        function mismatch(value: f64): Result<u8, bool> { return narrow<u8>(value); }
        function mismatchWritten(value: f64): Result<u8, ConversionError> { return narrow<u8,f64,bool>(value); }
        function gap(value: f64): u8 { return from(value); }
        function identity(value: char): char { return from<char>(value); }
        function descriptor(): (f64) => Result<u8, ConversionError> { return narrow; }
    ",
    );
    let report = analyze_typed(&graph, AnalysisLimits::default());
    let app = graph
        .modules()
        .iter()
        .find(|m| m.key.origin == Origin::Application)
        .unwrap();
    assert!(
        report.try_into_registration_issues().unwrap().is_empty(),
        "{:?}",
        report
            .try_into_registration_issues()
            .unwrap()
            .iter()
            .map(|issue| (issue.kind, String::from_utf8_lossy(issue.span.bytes())))
            .collect::<Vec<_>>()
    );
    let site = |span: &[u8]| {
        let node = app
            .syntax
            .nodes
            .iter()
            .position(|n| n.span.bytes() == span)
            .unwrap();
        report.bodies().conversion_use(app, node).unwrap().unwrap()
    };
    for name in ["equal", "describe"] {
        let declaration = app
            .declarations
            .iter()
            .find(|d| d.name.as_deref() == Some(name))
            .unwrap();
        assert!(
            report
                .bodies()
                .body(app, BodyOwner::Declaration(&declaration.id))
                .unwrap()
                .is_some(),
            "{name}"
        );
    }
    let numeric = site(b"narrow(value)");
    let Some(ConversionProvider::NumericTryInto(op)) = numeric.provider else {
        panic!("missing linked operation")
    };
    assert_eq!(
        numeric.protection,
        Some(ConversionProtection::RequiredNumeric)
    );
    assert_eq!(
        op.convert(PrimitiveValue::F64(-0.5)),
        Some(Ok(PrimitiveValue::U8(0)))
    );
    assert_eq!(
        op.convert(PrimitiveValue::F64(f64::NAN)),
        Some(Err(NumericConversionError::NotFinite))
    );
    assert_eq!(
        op.convert(PrimitiveValue::F64(256.0)),
        Some(Err(NumericConversionError::OutOfRange))
    );
    for call in [
        b"narrow<u8>(value)".as_slice(),
        b"narrow<u8,f64,bool>(value)",
    ] {
        let wrong = site(call);
        assert_eq!(wrong.provider, numeric.provider);
        assert!(matches!(
            wrong.error_agreement,
            ErrorAgreement::Disagrees { .. }
        ));
        assert!(
            report
                .bodies()
                .node(app, wrong.site.node)
                .unwrap()
                .is_none()
        );
    }
    let gap = site(b"from(value)");
    assert_eq!(
        gap.protection,
        Some(ConversionProtection::ProhibitedNumeric)
    );
    assert_eq!(gap.outcome, Some(PairOutcome::NumericGap));
    assert!(gap.provider.is_none());
    let identity = site(b"from<char>(value)");
    assert_eq!(
        identity.protection,
        Some(ConversionProtection::RequiredIdentity)
    );
    assert_eq!(identity.provider, Some(ConversionProvider::Identity));
    assert!(app.syntax.nodes.iter().enumerate().any(|(node, _)| {
        report
            .bodies()
            .conversion_use(app, node)
            .unwrap()
            .is_some_and(|u| {
                u.descriptor.is_some()
                    && u.descriptor_agreement == Some(true)
                    && u.provider == numeric.provider
            })
    }));
    let checked = app
        .declarations
        .iter()
        .find(|d| d.name.as_deref() == Some("checked"))
        .unwrap();
    assert!(
        report
            .bodies()
            .body(app, BodyOwner::Declaration(&checked.id))
            .unwrap()
            .is_some(),
        "{:?}; {:?}",
        report
            .bodies()
            .findings()
            .iter()
            .filter(|f| f.site.module == &app.key)
            .map(|f| f.kind)
            .collect::<Vec<_>>(),
        report
            .bodies()
            .obligations()
            .iter()
            .filter(|f| f.site.module == &app.key)
            .map(|f| f.reason)
            .collect::<Vec<_>>()
    );
}

fn parsing(root: &Path, mode: bool) {
    use std::fmt::Write;
    let widths = [
        "i8", "i16", "i32", "i64", "i128", "u8", "u16", "u32", "u64", "u128", "f32", "f64",
    ];
    let mut source = String::from("import std::convert::tryInto;\n");
    for width in widths {
        writeln!(
            source,
            "function parse_{width}(text: &string): void {{ let _ = tryInto<{width}>(text); }}"
        )
        .unwrap();
    }
    let graph = capture(root, mode, &source);
    let report = analyze_typed(&graph, AnalysisLimits::default());
    let app = graph
        .modules()
        .iter()
        .find(|m| m.key.origin == Origin::Application)
        .unwrap();
    assert!(
        !graph
            .bindings()
            .iter()
            .any(|binding| binding.module == app.key && binding.name == "ParseError")
    );
    for width in widths {
        let call = format!("tryInto<{width}>(text)");
        let node = app
            .syntax
            .nodes
            .iter()
            .position(|n| n.span.bytes() == call.as_bytes())
            .unwrap();
        let use_ = report.bodies().conversion_use(app, node).unwrap().unwrap();
        let Some(ConversionProvider::Parse(op)) = use_.provider else {
            panic!("{width}: {:?}", use_.blockers)
        };
        let Some(PairOutcome::Winner(id)) = use_.outcome else {
            panic!("missing registration")
        };
        let provider = report.conversion_provider(id).unwrap().unwrap();
        assert_eq!(Some(provider.operation), use_.provider);
        assert_eq!(provider.bridge.id.module.path, "convert.rill");
        let module = graph.modules().iter().find(|m| m.key == id.module).unwrap();
        assert!(
            report
                .bodies()
                .body(module, BodyOwner::Declaration(id))
                .unwrap()
                .is_some()
        );
        let Some(TypeTerm::Nominal {
            declaration,
            arguments,
        }) = use_.selected_result.as_deref()
        else {
            panic!("missing Result")
        };
        assert_eq!(**declaration, report.result_role().unwrap().declaration.id);
        let TypeTerm::Nominal { declaration, .. } = arguments[1].as_ref() else {
            panic!("missing ParseError")
        };
        assert!(
            graph
                .modules()
                .iter()
                .flat_map(|m| &m.declarations)
                .any(|d| d.id == **declaration
                    && d.id.module.path == "number.rill"
                    && d.name.as_deref() == Some("ParseError"))
        );
        assert_eq!(op.parse("007").unwrap().primitive(), op.destination());
        for invalid in [
            "", " 1", "1 ", "+1", "0x2", "1_0", "１", "1u8", "-NaN", "infinity", ".5", "1.", "1e+",
        ] {
            assert_eq!(
                op.parse(invalid),
                Err(DecimalParseError::InvalidSyntax),
                "{width}: {invalid}"
            );
        }
        match width {
            "i8" => {
                assert_eq!(op.parse("-128"), Ok(PrimitiveValue::I8(-128)));
                assert_eq!(op.parse("128"), Err(DecimalParseError::OutOfRange));
                assert_eq!(op.parse("NaN"), Err(DecimalParseError::InvalidSyntax));
            }
            "u128" => {
                assert_eq!(
                    op.parse("340282366920938463463374607431768211455"),
                    Ok(PrimitiveValue::U128(u128::MAX))
                );
                assert_eq!(
                    op.parse("340282366920938463463374607431768211456"),
                    Err(DecimalParseError::OutOfRange)
                );
                assert_eq!(op.parse("-0"), Err(DecimalParseError::InvalidSyntax));
            }
            "f32" => {
                assert_eq!(op.parse("1.25E+1"), Ok(PrimitiveValue::F32(12.5)));
                assert!(
                    matches!(op.parse("-1e-1000"), Ok(PrimitiveValue::F32(v)) if v.to_bits() == (-0.0f32).to_bits())
                );
                assert!(matches!(op.parse("NaN"), Ok(PrimitiveValue::F32(v)) if v.is_nan()));
                assert_eq!(op.parse("-inf"), Ok(PrimitiveValue::F32(f32::NEG_INFINITY)));
                assert_eq!(op.parse("1e100"), Err(DecimalParseError::OutOfRange));
                assert_eq!(
                    op.parse("1.000000059604644775390625"),
                    Ok(PrimitiveValue::F32(1.0))
                );
            }
            _ => {}
        }
    }
}

fn text_and_bytes(root: &Path, mode: bool) {
    let graph = capture(
        root,
        mode,
        "
        import std::convert::{into, tryInto};
        import std::bytes::Bytes;
        import std::collection::List;
        import std::result::Result;
        import std::char::ScalarError;
        import std::string::Utf8Error;
        function byteView(v: &string): &Bytes { return into(v); }
        function ownedString(v: &Bytes): Result<string, Utf8Error> { return tryInto(v); }
        function stringView(v: &Bytes): Result<&string, Utf8Error> { return tryInto(v); }
        function bytes(v: List<u8>): Bytes { return into(v); }
        function copyBytes(v: &List<u8>): Bytes { return into(v); }
        function list(v: Bytes): List<u8> { return into(v); }
        function copyList(v: &Bytes): List<u8> { return into(v); }
        function point(v: char): u32 { return into(v); }
        function scalar(v: u32): Result<char, ScalarError> { return tryInto(v); }
    ",
    );
    let report = analyze_typed(&graph, AnalysisLimits::default());
    let app = graph
        .modules()
        .iter()
        .find(|m| m.key.origin == Origin::Application)
        .unwrap();
    let selected: Vec<_> = app
        .syntax
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| matches!(n.span.bytes(), b"into(v)" | b"tryInto(v)"))
        .map(|(node, _)| {
            let use_ = report.bodies().conversion_use(app, node).unwrap().unwrap();
            assert!(
                report.bodies().node(app, node).unwrap().is_some(),
                "{:?}",
                use_.blockers
            );
            use_.provider.unwrap()
        })
        .collect();
    use ConversionProvider::*;
    assert_eq!(
        selected,
        [
            TextBytes,
            DecodeOwned,
            DecodeView,
            TakeList,
            CopyList,
            TakeBytes,
            CopyBytes,
            CodePoint,
            UnicodeScalar
        ]
    );
    use conversion_values::{
        Bytes, InvalidScalar, Utf8Error, code_point, decode_owned, decode_view, text_bytes,
        unicode_scalar,
    };
    use rill_collection::List;
    assert_eq!(code_point('🦀'), 0x1f980);
    assert_eq!(unicode_scalar(0x10ffff), Ok('\u{10ffff}'));
    assert_eq!(unicode_scalar(0xd800), Err(InvalidScalar));
    assert_eq!(unicode_scalar(0x110000), Err(InvalidScalar));
    let text = "\u{feff}\0é\r\n";
    let view = text_bytes(text);
    assert_eq!(view.as_ptr(), text.as_ptr());
    assert_eq!(decode_view(view).unwrap().as_ptr(), view.as_ptr());
    assert_eq!(decode_owned(view).unwrap(), text);
    for (input, valid_up_to, error_length) in [
        (&[b'a', 0xe1, 0x80, b'A'][..], 1, Some(2)),
        (&[0xe0, 0x80, 0x80], 0, Some(1)),
        (&[0xed, 0xa0, 0x80], 0, Some(1)),
        (&[0xf0, 0x90, 0x80], 0, None),
    ] {
        let error = Utf8Error {
            valid_up_to,
            error_length,
        };
        assert_eq!(decode_view(input), Err(error));
        assert_eq!(decode_owned(input), Err(error));
    }
    let mut list = List::from_elements([1, 2, 3]);
    let copy = Bytes::copy_list(&list);
    list[0] = 9;
    assert_eq!(copy.view(), [1, 2, 3]);
    assert_eq!(
        Bytes::take_list(list)
            .into_list()
            .into_owned_iter()
            .collect::<Vec<_>>(),
        [9, 2, 3]
    );
    let copied = copy.to_list();
    drop(copy);
    assert_eq!(copied.into_owned_iter().collect::<Vec<_>>(), [1, 2, 3]);
}

fn user_dispatch(root: &Path, mode: bool) {
    let graph = capture(root, mode, "
        import std::convert::{tryInto, tryFrom as cast};
        import std::result::Result;
        type Target = {};
        enum Source { Tag }
        implement<T> tryInto<Target>(value: T): Result<Target, string> { return Result.Ok(Target {}); }
        implement tryInto<Target>(value: Source): Result<Target, bool> { return Result.Ok(Target {}); }
        function tag(): Result<Target, bool> { return cast(Source.Tag); }
        function mismatch(): Result<Target, string> { return cast<Target>(Source.Tag); }
        function filtered(): Result<Target, string> { return cast<Target, Source.Tag>(Source.Tag); }
        function generic<T, E>(value: T): Result<Target, E> where tryInto(Target, T, E) { return cast<Target>(value); }
    ");
    let report = analyze_typed(&graph, AnalysisLimits::default());
    let app = graph
        .modules()
        .iter()
        .find(|m| m.key.origin == Origin::Application)
        .unwrap();
    let use_at = |span: &[u8]| {
        let node = app
            .syntax
            .nodes
            .iter()
            .position(|n| n.span.bytes() == span)
            .unwrap();
        report.bodies().conversion_use(app, node).unwrap().unwrap()
    };
    assert!(report.try_into_registration_issues().unwrap().is_empty());
    let selected = use_at(b"cast(Source.Tag)");
    assert!(matches!(selected.outcome, Some(PairOutcome::Winner(id)) if id.module == app.key));
    assert!(matches!(selected.error_agreement, ErrorAgreement::Agrees));
    assert!(selected.premise.is_none());
    let mismatch = use_at(b"cast<Target>(Source.Tag)");
    assert_eq!(mismatch.outcome, selected.outcome);
    assert!(matches!(
        mismatch.error_agreement,
        ErrorAgreement::Disagrees { .. }
    ));
    assert!(
        report
            .bodies()
            .findings()
            .iter()
            .any(|f| f.site.node == mismatch.site.node
                && f.site.module == &app.key
                && f.kind.code() == Some("R0313"))
    );
    assert!(matches!(
        use_at(b"cast<Target, Source.Tag>(Source.Tag)").error_agreement,
        ErrorAgreement::Agrees
    ));
    let generic = use_at(b"cast<Target>(value)");
    assert!(
        generic.premise.is_none() && generic.selected_result.is_some(),
        "{:?} {:?}; {:?}",
        generic.blockers,
        report
            .bodies()
            .goal(generic.goal.as_ref().unwrap())
            .unwrap()
            .judgment,
        report
            .bodies()
            .goals()
            .iter()
            .filter(|g| g.source.module == &app.key)
            .map(|g| (g.state, &g.judgment))
            .collect::<Vec<_>>()
    );
}

fn rejected_registrations(root: &Path, mode: bool) {
    let graph = capture(
        root,
        mode,
        "
        import std::convert::{from, tryFrom};
        import std::result::Result;
        implement from<u8>(value: i64): u8 { return 0u8; }
        implement from<bool>(value: bool): bool { return value; }
        implement tryFrom<u8>(value: f64): Result<u8,bool> { return Result.Ok(0u8); }
        function noReplacement(value: f64): Result<u8,bool> { return tryFrom(value); }
    ",
    );
    let report = analyze_typed(&graph, AnalysisLimits::default());
    let into = report.into_registration_issues().unwrap();
    let try_into = report.try_into_registration_issues().unwrap();
    assert!(
        into.iter()
            .any(|i| i.kind == RegistrationIssueKind::ProtectedIdentity)
    );
    assert!(
        into.iter()
            .any(|i| i.kind == RegistrationIssueKind::ProtectedNumericPair)
    );
    assert!(
        try_into
            .iter()
            .any(|i| i.kind == RegistrationIssueKind::ProtectedNumericPair)
    );
    let app = graph
        .modules()
        .iter()
        .find(|m| m.key.origin == Origin::Application)
        .unwrap();
    let node = app
        .syntax
        .nodes
        .iter()
        .position(|n| n.span.bytes() == b"tryFrom(value)")
        .unwrap();
    let use_ = report.bodies().conversion_use(app, node).unwrap().unwrap();
    assert!(use_.blockers.contains(&PairBlocker::Registration));
    assert_eq!(use_.protection, Some(ConversionProtection::RequiredNumeric));
    assert!(matches!(
        use_.provider,
        Some(ConversionProvider::NumericTryInto(_))
    ));
    assert!(use_.selected_result.is_none());
}

fn authority(root: &Path, mode: bool) {
    let no_trigger = capture(root, mode, "import std::number::ConversionError;");
    assert!(
        no_trigger
            .modules()
            .iter()
            .all(|m| !matches!(m.key.path.as_str(), "bytes.rill" | "convert.rill"))
    );
    let source = "import std::convert::tryInto; function parse(text: &string): void { let _ = tryInto<u8>(text); }";
    let graph = capture(root, mode, source);
    assert!(
        graph
            .modules()
            .iter()
            .any(|m| m.key.origin == Origin::Standard && m.key.path == "bytes.rill")
    );
    let report = analyze_typed(&graph, AnalysisLimits::default());
    assert_eq!(report.completion(), TypedCompletion::Incomplete);
    let stats = report.statistics();
    let exact = AnalysisLimits {
        max_work: stats.work,
        max_entries: stats.entries,
        max_depth: stats.max_depth,
    };
    assert_eq!(
        analyze_typed(&graph, exact).completion(),
        TypedCompletion::Incomplete
    );
    assert_eq!(
        analyze_typed(
            &graph,
            AnalysisLimits {
                max_work: exact.max_work - 1,
                ..exact
            }
        )
        .completion(),
        TypedCompletion::ResourceLimit
    );

    let selector = |name: &str| Selector {
        module: "core.rill".into(),
        name: name.into(),
    };
    let standard_root = root.join("lib/rill/std");
    let source_only = load(Request {
        entry: root.join("app/main.rill"),
        standard_root: standard_root.clone(),
        prelude: Prelude {
            copy: selector("copy"),
            dispose: selector("dispose"),
        },
        target: "aarch64-apple-darwin".into(),
        options: BTreeMap::new(),
        test_mode: mode,
        limits: Limits::default(),
    })
    .unwrap();
    let untrusted = analyze_typed(&source_only, AnalysisLimits::default());
    assert!(untrusted.try_into_role().is_none());
    let registered = graph
        .implementations()
        .iter()
        .find(|imp| {
            report
                .conversion_provider(&imp.declaration)
                .unwrap()
                .is_some()
        })
        .unwrap();
    assert!(matches!(
        untrusted.conversion_provider(&registered.declaration),
        Err(QueryError::InvalidRequest)
    ));

    // Removing a metadata-only module must not block an independent provider or create a receipt.
    let bytes = standard_root.join("bytes.rill");
    let hidden = standard_root.join("bytes.uninstalled");
    fs::rename(&bytes, &hidden).unwrap();
    let partial = capture(root, mode, source);
    fs::rename(&hidden, &bytes).unwrap();
    assert!(partial.modules().iter().all(|m| m.key.path != "bytes.rill"));
    let report = analyze_typed(&partial, AnalysisLimits::default());
    assert_eq!(report.completion(), TypedCompletion::Incomplete);
    let app = partial
        .modules()
        .iter()
        .find(|m| m.key.origin == Origin::Application)
        .unwrap();
    let call = app
        .syntax
        .nodes
        .iter()
        .position(|n| n.span.bytes() == b"tryInto<u8>(text)")
        .unwrap();
    assert!(matches!(
        report
            .bodies()
            .conversion_use(app, call)
            .unwrap()
            .unwrap()
            .provider,
        Some(ConversionProvider::Parse(_))
    ));
}

struct Cleanup(PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
