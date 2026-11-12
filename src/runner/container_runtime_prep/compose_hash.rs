//! Reading Compose's own service config hashes for one frozen render, and
//! projecting the private preflight tree's paths back to the canonical apply
//! directory so the observation is comparable.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Instant;

use effigy_containers::EffectiveContainerPolicy;

use crate::runner::error::RunnerError;

/// One `compose config --hash` reads a small map: service names and hashes.
const COMPOSE_HASH_STDOUT_BYTES: u64 = 64 * 1024;
const COMPOSE_HASH_STDERR_BYTES: u64 = 16 * 1024;

/// The service -> config-hash map one rendered policy produces.
///
/// `compose config --hash` is the backend's own answer, computed from the same
/// frozen closure the apply uses. A backend without it (or an unparsable answer)
/// refuses: an unsupported comparison is never treated as compatible.
pub(super) fn compose_service_config_hashes(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    deadline: Instant,
) -> Result<std::collections::BTreeMap<String, String>, RunnerError> {
    parse_service_config_hashes(&compose_config_output(
        repo_root,
        policy,
        &["config", "--hash", "*"],
        deadline,
    )?)
}

/// A loader may read env/label files or another Compose file before it returns
/// the expanded model. Only literal dependencies in the verified closure may
/// participate; unknown path interpolation/include contexts fail closed.
pub(super) fn verify_frozen_compose_reads(
    files: &std::collections::BTreeMap<String, Vec<u8>>,
    names: &[String],
) -> Result<(), RunnerError> {
    fn dependency(base: &Path, value: &serde_yaml::Value) -> Result<String, RunnerError> {
        let refuse = || {
            RunnerError::task_invocation(
                "Compose file dependency is not a literal path inside the frozen closure",
            )
        };
        let text = value.as_str().ok_or_else(refuse)?;
        if text.contains('$') {
            return Err(refuse());
        }
        let mut name = base.to_path_buf();
        for component in Path::new(text).components() {
            match component {
                std::path::Component::Normal(value) => name.push(value),
                std::path::Component::CurDir => {}
                std::path::Component::ParentDir => {
                    if !name.pop() {
                        return Err(refuse());
                    }
                }
                _ => return Err(refuse()),
            }
        }
        name.to_str().map(str::to_owned).ok_or_else(refuse)
    }
    let refuse = || {
        RunnerError::task_invocation("Compose file dependency is absent from the frozen closure")
    };
    let directory = names
        .first()
        .and_then(|name| Path::new(name).parent())
        .ok_or_else(refuse)?;
    let mut pending = names
        .iter()
        .map(|name| (name.clone(), directory.to_path_buf()))
        .collect::<Vec<_>>();
    let mut seen = std::collections::BTreeSet::new();
    while let Some((name, base)) = pending.pop() {
        if !seen.insert((name.clone(), base.clone())) {
            continue;
        }
        let document: serde_yaml::Value =
            serde_yaml::from_slice(files.get(&name).ok_or_else(refuse)?).map_err(|_| refuse())?;
        if document.get("include").is_some() {
            return Err(RunnerError::task_invocation(
                "Compose include context is unsupported for frozen preflight expansion",
            ));
        }
        let Some(services) = document
            .get("services")
            .and_then(serde_yaml::Value::as_mapping)
        else {
            continue;
        };
        for service in services.values() {
            for key in ["env_file", "label_file"] {
                if let Some(value) = service.get(key) {
                    let values = value
                        .as_sequence()
                        .map(|values| values.as_slice())
                        .unwrap_or(std::slice::from_ref(value));
                    for value in values {
                        let value = value.get("path").unwrap_or(value);
                        let name = dependency(&base, value)?;
                        if !files.contains_key(&name) {
                            return Err(refuse());
                        }
                    }
                }
            }
            if let Some(value) = service.get("extends").and_then(|value| value.get("file")) {
                let name = dependency(&base, value)?;
                let directory = Path::new(&name).parent().ok_or_else(refuse)?.to_path_buf();
                pending.push((name, directory));
            }
        }
    }
    Ok(())
}

/// Read the ordered input inventory from the same frozen closure the publisher
/// consumes. Private -f files must use the eventual apply's project directory:
/// identical YAML at another root otherwise resolves different bind/build paths.
pub(super) fn compose_config_args(
    policy: &EffectiveContainerPolicy,
    tail: &[&str],
) -> Result<(Vec<OsString>, Option<PathBuf>), RunnerError> {
    let refuse = || {
        RunnerError::task_invocation(
            "workspace Compose hash has no exact frozen apply input inventory",
        )
    };
    let render = policy.render.as_ref().ok_or_else(refuse)?;
    let metadata: serde_json::Value = serde_json::from_slice(
        render
            .closure
            .files()
            .get("inputs/environment.json")
            .ok_or_else(refuse)?,
    )
    .map_err(|_| refuse())?;
    let names: Vec<String> =
        serde_json::from_value(metadata.get("compose_files").cloned().ok_or_else(refuse)?)
            .map_err(|_| refuse())?;
    let first_name = names.first().ok_or_else(refuse)?;
    let first = policy.compose_files.first().ok_or_else(refuse)?;
    let read_root = first
        .ancestors()
        .nth(Path::new(first_name).components().count())
        .ok_or_else(refuse)?;
    if names.len() != policy.compose_files.len()
        || names.iter().zip(&policy.compose_files).any(|(name, file)| {
            !render.closure.files().contains_key(name) || *file != read_root.join(name)
        })
    {
        return Err(refuse());
    }
    verify_frozen_compose_reads(render.closure.files(), &names)?;
    let canonical_first = render.path.join(first_name);
    let project_directory = canonical_first.parent().ok_or_else(refuse)?;
    let needs_expansion = first.parent() != Some(project_directory);
    // Both the final hash model and the real apply use this explicit directory.
    let mut args = vec![
        OsString::from("compose"),
        OsString::from("--project-directory"),
        project_directory.as_os_str().to_owned(),
    ];
    if needs_expansion {
        args.extend([OsString::from("-f"), OsString::from("-")]);
    } else {
        for file in &policy.compose_files {
            args.extend([OsString::from("-f"), file.as_os_str().to_owned()]);
        }
    }
    args.extend([OsString::from("-p"), OsString::from(&policy.project_name)]);
    args.extend(tail.iter().map(OsString::from));
    Ok((args, needs_expansion.then(|| read_root.to_path_buf())))
}

/// Project only Compose filesystem path fields. An unexpected temporary-root
/// value (including an environment value) refuses instead of being silently
/// rewritten as though it were another path field.
pub(super) fn canonical_compose_model(
    bytes: &[u8],
    private: &Path,
    canonical: &Path,
) -> Result<Vec<u8>, RunnerError> {
    fn path(value: &mut serde_json::Value, private: &Path, canonical: &Path) {
        let next = value
            .as_str()
            .and_then(|value| Path::new(value).strip_prefix(private).ok())
            .map(|relative| canonical.join(relative).to_string_lossy().into_owned());
        if let Some(next) = next {
            *value = next.into();
        }
    }
    fn field(value: &mut serde_json::Value, key: &str, private: &Path, canonical: &Path) {
        if let Some(value) = value.get_mut(key) {
            path(value, private, canonical);
        }
    }
    fn contains_private(value: &serde_json::Value, root: &str) -> bool {
        match value {
            serde_json::Value::String(value) => value.contains(root),
            serde_json::Value::Array(values) => {
                values.iter().any(|value| contains_private(value, root))
            }
            serde_json::Value::Object(values) => values
                .iter()
                .any(|(key, value)| key.contains(root) || contains_private(value, root)),
            _ => false,
        }
    }
    let mut model: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|_| RunnerError::task_invocation("Compose returned no valid expanded model"))?;
    let services = model
        .get_mut("services")
        .and_then(serde_json::Value::as_object_mut)
        .ok_or_else(|| RunnerError::task_invocation("Compose returned no expanded services"))?;
    for service in services.values_mut() {
        if let Some(build) = service.get_mut("build") {
            field(build, "context", private, canonical);
            field(build, "dockerfile", private, canonical);
            if let Some(contexts) = build
                .get_mut("additional_contexts")
                .and_then(serde_json::Value::as_object_mut)
            {
                for context in contexts.values_mut() {
                    path(context, private, canonical);
                }
            }
        }
        if let Some(volumes) = service
            .get_mut("volumes")
            .and_then(serde_json::Value::as_array_mut)
        {
            for volume in volumes {
                if volume.get("type").and_then(serde_json::Value::as_str) == Some("bind") {
                    field(volume, "source", private, canonical);
                }
            }
        }
        if let Some(watch) = service
            .get_mut("develop")
            .and_then(|value| value.get_mut("watch"))
            .and_then(serde_json::Value::as_array_mut)
        {
            for item in watch {
                field(item, "path", private, canonical);
            }
        }
    }
    for section in ["configs", "secrets"] {
        if let Some(values) = model
            .get_mut(section)
            .and_then(serde_json::Value::as_object_mut)
        {
            for value in values.values_mut() {
                field(value, "file", private, canonical);
            }
        }
    }
    if contains_private(&model, &private.to_string_lossy()) {
        return Err(RunnerError::task_invocation("Compose model has an unsupported temporary-root reference; canonical apply equivalence is unproven"));
    }
    serde_json::to_vec(&model).map_err(|error| RunnerError::task_invocation(error.to_string()))
}

pub(super) fn compose_config_output(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    tail: &[&str],
    deadline: Instant,
) -> Result<Vec<u8>, RunnerError> {
    let (args, read_root) = compose_config_args(policy, tail)?;
    let mut stdout_retained = 0usize;
    let mut stderr_retained = 0usize;
    let mut capture = |args: Vec<OsString>, input: Option<&[u8]>| -> Result<Vec<u8>, RunnerError> {
        let plan = effigy_runtime::container_manager::compose_invocation_plan_from_args(
            repo_root,
            policy,
            args,
            effigy_containers::ContainerAction::Status,
            "workspace Compose config observation",
        )
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
        let program = plan.program.to_str().ok_or_else(|| {
            RunnerError::task_invocation("Compose observation program is not UTF-8")
        })?;
        // This existing owner supplies bounded in-memory stdin, a hard pipe cap
        // and process-group cleanup. No temporary model file can be published.
        let captured = effigy_containers::exec::run_input_capture_until(
            &plan.repo_root,
            program,
            &plan.args,
            input.unwrap_or_default(),
            deadline,
        )
        .map_err(|_| {
            RunnerError::task_invocation(
                "Compose config observation was unavailable within its deadline",
            )
        })?;
        stdout_retained += captured.stdout.len();
        stderr_retained += captured.stderr.len();
        if Instant::now() >= deadline {
            return Err(RunnerError::task_invocation(
                "Compose config observation deadline expired",
            ));
        }
        // The owner hard-bounds each pipe; the observation additionally refuses
        // aggregate output above the shared budget before using either result.
        if stdout_retained > COMPOSE_HASH_STDOUT_BYTES as usize
            || stderr_retained > COMPOSE_HASH_STDERR_BYTES as usize
        {
            return Err(RunnerError::task_invocation(
                "Compose config observation exceeded its cumulative capture budget",
            ));
        }
        if !captured.status.success() {
            return Err(RunnerError::task_invocation(format!(
                "Compose config observation failed: {}",
                String::from_utf8_lossy(&captured.stderr).trim(),
            )));
        }
        Ok(captured.stdout)
    };
    // Compose resolves env_file/extends against the verified private tree.
    // Project its absolute path fields to the canonical root in memory, then
    // let Compose load/hash that model at the exact apply directory. The model
    // is only an observation: it is never sealed, published or passed to apply.
    let expanded = if let Some(read_root) = read_root {
        let directory = policy
            .compose_files
            .first()
            .and_then(|file| file.parent())
            .ok_or_else(|| {
                RunnerError::task_invocation("private Compose input has no project directory")
            })?;
        let mut expansion =
            effigy_containers::compose::compose_args(policy, ["config", "--format", "json"]);
        expansion.splice(
            1..1,
            [
                OsString::from("--project-directory"),
                directory.as_os_str().to_owned(),
            ],
        );
        let model = capture(expansion, None)?;
        let canonical_root = &policy
            .render
            .as_ref()
            .ok_or_else(|| RunnerError::task_invocation("Compose model has no frozen render"))?
            .path;
        Some(canonical_compose_model(&model, &read_root, canonical_root)?)
    } else {
        None
    };
    capture(args, expanded.as_deref())
}

/// Parse `compose config --hash` output: one `service hash` line per service.
pub(super) fn parse_service_config_hashes(
    stdout: &[u8],
) -> Result<std::collections::BTreeMap<String, String>, RunnerError> {
    let text = String::from_utf8_lossy(stdout);
    let mut hashes = std::collections::BTreeMap::new();
    for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let Some((service, hash)) = line.split_once(char::is_whitespace) else {
            return Err(RunnerError::task_invocation(
                "compose config --hash printed no service hash pair",
            ));
        };
        let hash = hash.trim();
        if service.is_empty() || hash.is_empty() {
            return Err(RunnerError::task_invocation(
                "compose config --hash printed an empty service or hash",
            ));
        }
        hashes.insert(service.to_owned(), hash.to_owned());
    }
    if hashes.is_empty() {
        return Err(RunnerError::task_invocation(
            "compose config --hash reported no services",
        ));
    }
    Ok(hashes)
}
