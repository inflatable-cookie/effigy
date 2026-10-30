//! Guest-local fields frozen before publication; the v2 parser validates every
//! field against these inputs rather than admitting arbitrary document data.
use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuestPaths {
    pub host_checkout: String,
    pub guest_checkout: String,
    pub workdir: String,
    pub user: String,
    pub home: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuestSourcePin {
    pub kind: String,
    pub name: String,
    pub digest: String,
    pub snapshot: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuestContextInputs {
    pub version: String,
    pub paths: GuestPaths,
    pub sources: Vec<GuestSourcePin>,
}

fn absolute(path: &str) -> bool {
    path.starts_with('/')
        && !path.contains('\0')
        && !path.split('/').any(|part| part == ".." || part == ".")
}
impl GuestContextInputs {
    pub fn validate(&self) -> Result<(), RunError> {
        if self.version != env!("CARGO_PKG_VERSION")
            || !absolute(&self.paths.host_checkout)
            || !absolute(&self.paths.guest_checkout)
            || !absolute(&self.paths.workdir)
            || !absolute(&self.paths.home)
            || self.paths.user.trim().is_empty()
            || self.paths.user.contains('\0')
        {
            return Err(ActivationRefusal::SelectionMismatch.into());
        }
        let mut seen = std::collections::BTreeSet::new();
        for pin in &self.sources {
            if !matches!(
                pin.kind.as_str(),
                "bundle" | "catalog" | "manifest" | "source-lock" | "composed-manifest"
            ) || pin.name.trim().is_empty()
                || !seen.insert((&pin.kind, &pin.name))
                || pin
                    .snapshot
                    .as_ref()
                    .is_some_and(|snapshot| !absolute(snapshot))
            {
                return Err(ActivationRefusal::SelectionMismatch.into());
            }
        }
        Ok(())
    }
}
