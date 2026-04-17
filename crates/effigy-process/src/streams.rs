use std::io::Read;
use std::sync::mpsc::Sender;
use std::thread;

use super::{ProcessEvent, ProcessEventKind};

pub(super) fn spawn_stream_thread(
    process: String,
    mut reader: impl Read + Send + 'static,
    line_kind: ProcessEventKind,
    chunk_kind: ProcessEventKind,
    tx: Sender<ProcessEvent>,
) {
    thread::spawn(move || {
        let mut buf = [0u8; 4096];
        let mut line_buffer = Vec::<u8>::new();
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(read) => {
                    let chunk = buf[..read].to_vec();
                    let _ = tx.send(ProcessEvent {
                        process: process.clone(),
                        kind: chunk_kind.clone(),
                        payload: String::from_utf8_lossy(&chunk).into_owned(),
                        chunk: Some(chunk.clone()),
                    });
                    line_buffer.extend_from_slice(&chunk);
                    emit_complete_lines(&tx, &process, &line_kind, &mut line_buffer);
                }
                Err(_) => break,
            }
        }
        if !line_buffer.is_empty() {
            let line = decode_line(&line_buffer);
            let _ = tx.send(ProcessEvent {
                process,
                kind: line_kind,
                payload: line,
                chunk: None,
            });
        }
    });
}

fn emit_complete_lines(
    tx: &Sender<ProcessEvent>,
    process: &str,
    line_kind: &ProcessEventKind,
    line_buffer: &mut Vec<u8>,
) {
    loop {
        let Some(index) = line_buffer.iter().position(|byte| *byte == b'\n') else {
            break;
        };
        let line = line_buffer.drain(..=index).collect::<Vec<u8>>();
        let text = decode_line(&line);
        let _ = tx.send(ProcessEvent {
            process: process.to_owned(),
            kind: line_kind.clone(),
            payload: text,
            chunk: None,
        });
    }
}

fn decode_line(line: &[u8]) -> String {
    let mut slice = line;
    if slice.ends_with(b"\n") {
        slice = &slice[..slice.len() - 1];
    }
    if slice.ends_with(b"\r") {
        slice = &slice[..slice.len() - 1];
    }
    String::from_utf8_lossy(slice).into_owned()
}
