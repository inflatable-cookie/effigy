//! What a managed run's client and its detached supervisor exchange. Both are
//! the same verified binary (I-SV4), so the shapes carry no version history.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// The front end that started a run. It decides only what the client shows
/// and when it lets go; the supervisor's topology never depends on it (I-SV3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::runner) enum Front {
    Tui,
    Text,
    Detached,
}

/// The engine payload: the client's own task invocation, re-resolved on the
/// host by the supervisor, and the state home its runs are recorded under.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(in crate::runner) struct Payload {
    pub(in crate::runner) task: String,
    pub(in crate::runner) args: Vec<String>,
    pub(in crate::runner) front: Front,
    pub(in crate::runner) state_home: PathBuf,
    /// The TUI's terminal size, for workload terminals.
    pub(in crate::runner) terminal: Option<(u16, u16)>,
}

/// A supervisor event for its owning client. Delivery is best effort; every
/// fact here is also in the run journal or a run log (K10).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::runner) enum EngineEvent {
    /// A workload's process is now the run effect at `sequence`.
    Started { process: String, sequence: i64 },
    /// A workload's process ended with `code`.
    Exited { process: String, code: i32 },
    /// Every workload has started.
    StartupFinished,
    /// The run is stopping: workloads, then the environment, are settling.
    Stopping,
    /// What the client reports once the run closes: `error` fails it, with
    /// `code` as the exit status when a step's own code decided it.
    Outcome {
        error: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        code: Option<i32>,
    },
}

/// An owning client's command for one workload of its run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::runner) enum ClientFrame {
    Input { process: String, bytes: Vec<u8> },
    Restart { process: String },
    Terminate { process: String },
}

pub(in crate::runner) fn encode<T: Serialize>(value: &T) -> Vec<u8> {
    serde_json::to_vec(value).unwrap_or_default()
}

pub(in crate::runner) fn decode<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> Option<T> {
    serde_json::from_slice(bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_events_and_client_frames_round_trip() {
        for event in [
            EngineEvent::Started {
                process: "api".into(),
                sequence: 4,
            },
            EngineEvent::Exited {
                process: "api".into(),
                code: 7,
            },
            EngineEvent::StartupFinished,
        ] {
            assert_eq!(decode::<EngineEvent>(&encode(&event)), Some(event));
        }
        let frame = ClientFrame::Input {
            process: "api".into(),
            bytes: b"q".to_vec(),
        };
        assert_eq!(decode::<ClientFrame>(&encode(&frame)), Some(frame));
        assert_eq!(decode::<ClientFrame>(b"not json"), None);
    }
}
