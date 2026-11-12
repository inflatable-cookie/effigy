//! The runtime engine's own answer about whether it is running.
//!
//! "Is the engine up?" is answered by the engine's text on a failed command,
//! so the vocabulary belongs to the crate that runs it. This module is that
//! vocabulary's one decode: it reads an engine's answer once and hands back
//! [`EngineAbsence`], and no caller matches the text again (effigy#933).

use std::fmt;

/// The engine a command addressed, as the engine itself reported: it is not
/// running. Callers decide what a stopped engine means for their command; the
/// text that proved it never leaves this module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineAbsence {
    /// Colima's own daemon is not running. The profile is carried when the
    /// engine's answer names one, so a refusal can point at that profile.
    Colima { profile: Option<String> },
    /// The Docker daemon is unreachable from the command that asked.
    DockerDaemon,
}

impl EngineAbsence {
    /// Decode one completed command's output into the engine's answer, when the
    /// engine said it is not running. `stdout` is read too: `docker` prints the
    /// daemon's refusal on either stream depending on its own version.
    ///
    /// This is the only place these bytes are read as an engine state; every
    /// caller keeps the typed answer instead of matching message text.
    pub fn observed_in(stdout: &str, stderr: &str) -> Option<Self> {
        if let Some(profile) = colima_absence(stderr) {
            return Some(Self::Colima { profile });
        }
        let answered = |text: &str| {
            text.contains("Cannot connect to the Docker daemon")
                || text.contains("daemon is not running")
        };
        (answered(stderr) || answered(stdout)).then_some(Self::DockerDaemon)
    }
}

/// Colima's "is not running" line for its default daemon or one profile. The
/// profile name is only carried when the line states it.
fn colima_absence(stderr: &str) -> Option<Option<String>> {
    const DEFAULT: &str = "level=fatal msg=\"colima is not running\"";
    if stderr.contains(DEFAULT) {
        return Some(None);
    }
    const PREFIX: &str = "level=fatal msg=\"colima [profile=";
    let start = stderr.find(PREFIX)? + PREFIX.len();
    let rest = &stderr[start..];
    let end = rest.find("] is not running\"")?;
    Some(Some(rest[..end].to_owned()))
}

impl fmt::Display for EngineAbsence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Colima {
                profile: Some(profile),
            } => write!(f, "Colima profile `{profile}`"),
            Self::Colima { profile: None } => f.write_str("Colima"),
            Self::DockerDaemon => f.write_str("the Docker daemon"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::EngineAbsence;

    #[test]
    fn a_colima_answer_that_names_no_profile_is_the_default_daemon() {
        let stderr = "time=\"2026-04-20T08:31:53+01:00\" level=fatal msg=\"colima is not running\"";
        assert_eq!(
            EngineAbsence::observed_in("", stderr),
            Some(EngineAbsence::Colima { profile: None })
        );
    }

    #[test]
    fn a_colima_answer_names_the_profile_it_could_not_reach() {
        let stderr = "level=fatal msg=\"colima [profile=effigy-release] is not running\"";
        assert_eq!(
            EngineAbsence::observed_in("", stderr),
            Some(EngineAbsence::Colima {
                profile: Some("effigy-release".to_owned())
            })
        );
    }

    #[test]
    fn a_docker_daemon_refusal_is_read_from_either_stream() {
        let on_stderr =
            "Cannot connect to the Docker daemon at unix:///var/run/docker.sock. Is the docker daemon running?";
        let on_stdout = "the daemon is not running";
        for (stdout, stderr) in [(on_stdout, on_stderr), ("", on_stdout)] {
            assert_eq!(
                EngineAbsence::observed_in(stdout, stderr),
                Some(EngineAbsence::DockerDaemon)
            );
        }
    }

    #[test]
    fn any_other_failure_is_not_an_engine_answer() {
        for stderr in [
            "",
            "port is already allocated",
            "Error: No such container: web",
            "level=fatal msg=\"another colima command is running\"",
        ] {
            assert_eq!(EngineAbsence::observed_in("", stderr), None, "{stderr}");
        }
    }

    #[test]
    fn the_typed_answer_names_the_engine_a_person_should_start() {
        assert_eq!(
            EngineAbsence::Colima {
                profile: Some("effigy".to_owned())
            }
            .to_string(),
            "Colima profile `effigy`"
        );
        assert_eq!(EngineAbsence::DockerDaemon.to_string(), "the Docker daemon");
    }
}
