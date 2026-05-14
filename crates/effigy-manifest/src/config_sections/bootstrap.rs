use crate::{ManifestTask, ManifestTaskLikeDefinition};

#[derive(Debug, Clone, serde::Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
pub struct ManifestBootstrapConfig {
    #[serde(default, alias = "setup")]
    pub run: Option<ManifestBootstrapRun>,
    #[serde(default)]
    pub start: Option<ManifestBootstrapStart>,
    #[serde(default)]
    pub submodules: Option<ManifestBootstrapSubmodulesPolicy>,
    #[serde(default)]
    pub children: Vec<ManifestBootstrapChildConfig>,
}

/// Selector(s) to run as the bootstrap start task.
///
/// Accepts a single selector string, or an array of entries where each
/// entry is either a selector string (`"dev"`) or a table form
/// (`{ task = "dev" }`). Mixed arrays are allowed, mirroring the
/// flexibility of `[bootstrap].run`. Arrays run sequentially in
/// declaration order; the first failure aborts the chain.
/// Backward-compatible with the original scalar shape.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(untagged)]
pub enum ManifestBootstrapStart {
    Single(String),
    Multiple(Vec<ManifestBootstrapStartEntry>),
}

/// One entry in a multi-selector `[bootstrap].start` array.
///
/// Either a bare selector string (`"dev"`) or a table form carrying a
/// `task` selector (`{ task = "dev" }`). Args still travel inside the
/// selector string itself (e.g. `"dev --foo bar"`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(untagged)]
pub enum ManifestBootstrapStartEntry {
    Selector(String),
    Table(ManifestBootstrapStartTable),
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestBootstrapStartTable {
    pub task: String,
}

impl ManifestBootstrapStartEntry {
    pub fn selector(&self) -> &str {
        match self {
            Self::Selector(selector) => selector.as_str(),
            Self::Table(table) => table.task.as_str(),
        }
    }
}

impl ManifestBootstrapStart {
    /// Selectors in declaration order.
    pub fn selectors(&self) -> Vec<&str> {
        match self {
            Self::Single(selector) => vec![selector.as_str()],
            Self::Multiple(entries) => entries.iter().map(|entry| entry.selector()).collect(),
        }
    }

    /// Owned copy of all selectors in declaration order.
    pub fn to_owned_selectors(&self) -> Vec<String> {
        self.selectors().into_iter().map(String::from).collect()
    }
}

#[derive(Debug, serde::Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ManifestBootstrapSubmodulesPolicy {
    None,
    Init,
    Recursive,
}

#[derive(Debug, Clone, serde::Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
pub struct ManifestBootstrapChildConfig {
    pub path: String,
    pub repo: String,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default, alias = "setup")]
    pub run: Option<ManifestBootstrapRun>,
    #[serde(default = "default_bootstrap_child_required")]
    pub required: bool,
}

fn default_bootstrap_child_required() -> bool {
    true
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(transparent)]
pub struct ManifestBootstrapRun(ManifestTaskLikeDefinition);

impl ManifestBootstrapRun {
    pub fn into_manifest_task(self) -> ManifestTask {
        self.0.into_manifest_task()
    }

    pub fn as_manifest_task(&self) -> ManifestTask {
        self.0.as_manifest_task()
    }
}

#[cfg(test)]
mod tests {
    use super::ManifestBootstrapConfig;
    use crate::{ManifestManagedRun, ManifestManagedRunStep, ManifestTaskRunIn};

    #[test]
    fn bootstrap_run_accepts_compact_inline_task_run_in() {
        let bootstrap: ManifestBootstrapConfig = toml::from_str(
            r#"
run = { rhai = "scripts/bootstrap.rhai", run_in = "host" }
"#,
        )
        .expect("parse bootstrap run");

        let task = bootstrap.run.expect("bootstrap run").into_manifest_task();
        assert_eq!(task.run_in, Some(ManifestTaskRunIn::Host));
        let Some(ManifestManagedRun::Sequence(steps)) = task.run else {
            panic!("expected compact inline task to become one-step sequence");
        };
        assert!(matches!(
            steps.as_slice(),
            [ManifestManagedRunStep::Step(step)]
                if step.rhai.as_deref() == Some("scripts/bootstrap.rhai")
        ));
    }

    #[test]
    fn bootstrap_child_run_accepts_compact_inline_task_run_in() {
        let bootstrap: ManifestBootstrapConfig = toml::from_str(
            r#"
[[children]]
path = "app"
repo = "git@example.test/app.git"
run = { task = "bootstrap:child", run_in = "container" }
"#,
        )
        .expect("parse bootstrap child run");

        let task = bootstrap.children[0]
            .run
            .as_ref()
            .expect("child run")
            .as_manifest_task();
        assert_eq!(task.run_in, Some(ManifestTaskRunIn::Container));
        let Some(ManifestManagedRun::Sequence(steps)) = task.run else {
            panic!("expected compact inline task to become one-step sequence");
        };
        assert!(matches!(
            steps.as_slice(),
            [ManifestManagedRunStep::Step(step)]
                if step.task.as_deref() == Some("bootstrap:child")
        ));
    }

    #[test]
    fn bootstrap_run_preserves_existing_sequence_shape() {
        let bootstrap: ManifestBootstrapConfig = toml::from_str(
            r#"
run = [{ task = "bootstrap:root" }]
"#,
        )
        .expect("parse bootstrap run sequence");

        let task = bootstrap.run.expect("bootstrap run").into_manifest_task();
        assert!(matches!(task.run, Some(ManifestManagedRun::Sequence(_))));
    }
}
