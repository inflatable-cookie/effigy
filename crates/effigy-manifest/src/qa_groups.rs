//! Bounded QA group definitions (contract 051).
//!
//! Maintained groups live under `[qa.groups.<name>]` in the composed manifest.
//! Temporary groups live in one caller-selected file with a `[qa_group]` root
//! table and share this grammar. A group names existing task selectors; it
//! never carries a task body, include, directory glob, or environment
//! override. See `docs/knowledge/contracts/051-bounded-qa-groups-contract.md`.

use std::collections::BTreeSet;

use chrono::NaiveDate;

use super::draft_defs::ManifestDraftDate;

/// Group lifecycle surface: `maintained` in the manifest, `temporary` in an
/// explicit file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManifestQaLifecycle {
    Maintained,
    Temporary,
}

impl ManifestQaLifecycle {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Maintained => "maintained",
            Self::Temporary => "temporary",
        }
    }
}

/// Whether a group demands explicit caller scope tokens before it runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManifestQaScopePolicy {
    Required,
    Advisory,
}

impl ManifestQaScopePolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Required => "required",
            Self::Advisory => "advisory",
        }
    }
}

/// Declared proof role of one group member.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManifestQaMemberKind {
    Test,
    Compile,
    Docs,
    Proof,
    Setup,
}

impl ManifestQaMemberKind {
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw {
            "test" => Ok(Self::Test),
            "compile" => Ok(Self::Compile),
            "docs" => Ok(Self::Docs),
            "proof" => Ok(Self::Proof),
            "setup" => Ok(Self::Setup),
            other => Err(format!(
                "`{other}` is not a valid member `kind`; use test, compile, docs, proof, or setup"
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Test => "test",
            Self::Compile => "compile",
            Self::Docs => "docs",
            Self::Proof => "proof",
            Self::Setup => "setup",
        }
    }
}

/// Task surface a member resolves on. Maintained groups use `published` only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManifestQaMemberSurface {
    Published,
    Draft,
}

impl ManifestQaMemberSurface {
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw {
            "published" => Ok(Self::Published),
            "draft" => Ok(Self::Draft),
            other => Err(format!(
                "`{other}` is not a valid member `surface`; use published or draft"
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Published => "published",
            Self::Draft => "draft",
        }
    }
}

/// One typed caller-supplied or definition-side scope identity.
///
/// `path:` values are repository-relative, slash-normalized, case-sensitive
/// logical paths. Definition-side `path:` patterns may use `*` inside one
/// segment and `**` across whole segments; caller tokens are always concrete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QaScopeToken {
    Path(String),
    CargoPackage(String),
    BunPackage(String),
    Workspace(String),
    Input(String),
    External(String),
}

impl QaScopeToken {
    /// Parse and validate one typed token string.
    pub fn parse(raw: &str, allow_path_patterns: bool) -> Result<Self, String> {
        let (kind, value) = raw
            .split_once(':')
            .ok_or_else(|| format!("scope token `{raw}` needs a typed `kind:value` form"))?;
        if value.is_empty() {
            return Err(format!("scope token `{raw}` has an empty value"));
        }
        match kind {
            "path" => {
                validate_path_value(value, allow_path_patterns)
                    .map_err(|detail| format!("scope token `{raw}`: {detail}"))?;
                Ok(Self::Path(value.to_owned()))
            }
            "cargo-package" => Ok(Self::CargoPackage(value.to_owned())),
            "bun-package" => Ok(Self::BunPackage(value.to_owned())),
            "workspace" => Ok(Self::Workspace(value.to_owned())),
            "input" => Ok(Self::Input(value.to_owned())),
            "external" => Ok(Self::External(value.to_owned())),
            other => Err(format!(
                "scope token `{raw}` uses unsupported kind `{other}`; use path, cargo-package, bun-package, workspace, input, or external"
            )),
        }
    }

    /// Canonical token rendering used in evidence and payloads.
    pub fn as_token(&self) -> String {
        match self {
            Self::Path(value) => format!("path:{value}"),
            Self::CargoPackage(value) => format!("cargo-package:{value}"),
            Self::BunPackage(value) => format!("bun-package:{value}"),
            Self::Workspace(value) => format!("workspace:{value}"),
            Self::Input(value) => format!("input:{value}"),
            Self::External(value) => format!("external:{value}"),
        }
    }

    /// Whether this definition-side pattern matches one concrete caller token.
    ///
    /// Non-path kinds match exactly. `path:` patterns use `*` inside one
    /// segment and `**` across whole segments.
    pub fn matches(&self, caller: &QaScopeToken) -> bool {
        match (self, caller) {
            (Self::Path(pattern), Self::Path(value)) => path_pattern_matches(pattern, value),
            (Self::CargoPackage(pattern), Self::CargoPackage(value))
            | (Self::BunPackage(pattern), Self::BunPackage(value))
            | (Self::Workspace(pattern), Self::Workspace(value))
            | (Self::Input(pattern), Self::Input(value))
            | (Self::External(pattern), Self::External(value)) => pattern == value,
            _ => false,
        }
    }
}

fn validate_path_value(value: &str, allow_patterns: bool) -> Result<(), String> {
    if value.starts_with('/') {
        return Err("path tokens are repository-relative and cannot start with `/`".to_owned());
    }
    if value.contains('\\') {
        return Err("path tokens use `/` separators".to_owned());
    }
    for segment in value.split('/') {
        if segment.is_empty() || segment == "." || segment == ".." {
            return Err(format!(
                "path segments cannot be empty, `.`, or `..` (got `{value}`)"
            ));
        }
        if !allow_patterns && (segment.contains('*') || segment.contains('?')) {
            return Err(format!(
                "scope inputs are concrete; wildcard segments are definition-side patterns only (got `{value}`)"
            ));
        }
    }
    Ok(())
}

/// Match one concrete repository-relative path against a definition pattern.
///
/// `*` matches characters within one segment; `**` matches zero or more whole
/// segments. Matching is logical string work only; it never dereferences the
/// filesystem, so deleted files stay nameable.
pub fn path_pattern_matches(pattern: &str, value: &str) -> bool {
    let pattern_segments: Vec<&str> = pattern.split('/').collect();
    let value_segments: Vec<&str> = value.split('/').collect();
    segments_match(&pattern_segments, &value_segments)
}

fn segments_match(pattern: &[&str], value: &[&str]) -> bool {
    match pattern.split_first() {
        None => value.is_empty(),
        Some((&"**", rest)) => (0..=value.len()).any(|skip| segments_match(rest, &value[skip..])),
        Some((pattern_segment, rest)) => match value.split_first() {
            Some((value_segment, value_rest))
                if segment_matches(pattern_segment, value_segment) =>
            {
                segments_match(rest, value_rest)
            }
            _ => false,
        },
    }
}

fn segment_matches(pattern: &str, value: &str) -> bool {
    if pattern == "*" {
        return !value.is_empty();
    }
    match pattern.split_once('*') {
        None => pattern == value,
        Some((prefix, suffix)) => {
            value.len() >= prefix.len() + suffix.len()
                && value.starts_with(prefix)
                && value.ends_with(suffix)
        }
    }
}

/// One known unmapped input with the reason it has no trustworthy member.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestQaCoverageGap {
    pub input: QaScopeToken,
    pub reason: String,
}

/// One member of a QA group: an existing task selector plus reviewable
/// declarations. Targets, covers, companions, and limits are evidence
/// metadata; the resolved selector and fixed args determine execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestQaGroupMember {
    pub id: String,
    pub kind: ManifestQaMemberKind,
    pub surface: ManifestQaMemberSurface,
    /// Optional effective-catalog alias pinning cross-catalog lookup.
    pub catalog: Option<String>,
    /// Exact task selector within the member's catalog; never contains `/`.
    pub task: String,
    pub args: Vec<String>,
    pub targets: Vec<QaScopeToken>,
    pub covers: Vec<QaScopeToken>,
    pub companions: Vec<String>,
    pub limits: Vec<String>,
}

/// One QA group definition, validated. The parse tables live in
/// [`ManifestQaGroupTable`]; this shape is what resolution and execution read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestQaGroup {
    pub lifecycle: ManifestQaLifecycle,
    /// Group name. Maintained groups take theirs from the `[qa.groups]` key;
    /// temporary groups must declare it and match the caller's selector.
    pub name: String,
    /// Owning catalog alias. Required for temporary groups; maintained groups
    /// belong to the catalog that declares them.
    pub catalog: Option<String>,
    pub created: Option<ManifestDraftDate>,
    pub expires: Option<ManifestDraftDate>,
    pub purpose: String,
    pub scope_policy: ManifestQaScopePolicy,
    pub coverage_gaps: Vec<ManifestQaCoverageGap>,
    pub proof_limits: Vec<String>,
    pub expected_wall_ms: Option<u64>,
    pub expectation_basis: Option<String>,
    pub members: Vec<ManifestQaGroupMember>,
}

impl ManifestQaGroup {
    /// Advisory expiry evidence. Expiry never blocks selection or execution.
    pub fn is_expired_on(&self, today: NaiveDate) -> bool {
        self.expires
            .is_some_and(|expires| today > expires.to_naive_date())
    }
}

/// Group-name and member-id grammar: `[a-z][a-z0-9-]*`.
pub fn validate_qa_name_grammar(value: &str, label: &str) -> Result<(), String> {
    let mut chars = value.chars();
    let valid = match chars.next() {
        Some(first) if first.is_ascii_lowercase() => {
            chars.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
        }
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(format!(
            "{label} `{value}` must match [a-z][a-z0-9-]* and cannot contain `/`"
        ))
    }
}

// ---- Serde parse tables ------------------------------------------------------

use std::collections::BTreeMap;

/// Raw `[qa.groups.<name>]` / `[qa_group]` table before validation.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestQaGroupTable {
    pub lifecycle: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub catalog: Option<String>,
    #[serde(default)]
    pub created: Option<String>,
    #[serde(default)]
    pub expires: Option<String>,
    pub purpose: String,
    pub scope_policy: String,
    #[serde(default)]
    pub coverage_gaps: Vec<ManifestQaCoverageGapTable>,
    pub proof_limits: Vec<String>,
    #[serde(default)]
    pub expected_wall_ms: Option<u64>,
    #[serde(default)]
    pub expectation_basis: Option<String>,
    /// Parsed only to reject it with the precise prerequisite diagnostic.
    /// Run-scoped stop/signal attribution (lead `29e5f6f7`) has not landed.
    #[serde(default)]
    pub hard_timeout_ms: Option<u64>,
    pub members: Vec<ManifestQaMemberTable>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestQaCoverageGapTable {
    pub input: String,
    pub reason: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestQaMemberTable {
    pub id: String,
    pub kind: String,
    #[serde(default)]
    pub surface: Option<String>,
    #[serde(default)]
    pub catalog: Option<String>,
    pub task: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub targets: Vec<String>,
    #[serde(default)]
    pub covers: Vec<String>,
    #[serde(default)]
    pub companions: Vec<String>,
    pub limits: Vec<String>,
}

impl ManifestQaGroupTable {
    /// Validate and convert one raw table into a typed group definition.
    ///
    /// `name_key` is the `[qa.groups]` key for maintained groups; temporary
    /// groups must carry their own `name` and it must equal `name_key` when
    /// the caller supplied one.
    pub fn into_manifest_group(self, name_key: Option<&str>) -> Result<ManifestQaGroup, String> {
        if self.hard_timeout_ms.is_some() {
            return Err(
                "`hard_timeout_ms` is not supported: run-scoped stop and signal attribution (lead 29e5f6f7) has not landed, so a group deadline could not be enforced safely; remove the field and rely on expected_wall_ms evidence"
                    .to_owned(),
            );
        }
        let lifecycle = match self.lifecycle.as_str() {
            "maintained" => ManifestQaLifecycle::Maintained,
            "temporary" => ManifestQaLifecycle::Temporary,
            other => {
                return Err(format!(
                    "`{other}` is not a valid `lifecycle`; use maintained (manifest) or temporary (explicit file)"
                ))
            }
        };

        let name = match lifecycle {
            ManifestQaLifecycle::Maintained => {
                for field in ["name", "catalog", "created", "expires"] {
                    if field_is_set(&self, field) {
                        return Err(format!(
                            "maintained groups cannot declare `{field}`; that field belongs to temporary definitions"
                        ));
                    }
                }
                name_key
                    .ok_or_else(|| {
                        "maintained groups need their `[qa.groups]` key as name".to_owned()
                    })?
                    .to_owned()
            }
            ManifestQaLifecycle::Temporary => {
                let name = self.name.clone().ok_or_else(|| {
                    "temporary groups must declare `name` in the `[qa_group]` table".to_owned()
                })?;
                if let Some(key) = name_key {
                    if key != name {
                        return Err(format!(
                            "the caller-selected name `{key}` must equal the file's declared `name` `{name}`"
                        ));
                    }
                }
                name
            }
        };
        validate_qa_name_grammar(&name, "group name")?;

        let catalog = match lifecycle {
            ManifestQaLifecycle::Maintained => None,
            ManifestQaLifecycle::Temporary => {
                let catalog = self.catalog.clone().ok_or_else(|| {
                    "temporary groups must declare `catalog` as an effective-catalog alias; the file's directory never selects one"
                        .to_owned()
                })?;
                if catalog.trim().is_empty() {
                    return Err(
                        "temporary group `catalog` must be a non-empty catalog alias".to_owned(),
                    );
                }
                Some(catalog)
            }
        };

        let (created, expires) = match lifecycle {
            ManifestQaLifecycle::Maintained => (None, None),
            ManifestQaLifecycle::Temporary => {
                let created =
                    ManifestDraftDate::parse(self.created.as_deref().ok_or_else(|| {
                        "temporary groups must declare `created` as a strict YYYY-MM-DD date"
                            .to_owned()
                    })?)
                    .map_err(|detail| format!("group `{name}` `created`: {detail}"))?;
                let expires =
                    ManifestDraftDate::parse(self.expires.as_deref().ok_or_else(|| {
                        "temporary groups must declare `expires` as a strict YYYY-MM-DD date"
                            .to_owned()
                    })?)
                    .map_err(|detail| format!("group `{name}` `expires`: {detail}"))?;
                if expires < created {
                    return Err(format!(
                        "group `{name}` `expires` ({expires}) precedes `created` ({created})"
                    ));
                }
                (Some(created), Some(expires))
            }
        };

        let purpose = self.purpose.trim().to_owned();
        if purpose.is_empty() {
            return Err(format!("group `{name}` requires a non-empty `purpose`"));
        }
        let scope_policy = match self.scope_policy.as_str() {
            "required" => ManifestQaScopePolicy::Required,
            "advisory" => ManifestQaScopePolicy::Advisory,
            other => {
                return Err(format!(
                    "group `{name}` `scope_policy` `{other}` is invalid; use required or advisory"
                ))
            }
        };
        let proof_limits =
            non_empty_list(self.proof_limits, format!("group `{name}` `proof_limits`"))?;
        match (&self.expected_wall_ms, &self.expectation_basis) {
            (None, None) => {}
            (Some(_), Some(basis)) if !basis.trim().is_empty() => {}
            (Some(_), None) => {
                return Err(format!(
                    "group `{name}` declares `expected_wall_ms` without `expectation_basis`; the pair must appear together or not at all"
                ))
            }
            (None, Some(_)) => {
                return Err(format!(
                    "group `{name}` declares `expectation_basis` without `expected_wall_ms`; the pair must appear together or not at all"
                ))
            }
            (Some(_), Some(_)) => {
                return Err(format!(
                    "group `{name}` `expectation_basis` must be a non-empty explanation"
                ))
            }
        }
        let coverage_gaps = self
            .coverage_gaps
            .into_iter()
            .map(|gap| {
                let input = QaScopeToken::parse(&gap.input, true)
                    .map_err(|detail| format!("group `{name}` coverage gap: {detail}"))?;
                if gap.reason.trim().is_empty() {
                    return Err(format!(
                        "group `{name}` coverage gap `{}` requires a non-empty `reason`",
                        gap.input
                    ));
                }
                Ok(ManifestQaCoverageGap {
                    input,
                    reason: gap.reason.trim().to_owned(),
                })
            })
            .collect::<Result<Vec<_>, String>>()?;

        if self.members.is_empty() {
            return Err(format!("group `{name}` requires at least one member"));
        }
        let mut seen_ids = BTreeSet::new();
        let mut members = Vec::new();
        for table in self.members {
            let member = table.into_manifest_member(lifecycle, &name)?;
            if !seen_ids.insert(member.id.clone()) {
                return Err(format!(
                    "group `{name}` declares duplicate member id `{}`; member ids must be unique",
                    member.id
                ));
            }
            members.push(member);
        }
        for member in &members {
            for companion in &member.companions {
                if !seen_ids.contains(companion) {
                    return Err(format!(
                        "group `{name}` member `{}` names unknown companion `{companion}`; companions must be member ids in the same group",
                        member.id
                    ));
                }
            }
        }

        Ok(ManifestQaGroup {
            lifecycle,
            name,
            catalog,
            created,
            expires,
            purpose,
            scope_policy,
            coverage_gaps,
            proof_limits,
            expected_wall_ms: self.expected_wall_ms,
            expectation_basis: self.expectation_basis.as_deref().map(str::to_owned),
            members,
        })
    }
}

fn field_is_set(table: &ManifestQaGroupTable, field: &str) -> bool {
    match field {
        "name" => table.name.is_some(),
        "catalog" => table.catalog.is_some(),
        "created" => table.created.is_some(),
        "expires" => table.expires.is_some(),
        _ => false,
    }
}

impl ManifestQaMemberTable {
    fn into_manifest_member(
        self,
        lifecycle: ManifestQaLifecycle,
        group_name: &str,
    ) -> Result<ManifestQaGroupMember, String> {
        let label = format!("group `{group_name}` member `{}`", self.id);
        validate_qa_name_grammar(&self.id, "member id")
            .map_err(|detail| format!("{label}: {detail}"))?;
        let kind = ManifestQaMemberKind::parse(&self.kind)
            .map_err(|detail| format!("{label}: {detail}"))?;
        let surface = match (lifecycle, self.surface.as_deref()) {
            (ManifestQaLifecycle::Maintained, None | Some("published")) => {
                ManifestQaMemberSurface::Published
            }
            (ManifestQaLifecycle::Maintained, Some(other)) => {
                return Err(format!(
                    "{label}: maintained groups resolve published tasks only; `{other}` members are invalid"
                ))
            }
            (ManifestQaLifecycle::Temporary, None) => {
                return Err(format!(
                    "{label}: temporary groups must state `surface` explicitly as published or draft"
                ))
            }
            (ManifestQaLifecycle::Temporary, Some(raw)) => ManifestQaMemberSurface::parse(raw)
                .map_err(|detail| format!("{label}: {detail}"))?,
        };
        if let Some(catalog) = &self.catalog {
            if catalog.trim().is_empty() {
                return Err(format!(
                    "{label}: member `catalog` must be a non-empty alias"
                ));
            }
        }
        let task = self.task.trim().to_owned();
        if task.is_empty() {
            return Err(format!(
                "{label}: `task` must name an existing task selector"
            ));
        }
        if task.contains('/') {
            return Err(format!(
                "{label}: `task` selectors are exact names inside the owning catalog and cannot contain `/`; pin a cross-catalog member with `catalog`"
            ));
        }
        if self.targets.is_empty() {
            return Err(format!(
                "{label}: `targets` must name at least one explicit target"
            ));
        }
        let targets = parse_token_list(&self.targets, &label, "targets")?;
        let covers = parse_token_list(&self.covers, &label, "covers")?;
        let limits = non_empty_list(self.limits, format!("{label} `limits`"))?;
        Ok(ManifestQaGroupMember {
            id: self.id,
            kind,
            surface,
            catalog: self.catalog,
            task,
            args: self.args,
            targets,
            covers,
            companions: self.companions,
            limits,
        })
    }
}

fn parse_token_list(raw: &[String], label: &str, field: &str) -> Result<Vec<QaScopeToken>, String> {
    raw.iter()
        .map(|value| {
            QaScopeToken::parse(value, true)
                .map_err(|detail| format!("{label} `{field}`: {detail}"))
        })
        .collect()
}

fn non_empty_list(values: Vec<String>, label: String) -> Result<Vec<String>, String> {
    let trimmed: Vec<String> = values
        .into_iter()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .collect();
    if trimmed.is_empty() {
        return Err(format!("{label} must name at least one non-empty entry"));
    }
    Ok(trimmed)
}

/// `[qa]` manifest section. Only `groups` exists today; maintained groups
/// are validated at manifest-parse time so a bad definition fails load.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ManifestQaSection {
    pub groups: BTreeMap<String, ManifestQaGroup>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RawManifestQaSection {
    #[serde(default)]
    groups: BTreeMap<String, ManifestQaGroupTable>,
}

impl<'de> serde::Deserialize<'de> for ManifestQaSection {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = RawManifestQaSection::deserialize(deserializer)?;
        let mut groups = BTreeMap::new();
        for (name, table) in raw.groups {
            let group = table
                .into_manifest_group(Some(&name))
                .map_err(serde::de::Error::custom)?;
            groups.insert(name, group);
        }
        Ok(Self { groups })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        path_pattern_matches, validate_qa_name_grammar, ManifestQaGroupTable, QaScopeToken,
    };

    type Groups = crate::ManifestQaSection;

    #[test]
    fn name_grammar_rejects_slashes_and_uppercase() {
        assert!(validate_qa_name_grammar("agent-cli", "group name").is_ok());
        assert!(validate_qa_name_grammar("run", "group name").is_ok());
        assert!(validate_qa_name_grammar("a/b", "group name").is_err());
        assert!(validate_qa_name_grammar("Agent", "group name").is_err());
        assert!(validate_qa_name_grammar("-lead", "group name").is_err());
        assert!(validate_qa_name_grammar("9lives", "group name").is_err());
    }

    #[test]
    fn path_patterns_match_within_and_across_segments() {
        assert!(path_pattern_matches(
            "crates/**",
            "crates/effigy-cli/src/main.rs"
        ));
        assert!(path_pattern_matches("crates/**", "crates/x"));
        assert!(!path_pattern_matches("crates/**", "services/x"));
        assert!(path_pattern_matches("docs/*.md", "docs/README.md"));
        assert!(!path_pattern_matches("docs/*.md", "docs/deep/README.md"));
        assert!(path_pattern_matches("a*/b.md", "ab/b.md"));
        assert!(path_pattern_matches("a*/b.md", "a/b.md"));
    }

    #[test]
    fn typed_tokens_match_by_kind_exactly() {
        let pattern = QaScopeToken::parse("cargo-package:effigy-cli", true).expect("pattern");
        assert!(pattern
            .matches(&QaScopeToken::parse("cargo-package:effigy-cli", false).expect("caller")));
        assert!(
            !pattern.matches(&QaScopeToken::parse("cargo-package:other", false).expect("caller"))
        );
        assert!(!pattern.matches(&QaScopeToken::parse("path:effigy-cli", false).expect("caller")));
    }

    #[test]
    fn caller_tokens_reject_wildcards_and_escaping_paths() {
        assert!(QaScopeToken::parse("path:../escape", false).is_err());
        assert!(QaScopeToken::parse("path:crates/**", false).is_err());
        assert!(QaScopeToken::parse("path:/abs", false).is_err());
        assert!(QaScopeToken::parse("unknown:value", false).is_err());
        assert!(QaScopeToken::parse("input:", false).is_err());
        assert!(QaScopeToken::parse("input:cargo-lock", false).is_ok());
    }

    #[test]
    fn maintained_group_parses_members_and_pair_expectation() {
        let parsed: Groups = toml::from_str(
            r#"
[groups.agent-cli]
lifecycle = "maintained"
purpose = "Check the CLI"
scope_policy = "required"
coverage_gaps = []
proof_limits = ["Only the CLI crate"]
expected_wall_ms = 180000
expectation_basis = "Three warm runs"

[[groups.agent-cli.members]]
id = "cli-tests"
kind = "test"
surface = "published"
task = "test:cli"
args = ["-p", "cli"]
targets = ["cargo-package:cli"]
covers = ["cargo-package:cli"]
limits = ["Only the package tests"]

[[groups.agent-cli.members]]
id = "cli-compile"
kind = "compile"
task = "check:cli"
targets = ["cargo-package:cli"]
companions = ["cli-tests"]
limits = ["Does not compile consumers"]
"#,
        )
        .expect("parse maintained group");

        let group = parsed.groups.get("agent-cli").expect("group exists");
        assert_eq!(group.members.len(), 2);
        assert_eq!(group.members[1].companions, vec!["cli-tests"]);
        assert_eq!(group.expected_wall_ms, Some(180_000));
        assert!(group.created.is_none());
    }

    #[test]
    fn expectation_pair_is_required_together() {
        let error = toml::from_str::<Groups>(
            r#"
[groups.broken]
lifecycle = "maintained"
purpose = "Check"
scope_policy = "advisory"
proof_limits = ["No coverage claim"]
expected_wall_ms = 100
members = [{ id = "one", kind = "test", task = "t", targets = ["workspace:root"], limits = ["nothing"] }]
"#,
        )
        .expect_err("missing basis must fail");
        assert!(error.to_string().contains("expectation_basis"), "{error}");
    }

    #[test]
    fn hard_timeout_is_rejected_with_prerequisite_diagnostic() {
        let error = toml::from_str::<Groups>(
            r#"
[groups.deadline]
lifecycle = "maintained"
purpose = "Check"
scope_policy = "advisory"
proof_limits = ["No coverage claim"]
hard_timeout_ms = 1000
members = [{ id = "one", kind = "test", task = "t", targets = ["workspace:root"], limits = ["nothing"] }]
"#,
        )
        .expect_err("hard timeout must fail");
        assert!(error.to_string().contains("29e5f6f7"), "{error}");
    }

    #[test]
    fn maintained_members_reject_draft_surface_and_unknown_companions() {
        let error = toml::from_str::<Groups>(
            r#"
[groups.drafty]
lifecycle = "maintained"
purpose = "Check"
scope_policy = "advisory"
proof_limits = ["No coverage claim"]
members = [{ id = "one", kind = "test", surface = "draft", task = "t", targets = ["workspace:root"], limits = ["nothing"] }]
"#,
        )
        .expect_err("draft member in maintained group must fail");
        assert!(
            error.to_string().contains("published tasks only"),
            "{error}"
        );

        let error = toml::from_str::<Groups>(
            r#"
[groups.companion]
lifecycle = "maintained"
purpose = "Check"
scope_policy = "advisory"
proof_limits = ["No coverage claim"]
members = [{ id = "one", kind = "test", task = "t", targets = ["workspace:root"], companions = ["ghost"], limits = ["nothing"] }]
"#,
        )
        .expect_err("unknown companion must fail");
        assert!(error.to_string().contains("unknown companion"), "{error}");
    }

    #[test]
    fn temporary_grammar_requires_name_catalog_and_dates() {
        let table: ManifestQaGroupTable = toml::from_str(
            r#"
lifecycle = "temporary"
purpose = "One-off check"
scope_policy = "required"
proof_limits = ["Generator closure unmapped"]
members = [{ id = "one", kind = "proof", surface = "published", task = "check:x", targets = ["path:x/**"], limits = ["nothing"] }]
"#,
        )
        .expect("parse raw table");
        let error = table
            .into_manifest_group(Some("binding-check"))
            .expect_err("missing temporary fields must fail");
        assert!(error.to_string().contains("must declare `name`"), "{error}");
    }

    #[test]
    fn duplicate_member_ids_fail() {
        let error = toml::from_str::<Groups>(
            r#"
[groups.dupes]
lifecycle = "maintained"
purpose = "Check"
scope_policy = "advisory"
proof_limits = ["No coverage claim"]
members = [
  { id = "one", kind = "test", task = "t", targets = ["workspace:root"], limits = ["nothing"] },
  { id = "one", kind = "compile", task = "c", targets = ["workspace:root"], limits = ["nothing"] },
]
"#,
        )
        .expect_err("duplicate member id must fail");
        assert!(error.to_string().contains("duplicate member id"), "{error}");
    }

    #[test]
    fn deserialize_helper_validates_entries() {
        let error = toml::from_str::<crate::ManifestQaSection>(
            r#"
[groups.bad-name]
lifecycle = "maintained"
purpose = "Check"
scope_policy = "advisory"
proof_limits = ["No coverage claim"]
members = [{ id = "Bad_Id", kind = "test", task = "t", targets = ["workspace:root"], limits = ["nothing"] }]
"#,
        )
        .expect_err("bad member id must fail");
        assert!(error.to_string().contains("[a-z][a-z0-9-]*"), "{error}");

        let parsed: crate::ManifestQaSection = toml::from_str(
            r#"
[groups.fine]
lifecycle = "maintained"
purpose = "Check"
scope_policy = "advisory"
proof_limits = ["No coverage claim"]
members = [{ id = "one", kind = "test", task = "t", targets = ["workspace:root"], limits = ["nothing"] }]
"#,
        )
        .expect("valid section parses");
        assert_eq!(parsed.groups.len(), 1);
    }
}
