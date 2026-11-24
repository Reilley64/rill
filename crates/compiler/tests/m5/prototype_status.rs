use crate::pipeline::{self, ModeSummary, Outcome};
use compiler::analysis::ownership::Completion;
use std::{
    fs,
    path::{Path, PathBuf},
};

const COMMITTED: &str = include_str!("prototype-status.tsv");
const HELLO_WORLD: &str = "import std::log;\n\nlog(\"hello world\");\n";
const HEADER: &str = "# Prototype status: one row per program and installed load mode, regenerated through the public pipeline by prototype_status::committed_table_matches_the_pipeline (`mise run status:prototypes`).\n\
# Columns: program, mode (run, test), load (ok, or the first failure as kind:code:message:source text), typed (typed completion, - without one), then the application roots of the ownership graph by completion (checked counts roots with MIR; - without a receipt).\n\
program\tmode\tload\ttyped\tchecked\tincomplete\tsource_rejected\tresource_limit\n";

struct Program {
    name: String,
    source: Source,
}

enum Source {
    Text(String),
    Directory { root: PathBuf, entry: String },
}

fn programs(prototypes: &Path) -> Vec<Program> {
    let mut found = Vec::new();
    for entry in fs::read_dir(prototypes).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_str().unwrap().to_owned();
        if path.is_dir() {
            let mains = if path.join("main.rill").exists() {
                vec!["main.rill".to_owned()]
            } else {
                let mut files: Vec<String> = fs::read_dir(&path)
                    .unwrap()
                    .map(|file| file.unwrap().file_name().into_string().unwrap())
                    .filter(|file| file.ends_with(".rill"))
                    .collect();
                files.sort();
                files
            };
            for main in mains {
                found.push(Program {
                    name: format!("prototypes/{name}/{main}"),
                    source: Source::Directory {
                        root: path.clone(),
                        entry: main,
                    },
                });
            }
        } else if name.ends_with(".rill") {
            found.push(Program {
                name: format!("prototypes/{name}"),
                source: Source::Text(fs::read_to_string(&path).unwrap()),
            });
        }
    }
    found.sort_by(|a, b| a.name.cmp(&b.name));
    found.insert(
        0,
        Program {
            name: "hello-world".into(),
            source: Source::Text(HELLO_WORLD.into()),
        },
    );
    found
}

fn row(program: &str, summary: &ModeSummary) -> String {
    let mode = if summary.test_mode { "test" } else { "run" };
    let (load, typed, counts) = match &summary.outcome {
        Outcome::LoadFailed {
            kind,
            code,
            message,
            text,
        } => (
            format!(
                "{kind}:{}:{message}:{}",
                code.as_deref().unwrap_or("-"),
                text.chars().take(60).collect::<String>()
            ),
            "-".to_owned(),
            None,
        ),
        Outcome::Panicked(_) => ("panic".to_owned(), "-".to_owned(), None),
        Outcome::Analysed { typed, graph } => (
            "ok".to_owned(),
            typed.clone(),
            graph.as_ref().map(|graph| {
                let count = |completion: Completion| {
                    graph
                        .roots
                        .iter()
                        .filter(|root| root.completion == completion)
                        .count()
                };
                [
                    graph
                        .roots
                        .iter()
                        .filter(|root| root.completion == Completion::Checked && root.has_mir)
                        .count(),
                    count(Completion::Incomplete),
                    count(Completion::SourceRejected),
                    count(Completion::ResourceLimit),
                ]
            }),
        ),
    };
    let counts = counts.map_or_else(
        || ["-".to_owned(), "-".into(), "-".into(), "-".into()],
        |counts| counts.map(|count| count.to_string()),
    );
    format!("{program}\t{mode}\t{load}\t{typed}\t{}", counts.join("\t"))
}

#[test]
fn committed_table_matches_the_pipeline() {
    crate::installed(
        "prototype_status::committed_table_matches_the_pipeline",
        |request| {
            let app = request.entry.parent().unwrap().to_path_buf();
            let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
            let mut table = HEADER.to_owned();
            for program in programs(&repo.join("prototypes")) {
                let entry = match &program.source {
                    Source::Text(source) => {
                        pipeline::replace_with_source(&app, "main.rill", source);
                        "main.rill"
                    }
                    Source::Directory { root, entry } => {
                        pipeline::replace_directory(&app, root);
                        entry
                    }
                };
                for summary in pipeline::summarize(&request, &app, entry) {
                    table += &row(&program.name, &summary);
                    table.push('\n');
                }
            }
            if std::env::var_os("RILL_PROTOTYPE_STATUS_WRITE").is_some() {
                fs::write(
                    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/m5/prototype-status.tsv"),
                    &table,
                )
                .unwrap();
                return;
            }
            let changed: Vec<String> = COMMITTED
                .lines()
                .filter(|line| !table.lines().any(|now| now == *line))
                .map(|line| format!("- {line}"))
                .chain(
                    table
                        .lines()
                        .filter(|line| !COMMITTED.lines().any(|then| then == *line))
                        .map(|line| format!("+ {line}")),
                )
                .collect();
            assert!(
                changed.is_empty() && table == COMMITTED,
                "the committed prototype status differs; regenerate it with `mise run status:prototypes` and review the diff:\n{}",
                changed.join("\n")
            );
        },
    );
}

#[test]
fn rows_count_the_roots_by_completion() {
    crate::installed(
        "prototype_status::rows_count_the_roots_by_completion",
        |request| {
            let app = request.entry.parent().unwrap().to_path_buf();
            pipeline::replace_with_source(
                &app,
                "main.rill",
                &format!(
                    "{}{}",
                    crate::IMPORTS,
                    r#"
                function moved(text: string): string {
                    let taken = text;
                    return taken;
                }
                function reused(flag: bool, text: string): string {
                    if (flag) { let taken = text; }
                    return text;
                }
                function called(): i32 { let f = () => 1i32; return f(); }
            "#
                ),
            );
            let rows: Vec<String> = pipeline::summarize(&request, &app, "main.rill")
                .iter()
                .map(|summary| row("counted", summary))
                .collect();
            assert_eq!(
                rows,
                [
                    "counted\trun\tok\tChecked\t2\t1\t1\t0",
                    "counted\ttest\tok\tChecked\t2\t1\t1\t0"
                ]
            );
        },
    );
}
