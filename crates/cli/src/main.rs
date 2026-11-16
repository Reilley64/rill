//! Owns command-line parsing and composition of the compiler, LSP, and optional
//! LLVM backend. Only help, version, and invocation validation are implemented;
//! compiler operations and LSP startup are absent.

use std::env;
use std::process::ExitCode;

const HELP: &str = "\
Rill compiler scaffold

Usage: rill <command> [<entry>]
       rill --help
       rill --version

Commands (not implemented):
  build [<entry>]  Build an executable
  check [<entry>]  Check without code generation or execution
  run [<entry>]    Build and run a program
  test [<entry>]   Check and run expectations
  lsp             Start the language server

Global commands:
  help, -h, --help        Show this help
  version, -V, --version  Show version

This scaffold does not read source files, generate code, execute startup or
user code, run expectations, or start an LSP server. Entry paths are not opened.
Command options and additional arguments are not supported.

Help and version exit 0. Invalid invocations and unavailable commands exit 2
with an explanation on stderr. Unavailability is a scaffold limitation, not
a compiler check/build/test outcome or an internal compiler failure. The future
driver contract distinguishes operation failure (1), invalid configuration (2),
controlled internal failure (3), and interruption (130).
";

fn main() -> ExitCode {
    let args: Vec<_> = env::args_os().skip(1).collect();
    let args: Option<Vec<_>> = args.iter().map(|arg| arg.to_str()).collect();
    let Some(args) = args else {
        return fail("arguments must be valid UTF-8");
    };

    let Some(command) = args.first().copied() else {
        return fail("expected a command; use 'rill --help'");
    };
    let rest = &args[1..];

    match command {
        "help" | "-h" | "--help" if rest.is_empty() => {
            print!("{HELP}");
            ExitCode::SUCCESS
        }
        "version" | "-V" | "--version" if rest.is_empty() => {
            println!("rill {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        "help" | "-h" | "--help" | "version" | "-V" | "--version" => {
            fail("help and version do not accept additional arguments")
        }
        "build" | "check" | "run" | "test" => {
            if rest.len() > 1 {
                return fail("expected at most one entry path; use 'rill --help'");
            }
            if rest.first().is_some_and(|entry| entry.starts_with('-')) {
                return fail("command options are not supported; use 'rill --help'");
            }
            unavailable(command)
        }
        "lsp" => {
            if !rest.is_empty() {
                return fail("lsp does not accept arguments");
            }
            unavailable(command)
        }
        _ => fail("unknown command; use 'rill --help'"),
    }
}

fn unavailable(command: &str) -> ExitCode {
    fail(&format!(
        "'{command}' is not implemented in this scaffold; no compiler operation or LSP server was started"
    ))
}

fn fail(message: &str) -> ExitCode {
    eprintln!("rill: {message}");
    ExitCode::from(2)
}
