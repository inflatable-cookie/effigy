use std::io::Read;
use std::process::Child;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use super::super::diagnostics::format_exit_diagnostic;
use super::super::streams::spawn_stream_thread;
use super::super::{ProcessEvent, ProcessEventKind};

pub(super) fn attach_child_stream_threads(
    process_name: String,
    child: Arc<Mutex<Child>>,
    stdout: impl Read + Send + 'static,
    stderr: impl Read + Send + 'static,
    events_tx: &Sender<ProcessEvent>,
) {
    spawn_stream_thread(
        process_name.clone(),
        stdout,
        ProcessEventKind::Stdout,
        ProcessEventKind::StdoutChunk,
        events_tx.clone(),
    );
    spawn_stream_thread(
        process_name.clone(),
        stderr,
        ProcessEventKind::Stderr,
        ProcessEventKind::StderrChunk,
        events_tx.clone(),
    );

    let tx = events_tx.clone();
    thread::spawn(move || loop {
        let status = crate::locks::lock_tolerant(&child).try_wait();
        match status {
            Ok(Some(status)) => {
                let _ = tx.send(ProcessEvent {
                    process: process_name.clone(),
                    kind: ProcessEventKind::Exit,
                    payload: format_exit_diagnostic(status),
                    chunk: None,
                });
                break;
            }
            Ok(None) => thread::sleep(Duration::from_millis(40)),
            Err(err) => {
                let _ = tx.send(ProcessEvent {
                    process: process_name.clone(),
                    kind: ProcessEventKind::Exit,
                    payload: format!("wait-error={err}"),
                    chunk: None,
                });
                break;
            }
        }
    });
}
