//! Gate-scoped inventory inference, never an engine create response.
//! The environment owner must hold Gate(Env) across open, apply and close.
//! A same-user actor forging identical labels within that window is outside
//! the v0.15 threat model. Labels are expectations, not actor authentication.
use super::*;
use crate::runtime::scope::{CHECKOUT_LABEL, MANAGED_LABEL, SCOPE_LABEL};
use effigy_kernel::objects::Closure;
use std::collections::{BTreeMap, BTreeSet};
mod expectation;
mod network;
use expectation::ExpectedInventory;

const OWNERSHIP_LABELS: [&str; 4] = [MANAGED_LABEL, PROJECT_LABEL, SCOPE_LABEL, CHECKOUT_LABEL];

/// Image expectations observed before taking the activation gate.
#[derive(Debug)]
pub struct CreationPreflight {
    digest: String,
    project: String,
    primary_service: String,
    workdir: std::path::PathBuf,
    engine: String,
    context: String,
    images: BTreeMap<String, String>,
    references: BTreeMap<String, String>,
}
impl CreationPreflight {
    pub fn observe(
        backend: &mut dyn EnvironmentBackend,
        request: CreationRequest<'_>,
    ) -> Result<Self, CreationRefusal> {
        let render = request
            .render
            .ok_or_else(|| CreationRefusal::Unknown("preflight has no frozen render".into()))?;
        if let Some(metadata) = render.files().get("inputs/environment.json") {
            let metadata: serde_json::Value = serde_json::from_slice(metadata)
                .map_err(|error| CreationRefusal::Unknown(error.to_string()))?;
            if metadata
                .get("project_name")
                .and_then(|value| value.as_str())
                != Some(request.project)
                || metadata
                    .get("primary_service")
                    .and_then(|value| value.as_str())
                    != Some(request.primary_service)
                || metadata
                    .get("repo_root")
                    .and_then(|value| value.as_str())
                    .map(std::path::Path::new)
                    != Some(request.workdir)
            {
                return Err(CreationRefusal::Unknown(
                    "workspace identity differs from the frozen render".into(),
                ));
            }
            if metadata.get("backend_id").and_then(|value| value.as_str())
                != Some(backend.selection().engine.as_str())
                || metadata
                    .get("backend_context")
                    .and_then(|value| value.as_str())
                    != Some(backend.selection().context.as_str())
            {
                return Err(CreationRefusal::Unknown(
                    "backend identity differs from the frozen render".into(),
                ));
            }
        }
        let declared = declared_services(render, request.project)
            .map_err(|error| CreationRefusal::Unknown(error.to_string()))?;
        let images = images(backend, request, &declared)?;
        let references = declared
            .iter()
            .filter_map(|(service, declaration)| {
                declaration
                    .image
                    .as_ref()
                    .map(|reference| (service.clone(), reference.clone()))
            })
            .collect();
        Ok(Self {
            digest: render.digest().into(),
            project: request.project.into(),
            primary_service: request.primary_service.into(),
            workdir: request.workdir.into(),
            engine: backend.selection().engine.as_str().into(),
            context: backend.selection().context.clone(),
            images,
            references,
        })
    }

    /// The primary reference from this exact frozen render and backend.
    pub fn primary_image_reference<'a>(
        &'a self,
        request: CreationRequest<'_>,
        backend: &mut dyn EnvironmentBackend,
    ) -> Result<&'a str, CreationRefusal> {
        if std::time::Instant::now() >= request.deadline {
            return Err(CreationRefusal::Unknown(
                "preflight deadline expired".into(),
            ));
        }
        let render = request
            .render
            .ok_or_else(|| CreationRefusal::Unknown("preflight has no frozen render".into()))?;
        if self.digest != render.digest()
            || self.project != request.project
            || self.primary_service != request.primary_service
            || self.workdir != request.workdir
            || self.engine != backend.selection().engine.as_str()
            || self.context != backend.selection().context
        {
            return Err(CreationRefusal::ImageMismatch(
                "primary image preflight differs from the frozen plan".into(),
            ));
        }
        let reference = self
            .references
            .get(request.primary_service)
            .map(String::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                CreationRefusal::ImageMismatch("primary image reference is unavailable".into())
            })?;
        let current = match backend.image_id_until(request.workdir, reference, request.deadline) {
            Observed::Verified(id, _) => exact_image_id(&id),
            _ => None,
        };
        if current.as_ref() != self.images.get(request.primary_service) || current.is_none() {
            return Err(CreationRefusal::ImageMismatch(
                "primary image changed since preflight".into(),
            ));
        }
        if std::time::Instant::now() >= request.deadline {
            return Err(CreationRefusal::Unknown(
                "preflight deadline expired".into(),
            ));
        }
        Ok(reference)
    }
}

#[derive(Debug)]
pub(crate) struct CreationWindow {
    declared: BTreeMap<String, DeclaredService>,
    before: Vec<Workload>,
    expected: ExpectedInventory,
    images: BTreeMap<String, String>,
}

#[derive(Debug)]
pub(crate) struct InventoryDifference {
    pub expected_created: BTreeMap<String, Option<String>>,
    pub workspace_predecessor: Option<String>,
    pub image_identities: BTreeMap<String, ImageIdentity>,
    pub reference_resolutions: BTreeMap<String, String>,
    pub compose_files: Vec<std::path::PathBuf>,
    pub created: Vec<CreatedContainer>,
    pub attached: Vec<String>,
    pub observed: Vec<Workload>,
}

impl CreationWindow {
    pub(crate) fn open(
        backend: &mut dyn EnvironmentBackend,
        request: CreationRequest<'_>,
    ) -> Result<Self, CreationRefusal> {
        if std::time::Instant::now() >= request.deadline {
            return Err(CreationRefusal::Unknown(
                "inventory deadline expired".into(),
            ));
        }
        let render = request.render.ok_or_else(|| {
            CreationRefusal::Unknown("no frozen render declares the expected inventory".into())
        })?;
        let declared = declared_services(render, request.project)
            .map_err(|reason| CreationRefusal::Unknown(reason.to_string()))?;
        if declared.is_empty() || !declared.contains_key(request.primary_service) {
            return Err(CreationRefusal::Partial(
                "frozen render declares no workspace service".into(),
            ));
        }
        let images = images(backend, request, &declared)?;
        if let Some(preflight) = request.preflight {
            if preflight.digest != render.digest()
                || preflight.project != request.project
                || preflight.primary_service != request.primary_service
                || preflight.workdir != request.workdir
                || preflight.engine != backend.selection().engine.as_str()
                || preflight.context != backend.selection().context
                || preflight.images != images
            {
                return Err(CreationRefusal::ImageMismatch(
                    "preflight identities differ under the activation gate".into(),
                ));
            }
        }
        let before =
            rows(backend.inventory_until(request.workdir, request.project, request.deadline))?;
        validate(backend, request, &declared, &images, &before, false)?;
        let expected = ExpectedInventory::observe(backend, request, &declared, &before)?;
        Ok(Self {
            declared,
            before,
            expected,
            images,
        })
    }

    pub(crate) fn close(
        self,
        backend: &mut dyn EnvironmentBackend,
        request: CreationRequest<'_>,
    ) -> Result<InventoryDifference, CreationRefusal> {
        let after =
            rows(backend.inventory_until(request.workdir, request.project, request.deadline))?;
        let images = images(backend, request, &self.declared)?;
        if images != self.images {
            return Err(CreationRefusal::ImageMismatch(
                "declared image identity changed during apply".into(),
            ));
        }
        validate(backend, request, &self.declared, &images, &after, true)?;
        self.expected
            .verify(backend, request, &self.before, &after)?;
        if std::time::Instant::now() >= request.deadline {
            return Err(CreationRefusal::Unknown(
                "inventory deadline expired".into(),
            ));
        }
        let before: BTreeMap<_, _> = self
            .before
            .iter()
            .map(|row| (row.id.to_ascii_lowercase(), row))
            .collect();
        let hashes = if after.iter().any(|row| {
            ImageIdentity::needs_attached_config(
                backend,
                row,
                !before.contains_key(&row.id.to_ascii_lowercase()),
            )
        }) {
            frozen_config_hashes(
                backend,
                request.workdir,
                request.project,
                request.compose_files,
                &self.declared.keys().cloned().collect(),
                request.deadline,
            )?
        } else {
            BTreeMap::new()
        };
        let image_identities: BTreeMap<_, _> = after
            .iter()
            .map(|row| {
                let service = &row.labels[COMPOSE_SERVICE_LABEL];
                let reference = self.declared[service]
                    .image
                    .as_deref()
                    .ok_or_else(|| CreationRefusal::ImageMismatch(service.clone()))?;
                ImageIdentity::observe(
                    backend,
                    row,
                    reference,
                    &images[service],
                    !before.contains_key(&row.id.to_ascii_lowercase()),
                    hashes.get(service).map(String::as_str),
                )
                .map(|identity| (service.clone(), identity))
            })
            .collect::<Result<_, _>>()?;
        let mut created = Vec::new();
        let mut attached = Vec::new();
        for row in after.clone() {
            if let Some(previous) = before.get(&row.id.to_ascii_lowercase()) {
                if previous.name != row.name
                    || previous.labels != row.labels
                    || previous.image != row.image
                    || previous.image_reference != row.image_reference
                {
                    return Err(CreationRefusal::Unknown(
                        "an attached inventory row changed identity".into(),
                    ));
                }
                attached.push(row.id);
            } else {
                let service = row.labels[COMPOSE_SERVICE_LABEL].clone();
                created.push(CreatedContainer {
                    container_id: row.id,
                    name: row.name,
                    service: service.clone(),
                    image_id: images[&service].clone(),
                    image_reference: row.image_reference,
                    image_capability: image_identities[&service].capability(),
                });
            }
        }
        Ok(InventoryDifference {
            expected_created: self.expected.created,
            workspace_predecessor: self.expected.workspace_predecessor,
            image_identities,
            reference_resolutions: self
                .declared
                .iter()
                .map(|(service, expected)| {
                    expected
                        .image
                        .clone()
                        .map(|reference| (reference, images[service].clone()))
                        .ok_or_else(|| CreationRefusal::ImageMismatch(service.clone()))
                })
                .collect::<Result<_, _>>()?,
            compose_files: request.compose_files.to_vec(),
            created,
            attached,
            observed: after,
        })
    }
}

fn rows(observed: Observed<Vec<Workload>>) -> Result<Vec<Workload>, CreationRefusal> {
    match observed {
        Observed::Verified(rows, _) => Ok(rows),
        Observed::Absent(proof) => Err(CreationRefusal::Unknown(proof.to_string())),
        Observed::Unknown(reason) => Err(CreationRefusal::Unknown(reason.to_string())),
    }
}

fn images(
    backend: &mut dyn EnvironmentBackend,
    request: CreationRequest<'_>,
    declared: &BTreeMap<String, DeclaredService>,
) -> Result<BTreeMap<String, String>, CreationRefusal> {
    let mut images = BTreeMap::new();
    for (service, expected) in declared {
        let reference = expected.image.as_deref().ok_or_else(|| {
            CreationRefusal::ImageMismatch(format!(
                "service {service} has no frozen image reference"
            ))
        })?;
        let id = match backend.image_id_until(request.workdir, reference, request.deadline) {
            Observed::Verified(id, _) => exact_image_id(&id)
                .ok_or_else(|| CreationRefusal::ReferenceOnlyImage(reference.into()))?,
            Observed::Absent(proof) => return Err(CreationRefusal::Unknown(proof.to_string())),
            Observed::Unknown(reason) => return Err(CreationRefusal::Unknown(reason.to_string())),
        };
        images.insert(service.clone(), id);
    }
    Ok(images)
}

fn validate(
    backend: &dyn EnvironmentBackend,
    request: CreationRequest<'_>,
    declared: &BTreeMap<String, DeclaredService>,
    images: &BTreeMap<String, String>,
    rows: &[Workload],
    complete: bool,
) -> Result<(), CreationRefusal> {
    let mut ids = BTreeSet::new();
    let mut services = BTreeSet::new();
    for row in rows {
        if !super::super::backend::full_container_id(&row.id)
            || !ids.insert(row.id.to_ascii_lowercase())
        {
            return Err(CreationRefusal::Missing(
                "inventory contains a short or duplicate ID".into(),
            ));
        }
        let service = row
            .labels
            .get(COMPOSE_SERVICE_LABEL)
            .ok_or_else(|| CreationRefusal::Service(row.name.clone()))?;
        if !services.insert(service.clone()) {
            return Err(CreationRefusal::DuplicateService(service.clone()));
        }
        let expected = declared
            .get(service)
            .ok_or_else(|| CreationRefusal::Service(service.clone()))?;
        if row.labels.get(COMPOSE_PROJECT_LABEL).map(String::as_str) != Some(request.project) {
            return Err(CreationRefusal::Service(format!(
                "{} has another Compose project",
                row.name
            )));
        }
        match row.labels.get(COMPOSE_ONEOFF_LABEL) {
            Some(value) if value.eq_ignore_ascii_case("false") => {}
            None if backend.selection().kind == super::super::backend::BackendKind::Colima
                && expected
                    .networks
                    .as_ref()
                    .is_some_and(|networks| network::corroborates(row, networks)) => {}
            None => {
                return Err(CreationRefusal::Unknown(format!(
                "{} has no Compose one-off flag and no matching nerdctl name/network membership",
                row.name
            )))
            }
            Some(_) => return Err(CreationRefusal::OneOff(row.name.clone())),
        }
        if expected.labels.get(MANAGED_LABEL).map(String::as_str) != Some("true")
            || expected.labels.get(PROJECT_LABEL).map(String::as_str) != Some(request.project)
        {
            return Err(CreationRefusal::Service(
                "frozen ownership is not this managed project".into(),
            ));
        }
        for label in OWNERSHIP_LABELS {
            let value = expected
                .labels
                .get(label)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| CreationRefusal::Service(format!("render omits {label}")))?;
            if row.labels.get(label) != Some(value) {
                return Err(CreationRefusal::Service(format!(
                    "container {} mismatches {label}",
                    row.id
                )));
            }
        }
        let prefix = format!("{}-{service}-", request.project);
        if !row
            .name
            .strip_prefix(&prefix)
            .and_then(|index| index.parse::<u32>().ok())
            .is_some_and(|index| index > 0)
        {
            return Err(CreationRefusal::Service(row.name.clone()));
        }
        let reference = expected
            .image
            .as_deref()
            .ok_or_else(|| CreationRefusal::ImageMismatch(row.name.clone()))?;
        let same = same_reference(&row.image_reference, reference);
        if row.image_reference.is_empty() || (complete && !same) {
            return Err(CreationRefusal::ImageMismatch(row.name.clone()));
        }
        match exact_image_id(&row.image) {
            Some(id) if same && images.get(service) != Some(&id) => {
                return Err(CreationRefusal::ImageMismatch(row.name.clone()))
            }
            Some(_) => {}
            None if backend.selection().kind == super::super::backend::BackendKind::Colima
                && same_reference(&row.image, &row.image_reference) => {}
            None => return Err(CreationRefusal::ReferenceOnlyImage(row.name.clone())),
        }
    }
    if complete && services.len() != declared.len() {
        return Err(CreationRefusal::Partial(
            "post-apply inventory does not equal the frozen service set".into(),
        ));
    }
    Ok(())
}

fn inconsistent(detail: String) -> UnknownReason {
    UnknownReason::Inconsistent(detail)
}

/// The exact values the admitted render declares for one service.
#[derive(Debug, Default)]
struct DeclaredService {
    /// The ownership labels the render itself writes for the service.
    labels: BTreeMap<String, String>,
    /// The image reference the render declares for the service.
    image: Option<String>,
    networks: Option<BTreeSet<String>>,
}

/// Read the render's own declarations: which services it declares, the
/// ownership labels it writes for each and the image reference it names.
///
/// The render is the one authority for these values. A value declared twice
/// with different spellings refuses, because the window cannot then say which
/// one the apply used.
fn declared_services(
    render: &Closure,
    project: &str,
) -> Result<BTreeMap<String, DeclaredService>, UnknownReason> {
    let mut declared: BTreeMap<String, DeclaredService> = BTreeMap::new();
    let selected: Option<Vec<String>> = render
        .files()
        .get("inputs/environment.json")
        .map(|metadata| {
            let metadata: serde_json::Value = serde_json::from_slice(metadata)
                .map_err(|error| inconsistent(error.to_string()))?;
            serde_json::from_value(metadata.get("compose_files").cloned().ok_or_else(|| {
                inconsistent("frozen render has no Compose file inventory".into())
            })?)
            .map_err(|error| inconsistent(error.to_string()))
        })
        .transpose()?;
    let files = match selected {
        Some(paths) => paths
            .iter()
            .map(|path| {
                render
                    .files()
                    .get_key_value(path)
                    .ok_or_else(|| inconsistent(format!("frozen Compose file {path} is missing")))
            })
            .collect::<Result<Vec<_>, _>>()?,
        None => render.files().iter().collect(),
    };
    let mut networks = network::Effective::default();
    for (path, bytes) in files {
        let lower = path.to_ascii_lowercase();
        if !(lower.ends_with(".yml") || lower.ends_with(".yaml")) {
            continue;
        }
        let document: serde_yaml::Value = serde_yaml::from_slice(bytes).map_err(|error| {
            inconsistent(format!(
                "the admitted render file `{path}` is not Compose YAML: {error}"
            ))
        })?;
        networks.include(&document);
        let Some(services) = document
            .get("services")
            .and_then(|value| value.as_mapping())
        else {
            continue;
        };
        for (name, body) in services {
            let Some(name) = name.as_str() else { continue };
            let Some(body) = body.as_mapping() else {
                continue;
            };
            let service = declared.entry(name.to_owned()).or_default();
            if let Some(labels) = body.get("labels") {
                for (key, value) in label_pairs(labels)? {
                    if !OWNERSHIP_LABELS.contains(&key.as_str()) {
                        continue;
                    }
                    match service.labels.get(&key) {
                        Some(existing) if existing != &value => {
                            return Err(inconsistent(format!(
                                "the admitted render declares `{key}` as both `{existing}` and `{value}` for service `{name}`"
                            )))
                        }
                        Some(_) => {}
                        None => {
                            service.labels.insert(key, value);
                        }
                    }
                }
            }
            if let Some(image) = body.get("image").and_then(|value| value.as_str()) {
                match &service.image {
                    Some(existing) if existing != image => {
                        return Err(inconsistent(format!(
                            "the admitted render declares image `{existing}` and `{image}` for service `{name}`"
                        )))
                    }
                    Some(_) => {}
                    None => service.image = Some(image.to_owned()),
                }
            }
        }
    }
    for (name, service) in &mut declared {
        service.networks = networks.for_service(name, project).ok();
    }
    Ok(declared)
}

/// Every `key=value` pair a Compose `labels:` value declares, as a mapping or
/// as a list of `key=value` strings.
fn label_pairs(value: &serde_yaml::Value) -> Result<Vec<(String, String)>, UnknownReason> {
    let mut pairs = Vec::new();
    match value {
        serde_yaml::Value::Mapping(mapping) => {
            for (key, value) in mapping {
                let Some(key) = key.as_str() else { continue };
                let Some(value) = yaml_scalar(value) else {
                    continue;
                };
                pairs.push((key.to_owned(), value));
            }
        }
        serde_yaml::Value::Sequence(entries) => {
            for entry in entries {
                let Some(text) = entry.as_str() else { continue };
                let Some((key, value)) = text.split_once('=') else {
                    continue;
                };
                pairs.push((key.to_owned(), value.to_owned()));
            }
        }
        _ => {}
    }
    Ok(pairs)
}

fn yaml_scalar(value: &serde_yaml::Value) -> Option<String> {
    match value {
        serde_yaml::Value::String(text) => Some(text.clone()),
        serde_yaml::Value::Bool(flag) => Some(flag.to_string()),
        serde_yaml::Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}
