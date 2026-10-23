//! Lost-checkout discovery uses only the selected backend and retained shared
//! claims. Local records preserve retry work, never establish ownership.
use super::*;
use effigy_containers::{
    CHECKOUT_FENCE_LABEL, CHECKOUT_GENERATION_LABEL, CHECKOUT_LABEL, MANAGED_LABEL, PERSIST_LABEL,
    REPO_ROOT_LABEL,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path};

const LIMIT: usize = 1000;
#[derive(Debug, Clone)]
struct Node {
    kind: ObservedKind,
    identity: String,
    labels: BTreeMap<String, String>,
}
fn invalid(message: impl std::fmt::Display) -> RunnerError {
    RunnerError::task_invocation(format!("dead scope inventory: {message}"))
}
fn executable() -> Result<effigy_kernel::supervise::Executable, RunnerError> {
    static CURRENT: std::sync::OnceLock<Result<effigy_kernel::supervise::Executable, String>> =
        std::sync::OnceLock::new();
    CURRENT
        .get_or_init(|| effigy_kernel::supervise::Executable::current().map_err(|e| e.to_string()))
        .clone()
        .map_err(invalid)
}
fn environment() -> BTreeMap<std::ffi::OsString, effigy_kernel::custody::EnvValue> {
    std::env::vars_os()
        .filter(|(key, _)| {
            !matches!(
                key.to_str(),
                Some("EFFIGY_CUSTODY_NONCE" | "EFFIGY_PROBE_NONCE" | "EFFIGY_PARENT_CAPABILITY")
            )
        })
        .map(|(key, value)| (key, effigy_kernel::custody::EnvValue::Plain(value)))
        .collect()
}
fn resolved_program(program: &std::ffi::OsStr) -> Result<std::ffi::OsString, RunnerError> {
    let name = program
        .to_str()
        .ok_or_else(|| invalid("backend executable is not UTF-8"))?;
    let resolved = effigy_containers::compose::resolve_host_cli_program(name);
    if !Path::new(&resolved).is_absolute() {
        return Err(invalid("backend executable is unavailable on PATH"));
    }
    Ok(resolved)
}
fn operation(class: effigy_kernel::ctx::EffectClass) -> effigy_kernel::ctx::OperationContext {
    use effigy_kernel::ctx::*;
    OperationContext::new(
        Deadline::At(std::time::Instant::now() + std::time::Duration::from_secs(30)),
        CancelToken::default(),
        Limits {
            capture_tail: 16 << 20,
            log_rotate: 1 << 20,
            log_keep: 1,
            line_max: 65536,
            queue_bytes: 16 << 20,
            probe_concurrency: 2,
        },
        class,
        if effigy_core::container_detection::process_is_inside_effigy_workspace_container() {
            Placement::Guest
        } else {
            Placement::Host
        },
    )
}
fn capture(root: &Path, profile: &str, args: Vec<String>) -> Result<Vec<u8>, RunnerError> {
    use effigy_kernel::observe::Observed;
    use effigy_kernel::probe::ProbeCommand;
    let op = operation(effigy_kernel::ctx::EffectClass::Observe);
    if op.placement() != effigy_kernel::ctx::Placement::Host {
        return Err(invalid("host authority required"));
    }
    let plan = super::super::support::runtime_volume_plan(
        root,
        profile,
        &DockerCommand {
            program: "docker".into(),
            args,
            description: "inventory owned backend resources".into(),
        },
    )?;
    let runtime = crate::runner::task_engine::probe_runtime()
        .ok_or_else(|| invalid("probe custody is unavailable"))?;
    let cx = op
        .observe()
        .map_err(|_| invalid("observe context"))?
        .with_probes(runtime);
    let command = ProbeCommand {
        argv: std::iter::once(resolved_program(&plan.program)?)
            .chain(plan.args)
            .collect(),
        cwd: root.into(),
        env: environment(),
        deadline: std::time::Duration::from_secs(30),
        max_output: 16 << 20,
    };
    match cx.probe_process(command) {
        Observed::Verified(output, _)
            if output.exit.code == 0 && !output.truncated && output.warnings.is_empty() =>
        {
            Ok(output.output)
        }
        Observed::Verified(output, _) => Err(invalid(format!(
            "selected backend response is incomplete (exit {}, truncated {}, warnings {})",
            output.exit.code,
            output.truncated,
            output.warnings.len()
        ))),
        Observed::Unknown(reason) => {
            Err(invalid(format!("selected backend unavailable: {reason:?}")))
        }
        Observed::Absent(_) => Err(invalid("selected backend observation absent")),
    }
}
fn remove(
    run: &mut effigy_kernel::run::Run<'_>,
    root: &Path,
    profile: &str,
    resource: &ObservedResource,
) -> Result<bool, RunnerError> {
    use effigy_kernel::custody::{Custody, Lifetime, ProcessSpec, Terminal};
    use effigy_kernel::observe::Observed;
    let args = match resource.kind {
        ObservedKind::Container => vec!["rm".into(), "-f".into(), resource.name.clone()],
        ObservedKind::Network => vec!["network".into(), "rm".into(), resource.name.clone()],
        ObservedKind::Volume => vec!["volume".into(), "rm".into(), resource.name.clone()],
        _ => return Err(invalid("not a backend deletion")),
    };
    let plan = super::super::support::runtime_volume_plan(
        root,
        profile,
        &DockerCommand {
            program: "docker".into(),
            args,
            description: "retire exact owned backend resource".into(),
        },
    )?;
    let op = operation(effigy_kernel::ctx::EffectClass::Mutate);
    let cx = op.effects().map_err(|_| invalid("mutation context"))?;
    let spec = ProcessSpec {
        argv: std::iter::once(resolved_program(&plan.program)?)
            .chain(plan.args)
            .collect(),
        cwd: root.into(),
        env: environment(),
        grace: std::time::Duration::from_millis(100),
        lifetime: Lifetime::Finite(std::time::Duration::from_secs(30)),
        terminal: Terminal::None,
        label: "retire owned resource".into(),
    };
    let mut process = Custody::spawn(run, spec, &cx).map_err(|e| invalid(format!("{e:?}")))?;
    if run.phase().map_err(invalid)? != effigy_kernel::run::Phase::Active {
        run.advance(effigy_kernel::run::Phase::Active)
            .map_err(invalid)?;
    }
    match process.wait(&op) {
        Observed::Verified(exit, _) => {
            let success = exit.code == 0;
            process.complete(run, exit).map_err(invalid)?;
            Ok(success)
        }
        other => Err(invalid(format!("deletion outcome is unknown: {other:?}"))),
    }
}
fn nodes(root: &Path, profile: &str) -> Result<Vec<Node>, RunnerError> {
    let mut result = Vec::new();
    for (kind, list, inspect) in [
        (
            ObservedKind::Container,
            vec!["ps", "-a", "-q"],
            vec!["inspect"],
        ),
        (
            ObservedKind::Network,
            vec!["network", "ls", "-q"],
            vec!["network", "inspect"],
        ),
        (
            ObservedKind::Volume,
            vec!["volume", "ls", "-q"],
            vec!["volume", "inspect"],
        ),
    ] {
        let mut args = list.into_iter().map(str::to_owned).collect::<Vec<_>>();
        args.extend(["--filter".into(), format!("label={MANAGED_LABEL}=true")]);
        let listed = capture(root, profile, args)?;
        let names = parse_listed_resource_names(std::str::from_utf8(&listed).map_err(invalid)?)
            .map_err(invalid)?;
        if names.len() > LIMIT {
            return Err(invalid("backend resource count exceeds bound"));
        }
        for chunk in names.chunks(64) {
            let mut args = inspect.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
            args.extend_from_slice(chunk);
            let bytes = capture(root, profile, args)?;
            let values: Vec<serde_json::Value> = serde_json::from_slice(&bytes).map_err(invalid)?;
            if values.len() != chunk.len() {
                return Err(invalid("inspect did not return every requested resource"));
            }
            let mut matched = BTreeSet::new();
            for value in values {
                let labels = value
                    .pointer("/Config/Labels")
                    .or_else(|| value.get("Labels"))
                    .ok_or_else(|| invalid("inspect labels missing"))?;
                let labels: BTreeMap<String, String> =
                    serde_json::from_value(labels.clone()).map_err(invalid)?;
                let labels = labels
                    .into_iter()
                    .filter(|(key, _)| {
                        matches!(
                            key.as_str(),
                            CHECKOUT_LABEL
                                | REPO_ROOT_LABEL
                                | SCOPE_LABEL
                                | PROJECT_LABEL
                                | COMPOSE_PROJECT_LABEL
                                | CHECKOUT_FENCE_LABEL
                                | CHECKOUT_GENERATION_LABEL
                                | MANAGED_LABEL
                                | PERSIST_LABEL
                                | "com.docker.compose.external"
                        )
                    })
                    .collect::<BTreeMap<_, _>>();
                if labels.values().any(|value| value.len() > 4096) {
                    return Err(invalid("ownership label exceeds bound"));
                }
                let identity = value
                    .get(if kind == ObservedKind::Volume {
                        "Name"
                    } else {
                        "Id"
                    })
                    .and_then(serde_json::Value::as_str)
                    .filter(|s| !s.is_empty())
                    .ok_or_else(|| invalid("inspect identity missing"))?
                    .to_owned();
                if identity.len() > 256 {
                    return Err(invalid("inspect identity exceeds bound"));
                }
                let requested = chunk
                    .iter()
                    .filter(|name| {
                        **name == identity
                            || (kind != ObservedKind::Volume
                                && name.len() >= 12
                                && identity.starts_with(name.as_str()))
                    })
                    .collect::<Vec<_>>();
                if requested.len() != 1 || !matched.insert(requested[0].clone()) {
                    return Err(invalid("inspect identity does not match its request"));
                }
                result.push(Node {
                    kind,
                    identity,
                    labels,
                });
            }
        }
    }
    Ok(result)
}
fn scope_token(node: &Node) -> Option<&str> {
    if node.labels.get(MANAGED_LABEL).map(String::as_str) != Some("true") {
        return None;
    }
    let token = node.labels.get(SCOPE_LABEL)?;
    (token.len() == 32
        && token
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
    .then_some(token.as_str())
}
fn scope(node: &Node) -> Option<(&str, &str)> {
    if node.labels.get(MANAGED_LABEL).map(String::as_str) != Some("true") {
        return None;
    }
    let token = node.labels.get(SCOPE_LABEL)?;
    if token.len() != 32
        || !token
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return None;
    }
    let project = node
        .labels
        .get(PROJECT_LABEL)
        .or_else(|| node.labels.get(COMPOSE_PROJECT_LABEL))?;
    if !effigy_core::worktree_scope::PROJECT_TAGS
        .iter()
        .any(|tag| project.contains(&format!("-{tag}-{}", &token[..12])))
    {
        return None;
    }
    Some((token, project))
}
fn path(label: &str) -> Option<&Path> {
    let p = Path::new(label);
    (p.is_absolute()
        && p.components()
            .all(|part| matches!(part, Component::RootDir | Component::Normal(_))))
    .then_some(p)
}
fn dead_generation(
    checkout: &Path,
    expected: effigy_kernel::fs::DirectoryIdentity,
    token: &str,
) -> Result<bool, String> {
    use effigy_kernel::fs::DirFd;
    match std::fs::symlink_metadata(checkout) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
            let actual = DirFd::open(checkout)
                .and_then(|dir| dir.identity())
                .map_err(|e| e.to_string())?;
            if actual == expected {
                return Ok(false);
            }
            match effigy_core::worktree_scope::checkout_token(checkout)
                .map_err(|e| e.to_string())?
            {
                Some(current) if current != token => Ok(true),
                _ => Err("checkout path changed without a verified new generation".into()),
            }
        }
        Ok(_) => Err("checkout path is not a trusted directory".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let mut parent = checkout.parent();
            while let Some(path) = parent {
                match std::fs::symlink_metadata(path) {
                    Ok(_) => {
                        return DirFd::open(path)
                            .and_then(|dir| dir.identity())
                            .map_err(|e| e.to_string())
                            .and_then(|identity| {
                                if identity.device == expected.device {
                                    Ok(true)
                                } else {
                                    Err("checkout volume is unavailable or changed".into())
                                }
                            })
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        parent = path.parent()
                    }
                    Err(error) => return Err(error.to_string()),
                }
            }
            Err("checkout volume is unknown".into())
        }
        Err(error) => Err(error.to_string()),
    }
}
fn generation(node: &Node) -> Option<effigy_kernel::fs::DirectoryIdentity> {
    node.labels
        .get(CHECKOUT_GENERATION_LABEL)
        .filter(|text| text.len() < 256)
        .and_then(|text| serde_json::from_str(text).ok())
}
fn discover(
    root: &Path,
    profile: &str,
    inventory: &[Node],
) -> Result<(Vec<ScopeRecord>, Vec<String>), RunnerError> {
    let mut records = BTreeMap::new();
    let mut paths: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut fences: BTreeMap<String, Vec<effigy_kernel::gate::CheckoutFenceIdentity>> =
        BTreeMap::new();
    let mut unprotected = BTreeSet::new();
    let mut generations: BTreeMap<String, Vec<effigy_kernel::fs::DirectoryIdentity>> =
        BTreeMap::new();
    let mut unknown = Vec::new();
    for node in inventory {
        let Some(token) = scope_token(node) else {
            continue;
        };
        if scope(node).is_none() {
            unprotected.insert(token.to_owned());
            unknown.push(format!("{token}: project ownership is ambiguous"));
            continue;
        }
        match generation(node) {
            Some(identity) => {
                let values = generations.entry(token.into()).or_default();
                if !values.contains(&identity) {
                    values.push(identity);
                }
            }
            None => {
                unprotected.insert(token.into());
            }
        }
        match node
            .labels
            .get(CHECKOUT_FENCE_LABEL)
            .filter(|text| text.len() < 256)
            .and_then(|text| {
                serde_json::from_str::<effigy_kernel::gate::CheckoutFenceIdentity>(text).ok()
            }) {
            Some(identity) => {
                let values = fences.entry(token.into()).or_default();
                if !values.contains(&identity) {
                    values.push(identity);
                }
            }
            None => {
                unprotected.insert(token.to_owned());
            }
        }
        if let Some(checkout) = node
            .labels
            .get(CHECKOUT_LABEL)
            .or_else(|| node.labels.get(REPO_ROOT_LABEL))
            .and_then(|label| path(label))
        {
            paths
                .entry(token.into())
                .or_default()
                .insert(checkout.display().to_string());
        }
    }
    // Shared route owners are retained after checkout removal. Their exact
    // scope/path adds provenance for old volume/network labels lacking paths.
    let table = RouteTable::load(&super::super::gateway_registration::gateway_route_table_path()?)
        .map_err(invalid)?;
    for route in table.all_routes() {
        if let Some(token) = &route.scope {
            if let Some(checkout) = path(&route.project) {
                paths
                    .entry(token.clone())
                    .or_default()
                    .insert(checkout.display().to_string());
            }
        }
    }
    if let Some(home) = effigy_containers::effigy_home() {
        let registry = PortRegistry::load(&home.join("ports.json")).map_err(invalid)?;
        for node in inventory {
            if let Some((token, project)) = scope(node) {
                if let Some(allocation) = registry.get(project) {
                    if let Some(checkout) = path(&allocation.project) {
                        paths
                            .entry(token.into())
                            .or_default()
                            .insert(checkout.display().to_string());
                    }
                }
            }
        }
    }
    for node in inventory {
        let Some((token, project)) = scope(node) else {
            continue;
        };
        if unprotected.contains(token)
            || fences.get(token).is_none_or(|entries| entries.len() != 1)
            || generations
                .get(token)
                .is_none_or(|entries| entries.len() != 1)
        {
            unknown.push(format!(
                "{token}: checkout custody fence is missing or ambiguous"
            ));
            continue;
        }
        let Some(candidates) = paths.get(token).filter(|paths| paths.len() == 1) else {
            unknown.push(format!("{token}: missing or ambiguous checkout provenance"));
            continue;
        };
        let checkout = candidates.first().expect("one candidate");
        let generation = generations
            .get(token)
            .and_then(|entries| entries.first())
            .copied()
            .expect("one checked generation");
        match dead_generation(Path::new(checkout), generation, token) {
            Ok(true) => {}
            Ok(false) => continue,
            Err(reason) => {
                unknown.push(format!("{token}: {reason}"));
                continue;
            }
        }

        let record = records
            .entry(token.to_owned())
            .or_insert_with(|| ScopeRecord {
                schema: "effigy.runtime-scope.v1".into(),
                schema_version: 1,
                token: token.into(),
                host_key: token[..8].into(),
                checkout: checkout.clone(),
                updated_unix: 0,
                compose_kind: ScopeComposeKind::Generated,
                profile: profile.into(),
                profiles: vec![profile.into()],
                project_names: vec![],
                retain_project_names: vec![],
                repo_owned_projects: vec![],
                owned_volumes: vec![],
                retain_volumes: vec![],
                routes: vec![],
                retain_routes: vec![],
                loopback_identities: vec![],
                retain_loopback_identities: vec![],
                pending_tls_certs: vec![],
                checkout_generation: Some(generation),
                checkout_fence: fences
                    .get(token)
                    .and_then(|entries| entries.first())
                    .copied(),
            });
        if !record.project_names.contains(&project.to_owned()) {
            record.project_names.push(project.into());
        }
        if node.kind == ObservedKind::Volume {
            record.owned_volumes.push(node.identity.clone());
            if node.labels.get(PERSIST_LABEL).map(String::as_str) != Some("false") {
                record.retain_volumes.push(node.identity.clone());
            }
        }
    }
    for record in records.values_mut() {
        for route in table.all_routes() {
            if route.scope.as_deref() == Some(&record.token) && route.project == record.checkout {
                record.routes.push(route.domain.clone());
                if route.tls {
                    record.pending_tls_certs.push(route.domain.clone());
                }
            }
        }
        for project in &record.project_names {
            record
                .loopback_identities
                .push(effigy_gateway::loopback::project_loopback_identity(
                    project,
                    Path::new(&record.checkout),
                ));
        }
    }
    // Progress outlives a last backend deletion, without recreating its old root.
    for record in effigy_containers::load_local_scope_records(root).map_err(invalid)? {
        if record.checkout_generation.is_some_and(|identity| {
            dead_generation(Path::new(&record.checkout), identity, &record.token) == Ok(true)
        }) && record.observation_profiles() == [profile]
            && !inventory
                .iter()
                .any(|node| scope_token(node).is_some_and(|token| token == record.token))
        {
            records.entry(record.token.clone()).or_insert(record);
        }
    }
    unknown.sort();
    unknown.dedup();
    Ok((records.into_values().collect(), unknown))
}
fn observed(record: &ScopeRecord, profile: &str, inventory: &[Node]) -> Vec<ObservedResource> {
    inventory
        .iter()
        .filter(|node| scope(node).is_some_and(|(token, _)| token == record.token))
        .map(|node| ObservedResource {
            kind: node.kind,
            name: node.identity.clone(),
            scope_label: Some(record.token.clone()),
            project_label: scope(node).map(|(_, project)| project.into()),
            persist: node.kind == ObservedKind::Volume
                && node.labels.get(PERSIST_LABEL).map(String::as_str) != Some("false"),
            external: node
                .labels
                .get("com.docker.compose.external")
                .is_some_and(|s| s == "true"),
            profile: Some(profile.into()),
        })
        .collect()
}
fn retire_inner(
    run: &mut effigy_kernel::run::Run<'_>,
    root: &Path,
    profile: &str,
    record: &ScopeRecord,
) -> Result<serde_json::Value, RunnerError> {
    let expected = record
        .checkout_fence
        .ok_or_else(|| invalid("checkout custody fence is unknown"))?;
    let cx = operation(effigy_kernel::ctx::EffectClass::Mutate)
        .effects()
        .map_err(|_| invalid("retirement context"))?;
    let home = effigy_kernel::root::StateRoot::home_from_env()
        .ok_or_else(|| invalid("HOME is required"))?;
    let state =
        effigy_kernel::root::StateRoot::runtime_machine_effects(&cx, &home).map_err(invalid)?;
    let _fence = effigy_kernel::gate::Gate::checkout_retirement(
        &cx,
        &state,
        &record.token,
        expected,
        effigy_kernel::gate::Wait::FailFast,
    )
    .map_err(invalid)?;
    let record = remember_pending_tls(root, record)?;
    upsert_scope_record_at(root, &record).map_err(invalid)?;
    let inventory = nodes(root, profile)?;
    let (verified, _) = discover(root, profile, &inventory)?;
    if !verified
        .iter()
        .any(|entry| entry.token == record.token && entry.checkout == record.checkout)
    {
        return Err(invalid(
            "ownership changed or became ambiguous before retirement",
        ));
    }
    let mut resources = observed(&record, profile, &inventory);
    resources.extend(observe_routes(&record)?);
    resources.extend(observe_ports(&record)?);
    resources.extend(observe_loopbacks(&record)?);
    resources.extend(observe_tls_certs(&record)?);
    let plan = plan_retirement(&record, &resources);
    let mut removed = Vec::new();
    let mut done = BTreeSet::new();
    for resource in &plan.delete {
        if record.checkout_generation.is_none_or(|identity| {
            dead_generation(Path::new(&record.checkout), identity, &record.token) != Ok(true)
        }) {
            return Err(invalid("checkout reappeared during retirement"));
        }
        let ticket=run.intend(effigy_kernel::run::EffectSpec{target:run.fence().clone(),kind:"container:retire-dead".into(),payload:serde_json::json!({"backend":profile,"scope":record.token,"checkout":record.checkout,"resource":resource.name,"kind":format!("{:?}",resource.kind)}),settlement:effigy_kernel::run::Settlement::Irreversible,required:false}).map_err(invalid)?;
        if matches!(
            resource.kind,
            ObservedKind::Container | ObservedKind::Network | ObservedKind::Volume
        ) {
            let fresh = nodes(root, profile)?;
            let (owners, _) = discover(root, profile, &fresh)?;
            if !owners.iter().any(|owner| {
                owner.token == record.token
                    && owner.checkout == record.checkout
                    && owner.checkout_generation == record.checkout_generation
                    && owner.checkout_fence == record.checkout_fence
            }) || !observed(&record, profile, &fresh).iter().any(|current| {
                current.kind == resource.kind
                    && current.name == resource.name
                    && !current.persist
                    && !current.external
            }) {
                return Err(invalid("backend ownership changed before deletion"));
            }
        }
        let deleted = match resource.kind {
            ObservedKind::Container | ObservedKind::Network | ObservedKind::Volume => {
                remove(run, root, profile, resource)?
            }
            ObservedKind::Route | ObservedKind::TlsCert => {
                if done.insert("routes") {
                    delete_routes_and_owned_certs(&record)?
                } else {
                    true
                }
            }
            ObservedKind::Port => {
                if done.insert("ports") {
                    delete_ports(&record)?
                } else {
                    true
                }
            }
            ObservedKind::Loopback => {
                if done.insert("loopbacks") {
                    delete_loopbacks(&record)?
                } else {
                    true
                }
            }
        };
        if deleted {
            run.applied(&ticket, serde_json::Value::Null)
                .map_err(invalid)?;
            removed.push(resource.clone());
        }
    }
    let after = nodes(root, profile)?;
    let mut resources = observed(&record, profile, &after);
    resources.extend(observe_routes(&record)?);
    resources.extend(observe_ports(&record)?);
    resources.extend(observe_loopbacks(&record)?);
    resources.extend(observe_tls_certs(&record)?);
    let remaining = remaining_after(&record, &resources);
    let complete = remaining.is_empty();
    if complete {
        remove_scope_record(root, &record.token).map_err(invalid)?;
    }
    let report = retire_report(Some(&record), Some(&plan), &removed, &remaining, complete);
    let rendered = render_container_report(report, true);
    if !complete {
        return Err(RunnerError::CommandJsonFailure { rendered });
    }
    serde_json::from_str(&rendered).map_err(invalid)
}
pub(super) fn retire(
    root: &Path,
    profile: &str,
    record: &ScopeRecord,
) -> Result<serde_json::Value, RunnerError> {
    let plan = effigy_kernel::supervise::LifecyclePlan {
        executable: executable()?,
        placement: effigy_kernel::supervise::Placement::Inline,
    };
    match effigy_containers::with_scope_retirement(root, &record.token, plan, |run| {
        retire_inner(run, root, profile, record)
    }) {
        Ok(value) => Ok(value),
        Err(effigy_containers::ScopeJournalError::Storage(error)) => Err(invalid(error)),
        Err(effigy_containers::ScopeJournalError::Operation(error)) => Err(error),
    }
}
pub(super) fn run(
    repo_override: Option<PathBuf>,
    yes: bool,
    output_json: bool,
) -> Result<String, RunnerError> {
    let root = journal_checkout(repo_override)?;
    let profile = effigy_containers::backend::global_endpoint(None);
    let inventory = nodes(&root, &profile)?;
    let (records, unknown) = discover(&root, &profile, &inventory)?;
    if !records.is_empty() {
        maybe_confirm_destructive_container_action(
            "effigy container retire --dead",
            &format!(
                "Retire {} positively missing checkout scope(s) on {profile}",
                records.len()
            ),
            output_json,
            yes,
        )?;
    }
    let mut outcomes = Vec::new();
    let mut failed = false;
    for record in records {
        match retire(&root, &profile, &record) {
            Ok(report) => {
                let ok = report.get("ok").and_then(serde_json::Value::as_bool) == Some(true);
                failed |= !ok;
                outcomes.push(serde_json::json!({"token":record.token,"checkout":record.checkout,"ok":ok,"report":report}));
            }
            Err(error) => {
                failed = true;
                outcomes.push(serde_json::json!({"token":record.token,"checkout":record.checkout,"ok":false,"error":error.to_string()}));
            }
        }
    }
    let value = serde_json::json!({"schema":DEAD_RETIRE_SCHEMA,"backend":profile,"scopes":outcomes,"unknown":unknown});
    let rendered = if output_json {
        value.to_string()
    } else {
        serde_json::to_string_pretty(&value).map_err(invalid)?
    };
    if failed {
        if output_json {
            Err(RunnerError::CommandJsonFailure { rendered })
        } else {
            Err(invalid(rendered))
        }
    } else {
        Ok(rendered)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn node(root: &Path, token: &str, kind: ObservedKind) -> Node {
        Node {
            kind,
            identity: format!("owned-{token}"),
            labels: BTreeMap::from([
                (MANAGED_LABEL.into(), "true".into()),
                (SCOPE_LABEL.into(), token.into()),
                (PROJECT_LABEL.into(), format!("app-web-wt-{}", &token[..12])),
                (CHECKOUT_LABEL.into(), root.display().to_string()),
                (
                    CHECKOUT_FENCE_LABEL.into(),
                    r#"{"device":7,"inode":11}"#.into(),
                ),
                (PERSIST_LABEL.into(), "false".into()),
                (
                    CHECKOUT_GENERATION_LABEL.into(),
                    serde_json::to_string(
                        &effigy_kernel::fs::DirFd::open(if root.exists() {
                            root
                        } else {
                            root.parent().unwrap()
                        })
                        .unwrap()
                        .identity()
                        .unwrap(),
                    )
                    .unwrap(),
                ),
            ]),
        }
    }
    #[test]
    fn dead_generation_requires_the_recorded_volume_and_verified_replacement() {
        let temp = tempfile::tempdir().unwrap();
        let checkout = temp.path().join("checkout");
        std::fs::create_dir(&checkout).unwrap();
        let identity = effigy_kernel::fs::DirFd::open(&checkout)
            .unwrap()
            .identity()
            .unwrap();
        let token = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        assert_eq!(dead_generation(&checkout, identity, token), Ok(false));
        std::fs::rename(&checkout, temp.path().join("removed")).unwrap();
        assert_eq!(dead_generation(&checkout, identity, token), Ok(true));
        let mut foreign = identity;
        foreign.device += 1;
        assert!(dead_generation(&checkout, foreign, token).is_err());
        std::fs::create_dir(&checkout).unwrap();
        assert!(dead_generation(&checkout, identity, token).is_err());
        effigy_core::worktree_scope::prepare_checkout_token(&checkout).unwrap();
        assert_eq!(dead_generation(&checkout, identity, token), Ok(true));
    }

    #[test]
    fn dead_scope_discovery_requires_consistent_path_generation_and_fence_without_an_index() {
        let temp = tempfile::tempdir().unwrap();
        let caller = temp.path().join("caller");
        let live = temp.path().join("live");
        let missing = temp.path().join("missing");
        std::fs::create_dir_all(&caller).unwrap();
        std::fs::create_dir_all(&live).unwrap();
        effigy_containers::with_test_effigy_home(&temp.path().join("home/.effigy"), || {
            let token = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
            let owned = node(&missing, token, ObservedKind::Container);
            let active = node(
                &live,
                "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                ObservedKind::Container,
            );
            let (records, unknown) =
                discover(&caller, "selected", &[owned.clone(), active]).unwrap();
            assert_eq!(records.len(), 1);
            assert_eq!(records[0].checkout, missing.display().to_string());
            assert!(unknown.is_empty());
            assert!(!temp.path().join("home").exists(), "discovery is pure");
            let mut unprepared = owned.clone();
            unprepared.labels.remove(CHECKOUT_FENCE_LABEL);
            let (records, unknown) =
                discover(&caller, "selected", &[owned.clone(), unprepared]).unwrap();
            assert!(records.is_empty());
            assert!(!unknown.is_empty());
            let mut ambiguous = owned.clone();
            ambiguous
                .labels
                .insert(CHECKOUT_LABEL.into(), live.display().to_string());
            assert!(discover(&caller, "selected", &[owned.clone(), ambiguous])
                .unwrap()
                .0
                .is_empty());
            let mut replaced = owned.clone();
            replaced.labels.insert(
                CHECKOUT_FENCE_LABEL.into(),
                r#"{"device":7,"inode":12}"#.into(),
            );
            assert!(discover(&caller, "selected", &[owned.clone(), replaced])
                .unwrap()
                .0
                .is_empty());
            let mut foreign = owned.clone();
            foreign
                .labels
                .insert(PROJECT_LABEL.into(), "unrelated-project".into());
            assert!(discover(&caller, "selected", &[owned, foreign])
                .unwrap()
                .0
                .is_empty());
        });
    }
    #[test]
    fn dead_scope_inventory_preserves_persistent_unknown_and_foreign_resources() {
        let temp = tempfile::tempdir().unwrap();
        let caller = temp.path().join("caller");
        std::fs::create_dir_all(&caller).unwrap();
        let missing = temp.path().join("gone");
        effigy_containers::with_test_effigy_home(&temp.path().join("home/.effigy"), || {
            let token = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
            let mutable = node(&missing, token, ObservedKind::Volume);
            let mut persistent = mutable.clone();
            persistent.identity = "persistent".into();
            persistent
                .labels
                .insert(PERSIST_LABEL.into(), "true".into());
            let mut unknown = mutable.clone();
            unknown.identity = "unclassified".into();
            unknown.labels.remove(PERSIST_LABEL);
            let foreign = node(
                &missing,
                "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                ObservedKind::Volume,
            );
            let inventory = [mutable, persistent, unknown, foreign];
            let (records, _) = discover(&caller, "selected", &inventory).unwrap();
            let record = records.iter().find(|r| r.token == token).unwrap();
            let plan = plan_retirement(record, &observed(record, "selected", &inventory));
            assert_eq!(plan.delete.len(), 1);
            assert_eq!(plan.retain.len(), 2);
            assert!(!plan
                .delete
                .iter()
                .any(|resource| resource.name.contains("bbbb")));
            assert!(!temp.path().join("home").exists());
        });
    }
    #[test]
    fn dead_scope_retry_inventory_cannot_override_new_live_or_ambiguous_backend_ownership() {
        let temp = tempfile::tempdir().unwrap();
        let caller = temp.path().join("caller");
        let live = temp.path().join("live");
        std::fs::create_dir_all(&caller).unwrap();
        std::fs::create_dir_all(&live).unwrap();
        let missing = temp.path().join("gone");
        let token = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        effigy_containers::with_test_effigy_home(&temp.path().join("home/.effigy"), || {
            let (records, _) = discover(
                &caller,
                "selected",
                &[node(&missing, token, ObservedKind::Container)],
            )
            .unwrap();
            std::fs::create_dir_all(temp.path().join("home")).unwrap();
            upsert_scope_record_at(&caller, &records[0]).unwrap();
            assert_eq!(
                discover(&caller, "selected", &[]).unwrap().0.len(),
                1,
                "retained cleanup survives last container removal"
            );
            assert!(discover(&caller, "other-backend", &[])
                .unwrap()
                .0
                .is_empty());
            assert!(
                discover(
                    &caller,
                    "selected",
                    &[node(&live, token, ObservedKind::Container)]
                )
                .unwrap()
                .0
                .is_empty(),
                "local hints cannot override a live backend owner"
            );
            let mut unknown = node(&missing, token, ObservedKind::Container);
            unknown.labels.remove(CHECKOUT_FENCE_LABEL);
            assert!(discover(&caller, "selected", &[unknown])
                .unwrap()
                .0
                .is_empty());
            assert!(!missing.exists());
            assert!(!temp.path().join("home/.effigy/state/machine.db").exists());
        });
    }
}
