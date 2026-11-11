//! Image evidence names what the selected backend can actually observe.
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageCapability {
    ContainerImageId,
    ReferenceResolvedId,
    DigestReference,
    AttachedReferenceConfigHash,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageIdentity {
    rendered_reference: String,
    observed_reference: String,
    resolved_id: Option<String>,
    config_hash: Option<String>,
    capability: ImageCapability,
}

impl ImageIdentity {
    pub fn rendered_reference(&self) -> &str {
        &self.rendered_reference
    }
    pub fn observed_reference(&self) -> &str {
        &self.observed_reference
    }
    pub fn resolved_id(&self) -> Option<&str> {
        self.resolved_id.as_deref()
    }
    pub fn config_hash(&self) -> Option<&str> {
        self.config_hash.as_deref()
    }
    pub(super) fn needs_attached_config(
        backend: &dyn EnvironmentBackend,
        row: &Workload,
        created: bool,
    ) -> bool {
        !created
            && backend.selection().kind == super::super::backend::BackendKind::Colima
            && exact_image_id(&row.image).is_none()
            && !row.image_reference.contains('@')
    }
    pub fn capability(&self) -> ImageCapability {
        self.capability
    }

    pub(super) fn observe(
        backend: &mut dyn EnvironmentBackend,
        row: &Workload,
        reference: &str,
        resolved_id: &str,
        created_in_window: bool,
        rendered_config_hash: Option<&str>,
        observation: (&std::path::Path, std::time::Instant),
    ) -> Result<Self, CreationRefusal> {
        let resolved_id = exact_image_id(resolved_id)
            .ok_or_else(|| CreationRefusal::ReferenceOnlyImage(reference.into()))?;
        if !same_reference(reference, &row.image_reference)
            && !digest_reference_binds(reference, &row.image_reference)
        {
            // Nerdctl can replace even a digest reference with a local tag in
            // both inspect image fields. Only a new ID in this guarded create
            // window may bind that spelling through an independent lookup.
            // Attached rows never gain image identity from today's tag table.
            let alias = if created_in_window
                && backend.selection().kind == super::super::backend::BackendKind::Colima
                && same_reference(&row.image, &row.image_reference)
            {
                match backend.image_id_until(observation.0, &row.image_reference, observation.1) {
                    Observed::Verified(id, _) => exact_image_id(&id),
                    _ => None,
                }
            } else {
                None
            };
            if alias.as_ref() != Some(&resolved_id) {
                return Err(CreationRefusal::ImageMismatch(row.id.clone()));
            }
        }
        let capability = match exact_image_id(&row.image) {
            Some(id) if id == resolved_id => ImageCapability::ContainerImageId,
            Some(_) => return Err(CreationRefusal::ImageMismatch(row.id.clone())),
            None if backend.selection().kind == super::super::backend::BackendKind::Colima
                && same_reference(&row.image, &row.image_reference) =>
            {
                if same_reference(reference, &row.image_reference)
                    && reference
                        .rsplit_once('@')
                        .is_some_and(|(_, digest)| exact_image_id(digest).is_some())
                {
                    ImageCapability::DigestReference
                } else if created_in_window {
                    ImageCapability::ReferenceResolvedId
                } else {
                    let expected = rendered_config_hash
                        .filter(|hash| hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()))
                        .ok_or_else(|| CreationRefusal::Unknown(
                            "attached reference-only image requires the frozen Compose config hash".into(),
                        ))?;
                    if row
                        .labels
                        .get("com.docker.compose.config-hash")
                        .map(String::as_str)
                        != Some(expected)
                    {
                        return Err(CreationRefusal::ImageMismatch(format!(
                            "{} config hash differs from render",
                            row.id
                        )));
                    }
                    ImageCapability::AttachedReferenceConfigHash
                }
            }
            None => return Err(CreationRefusal::ReferenceOnlyImage(row.id.clone())),
        };
        Ok(Self {
            rendered_reference: reference.into(),
            observed_reference: row.image_reference.clone(),
            resolved_id: (capability != ImageCapability::AttachedReferenceConfigHash)
                .then_some(resolved_id),
            config_hash: if capability == ImageCapability::AttachedReferenceConfigHash {
                rendered_config_hash.map(str::to_owned)
            } else {
                None
            },
            capability,
        })
    }
}

pub(super) fn same_reference(left: &str, right: &str) -> bool {
    match (canonical_reference(left), canonical_reference(right)) {
        (Some(left), Some(right)) => left == right,
        _ => false,
    }
}

/// A bare-digest render names content, never a repository. When the frozen
/// reference is `sha256:<digest>`, an engine may report the same immutable
/// digest under the repository spelling it imported the image with (nerdctl
/// answers `import@sha256:<digest>` for an OCI layout it loaded without a
/// reference name). That is the same content: the container's own image id is
/// still checked against this backend's lookup for the declared digest, so the
/// binding stays content-addressed. A tag, another digest, or a reference-only
/// spelling of this form for a tagged render never binds.
pub(super) fn digest_reference_binds(declared: &str, reported: &str) -> bool {
    let Some(declared) = exact_image_id(declared) else {
        return false;
    };
    reported
        .split_once('@')
        .map(|(_, digest)| digest)
        .and_then(exact_image_id)
        .is_some_and(|reported| reported == declared)
}

/// Expand only Docker's familiar-name spelling, never a different repository,
/// tag or digest. See distribution/reference's ParseNormalizedNamed/TagNameOnly.
fn canonical_reference(reference: &str) -> Option<String> {
    if reference.is_empty() || reference.chars().any(char::is_whitespace) {
        return None;
    }
    if let Some(id) = exact_image_id(reference) {
        return Some(id);
    }
    let (name, digest) = match reference.split_once('@') {
        Some((name, digest)) if digest.starts_with("sha256:") => {
            (name, Some(exact_image_id(digest)?))
        }
        Some(_) => return None,
        None => (reference, None),
    };
    let (domain, mut path) = match name.split_once('/') {
        Some((first, rest))
            if first == "localhost" || first.contains('.') || first.contains(':') =>
        {
            (first, rest.to_owned())
        }
        _ => ("docker.io", name.to_owned()),
    };
    let domain = if domain == "index.docker.io" {
        "docker.io"
    } else {
        domain
    };
    if path.is_empty() || path.contains('@') || path.starts_with('/') {
        return None;
    }
    if domain == "docker.io" && !path.contains('/') {
        path = format!("library/{path}");
    }
    if digest.is_none() && !path.rsplit('/').next()?.contains(':') {
        path.push_str(":latest");
    }
    Some(match digest {
        Some(digest) => format!("{domain}/{path}@{digest}"),
        None => format!("{domain}/{path}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn familiar_references_preserve_repository_tag_registry_and_digest() {
        for (left, right) in [
            ("alpine", "docker.io/library/alpine:latest"),
            ("alpine:3.19", "docker.io/library/alpine:3.19"),
            (
                "index.docker.io/alpine:3.19",
                "docker.io/library/alpine:3.19",
            ),
            ("team/app", "docker.io/team/app:latest"),
            ("localhost:5000/app", "localhost:5000/app:latest"),
        ] {
            assert!(same_reference(left, right), "{left}: {right}");
        }
        for (left, right) in [
            ("alpine:3.19", "alpine:latest"),
            ("alpine:3.19", "foreign/alpine:3.19"),
            ("localhost:5000/app", "localhost:5001/app"),
            ("", ""),
            (" alpine", "alpine"),
            ("alpine@sha256:short", "alpine@sha256:short"),
        ] {
            assert!(!same_reference(left, right), "{left}: {right}");
        }
        let digest = "a".repeat(64);
        assert!(!same_reference(
            &format!("sha256:{digest}"),
            &format!("docker.io/library/sha256:{digest}")
        ));
        assert!(same_reference(
            &format!("alpine@sha256:{digest}"),
            &format!("docker.io/library/alpine@sha256:{digest}")
        ));
        assert!(!same_reference(
            &format!("alpine@sha256:{digest}"),
            "alpine"
        ));
    }
}
