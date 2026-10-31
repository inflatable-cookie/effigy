//! Image evidence names what the selected backend can actually observe.
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageCapability {
    ContainerImageId,
    ReferenceResolvedId,
    DigestReference,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageIdentity {
    rendered_reference: String,
    observed_reference: String,
    resolved_id: String,
    capability: ImageCapability,
}

impl ImageIdentity {
    pub fn rendered_reference(&self) -> &str {
        &self.rendered_reference
    }
    pub fn observed_reference(&self) -> &str {
        &self.observed_reference
    }
    pub fn resolved_id(&self) -> &str {
        &self.resolved_id
    }
    pub fn capability(&self) -> ImageCapability {
        self.capability
    }

    pub(super) fn observe(
        backend: &dyn EnvironmentBackend,
        row: &Workload,
        reference: &str,
        resolved_id: &str,
    ) -> Result<Self, CreationRefusal> {
        if !same_reference(reference, &row.image_reference) {
            return Err(CreationRefusal::ImageMismatch(row.id.clone()));
        }
        let resolved_id = exact_image_id(resolved_id)
            .ok_or_else(|| CreationRefusal::ReferenceOnlyImage(reference.into()))?;
        let capability = match exact_image_id(&row.image) {
            Some(id) if id == resolved_id => ImageCapability::ContainerImageId,
            Some(_) => return Err(CreationRefusal::ImageMismatch(row.id.clone())),
            None if backend.selection().kind == super::super::backend::BackendKind::Colima
                && same_reference(&row.image, reference) =>
            {
                if reference
                    .rsplit_once('@')
                    .is_some_and(|(_, digest)| exact_image_id(digest).is_some())
                {
                    ImageCapability::DigestReference
                } else {
                    ImageCapability::ReferenceResolvedId
                }
            }
            None => return Err(CreationRefusal::ReferenceOnlyImage(row.id.clone())),
        };
        Ok(Self {
            rendered_reference: reference.into(),
            observed_reference: row.image_reference.clone(),
            resolved_id,
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
