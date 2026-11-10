//! Execution-only recovery for Effigy's private, source-bound Linux builder.
//! A retained builder keeps its original admission. No stack is readmitted,
//! stopped or recreated; every build targets a freshly verified immutable ID.
//!
//! The one thing the build does refresh is the source it compiles. A VM reaches
//! only the host paths its Colima profile shares, so a development checkout
//! outside every shared mount binds its own staged source closure under the
//! profile's mount (effigy#715). The closure is staged before anything renders
//! or activates, proven through the container's own bind before the build, and
//! only the verified artifact bytes reach the caller.
//!
//! Cargo intermediates and the three artifact roles stay in the build's own
//! private tree inside the builder (effigy#743, effigy#749). The `/workspace`
//! source bind is read-only: the compiler has no writable host bind. After
//! positive closure the host transfers the three capped roles through a bounded
//! stream and atomically publishes a complete verified set.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::Path;
use std::time::{Duration, Instant};

use effigy_containers::exec::{
    list_compose_containers_for_policy_including_stopped_with_deadline,
    list_running_compose_containers_for_policy_with_deadline, observed_workspace_artifact_builder,
    verify_workspace_artifact_builder, verify_workspace_artifact_builder_declaration,
    workspace_artifact_builder_preparation, VerifiedComposeContainer, WorkspaceBuilderPreparation,
};
use effigy_containers::{
    load_container_policy, ContainerAction, EffectiveContainerPolicy, ARTIFACT_BINARY_CAP_BYTES,
    ARTIFACT_RECEIPT_CAP_BYTES, ARTIFACT_SIDECAR_CAP_BYTES, STAGED_SOURCE_CAP_BYTES,
};

use crate::runner::environment;
use crate::runner::error::RunnerError;
use crate::runner::exec_command::{
    BoundedCaptureFailure, BoundedCommandBudget, BoundedCommandPhase, BoundedScopedCaptureFailure,
};
use crate::runner::task_engine::TaskRun;

use super::workspace_artifact_source;

/// Whether the builder's environment is recorded as retained: its record
/// published `active`, so an existing container is the one it describes.
fn builder_retained(policy: &effigy_containers::EffectiveContainerPolicy) -> bool {
    environment::record(policy).is_some_and(|record| record.active())
}

/// Restriction-only switch set by the Recipe E entry. `1` makes this build reuse
/// the already running, already configured profile and refuse every start,
/// create, restart, settings rewrite or unit start. Absence keeps ordinary
/// acquisition. Any other value refuses, so the switch can never widen authority.
const NO_CHANGE_ENV: &str = "EFFIGY_LINUX_RELEASE_REQUIRE_RUNNING";

/// The build's cargo tree: a fixed private path on the builder's own writable
/// layer. It is not a profile mount, not a host bind and not shared with another
/// checkout, so intermediates never reach the shared mount the source lives in.
const BUILD_TREE_ROOT: &str = "/var/tmp/effigy-artifact-build";
/// Declared cap on the build tree's retained bytes and the free-space floor the
/// builder's own filesystem must still show before a build starts. The cap is a
/// conservative premise for a from-scratch release build of this workspace; the
/// floor keeps the tree, the retained artifact and the container's own writes
/// inside the VM disk's remaining headroom.
const BUILD_TREE_CAP_BYTES: u64 = 25 * 1024 * 1024 * 1024;
const BUILD_TREE_HEADROOM_BYTES: u64 = BUILD_TREE_CAP_BYTES + 5 * 1024 * 1024 * 1024;
/// Schema of the one record a successful build publishes about its own writers.
const BOUNDARY_SCHEMA: &str = "effigy.builder-write-boundary.v1";

/// Which half of the build a probe observes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProbePhase {
    /// Nothing is compiled yet: the tree is fresh (or was reset above its cap),
    /// the source is the recorded closure and the filesystem has headroom.
    Before,
    /// The build returned: the same closure is still the only source, the export
    /// holds exactly the declared names within their caps, no build process is
    /// left and the tree's bytes are stable across a settle interval.
    After,
}

impl ProbePhase {
    fn label(self) -> &'static str {
        match self {
            Self::Before => "before",
            Self::After => "after",
        }
    }

    fn keys(self) -> &'static [&'static str] {
        match self {
            Self::Before => &[
                "census",
                "source_files",
                "nested_workspace_mounts",
                "source_bind_ro",
                "build_output_entries",
                "build_tree_mounts",
                "build_tree_retained_bytes",
                "build_tree_reset",
                "build_tree_free_bytes",
            ],
            Self::After => &[
                "census",
                "source_files",
                "nested_workspace_mounts",
                "source_bind_ro",
                "export_dir_entries",
                "export_unexpected",
                "export_unexpected_kind",
                "export_binary_bytes",
                "export_receipt_bytes",
                "export_sidecar_bytes",
                "build_tree_retained_bytes",
                "build_tree_open_writers",
                "build_tree_quiescent",
                "build_tree_free_bytes",
            ],
        }
    }
}

/// What one probe observed. Every value is a non-negative integer the container
/// itself reported; nothing here is derived from a host-side sample.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Probe {
    phase: ProbePhase,
    values: BTreeMap<String, u64>,
    /// What the after-probe found where only the artifact export belongs. It is
    /// a fixed vocabulary rather than a path, so a refusal can name the surface
    /// without carrying container text into a message.
    unexpected_kind: String,
}

impl Probe {
    fn get(&self, key: &str) -> u64 {
        self.values.get(key).copied().unwrap_or_default()
    }

    fn unexpected(&self) -> &str {
        if self.unexpected_kind.is_empty() {
            "artifacts"
        } else {
            &self.unexpected_kind
        }
    }
}

/// What the after-probe reports where only the artifact export belongs.
const EXPORT_UNEXPECTED_KINDS: [&str; 7] = [
    "artifacts",
    "target",
    "tree",
    "service",
    "entry",
    "link",
    "missing",
];

/// Parse a probe's stdout strictly: `key value` lines only, the exact expected
/// key set, decimal integers, no duplicates and nothing else.
fn parse_probe(stdout: &str, phase: ProbePhase) -> Result<Probe, RunnerError> {
    let keys = phase.keys();
    let mut values = BTreeMap::new();
    let mut unexpected_kind = String::new();
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some((key, raw)) = line.split_once(' ') else {
            return Err(RunnerError::task_invocation(format!(
                "workspace artifact {} boundary probe reported an unparsable line: {line}",
                phase.label()
            )));
        };
        let key = key.trim();
        let raw = raw.trim();
        if !keys.contains(&key) {
            return Err(RunnerError::task_invocation(format!(
                "workspace artifact {} boundary probe reported an unknown field `{key}`",
                phase.label()
            )));
        }
        if values.contains_key(key)
            || (key == "export_unexpected_kind" && !unexpected_kind.is_empty())
        {
            return Err(RunnerError::task_invocation(format!(
                "workspace artifact {} boundary probe reported `{key}` twice",
                phase.label()
            )));
        }
        if key == "export_unexpected_kind" {
            if !EXPORT_UNEXPECTED_KINDS.contains(&raw) {
                return Err(RunnerError::task_invocation(format!(
                    "workspace artifact {} boundary probe reported an unknown unexpected-entry kind",
                    phase.label()
                )));
            }
            unexpected_kind = raw.to_owned();
            continue;
        }
        if raw.is_empty() || raw.len() > 20 || !raw.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(RunnerError::task_invocation(format!(
                "workspace artifact {} boundary probe reported a non-integer `{key}` value",
                phase.label()
            )));
        }
        values.insert(key.to_owned(), raw.parse::<u64>().map_err(|error| {
            RunnerError::task_invocation(format!(
                "workspace artifact {} boundary probe reported an unreadable `{key}` value: {error}",
                phase.label()
            ))
        })?);
    }
    let missing = keys
        .iter()
        .filter(|key| {
            if **key == "export_unexpected_kind" {
                unexpected_kind.is_empty()
            } else {
                !values.contains_key(**key)
            }
        })
        .copied()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(RunnerError::task_invocation(format!(
            "workspace artifact {} boundary probe did not report {}",
            phase.label(),
            missing.join(", ")
        )));
    }
    Ok(Probe {
        phase,
        values,
        unexpected_kind,
    })
}

/// The staged export's declared per-file caps, in the order the probe reports
/// them.
fn export_caps() -> [(&'static str, u64); 3] {
    [
        ("export_binary_bytes", ARTIFACT_BINARY_CAP_BYTES),
        ("export_receipt_bytes", ARTIFACT_RECEIPT_CAP_BYTES),
        ("export_sidecar_bytes", ARTIFACT_SIDECAR_CAP_BYTES),
    ]
}

/// Refuse a probe reading whose observed writers leave the declared boundary.
fn check_probe(probe: &Probe) -> Result<(), RunnerError> {
    let refuse = |reason: String| Err(RunnerError::task_invocation(reason));
    if probe.get("nested_workspace_mounts") != 0 {
        return refuse(format!(
            "the builder mounts {} path(s) below `/workspace`; the source bind is not the only host surface",
            probe.get("nested_workspace_mounts")
        ));
    }
    if probe.get("source_bind_ro") != 1 {
        return refuse(
            "the builder's `/workspace` source bind is not read-only; compiler writes through it are not contained"
                .to_owned(),
        );
    }
    if probe.get("build_tree_retained_bytes") > BUILD_TREE_CAP_BYTES {
        return refuse(format!(
            "the builder's cargo tree holds {} bytes, above its declared {BUILD_TREE_CAP_BYTES}-byte cap",
            probe.get("build_tree_retained_bytes")
        ));
    }
    if probe.get("build_tree_free_bytes") < BUILD_TREE_HEADROOM_BYTES {
        return refuse(format!(
            "the builder's filesystem shows {} free bytes, below the declared {BUILD_TREE_HEADROOM_BYTES}-byte floor; no build ran",
            probe.get("build_tree_free_bytes")
        ));
    }
    match probe.phase {
        ProbePhase::Before => {
            if probe.get("build_tree_mounts") != 0 {
                return refuse(
                    "the builder's cargo tree is covered by a mount instead of its own writable layer"
                        .to_owned(),
                );
            }
            Ok(())
        }
        ProbePhase::After => {
            if probe.get("export_unexpected") != 0 {
                return refuse(unexpected_refusal(probe));
            }
            for (key, cap) in export_caps() {
                if probe.get(key) > cap {
                    return refuse(format!(
                        "the artifact export's `{key}` is {} bytes, above its declared {cap}-byte cap",
                        probe.get(key)
                    ));
                }
            }
            if probe.get("export_binary_bytes") == 0 {
                return refuse("the artifact export wrote an empty binary".to_owned());
            }
            if probe.get("build_tree_open_writers") != 0 {
                return refuse(
                    "a build process still holds the builder's cargo tree; its closure is unknown"
                        .to_owned(),
                );
            }
            if probe.get("build_tree_quiescent") != 1 {
                return refuse(
                    "the builder's cargo tree was still changing after the build returned; its closure is unknown"
                        .to_owned(),
                );
            }
            Ok(())
        }
    }
}

/// Why a build's writes leave the declared host surface, named by where the
/// census found them. The obsolete cargo target an older producer wrote into the
/// bind is its own case: that is the writer this boundary exists to end.
fn unexpected_refusal(probe: &Probe) -> String {
    let cause = match probe.unexpected() {
        "target" => "the build wrote into `/workspace/.effigy/linux-release/target`, the cargo tree an older producer kept inside the bound source",
        "tree" => "the build wrote into `/workspace/.effigy` through the source bind",
        "service" => "the build wrote something other than the declared artifact roles under `/workspace/.effigy/linux-release`",
        "link" => "the bound source holds a link or a non-directory where a directory belongs",
        "missing" => "the build wrote no declared artifact roles into the container-local tree",
        _ => "the container-local artifact roles are not the three declared files",
    };
    format!("{cause}; compiler writes through the source bind are not contained")
}

/// The container-side script one probe runs. It carries no caller input: the
/// inventory, its digest and the expected file count come from the staged
/// record, and the export name from the requested target.
fn probe_script(
    phase: ProbePhase,
    export_name: &str,
    inventory: &str,
    inventory_digest: &str,
    source_files: usize,
) -> String {
    let quote = effigy_core::shell::shell_quote;
    let mut script = String::from("set -eu\ncd /workspace\nworkspace_target=/workspace\n");
    script.push_str(&format!("inventory={}\n", quote(inventory)));
    script.push_str(&format!("inventory_digest={}\n", quote(inventory_digest)));
    script.push_str(&format!("tree={}\n", quote(BUILD_TREE_ROOT)));
    script.push_str(&format!("name={}\n", quote(export_name)));
    script.push_str(&format!(
        "expected_files={source_files}\n{}\ncap={BUILD_TREE_CAP_BYTES}\nfloor={BUILD_TREE_HEADROOM_BYTES}\n",
        CENSUS_SCRIPT
    ));
    match phase {
        ProbePhase::Before => script.push_str(BEFORE_SCRIPT),
        ProbePhase::After => script.push_str(AFTER_SCRIPT),
    }
    script
}

/// The closure census both probes run: the inventory's own digest, every listed
/// file's digest, the file count and any mount below `/workspace`.
///
/// The primitives are POSIX shell plus coreutils so the same script can be
/// replayed against a fixture directory on the host: no `stat -c`, no
/// `find -printf` and no `df -B`. The one Linux-only input is the mount table,
/// which the production container has.
const CENSUS_SCRIPT: &str = r#"if [ ! -f "$inventory" ]; then
  census=0
  source_files=0
else
  [ "$(sha256sum "$inventory" | cut -d' ' -f1)" = "$inventory_digest" ] || { echo "inventory-mismatch"; exit 42; }
  while IFS= read -r line; do
    [ -n "$line" ] || continue
    sum=$(printf '%s' "$line" | cut -d' ' -f1)
    path=$(printf '%s' "$line" | cut -d' ' -f3-)
    [ "$(sha256sum -- "$path" | cut -d' ' -f1)" = "$sum" ] || { echo "digest-mismatch $path"; exit 43; }
  done < "$inventory"
  source_files=$(find . -path ./.effigy -prune -o -path "./$inventory" -prune -o -type f -print | wc -l | tr -d ' ')
  [ "$source_files" = "$expected_files" ] || { echo "file-count $source_files"; exit 44; }
  census=1
fi
nested=$(awk -v w="$workspace_target" 'index($5, w "/") == 1 {n++} END {print n+0}' /proc/self/mountinfo)
source_bind_ro=$(awk -v w="$workspace_target" '$5 == w {
  n=split($6, a, ",")
  for (i = 1; i <= n; i++) if (a[i] == "ro") found = 1
}
END { print found + 0 }' /proc/self/mountinfo)"#;

/// Pre-build: the tree is the container's own layer, is inside its cap and has
/// headroom, the bound source carries no build output of its own, and nothing
/// has been compiled yet.
const BEFORE_SCRIPT: &str = r#"covered=$(awk -v r="$tree" '$5 != "/" && ($5 == r || substr(r, 1, length($5) + 1) == $5 "/") {n++} END {print n+0}' /proc/self/mountinfo)
[ "$covered" = 0 ] || { echo "build-tree-mount $covered"; exit 47; }
mkdir -p "$tree/target"
chmod 700 "$tree" "$tree/target"
retained=$(du -sk "$tree" | awk '{print $1 * 1024}')
reset=0
if [ "$retained" -gt "$cap" ]; then
  rm -rf -- "$tree"
  mkdir -p "$tree/target"
  chmod 700 "$tree" "$tree/target"
  retained=0
  reset=1
fi
build_output_entries=0
if [ -e ./.effigy ] || [ -L ./.effigy ]; then
  build_output_entries=$(find ./.effigy | wc -l | tr -d ' ')
fi
free=$(( $(df -P -k "$tree" | awk 'NR==2 {print $4}') * 1024 ))
echo "census $census"
echo "source_files $source_files"
echo "nested_workspace_mounts $nested"
echo "source_bind_ro $source_bind_ro"
echo "build_output_entries $build_output_entries"
echo "build_tree_mounts $covered"
echo "build_tree_retained_bytes $retained"
echo "build_tree_reset $reset"
echo "build_tree_free_bytes $free""#;

/// Post-build: the source is unchanged and still read-only, the three artifact
/// roles live in the container-local tree, no build process remains and the
/// tree is settled. A compiler write through the source bind — the obsolete
/// `linux-release/target`, any `.effigy` on a staged bind, a remount that made
/// `/workspace` writable — is named and refused rather than published.
const AFTER_SCRIPT: &str = r#"export_dir_entries=0
export_unexpected=0
export_unexpected_kind=artifacts
mark() {
  if [ "$export_unexpected" = 0 ]; then export_unexpected_kind=$1; fi
  export_unexpected=$((export_unexpected + 1))
}
if [ "$census" = 1 ]; then
  # A staged bind is the closure and nothing else: the compiler has no writable
  # host bind, so any `.effigy` here is a write that escaped containment.
  if [ -e ./.effigy ] || [ -L ./.effigy ]; then
    mark tree
  fi
fi
if [ -e ./.effigy/linux-release/target ] || [ -L ./.effigy/linux-release/target ]; then
  mark target
fi
release_dir="$tree/target/${name#effigy-}/release"
if [ ! -d "$release_dir" ] || [ -L "$release_dir" ]; then
  mark missing
  echo "missing-export release-dir"; exit 53
fi
for file in effigy effigy.receipt effigy.active-version; do
  path="$release_dir/$file"
  if [ ! -f "$path" ] || [ -L "$path" ]; then
    echo "missing-export $file"; exit 53
  fi
  export_dir_entries=$((export_dir_entries + 1))
done
binary=$(wc -c < "$release_dir/effigy" | tr -d ' ')
receipt=$(wc -c < "$release_dir/effigy.receipt" | tr -d ' ')
sidecar=$(wc -c < "$release_dir/effigy.active-version" | tr -d ' ')
retained=$(du -sk "$tree" | awk '{print $1 * 1024}')
[ "$retained" -le "$cap" ] || { echo "build-tree-over-cap $retained"; exit 54; }
writers=0
for proc in /proc/[0-9]*; do
  exe=$(readlink "$proc/exe" 2>/dev/null || true)
  case "$exe" in */cargo|*/rustc|*/rustc_driver*|*/cc|*/ld|*/ld.*) writers=1 ;; esac
  cwd=$(readlink "$proc/cwd" 2>/dev/null || true)
  case "$cwd" in "$tree"|"$tree"/*) writers=1 ;; esac
done
sleep 1
stable=$(du -sk "$tree" | awk '{print $1 * 1024}')
quiescent=0
if [ "$retained" = "$stable" ]; then quiescent=1; fi
free=$(( $(df -P -k "$tree" | awk 'NR==2 {print $4}') * 1024 ))
echo "census $census"
echo "source_files $source_files"
echo "nested_workspace_mounts $nested"
echo "source_bind_ro $source_bind_ro"
echo "export_dir_entries $export_dir_entries"
echo "export_unexpected $export_unexpected"
echo "export_unexpected_kind $export_unexpected_kind"
echo "export_binary_bytes $binary"
echo "export_receipt_bytes $receipt"
echo "export_sidecar_bytes $sidecar"
echo "build_tree_retained_bytes $retained"
echo "build_tree_open_writers $writers"
echo "build_tree_quiescent $quiescent"
echo "build_tree_free_bytes $free""#;

/// Run one probe through the verified builder and return what it observed.
fn run_probe(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    binding: &VerifiedComposeContainer,
    phase: ProbePhase,
    export_name: &str,
    staged: Option<&workspace_artifact_source::StagedSource>,
    source_files: usize,
) -> Result<Probe, RunnerError> {
    let (inventory, inventory_digest) = match staged {
        Some(staged) => (
            effigy_containers::artifact_source::STAGE_INVENTORY_FILE.to_owned(),
            staged.inventory_digest.clone(),
        ),
        None => (String::new(), String::new()),
    };
    let script = probe_script(
        phase,
        export_name,
        &inventory,
        &inventory_digest,
        source_files,
    );
    let (success, reported, diagnostics) = run_builder_script(
        repo_root,
        policy,
        binding,
        &script,
        "workspace artifact boundary probe",
    )?;
    if !success {
        // A refused probe names its cause from whichever stream carried it: the
        // script reports its own refusals on stdout, and a shell that stops
        // early reports on stderr.
        let cause = if !reported.is_empty() {
            reported.lines().next().unwrap_or_default().to_owned()
        } else if !diagnostics.is_empty() {
            diagnostics.lines().next().unwrap_or_default().to_owned()
        } else {
            "no output".to_owned()
        };
        return Err(RunnerError::task_invocation(format!(
            "workspace artifact {} boundary probe refused: {cause}",
            phase.label()
        )));
    }
    let probe = parse_probe(&reported, phase)?;
    check_probe(&probe)?;
    // A staged build compiles the closure its inventory names, so the census
    // must have run and matched. A checkout bind has no inventory to census.
    match (staged.is_some(), probe.get("census")) {
        (true, 1) => {}
        (true, _) => {
            return Err(RunnerError::task_invocation(format!(
                "the Linux builder's /workspace does not carry the {source_files} staged source files this build recorded; the {} boundary probe found no inventory to prove them; nothing was compiled",
                phase.label()
            )))
        }
        (false, 0) => {}
        (false, _) => {
            return Err(RunnerError::task_invocation(
                "the boundary probe censused an inventory a checkout source bind does not have"
                    .to_owned(),
            ))
        }
    }
    // A staged bind is the closure and nothing else: staging clears the build's
    // own output, so any `.effigy` entry already there is unattributed output from
    // another writer, not this build's starting state.
    if staged.is_some() && phase == ProbePhase::Before && probe.get("build_output_entries") != 0 {
        return Err(RunnerError::task_invocation(format!(
            "the staged source already holds {} build-output entr(ies) this build did not write; its source bind is not attributable, so nothing was compiled",
            probe.get("build_output_entries")
        )));
    }
    Ok(probe)
}

/// Execute one script inside the verified builder and return whether it
/// succeeded plus its captured stdout and stderr.
fn run_builder_script(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    binding: &VerifiedComposeContainer,
    script: &str,
    label: &str,
) -> Result<(bool, String, String), RunnerError> {
    let mut args = ["exec", "-T", "-w", "/workspace", "builder"]
        .map(OsString::from)
        .to_vec();
    args.extend(["sh".to_owned(), "-lc".to_owned(), script.to_owned()].map(OsString::from));
    let plan = effigy_runtime::container_manager::compose_invocation_plan_from_tail_args(
        repo_root,
        policy,
        args,
        ContainerAction::Exec,
        label,
    )?;
    let output = crate::runner::exec_command::run_verified_compose_exec_plan(
        policy, &plan, binding, true, None, None,
    )?;
    Ok((
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).trim().to_owned(),
        String::from_utf8_lossy(&output.stderr).trim().to_owned(),
    ))
}

/// The one record a successful build publishes about its own writers.
///
/// Every field is observed through the container's own view. `no_change` names
/// the restriction-only switch this build actually ran under, so a source that
/// does not implement the switch cannot produce this record at all. The
/// build-tree cost is stated as what it is — VM-side storage — and never as a
/// host-write count: releasing it returns filesystem space, not necessarily the
/// Colima backing file's allocation.
///
/// `scope` names the finite-before-write host surfaces: staging/render, the
/// host-capped three-role transfer, evidence, and destination publish. The
/// cargo tree remains VM-side disclosed storage, not a host-bind counter.
#[allow(clippy::too_many_arguments)]
fn boundary_marker(
    triple: &str,
    no_change: bool,
    switch_value: Option<&str>,
    staged: Option<&workspace_artifact_source::StagedSource>,
    copied_bytes: u64,
    export_released: bool,
    before: &Probe,
    after: &Probe,
) -> String {
    let release_dir = format!("{BUILD_TREE_ROOT}/target/{triple}/release");
    let marker = serde_json::json!({
        "schema": BOUNDARY_SCHEMA,
        "scope": "finite-before-write: read-only source bind, host-capped three-role transfer and local identity binding within the sidecar cap, staging/render and evidence; cargo tree is VM-side disclosed storage",
        "target": triple,
        "no_change": {
            "switch": NO_CHANGE_ENV,
            "value": switch_value,
            "restricted": no_change,
        },
        "source_bind": {
            "target": "/workspace",
            "read_only": after.get("source_bind_ro") == 1,
            "nested_mounts": after.get("nested_workspace_mounts"),
            "census": after.get("census") == 1,
            "source_files": after.get("source_files"),
            "build_output_entries_before": before.get("build_output_entries"),
        },
        "staged": {
            "mode": if staged.is_some() { "staged" } else { "checkout" },
            "files": staged.map(|staged| staged.manifest.files.len()).unwrap_or_default(),
            "bytes": staged.map(|staged| staged.bytes).unwrap_or_default(),
            "cap_bytes": STAGED_SOURCE_CAP_BYTES,
            "changed": staged.map(|staged| staged.changed).unwrap_or_default(),
        },
        "export": {
            "directory": release_dir,
            "entries": after.get("export_dir_entries"),
            "unexpected": after.get("export_unexpected"),
            "binary_bytes": after.get("export_binary_bytes"),
            "receipt_bytes": after.get("export_receipt_bytes"),
            "sidecar_bytes": after.get("export_sidecar_bytes"),
            "caps": {
                "binary": ARTIFACT_BINARY_CAP_BYTES,
                "receipt": ARTIFACT_RECEIPT_CAP_BYTES,
                "sidecar": ARTIFACT_SIDECAR_CAP_BYTES,
            },
            "released": export_released,
        },
        "transfer": {
            "mode": "host-capped",
            "bytes": copied_bytes,
        },
        "build_tree": {
            "path": BUILD_TREE_ROOT,
            "cap_bytes": BUILD_TREE_CAP_BYTES,
            "headroom_floor_bytes": BUILD_TREE_HEADROOM_BYTES,
            "retained_bytes": after.get("build_tree_retained_bytes"),
            "reset_before": before.get("build_tree_reset") == 1,
            "free_bytes_before": before.get("build_tree_free_bytes"),
            "free_bytes_after": after.get("build_tree_free_bytes"),
            "writers_closed": after.get("build_tree_open_writers") == 0,
            "quiescent": after.get("build_tree_quiescent") == 1,
            "storage": "builder-writable-layer (Colima VM disk); cleanup releases filesystem space, not backing-file allocation",
        },
    });
    serde_json::to_string(&marker).unwrap_or_else(|_| String::from("{}"))
}

/// Host-controlled transfer of the three artifact roles. The compiler never
/// chooses the destination: each role is streamed through a verified exec of
/// `cat -- <container-path>` with that role's cap enforced while receiving.
fn transfer_produced_artifact(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    binding: &VerifiedComposeContainer,
    triple: &str,
    active_version: &str,
    install_identity: &str,
) -> Result<workspace_artifact_source::ProducedArtifact, RunnerError> {
    let release = format!("{BUILD_TREE_ROOT}/target/{triple}/release");
    let binary = transfer_role(
        repo_root,
        policy,
        binding,
        &format!("{release}/effigy"),
        ARTIFACT_BINARY_CAP_BYTES,
        "binary",
    )?;
    let receipt = transfer_role(
        repo_root,
        policy,
        binding,
        &format!("{release}/effigy.receipt"),
        ARTIFACT_RECEIPT_CAP_BYTES,
        "receipt",
    )?;
    let sidecar = transfer_role(
        repo_root,
        policy,
        binding,
        &format!("{release}/effigy.active-version"),
        ARTIFACT_SIDECAR_CAP_BYTES,
        "sidecar",
    )?;
    workspace_artifact_source::publish_produced_artifact(
        repo_root,
        triple,
        active_version,
        install_identity,
        binary,
        receipt,
        sidecar,
    )
}

fn transfer_role(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    binding: &VerifiedComposeContainer,
    container_path: &str,
    cap: u64,
    role: &str,
) -> Result<Vec<u8>, RunnerError> {
    let mut args = ["exec", "-T", "builder", "cat", "--"]
        .map(OsString::from)
        .to_vec();
    args.push(OsString::from(container_path));
    let plan = effigy_runtime::container_manager::compose_invocation_plan_from_tail_args(
        repo_root,
        policy,
        args,
        ContainerAction::Exec,
        "workspace artifact transfer",
    )?;
    let exec_args = crate::runner::exec_command::verified_target_exec_args(&plan, binding.id())?;
    let runtime = effigy_runtime::container_manager::runtime_invocation_plan(
        repo_root,
        policy,
        "docker",
        &exec_args,
        ContainerAction::Exec,
        "workspace artifact transfer",
    )
    .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let origin = Instant::now();
    let mut budget =
        BoundedCommandBudget::new(origin, origin + transfer_role_deadline(), cap, 4096);
    let capture = budget
        .run_command(
            &runtime.repo_root,
            runtime.program.as_os_str(),
            &runtime.args,
            None,
            &[],
            BoundedCommandPhase::CommandExecution,
        )
        .map_err(|failure| transfer_capture_error(role, cap, failure))?;
    if !capture.status.success() {
        return Err(RunnerError::task_invocation(format!(
            "artifact transfer of `{role}` failed with status {}; original artifact retained",
            capture.status
        )));
    }
    if capture.stdout.len() as u64 > cap {
        return Err(RunnerError::task_invocation(format!(
            "artifact transfer of `{role}` is {} bytes, above its declared {cap}-byte cap; original artifact retained",
            capture.stdout.len()
        )));
    }
    Ok(capture.stdout)
}

fn transfer_role_deadline() -> Duration {
    const DEFAULT_SECS: u64 = 120;
    let secs = std::env::var("EFFIGY_TEST_ARTIFACT_TRANSFER_DEADLINE_SECS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|secs| *secs > 0)
        .unwrap_or(DEFAULT_SECS);
    Duration::from_secs(secs)
}

fn transfer_capture_error(
    role: &str,
    cap: u64,
    failure: BoundedScopedCaptureFailure,
) -> RunnerError {
    let reason = match failure.failure {
        BoundedCaptureFailure::Overflow {
            limit_bytes,
            retained_bytes,
            ..
        } => format!(
            "artifact transfer of `{role}` exceeded its {limit_bytes}-byte cap after {retained_bytes} bytes; original artifact retained"
        ),
        BoundedCaptureFailure::Deadline => format!(
            "artifact transfer of `{role}` exceeded its deadline; original artifact retained"
        ),
        BoundedCaptureFailure::Launch => {
            format!("artifact transfer of `{role}` could not be launched; original artifact retained")
        }
        BoundedCaptureFailure::ReadFailure { .. }
        | BoundedCaptureFailure::IncompleteOutput { .. } => format!(
            "artifact transfer of `{role}` was interrupted before {cap} bytes; original artifact retained"
        ),
    };
    RunnerError::task_invocation(reason)
}

fn no_change_requested(raw: Option<&str>) -> Result<bool, RunnerError> {
    match raw {
        None => Ok(false),
        Some("1") => Ok(true),
        Some(other) => Err(RunnerError::task_invocation(format!(
            "{NO_CHANGE_ENV} must be `1` when set, not `{other}`"
        ))),
    }
}

pub(in crate::runner) fn build(repo_root: &Path, command: &[String]) -> Result<(), RunnerError> {
    // The maintained task already holds its ordinary task lock. This shared
    // operation lock also serializes callers using the helper from another
    // catalog/task declaration, without trying to reacquire that outer lock.
    let _section = TaskRun::exclusive(repo_root, "shared:linux-workspace-artifact-builder", 0)?;
    let stop = effigy_rhai::install_stop_requested_flag()
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let check_interruption = || {
        if stop.load(std::sync::atomic::Ordering::Relaxed) {
            Err(RunnerError::task_invocation(
                "workspace artifact build interrupted; original admission retained",
            ))
        } else {
            Ok(())
        }
    };
    check_interruption()?;
    let mut policy = load_container_policy(repo_root, Some("linux-release"))?;
    if command.is_empty()
        || policy.primary_service != "builder"
        || policy.profile != "effigy-release"
        || !policy.shared_services.is_empty()
        || !policy.declared_ports.is_empty()
        || !policy.dns_routes.is_empty()
        || policy.dns_domain.is_some()
        || !policy.host_processes.is_empty()
    {
        return Err(RunnerError::task_invocation(
            "workspace artifact build requires the private no-route Linux builder declaration",
        ));
    }
    verify_workspace_artifact_builder_declaration(&policy)?;
    effigy_containers::validate_container_policy(repo_root, &policy)?;
    let host_binary = std::env::current_exe().map_err(RunnerError::Cwd)?;
    let install_identity = crate::runner::workspace_command::provisioning::local_artifact_identity(
        &host_binary,
        repo_root,
    )?;
    // Stage this checkout's source closure before anything renders, activates
    // or inspects: the bind the render names must already carry the bytes the
    // build will compile. A checkout inside a mount the profile shares needs no
    // staging and binds its own path, exactly as before.
    let staged = match effigy_containers::artifact_source::policy_stage(&policy)? {
        Some(plan) => {
            check_interruption()?;
            let staged = workspace_artifact_source::stage(repo_root, &policy, &plan)?;
            // The staged source is this build's evidence: it must be a record
            // whose closure was written and verified, never a pending one.
            staged
                .manifest
                .clone()
                .require_complete()
                .map_err(RunnerError::task_invocation)?;
            if staged.changed {
                eprintln!(
                    "[info] staged this checkout's source for the Linux builder at `{}`",
                    plan.source.display()
                );
            }
            Some((plan, staged))
        }
        None => None,
    };
    // Checkout identity and the external custody lease participate in render
    // labels. Establish them before taking the execution-only source snapshot;
    // this does not publish or replace the retained environment generation.
    policy = effigy_containers::policy::render::prepare_inputs(&policy)?;
    // The Compose client runs inside the VM for this backend, so its inputs must
    // be inside the mount too: the published render closure is staged beside the
    // source and every Compose plan below names those files. The render digest is
    // unchanged, so this neither publishes nor supersedes the environment.
    effigy_containers::artifact_source::stage_compose_render(&mut policy)?;
    // Validate the paths the guest will actually consume, after both source
    // and Compose staging, and still before any runtime or builder effect.
    effigy_containers::validate_compose_backend_runtime(repo_root, &policy)?;
    // The builder's profile is a managed Colima profile like any other
    // environment: create it when the user deleted it, start it when stopped
    // and restart it when its managed settings drifted. A runtime inventory
    // cannot even run against a missing profile, so this comes before the first
    // one (effigy#316).
    let switch_value = std::env::var(NO_CHANGE_ENV).ok();
    let no_change = no_change_requested(switch_value.as_deref())?;
    let runtime = if no_change {
        effigy_containers::exec::verify_runtime_backend_no_change(&policy, repo_root)
    } else {
        effigy_containers::exec::ensure_runtime_backend_running_report(&policy, repo_root)
    }
    .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    // Preserve the existing runtime drift guard; this operation grants no
    // authority to change backend, profile or the owned builder's generation.
    let mut deadline = Instant::now() + Duration::from_secs(15);
    let running = list_running_compose_containers_for_policy_with_deadline(
        repo_root,
        &policy,
        Some(deadline),
    )?;
    if let Some(message) =
        super::support::running_container_runtime_mismatch(repo_root, &policy, &running)
    {
        return Err(RunnerError::task_invocation(message));
    }
    // The build's inventory gets a fresh budget after the mismatch check.
    deadline = Instant::now() + Duration::from_secs(15);
    let mut retained = builder_retained(&policy).then_some(());
    // A profile Effigy just created or restarted discards the container the
    // record described. Recreating the builder is the repair that transition
    // owns.
    if runtime.profile_created_or_restarted {
        retained = None;
    }
    let rows = list_compose_containers_for_policy_including_stopped_with_deadline(
        repo_root,
        &policy,
        Some(deadline),
    )?;
    let binding = match workspace_artifact_builder_preparation(
        repo_root,
        &policy,
        &rows,
        deadline,
        retained.is_some(),
        no_change,
    )? {
        WorkspaceBuilderPreparation::Reuse(binding) => binding,
        WorkspaceBuilderPreparation::Activate(observed) => {
            // Mutation still requires the same exact identity and source at its
            // boundary. The pure preparation decision grants no lasting lease.
            check_interruption()?;
            if let Some(observed) = observed.as_ref() {
                confirm_transition_target(repo_root, &policy, observed)?;
            }
            check_interruption()?;
            super::lifecycle::run_container_up(
                repo_root,
                Some("linux-release"),
                super::UpSelection::Direct,
                false,
                true,
                false,
                effigy_containers::environment::Retention::Persistent,
                &[],
            )?;
            policy = load_container_policy(repo_root, Some("linux-release"))?;
            retained = builder_retained(&policy).then_some(());
            deadline = Instant::now() + Duration::from_secs(15);
            let replanned = list_compose_containers_for_policy_including_stopped_with_deadline(
                repo_root,
                &policy,
                Some(deadline),
            )?;
            let [replanned] = replanned.as_slice() else {
                return Err(RunnerError::task_invocation(
                    "workspace artifact builder inventory after activation must contain exactly one owned builder; no build attempted",
                ));
            };
            verify_workspace_artifact_builder(repo_root, &policy, replanned, deadline)?
        }
    };
    let mut args = ["exec", "-T", "-w", "/workspace", "builder"]
        .map(OsString::from)
        .to_vec();
    args.extend(command.iter().map(OsString::from));
    let plan = effigy_runtime::container_manager::compose_invocation_plan_from_tail_args(
        repo_root,
        &policy,
        args,
        ContainerAction::Exec,
        "workspace artifact build",
    )?;
    // The build holds its environment: no transition stops the builder
    // while it executes (R4).
    // A builder started outside the reconciler has no active record yet; the
    // build then runs unheld, exactly as before.
    let mut held = environment::Held::begin(repo_root, &policy, "workspace-artifact-build")?;
    if let Some(held) = held.as_mut() {
        held.attach_active()?;
    }
    if builder_retained(&policy).then_some(()) != retained {
        return Err(RunnerError::task_invocation(
            "workspace artifact builder environment changed before build; no exec attempted",
        ));
    }
    // The writer's own boundary and the source it compiles, observed through the
    // container's own view before any byte is written: the bind carries exactly
    // the recorded closure (an empty VM directory auto-created for an unreachable
    // host path, another source, a changed file or an unaccounted extra file all
    // fail by name), the cargo tree is the builder's private layer with headroom,
    // and nothing is mounted below `/workspace`.
    let triple = std::env::var("EFFIGY_LINUX_RELEASE_TRIPLE")
        .ok()
        .map(|triple| triple.trim().to_owned())
        .filter(|triple| !triple.is_empty())
        .unwrap_or_else(|| "x86_64-unknown-linux-gnu".to_owned());
    let export_name = format!("effigy-{triple}");
    let staged_source = staged.as_ref().map(|(_, staged)| staged);
    let source_files = staged_source
        .map(|staged| staged.manifest.files.len())
        .unwrap_or_default();
    check_interruption()?;
    let before = run_probe(
        repo_root,
        &policy,
        &binding,
        ProbePhase::Before,
        &export_name,
        staged_source,
        source_files,
    )?;
    check_interruption()?;
    let output = crate::runner::exec_command::run_verified_compose_exec_plan(
        &policy, &plan, &binding, false, None, None,
    )?;
    check_interruption()?;
    if !output.status.success() {
        return Err(RunnerError::task_invocation(format!(
            "linux workspace artifact build failed with status {}; builder and original admission retained for retry, and no boundary record is published",
            output.status
        )));
    }
    // Only bytes its own producer receipt blesses for this target and version
    // reach the caller, and only while the source still matches what was
    // compiled: a caller edit or a changed staged tree during a long build
    // makes the artifact describe source that is no longer the recorded one.
    if let Some((plan, staged_source)) = &staged {
        workspace_artifact_source::verify_source_unchanged(plan, staged_source)?;
        check_interruption()?;
    }
    // The source bind is still read-only, the three roles live in the
    // container-local tree within their caps, and the build's writers are
    // closed — all observed through the container's own view before any byte
    // is transferred.
    check_interruption()?;
    let after = run_probe(
        repo_root,
        &policy,
        &binding,
        ProbePhase::After,
        &export_name,
        staged_source,
        source_files,
    )?;
    check_interruption()?;
    if crate::runner::workspace_command::provisioning::local_artifact_identity(
        &host_binary,
        repo_root,
    )? != install_identity
    {
        return Err(RunnerError::task_invocation(
            "workspace artifact host identity changed during the build; no artifact published",
        ));
    }
    let produced = transfer_produced_artifact(
        repo_root,
        &policy,
        &binding,
        &triple,
        &effigy_core::build_info::active_version(),
        &install_identity,
    )?;
    let copied_bytes = produced.bytes;
    let export_released = match &staged {
        Some((plan, _)) => workspace_artifact_source::release_leftover_staged_export(plan)?,
        None => true,
    };
    check_interruption()?;
    println!(
        "{}",
        boundary_marker(
            &triple,
            no_change,
            switch_value.as_deref(),
            staged_source,
            copied_bytes,
            export_released,
            &before,
            &after,
        )
    );
    Ok(())
}

/// Repeat the inventory and identity observation the transition relies on: one
/// owned builder, with the same full ID, source and never-started state.
fn confirm_transition_target(
    repo_root: &Path,
    policy: &effigy_containers::EffectiveContainerPolicy,
    observed: &effigy_containers::exec::ObservedWorkspaceArtifactBuilder,
) -> Result<(), RunnerError> {
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut rows = list_compose_containers_for_policy_including_stopped_with_deadline(
        repo_root,
        policy,
        Some(deadline),
    )?;
    let [row] = rows.as_mut_slice() else {
        return Err(RunnerError::task_invocation(
            "workspace artifact builder inventory changed before the transition; no transition attempted",
        ));
    };
    let current = observed_workspace_artifact_builder(repo_root, policy, row, deadline)
        .map_err(RunnerError::from)?;
    if &current != observed {
        return Err(RunnerError::task_invocation(
            "workspace artifact builder identity changed before the transition; no transition attempted",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        boundary_marker, check_probe, no_change_requested, parse_probe, probe_script, ProbePhase,
        ARTIFACT_BINARY_CAP_BYTES, BUILD_TREE_CAP_BYTES, BUILD_TREE_HEADROOM_BYTES,
        BUILD_TREE_ROOT,
    };

    #[test]
    fn no_change_switch_only_narrows_and_refuses_unknown_values() {
        assert!(!no_change_requested(None).expect("absent keeps acquisition"));
        assert!(no_change_requested(Some("1")).expect("exact one"));
        for raw in ["", "0", "true", " 1", "yes"] {
            assert!(
                no_change_requested(Some(raw)).is_err(),
                "{raw:?} must refuse"
            );
        }
    }

    /// The probe's own stdout shape: every declared field once, integers only.
    fn before_output(overrides: &[(&str, u64)]) -> String {
        let mut rows = [
            ("census", 1u64),
            ("source_files", 2134),
            ("nested_workspace_mounts", 0),
            ("source_bind_ro", 1),
            ("build_output_entries", 0),
            ("build_tree_mounts", 0),
            ("build_tree_retained_bytes", 0),
            ("build_tree_reset", 0),
            ("build_tree_free_bytes", BUILD_TREE_HEADROOM_BYTES),
        ];
        for (key, value) in overrides {
            for row in rows.iter_mut() {
                if row.0 == *key {
                    row.1 = *value;
                }
            }
        }
        rows.iter()
            .map(|(key, value)| format!("{key} {value}"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn after_output(overrides: &[(&str, u64)]) -> String {
        let mut rows = [
            ("census", 1u64),
            ("source_files", 2134),
            ("nested_workspace_mounts", 0),
            ("source_bind_ro", 1),
            ("export_dir_entries", 3),
            ("export_unexpected", 0),
            ("export_unexpected_kind", 0),
            ("export_binary_bytes", 17 * 1024 * 1024),
            ("export_receipt_bytes", 220),
            ("export_sidecar_bytes", 20),
            ("build_tree_retained_bytes", 1024),
            ("build_tree_open_writers", 0),
            ("build_tree_quiescent", 1),
            ("build_tree_free_bytes", BUILD_TREE_HEADROOM_BYTES),
        ];
        for (key, value) in overrides {
            for row in rows.iter_mut() {
                if row.0 == *key {
                    row.1 = *value;
                }
            }
        }
        rows.iter()
            .map(|(key, value)| {
                if *key == "export_unexpected_kind" {
                    // A token, never a number: the census names the surface it
                    // found rather than carrying a path.
                    format!("{key} {}", if *value == 0 { "artifacts" } else { "target" })
                } else {
                    format!("{key} {value}")
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn a_complete_probe_is_accepted_in_both_phases() {
        for (phase, raw) in [
            (ProbePhase::Before, before_output(&[])),
            (ProbePhase::After, after_output(&[])),
        ] {
            let probe = parse_probe(&raw, phase).expect("complete probe");
            check_probe(&probe).expect("within the declared boundary");
        }
    }

    #[test]
    fn a_probe_refuses_missing_unknown_duplicate_or_non_integer_fields() {
        let complete = before_output(&[]);
        let missing = complete
            .lines()
            .filter(|line| !line.starts_with("build_tree_mounts "))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(parse_probe(&missing, ProbePhase::Before).is_err());
        let unknown = format!("{complete}\nbuild_tree_extra 1");
        assert!(parse_probe(&unknown, ProbePhase::Before).is_err());
        let duplicate = format!("{complete}\nbuild_tree_mounts 0");
        assert!(parse_probe(&duplicate, ProbePhase::Before).is_err());
        let negative = complete.replace("build_tree_mounts 0", "build_tree_mounts -1");
        assert!(parse_probe(&negative, ProbePhase::Before).is_err());
        let textual = complete.replace("build_tree_mounts 0", "build_tree_mounts yes");
        assert!(parse_probe(&textual, ProbePhase::Before).is_err());
        // The before-phase key set never satisfies the after phase.
        assert!(parse_probe(&complete, ProbePhase::After).is_err());
    }

    #[test]
    fn the_after_census_names_every_path_outside_the_declared_export() {
        for kind in ["target", "tree", "service", "entry", "link", "missing"] {
            let raw = after_output(&[("export_unexpected", 1), ("export_unexpected_kind", 1)])
                .replace(
                    "export_unexpected_kind target",
                    &format!("export_unexpected_kind {kind}"),
                );
            let probe = parse_probe(&raw, ProbePhase::After).expect("parsed probe");
            let refusal = check_probe(&probe).expect_err("must refuse").to_string();
            assert!(refusal.contains("not contained"), "{kind}: {refusal}");
            if kind != "entry" {
                // Every surface the census can find has its own naming; only an
                // entry inside the export directory is the artifact-shape case.
                assert!(
                    !refusal.contains("artifact-shaped"),
                    "{kind}: the refusal names the surface: {refusal}"
                );
            }
        }
        // The obsolete cargo tree inside the bind is its own case by name.
        let probe = parse_probe(
            &after_output(&[("export_unexpected", 1), ("export_unexpected_kind", 1)]),
            ProbePhase::After,
        )
        .expect("parsed probe");
        let refusal = check_probe(&probe).expect_err("must refuse").to_string();
        assert!(refusal.contains("linux-release/target"), "{refusal}");
        // An unknown kind token is never accepted.
        let raw = after_output(&[]).replace(
            "export_unexpected_kind artifacts",
            "export_unexpected_kind else",
        );
        assert!(parse_probe(&raw, ProbePhase::After).is_err());
    }

    #[test]
    fn every_unbounded_writer_refuses_at_its_declared_boundary() {
        let cases = [
            ("nested_workspace_mounts", 1u64),
            ("source_bind_ro", 0),
            ("build_tree_free_bytes", BUILD_TREE_HEADROOM_BYTES - 1),
        ];
        for (key, value) in cases {
            let probe = parse_probe(&before_output(&[(key, value)]), ProbePhase::Before)
                .expect("parsed probe");
            assert!(
                check_probe(&probe).is_err(),
                "{key}={value} must leave the boundary"
            );
        }
        let cases = [
            ("nested_workspace_mounts", 1u64),
            ("source_bind_ro", 0),
            ("export_unexpected", 1),
            ("export_binary_bytes", ARTIFACT_BINARY_CAP_BYTES + 1),
            ("export_receipt_bytes", 65537),
            ("export_sidecar_bytes", 4097),
            ("export_binary_bytes", 0),
            ("build_tree_retained_bytes", BUILD_TREE_CAP_BYTES + 1),
            ("build_tree_open_writers", 1),
            ("build_tree_quiescent", 0),
            ("build_tree_free_bytes", BUILD_TREE_HEADROOM_BYTES - 1),
        ];
        for (key, value) in cases {
            let probe =
                parse_probe(&after_output(&[(key, value)]), ProbePhase::After).expect("parsed");
            assert!(
                check_probe(&probe).is_err(),
                "{key}={value} must leave the boundary"
            );
        }
        // A build tree covered by a mount is not the container's own layer.
        let probe = parse_probe(
            &before_output(&[("build_tree_mounts", 1)]),
            ProbePhase::Before,
        )
        .expect("parsed");
        assert!(check_probe(&probe).is_err());
    }

    /// The scripts are pure text over the constants, and the export name is the
    /// only value that comes from the requested target.
    #[test]
    fn the_probe_scripts_census_the_whole_build_service_tree() {
        // A staged bind's `.effigy` is a write through the source bind: the
        // after-probe names it, and the obsolete cargo tree path is its own case.
        let after = probe_script(
            ProbePhase::After,
            "effigy-aarch64-unknown-linux-gnu",
            ".effigy-builder-source.inventory",
            "b".repeat(64).as_str(),
            2134,
        );
        assert!(after.contains("./.effigy/linux-release/target"));
        assert!(after.contains("source_bind_ro"));
        assert!(after.contains("export_unexpected_kind"));
        assert!(after.contains("export_unexpected=$((export_unexpected + 1))"));
        assert!(after.contains(BUILD_TREE_ROOT));
        assert!(after.contains("release_dir=\"$tree/target/"));
        // The before-probe reports pre-existing build output instead of assuming
        // the bind starts clean, and requires the source bind to be read-only.
        let before = probe_script(
            ProbePhase::Before,
            "effigy-aarch64-unknown-linux-gnu",
            ".effigy-builder-source.inventory",
            "b".repeat(64).as_str(),
            2134,
        );
        assert!(before.contains("build_output_entries="));
        assert!(before.contains("find ./.effigy"));
        assert!(before.contains("source_bind_ro"));
    }

    #[test]
    fn probes_name_the_declared_boundary_and_carry_no_caller_input() {
        for phase in [ProbePhase::Before, ProbePhase::After] {
            let script = probe_script(
                phase,
                "effigy-aarch64-unknown-linux-gnu",
                ".effigy-builder-source.inventory",
                "a".repeat(64).as_str(),
                2134,
            );
            assert!(script.contains(BUILD_TREE_ROOT), "{}", phase.label());
            assert!(script.contains(&BUILD_TREE_CAP_BYTES.to_string()));
            assert!(script.contains(&BUILD_TREE_HEADROOM_BYTES.to_string()));
            assert!(script.contains("effigy-aarch64-unknown-linux-gnu"));
            assert!(script.contains("expected_files=2134"));
            assert!(script.contains("source_bind_ro"));
            match phase {
                ProbePhase::Before => assert!(!script.contains("export_binary_bytes")),
                ProbePhase::After => {
                    assert!(script.contains("export_binary_bytes"));
                    assert!(script.contains("build_tree_open_writers"));
                    assert!(script.contains("./.effigy/linux-release/target"));
                    assert!(script.contains("release_dir=\"$tree/target/"));
                }
            }
        }
    }

    /// The published record states the writers, their caps and the VM-side cost
    /// of the build tree without claiming it as a host write count.
    #[test]
    fn the_boundary_record_names_every_writer_and_its_cap() {
        let before = parse_probe(&before_output(&[]), ProbePhase::Before).expect("before");
        let after = parse_probe(&after_output(&[]), ProbePhase::After).expect("after");
        let marker: serde_json::Value = serde_json::from_str(&boundary_marker(
            "aarch64-unknown-linux-gnu",
            true,
            Some("1"),
            None,
            4096,
            true,
            &before,
            &after,
        ))
        .expect("record parses");
        assert_eq!(marker["schema"], "effigy.builder-write-boundary.v1");
        assert!(
            marker["scope"]
                .as_str()
                .is_some_and(|text| text.contains("finite-before-write")),
            "{marker}"
        );
        assert_eq!(marker["source_bind"]["read_only"], true);
        assert_eq!(
            marker["no_change"]["switch"],
            "EFFIGY_LINUX_RELEASE_REQUIRE_RUNNING"
        );
        assert_eq!(marker["no_change"]["restricted"], true);
        assert_eq!(marker["source_bind"]["nested_mounts"], 0);
        assert_eq!(marker["source_bind"]["source_files"], 2134);
        assert_eq!(marker["source_bind"]["build_output_entries_before"], 0);
        assert_eq!(marker["build_tree"]["path"], BUILD_TREE_ROOT);
        assert_eq!(marker["build_tree"]["cap_bytes"], BUILD_TREE_CAP_BYTES);
        assert_eq!(marker["build_tree"]["writers_closed"], true);
        assert_eq!(
            marker["export"]["caps"]["binary"],
            ARTIFACT_BINARY_CAP_BYTES
        );
        assert_eq!(marker["export"]["released"], true);
        assert_eq!(
            marker["export"]["directory"],
            "/var/tmp/effigy-artifact-build/target/aarch64-unknown-linux-gnu/release"
        );
        assert_eq!(marker["transfer"]["bytes"], 4096);
        assert_eq!(marker["transfer"]["mode"], "host-capped");
        assert_eq!(marker["staged"]["mode"], "checkout");
        assert!(marker["build_tree"]["storage"]
            .as_str()
            .is_some_and(|text| text.contains("VM disk")),);
    }
}
