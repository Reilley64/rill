use super::*;
use compiler::analysis::ownership::{RootReport, analyze_ownership};
use compiler::analysis::scalars::BodyOwner;
use std::collections::BTreeMap;

const BLESS: &str = "RILL_BLESS_MIR";
const HEADER: &str = "# Digests of the ownership output over the legacy-caps fixtures, checked by m5::legacy_mir. Regenerate: RILL_BLESS_MIR=1 mise run test:m5 -- legacy_mir\n# Columns: fixture, mode, root, digest. A root is a module path and function, a `disposer` root, `standard` (every standard-library root together) or `graph` (completion and findings).\n";

#[test]
fn emitted_mir_matches_published_digests() {
    installed(
        "legacy_mir::emitted_mir_matches_published_digests",
        |request| {
            let tests = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/m5");
            let app = request.entry.parent().unwrap();
            let install = app.parent().unwrap();
            let installs = [install.to_path_buf(), fs::canonicalize(install).unwrap()]
                .map(|path| format!("{path:?}").trim_matches('"').to_string());
            let mut fixtures = fs::read_dir(tests.join("legacy-caps"))
                .unwrap()
                .map(|entry| entry.unwrap().file_name().into_string().unwrap())
                .collect::<Vec<_>>();
            fixtures.sort();
            let mut observed = BTreeMap::new();
            for fixture in &fixtures {
                for stale in fs::read_dir(app).unwrap() {
                    fs::remove_file(stale.unwrap().path()).unwrap();
                }
                for file in fs::read_dir(tests.join("legacy-caps").join(fixture)).unwrap() {
                    let file = file.unwrap();
                    fs::copy(file.path(), app.join(file.file_name())).unwrap();
                }
                for test_mode in [false, true] {
                    let mode = if test_mode { "test" } else { "normal" };
                    let graph = loader::load_installed(InstalledRequest {
                        test_mode,
                        ..request.clone()
                    })
                    .unwrap();
                    let typed = analyze_typed(&graph, AnalysisLimits::default());
                    let mut rows = Vec::new();
                    match typed.receipt() {
                        None => rows.push(("-".to_string(), "no-receipt".to_string())),
                        Some(receipt) => {
                            let report = analyze_ownership(&receipt, AnalysisLimits::default());
                            let mut standard = String::new();
                            for (kind, roots) in [
                                ("", report.roots()),
                                ("disposer ", report.active_disposers()),
                            ] {
                                for root in roots {
                                    let text = stable(&format!("{root:?}"), &installs);
                                    if root.site().module.origin == Origin::Standard {
                                        standard.push_str(&text);
                                    } else {
                                        rows.push((
                                            format!("{kind}{}", subject(&graph, root)),
                                            digest(&text),
                                        ));
                                    }
                                }
                            }
                            rows.push(("standard".to_string(), digest(&standard)));
                            let graph_text = format!(
                                "{:?} {}",
                                report.completion(),
                                stable(&format!("{:?}", report.findings()), &installs)
                            );
                            rows.push(("graph".to_string(), digest(&graph_text)));
                        }
                    }
                    for (root, digest) in rows {
                        let key = format!("{fixture}\t{mode}\t{root}");
                        assert!(
                            observed.insert(key.clone(), digest).is_none(),
                            "two rows for {key}"
                        );
                    }
                }
            }

            let manifest = tests.join("legacy-mir.tsv");
            if std::env::var_os(BLESS).is_some() {
                let mut text = HEADER.to_string();
                for (key, digest) in &observed {
                    text.push_str(&format!("{key}\t{digest}\n"));
                }
                fs::write(&manifest, text).unwrap();
                return;
            }
            let mut published = BTreeMap::new();
            for line in fs::read_to_string(&manifest).unwrap().lines() {
                if !line.starts_with('#') {
                    let (key, digest) = line.rsplit_once('\t').unwrap();
                    published.insert(key.to_string(), digest.to_string());
                }
            }
            let mut failures = Vec::new();
            for (key, now) in &observed {
                match published.get(key) {
                    Some(then) if then == now => {}
                    Some(then) => {
                        failures.push(format!("changed {key}: published {then}; now {now}"))
                    }
                    None => failures.push(format!("unpublished {key}: now {now}")),
                }
            }
            for key in published.keys().filter(|key| !observed.contains_key(*key)) {
                failures.push(format!("no longer produced {key}"));
            }
            assert!(
                failures.is_empty(),
                "{} rows of legacy-mir.tsv (fixture, mode, root) differ; if the emitted MIR was meant to change, regenerate with `{BLESS}=1 mise run test:m5 -- legacy_mir`:\n{}",
                failures.len(),
                failures.join("\n")
            );
        },
    );
}

fn subject(graph: &loader::Graph, root: &RootReport<'_>) -> String {
    let module = graph
        .modules()
        .iter()
        .find(|m| &m.key == root.site().module)
        .unwrap();
    let name = match root.owner() {
        BodyOwner::Startup => "Startup".to_string(),
        BodyOwner::Declaration(id) => {
            let declaration = module.declarations.iter().find(|d| &d.id == id).unwrap();
            match &declaration.name {
                Some(name) if id.kind == loader::DeclarationKind::Function => {
                    format!("Function:{name}")
                }
                _ => format!("{:?}@{}", id.kind, id.start),
            }
        }
    };
    format!("{}#{name}", module.key.path)
}

fn stable(debug: &str, installs: &[String; 2]) -> String {
    const SNAPSHOT: &str = "snapshot: Snapshot(";
    let mut text = String::with_capacity(debug.len() / 16);
    let mut rest = debug;
    while let Some(at) = rest.find(SNAPSHOT) {
        text.push_str(&rest[..at]);
        let mut depth = 1;
        let mut quoted = false;
        let mut escaped = false;
        let mut end = at + SNAPSHOT.len();
        for byte in rest[end..].bytes() {
            end += 1;
            match byte {
                _ if escaped => escaped = false,
                b'\\' if quoted => escaped = true,
                b'"' => quoted = !quoted,
                b'(' if !quoted => depth += 1,
                b')' if !quoted => depth -= 1,
                _ => {}
            }
            if depth == 0 {
                break;
            }
        }
        assert_eq!(depth, 0, "unterminated snapshot in the Debug text");
        rest = rest[end..].strip_prefix(", ").unwrap();
    }
    text.push_str(rest);
    let mut longest_first = installs.each_ref();
    longest_first.sort_by_key(|install| std::cmp::Reverse(install.len()));
    for install in longest_first {
        text = text.replace(install.as_str(), "<install>");
    }
    for (what, needle) in [
        ("a snapshot", "Snapshot"),
        ("the install path", installs[0].as_str()),
        ("the canonical install path", installs[1].as_str()),
        ("a pointer", "0x"),
    ] {
        if let Some(at) = text.find(needle) {
            let from = text.floor_char_boundary(at.saturating_sub(80));
            let to = text.ceil_char_boundary((at + 80).min(text.len()));
            panic!("the stable text holds {what}: {}", &text[from..to]);
        }
    }
    text
}

fn digest(text: &str) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in text.bytes() {
        hash = (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}
