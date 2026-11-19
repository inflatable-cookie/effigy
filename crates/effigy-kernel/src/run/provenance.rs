//! One bounded, redacted operator event for a gateway action.
//!
//! Recording is diagnostic evidence. It never authorizes, refuses or delays an
//! operator action and it never reads the process table: the event names what
//! the invoking process itself publishes.

/// Longest text kept for the cause or the caller name.
pub const TEXT_LIMIT: usize = 96;

/// Which operator action an event records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OperatorAction {
    Up,
    Down,
    Setup,
}
impl OperatorAction {
    /// The existing event name for a stop is kept; `up` and `setup` follow it.
    pub fn event(self) -> &'static str {
        match self {
            Self::Up => "gateway_up",
            Self::Down => "gateway_stop",
            Self::Setup => "gateway_setup",
        }
    }
}

/// The one operator event: which action ran, who asked, from where and why.
/// `caller` is the invoking program's own file name, so a flag value or a
/// deeper argument never reaches the log.
pub fn event(
    action: OperatorAction,
    cause: &str,
    pid: u32,
    argv: &[&str],
    cwd: Option<&std::path::Path>,
) -> serde_json::Value {
    serde_json::json!({
        "event": action.event(),
        "action": action,
        "pid": pid,
        "caller": caller(argv.first().copied()),
        "cwd": cwd,
        "cause": bounded(cause, TEXT_LIMIT),
    })
}

/// The program name of `argv[0]`, bounded. An unreadable or empty argument
/// records the same bounded absence instead of a guess.
fn caller(argv0: Option<&str>) -> String {
    let Some(argv0) = argv0 else {
        return "unknown".into();
    };
    let name = std::path::Path::new(argv0)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(argv0);
    if name.is_empty() {
        "unknown".into()
    } else {
        bounded(name, TEXT_LIMIT)
    }
}

fn bounded(value: &str, limit: usize) -> String {
    let mut bounded = String::new();
    for ch in value.chars() {
        if bounded.chars().count() == limit {
            bounded.push('…');
            break;
        }
        bounded.push(if ch.is_control() { ' ' } else { ch });
    }
    bounded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_operator_event_names_the_action_caller_and_bounded_cause() {
        let value = event(
            OperatorAction::Down,
            "route table empty for 300 s",
            42,
            &["/usr/local/bin/effigy", "gateway", "down"],
            Some(std::path::Path::new("/work")),
        );
        assert_eq!(value["event"], "gateway_stop");
        assert_eq!(value["action"], "down");
        assert_eq!(value["pid"], 42);
        assert_eq!(value["caller"], "effigy");
        assert_eq!(value["cwd"], "/work");
        assert_eq!(value["cause"], "route table empty for 300 s");
        assert!(value.get("parents").is_none() && value.get("markers").is_none());
    }

    #[test]
    fn the_event_is_bounded_and_never_keeps_a_flag_value_or_control_byte() {
        let secret = "s".repeat(TEXT_LIMIT * 2);
        let value = event(
            OperatorAction::Up,
            "started",
            1,
            &[secret.as_str(), "--token=hunter2"],
            None,
        );
        let caller = value["caller"].as_str().unwrap();
        assert!(caller.chars().count() <= TEXT_LIMIT + 1, "{caller}");
        let text = value.to_string();
        assert!(!text.contains("hunter2"), "{text}");
        let noisy = event(OperatorAction::Setup, "bad\u{1b}[31m", 1, &[], None);
        assert_eq!(noisy["cause"], "bad [31m");
        assert_eq!(noisy["caller"], "unknown");
    }
}
