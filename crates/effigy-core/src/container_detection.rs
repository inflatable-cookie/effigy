//! Whether this process runs inside a container, and which marker proved it.
//!
//! Four independent markers name a container: Effigy's workspace entrypoint,
//! Docker's `/.dockerenv`, Podman's `/run/.containerenv`, and a container
//! family named in `/proc/1/cgroup`. A marker that could not be read stays
//! distinct from one that is absent, so a caller never learns "no container"
//! from a probe that never answered.

use std::path::PathBuf;

const DOCKER_ENV_PATH: &str = "/.dockerenv";
const PODMAN_ENV_PATH: &str = "/run/.containerenv";
const CGROUP_PATH: &str = "/proc/1/cgroup";
const EFFIGY_WORKSPACE_ENTRYPOINT_PATH: &str = "/usr/local/bin/effigy-entrypoint";

/// The cgroup families that name a container member.
const CGROUP_CONTAINER_MARKERS: &[&str] = &["docker", "containerd", "kubepods"];

/// The marker that established a container.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerMarker {
    /// This is an Effigy workspace container.
    EffigyWorkspaceEntrypoint,
    DockerEnvFile,
    PodmanEnvFile,
    /// `/proc/1/cgroup` names a container family.
    Cgroup,
}

/// One marker observation: what it showed, or what stopped the observation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Observation {
    /// `true` when the marker is present.
    Observed(bool),
    /// The observation could not be made; the reason is kept so the verdict can
    /// stay unknown instead of claiming a host.
    Unreadable(String),
}

/// What a marker probe established.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContainerVerdict {
    /// A readable marker proved this process runs inside a container.
    Container(ContainerMarker),
    /// Every readable marker was absent: this is a host process.
    Host,
    /// A marker could not be read and no readable marker matched. `path` names
    /// the marker that failed, so a caller reports the reason rather than a host.
    Unknown { path: PathBuf, error: String },
}

impl ContainerVerdict {
    /// Whether the probe proved a container. `Unknown` is not proof.
    pub fn is_container(&self) -> bool {
        matches!(self, Self::Container(_))
    }
}
/// Interpret the four markers. The Effigy workspace entrypoint is the most
/// specific, so it is reported first; a present marker wins over an unreadable
/// sibling, because one proof is enough.
pub fn verdict_from_markers(
    effigy_workspace: Observation,
    docker_env_file: Observation,
    podman_env_file: Observation,
    container_cgroup: Observation,
) -> ContainerVerdict {
    let markers = [
        (
            EFFIGY_WORKSPACE_ENTRYPOINT_PATH,
            ContainerMarker::EffigyWorkspaceEntrypoint,
            &effigy_workspace,
        ),
        (
            DOCKER_ENV_PATH,
            ContainerMarker::DockerEnvFile,
            &docker_env_file,
        ),
        (
            PODMAN_ENV_PATH,
            ContainerMarker::PodmanEnvFile,
            &podman_env_file,
        ),
        (CGROUP_PATH, ContainerMarker::Cgroup, &container_cgroup),
    ];
    for (_, marker, observation) in markers {
        if *observation == Observation::Observed(true) {
            return ContainerVerdict::Container(marker);
        }
    }
    for (path, _, observation) in markers {
        if let Observation::Unreadable(error) = observation {
            return ContainerVerdict::Unknown {
                path: PathBuf::from(path),
                error: error.clone(),
            };
        }
    }
    ContainerVerdict::Host
}

/// Probe this process's markers and interpret them.
pub fn detect_container() -> ContainerVerdict {
    verdict_from_markers(
        probe_marker(EFFIGY_WORKSPACE_ENTRYPOINT_PATH),
        probe_marker(DOCKER_ENV_PATH),
        probe_marker(PODMAN_ENV_PATH),
        probe_cgroup(),
    )
}

/// Whether this process is an Effigy workspace container, judged by its own
/// entrypoint marker.
pub fn process_is_inside_effigy_workspace_container() -> bool {
    probe_marker(EFFIGY_WORKSPACE_ENTRYPOINT_PATH) == Observation::Observed(true)
}

fn probe_marker(path: &str) -> Observation {
    match std::fs::metadata(path) {
        Ok(_) => Observation::Observed(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Observation::Observed(false),
        Err(error) => Observation::Unreadable(error.to_string()),
    }
}

fn probe_cgroup() -> Observation {
    match std::fs::read_to_string(CGROUP_PATH) {
        Ok(contents) => Observation::Observed(cgroup_names_container(&contents)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Observation::Observed(false),
        Err(error) => Observation::Unreadable(error.to_string()),
    }
}

/// Whether `/proc/1/cgroup` text names a container family.
fn cgroup_names_container(contents: &str) -> bool {
    CGROUP_CONTAINER_MARKERS
        .iter()
        .any(|marker| contents.contains(marker))
}

#[cfg(test)]
mod tests {
    use super::{
        cgroup_names_container, verdict_from_markers, ContainerMarker, ContainerVerdict,
        Observation,
    };
    use std::path::PathBuf;

    fn verdict(
        effigy_workspace: bool,
        docker_env_file: bool,
        podman_env_file: bool,
        cgroup: bool,
    ) -> ContainerVerdict {
        verdict_from_markers(
            Observation::Observed(effigy_workspace),
            Observation::Observed(docker_env_file),
            Observation::Observed(podman_env_file),
            Observation::Observed(cgroup),
        )
    }

    /// Each marker, on its own, establishes a container and names itself.
    #[test]
    fn verdict_names_the_marker_that_established_a_container() {
        assert_eq!(
            verdict(false, true, false, false),
            ContainerVerdict::Container(ContainerMarker::DockerEnvFile)
        );
        assert_eq!(
            verdict(false, false, true, false),
            ContainerVerdict::Container(ContainerMarker::PodmanEnvFile)
        );
        assert_eq!(
            verdict(true, false, false, false),
            ContainerVerdict::Container(ContainerMarker::EffigyWorkspaceEntrypoint)
        );
        assert_eq!(
            verdict(false, false, false, true),
            ContainerVerdict::Container(ContainerMarker::Cgroup)
        );
    }

    /// The most specific marker is reported when several are present, so the
    /// workspace handoff never mistakes a generic container for its own.
    #[test]
    fn effigy_workspace_entrypoint_outranks_a_generic_docker_marker() {
        assert_eq!(
            verdict(true, true, false, true),
            ContainerVerdict::Container(ContainerMarker::EffigyWorkspaceEntrypoint)
        );
    }

    #[test]
    fn readable_absent_markers_are_a_host() {
        assert_eq!(verdict(false, false, false, false), ContainerVerdict::Host);
    }

    /// An unreadable marker is not a host: the verdict keeps the reason, where
    /// the previous boolean folded it into "not a container".
    #[test]
    fn unreadable_marker_is_unknown_not_host() {
        assert_eq!(
            verdict_from_markers(
                Observation::Observed(false),
                Observation::Unreadable("permission denied".to_owned()),
                Observation::Observed(false),
                Observation::Observed(false),
            ),
            ContainerVerdict::Unknown {
                path: PathBuf::from("/.dockerenv"),
                error: "permission denied".to_owned()
            }
        );
    }

    #[test]
    fn unreadable_cgroup_is_unknown_not_host() {
        assert_eq!(
            verdict_from_markers(
                Observation::Observed(false),
                Observation::Observed(false),
                Observation::Observed(false),
                Observation::Unreadable("input/output error".to_owned()),
            ),
            ContainerVerdict::Unknown {
                path: PathBuf::from("/proc/1/cgroup"),
                error: "input/output error".to_owned()
            }
        );
    }

    /// The cgroup text itself is the marker: a docker, containerd or kubepods
    /// path names a container, and a host slice hierarchy does not.
    #[test]
    fn cgroup_text_names_a_container_family() {
        for contents in [
            "12:pids:/docker/4f1c2d",
            "1:name=systemd:/system.slice/containerd.service",
            "0::/kubepods/burstable/pod0b7c",
        ] {
            assert!(cgroup_names_container(contents), "{contents}");
        }
        for contents in ["0::/", "0::/user.slice/user-501.slice/session-1.scope", ""] {
            assert!(!cgroup_names_container(contents), "{contents}");
        }
    }

    /// One readable proof is enough: an unreadable sibling does not turn a
    /// proven container into an unknown.
    #[test]
    fn a_present_marker_wins_over_an_unreadable_sibling() {
        assert_eq!(
            verdict_from_markers(
                Observation::Observed(false),
                Observation::Observed(true),
                Observation::Observed(false),
                Observation::Unreadable("input/output error".to_owned()),
            ),
            ContainerVerdict::Container(ContainerMarker::DockerEnvFile)
        );
    }
}
