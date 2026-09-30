//! Bounded runtime participation detection used before container inventory.
//!
//! Inventory completeness is what licenses stale loopback reclamation, so a
//! runtime may be skipped only when it is *proven* unable to own running
//! containers. A runtime that is running, reachable, or whose state cannot be
//! established keeps its inventory authoritative: a later probe failure stays
//! actionable and prevents uncertain reclamation.
//!
//! Detection never launches Docker, Colima, or a container. Docker
//! participation is resolved from every endpoint it could be using —
//! `DOCKER_CONTEXT`, `DOCKER_HOST`, the stored current context, then the
//! platform default socket — and verified with a non-launching Unix-socket
//! probe. Colima participation follows from its CLI or leftover Colima/Lima
//! state, because a Colima VM can outlive its client CLI.
//!
//! Preferred backend, CLI presence, a missing socket, unreadable state and
//! stderr text are never sufficient on their own to conclude that a runtime
//! owns nothing. Only an absent *effective* local endpoint, with no configured
//! remote endpoint and no Colima runtime state, proves a runtime inactive.

use std::path::{Path, PathBuf};

/// Whether a runtime can own running Compose containers on this host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuntimeParticipation {
    /// Proven unable to own running containers: no reachable local endpoint,
    /// no configured remote endpoint, and no runtime state. Its inventory can
    /// be skipped without weakening the completeness claim.
    Inactive,
    /// Running, reachable, or not provably inactive. Its inventory is
    /// authoritative, so a failed probe keeps the inventory incomplete.
    Participating,
}

/// One Docker endpoint that could be effective for this host. Resolution never
/// starts a runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
enum DockerEndpoint {
    /// A local Unix socket that can be probed directly.
    Unix(PathBuf),
    /// A remote or otherwise locally unprobeable endpoint that must not be
    /// silently omitted.
    Remote,
    /// No endpoint is configured; probe the platform's default local sockets.
    DefaultLocal,
    /// An endpoint is configured but could not be resolved.
    Unresolved,
}

/// Outcome of a single non-launching endpoint probe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SocketProbe {
    /// Something is listening on the endpoint.
    Reachable,
    /// Nothing exists at the endpoint.
    Absent,
    /// The endpoint exists but its state could not be established.
    Indeterminate,
}

/// Colima state left behind by a profile, independent of its CLI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ColimaStateEvidence {
    /// No Colima home or Lima instance state exists.
    None,
    /// Colima home or Lima instance state exists; a VM may still be running.
    Present,
    /// State could not be inspected.
    Unknown,
}

pub(super) fn docker_runtime_participation() -> RuntimeParticipation {
    classify_docker_participation(docker_endpoint_candidates(), probe_unix_socket)
}

pub(super) fn colima_runtime_participation() -> RuntimeParticipation {
    classify_colima_participation(
        crate::manager::command_exists("colima"),
        colima_state_evidence(crate::runtime::dns::colima_home_dir().as_deref()),
    )
}

fn classify_docker_participation(
    candidates: Vec<DockerEndpoint>,
    mut probe: impl FnMut(&Path) -> SocketProbe,
) -> RuntimeParticipation {
    if candidates.is_empty() {
        return RuntimeParticipation::Participating;
    }
    // Any remote or unresolvable configured endpoint keeps Docker
    // authoritative, even when another configured local endpoint is absent.
    if candidates.iter().any(|endpoint| {
        matches!(
            endpoint,
            DockerEndpoint::Remote | DockerEndpoint::Unresolved
        )
    }) {
        return RuntimeParticipation::Participating;
    }
    for endpoint in candidates {
        match endpoint {
            DockerEndpoint::Remote | DockerEndpoint::Unresolved => unreachable!("handled above"),
            DockerEndpoint::Unix(path) => {
                if classify_probe(probe(&path)) == RuntimeParticipation::Participating {
                    return RuntimeParticipation::Participating;
                }
            }
            DockerEndpoint::DefaultLocal => {
                for candidate in default_docker_unix_sockets() {
                    match probe(&candidate) {
                        SocketProbe::Reachable | SocketProbe::Indeterminate => {
                            return RuntimeParticipation::Participating;
                        }
                        SocketProbe::Absent => {}
                    }
                }
            }
        }
    }
    RuntimeParticipation::Inactive
}

fn classify_probe(probe: SocketProbe) -> RuntimeParticipation {
    match probe {
        SocketProbe::Reachable => RuntimeParticipation::Participating,
        // A stale or unreadable local endpoint is ambiguous, never proof of
        // inactivity.
        SocketProbe::Indeterminate => RuntimeParticipation::Participating,
        SocketProbe::Absent => RuntimeParticipation::Inactive,
    }
}

fn classify_colima_participation(
    cli_available: bool,
    state: ColimaStateEvidence,
) -> RuntimeParticipation {
    if cli_available {
        return RuntimeParticipation::Participating;
    }
    // Without the CLI, a Colima VM can still be running and owning containers.
    // Only the absence of any Colima/Lima state proves the runtime inactive.
    match state {
        ColimaStateEvidence::None => RuntimeParticipation::Inactive,
        ColimaStateEvidence::Present | ColimaStateEvidence::Unknown => {
            RuntimeParticipation::Participating
        }
    }
}

#[cfg(unix)]
fn probe_unix_socket(path: &Path) -> SocketProbe {
    match std::os::unix::net::UnixStream::connect(path) {
        Ok(_) => SocketProbe::Reachable,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => SocketProbe::Absent,
        Err(_) => SocketProbe::Indeterminate,
    }
}

#[cfg(not(unix))]
fn probe_unix_socket(_path: &Path) -> SocketProbe {
    SocketProbe::Indeterminate
}

fn default_docker_unix_sockets() -> Vec<PathBuf> {
    let mut candidates = vec![PathBuf::from("/var/run/docker.sock")];
    if let Some(home) = std::env::var_os("HOME") {
        candidates.push(PathBuf::from(home).join(".docker/run/docker.sock"));
    }
    if let Some(runtime_dir) = std::env::var_os("XDG_RUNTIME_DIR") {
        candidates.push(PathBuf::from(runtime_dir).join("docker.sock"));
    }
    candidates
}

fn docker_endpoint_candidates() -> Vec<DockerEndpoint> {
    docker_endpoint_candidates_from(
        non_empty_env("DOCKER_HOST"),
        non_empty_env("DOCKER_CONTEXT"),
        docker_config_dir(),
    )
}

/// Collect every Docker endpoint that could be effective for this host.
///
/// `DOCKER_CONTEXT` overrides `DOCKER_HOST` and the stored current context, and
/// a named context endpoint overrides `DOCKER_HOST`. Candidate collection keeps
/// all of them so a remote runtime selected by any of them is never omitted;
/// only a host where every candidate is an absent local endpoint is inactive.
fn docker_endpoint_candidates_from(
    docker_host: Option<String>,
    docker_context: Option<String>,
    config_dir: Option<PathBuf>,
) -> Vec<DockerEndpoint> {
    let mut candidates = Vec::new();
    if let Some(name) = docker_context
        .as_deref()
        .filter(|name| !is_default_context_name(name))
    {
        candidates.push(resolve_context_endpoint_from(name, config_dir.as_deref()));
    }
    if let Some(host) = docker_host.as_deref() {
        candidates.push(endpoint_from_host(host));
    }
    match config_dir.as_deref().map(config_context) {
        Some(DockerConfigContext::Named(Some(name))) if !is_default_context_name(&name) => {
            candidates.push(resolve_context_endpoint_from(&name, config_dir.as_deref()));
        }
        Some(DockerConfigContext::Unreadable) => candidates.push(DockerEndpoint::Unresolved),
        _ => {}
    }
    candidates.push(DockerEndpoint::DefaultLocal);
    candidates
}

fn is_default_context_name(name: &str) -> bool {
    name.is_empty() || name.eq_ignore_ascii_case("default")
}

fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn docker_config_dir() -> Option<PathBuf> {
    if let Some(dir) = non_empty_env("DOCKER_CONFIG") {
        return Some(PathBuf::from(dir));
    }
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".docker"))
}

enum DockerConfigContext {
    /// No `config.json`, so Docker's default context is in effect.
    Absent,
    /// `config.json` parsed; the value is the configured context, if any.
    Named(Option<String>),
    /// `config.json` exists but could not be read or parsed.
    Unreadable,
}

fn config_context(config_dir: &Path) -> DockerConfigContext {
    let path = config_dir.join("config.json");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return if path.exists() {
            DockerConfigContext::Unreadable
        } else {
            DockerConfigContext::Absent
        };
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return DockerConfigContext::Unreadable;
    };
    DockerConfigContext::Named(
        value
            .get("currentContext")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned),
    )
}

/// Resolve a named Docker context to its effective endpoint. A configured
/// context that cannot be resolved stays participating, so an unreachable
/// remote dependency is never silently omitted.
fn resolve_context_endpoint_from(name: &str, config_dir: Option<&Path>) -> DockerEndpoint {
    let Some(contexts_dir) = config_dir.map(|dir| dir.join("contexts").join("meta")) else {
        return DockerEndpoint::Unresolved;
    };
    let Ok(entries) = std::fs::read_dir(&contexts_dir) else {
        return DockerEndpoint::Unresolved;
    };
    for entry in entries.flatten() {
        let path = entry.path().join("meta.json");
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        if value.get("Name").and_then(serde_json::Value::as_str) != Some(name) {
            continue;
        }
        let Some(host) = value
            .get("Endpoints")
            .and_then(|endpoints| endpoints.get("docker"))
            .and_then(|docker| docker.get("Host"))
            .and_then(serde_json::Value::as_str)
        else {
            return DockerEndpoint::Unresolved;
        };
        return endpoint_from_host(host);
    }
    DockerEndpoint::Unresolved
}

fn endpoint_from_host(host: &str) -> DockerEndpoint {
    let host = host.trim();
    if host.is_empty() {
        return DockerEndpoint::Unresolved;
    }
    if let Some(path) = host.strip_prefix("unix://") {
        return if path.is_empty() {
            DockerEndpoint::Unresolved
        } else {
            DockerEndpoint::Unix(PathBuf::from(path))
        };
    }
    if host.starts_with("npipe://")
        || host.starts_with("tcp://")
        || host.starts_with("http://")
        || host.starts_with("https://")
        || host.starts_with("ssh://")
    {
        return DockerEndpoint::Remote;
    }
    if host.starts_with('/') {
        return DockerEndpoint::Unix(PathBuf::from(host));
    }
    DockerEndpoint::Unresolved
}

/// Inspect Colima state that can outlive the CLI. A running Lima instance
/// lives under the Colima home's `_lima` directory, so its presence keeps
/// Colima authoritative even when `colima` is not on `PATH`.
fn colima_state_evidence(colima_home: Option<&Path>) -> ColimaStateEvidence {
    let Some(home) = colima_home else {
        return ColimaStateEvidence::None;
    };
    match std::fs::read_dir(home.join("_lima")) {
        Ok(entries) => {
            if entries
                .filter_map(Result::ok)
                .any(|entry| entry.path().is_dir())
            {
                ColimaStateEvidence::Present
            } else {
                ColimaStateEvidence::None
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => ColimaStateEvidence::None,
        Err(_) => ColimaStateEvidence::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn absent_explicit_unix_endpoint_is_inactive() {
        let probes = Cell::new(0);
        let participation = classify_docker_participation(
            vec![DockerEndpoint::Unix(PathBuf::from(
                "/tmp/effigy-absent.sock",
            ))],
            |_| {
                probes.set(probes.get() + 1);
                SocketProbe::Absent
            },
        );

        assert_eq!(participation, RuntimeParticipation::Inactive);
        assert_eq!(probes.get(), 1);
    }

    #[test]
    fn reachable_unix_endpoint_is_participating() {
        let participation = classify_docker_participation(
            vec![DockerEndpoint::Unix(PathBuf::from("/tmp/effigy-live.sock"))],
            |_| SocketProbe::Reachable,
        );

        assert_eq!(participation, RuntimeParticipation::Participating);
    }

    #[test]
    fn stale_or_unreadable_unix_endpoint_stays_fail_closed() {
        let participation = classify_docker_participation(
            vec![DockerEndpoint::Unix(PathBuf::from(
                "/tmp/effigy-stale.sock",
            ))],
            |_| SocketProbe::Indeterminate,
        );

        assert_eq!(participation, RuntimeParticipation::Participating);
    }

    #[test]
    fn remote_and_unresolved_endpoints_are_never_omitted() {
        for remote in [DockerEndpoint::Remote, DockerEndpoint::Unresolved] {
            let participation = classify_docker_participation(
                vec![
                    DockerEndpoint::Unix(PathBuf::from("/tmp/effigy-absent.sock")),
                    remote,
                ],
                |_| panic!("a configured remote or unresolved endpoint must not be probed away"),
            );
            assert_eq!(participation, RuntimeParticipation::Participating);
        }
    }

    #[test]
    fn empty_candidate_set_stays_fail_closed() {
        let participation =
            classify_docker_participation(Vec::new(), |_| panic!("no endpoints to probe"));

        assert_eq!(participation, RuntimeParticipation::Participating);
    }

    #[test]
    fn default_local_endpoint_is_inactive_only_after_every_candidate_is_absent() {
        let probes = Cell::new(0);
        let participation =
            classify_docker_participation(vec![DockerEndpoint::DefaultLocal], |_| {
                probes.set(probes.get() + 1);
                SocketProbe::Absent
            });

        assert_eq!(participation, RuntimeParticipation::Inactive);
        assert_eq!(probes.get(), default_docker_unix_sockets().len());
    }

    #[test]
    fn default_local_endpoint_with_one_reachable_candidate_is_participating() {
        let participation =
            classify_docker_participation(vec![DockerEndpoint::DefaultLocal], |path| {
                if path == PathBuf::from("/var/run/docker.sock") {
                    SocketProbe::Reachable
                } else {
                    SocketProbe::Absent
                }
            });

        assert_eq!(participation, RuntimeParticipation::Participating);
    }

    #[test]
    fn default_local_endpoint_with_one_stale_candidate_is_participating() {
        let participation =
            classify_docker_participation(vec![DockerEndpoint::DefaultLocal], |path| {
                if path == PathBuf::from("/var/run/docker.sock") {
                    SocketProbe::Indeterminate
                } else {
                    SocketProbe::Absent
                }
            });

        assert_eq!(participation, RuntimeParticipation::Participating);
    }

    #[test]
    fn colima_participates_with_or_without_leftover_state_when_its_cli_exists() {
        for state in [
            ColimaStateEvidence::None,
            ColimaStateEvidence::Present,
            ColimaStateEvidence::Unknown,
        ] {
            assert_eq!(
                classify_colima_participation(true, state),
                RuntimeParticipation::Participating
            );
        }
    }

    #[test]
    fn colima_without_a_cli_stays_fail_closed_when_state_remains() {
        // A running Colima VM can outlive its client CLI, so a missing binary
        // is not proof of inactivity while Colima or Lima state exists.
        assert_eq!(
            classify_colima_participation(false, ColimaStateEvidence::Present),
            RuntimeParticipation::Participating
        );
        assert_eq!(
            classify_colima_participation(false, ColimaStateEvidence::Unknown),
            RuntimeParticipation::Participating
        );
    }

    #[test]
    fn colima_without_a_cli_or_any_state_is_inactive() {
        assert_eq!(
            classify_colima_participation(false, ColimaStateEvidence::None),
            RuntimeParticipation::Inactive
        );
    }

    #[test]
    fn endpoint_hosts_are_classified_by_transport() {
        assert_eq!(
            endpoint_from_host("unix:///Users/me/.docker/run/docker.sock"),
            DockerEndpoint::Unix(PathBuf::from("/Users/me/.docker/run/docker.sock"))
        );
        assert_eq!(
            endpoint_from_host("/tmp/docker.sock"),
            DockerEndpoint::Unix(PathBuf::from("/tmp/docker.sock"))
        );
        assert_eq!(
            endpoint_from_host("tcp://10.0.0.5:2375"),
            DockerEndpoint::Remote
        );
        assert_eq!(endpoint_from_host("ssh://builder"), DockerEndpoint::Remote);
        assert_eq!(
            endpoint_from_host("npipe:////./pipe/docker"),
            DockerEndpoint::Remote
        );
        assert_eq!(endpoint_from_host("  "), DockerEndpoint::Unresolved);
        assert_eq!(
            endpoint_from_host("relative.sock"),
            DockerEndpoint::Unresolved
        );
    }

    fn docker_context_fixture(contexts: &[(&str, Option<&str>)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("create docker config fixture");
        let meta_root = dir.path().join("contexts").join("meta");
        for (index, (name, host)) in contexts.iter().enumerate() {
            let entry = meta_root.join(format!("context-{index}"));
            std::fs::create_dir_all(&entry).expect("create context meta dir");
            let endpoints = match host {
                Some(host) => format!(
                    "{{\"docker\":{{\"Host\":{}}}}}",
                    serde_json::Value::String((*host).to_owned())
                ),
                None => "{}".to_owned(),
            };
            std::fs::write(
                entry.join("meta.json"),
                format!(
                    "{{\"Name\":{},\"Metadata\":{{}},\"Endpoints\":{endpoints}}}",
                    serde_json::Value::String((*name).to_owned())
                ),
            )
            .expect("write context meta");
        }
        dir
    }

    fn write_current_context(dir: &tempfile::TempDir, context: &str) {
        std::fs::write(
            dir.path().join("config.json"),
            format!(
                "{{\"currentContext\":{}}}",
                serde_json::Value::String(context.to_owned())
            ),
        )
        .expect("write docker config");
    }

    #[test]
    fn docker_context_overrides_an_absent_docker_host() {
        // The reviewed defect: a remote context selected by DOCKER_CONTEXT
        // must not be omitted because DOCKER_HOST points at an absent socket.
        let dir = docker_context_fixture(&[("remote", Some("tcp://10.0.0.5:2375"))]);
        let candidates = docker_endpoint_candidates_from(
            Some("unix:///tmp/absent.sock".to_owned()),
            Some("remote".to_owned()),
            Some(dir.path().to_path_buf()),
        );

        assert!(candidates.contains(&DockerEndpoint::Remote));
        assert!(candidates.contains(&DockerEndpoint::Unix(PathBuf::from("/tmp/absent.sock"))));
        assert_eq!(
            classify_docker_participation(candidates, |_| {
                panic!("a configured remote endpoint must not be probed away")
            }),
            RuntimeParticipation::Participating
        );
    }

    #[test]
    fn a_remote_docker_host_is_not_omitted_by_an_absent_context() {
        let dir = docker_context_fixture(&[(
            "desktop-linux",
            Some("unix:///Users/me/.docker/run/docker.sock"),
        )]);
        write_current_context(&dir, "desktop-linux");
        let candidates = docker_endpoint_candidates_from(
            Some("ssh://builder".to_owned()),
            None,
            Some(dir.path().to_path_buf()),
        );

        assert!(candidates.contains(&DockerEndpoint::Remote));
        assert_eq!(
            classify_docker_participation(candidates, |_| {
                panic!("a configured remote endpoint must not be probed away")
            }),
            RuntimeParticipation::Participating
        );
    }

    #[test]
    fn docker_context_and_host_candidates_are_both_kept() {
        let dir = docker_context_fixture(&[(
            "desktop-linux",
            Some("unix:///Users/me/.docker/run/docker.sock"),
        )]);
        let candidates = docker_endpoint_candidates_from(
            Some("unix:///tmp/other.sock".to_owned()),
            Some("desktop-linux".to_owned()),
            Some(dir.path().to_path_buf()),
        );

        assert!(candidates.contains(&DockerEndpoint::Unix(PathBuf::from(
            "/Users/me/.docker/run/docker.sock"
        ))));
        assert!(candidates.contains(&DockerEndpoint::Unix(PathBuf::from("/tmp/other.sock"))));
    }

    #[test]
    fn stored_current_context_is_a_candidate_without_env_overrides() {
        let dir = docker_context_fixture(&[(
            "desktop-linux",
            Some("unix:///Users/me/.docker/run/docker.sock"),
        )]);
        write_current_context(&dir, "desktop-linux");

        assert_eq!(
            docker_endpoint_candidates_from(None, None, Some(dir.path().to_path_buf())),
            vec![
                DockerEndpoint::Unix(PathBuf::from("/Users/me/.docker/run/docker.sock")),
                DockerEndpoint::DefaultLocal,
            ]
        );
    }

    #[test]
    fn configured_but_unresolvable_context_stays_fail_closed() {
        let dir = docker_context_fixture(&[]);
        write_current_context(&dir, "missing-context");
        let candidates =
            docker_endpoint_candidates_from(None, None, Some(dir.path().to_path_buf()));

        assert!(candidates.contains(&DockerEndpoint::Unresolved));
        assert_eq!(
            classify_docker_participation(candidates, |_| {
                panic!("an unresolvable context must not be probed away")
            }),
            RuntimeParticipation::Participating
        );
    }

    #[test]
    fn unreadable_config_is_fail_closed() {
        let dir = docker_context_fixture(&[]);
        std::fs::write(dir.path().join("config.json"), "{ not json").expect("write bad config");
        let candidates =
            docker_endpoint_candidates_from(None, None, Some(dir.path().to_path_buf()));

        assert!(candidates.contains(&DockerEndpoint::Unresolved));
        assert_eq!(
            classify_docker_participation(candidates, |_| {
                panic!("an unreadable config must not be probed away")
            }),
            RuntimeParticipation::Participating
        );
    }

    #[test]
    fn context_without_a_docker_endpoint_is_fail_closed() {
        let dir = docker_context_fixture(&[("bare", None)]);
        write_current_context(&dir, "bare");
        let candidates =
            docker_endpoint_candidates_from(None, None, Some(dir.path().to_path_buf()));

        assert!(candidates.contains(&DockerEndpoint::Unresolved));
        assert_eq!(
            classify_docker_participation(candidates, |_| {
                panic!("a context without an endpoint must not be probed away")
            }),
            RuntimeParticipation::Participating
        );
    }

    #[test]
    fn absent_config_falls_back_to_the_platform_default() {
        let dir = tempfile::tempdir().expect("create empty docker config fixture");

        assert_eq!(
            docker_endpoint_candidates_from(None, None, Some(dir.path().to_path_buf())),
            vec![DockerEndpoint::DefaultLocal]
        );
        assert_eq!(
            docker_endpoint_candidates_from(None, None, None),
            vec![DockerEndpoint::DefaultLocal]
        );
    }

    #[test]
    fn default_context_name_uses_the_platform_default() {
        let dir = docker_context_fixture(&[("desktop-linux", Some("unix:///tmp/live.sock"))]);
        write_current_context(&dir, "default");

        assert_eq!(
            docker_endpoint_candidates_from(None, None, Some(dir.path().to_path_buf())),
            vec![DockerEndpoint::DefaultLocal]
        );
    }

    #[test]
    fn colima_state_evidence_tracks_lima_instance_directories() {
        let dir = tempfile::tempdir().expect("create colima home fixture");

        assert_eq!(
            colima_state_evidence(Some(dir.path())),
            ColimaStateEvidence::None
        );

        let instance = dir.path().join("_lima").join("colima-effigy");
        std::fs::create_dir_all(&instance).expect("create lima instance dir");
        assert_eq!(
            colima_state_evidence(Some(dir.path())),
            ColimaStateEvidence::Present
        );

        assert_eq!(colima_state_evidence(None), ColimaStateEvidence::None);
    }
}
