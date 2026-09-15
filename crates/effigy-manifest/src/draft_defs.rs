//! Lifecycle-labelled draft task definitions.
//!
//! Drafts live under `[drafts]` and reuse the published task runtime body, but
//! carry strict lifecycle metadata (`created`, optional `expires`, non-empty
//! `purpose`). They require the full table form so lifecycle metadata cannot be
//! smuggled behind compact string or sequence shorthand.
//!
//! The published/draft model is a product-lifecycle and discovery split; it is
//! never an access-control or secrecy boundary. See contract `046`.

use std::collections::BTreeMap;

use chrono::NaiveDate;

use super::task_runtime::{
    ManifestEnvFileDirective, ManifestManagedConcurrentEntry, ManifestManagedProfile,
    ManifestManagedRun, ManifestTask, ManifestTaskCache, ManifestTaskRunIn,
    ManifestTaskSecretsMode,
};

/// Strict `YYYY-MM-DD` lifecycle date used by `[drafts]` metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ManifestDraftDate(NaiveDate);

impl ManifestDraftDate {
    /// Parse one strict zero-padded `YYYY-MM-DD` calendar date.
    pub fn parse(raw: &str) -> Result<Self, String> {
        if raw.len() != 10 {
            return Err(format!(
                "`{raw}` is not a strict `YYYY-MM-DD` date; use exactly ten characters such as `2026-09-15`"
            ));
        }
        let bytes = raw.as_bytes();
        let shaped = bytes[4] == b'-'
            && bytes[7] == b'-'
            && bytes
                .iter()
                .enumerate()
                .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit());
        if !shaped {
            return Err(format!(
                "`{raw}` is not a strict `YYYY-MM-DD` date; use exactly ten characters such as `2026-09-15`"
            ));
        }
        NaiveDate::parse_from_str(raw, "%Y-%m-%d")
            .map(Self)
            .map_err(|_| format!("`{raw}` is not a real calendar date"))
    }

    pub fn to_naive_date(self) -> NaiveDate {
        self.0
    }

    /// Canonical zero-padded rendering.
    pub fn as_iso(self) -> String {
        self.0.format("%Y-%m-%d").to_string()
    }

    /// Today's date in the local calendar, used by production inventory and
    /// doctor surfaces. Tests inject an explicit date instead.
    pub fn today_local() -> Self {
        Self(chrono::Local::now().date_naive())
    }
}

impl std::fmt::Display for ManifestDraftDate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.as_iso())
    }
}

/// One lifecycle-labelled draft: metadata plus an ordinary task body.
#[derive(Debug, Clone)]
pub struct ManifestDraft {
    pub created: ManifestDraftDate,
    pub expires: Option<ManifestDraftDate>,
    pub purpose: String,
    pub task: ManifestTask,
}

impl ManifestDraft {
    /// Expired when the evaluation date is strictly after `expires`.
    ///
    /// Expiry is advisory evidence only; it never disables, mutates, or hides a
    /// draft.
    pub fn is_expired_on(&self, today: NaiveDate) -> bool {
        self.expires
            .is_some_and(|expires| today > expires.to_naive_date())
    }
}

/// Raw `[drafts.<name>]` table. Duplicates the published task body fields so
/// `deny_unknown_fields` keeps working without serde `flatten` caveats.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestDraftTable {
    pub created: String,
    #[serde(default)]
    pub expires: Option<String>,
    pub purpose: String,
    #[serde(default)]
    pub run: Option<ManifestManagedRun>,
    #[serde(default)]
    pub run_in: Option<ManifestTaskRunIn>,
    #[serde(default)]
    pub system: Option<String>,
    #[serde(default)]
    pub workspace: Option<String>,
    #[serde(default)]
    pub stay_in_shell: Option<bool>,
    #[serde(default)]
    pub lock: Option<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub env_file: Option<ManifestEnvFileDirective>,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub fail_on_non_zero: Option<bool>,
    #[serde(default)]
    pub container_lifecycle: Option<bool>,
    #[serde(default)]
    pub secrets: Option<ManifestTaskSecretsMode>,
    #[serde(default)]
    pub gateway: Option<bool>,
    #[serde(default)]
    pub health_wait: Option<bool>,
    #[serde(default)]
    pub health_wait_timeout_secs: Option<u64>,
    #[serde(default)]
    pub ready_message: Option<String>,
    #[serde(default)]
    pub concurrent: Vec<ManifestManagedConcurrentEntry>,
    #[serde(default)]
    pub profiles: BTreeMap<String, ManifestManagedProfile>,
    #[serde(default)]
    pub cache: Option<ManifestTaskCache>,
}

impl ManifestDraftTable {
    fn into_manifest_draft(self, name: &str) -> Result<ManifestDraft, String> {
        let created = ManifestDraftDate::parse(&self.created)
            .map_err(|detail| format!("draft `{name}` `created`: {detail}"))?;
        let expires = match self.expires.as_deref() {
            Some(raw) => {
                let expires = ManifestDraftDate::parse(raw)
                    .map_err(|detail| format!("draft `{name}` `expires`: {detail}"))?;
                if expires < created {
                    return Err(format!(
                        "draft `{name}` `expires` ({expires}) precedes `created` ({created})"
                    ));
                }
                Some(expires)
            }
            None => None,
        };
        let purpose = self.purpose.trim().to_owned();
        if purpose.is_empty() {
            return Err(format!(
                "draft `{name}` requires a non-empty `purpose` describing why it exists"
            ));
        }
        Ok(ManifestDraft {
            created,
            expires,
            purpose,
            task: ManifestTask {
                run: self.run,
                run_in: self.run_in,
                system: self.system,
                workspace: self.workspace,
                stay_in_shell: self.stay_in_shell,
                lock: self.lock,
                env: self.env,
                env_file: self.env_file,
                mode: self.mode,
                fail_on_non_zero: self.fail_on_non_zero,
                container_lifecycle: self.container_lifecycle,
                secrets: self.secrets,
                gateway: self.gateway,
                health_wait: self.health_wait,
                health_wait_timeout_secs: self.health_wait_timeout_secs,
                ready_message: self.ready_message,
                concurrent: self.concurrent,
                profiles: self.profiles,
                cache: self.cache,
            },
        })
    }
}

/// Reject compact draft shorthand before lifecycle metadata can be omitted.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(untagged)]
pub enum ManifestDraftLikeDefinition {
    CompactString(String),
    CompactSequence(Vec<toml::Value>),
    Table(Box<ManifestDraftTable>),
}

impl ManifestDraftLikeDefinition {
    fn into_manifest_draft(self, name: &str) -> Result<ManifestDraft, String> {
        match self {
            Self::Table(table) => table.into_manifest_draft(name),
            Self::CompactString(_) => Err(format!(
                "draft `{name}` must be a full `[drafts.{name}]` table with `created`, `purpose`, and a task body; compact string shorthand cannot carry lifecycle metadata"
            )),
            Self::CompactSequence(_) => Err(format!(
                "draft `{name}` must be a full `[drafts.{name}]` table with `created`, `purpose`, and a task body; compact sequence shorthand cannot carry lifecycle metadata"
            )),
        }
    }
}

pub fn deserialize_drafts<'de, D>(
    deserializer: D,
) -> Result<BTreeMap<String, ManifestDraft>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let definitions =
        <BTreeMap<String, ManifestDraftLikeDefinition> as serde::Deserialize>::deserialize(
            deserializer,
        )?;
    let mut drafts = BTreeMap::new();
    for (name, definition) in definitions {
        let draft = definition
            .into_manifest_draft(&name)
            .map_err(serde::de::Error::custom)?;
        drafts.insert(name, draft);
    }
    Ok(drafts)
}

/// Extract the physical manifest that declared each draft from composition
/// provenance.
///
/// Keys look like `drafts.<name>` (and deeper `drafts.<name>.<field>`); the
/// first segment below `drafts` is the draft name. Explicitly included dated
/// fragments therefore report their own file.
pub fn draft_source_map(
    value_sources: &[crate::ManifestCompositionValueSource],
) -> BTreeMap<String, std::path::PathBuf> {
    let mut sources = BTreeMap::new();
    for source in value_sources {
        let Some(rest) = source.path.strip_prefix("drafts.") else {
            continue;
        };
        let name = rest.split('.').next().unwrap_or_default();
        if name.is_empty() {
            continue;
        }
        sources
            .entry(name.to_owned())
            .or_insert_with(|| source.source.clone());
    }
    sources
}

#[cfg(test)]
mod tests {
    use super::{deserialize_drafts, ManifestDraftDate};

    #[derive(Debug, serde::Deserialize)]
    struct DraftsEnvelope {
        #[serde(deserialize_with = "deserialize_drafts")]
        drafts: std::collections::BTreeMap<String, crate::ManifestDraft>,
    }

    #[test]
    fn full_table_draft_parses_lifecycle_and_body() {
        let parsed: DraftsEnvelope = toml::from_str(
            r#"
[drafts.provider-smoke]
created = "2026-09-15"
expires = "2026-09-29"
purpose = "Validate temporary provider integration"
run = [{ task = "build" }, { run = "./scripts/provider-smoke {args}" }]
run_in = "host"
"#,
        )
        .expect("parse full draft table");

        let draft = parsed.drafts.get("provider-smoke").expect("draft exists");
        assert_eq!(draft.created.as_iso(), "2026-09-15");
        assert_eq!(
            draft.expires.map(ManifestDraftDate::as_iso).as_deref(),
            Some("2026-09-29")
        );
        assert_eq!(draft.purpose, "Validate temporary provider integration");
        assert!(draft.task.run.is_some());
    }

    #[test]
    fn string_draft_value_is_rejected() {
        let error = toml::from_str::<DraftsEnvelope>(
            r#"
[drafts]
smoke = "cargo run"
"#,
        )
        .expect_err("compact string shorthand must fail");
        assert!(
            error.to_string().contains("full `[drafts.smoke]` table"),
            "{error}"
        );
    }

    #[test]
    fn malformed_date_is_rejected() {
        let error = toml::from_str::<DraftsEnvelope>(
            r#"
[drafts.smoke]
created = "2026-9-5"
purpose = "temporary"
run = "cargo run"
"#,
        )
        .expect_err("non-strict date must fail");
        assert!(error.to_string().contains("strict `YYYY-MM-DD`"), "{error}");
    }

    #[test]
    fn reversed_dates_are_rejected() {
        let error = toml::from_str::<DraftsEnvelope>(
            r#"
[drafts.smoke]
created = "2026-09-15"
expires = "2026-09-01"
purpose = "temporary"
run = "cargo run"
"#,
        )
        .expect_err("expiry before creation must fail");
        assert!(error.to_string().contains("precedes `created`"), "{error}");
    }

    #[test]
    fn empty_purpose_is_rejected() {
        let error = toml::from_str::<DraftsEnvelope>(
            r#"
[drafts.smoke]
created = "2026-09-15"
purpose = "   "
run = "cargo run"
"#,
        )
        .expect_err("empty purpose must fail");
        assert!(error.to_string().contains("non-empty `purpose`"), "{error}");
    }

    fn validate(body: &str) -> Result<(), String> {
        let manifest: crate::TaskManifest = toml::from_str(body).expect("parse manifest");
        manifest
            .validate(std::path::Path::new("/tmp/effigy.toml"))
            .map_err(|error| error.to_string())
    }

    #[test]
    fn cross_surface_name_collision_is_rejected() {
        let error = validate(
            r#"
[tasks.smoke]
run = "printf published"

[drafts.smoke]
created = "2026-09-15"
purpose = "collides"
run = "printf draft"
"#,
        )
        .expect_err("collision must fail");
        assert!(error.contains("both `[tasks]` and `[drafts]`"), "{error}");
    }

    #[test]
    fn published_task_may_not_reference_a_draft() {
        let error = validate(
            r#"
[drafts.smoke]
created = "2026-09-15"
purpose = "temporary"
run = "printf draft"

[tasks.build]
run = [{ draft = "smoke" }]
"#,
        )
        .expect_err("published->draft must fail");
        assert!(
            error.contains("cannot depend on disposable drafts"),
            "{error}"
        );
    }

    #[test]
    fn draft_may_reference_published_and_other_drafts() {
        validate(
            r#"
[tasks.build]
run = "printf published"

[drafts.base]
created = "2026-09-15"
purpose = "base"
run = "printf base"

[drafts.smoke]
created = "2026-09-15"
purpose = "composes"
run = [{ task = "build" }, { draft = "base" }]
"#,
        )
        .expect("draft composition must be valid");
    }

    #[test]
    fn manifest_without_drafts_is_unchanged() {
        validate(
            r#"
[tasks.build]
run = "printf build"
"#,
        )
        .expect("published-only manifest validates");
    }
}
