use std::{
    fs, io,
    process::{Command, Output, Stdio},
    sync::Mutex,
};

pub(crate) fn run_copied_probe(command: &mut Command) -> io::Result<Output> {
    static LAUNCH: Mutex<()> = Mutex::new(());
    let child = {
        // A concurrent fork can hold the writable copy descriptor until exec,
        // causing Linux to reject an otherwise finished copy with ETXTBSY.
        let _launch = LAUNCH.lock().expect("probe launch lock poisoned");
        fs::copy(std::env::current_exe()?, command.get_program())?;
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?
    };
    let output = child.wait_with_output()?;
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
