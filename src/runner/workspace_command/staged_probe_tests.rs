//! Production-shaped proofs for the staged guest probe.
//!
//! Each test drives the same `ComposeWorkspaceGuest` exec route the publication
//! uses against a strict fake backend that stands in only for the container
//! runtime and the guest process boundary: the argv shape, the staged-object
//! gate and the probe document are the production ones, and the guest entry's
//! own `effigy_cli` parser checks the arguments the probe emits.
//!
//! No live machine gateway, Compose project or shared Acow state is touched.
//! These proofs establish the adapter's mapping and refusals; they are not
//! native backend evidence for any particular Docker engine or image store,
//! which belongs to the candidate qualification gates.

use super::super::reserved_guest::WorkspaceGuestProbe;
use super::*;
use crate::contract_test_support::EnvGuard;
use crate::runner::workspace_client::guest_probe::GuestProbeReply;
use crate::runner::workspace_client::identity::GuestSelfObservation;
use effigy_cli::{Command, GuestProbeArgs};
use effigy_test_support::Fixture;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const CONTAINER_ID: &str = "9d1c0b6a7f5e4d3c2b1a09f8e7d6c5b4a39281706f5e4d3c2b1a09f8e7d6c5b4";
const BOOT_ID: &str = "3f6a1c92-5b7e-4c1d-8a2f-0e9d7c6b5a41";
const CONTEXT_PATH: &str = "/run/effigy/context/7/context.json";
const PIN_HEX: &str = "a71c2f0b9d4e6a8c1f3b5d7e9a0c2e4f6b8d0a2c4e6f8b0d2a4c6e8f0b2d4a6c";
const PEER: &str = "192.168.65.254:49152";

/// A strict fake backend for the Compose exec route. The fake records the argv
/// it is called with and then does whatever the scenario body says, so a test
/// can shape the guest's answer without the adapter ever knowing.
struct Backend {
    fixture: Fixture,
    capture: PathBuf,
    reply: PathBuf,
    spawns: PathBuf,
}

impl Backend {
    fn new(label: &str) -> Self {
        let fixture = Fixture::new(&format!("effigy-staged-probe-{label}-")).expect("fixture root");
        std::fs::create_dir(fixture.path().join("bin")).expect("bin directory");
        let reply = fixture.path().join("reply.json");
        std::fs::write(&reply, "{}").expect("initial reply");
        let capture = fixture.path().join("exec-argv");
        let spawns = fixture.path().join("spawns");
        let backend = Self {
            fixture,
            capture,
            reply,
            spawns,
        };
        backend.body(&format!(
            "exec cat {}",
            shell_quote(&backend.reply.display().to_string())
        ));
        backend
    }

    /// Replace the fake guest's behaviour. The argv and the spawn count are
    /// always recorded first, so a proof can show that a refusal spawned
    /// nothing.
    fn body(&self, body: &str) {
        let program = self.fixture.path().join("bin").join("docker");
        std::fs::write(
            &program,
            format!(
                "#!/bin/sh\nprintf '%s\\0' \"$@\" > {capture}\nprintf 'x' >> {spawns}\n{body}\n",
                capture = shell_quote(&self.capture.display().to_string()),
                spawns = shell_quote(&self.spawns.display().to_string()),
            ),
        )
        .expect("fake backend script");
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755))
            .expect("fake backend mode");
    }

    /// How many external execs this guest was asked to perform.
    fn spawns(&self) -> usize {
        std::fs::read(&self.spawns)
            .map(|bytes| bytes.len())
            .unwrap_or(0)
    }

    fn respond(&self, reply: &serde_json::Value) {
        std::fs::write(&self.reply, reply.to_string()).expect("guest reply");
    }

    fn path(&self) -> &Path {
        self.fixture.path()
    }

    fn policy(&self) -> effigy_containers::EffectiveContainerPolicy {
        let mut policy = crate::runner::test_support::effective_container_policy(
            "dev",
            "dev",
            "app",
            self.fixture.path().join("compose.yml"),
        );
        policy.repo_root = self.fixture.path().to_path_buf();
        policy
    }

    fn env(&self) -> EnvGuard {
        EnvGuard::set_many(&[
            (
                "PATH",
                Some(format!(
                    "{}:{}",
                    self.fixture.path().join("bin").display(),
                    std::env::var("PATH").unwrap_or_default()
                )),
            ),
            ("EFFIGY_COMPOSE_BACKEND", Some("docker".to_owned())),
        ])
    }

    /// The backend's argv. `%s\0` keeps an argument that itself contains
    /// newlines — the shell that runs the staged object — as one argument.
    fn argv(&self) -> Vec<String> {
        std::fs::read_to_string(&self.capture)
            .expect("recorded backend argv")
            .split('\0')
            .filter(|arg| !arg.is_empty())
            .map(str::to_owned)
            .collect()
    }

    fn probe<'a>(
        &'a self,
        policy: &'a effigy_containers::EffectiveContainerPolicy,
        target: &ContainerIdentity,
    ) -> StagedGuestProbe<'a> {
        StagedGuestProbe::new(self.fixture.path(), policy, target).expect("staged guest probe")
    }

    fn linked_probe<'a>(
        &'a self,
        policy: &'a effigy_containers::EffectiveContainerPolicy,
    ) -> StagedGuestProbe<'a> {
        self.probe(policy, &target())
    }
}

fn target() -> ContainerIdentity {
    ContainerIdentity {
        container_id: CONTAINER_ID.to_owned(),
        vm_boot_id: BOOT_ID.to_owned(),
        backend: "docker-desktop".to_owned(),
    }
}

fn claimed() -> WorkspaceContainerIdentity {
    WorkspaceContainerIdentity {
        container_id: CONTAINER_ID.to_owned(),
        vm_boot_id: BOOT_ID.to_owned(),
        platform: "x86_64".to_owned(),
        libc: "gnu".to_owned(),
    }
}

fn self_observation() -> GuestSelfObservation {
    GuestSelfObservation {
        container_id: CONTAINER_ID.to_owned(),
        vm_boot_id: BOOT_ID.to_owned(),
        platform: "x86_64".to_owned(),
        libc: "gnu".to_owned(),
    }
}

fn object() -> GuestObject {
    GuestObject {
        path: "/run/effigy/objects/bin/effigy".to_owned(),
        digest: Digest::of(b"immutable staged guest executable"),
    }
}

fn context() -> GuestObject {
    GuestObject {
        path: CONTEXT_PATH.to_owned(),
        digest: Digest::of(b"immutable staged full v2 context"),
    }
}

fn pin() -> Digest {
    Digest::parse_hex(PIN_HEX).expect("canonical pin")
}

/// The digest a probe reply must name for the object the publication staged.
fn pinned_executable() -> String {
    format!("sha256:{}", object().digest.hex())
}

/// A probe reply built from the guest entry's own reply type, so these proofs
/// fail if the wire document and the adapter ever disagree.
fn reply(identity: &WorkspaceContainerIdentity) -> serde_json::Value {
    let document = GuestProbeReply {
        schema: "effigy.guest-probe.v1".to_owned(),
        identity: GuestSelfObservation {
            container_id: identity.container_id.clone(),
            vm_boot_id: identity.vm_boot_id.clone(),
            platform: identity.platform.clone(),
            libc: identity.libc.clone(),
        },
        executable: Some(pinned_executable()),
        address: None,
        peer: None,
        pin: None,
    };
    serde_json::to_value(document).expect("probe reply document")
}

fn observed_unknown<T: std::fmt::Debug>(observed: &Observed<T>) -> String {
    match observed {
        Observed::Unknown(reason) => reason.to_string(),
        other => panic!("expected Unknown, saw {other:?}"),
    }
}

fn guest_argv(args: Vec<String>) -> Command {
    effigy_cli::parse_command(args).expect("the guest entry parses the probe's argv")
}

#[test]
fn identity_observes_the_staged_object_in_the_verified_target() {
    let backend = Backend::new("identity");
    backend.respond(&reply(&claimed()));
    let _env = backend.env();
    let policy = backend.policy();
    let staged = object();
    let mut probe = backend.linked_probe(&policy);

    let observed = probe.identity(&claimed(), &staged, Duration::from_secs(5));

    match observed {
        Observed::Verified(identity, evidence) => {
            assert_eq!(identity, self_observation(), "the guest's own identity");
            assert_eq!(
                evidence.source(),
                "bounded exec of the staged digest object reported its own container identity"
            );
        }
        other => panic!("expected a verified identity, saw {other:?}"),
    }
    let argv = backend.argv();
    assert_eq!(argv.first().map(String::as_str), Some("exec"));
    // The exact verified container id, never the Compose service alias.
    assert!(argv.iter().any(|arg| arg == CONTAINER_ID));
    assert!(!argv.iter().any(|arg| arg == "app"));
    let script = argv.last().expect("the shell that runs the object");
    // The digest is taken from the descriptor the gate then executes, and a
    // refusal is marker-signalled so a guest exit status is never mistaken for
    // one.
    assert!(script.contains(&staged.digest.hex()));
    assert!(script.contains("exec 3< \"$p\""));
    assert!(script.contains("sha256sum <&3"));
    assert!(script.contains("exec /proc/self/fd/3"));
    assert!(!script.contains("exec \"$p\""), "never the mutable path");
    assert!(
        script
            .find("h=${h%% *}")
            .expect("the digest field the gate compares")
            < script.find("exec /proc/self/fd/3").expect("object exec"),
        "the digest gate precedes the exec"
    );
    assert!(
        script
            .find(GATE_OBJECT_DIFFERS)
            .expect("the gate's own refusal marker")
            < script.find("exec /proc/self/fd/3").expect("object exec"),
        "a refusal comes from the gate, before the object runs"
    );
    assert!(script.contains("__guest-identity"));
    assert_eq!(
        guest_argv(identity_args()),
        Command::InternalGuestProbe(GuestProbeArgs::Identity)
    );
}

#[test]
fn identity_refuses_another_identity_target_or_object() {
    let backend = Backend::new("identity-negative");
    let _env = backend.env();
    let policy = backend.policy();

    // A guest that reports another container is uncertainty, never a value.
    let mut elsewhere = claimed();
    elsewhere.container_id = "b".repeat(64);
    backend.respond(&reply(&elsewhere));
    let mut probe = backend.linked_probe(&policy);
    let reason = observed_unknown(&probe.identity(&claimed(), &object(), Duration::from_secs(5)));
    assert!(reason.contains("another identity"), "{reason}");

    // A boot that moved on refuses before any exec: the capture still holds
    // the first observation's argv.
    let captured = std::fs::read_to_string(&backend.capture).expect("first argv");
    let mut moved = claimed();
    moved.vm_boot_id = "8e7d6c5b-4a39-4172-9605-8d7c6b5a4132".to_owned();
    let reason = observed_unknown(&probe.identity(&moved, &object(), Duration::from_secs(5)));
    assert!(reason.contains("another container or boot"), "{reason}");
    assert_eq!(
        std::fs::read_to_string(&backend.capture).expect("unchanged argv"),
        captured,
        "a refused target spawns no probe"
    );

    // The object bound by the first observation cannot be replaced later.
    let mut restaged = object();
    restaged.digest = Digest::of(b"a replacement object");
    let reason = observed_unknown(&probe.identity(&claimed(), &restaged, Duration::from_secs(5)));
    assert!(reason.contains("another immutable object"), "{reason}");

    // A Compose service alias or a short id can never name the exec target.
    for refused in ["app", "9d1c0b6a"] {
        let id = ContainerIdentity {
            container_id: refused.to_owned(),
            vm_boot_id: BOOT_ID.to_owned(),
            backend: "docker-desktop".to_owned(),
        };
        assert!(
            StagedGuestProbe::new(backend.path(), &policy, &id).is_err(),
            "`{refused}` is not a verified container id"
        );
    }
}

#[test]
fn identity_distinguishes_absence_from_an_unreadable_object() {
    let backend = Backend::new("identity-exits");
    let _env = backend.env();
    let policy = backend.policy();
    let staged = object();

    // A staged program that exits with the gate's old-style code is not an
    // absence proof: the gate's marker must name the refusal too.
    for code in ["3", "64", "65", "66", "67", "68"] {
        backend.body(&format!("printf 'the object is fine' >&2; exit {code}"));
        let mut probe = backend.linked_probe(&policy);
        let reason = observed_unknown(&probe.identity(&claimed(), &staged, Duration::from_secs(5)));
        assert!(
            reason.contains(&format!("ended with {code}")),
            "exit {code} is a failed observation, not a gate refusal: {reason}"
        );
        assert!(reason.contains("the object is fine"), "{reason}");
    }

    // Only the gate's own marker plus its reserved code proves absence.
    backend.body(&format!("printf '{GATE_OBJECT_ABSENT}\\n' >&2; exit 64"));
    let mut probe = backend.linked_probe(&policy);
    assert_eq!(
        probe.identity(&claimed(), &staged, Duration::from_secs(5)),
        Observed::Absent(AbsenceProof::NoFile {
            name: staged.path.clone()
        })
    );

    // The marker with the wrong code does not become an absence proof either.
    backend.body(&format!("printf '{GATE_OBJECT_ABSENT}\\n' >&2; exit 70"));
    let mut probe = backend.linked_probe(&policy);
    let reason = observed_unknown(&probe.identity(&claimed(), &staged, Duration::from_secs(5)));
    assert!(reason.contains("ended with 70"), "{reason}");

    backend.body(&format!("printf '{GATE_NO_HASH_TOOL}\\n' >&2; exit 65"));
    let mut probe = backend.linked_probe(&policy);
    let reason = observed_unknown(&probe.identity(&claimed(), &staged, Duration::from_secs(5)));
    assert!(reason.contains("no SHA-256 utility"), "{reason}");

    backend.body(&format!("printf '{GATE_OBJECT_DIFFERS}\\n' >&2; exit 66"));
    let mut probe = backend.linked_probe(&policy);
    let reason = observed_unknown(&probe.identity(&claimed(), &staged, Duration::from_secs(5)));
    assert!(reason.contains("requested immutable digest"), "{reason}");

    // A path occupied by something that is not the immutable regular file is a
    // refusal, not the absence of the object.
    backend.body(&format!(
        "printf '{GATE_OBJECT_NOT_REGULAR}\\n' >&2; exit 67"
    ));
    let mut probe = backend.linked_probe(&policy);
    let reason = observed_unknown(&probe.identity(&claimed(), &staged, Duration::from_secs(5)));
    assert!(
        reason.contains("not the immutable regular file"),
        "{reason}"
    );

    backend.body(&format!(
        "printf '{GATE_OBJECT_UNREADABLE}\\n' >&2; exit 68"
    ));
    let mut probe = backend.linked_probe(&policy);
    let reason = observed_unknown(&probe.identity(&claimed(), &staged, Duration::from_secs(5)));
    assert!(reason.contains("could not be opened"), "{reason}");

    backend.body("printf 'not a probe document'");
    let mut probe = backend.linked_probe(&policy);
    let reason = observed_unknown(&probe.identity(&claimed(), &staged, Duration::from_secs(5)));
    assert!(reason.contains("no probe document"), "{reason}");
}

#[test]
fn identity_refuses_a_reply_from_another_executable() {
    let backend = Backend::new("other-executable");
    let _env = backend.env();
    let policy = backend.policy();
    let staged = object();

    // The guest hashes `/proc/self/exe`; a reply for another binary is not
    // evidence about the staged object, whatever path was executed.
    for other in [
        serde_json::json!(format!("sha256:{}", "e".repeat(64))),
        serde_json::Value::Null,
    ] {
        let mut reply = reply(&claimed());
        reply["executable"] = other.clone();
        backend.respond(&reply);
        let mut probe = backend.linked_probe(&policy);
        let reason = observed_unknown(&probe.identity(&claimed(), &staged, Duration::from_secs(5)));
        assert!(
            reason.contains("another executable than the pinned object"),
            "{reason}"
        );
    }
    assert_eq!(backend.spawns(), 2, "each refusal is one observation");
}

#[test]
fn the_object_gate_runs_the_pinned_bytes_and_refuses_everything_else() {
    // The gate is real POSIX shell over real files and the image's own digest
    // tools: this is the only step between a staged path and running it.
    let fixture = Fixture::new("effigy-staged-probe-gate-").expect("fixture root");
    let object_bytes = b"#!/bin/sh\nprintf 'probe-ran'\n".to_vec();
    let digest = Digest::of(&object_bytes);
    let replacement_bytes = b"#!/bin/sh\nprintf 'replaced'\n";
    // The object path is digest-addressed, as production object paths are, so a
    // path token can never stand in for the hashed bytes.
    let object_path = fixture.path().join(digest.hex());
    let object_text = object_path.display().to_string();
    std::fs::write(&object_path, &object_bytes).expect("staged object");
    std::fs::set_permissions(&object_path, std::fs::Permissions::from_mode(0o755))
        .expect("staged object mode");
    let empty_bin = fixture.path().join("empty-bin");
    std::fs::create_dir(&empty_bin).expect("empty bin");
    let system_path = Path::new("/usr/bin:/bin:/usr/sbin:/sbin");

    let gate = |path: &str, digest: Digest, search_path: &Path| {
        let script = staged_object_probe_script(
            &GuestObject {
                path: path.to_owned(),
                digest,
            },
            &identity_args(),
        );
        let output = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(script)
            .env("PATH", search_path)
            .output()
            .expect("run the staged-object gate");
        (
            output.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output.stdout).to_string(),
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        )
    };

    #[cfg(target_os = "linux")]
    {
        let (code, stdout, stderr) = gate(&object_text, digest.clone(), system_path);
        assert_eq!(
            (code, stdout.as_str(), stderr.as_str()),
            (0, "probe-ran", "")
        );
    }

    // A path replaced before the gate runs is refused, never executed.
    std::fs::write(&object_path, replacement_bytes).expect("replaced object");
    let (code, stdout, stderr) = gate(&object_text, digest.clone(), system_path);
    assert_eq!(code, 66);
    assert_eq!(stderr, GATE_OBJECT_DIFFERS);
    assert!(stdout.is_empty(), "a refused object never runs");

    // The gate takes the digest from the descriptor it then executes, so a path
    // replaced *after* the digest was read is neither hashed nor run. This tool
    // hashes the pinned bytes honestly and then swaps the object behind the
    // gate's back: exactly the race a path-only check loses.
    std::fs::write(&object_path, &object_bytes).expect("restore staged object");
    let racing_bin = fixture.path().join("racing-bin");
    std::fs::create_dir(&racing_bin).expect("racing bin");
    let racing_tool = racing_bin.join("sha256sum");
    std::fs::write(
        &racing_tool,
        format!(
            "#!/bin/sh\nh=$(shasum -a 256)\nprintf '%s\\n' \"$h\"\nrm -f {object} && printf '%s' {replacement} > {object}\n",
            object = shell_quote(&object_text),
            replacement = shell_quote(&String::from_utf8_lossy(replacement_bytes)),
        ),
    )
    .expect("racing digest tool");
    std::fs::set_permissions(&racing_tool, std::fs::Permissions::from_mode(0o755))
        .expect("racing tool mode");
    let racing_path = format!(
        "{}:{system_path}",
        racing_bin.display(),
        system_path = system_path.display()
    );
    let (code, stdout, stderr) = gate(&object_text, digest.clone(), Path::new(&racing_path));
    assert_eq!(
        std::fs::read(&object_path).expect("swapped object"),
        replacement_bytes,
        "the proof actually performed the swap"
    );
    #[cfg(target_os = "linux")]
    {
        // The descriptor still names the pinned inode, so the pinned bytes run.
        assert_eq!((code, stderr.as_str()), (0, ""));
        assert_eq!(
            stdout, "probe-ran",
            "the executed bytes are the bytes the digest was taken from"
        );
    }
    #[cfg(not(target_os = "linux"))]
    {
        // A digest taken from the path would have differed (66); reaching the
        // exec proves the digest came from the descriptor. This kernel cannot
        // execute a descriptor, so the gate refuses instead of falling back to
        // the mutable path.
        assert_eq!(
            code, 126,
            "the digest came from the descriptor, and the run fails closed: {stderr}"
        );
        assert!(stdout.is_empty(), "the gate never falls back to `$p`");
    }

    // Something occupying the object path that is not the immutable regular
    // file is its own refusal, never a verified absence.
    let occupied = fixture.path().join("occupied-object");
    std::fs::create_dir(&occupied).expect("occupied object");
    let (code, stdout, stderr) = gate(&occupied.display().to_string(), digest.clone(), system_path);
    assert_eq!(code, 67);
    assert_eq!(stderr, GATE_OBJECT_NOT_REGULAR);
    assert!(stdout.is_empty());
    std::fs::remove_dir(&occupied).expect("remove occupied object");
    std::os::unix::fs::symlink(&object_path, &occupied).expect("symlinked object");
    let (code, _, stderr) = gate(&occupied.display().to_string(), digest.clone(), system_path);
    assert_eq!((code, stderr.as_str()), (67, GATE_OBJECT_NOT_REGULAR));
    std::fs::remove_file(&occupied).expect("remove symlink");

    // Only a path with nothing at it is a verified absence.
    let missing = fixture.path().join("missing-object");
    let (code, stdout, stderr) = gate(&missing.display().to_string(), digest.clone(), system_path);
    assert_eq!(code, 64);
    assert_eq!(stderr, GATE_OBJECT_ABSENT);
    assert!(stdout.is_empty());

    // An image with no digest tool is unavailable, not a pass.
    std::fs::write(&object_path, &object_bytes).expect("restore staged object");
    let (code, stdout, stderr) = gate(&object_text, digest, empty_bin.as_path());
    assert_eq!(code, 65);
    assert_eq!(stderr, GATE_NO_HASH_TOOL);
    assert!(stdout.is_empty());
}

#[test]
fn the_publication_guest_shares_one_deadline_and_refuses_without_spawning() {
    let backend = Backend::new("deadline-guest");
    let _env = backend.env();
    let policy = backend.policy();
    let target = target();

    // Reads, digests and the identity probe all go through the bounded Compose
    // route the publication stages with.
    backend.body("printf 'context bytes'");
    let mut guest = DeadlineGuest::new(
        backend.path(),
        &policy,
        &target,
        Instant::now() + Duration::from_secs(5),
    )
    .expect("deadline guest");
    assert_eq!(
        guest
            .read_file("/run/effigy/context/7/context.json")
            .expect("bounded read"),
        Some(b"context bytes".to_vec())
    );
    assert_eq!(backend.spawns(), 1);

    backend.body(&format!(
        "printf '{PIN_HEX}  /run/effigy/objects/bin/effigy\\n'"
    ));
    assert_eq!(
        guest
            .file_sha256("/run/effigy/objects/bin/effigy")
            .expect("bounded digest")
            .as_deref(),
        Some(PIN_HEX)
    );
    backend.body(&format!(
        "printf 'boot={BOOT_ID}\\nplatform=x86_64\\nlibc=gnu\\n'"
    ));
    let observed = guest.probe_container_identity().expect("bounded identity");
    assert_eq!(observed.container_id, target.container_id);
    assert_eq!(observed.vm_boot_id, target.vm_boot_id);
    assert_eq!(backend.spawns(), 3, "one bounded exec per operation");

    // One absolute deadline: past it, every operation refuses before spawning.
    let mut expired = DeadlineGuest::new(
        backend.path(),
        &policy,
        &target,
        Instant::now() - Duration::from_secs(1),
    )
    .expect("deadline guest");
    assert!(expired
        .read_file("/run/effigy/context/7/context.json")
        .is_err());
    assert!(expired.probe_container_identity().is_err());
    assert_eq!(backend.spawns(), 3, "an expired deadline spawns nothing");

    // A Compose service alias can never become the exec target.
    let alias = ContainerIdentity {
        container_id: "app".to_owned(),
        vm_boot_id: BOOT_ID.to_owned(),
        backend: "docker-desktop".to_owned(),
    };
    assert!(DeadlineGuest::new(
        backend.path(),
        &policy,
        &alias,
        Instant::now() + Duration::from_secs(5)
    )
    .is_err());
}

#[test]
fn context_dial_reports_the_guests_own_peer_and_pin() {
    let backend = Backend::new("context-dial");
    let mut expected = reply(&claimed());
    expected["peer"] = serde_json::json!(PEER);
    expected["pin"] = serde_json::json!(format!("sha256:{PIN_HEX}"));
    backend.respond(&expected);
    let _env = backend.env();
    let policy = backend.policy();
    let staged = object();
    let context = context();
    let mut probe = backend.linked_probe(&policy);

    let observed = probe.context_dial(&claimed(), &staged, &context, Duration::from_millis(1500));

    match observed {
        Observed::Verified(evidence, _) => {
            assert_eq!(evidence.target, claimed());
            assert_eq!(evidence.peer.to_string(), PEER);
            assert_eq!(evidence.pin, pin());
        }
        other => panic!("expected a verified dial, saw {other:?}"),
    }
    let argv = backend.argv();
    let script = argv.last().expect("the shell that runs the object");
    assert!(script.contains("--context-dial"));
    assert!(script.contains(&context.path));
    assert!(script.contains(&context.digest.hex()));
    assert!(script.contains("'--timeout-ms' '1500'"));
    // The guest entry's own parser accepts exactly these arguments.
    let args = context_dial_args(&context, Duration::from_millis(1500));
    assert_eq!(
        guest_argv(args),
        Command::InternalGuestProbe(GuestProbeArgs::ContextDial {
            context: CONTEXT_PATH.to_owned(),
            digest: context.digest.hex(),
            timeout_ms: 1500,
        })
    );
}

#[test]
fn context_dial_refuses_an_absent_peer_or_pin() {
    let backend = Backend::new("context-dial-negative");
    let _env = backend.env();
    let policy = backend.policy();
    let staged = object();
    let context = context();

    // Identity, peer and pin all come from the guest's own document read.
    let mut elsewhere = claimed();
    elsewhere.container_id = "c".repeat(64);
    let mut wrong_identity = reply(&elsewhere);
    wrong_identity["peer"] = serde_json::json!(PEER);
    wrong_identity["pin"] = serde_json::json!(format!("sha256:{PIN_HEX}"));
    backend.respond(&wrong_identity);
    let mut probe = backend.linked_probe(&policy);
    let reason = observed_unknown(&probe.context_dial(
        &claimed(),
        &staged,
        &context,
        Duration::from_secs(5),
    ));
    assert!(reason.contains("another identity"), "{reason}");

    let mut without_peer = reply(&claimed());
    without_peer["pin"] = serde_json::json!(format!("sha256:{PIN_HEX}"));
    backend.respond(&without_peer);
    let mut probe = backend.linked_probe(&policy);
    let reason = observed_unknown(&probe.context_dial(
        &claimed(),
        &staged,
        &context,
        Duration::from_secs(5),
    ));
    assert!(reason.contains("no peer"), "{reason}");

    let mut without_pin = reply(&claimed());
    without_pin["peer"] = serde_json::json!(PEER);
    backend.respond(&without_pin);
    let mut probe = backend.linked_probe(&policy);
    let reason = observed_unknown(&probe.context_dial(
        &claimed(),
        &staged,
        &context,
        Duration::from_secs(5),
    ));
    assert!(reason.contains("no canonical pin"), "{reason}");

    let mut malformed_pin = reply(&claimed());
    malformed_pin["peer"] = serde_json::json!(PEER);
    malformed_pin["pin"] = serde_json::json!("sha256:not-a-digest");
    backend.respond(&malformed_pin);
    let mut probe = backend.linked_probe(&policy);
    let reason = observed_unknown(&probe.context_dial(
        &claimed(),
        &staged,
        &context,
        Duration::from_secs(5),
    ));
    assert!(reason.contains("no canonical pin"), "{reason}");
}

#[test]
fn alias_resolution_and_pinned_dial_refuse_another_alias_peer_or_pin() {
    let backend = Backend::new("serving");
    let _env = backend.env();
    let policy = backend.policy();
    // The channel methods receive only a target, so the publication's own
    // identity observation binds the object they execute.
    backend.respond(&reply(&claimed()));
    let mut probe = backend.linked_probe(&policy);
    assert!(matches!(
        probe.identity(&claimed(), &object(), Duration::from_secs(5)),
        Observed::Verified(..)
    ));

    let reason =
        observed_unknown(&probe.resolve(&claimed(), "another-host", Duration::from_secs(5)));
    assert!(reason.contains("never prepared"), "{reason}");

    let mut resolved = reply(&claimed());
    resolved["address"] = serde_json::json!("192.168.65.254");
    backend.respond(&resolved);
    match probe.resolve(&claimed(), "effigy-host", Duration::from_secs(5)) {
        Observed::Verified(alias, _) => {
            assert_eq!(alias.target, claimed());
            assert_eq!(alias.address.to_string(), "192.168.65.254");
        }
        other => panic!("expected a verified alias, saw {other:?}"),
    }
    assert_eq!(
        guest_argv(resolve_args()),
        Command::InternalGuestProbe(GuestProbeArgs::Resolve)
    );

    for refused in ["192.168.65.254:49152", "effigy-host:0", "localhost:49152"] {
        let reason =
            observed_unknown(&probe.dial(&claimed(), refused, &pin(), Duration::from_secs(5)));
        assert!(
            reason.contains("never prepared"),
            "`{refused}` named {reason}"
        );
    }
    let reason = observed_unknown(&probe.dial(
        &claimed(),
        "effigy-host:49152",
        &Digest([0; 32]),
        Duration::from_secs(5),
    ));
    assert!(reason.contains("never prepared"), "{reason}");

    // A dial that observed another pin, or no peer at all, is not the session's.
    let mut another_pin = reply(&claimed());
    another_pin["peer"] = serde_json::json!(PEER);
    another_pin["pin"] = serde_json::json!(format!("sha256:{}", "d".repeat(64)));
    backend.respond(&another_pin);
    let reason = observed_unknown(&probe.dial(
        &claimed(),
        "effigy-host:49152",
        &pin(),
        Duration::from_secs(5),
    ));
    assert!(reason.contains("another session pin"), "{reason}");

    let mut without_peer = reply(&claimed());
    without_peer["pin"] = serde_json::json!(format!("sha256:{PIN_HEX}"));
    backend.respond(&without_peer);
    let reason = observed_unknown(&probe.dial(
        &claimed(),
        "effigy-host:49152",
        &pin(),
        Duration::from_secs(5),
    ));
    assert!(reason.contains("no peer"), "{reason}");

    let mut matched = reply(&claimed());
    matched["peer"] = serde_json::json!(PEER);
    matched["pin"] = serde_json::json!(format!("sha256:{PIN_HEX}"));
    backend.respond(&matched);
    match probe.dial(
        &claimed(),
        "effigy-host:49152",
        &pin(),
        Duration::from_millis(1500),
    ) {
        Observed::Verified(evidence, _) => {
            assert_eq!(evidence.target, claimed());
            assert_eq!(evidence.peer.to_string(), PEER);
            assert_eq!(evidence.pin, pin());
        }
        other => panic!("expected a verified pinned dial, saw {other:?}"),
    }
    assert_eq!(
        guest_argv(dial_args(
            "effigy-host:49152",
            &pin(),
            Duration::from_millis(1500)
        )),
        Command::InternalGuestProbe(GuestProbeArgs::Dial {
            endpoint: "effigy-host:49152".to_owned(),
            pin: format!("sha256:{PIN_HEX}"),
            timeout_ms: 1500,
        })
    );
}

#[test]
fn a_hung_staged_probe_is_bounded_and_reported_as_a_timeout() {
    let backend = Backend::new("timeout");
    backend.body("exec sleep 2");
    let _env = backend.env();
    let policy = backend.policy();
    let mut probe = backend.linked_probe(&policy);

    let started = Instant::now();
    let reason =
        observed_unknown(&probe.identity(&claimed(), &object(), Duration::from_millis(150)));

    assert!(
        reason.contains("timed out") || reason.contains("staged guest observation"),
        "{reason}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "the bounded exec ended at its deadline, took {:?}",
        started.elapsed()
    );
}

#[test]
fn the_guest_dial_budget_is_never_zero_and_never_past_the_entry_ceiling() {
    assert_eq!(guest_dial_ms(Duration::ZERO), 1);
    assert_eq!(guest_dial_ms(Duration::from_millis(1500)), 1500);
    assert_eq!(guest_dial_ms(Duration::from_secs(120)), 30_000);
}
