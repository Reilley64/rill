use super::*;

#[test]
fn scratch_probe() {
    let mut out = String::new();
    probe_with_typed(
        "rfc_cases::o12::scratch_probe",
        &std::fs::read_to_string(std::env::var("O12_SRC").unwrap()).unwrap(),
        |_, module, receipt, _| {
            out.clear();
            for decl in module.declarations.iter().filter(|d| d.id.kind == DeclarationKind::Function) {
                let r = lower_body(receipt, module, &decl.id, AnalysisLimits::default()).unwrap();
                out.push_str(&format!(
                    "PROBE {:?}: {:?} {:?} blocked={:?} findings={:?}\n",
                    decl.name, r.completion(), r.statistics(),
                    r.blocked_at().map(|s| text(&s)),
                    r.findings().iter().map(|f| (f.code(), text(&f.site), text(&f.related))).collect::<Vec<_>>()
                ));
                if let Some(mir) = r.mir() {
                    for op in linear(mir, mir.entry) { out.push_str(&format!("   {}\n", brief(op))); }
                }
            }
        },
    );
    panic!("{out}");
}

fn pl(place: &compiler::analysis::ownership::Place<'_>) -> String {
    let root = String::from_utf8_lossy(place.root.span.bytes()).into_owned();
    match place.field() {
        Some(field) => format!("{root}.{}", field.name.as_deref().unwrap()),
        None => root,
    }
}

fn brief(op: &Operation<'_>) -> String {
    match op {
        Operation::Copy { from, into } => format!("Copy {} -> {:?}", pl(from), into.as_ref().map(pl)),
        Operation::Move { from, into } => format!("Move {} -> {:?}", pl(from), into.as_ref().map(pl)),
        Operation::Initialize(p) => format!("Initialize {}", pl(p)),
        Operation::Export(p) => format!("Export {}", pl(p)),
        Operation::Borrow { source, holder, exclusive } => format!("Borrow {} -> {} excl={exclusive}", pl(source), pl(holder)),
        Operation::CallIntoTemporary { arguments, into, .. } => format!("CallIntoTemporary [{}] -> {}", arguments.iter().map(pl).collect::<Vec<_>>().join(", "), pl(into)),
        Operation::CopyReferent { from, into } => format!("CopyReferent {} -> {}", pl(from), pl(into)),
        Operation::Cleanup(actions) => format!("Cleanup [{}]", actions.iter().map(|a| match a {
            Obligation::EndStorage(p) => format!("end {}", pl(p)),
            other => format!("{:?}", other).chars().take(40).collect(),
        }).collect::<Vec<_>>().join(", ")),
        other => format!("{:?}", other).chars().take(60).collect(),
    }
}
