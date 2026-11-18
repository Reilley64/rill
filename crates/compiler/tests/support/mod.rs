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
    child.wait_with_output()
}
