mod check;

use std::env;
use std::process::ExitCode;

const HELP: &str = "\
Rill compiler scaffold

Usage: rill <command> [<entry>]
       rill --help
       rill --version

Commands:
  check <entry>    Check without code generation or execution

Commands (not implemented):
  build [<entry>]  Build an executable
  run [<entry>]    Build and run a program
  test [<entry>]   Check and run expectations
  lsp             Start the language server

Global commands:
  help, -h, --help        Show this help
  version, -V, --version  Show version

check reads the entry and its imports from the installed layout beside the
rill binary (<prefix>/bin/rill, <prefix>/lib/rill/std and
<prefix>/lib/rill/runtime/<target>), runs the front end and prints its report
on standard error. It generates no code and executes no startup, user code or
expectation. The other commands do not read source files, generate code,
execute user code, run expectations, or start an LSP server, and do not open
entry paths. Command options and additional arguments are not supported.

Help and version exit 0. check exits 0 for a compilable program, 1 for a
source finding or a valid program the compiler cannot compile yet (R0605), 2
for an invalid invocation such as an unreadable entry, and 3 when analysis
stops at a limit and is incomplete. Invalid invocations and unavailable
commands exit 2 with an explanation on stderr. Unavailability is a scaffold
limitation, not a compiler check/build/test outcome or an internal compiler
failure. The driver contract also defines interruption (130).
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
            match (command, rest.first()) {
                ("check", Some(entry)) => check::run(entry),
                ("check", None) => fail("check expects an entry path; use 'rill --help'"),
                _ => unavailable(command),
            }
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
