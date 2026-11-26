use std::{
    io,
    path::Path,
    process::{Command, Output},
};

pub(crate) fn run_copied_probe(command: &mut Command) -> io::Result<Output> {
    rill_test_support::stage_current_exe(Path::new(command.get_program()))?;
    let output = rill_test_support::run_staged(command)?;
    if std::env::var_os("RILL_M5_PROBE_VERBOSE").is_some() {
        print!("{}", String::from_utf8_lossy(&output.stdout));
        eprint!("{}", String::from_utf8_lossy(&output.stderr));
    }
    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("running 1 test\n") && stdout.contains("test result: ok. 1 passed;"),
            "probe child ran no test or more than one:\n{stdout}"
        );
    }
    Ok(output)
}
