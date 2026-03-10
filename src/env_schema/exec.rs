use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use super::error::EnvSchemaError;

pub(super) fn run_exec_command(
    command: &str,
    timeout: Duration,
    cwd: &Path,
) -> Result<String, EnvSchemaError> {
    let mut child = Command::new("sh")
        .arg("-c")
        .arg(command)
        .current_dir(cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| EnvSchemaError::ExecSpawn {
            command: command.to_owned(),
            error,
        })?;

    let mut stdout_handle = child.stdout.take().expect("stdout piped");
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        let mut buf = String::new();
        stdout_handle.read_to_string(&mut buf).ok();
        tx.send(buf).ok();
    });

    match rx.recv_timeout(timeout) {
        Ok(output) => {
            let status = child.wait().map_err(|error| EnvSchemaError::ExecSpawn {
                command: command.to_owned(),
                error,
            })?;

            if !status.success() {
                let mut stderr_output = String::new();
                if let Some(mut stderr) = child.stderr.take() {
                    stderr.read_to_string(&mut stderr_output).ok();
                }
                return Err(EnvSchemaError::ExecFailed {
                    command: command.to_owned(),
                    code: status.code(),
                    stderr: stderr_output.trim().to_owned(),
                });
            }

            Ok(output.trim().to_owned())
        }
        Err(_) => {
            child.kill().ok();
            child.wait().ok();
            Err(EnvSchemaError::ExecTimeout {
                command: command.to_owned(),
                timeout,
            })
        }
    }
}
