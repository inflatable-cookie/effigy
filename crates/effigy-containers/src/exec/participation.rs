//! Bounded runtime participation detection used before container inventory.
//!
//! Inventory completeness is what licenses stale loopback reclamation, so a
//! runtime may be skipped only when it is *proven* unable to own running
//! containers. A runtime that is running, reachable, or whose state cannot be
//! established keeps its inventory authoritative: a later probe failure stays
//! actionable and prevents uncertain reclamation.
//!
//! Detection never launches Docker, Colima, or a container. Docker
//! participation is resolved from its effective endpoint — `DOCKER_HOST`,
//! `DOCKER_CONTEXT`, the Docker context store, then the platform default
//! socket — and verified with a non-launching Unix-socket probe. Colima
//! participation follows from whether its CLI exists, because a Colima profile
//! has no remote endpoint that could outlive its client.
//!
//! Preferred backend, CLI presence, a missing socket, unreadable state and
//! stderr text are never sufficient on their own to conclude that a runtime
//! owns nothing. Only an unresolvable-or-absent *effective* local endpoint
//! proves a runtime inactive.

use std::path::{Path, PathBuf};

/// Whether a runtime can own running Compose containers on this host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuntimeParticipation {
    /// Proven unable to own running containers: no reachable local endpoint
    /// and no configured remote endpoint. Its inventory can be skipped without
    /// weakening the completeness claim.
    Inactive,
    /// Running, reachable, or not provably inactive. Its inventory is
    /// authoritative, so a failed probe keeps the inventory incomplete.
    Participating,
}

/// The effective Docker endpoint after environment, context, and platform
/// defaults have been resolved. Resolution never starts a runtime.
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

pub(super) fn docker_runtime_participation() -> RuntimeParticipation {
    classify_docker_participation(resolve_docker_endpoint(), probe_unix_socket)
}

pub(super) fn colima_runtime_participation() -> RuntimeParticipation {
    classify_colima_participation(crate::manager::command_exists("colima"))
}

fn classify_colima_participation(cli_available: bool) -> RuntimeParticipation {
    if cli_available {
        RuntimeParticipation::Participating
    } else {
        RuntimeParticipation::Inactive
    }
}

fn classify_docker_participation(
    endpoint: DockerEndpoint,
    mut probe: impl FnMut(&Path) -> SocketProbe,
) -> RuntimeParticipation {
    match endpoint {
        DockerEndpoint::Remote | DockerEndpoint::Unresolved => RuntimeParticipation::Participating,
        DockerEndpoint::Unix(path) => classify_probe(probe(&path)),
        DockerEndpoint::DefaultLocal => {
            let mut probed = false;
            for candidate in default_docker_unix_sockets() {
                probed = true;
                match probe(&candidate) {
                    SocketProbe::Reachable | SocketProbe::Indeterminate => {
                        return RuntimeParticipation::Participating;
                    }
                    SocketProbe::Absent => {}
                }
            }
            if probed {
                RuntimeParticipation::Inactive
            } else {
                RuntimeParticipation::Participating
            }
        }
    }
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

fn resolve_docker_endpoint() -> DockerEndpoint {
    resolve_docker_endpoint_from(
        non_empty_env("DOCKER_HOST"),
        non_empty_env("DOCKER_CONTEXT"),
        docker_config_dir(),
    )
}

fn resolve_docker_endpoint_from(
    docker_host: Option<String>,
    docker_context: Option<String>,
    config_dir: Option<PathBuf>,
) -> DockerEndpoint {
    if let Some(host) = docker_host {
        return endpoint_from_host(&host);
    }
    let context = match docker_context {
        Some(name) => Some(name),
        None => match config_dir.as_deref().map(config_context) {
            Some(DockerConfigContext::Named(name)) => name,
            Some(DockerConfigContext::Absent) => None,
            // A config that exists but cannot be read may name a remote
            // endpoint we cannot remark on, so it is never treated as default.
            Some(DockerConfigContext::Unreadable) => return DockerEndpoint::Unresolved,
            None => None,
        },
    };
    let context = context.filter(|name| !name.eq_ignore_ascii_case("default"));
    match context {
        Some(name) => resolve_context_endpoint_from(&name, config_dir.as_deref()),
        None => DockerEndpoint::DefaultLocal,
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn colima_participates_exactly_when_its_cli_exists() {
        assert_eq!(
            classify_colima_participation(true),
            RuntimeParticipation::Participating
        );
        assert_eq!(
            classify_colima_participation(false),
            RuntimeParticipation::Inactive
        );
    }

    #[test]
    fn absent_explicit_unix_endpoint_is_inactive() {
        let probes = Cell::new(0);
        let participation = classify_docker_participation(
            DockerEndpoint::Unix(PathBuf::from("/tmp/effigy-absent.sock")),
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
            DockerEndpoint::Unix(PathBuf::from("/tmp/effigy-live.sock")),
            |_| SocketProbe::Reachable,
        );

        assert_eq!(participation, RuntimeParticipation::Participating);
    }

    #[test]
    fn stale_or_unreadable_unix_endpoint_stays_fail_closed() {
        for probe in [SocketProbe::Indeterminate] {
            let participation = classify_docker_participation(
                DockerEndpoint::Unix(PathBuf::from("/tmp/effigy-stale.sock")),
                |_| probe,
            );
            assert_eq!(participation, RuntimeParticipation::Participating);
        }
    }

    #[test]
    fn remote_and_unresolved_endpoints_are_never_omitted() {
        for endpoint in [DockerEndpoint::Remote, DockerEndpoint::Unresolved] {
            let participation = classify_docker_participation(endpoint, |_| {
                panic!("remote or unresolved endpoints must not be probed")
            });
            assert_eq!(participation, RuntimeParticipation::Participating);
        }
    }

    #[test]
    fn default_local_endpoint_is_inactive_only_after_every_candidate_is_absent() {
        let probes = Cell::new(0);
        let participation = classify_docker_participation(DockerEndpoint::DefaultLocal, |_| {
            probes.set(probes.get() + 1);
            SocketProbe::Absent
        });

        assert_eq!(participation, RuntimeParticipation::Inactive);
        assert_eq!(probes.get(), default_docker_unix_sockets().len());
    }

    #[test]
    fn default_local_endpoint_with_one_reachable_candidate_is_participating() {
        let participation = classify_docker_participation(DockerEndpoint::DefaultLocal, |path| {
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
        let participation = classify_docker_participation(DockerEndpoint::DefaultLocal, |path| {
            if path == PathBuf::from("/var/run/docker.sock") {
                SocketProbe::Indeterminate
            } else {
                SocketProbe::Absent
            }
        });

        assert_eq!(participation, RuntimeParticipation::Participating);
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
    fn explicit_docker_host_wins_over_the_context_store() {
        let dir = docker_context_fixture(&[("remote", Some("tcp://10.0.0.5:2375"))]);
        write_current_context(&dir, "remote");

        let endpoint = resolve_docker_endpoint_from(
            Some("unix:///tmp/explicit.sock".to_owned()),
            None,
            Some(dir.path().to_path_buf()),
        );

        assert_eq!(
            endpoint,
            DockerEndpoint::Unix(PathBuf::from("/tmp/explicit.sock"))
        );
    }

    #[test]
    fn docker_context_override_resolves_through_the_store() {
        let dir = docker_context_fixture(&[
            (
                "desktop-linux",
                Some("unix:///Users/me/.docker/run/docker.sock"),
            ),
            ("remote", Some("ssh://builder")),
        ]);

        assert_eq!(
            resolve_docker_endpoint_from(
                None,
                Some("desktop-linux".to_owned()),
                Some(dir.path().to_path_buf())
            ),
            DockerEndpoint::Unix(PathBuf::from("/Users/me/.docker/run/docker.sock"))
        );
        assert_eq!(
            resolve_docker_endpoint_from(
                None,
                Some("remote".to_owned()),
                Some(dir.path().to_path_buf())
            ),
            DockerEndpoint::Remote
        );
    }

    #[test]
    fn configured_but_unresolvable_context_stays_fail_closed() {
        let dir = docker_context_fixture(&[]);
        write_current_context(&dir, "missing-context");

        assert_eq!(
            resolve_docker_endpoint_from(None, None, Some(dir.path().to_path_buf())),
            DockerEndpoint::Unresolved
        );
    }

    #[test]
    fn unreadable_config_is_fail_closed() {
        let dir = docker_context_fixture(&[]);
        std::fs::write(dir.path().join("config.json"), "{ not json").expect("write bad config");

        assert_eq!(
            resolve_docker_endpoint_from(None, None, Some(dir.path().to_path_buf())),
            DockerEndpoint::Unresolved
        );
    }

    #[test]
    fn context_without_a_docker_endpoint_is_fail_closed() {
        let dir = docker_context_fixture(&[("bare", None)]);
        write_current_context(&dir, "bare");

        assert_eq!(
            resolve_docker_endpoint_from(None, None, Some(dir.path().to_path_buf())),
            DockerEndpoint::Unresolved
        );
    }

    #[test]
    fn absent_config_falls_back_to_the_platform_default() {
        let dir = tempfile::tempdir().expect("create empty docker config fixture");

        assert_eq!(
            resolve_docker_endpoint_from(None, None, Some(dir.path().to_path_buf())),
            DockerEndpoint::DefaultLocal
        );
        assert_eq!(
            resolve_docker_endpoint_from(None, None, None),
            DockerEndpoint::DefaultLocal
        );
    }

    #[test]
    fn default_context_name_uses_the_platform_default() {
        let dir = docker_context_fixture(&[("desktop-linux", Some("unix:///tmp/live.sock"))]);
        write_current_context(&dir, "default");

        assert_eq!(
            resolve_docker_endpoint_from(None, None, Some(dir.path().to_path_buf())),
            DockerEndpoint::DefaultLocal
        );
    }
}
