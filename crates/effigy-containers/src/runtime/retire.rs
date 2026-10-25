//! Runtime resources a checkout owns, found by label.
//!
//! There is no plan and no record: retirement deletes every labelled
//! container, volume and network the runtime reports and then says exactly what
//! it removed, what it kept on purpose and what it could not delete.

use std::collections::BTreeMap;

use crate::runtime::scope::PERSIST_LABEL;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ResourceKind {
    Container,
    Volume,
    Network,
}

impl ResourceKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Container => "container",
            Self::Volume => "volume",
            Self::Network => "network",
        }
    }
}

/// One labelled runtime object this checkout owns.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct OwnedResource {
    pub kind: ResourceKind,
    pub name: String,
    /// Backend profile the object lives on.
    pub profile: String,
    /// Flagship data volume the runtime marks `com.effigy.persist=true`.
    pub persist: bool,
}

impl OwnedResource {
    /// Whether a backend's labels mark this object as kept data.
    pub fn labels_mark_persistent(labels: &BTreeMap<String, String>) -> bool {
        labels
            .get(PERSIST_LABEL)
            .is_some_and(|value| value.eq_ignore_ascii_case("true"))
    }
}

/// Sort and de-duplicate one backend's labelled objects.
pub fn unique_resources(mut found: Vec<OwnedResource>) -> Vec<OwnedResource> {
    found.sort_by(|left, right| {
        left.kind
            .cmp(&right.kind)
            .then(left.name.cmp(&right.name))
            .then(left.profile.cmp(&right.profile))
    });
    found.dedup_by(|left, right| {
        left.kind == right.kind && left.name == right.name && left.profile == right.profile
    });
    found
}

/// A resource retirement deliberately left in place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeptResource {
    pub resource: OwnedResource,
    pub reason: &'static str,
}

/// A resource retirement could not delete, with the runtime's own error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailedResource {
    pub resource: OwnedResource,
    pub error: String,
}

/// Gateway state released for the checkout path, by checkout path alone.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GatewayRelease {
    pub routes: Vec<String>,
    /// Loopback aliases released for the checkout path.
    pub loopbacks: usize,
}

/// Everything one retirement did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RetireOutcome {
    pub removed: Vec<OwnedResource>,
    pub kept: Vec<KeptResource>,
    pub failed: Vec<FailedResource>,
    pub gateway: GatewayRelease,
}

impl RetireOutcome {
    /// Clean only when nothing that should have gone is still there.
    pub fn ok(&self) -> bool {
        self.failed.is_empty()
    }

    pub fn idempotent(&self) -> bool {
        self.removed.is_empty() && self.failed.is_empty()
    }

    /// What is still there after this retirement, in report order.
    pub fn remaining(&self) -> Vec<&FailedResource> {
        self.failed.iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resource(kind: ResourceKind, name: &str, persist: bool) -> OwnedResource {
        OwnedResource {
            kind,
            name: name.to_owned(),
            profile: "effigy".to_owned(),
            persist,
        }
    }

    #[test]
    fn unique_resources_sorts_and_dedupes_per_backend() {
        let found = unique_resources(vec![
            resource(ResourceKind::Volume, "b", false),
            resource(ResourceKind::Container, "web", false),
            resource(ResourceKind::Volume, "b", false),
            resource(ResourceKind::Volume, "a", false),
        ]);
        assert_eq!(
            found,
            vec![
                resource(ResourceKind::Container, "web", false),
                resource(ResourceKind::Volume, "a", false),
                resource(ResourceKind::Volume, "b", false),
            ]
        );
    }

    #[test]
    fn a_failed_deletion_is_never_reported_clean() {
        let mut outcome = RetireOutcome::default();
        outcome
            .removed
            .push(resource(ResourceKind::Volume, "data", false));
        outcome.failed.push(FailedResource {
            resource: resource(ResourceKind::Volume, "kept-by-runtime", false),
            error: "volume is in use".to_owned(),
        });
        assert!(!outcome.ok());
        assert!(!outcome.idempotent());
        assert_eq!(outcome.remaining().len(), 1);
        assert!(RetireOutcome::default().ok());
        assert!(RetireOutcome::default().idempotent());
    }

    #[test]
    fn persist_label_marks_kept_data() {
        let mut labels = BTreeMap::new();
        labels.insert(PERSIST_LABEL.to_owned(), "true".to_owned());
        assert!(OwnedResource::labels_mark_persistent(&labels));
        labels.insert(PERSIST_LABEL.to_owned(), "false".to_owned());
        assert!(!OwnedResource::labels_mark_persistent(&labels));
    }
}
