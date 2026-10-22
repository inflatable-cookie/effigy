//! One-release translation of retired `compose_file` environments.
//!
//! A repo-owned Compose file is read as project-layer catalog fragments, one
//! per Compose service. `effigy container migrate` writes exactly these
//! fragments and service declarations, so the render before and after
//! migration is the same. Anything a fragment cannot express refuses with the
//! offending key instead of being approximated.
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Component, Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use effigy_catalog::fragment::{CatalogFragment, CatalogResolver, FragmentSource};
use effigy_manifest::{ManifestContainerConfig, ManifestContainerServiceConfig};
use serde_yaml::{Mapping, Value};

use crate::{ContainerPolicyError, PROJECT_LOCAL_CATALOG_DIR};

/// One translated Compose service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigratedFragment {
    /// Catalog fragment name and directory name.
    pub name: String,
    /// Compose service name, kept as the manifest service key.
    pub service: String,
    /// Project-layer directory the fragment resolves from after migration.
    pub directory: PathBuf,
    pub service_toml: String,
    pub compose_fragment: String,
}

/// The translation of one `compose_file` environment.
#[derive(Debug, Clone)]
pub struct ComposeFileMigration {
    pub container: String,
    /// Repo-owned Compose file the translation read.
    pub compose_file: PathBuf,
    pub fragments: Vec<MigratedFragment>,
    /// The environment as declared after migration.
    pub config: ManifestContainerConfig,
}

impl ComposeFileMigration {
    /// The `[containers.<name>.services.<service>]` tables migration writes.
    pub fn services(&self) -> BTreeMap<String, String> {
        self.fragments
            .iter()
            .map(|fragment| (fragment.service.clone(), fragment.name.clone()))
            .collect()
    }
}

fn refuse(container: &str, compose: &Path, reason: impl std::fmt::Display) -> ContainerPolicyError {
    ContainerPolicyError::TaskInvocation(format!(
        "container `{container}` compose_file {} cannot be read as a catalog fragment: {reason}; move the service into a project catalog fragment under `{PROJECT_LOCAL_CATALOG_DIR}/` by hand",
        compose.display()
    ))
}

/// Translate a `compose_file` environment. Returns `None` for catalog-driven
/// environments. Reads the Compose file; writes nothing.
pub fn translate(
    repo_root: &Path,
    container: &str,
    config: &ManifestContainerConfig,
) -> Result<Option<ComposeFileMigration>, ContainerPolicyError> {
    let Some(raw) = config.compose_file.as_deref() else {
        return Ok(None);
    };
    if !config.services.is_empty() {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "container `{container}` cannot combine `compose_file` with `[containers.{container}.services]`"
        )));
    }
    if Path::new(raw).is_absolute() {
        return Err(ContainerPolicyError::TaskInvocation(
            "`containers.*.compose_file` must stay repo-relative in v1".to_owned(),
        ));
    }
    let compose_file = repo_root.join(raw);
    let source =
        std::fs::read_to_string(&compose_file).map_err(|error| ContainerPolicyError::Read {
            path: compose_file.clone(),
            error,
        })?;
    let document: Value = serde_yaml::from_str(&source)
        .map_err(|error| refuse(container, &compose_file, format!("invalid YAML: {error}")))?;
    let paths = Paths {
        repo_root: normalize(repo_root),
        compose_dir: normalize(compose_file.parent().unwrap_or(repo_root)),
    };
    let fail = |reason: String| refuse(container, &compose_file, reason);
    let root = document
        .as_mapping()
        .ok_or_else(|| fail("the document is not a mapping".to_owned()))?;
    let mut declared_volumes = BTreeSet::new();
    for (key, value) in root {
        let key = key.as_str().unwrap_or("<non-string key>");
        match key {
            "services" | "version" => {}
            "volumes" => {
                for (name, options) in value.as_mapping().into_iter().flatten() {
                    let name = name
                        .as_str()
                        .ok_or_else(|| fail("a volume name is not a string".to_owned()))?;
                    if !options.is_null() && options.as_mapping().is_none_or(|map| !map.is_empty())
                    {
                        return Err(fail(format!(
                            "volume `{name}` declares options; fragments own volume names and drivers"
                        )));
                    }
                    declared_volumes.insert(name.to_owned());
                }
            }
            other => return Err(fail(format!("top-level `{other}` is not supported"))),
        }
    }
    let services = root
        .get("services")
        .and_then(Value::as_mapping)
        .filter(|services| !services.is_empty())
        .ok_or_else(|| fail("it declares no services".to_owned()))?;

    let lower = CatalogResolver::new(None);
    let mut fragments = Vec::new();
    let mut translated = config.clone();
    translated.compose_file = None;
    let mut rewritten = Vec::new();
    for (name, definition) in services {
        let service = name
            .as_str()
            .ok_or_else(|| fail("a service name is not a string".to_owned()))?;
        let mut definition = definition.clone();
        reject_template_syntax(&definition).map_err(&fail)?;
        let body = definition
            .as_mapping_mut()
            .ok_or_else(|| fail(format!("service `{service}` is not a mapping")))?;
        rewrite_service(service, body, &paths, &declared_volumes).map_err(&fail)?;
        rewrite_ports(service, body).map_err(&fail)?;
        rewritten.push((service.to_owned(), definition));
    }
    publish_declared_ports(config, &mut rewritten).map_err(&fail)?;
    for (service, definition) in rewritten {
        let service = service.as_str();
        let mut fragment_name = fragment_name(container, service);
        if lower.resolve(&fragment_name).is_ok() {
            fragment_name.push_str("-migrated");
        }
        let mut wrapper = Mapping::new();
        wrapper.insert(Value::String("{{ service_name }}".into()), definition);
        let mut services = Mapping::new();
        services.insert(Value::String("services".into()), Value::Mapping(wrapper));
        let compose_fragment = serde_yaml::to_string(&Value::Mapping(services))
            .map_err(|error| fail(format!("cannot serialize service `{service}`: {error}")))?;
        let relative = compose_file
            .strip_prefix(repo_root)
            .unwrap_or(&compose_file)
            .display()
            .to_string();
        let service_toml = format!(
            "[service]\nname = {}\ndescription = {}\n",
            toml_string(&fragment_name),
            toml_string(&format!(
                "Service `{service}` migrated from {relative} by `effigy container migrate`."
            )),
        );
        translated.services.insert(
            service.to_owned(),
            ManifestContainerServiceConfig {
                catalog: fragment_name.clone(),
                variant: None,
                config: None,
                shared: None,
                params: BTreeMap::new(),
            },
        );
        fragments.push(MigratedFragment {
            directory: repo_root
                .join(PROJECT_LOCAL_CATALOG_DIR)
                .join(&fragment_name),
            name: fragment_name,
            service: service.to_owned(),
            service_toml,
            compose_fragment,
        });
    }
    let names = fragments.iter().map(|f| &f.name).collect::<BTreeSet<_>>();
    if names.len() != fragments.len() {
        return Err(fail(
            "two services map to the same fragment name".to_owned(),
        ));
    }
    Ok(Some(ComposeFileMigration {
        container: container.to_owned(),
        compose_file,
        fragments,
        config: translated,
    }))
}

/// Everything `effigy container migrate` writes, computed without writing.
#[derive(Debug, Clone)]
pub struct MigrationPlan {
    pub migration: ComposeFileMigration,
    /// The checkout manifest file that declares `compose_file`.
    pub manifest: PathBuf,
    pub manifest_after: String,
}

/// Plan the migration of one `compose_file` environment.
pub fn plan_migration(
    repo_root: &Path,
    requested: Option<&str>,
) -> Result<MigrationPlan, ContainerPolicyError> {
    let repo_root = repo_root
        .canonicalize()
        .map_err(|error| ContainerPolicyError::Read {
            path: repo_root.into(),
            error,
        })?;
    let loaded = effigy_manifest::load_task_manifest_with_inspection(
        &repo_root.join(effigy_manifest::TASK_MANIFEST_FILE),
    )?;
    let containers = loaded.manifest.containers.as_ref().ok_or_else(|| {
        ContainerPolicyError::TaskInvocation(
            "manifest does not define a `[containers]` registry".to_owned(),
        )
    })?;
    let name = super::load::resolve_container_name(&loaded.manifest, containers, requested)?;
    let config = containers.environments.get(&name).ok_or_else(|| {
        ContainerPolicyError::TaskInvocation(format!(
            "container `{name}` is not defined in `[containers]`"
        ))
    })?;
    let migration = translate(&repo_root, &name, config)?.ok_or_else(|| {
        ContainerPolicyError::TaskInvocation(format!(
            "container `{name}` does not declare `compose_file`; nothing to migrate"
        ))
    })?;
    let mut declaring = Vec::new();
    for path in &loaded.evaluation_order {
        let text = std::fs::read_to_string(path).map_err(|error| ContainerPolicyError::Read {
            path: path.clone(),
            error,
        })?;
        let document = text.parse::<toml_edit::DocumentMut>().map_err(|error| {
            ContainerPolicyError::TaskInvocation(format!("{}: {error}", path.display()))
        })?;
        let declares = document
            .get("containers")
            .and_then(|containers| containers.get(&name))
            .and_then(|container| container.get("compose_file"))
            .is_some();
        if declares {
            declaring.push((path.clone(), text, document));
        }
    }
    let [(manifest, _, mut document)] = <[_; 1]>::try_from(declaring).map_err(|found| {
        ContainerPolicyError::TaskInvocation(format!(
            "container `{name}` declares `compose_file` in {} manifest files; expected exactly one",
            found.len()
        ))
    })?;
    if manifest.strip_prefix(&repo_root).is_err() {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "container `{name}` takes `compose_file` from {}, outside this checkout; migrate it where it is declared",
            manifest.display()
        )));
    }
    let manifest_after =
        rewrite_manifest(&mut document, &name, &migration.services()).map_err(|reason| {
            ContainerPolicyError::TaskInvocation(format!("{}: {reason}", manifest.display()))
        })?;
    Ok(MigrationPlan {
        migration,
        manifest,
        manifest_after,
    })
}

/// Replace `compose_file` with one `services.<service>` table per fragment.
/// Comments and the order of unrelated keys are preserved.
fn rewrite_manifest(
    document: &mut toml_edit::DocumentMut,
    container: &str,
    services: &BTreeMap<String, String>,
) -> Result<String, String> {
    let entry = document
        .get_mut("containers")
        .and_then(|containers| containers.get_mut(container))
        .ok_or("the container table vanished")?;
    match entry {
        toml_edit::Item::Table(table) => {
            table.remove("compose_file");
            let position = table.position();
            let mut declared = toml_edit::Table::new();
            declared.set_implicit(true);
            for (service, catalog) in services {
                let mut service_table = toml_edit::Table::new();
                if let Some(position) = position {
                    service_table.set_position(Some(position));
                }
                service_table.insert("catalog", toml_edit::value(catalog.as_str()));
                declared.insert(service, toml_edit::Item::Table(service_table));
            }
            table.insert("services", toml_edit::Item::Table(declared));
        }
        toml_edit::Item::Value(toml_edit::Value::InlineTable(table)) => {
            table.remove("compose_file");
            let mut declared = toml_edit::InlineTable::new();
            for (service, catalog) in services {
                let mut service_table = toml_edit::InlineTable::new();
                service_table.insert("catalog", catalog.as_str().into());
                declared.insert(service, toml_edit::Value::InlineTable(service_table));
            }
            table.insert("services", toml_edit::Value::InlineTable(declared));
        }
        _ => return Err(format!("`containers.{container}` is not a table")),
    }
    Ok(document.to_string())
}

/// Translate for policy resolution: register the fragments for this checkout
/// and warn once. A fragment already on disk must match the translation.
pub(crate) fn translate_for_policy(
    repo_root: &Path,
    container: &str,
    config: &ManifestContainerConfig,
) -> Result<Option<ManifestContainerConfig>, ContainerPolicyError> {
    let Some(migration) = translate(repo_root, container, config)? else {
        return Ok(None);
    };
    let mut overlay = HashMap::new();
    for fragment in &migration.fragments {
        if fragment.directory.exists() && !fragment_matches(fragment) {
            return Err(ContainerPolicyError::TaskInvocation(format!(
                "container `{container}` still declares `compose_file`, but {} already holds a different fragment; finish `effigy container migrate {container}` or remove `compose_file`",
                fragment.directory.display()
            )));
        }
        let loaded = CatalogResolver::fragment_from_sources(
            &fragment.name,
            &fragment.service_toml,
            &fragment.compose_fragment,
            FragmentSource::Project(fragment.directory.clone()),
        )?;
        overlay.insert(fragment.name.clone(), loaded);
    }
    register(repo_root, overlay);
    warn_once(repo_root, container, &migration);
    Ok(Some(migration.config))
}

/// Whether an on-disk fragment directory already holds exactly these bytes.
pub fn fragment_matches(fragment: &MigratedFragment) -> bool {
    let read = |name: &str| std::fs::read_to_string(fragment.directory.join(name)).ok();
    read("service.toml").as_deref() == Some(fragment.service_toml.as_str())
        && read("compose.fragment.yml").as_deref() == Some(fragment.compose_fragment.as_str())
}

type Overlays = BTreeMap<PathBuf, HashMap<String, CatalogFragment>>;

fn overlays() -> &'static Mutex<Overlays> {
    static OVERLAYS: OnceLock<Mutex<Overlays>> = OnceLock::new();
    OVERLAYS.get_or_init(Mutex::default)
}

fn overlay_key(repo_root: &Path) -> PathBuf {
    repo_root
        .canonicalize()
        .unwrap_or_else(|_| repo_root.to_path_buf())
}

fn register(repo_root: &Path, overlay: HashMap<String, CatalogFragment>) {
    let mut overlays = overlays().lock().unwrap_or_else(|e| e.into_inner());
    overlays
        .entry(overlay_key(repo_root))
        .or_default()
        .extend(overlay);
}

/// Translated fragments for this checkout, layered below its catalog directory.
pub(crate) fn overlay_for(repo_root: &Path) -> HashMap<String, CatalogFragment> {
    overlays()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&overlay_key(repo_root))
        .cloned()
        .unwrap_or_default()
}

fn warn_once(repo_root: &Path, container: &str, migration: &ComposeFileMigration) {
    static WARNED: OnceLock<Mutex<BTreeSet<(PathBuf, String)>>> = OnceLock::new();
    let warned = WARNED.get_or_init(Mutex::default);
    let mut warned = warned.lock().unwrap_or_else(|e| e.into_inner());
    if !warned.insert((overlay_key(repo_root), container.to_owned())) {
        return;
    }
    let fragments = migration
        .fragments
        .iter()
        .map(|fragment| format!("{PROJECT_LOCAL_CATALOG_DIR}/{}/", fragment.name))
        .collect::<Vec<_>>()
        .join(", ");
    eprintln!(
        "[warn] `[containers.{container}].compose_file` is deprecated in v0.15 and is read as project catalog fragment(s) for one release; run `effigy container migrate {container}` to write {fragments} and replace the key."
    );
}

fn fragment_name(container: &str, service: &str) -> String {
    let mut name = String::new();
    for c in format!("{container}-{service}").chars() {
        let c = c.to_ascii_lowercase();
        if c.is_ascii_alphanumeric() {
            name.push(c);
        } else if !name.ends_with('-') {
            name.push('-');
        }
    }
    name.trim_matches('-').to_owned()
}

fn toml_string(value: &str) -> String {
    toml::Value::String(value.to_owned()).to_string()
}

struct Paths {
    repo_root: PathBuf,
    compose_dir: PathBuf,
}

impl Paths {
    /// A Compose-relative path, re-expressed against the template's repo root.
    fn relocate(&self, raw: &str) -> String {
        if Path::new(raw).is_absolute() || raw.starts_with('~') {
            return raw.to_owned();
        }
        let target = normalize(&self.compose_dir.join(raw));
        let mut root = self.repo_root.clone();
        let mut up = 0;
        while !target.starts_with(&root) {
            if !root.pop() {
                return target.display().to_string();
            }
            up += 1;
        }
        let rest = target.strip_prefix(&root).unwrap_or(Path::new(""));
        let mut rendered = String::from("{{ repo_root }}");
        for _ in 0..up {
            rendered.push_str("/..");
        }
        if !rest.as_os_str().is_empty() {
            rendered.push('/');
            rendered.push_str(&rest.display().to_string());
        }
        rendered
    }
}

/// Lexical normalization: `.` and `..` resolved without touching the disk.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn reject_template_syntax(value: &Value) -> Result<(), String> {
    match value {
        Value::String(text) => {
            for marker in ["{{", "{%", "{#"] {
                if text.contains(marker) {
                    return Err(format!(
                        "value `{text}` contains template syntax `{marker}`"
                    ));
                }
            }
            if text.replace("$$", "").contains('$') {
                return Err(format!(
                    "value `{text}` uses Compose interpolation; generated Compose treats `$` literally"
                ));
            }
            Ok(())
        }
        Value::Sequence(items) => items.iter().try_for_each(reject_template_syntax),
        Value::Mapping(map) => map.iter().try_for_each(|(key, value)| {
            if key.as_str() == Some("<<") {
                return Err("YAML merge keys are not supported".to_owned());
            }
            reject_template_syntax(key)?;
            reject_template_syntax(value)
        }),
        Value::Tagged(tagged) => reject_template_syntax(&tagged.value),
        Value::Null | Value::Bool(_) | Value::Number(_) => Ok(()),
    }
}

fn rewrite_service(
    service: &str,
    body: &mut Mapping,
    paths: &Paths,
    volumes: &BTreeSet<String>,
) -> Result<(), String> {
    for key in [
        "extends",
        "networks",
        "network_mode",
        "configs",
        "secrets",
        "volumes_from",
    ] {
        if body.contains_key(key) {
            return Err(format!("service `{service}` uses `{key}`"));
        }
    }
    if let Some(build) = body.get_mut("build") {
        match build {
            Value::String(context) => *context = relocate_context(paths, context),
            Value::Mapping(build) => {
                if build.contains_key("additional_contexts") {
                    return Err(format!(
                        "service `{service}` uses `build.additional_contexts`"
                    ));
                }
                if let Some(Value::String(context)) = build.get_mut("context") {
                    *context = relocate_context(paths, context);
                }
            }
            _ => return Err(format!("service `{service}` has an invalid `build`")),
        }
    }
    if let Some(env_file) = body.get_mut("env_file") {
        let entries = match env_file {
            Value::Sequence(entries) => entries.iter_mut().collect::<Vec<_>>(),
            other => vec![other],
        };
        for entry in entries {
            let Value::String(path) = entry else {
                return Err(format!("service `{service}` has a structured `env_file`"));
            };
            *path = paths.relocate(path);
        }
    }
    if let Some(entries) = body.get_mut("volumes") {
        let entries = entries
            .as_sequence_mut()
            .ok_or_else(|| format!("service `{service}` has invalid `volumes`"))?;
        for entry in entries {
            match entry {
                Value::String(raw) => *raw = rewrite_volume(service, raw, paths, volumes)?,
                Value::Mapping(long) => {
                    let kind = long.get("type").and_then(Value::as_str).unwrap_or("volume");
                    match kind {
                        "bind" => {
                            if let Some(Value::String(source)) = long.get_mut("source") {
                                *source = paths.relocate(source);
                            }
                        }
                        "tmpfs" => {}
                        other => {
                            return Err(format!(
                                "service `{service}` uses a long-syntax `{other}` mount; use `name:/path`"
                            ))
                        }
                    }
                }
                _ => return Err(format!("service `{service}` has an invalid volume entry")),
            }
        }
    }
    Ok(())
}

/// Normalize published ports to `HOST:CONTAINER` (or `CONTAINER`). As in any
/// catalog fragment, Effigy's port policy owns the publish address and the
/// host port unless `[host].ports` fixes it.
fn rewrite_ports(service: &str, body: &mut Mapping) -> Result<(), String> {
    let Some(entries) = body.get_mut("ports") else {
        return Ok(());
    };
    let entries = entries
        .as_sequence_mut()
        .ok_or_else(|| format!("service `{service}` has invalid `ports`"))?;
    let unsupported = |raw: &str| {
        format!("service `{service}` publishes `{raw}`; only TCP `[127.0.0.1:]HOST:CONTAINER` or `CONTAINER` mappings translate")
    };
    for entry in entries {
        let (host, container) = match entry {
            Value::String(raw) => {
                let mapping = raw.strip_suffix("/tcp").unwrap_or(raw);
                match mapping.split(':').collect::<Vec<_>>().as_slice() {
                    [container] => (None, (*container).to_owned()),
                    [host, container] | ["127.0.0.1", host, container] => {
                        (Some((*host).to_owned()), (*container).to_owned())
                    }
                    _ => return Err(unsupported(raw)),
                }
            }
            Value::Number(number) => (None, number.to_string()),
            Value::Mapping(long) => {
                let text = |key: &str| match long.get(key) {
                    Some(Value::Number(number)) => Some(number.to_string()),
                    Some(Value::String(text)) => Some(text.clone()),
                    _ => None,
                };
                let extra = long
                    .keys()
                    .filter_map(Value::as_str)
                    .find(|key| !matches!(*key, "target" | "published" | "protocol" | "host_ip"));
                if extra.is_some()
                    || text("protocol").is_some_and(|protocol| protocol != "tcp")
                    || text("host_ip").is_some_and(|ip| ip != "127.0.0.1")
                {
                    return Err(unsupported("a long-syntax port"));
                }
                let container =
                    text("target").ok_or_else(|| unsupported("a port without target"))?;
                (text("published"), container)
            }
            _ => return Err(unsupported("a port entry")),
        };
        let valid = |value: &str| value.parse::<u16>().is_ok();
        if !valid(&container) || host.as_deref().is_some_and(|host| !valid(host)) {
            return Err(unsupported(&format!(
                "{}{container}",
                host.as_deref()
                    .map(|host| format!("{host}:"))
                    .unwrap_or_default()
            )));
        }
        *entry = Value::String(match &host {
            Some(host) => format!("{host}:{container}"),
            None => container,
        });
    }
    Ok(())
}

/// Direct mode reported `[host].ports` without publishing them; generated
/// Compose requires each declared mapping to be published. A declared mapping
/// no service publishes is published by the primary service.
fn publish_declared_ports(
    config: &ManifestContainerConfig,
    services: &mut [(String, Value)],
) -> Result<(), String> {
    let declared = config
        .host
        .as_ref()
        .map(|host| host.ports.as_slice())
        .unwrap_or_default();
    let published = services
        .iter()
        .filter_map(|(_, definition)| definition.get("ports")?.as_sequence())
        .flatten()
        .filter_map(|entry| entry.as_str()?.rsplit(':').next().map(str::to_owned))
        .collect::<BTreeSet<_>>();
    let missing = declared
        .iter()
        .filter(|mapping| {
            mapping
                .rsplit(':')
                .next()
                .is_some_and(|container| !published.contains(container))
        })
        .cloned()
        .collect::<Vec<_>>();
    if missing.is_empty() {
        return Ok(());
    }
    let primary = config.primary_service.as_deref().unwrap_or_default();
    let (_, definition) = services
        .iter_mut()
        .find(|(service, _)| service == primary)
        .ok_or_else(|| format!("`[host].ports` need primary service `{primary}`"))?;
    let body = definition
        .as_mapping_mut()
        .ok_or_else(|| format!("service `{primary}` is not a mapping"))?;
    let ports = body
        .entry(Value::String("ports".into()))
        .or_insert_with(|| Value::Sequence(Vec::new()))
        .as_sequence_mut()
        .ok_or_else(|| format!("service `{primary}` has invalid `ports`"))?;
    ports.extend(missing.into_iter().map(Value::String));
    Ok(())
}

fn relocate_context(paths: &Paths, context: &str) -> String {
    if context.contains("://") || context.starts_with("git@") {
        return context.to_owned();
    }
    paths.relocate(context)
}

fn rewrite_volume(
    service: &str,
    raw: &str,
    paths: &Paths,
    volumes: &BTreeSet<String>,
) -> Result<String, String> {
    let mut parts = raw.splitn(3, ':');
    let source = parts.next().unwrap_or_default();
    let Some(target) = parts.next() else {
        // An anonymous volume carries no host identity.
        return Ok(raw.to_owned());
    };
    let options = parts.next();
    let source = if source.starts_with(['.', '/', '~']) {
        paths.relocate(source)
    } else if volumes.contains(source) {
        // One name per project volume, shared by every service that mounts it.
        format!("{{{{ project_name }}}}-{source}")
    } else {
        return Err(format!(
            "service `{service}` mounts undeclared volume `{source}`"
        ));
    };
    Ok(match options {
        Some(options) => format!("{source}:{target}:{options}"),
        None => format!("{source}:{target}"),
    })
}

#[cfg(test)]
#[path = "compose_file/tests.rs"]
mod tests;
