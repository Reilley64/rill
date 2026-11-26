mod check;
#[cfg(feature = "llvm")]
mod compile;
#[cfg(feature = "llvm")]
mod interrupt;
#[cfg(feature = "llvm")]
mod native_libs;

use std::env;
use std::ffi::OsString;
use std::process::ExitCode;

#[cfg(feature = "llvm")]
const HELP: &str = "\
Rill compiler scaffold

Usage: rill <command> [<entry>]
       rill --help
       rill --version

Commands:
  check <entry>    Check without code generation or execution
  build <entry> [-o <path>]
                   Build an executable, ./<entry stem> by default
  run <entry> [<argument>...]
                   Build and run a program, then delete the executable

Commands (not implemented):
  test [<entry>]   Check and run expectations
  lsp             Start the language server

Global commands:
  help, -h, --help        Show this help
  version, -V, --version  Show version

check, build and run read the entry and its imports from the installed layout
beside the rill binary (<prefix>/bin/rill, <prefix>/lib/rill/std and
<prefix>/lib/rill/runtime/<target>), run the front end and print its report on
standard error. check generates no code and executes no startup, user code or
expectation. build and run then lower the program to an object and link it
with the runtime archive of that layout through the host cc. run passes every
argument after the entry to the program untouched, with the entry as argument
zero, and exits with the program's status, or 128 plus the signal number when
a signal ends it. SIGINT or SIGTERM sent to run stops the program, deletes the
executable and exits 130 or 143. build and run leave no executable on a failure. The other
commands do not read source files, generate code, execute user code, run
expectations, or start an LSP server, and do not open
entry paths.

Help and version exit 0. check exits 0 for a compilable program, 1 for a
source finding or a valid program the compiler cannot compile yet (R0605), 2
for an invalid invocation such as an unreadable entry, and 3 when analysis
stops at a limit and is incomplete. build and run use the same statuses, exit
1 when cc is missing or fails, and exit 3 when the backend fails. Invalid
invocations and unavailable
commands exit 2 with an explanation on stderr. Unavailability is a scaffold
limitation, not a compiler check/build/test outcome or an internal compiler
failure. The driver contract also defines interruption (130).
";

#[cfg(not(feature = "llvm"))]
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
    #[cfg_attr(not(feature = "llvm"), allow(unused_mut))]
    let mut raw: Vec<OsString> = env::args_os().skip(1).collect();
    #[cfg(feature = "llvm")]
    let check_allocations = take_check_allocations(&mut raw);
    let program_arguments = cfg!(feature = "llvm") && raw.first().is_some_and(|arg| arg == "run");
    let checked = if program_arguments {
        raw.len().min(2)
    } else {
        raw.len()
    };
    let args: Option<Vec<_>> = raw[..checked].iter().map(|arg| arg.to_str()).collect();
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
            let takes_more = cfg!(feature = "llvm") && matches!(command, "build" | "run");
            if rest.len() > 1 && !takes_more {
                return fail("expected at most one entry path; use 'rill --help'");
            }
            if rest.first().is_some_and(|entry| entry.starts_with('-')) {
                return fail("command options are not supported; use 'rill --help'");
            }
            match (command, rest.first()) {
                ("check", Some(entry)) => check::run(entry),
                ("check", None) => fail("check expects an entry path; use 'rill --help'"),
                #[cfg(feature = "llvm")]
                ("build", Some(entry)) => build(entry, &rest[1..], check_allocations),
                #[cfg(feature = "llvm")]
                ("run", Some(entry)) => compile::run(entry, &raw[2..], check_allocations),
                #[cfg(feature = "llvm")]
                ("build" | "run", None) => fail(&format!(
                    "{command} expects an entry path; use 'rill --help'"
                )),
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

#[cfg(feature = "llvm")]
fn take_check_allocations(raw: &mut Vec<OsString>) -> bool {
    let Some(command) = raw.first().and_then(|command| command.to_str()) else {
        return false;
    };
    let (builds, runs) = (command == "build", command == "run");
    if !builds && !runs {
        return false;
    }
    let mut found = false;
    let mut index = 1;
    while index < raw.len() {
        let arg = &raw[index];
        if arg == "--check-allocations" {
            raw.remove(index);
            found = true;
            continue;
        }
        if runs && !arg.to_str().is_some_and(|arg| arg.starts_with('-')) {
            break;
        }
        index += if builds && arg == "-o" { 2 } else { 1 };
    }
    found
}

#[cfg(feature = "llvm")]
fn build(entry: &str, options: &[&str], check_allocations: bool) -> ExitCode {
    match options {
        [] => compile::build(entry, None, check_allocations),
        ["-o", path] => compile::build(entry, Some(path), check_allocations),
        ["-o"] => fail("-o expects an output path; use 'rill --help'"),
        _ => fail("build accepts only -o <path> after the entry; use 'rill --help'"),
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
