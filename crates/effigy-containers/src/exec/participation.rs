//! Bounded runtime participation detection used before container inventory.
//!
//! A runtime may be skipped only when it is *proven* unable to own running
//! containers, so a listing never silently drops an environment that is up. A
//! runtime that is running, reachable, or whose state cannot be established
//! keeps its inventory authoritative: a later probe failure stays actionable.
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

/// Deterministic environment classification for tests. Local endpoints are
/// treated as absent so the verdict depends only on the configured endpoints
/// and never probes a live daemon.
#[cfg(test)]
pub(super) fn environment_participation_for_test() -> RuntimeParticipation {
    classify_docker_participation(docker_endpoint_candidates(), |_| SocketProbe::Absent)
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
        docker_env_override("DOCKER_HOST"),
        docker_env_override("DOCKER_CONTEXT"),
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
    docker_host: DockerOverride,
    docker_context: DockerOverride,
    config_dir: Option<PathBuf>,
) -> Vec<DockerEndpoint> {
    let mut candidates = Vec::new();
    match &docker_context {
        DockerOverride::Unset => {}
        // A set-but-unreadable override is an explicit unknown endpoint, never
        // an absent one: it could name any context, including a remote one.
        DockerOverride::NonUnicode => candidates.push(DockerEndpoint::Unresolved),
        DockerOverride::Text(name) if !is_default_context_name(name) => {
            candidates.push(resolve_context_endpoint_from(name, config_dir.as_deref()));
        }
        DockerOverride::Text(_) => {}
    }
    match &docker_host {
        DockerOverride::Unset => {}
        DockerOverride::NonUnicode => candidates.push(DockerEndpoint::Unresolved),
        DockerOverride::Text(host) => candidates.push(endpoint_from_host(host)),
    }
    match config_dir.as_deref() {
        Some(dir) => match config_context(dir) {
            DockerConfigContext::Named(Some(name)) if !is_default_context_name(&name) => {
                candidates.push(resolve_context_endpoint_from(&name, Some(dir)));
            }
            DockerConfigContext::Unreadable => candidates.push(DockerEndpoint::Unresolved),
            _ => {}
        },
        // Without a config directory we cannot tell whether a stored context
        // selects a remote endpoint, so Docker stays fail-closed.
        None => candidates.push(DockerEndpoint::Unresolved),
    }
    candidates.push(DockerEndpoint::DefaultLocal);
    candidates
}

fn is_default_context_name(name: &str) -> bool {
    // Docker context names are case-sensitive, so `DEFAULT` is a distinct,
    // selectable context that must not be collapsed into the default context.
    // Collapsing it could omit a remote endpoint chosen by `DOCKER_CONTEXT` or
    // the stored current context.
    name.is_empty() || name == "default"
}

/// A `DOCKER_HOST`/`DOCKER_CONTEXT` override as it can actually be set.
#[derive(Debug, Clone, PartialEq, Eq)]
enum DockerOverride {
    /// The variable is unset, or set to an empty value.
    Unset,
    /// The variable holds interpretable text.
    Text(String),
    /// The variable is set but not valid Unicode. The effective endpoint is
    /// unknown rather than absent, because Docker on Unix sees raw bytes.
    NonUnicode,
}

fn docker_env_override(name: &str) -> DockerOverride {
    match std::env::var_os(name) {
        None => DockerOverride::Unset,
        Some(value) if value.is_empty() => DockerOverride::Unset,
        Some(value) => match value.into_string() {
            Ok(text) => DockerOverride::Text(text),
            Err(_) => DockerOverride::NonUnicode,
        },
    }
}

fn docker_config_dir() -> Option<PathBuf> {
    // Keep the configured path byte-for-byte: Docker reads DOCKER_CONFIG
    // literally, and a normalized path could select a different, or no,
    // config directory and hide a stored remote context.
    if let Some(dir) = std::env::var_os("DOCKER_CONFIG").filter(|value| !value.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".docker"))
}

/// The CLI's current context name from its config, as the CLI reads it, when
/// no environment variable names one: `default` without a config or without a
/// `currentContext`. `None` when the config cannot be read or is invalid, so a
/// caller never mistakes a hidden remote context for the default (fail closed).
pub(crate) fn docker_config_current_context() -> Option<String> {
    current_context_in(docker_config_dir().as_deref())
}

fn current_context_in(config_dir: Option<&Path>) -> Option<String> {
    let Some(dir) = config_dir else {
        return Some("default".to_owned());
    };
    match config_context(dir) {
        DockerConfigContext::Absent | DockerConfigContext::Named(None) => {
            Some("default".to_owned())
        }
        DockerConfigContext::Named(Some(name)) if name.is_empty() => Some("default".to_owned()),
        DockerConfigContext::Named(Some(name)) => Some(name),
        DockerConfigContext::Unreadable => None,
    }
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
    match std::fs::read_to_string(&path) {
        Ok(text) => match serde_json::from_str::<serde_json::Value>(&text) {
            Ok(serde_json::Value::Object(map)) => match map.get("currentContext") {
                None => DockerConfigContext::Named(None),
                Some(serde_json::Value::String(name)) => {
                    DockerConfigContext::Named(Some(name.clone()))
                }
                // A present but schema-invalid currentContext could hide a
                // selected remote context, so it is never read as absent.
                Some(_) => DockerConfigContext::Unreadable,
            },
            // A non-object root is not a valid Docker config.
            Ok(_) => DockerConfigContext::Unreadable,
            Err(_) => DockerConfigContext::Unreadable,
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            // A missing file only means a missing context when the config
            // directory itself is reachable. An inaccessible directory can
            // still hold a stored remote context, so it stays fail-closed.
            match std::fs::metadata(config_dir) {
                Ok(_) => DockerConfigContext::Absent,
                Err(metadata_error) if metadata_error.kind() == std::io::ErrorKind::NotFound => {
                    DockerConfigContext::Absent
                }
                Err(_) => DockerConfigContext::Unreadable,
            }
        }
        Err(_) => DockerConfigContext::Unreadable,
    }
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
/// Colima authoritative even when `colima` is not on `PATH`. State that cannot
/// be inspected is never proof of inactivity.
fn colima_state_evidence(colima_home: Option<&Path>) -> ColimaStateEvidence {
    let Some(home) = colima_home else {
        // Without a Colima home we cannot tell whether a VM is running.
        return ColimaStateEvidence::Unknown;
    };
    let instances = home.join("_lima");
    let entries = match std::fs::read_dir(&instances) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            // A dangling symlink at `_lima` still records Colima state: it was
            // created for an instance and must not be read as absent.
            return match std::fs::symlink_metadata(&instances) {
                Ok(_) => ColimaStateEvidence::Present,
                Err(metadata_error) if metadata_error.kind() == std::io::ErrorKind::NotFound => {
                    ColimaStateEvidence::None
                }
                Err(_) => ColimaStateEvidence::Unknown,
            };
        }
        // An inaccessible or malformed `_lima` path could still hold a running
        // instance, so it stays unknowable rather than absent.
        Err(_) => return ColimaStateEvidence::Unknown,
    };
    let mut present = false;
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => return ColimaStateEvidence::Unknown,
        };
        match entry.file_type() {
            // A symlinked or dangling entry can still be a Lima instance
            // directory, so it counts as present rather than absent.
            Ok(file_type) if file_type.is_dir() || file_type.is_symlink() => present = true,
            Ok(_) => {}
            Err(_) => return ColimaStateEvidence::Unknown,
        }
    }
    if present {
        ColimaStateEvidence::Present
    } else {
        ColimaStateEvidence::None
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

    /// Every probe outcome, candidate set and empty set, with the verdict it
    /// must produce and how often the runtime was probed.
    #[test]
    fn classify_docker_participation_cases() {
        type Row = (
            &'static str,
            Vec<DockerEndpoint>,
            fn(&Path) -> SocketProbe,
            RuntimeParticipation,
            Option<usize>,
        );
        let cases: [Row; 8] = [
            (
                "an absent explicit endpoint",
                vec![DockerEndpoint::Unix(PathBuf::from(
                    "/tmp/effigy-absent.sock",
                ))],
                |_| SocketProbe::Absent,
                RuntimeParticipation::Inactive,
                Some(1),
            ),
            (
                "a reachable endpoint",
                vec![DockerEndpoint::Unix(PathBuf::from("/tmp/effigy-live.sock"))],
                |_| SocketProbe::Reachable,
                RuntimeParticipation::Participating,
                None,
            ),
            (
                "a stale or unreadable endpoint",
                vec![DockerEndpoint::Unix(PathBuf::from(
                    "/tmp/effigy-stale.sock",
                ))],
                |_| SocketProbe::Indeterminate,
                RuntimeParticipation::Participating,
                None,
            ),
            (
                "a configured remote endpoint beside an absent local one",
                vec![
                    DockerEndpoint::Unix(PathBuf::from("/tmp/effigy-absent.sock")),
                    DockerEndpoint::Remote,
                ],
                |_| panic!("a configured remote endpoint must not be probed away"),
                RuntimeParticipation::Participating,
                Some(0),
            ),
            (
                "a configured endpoint this build cannot resolve",
                vec![
                    DockerEndpoint::Unix(PathBuf::from("/tmp/effigy-absent.sock")),
                    DockerEndpoint::Unresolved,
                ],
                |_| panic!("an unresolved endpoint must not be probed away"),
                RuntimeParticipation::Participating,
                Some(0),
            ),
            (
                "no candidate at all",
                Vec::new(),
                |_| panic!("no endpoints to probe"),
                RuntimeParticipation::Participating,
                Some(0),
            ),
            (
                "every platform-default socket absent",
                vec![DockerEndpoint::DefaultLocal],
                |_| SocketProbe::Absent,
                RuntimeParticipation::Inactive,
                Some(default_docker_unix_sockets().len()),
            ),
            (
                "one reachable platform-default socket",
                vec![DockerEndpoint::DefaultLocal],
                |path| {
                    if path == Path::new("/var/run/docker.sock") {
                        SocketProbe::Reachable
                    } else {
                        SocketProbe::Absent
                    }
                },
                RuntimeParticipation::Participating,
                None,
            ),
        ];
        for (name, candidates, probe, expected, probes) in cases {
            let observed = Cell::new(0);
            let participation = classify_docker_participation(candidates, |path| {
                observed.set(observed.get() + 1);
                probe(path)
            });
            assert_eq!(participation, expected, "{name}");
            if let Some(probes) = probes {
                assert_eq!(observed.get(), probes, "{name}");
            }
        }

        // A stale platform-default socket is ambiguous, never absent.
        assert_eq!(
            classify_docker_participation(vec![DockerEndpoint::DefaultLocal], |path| {
                if path == Path::new("/var/run/docker.sock") {
                    SocketProbe::Indeterminate
                } else {
                    SocketProbe::Absent
                }
            }),
            RuntimeParticipation::Participating
        );
    }

    #[test]
    fn colima_participation_follows_the_cli_and_leftover_state() {
        let cases: [(bool, ColimaStateEvidence, RuntimeParticipation); 6] = [
            (
                true,
                ColimaStateEvidence::None,
                RuntimeParticipation::Participating,
            ),
            (
                true,
                ColimaStateEvidence::Present,
                RuntimeParticipation::Participating,
            ),
            (
                true,
                ColimaStateEvidence::Unknown,
                RuntimeParticipation::Participating,
            ),
            // A running Colima VM can outlive its client CLI, so a missing
            // binary is not proof of inactivity while state remains.
            (
                false,
                ColimaStateEvidence::Present,
                RuntimeParticipation::Participating,
            ),
            (
                false,
                ColimaStateEvidence::Unknown,
                RuntimeParticipation::Participating,
            ),
            (
                false,
                ColimaStateEvidence::None,
                RuntimeParticipation::Inactive,
            ),
        ];
        for (cli_available, state, expected) in cases {
            assert_eq!(
                classify_colima_participation(cli_available, state),
                expected,
                "cli {cli_available} state {state:?}"
            );
        }
    }

    #[test]
    fn endpoint_hosts_are_classified_by_transport() {
        let cases = [
            (
                "unix:///Users/me/.docker/run/docker.sock",
                DockerEndpoint::Unix(PathBuf::from("/Users/me/.docker/run/docker.sock")),
            ),
            (
                "/tmp/docker.sock",
                DockerEndpoint::Unix(PathBuf::from("/tmp/docker.sock")),
            ),
            ("tcp://10.0.0.5:2375", DockerEndpoint::Remote),
            ("ssh://builder", DockerEndpoint::Remote),
            ("npipe:////./pipe/docker", DockerEndpoint::Remote),
            ("  ", DockerEndpoint::Unresolved),
            ("relative.sock", DockerEndpoint::Unresolved),
        ];
        for (host, expected) in cases {
            assert_eq!(endpoint_from_host(host), expected, "host {host:?}");
        }
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

    /// One Docker configuration shape, as data, so the fail-closed table is a
    /// list of inputs rather than a list of fixture builders.
    enum ConfigShape {
        /// `config.json` holding exactly these bytes.
        Raw(&'static str),
        /// These contexts, an optional stored current context, and the
        /// `DOCKER_CONTEXT` override the caller sets.
        Stored(
            &'static [(&'static str, Option<&'static str>)],
            Option<&'static str>,
            DockerOverride,
        ),
        /// Overrides only, with no config directory this build can find.
        Overrides(DockerOverride, DockerOverride),
        /// A config path below a regular file.
        BelowFile,
    }

    fn candidates_for(shape: &ConfigShape) -> Vec<DockerEndpoint> {
        match shape {
            ConfigShape::Raw(raw) => {
                let dir = tempfile::tempdir().expect("create docker config fixture");
                std::fs::write(dir.path().join("config.json"), raw).expect("write config");
                docker_endpoint_candidates_from(
                    DockerOverride::Unset,
                    DockerOverride::Unset,
                    Some(dir.path().to_path_buf()),
                )
            }
            ConfigShape::Stored(contexts, current, docker_context) => {
                let dir = docker_context_fixture(contexts);
                if let Some(current) = current {
                    write_current_context(&dir, current);
                }
                docker_endpoint_candidates_from(
                    DockerOverride::Unset,
                    docker_context.clone(),
                    Some(dir.path().to_path_buf()),
                )
            }
            ConfigShape::Overrides(host, context) => {
                docker_endpoint_candidates_from(host.clone(), context.clone(), None)
            }
            ConfigShape::BelowFile => {
                // A non-`NotFound` config read error must not be read as an
                // absent stored context: the directory may still select a
                // remote endpoint.
                let dir = tempfile::tempdir().expect("create fixture root");
                let blocking_file = dir.path().join("not-a-directory");
                std::fs::write(&blocking_file, "file").expect("write blocking file");
                docker_endpoint_candidates_from(
                    DockerOverride::Unset,
                    DockerOverride::Unset,
                    Some(blocking_file.join(".docker")),
                )
            }
        }
    }

    /// Every configuration shape that cannot prove Docker inactive stays
    /// participating, and no probe is allowed to talk it out of that. Each row
    /// must also resolve a remote or unresolved endpoint, never only the
    /// platform default.
    #[test]
    fn docker_config_shapes_stay_participating() {
        let cases: [(&str, ConfigShape); 12] = [
            (
                "a stored context this build cannot resolve",
                ConfigShape::Stored(&[], Some("missing-context"), DockerOverride::Unset),
            ),
            ("an unreadable config", ConfigShape::Raw("{ not json")),
            (
                "a config that is not an object",
                ConfigShape::Raw("[1, 2, 3]"),
            ),
            (
                "a numeric currentContext",
                ConfigShape::Raw("{\"currentContext\": 42}"),
            ),
            (
                "a null currentContext",
                ConfigShape::Raw("{\"currentContext\": null}"),
            ),
            (
                "an object currentContext",
                ConfigShape::Raw("{\"currentContext\": {\"name\": \"remote\"}}"),
            ),
            (
                "a context without a Docker endpoint",
                ConfigShape::Stored(&[("bare", None)], Some("bare"), DockerOverride::Unset),
            ),
            (
                "an unknown config directory",
                ConfigShape::Overrides(DockerOverride::Unset, DockerOverride::Unset),
            ),
            ("a config path below a file", ConfigShape::BelowFile),
            (
                "a non-Unicode override",
                ConfigShape::Overrides(DockerOverride::NonUnicode, DockerOverride::NonUnicode),
            ),
            (
                // Docker context names are case-sensitive, so a context
                // literally named `DEFAULT` is selectable and must keep a
                // remote endpoint authoritative.
                "a remote context named DEFAULT by the environment",
                ConfigShape::Stored(
                    &[("DEFAULT", Some("tcp://10.0.0.5:2375"))],
                    None,
                    DockerOverride::Text("DEFAULT".to_owned()),
                ),
            ),
            (
                "a remote DEFAULT context in the store",
                ConfigShape::Stored(
                    &[("DEFAULT", Some("ssh://builder"))],
                    Some("DEFAULT"),
                    DockerOverride::Unset,
                ),
            ),
        ];
        for (name, shape) in cases {
            let candidates = candidates_for(&shape);
            assert!(
                candidates.iter().any(|endpoint| matches!(
                    endpoint,
                    DockerEndpoint::Remote | DockerEndpoint::Unresolved
                )),
                "{name}: expected a remote or unresolved candidate, got {candidates:?}"
            );
            assert_eq!(
                classify_docker_participation(candidates, |_| {
                    panic!(
                    "{name}: a remote, unresolved or unreadable configuration must not be probed away"
                )
                }),
                RuntimeParticipation::Participating,
                "{name}"
            );
        }

        #[cfg(unix)]
        {
            // The environment is read as raw bytes, so a non-Unicode value is
            // an unknown endpoint rather than an absent one.
            let candidates = {
                use std::os::unix::ffi::OsStrExt;
                let _lock = crate::test_env_lock();
                let previous_host = std::env::var_os("DOCKER_HOST");
                let previous_context = std::env::var_os("DOCKER_CONTEXT");
                std::env::set_var(
                    "DOCKER_HOST",
                    std::ffi::OsStr::from_bytes(b"unix:///tmp/\xff\xfe.sock"),
                );
                std::env::remove_var("DOCKER_CONTEXT");
                let candidates = docker_endpoint_candidates();
                match previous_host {
                    Some(value) => std::env::set_var("DOCKER_HOST", value),
                    None => std::env::remove_var("DOCKER_HOST"),
                }
                match previous_context {
                    Some(value) => std::env::set_var("DOCKER_CONTEXT", value),
                    None => std::env::remove_var("DOCKER_CONTEXT"),
                }
                candidates
            };
            assert!(candidates.contains(&DockerEndpoint::Unresolved));
            assert_eq!(
                classify_docker_participation(candidates, |_| SocketProbe::Absent),
                RuntimeParticipation::Participating
            );
        }
    }

    #[test]
    fn docker_context_overrides_an_absent_docker_host() {
        // The reviewed defect: a remote context selected by DOCKER_CONTEXT
        // must not be omitted because DOCKER_HOST points at an absent socket.
        let dir = docker_context_fixture(&[("remote", Some("tcp://10.0.0.5:2375"))]);
        let candidates = docker_endpoint_candidates_from(
            DockerOverride::Text("unix:///tmp/absent.sock".to_owned()),
            DockerOverride::Text("remote".to_owned()),
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
            DockerOverride::Text("ssh://builder".to_owned()),
            DockerOverride::Unset,
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
            DockerOverride::Text("unix:///tmp/other.sock".to_owned()),
            DockerOverride::Text("desktop-linux".to_owned()),
            Some(dir.path().to_path_buf()),
        );

        assert!(candidates.contains(&DockerEndpoint::Unix(PathBuf::from(
            "/Users/me/.docker/run/docker.sock"
        ))));
        assert!(candidates.contains(&DockerEndpoint::Unix(PathBuf::from("/tmp/other.sock"))));
    }

    /// Without an environment override the stored current context names the
    /// endpoint, and the platform default socket is still probed beside it.
    #[test]
    fn stored_current_context_and_default_context_name_select_the_endpoint() {
        let dir = docker_context_fixture(&[(
            "desktop-linux",
            Some("unix:///Users/me/.docker/run/docker.sock"),
        )]);
        write_current_context(&dir, "desktop-linux");
        let cases = [
            (
                "desktop-linux",
                vec![
                    DockerEndpoint::Unix(PathBuf::from("/Users/me/.docker/run/docker.sock")),
                    DockerEndpoint::DefaultLocal,
                ],
            ),
            ("default", vec![DockerEndpoint::DefaultLocal]),
        ];
        for (context, expected) in cases {
            write_current_context(&dir, context);
            assert_eq!(
                docker_endpoint_candidates_from(
                    DockerOverride::Unset,
                    DockerOverride::Unset,
                    Some(dir.path().to_path_buf())
                ),
                expected,
                "current context {context}"
            );
        }

        // A config directory with no stored context is the platform default.
        let empty = tempfile::tempdir().expect("create empty docker config fixture");
        assert_eq!(
            docker_endpoint_candidates_from(
                DockerOverride::Unset,
                DockerOverride::Unset,
                Some(empty.path().to_path_buf())
            ),
            vec![DockerEndpoint::DefaultLocal]
        );
    }

    /// Remote custody targets the context this resolves (effigy#236): an
    /// absent config or context is `default`, a named one is kept, and an
    /// unreadable or invalid config refuses instead of guessing.
    #[test]
    fn current_context_resolves_the_cli_context_and_refuses_an_invalid_config() {
        assert_eq!(current_context_in(None).as_deref(), Some("default"));
        let dir = tempfile::tempdir().expect("create docker config fixture");
        assert_eq!(
            current_context_in(Some(dir.path())).as_deref(),
            Some("default")
        );
        for (raw, expected) in [
            ("{}", Some("default")),
            ("{\"currentContext\": \"\"}", Some("default")),
            (
                "{\"currentContext\": \"desktop-linux\"}",
                Some("desktop-linux"),
            ),
            ("{\"currentContext\": 42}", None),
            ("[1, 2, 3]", None),
            ("not json", None),
        ] {
            std::fs::write(dir.path().join("config.json"), raw).expect("write config");
            assert_eq!(
                current_context_in(Some(dir.path())).as_deref(),
                expected,
                "{raw}"
            );
        }
    }

    #[test]
    fn docker_config_dir_preserves_exact_bytes() {
        let _lock = crate::test_env_lock();
        let previous = std::env::var_os("DOCKER_CONFIG");
        let root = tempfile::tempdir().expect("create fixture root");
        let config_dir = root.path().join("config ");
        std::fs::create_dir_all(&config_dir).expect("create trailing-space config dir");
        std::env::set_var("DOCKER_CONFIG", config_dir.as_os_str());

        let resolved = docker_config_dir();

        match previous {
            Some(value) => std::env::set_var("DOCKER_CONFIG", value),
            None => std::env::remove_var("DOCKER_CONFIG"),
        }
        assert_eq!(resolved, Some(config_dir));
    }

    #[cfg(unix)]
    #[test]
    fn inaccessible_config_directory_is_fail_closed() {
        use std::os::unix::fs::PermissionsExt;

        let dir = docker_context_fixture(&[("remote", Some("tcp://10.0.0.5:2375"))]);
        write_current_context(&dir, "remote");
        let restore = |mode: u32| {
            let mut permissions = std::fs::metadata(dir.path())
                .expect("config dir metadata")
                .permissions();
            permissions.set_mode(mode);
            std::fs::set_permissions(dir.path(), permissions).expect("restore config dir mode");
        };
        restore(0o000);

        let readable_even_locked = std::fs::read_to_string(dir.path().join("config.json")).is_ok();
        if readable_even_locked {
            // Running as a user that bypasses permissions (for example root);
            // the deterministic "config path below a file" row above covers
            // this I/O classification instead.
            restore(0o700);
            return;
        }

        let candidates = docker_endpoint_candidates_from(
            DockerOverride::Unset,
            DockerOverride::Unset,
            Some(dir.path().to_path_buf()),
        );
        restore(0o700);

        assert!(candidates.contains(&DockerEndpoint::Unresolved));
        assert_eq!(
            classify_docker_participation(candidates, |_| {
                panic!("an inaccessible config directory must not be probed away")
            }),
            RuntimeParticipation::Participating
        );
    }

    /// Colima state that can outlive the CLI: only a reachable Colima home with
    /// no instance directory at all is absent. A present, symlinked, dangling
    /// or unreadable record keeps Colima authoritative.
    #[test]
    fn colima_state_evidence_cases() {
        let root = tempfile::tempdir().expect("create colima home fixture");
        let empty = root.path().join("empty");
        let instance = root.path().join("instance");
        let off_layout = root.path().join("off-layout");
        let symlinked = root.path().join("symlinked");
        let dangling_entry = root.path().join("dangling-entry");
        let dangling_path = root.path().join("dangling-path");
        for home in [
            &empty,
            &instance,
            &off_layout,
            &symlinked,
            &dangling_entry,
            &dangling_path,
        ] {
            std::fs::create_dir_all(home).expect("create colima home");
        }
        std::fs::create_dir_all(instance.join("_lima").join("colima-effigy"))
            .expect("create lima instance dir");
        std::fs::write(off_layout.join("_lima"), "not a directory")
            .expect("write off-layout _lima file");
        for home in [&symlinked, &dangling_entry] {
            std::fs::create_dir_all(home.join("_lima")).expect("create lima home");
        }
        let target = root.path().join("real-instance");
        std::fs::create_dir_all(&target).expect("create symlink target");
        std::os::unix::fs::symlink(&target, symlinked.join("_lima").join("colima-linked"))
            .expect("create symlinked instance");
        std::os::unix::fs::symlink(
            root.path().join("missing-instance"),
            dangling_entry.join("_lima").join("colima-dangling"),
        )
        .expect("create dangling instance link");
        std::os::unix::fs::symlink(
            root.path().join("missing-lima"),
            dangling_path.join("_lima"),
        )
        .expect("create dangling _lima link");

        let cases: [(&str, Option<&Path>, ColimaStateEvidence); 7] = [
            ("no Colima home", None, ColimaStateEvidence::Unknown),
            (
                "an empty Colima home",
                Some(&empty),
                ColimaStateEvidence::None,
            ),
            (
                "a lima instance directory",
                Some(&instance),
                ColimaStateEvidence::Present,
            ),
            (
                "an off-layout _lima file",
                Some(&off_layout),
                ColimaStateEvidence::Unknown,
            ),
            (
                "a symlinked instance entry",
                Some(&symlinked),
                ColimaStateEvidence::Present,
            ),
            (
                "a dangling instance entry",
                Some(&dangling_entry),
                ColimaStateEvidence::Present,
            ),
            (
                "a dangling _lima link",
                Some(&dangling_path),
                ColimaStateEvidence::Present,
            ),
        ];
        for (name, colima_home, expected) in cases {
            assert_eq!(colima_state_evidence(colima_home), expected, "{name}");
        }

        // A present Colima record keeps an absent CLI participating.
        assert_eq!(
            classify_colima_participation(false, colima_state_evidence(Some(&instance))),
            RuntimeParticipation::Participating
        );
    }

    #[cfg(unix)]
    #[test]
    fn inaccessible_lima_instance_state_is_unknown() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("create colima home fixture");
        let instances = dir.path().join("_lima");
        std::fs::create_dir_all(instances.join("colima-effigy")).expect("create instance dir");
        let restore = |mode: u32| {
            let mut permissions = std::fs::metadata(&instances)
                .expect("lima dir metadata")
                .permissions();
            permissions.set_mode(mode);
            std::fs::set_permissions(&instances, permissions).expect("restore lima dir mode");
        };
        restore(0o000);

        let readable_even_locked = std::fs::read_dir(&instances).is_ok();
        if readable_even_locked {
            // Running as a user that bypasses permissions (for example root);
            // the off-layout and no-home rows above cover this classification
            // instead.
            restore(0o700);
            return;
        }

        let evidence = colima_state_evidence(Some(dir.path()));
        restore(0o700);

        assert_eq!(evidence, ColimaStateEvidence::Unknown);
    }
}
