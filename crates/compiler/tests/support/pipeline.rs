use compiler::analysis::ownership::{Completion, Operation, analyze_ownership};
use compiler::analysis::scalars::{BodyOwner, Site};
use compiler::analysis::typed::analyze_typed;
use compiler::analysis::{AnalysisLimits, Statistics};
use compiler::loader::{self, InstalledRequest, Origin};
use std::{
    fs,
    panic::{self, AssertUnwindSafe},
    path::Path,
};

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct ModeSummary {
    pub(crate) test_mode: bool,
    pub(crate) outcome: Outcome,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum Outcome {
    LoadFailed {
        kind: String,
        code: Option<String>,
        message: String,
        text: String,
    },
    Analysed {
        typed: String,
        graph: Option<GraphSummary>,
    },
    Panicked(String),
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct GraphSummary {
    pub(crate) completion: String,
    pub(crate) roots: Vec<RootSummary>,
    pub(crate) findings: Vec<String>,
    pub(crate) statistics: Statistics,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct RootSummary {
    pub(crate) module: String,
    pub(crate) owner: String,
    pub(crate) completion: Completion,
    pub(crate) has_mir: bool,
    pub(crate) blocked_at: Option<String>,
    pub(crate) whole_value_uses: Vec<String>,
}

pub(crate) fn summarize(request: &InstalledRequest, app: &Path, entry: &str) -> [ModeSummary; 2] {
    [false, true].map(|test_mode| {
        let request = InstalledRequest {
            entry: app.join(entry),
            test_mode,
            ..request.clone()
        };
        let outcome = panic::catch_unwind(AssertUnwindSafe(|| outcome(request, app)))
            .unwrap_or_else(|payload| {
                Outcome::Panicked(
                    payload
                        .downcast_ref::<&str>()
                        .map(|text| text.to_string())
                        .or_else(|| payload.downcast_ref::<String>().cloned())
                        .unwrap_or_default(),
                )
            });
        ModeSummary { test_mode, outcome }
    })
}

pub(crate) fn replace_directory(app: &Path, from: &Path) {
    let _ = fs::remove_dir_all(app);
    fs::create_dir_all(app).unwrap();
    copy_tree(from, app);
}

pub(crate) fn replace_with_source(app: &Path, entry: &str, source: &str) {
    let _ = fs::remove_dir_all(app);
    fs::create_dir_all(app).unwrap();
    fs::write(app.join(entry), source).unwrap();
}

fn copy_tree(from: &Path, to: &Path) {
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            fs::create_dir_all(&target).unwrap();
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn outcome(request: InstalledRequest, app: &Path) -> Outcome {
    let graph = match loader::load_installed(request) {
        Ok(graph) => graph,
        Err(failure) => {
            let first = failure.findings.first();
            return Outcome::LoadFailed {
                kind: format!("{:?}", failure.kind),
                code: first.and_then(|finding| finding.code).map(str::to_owned),
                message: first
                    .map_or(failure.message.as_str(), |finding| finding.message.as_str())
                    .replace(&app.display().to_string(), "<app>"),
                text: first
                    .and_then(|finding| finding.span.as_ref())
                    .or(failure.span.as_ref())
                    .map(|span| span.bytes().escape_ascii().to_string())
                    .unwrap_or_default(),
            };
        }
    };
    let typed = analyze_typed(&graph, AnalysisLimits::default());
    let graph = typed.receipt().map(|receipt| {
        let report = analyze_ownership(&receipt, AnalysisLimits::default());
        let roots = report
            .roots()
            .iter()
            .filter(|root| root.site().module.origin == Origin::Application)
            .map(|root| {
                let owner = match root.owner() {
                    BodyOwner::Startup => "startup".to_owned(),
                    BodyOwner::Declaration(id) => {
                        let name = graph
                            .modules()
                            .iter()
                            .find(|module| module.key == id.module)
                            .and_then(|module| module.declarations.iter().find(|d| &d.id == id))
                            .and_then(|declaration| declaration.name.clone());
                        format!(
                            "{:?}:{}",
                            id.kind,
                            name.unwrap_or_else(|| id.start.to_string())
                        )
                    }
                };
                RootSummary {
                    module: root.site().module.path.clone(),
                    owner,
                    completion: root.completion(),
                    has_mir: root.mir().is_some(),
                    blocked_at: root.blocked_at().map(|site| text(&site)),
                    whole_value_uses: root
                        .ordinary_mir()
                        .into_iter()
                        .flat_map(|mir| &mir.blocks)
                        .filter_map(|block| match &block.operation {
                            Operation::Copy { from, .. } if from.projection.is_none() => Some(
                                format!("Copy {}@{}", text(&block.site), block.site.span.start()),
                            ),
                            Operation::Move { from, .. } if from.projection.is_none() => Some(
                                format!("Move {}@{}", text(&block.site), block.site.span.start()),
                            ),
                            _ => None,
                        })
                        .collect(),
                }
            })
            .collect();
        GraphSummary {
            completion: format!("{:?}", report.completion()),
            roots,
            findings: report
                .findings()
                .iter()
                .map(|finding| {
                    format!(
                        "{} {}@{} {:?} related {:?}",
                        finding.code(),
                        finding.site.module.path,
                        finding.site.span.start(),
                        text(&finding.site),
                        text(&finding.related)
                    )
                })
                .collect(),
            statistics: report.statistics(),
        }
    });
    Outcome::Analysed {
        typed: format!("{:?}", typed.completion()),
        graph,
    }
}

fn text(site: &Site<'_>) -> String {
    site.span.bytes().escape_ascii().to_string()
}
