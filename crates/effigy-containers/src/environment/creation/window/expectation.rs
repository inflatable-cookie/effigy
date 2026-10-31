//! An expectation fixed before apply, independent of the resulting ID set.
use super::*;

#[derive(Debug)]
pub(super) struct ExpectedInventory {
    pub(super) created: BTreeMap<String, Option<String>>,
    attached: BTreeMap<String, String>,
    previous_images: BTreeMap<String, String>,
}
impl ExpectedInventory {
    pub(super) fn observe(
        backend: &mut dyn EnvironmentBackend,
        request: CreationRequest<'_>,
        declared: &BTreeMap<String, DeclaredService>,
        before: &[Workload],
    ) -> Result<Self, CreationRefusal> {
        let mut expected = Self {
            created: BTreeMap::new(),
            attached: BTreeMap::new(),
            previous_images: BTreeMap::new(),
        };
        for (service, desired) in declared {
            let previous = before
                .iter()
                .find(|row| row.labels.get(COMPOSE_SERVICE_LABEL) == Some(service));
            match previous {
                None => {
                    expected.created.insert(service.clone(), None);
                }
                Some(row) if desired.image.as_deref() != Some(row.image_reference.as_str()) => {
                    // An explicitly changed image reference is a verifiable
                    // recreation input. The old container must still match
                    // the immutable identity of its own reference; a retag is
                    // uncertainty, even when replacement was requested.
                    let resolved = match backend.image_id_until(
                        request.workdir,
                        &row.image_reference,
                        request.deadline,
                    ) {
                        Observed::Verified(id, _) => exact_image_id(&id),
                        _ => None,
                    };
                    if resolved.is_none() || resolved != exact_image_id(&row.image) {
                        return Err(CreationRefusal::ImageMismatch(row.id.clone()));
                    }
                    expected
                        .previous_images
                        .insert(row.image_reference.clone(), row.image.clone());
                    expected
                        .created
                        .insert(service.clone(), Some(row.id.clone()));
                }
                Some(row) => {
                    expected.attached.insert(service.clone(), row.id.clone());
                }
            }
        }
        Ok(expected)
    }

    pub(super) fn verify(
        &self,
        backend: &mut dyn EnvironmentBackend,
        request: CreationRequest<'_>,
        before: &[Workload],
        after: &[Workload],
    ) -> Result<(), CreationRefusal> {
        for (reference, image) in &self.previous_images {
            let resolved =
                match backend.image_id_until(request.workdir, reference, request.deadline) {
                    Observed::Verified(id, _) => exact_image_id(&id),
                    _ => None,
                };
            if resolved.is_none() || resolved != exact_image_id(image) {
                return Err(CreationRefusal::ImageMismatch(reference.clone()));
            }
        }
        let previous: BTreeSet<_> = before
            .iter()
            .map(|row| row.id.to_ascii_lowercase())
            .collect();
        let current: BTreeMap<_, _> = after
            .iter()
            .map(|row| (row.labels[COMPOSE_SERVICE_LABEL].clone(), row))
            .collect();
        for (service, id) in &self.attached {
            if current
                .get(service)
                .is_none_or(|row| !row.id.eq_ignore_ascii_case(id))
            {
                return Err(CreationRefusal::Partial(format!(
                    "attached service {service} was unexpectedly replaced"
                )));
            }
        }
        let mut expected_new = BTreeSet::new();
        for service in self.created.keys() {
            let row = current.get(service).ok_or_else(|| {
                CreationRefusal::Partial(format!("expected creation for {service} is missing"))
            })?;
            if previous.contains(&row.id.to_ascii_lowercase()) {
                return Err(CreationRefusal::Partial(format!(
                    "expected creation for {service} reused a pre-apply ID"
                )));
            }
            expected_new.insert(row.id.to_ascii_lowercase());
        }
        let actual_new: BTreeSet<_> = after
            .iter()
            .map(|row| row.id.to_ascii_lowercase())
            .filter(|id| !previous.contains(id))
            .collect();
        if actual_new != expected_new {
            return Err(CreationRefusal::Partial(
                "new IDs differ from the pre-apply expected creation/recreation set".into(),
            ));
        }
        Ok(())
    }
}
