use std::collections::HashMap;
#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;
use std::process::Child;
use std::sync::{Arc, Mutex};

use super::ProcessSpec;

pub(super) fn collect_exit_diagnostics(
    specs: &HashMap<String, ProcessSpec>,
    process_map: &HashMap<String, Arc<Mutex<Child>>>,
) -> Vec<(String, String)> {
    let mut diagnostics = specs
        .keys()
        .map(|name| {
            let diagnostic = if let Some(child) = process_map.get(name) {
                match child.lock().expect("child lock").try_wait() {
                    Ok(Some(status)) => format_exit_diagnostic(status),
                    Ok(None) => "running".to_owned(),
                    Err(err) => format!("wait-error={err}"),
                }
            } else {
                "not-tracked".to_owned()
            };
            (name.clone(), diagnostic)
        })
        .collect::<Vec<(String, String)>>();
    diagnostics.sort_by(|a, b| a.0.cmp(&b.0));
    diagnostics
}

pub(super) fn format_exit_diagnostic(status: std::process::ExitStatus) -> String {
    #[cfg(unix)]
    {
        if let Some(code) = status.code() {
            return format!("exit={code}");
        }
        if let Some(signal) = status.signal() {
            return format!("signal={signal}");
        }
        "exit=unknown".to_owned()
    }
    #[cfg(not(unix))]
    {
        format!("exit={}", status.code().unwrap_or(-1))
    }
}
