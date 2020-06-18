use std::io;
use std::io::Write;
use std::process::Command;
use std::sync::Mutex;

use crate::error::print_error;
use crate::exit_codes::ExitCode;

/// Executes a command.
pub fn execute_command(mut cmd: Command, out_perm: &Mutex<()>) -> ExitCode {
    // Spawn the supplied command.
    /* let output = cmd.output(); */
    // This sort of works:
    let child = cmd.spawn().expect("failed to wait on child");
    let output = child
    .wait_with_output()
    .expect("failed to wait on child"); 
    ExitCode::Success
    /* let child_handle = cmd.spawn();
    match child_handle {
        Ok(output) => {
            let exit_code = output.wait();
        }
        Err() => {

        }
    } */
    // Then wait for the command to exit, if it was spawned.
    /* match output {
        Ok(output) => {
            // While this lock is active, this thread will be the only thread allowed
            // to write its outputs.
            let _lock = out_perm.lock().unwrap();

            let stdout = io::stdout();
            let stderr = io::stderr();

            //let _ = stdout.lock().write_all(&output.stdout);
            //let _ = stderr.lock().write_all(&output.stderr);

            if output.status.code() == Some(0) {
                ExitCode::Success
            } else {
                ExitCode::GeneralError
            }
        }
        Err(ref why) if why.kind() == io::ErrorKind::NotFound => {
            print_error(format!("Command not found: {:?}", cmd));
            ExitCode::GeneralError
        }
        Err(why) => {
            print_error(format!("Problem while executing command: {}", why));
            ExitCode::GeneralError
        }
    } */
}
