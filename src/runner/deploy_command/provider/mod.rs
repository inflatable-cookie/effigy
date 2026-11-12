use effigy_ui::steps::ProgressFrame;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use effigy_ui::steps::ProgressSource;
use serde::{Deserialize, Serialize};

use descriptor::{
    read_provider_descriptor, validate_capability_paths, validate_provider_name,
    DeployProviderDescriptor,
};
use crate::runner::error::RunnerError;

pub(in crate::runner::deploy_command) mod context;
pub(in crate::runner::deploy_command) mod descriptor;
pub(in crate::runner::deploy_command) mod phase;
pub(in crate::runner::deploy_command) mod report;
pub(in crate::runner::deploy_command) mod source;

use source::resolve_git_provider_source;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::runner::deploy_command) struct ManifestDeployProviderConfig {
    pub source: DeployProviderSource,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
pub(in crate::runner::deploy_command) enum DeployProviderSource {
    Path {
        dir: String,
    },
    Git {
        url: String,
        #[serde(default)]
        r#ref: Option<String>,
        /// Full commit SHA. A pinned provider is reused from the cache without
        /// any network access; a floating `ref` is only refreshed online.
        #[serde(default)]
        rev: Option<String>,
    },
    Oci {
        url: String,
    },
}

/// Whether resolving a provider package may use the network.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::runner::deploy_command) enum DeployProviderFetch {
    /// Use the cached checkout only: never clone or fetch. A working tree can
    /// still be materialized from objects the cache already holds.
    Offline,
    /// Refresh the cached checkout from its origin when it is not already at
    /// the requested commit.
    Online,
}

/// Where a resolved provider package came from, and exactly which commit ran.
#[derive(Debug, Clone, Serialize)]
pub(in crate::runner::deploy_command) struct DeployProviderProvenance {
    /// `path` or `git`.
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requested_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requested_rev: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    /// `path`, `cached` or `fetched`.
    pub resolution: String,
}

impl DeployProviderProvenance {
    pub(in crate::runner::deploy_command) fn path() -> Self {
        Self {
            kind: "path".to_owned(),
            url: None,
            requested_ref: None,
            requested_rev: None,
            commit: None,
            resolution: "path".to_owned(),
        }
    }

    /// Short human label for the resolved commit and how it was obtained.
    pub(in crate::runner::deploy_command) fn describe(&self) -> String {
        match (&self.commit, self.resolution.as_str()) {
            (Some(commit), "cached") => format!("cached checkout at commit {commit}"),
            (Some(commit), "fetched") => format!("fetched commit {commit}"),
            (Some(commit), _) => format!("commit {commit}"),
            (None, "path") => "path source".to_owned(),
            (None, resolution) => format!("unresolved {resolution} source"),
        }
    }
}

#[derive(Debug, Clone)]
pub(in crate::runner::deploy_command) struct DeployProviderPackage {
    pub root: PathBuf,
    pub descriptor: DeployProviderDescriptor,
    pub provenance: DeployProviderProvenance,
    /// Non-fatal source warnings, for example a floating provider ref.
    pub warnings: Vec<String>,
}

pub(in crate::runner::deploy_command) fn resolve_provider_package(
    repo_root: &Path,
    provider_name: &str,
    providers: &BTreeMap<String, ManifestDeployProviderConfig>,
    fetch: DeployProviderFetch,
) -> Result<Option<DeployProviderPackage>, RunnerError> {
    validate_provider_name(provider_name)?;
    let Some(config) = providers.get(provider_name) else {
        return Ok(None);
    };
    let step = match &config.source {
        DeployProviderSource::Git { url, r#ref, rev } => {
            let progress = provider_fetch_progress(
                provider_name,
                url,
                r#ref.as_deref(),
                rev.as_deref(),
                fetch,
            );
            Some(effigy_ui::steps::live_step(progress))
        }
        DeployProviderSource::Path { .. } | DeployProviderSource::Oci { .. } => None,
    };
    let result = resolve_provider_package_inner(repo_root, provider_name, config, fetch);
    let (ok, summary) = match &result {
        Ok(package) => (
            true,
            provider_resolved_summary(provider_name, &package.provenance),
        ),
        Err(error) => (
            false,
            format!(
                "could not resolve deploy provider `{provider_name}`: {}",
                error.human_error().what_failed()
            ),
        ),
    };
    match step {
        Some(step) if ok => step.succeed(&summary),
        Some(step) => step.fail(&summary),
        None => effigy_ui::steps::finish_line(ok, &summary),
    }
    result.map(Some)
}

/// The live state of a git provider fetch: the current action names the
/// policy in force, and the wait names the exact source and pin.
fn provider_fetch_progress(
    provider_name: &str,
    url: &str,
    reference: Option<&str>,
    rev: Option<&str>,
    fetch: DeployProviderFetch,
) -> ProgressSource {
    ProgressSource::new(
        ProgressFrame::new(resolving_provider_label(provider_name, fetch))
            .label(format!("Resolving {provider_name} provider"))
            .waiting_for(provider_source_awaiting(url, reference, rev)),
    )
}

/// What a git provider fetch waits on: `git source <url> at rev <sha>`, or the
/// ref, or the default branch when the source names neither.
fn provider_source_awaiting(url: &str, reference: Option<&str>, rev: Option<&str>) -> String {
    let pin = match (
        rev,
        reference.map(str::trim).filter(|value| !value.is_empty()),
    ) {
        (Some(rev), _) => format!("rev {rev}"),
        (None, Some(reference)) => format!("ref {reference}"),
        (None, None) => "the default branch".to_owned(),
    };
    format!("git source {url} at {pin}")
}

/// `resolving deploy provider acme (offline)`: the label says which source
/// policy is in force, because "offline" explains a cache miss.
pub(in crate::runner::deploy_command) fn resolving_provider_label(provider_name: &str, fetch: DeployProviderFetch) -> String {
    let mode = match fetch {
        DeployProviderFetch::Offline => "offline",
        DeployProviderFetch::Online => "online",
    };
    format!("resolving deploy provider {provider_name} ({mode})")
}

/// One line naming where the provider came from and exactly which commit ran.
pub(in crate::runner::deploy_command) fn provider_resolved_summary(
    provider_name: &str,
    provenance: &DeployProviderProvenance,
) -> String {
    format!(
        "resolved deploy provider `{provider_name}` from {}",
        provenance.describe()
    )
}

fn resolve_provider_package_inner(
    repo_root: &Path,
    provider_name: &str,
    config: &ManifestDeployProviderConfig,
    fetch: DeployProviderFetch,
) -> Result<DeployProviderPackage, RunnerError> {
    let mut warnings = Vec::new();
    let (root, provenance) = match &config.source {
        DeployProviderSource::Path { dir } => {
            (repo_root.join(dir), DeployProviderProvenance::path())
        }
        DeployProviderSource::Git { url, r#ref, rev } => resolve_git_provider_source(
            repo_root,
            provider_name,
            url,
            r#ref.as_deref(),
            rev.as_deref(),
            fetch,
            &mut warnings,
        )?,
        DeployProviderSource::Oci { url } => {
            return Err(RunnerError::task_invocation(format!(
                "deploy provider `{provider_name}` uses OCI source `{url}`, which Effigy cannot materialize yet\nnext: declare a `path` or `git` source for `{provider_name}` in `[deploy.providers]`"
            )));
        }
    };
    let descriptor = read_provider_descriptor(&root)?;
    if descriptor.provider.schema != "effigy.deploy-provider.v1" {
        return Err(RunnerError::task_invocation(format!(
            "deploy provider `{provider_name}` declares descriptor schema `{}`, not `effigy.deploy-provider.v1`\nnext: update the provider package at {} to the v1 descriptor",
            descriptor.provider.schema,
            root.display()
        )));
    }
    if descriptor.provider.name != provider_name {
        return Err(RunnerError::task_invocation(format!(
            "deploy provider `{provider_name}` resolved a descriptor that names itself `{}`\nnext: make `[deploy.providers.{provider_name}]` point at that provider's package, or fix the descriptor's `provider.name`",
            descriptor.provider.name
        )));
    }
    validate_capability_paths(&root, provider_name, &descriptor.capabilities)?;
    Ok(DeployProviderPackage {
        root,
        descriptor,
        provenance,
        warnings,
    })
}


#[cfg(test)]
mod tests {
    use super::{
        provider_fetch_progress, provider_resolved_summary, resolving_provider_label,
        DeployProviderFetch, DeployProviderProvenance,
    };
    use std::time::Duration;

    #[test]
    fn provider_resolution_names_the_provider_and_the_source_policy() {
        assert_eq!(
            resolving_provider_label("acme", DeployProviderFetch::Offline),
            "resolving deploy provider acme (offline)"
        );
        assert_eq!(
            resolving_provider_label("acme", DeployProviderFetch::Online),
            "resolving deploy provider acme (online)"
        );
    }

    #[test]
    fn the_resolution_line_names_the_exact_commit() {
        let provenance = DeployProviderProvenance {
            kind: "git".to_owned(),
            url: Some("https://example.test/acme.git".to_owned()),
            requested_ref: Some("main".to_owned()),
            requested_rev: None,
            commit: Some("1a2b3c4d".to_owned()),
            resolution: "cached".to_owned(),
        };
        assert_eq!(
            provider_resolved_summary("acme", &provenance),
            "resolved deploy provider `acme` from cached checkout at commit 1a2b3c4d"
        );
    }

    #[test]
    fn a_fetch_frame_names_the_exact_source_and_pin_as_time_passes() {
        let url = "https://example.test/acme.git";
        let pinned = provider_fetch_progress(
            "acme",
            url,
            Some("main"),
            Some("1a2b3c4d5e6f70819a2b3c4d5e6f70819a2b3c4d"),
            DeployProviderFetch::Offline,
        );
        let detail = pinned.detailed_frame_at(Duration::from_secs(5));
        assert!(detail.contains(url));
        assert!(detail.contains("1a2b3c4d5e6f70819a2b3c4d5e6f70819a2b3c4d"));
        // The producer keeps source evidence out of the concise terminal phrase.
        assert_eq!(
            pinned.frame_at(Duration::from_secs(5)),
            "Resolving acme provider · 5s"
        );
        let floating = provider_fetch_progress(
            "acme",
            url,
            Some(" main "),
            None,
            DeployProviderFetch::Online,
        );
        assert_eq!(
            floating.frame_at(Duration::from_secs(2)),
            "Resolving acme provider · 2s"
        );
    }
}
