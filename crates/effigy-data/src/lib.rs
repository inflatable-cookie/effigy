use std::collections::BTreeMap;
use std::path::PathBuf;
use std::{fmt, str::FromStr};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DataTargetRef {
    value: String,
}

impl DataTargetRef {
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
        }
    }

    pub fn as_str(&self) -> &str {
        &self.value
    }
}

impl fmt::Display for DataTargetRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.value)
    }
}

impl FromStr for DataTargetRef {
    type Err = std::convert::Infallible;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(Self::new(value))
    }
}

impl From<&str> for DataTargetRef {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for DataTargetRef {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedDataTarget {
    pub name: DataTargetRef,
    pub database: String,
    pub service: Option<String>,
    pub service_kind: Option<DatabaseServiceKind>,
}

impl ResolvedDataTarget {
    pub fn new(name: impl Into<DataTargetRef>, database: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            database: database.into(),
            service: None,
            service_kind: None,
        }
    }

    pub fn service(mut self, service: impl Into<String>) -> Self {
        self.service = Some(service.into());
        self
    }

    pub fn service_kind(mut self, service_kind: DatabaseServiceKind) -> Self {
        self.service_kind = Some(service_kind);
        self
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DataTargetManifestInput {
    pub bundle_databases: Vec<String>,
    pub data_targets: Vec<DataTargetManifestEntry>,
}

impl DataTargetManifestInput {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn bundle_databases(mut self, databases: Vec<String>) -> Self {
        self.bundle_databases = databases;
        self
    }

    pub fn data_targets(mut self, targets: Vec<DataTargetManifestEntry>) -> Self {
        self.data_targets = targets;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataTargetManifestEntry {
    pub name: String,
    pub service: String,
    pub database: String,
}

impl DataTargetManifestEntry {
    pub fn new(
        name: impl Into<String>,
        service: impl Into<String>,
        database: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            service: service.into(),
            database: database.into(),
        }
    }
}

pub fn collect_manifest_data_targets(input: &DataTargetManifestInput) -> Vec<ResolvedDataTarget> {
    let mut targets = BTreeMap::<String, ResolvedDataTarget>::new();
    for target in &input.bundle_databases {
        let target = target.trim();
        if target.is_empty() {
            continue;
        }
        targets.insert(
            target.to_owned(),
            ResolvedDataTarget::new(target.to_owned(), target.to_owned()),
        );
    }
    for target in &input.data_targets {
        let name = target.name.trim();
        let service = target.service.trim();
        let database = target.database.trim();
        if name.is_empty() || service.is_empty() || database.is_empty() {
            continue;
        }
        targets.insert(
            name.to_owned(),
            ResolvedDataTarget::new(name.to_owned(), database.to_owned())
                .service(service.to_owned()),
        );
    }
    targets.into_values().collect()
}

pub fn select_data_targets(
    declared_targets: &[ResolvedDataTarget],
    requested_targets: &[Option<String>],
) -> Result<Vec<Option<DataTargetRef>>, DataTargetSelectionError> {
    let mut seen_targets = std::collections::BTreeSet::<String>::new();
    let mut selected = Vec::with_capacity(requested_targets.len());
    let valid_targets = || {
        declared_targets
            .iter()
            .map(|target| target.name.to_string())
            .collect::<Vec<_>>()
    };

    for (index, requested) in requested_targets.iter().enumerate() {
        let effective_target = match requested.as_deref() {
            Some(target) => {
                if !declared_targets.is_empty()
                    && !declared_targets
                        .iter()
                        .any(|declared| declared.name.as_str() == target)
                {
                    return Err(DataTargetSelectionError::UnknownTarget {
                        index,
                        target: target.to_owned(),
                        valid_targets: valid_targets(),
                    });
                }
                Some(DataTargetRef::from(target))
            }
            None => match declared_targets {
                [declared] => Some(declared.name.clone()),
                targets if targets.len() > 1 => {
                    return Err(DataTargetSelectionError::MissingTarget {
                        index,
                        valid_targets: valid_targets(),
                    });
                }
                _ => None,
            },
        };

        if let Some(target) = effective_target.as_ref() {
            if !seen_targets.insert(target.to_string()) {
                return Err(DataTargetSelectionError::DuplicateTarget {
                    index,
                    target: target.to_string(),
                });
            }
        }
        selected.push(effective_target);
    }

    Ok(selected)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataTargetSelectionError {
    UnknownTarget {
        index: usize,
        target: String,
        valid_targets: Vec<String>,
    },
    MissingTarget {
        index: usize,
        valid_targets: Vec<String>,
    },
    DuplicateTarget {
        index: usize,
        target: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatabaseService {
    pub name: String,
    pub kind: DatabaseServiceKind,
    pub password: String,
    pub declared_databases: Vec<String>,
    pub primary_database: Option<String>,
}

impl DatabaseService {
    pub fn new(name: impl Into<String>, kind: DatabaseServiceKind) -> Self {
        Self {
            name: name.into(),
            kind,
            password: "secret".to_owned(),
            declared_databases: Vec::new(),
            primary_database: None,
        }
    }

    pub fn password(mut self, password: impl Into<String>) -> Self {
        self.password = password.into();
        self
    }

    pub fn declared_databases(mut self, databases: Vec<String>) -> Self {
        self.declared_databases = databases;
        self
    }

    pub fn primary_database(mut self, database: impl Into<String>) -> Self {
        self.primary_database = Some(database.into());
        self
    }

    pub fn primary_database_opt(mut self, database: Option<String>) -> Self {
        self.primary_database = database;
        self
    }
}

pub fn select_database_service<'a>(
    services: &'a [DatabaseService],
    requested_service: Option<&str>,
    database: &str,
) -> Result<&'a DatabaseService, DatabaseServiceSelectionError> {
    if let Some(requested_service) = requested_service {
        return services
            .iter()
            .find(|service| service.name == requested_service)
            .ok_or_else(|| DatabaseServiceSelectionError::UnknownService {
                service: requested_service.to_owned(),
            });
    }

    let declared_matches = services
        .iter()
        .filter(|service| {
            service
                .declared_databases
                .iter()
                .any(|entry| entry == database)
        })
        .collect::<Vec<_>>();
    if declared_matches.len() == 1 {
        return Ok(declared_matches[0]);
    }
    if declared_matches.len() > 1 {
        return Err(DatabaseServiceSelectionError::AmbiguousDeclaredDatabase {
            database: database.to_owned(),
            services: service_labels(&declared_matches),
        });
    }

    let primary_matches = services
        .iter()
        .filter(|service| service.primary_database.as_deref() == Some(database))
        .collect::<Vec<_>>();
    if primary_matches.len() == 1 {
        return Ok(primary_matches[0]);
    }
    if primary_matches.len() > 1 {
        return Err(DatabaseServiceSelectionError::AmbiguousPrimaryDatabase {
            database: database.to_owned(),
            services: service_labels(&primary_matches),
        });
    }

    Err(DatabaseServiceSelectionError::NoServiceForDatabase {
        database: database.to_owned(),
    })
}

fn service_labels(services: &[&DatabaseService]) -> Vec<String> {
    services
        .iter()
        .map(|service| format!("{} ({})", service.name, service.kind.catalog()))
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DatabaseServiceSelectionError {
    UnknownService {
        service: String,
    },
    AmbiguousDeclaredDatabase {
        database: String,
        services: Vec<String>,
    },
    AmbiguousPrimaryDatabase {
        database: String,
        services: Vec<String>,
    },
    NoServiceForDatabase {
        database: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseServiceKind {
    Postgres,
    MariaDb,
}

impl DatabaseServiceKind {
    pub fn from_catalog(value: impl AsRef<str>) -> Option<Self> {
        match value.as_ref() {
            "postgres" => Some(Self::Postgres),
            "mariadb" | "mysql" => Some(Self::MariaDb),
            _ => None,
        }
    }

    pub fn catalog(self) -> &'static str {
        match self {
            Self::Postgres => "postgres",
            Self::MariaDb => "mariadb",
        }
    }

    pub fn client_name(self) -> &'static str {
        match self {
            Self::Postgres => "psql",
            Self::MariaDb => "mysql",
        }
    }

    pub fn dump_tool_name(self) -> &'static str {
        match self {
            Self::Postgres => "pg_dump",
            Self::MariaDb => "mysqldump",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataSeedInput {
    pub source: DataSeedSource,
    pub target: Option<DataTargetRef>,
}

impl DataSeedInput {
    pub fn new(source: DataSeedSource) -> Self {
        Self {
            source,
            target: None,
        }
    }

    pub fn target(mut self, target: impl Into<DataTargetRef>) -> Self {
        self.target = Some(target.into());
        self
    }
}

pub fn normalize_seed_source_path(cwd: &std::path::Path, path: PathBuf) -> PathBuf {
    if is_oci_artifact_ref_path(&path) || path.is_absolute() {
        path
    } else {
        cwd.join(path)
    }
}

pub fn normalize_dump_destination_path(
    cwd: &std::path::Path,
    path: PathBuf,
    home: Option<&std::path::Path>,
) -> PathBuf {
    let expanded = expand_tilde_path(path, home);
    if is_oci_artifact_ref_path(&expanded) || expanded.is_absolute() {
        expanded
    } else {
        cwd.join(expanded)
    }
}

fn expand_tilde_path(path: PathBuf, home: Option<&std::path::Path>) -> PathBuf {
    let s = path.as_os_str().to_string_lossy();
    let Some(rest) = s.strip_prefix('~') else {
        return path;
    };
    if !(rest.is_empty() || rest.starts_with('/') || rest.starts_with('\\')) {
        return path;
    }
    match home {
        Some(home) => PathBuf::from(format!("{}{}", home.display(), rest)),
        None => path,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataSeedSource {
    Local(PathBuf),
    Oci(String),
}

impl DataSeedSource {
    pub fn from_raw_path(path: PathBuf) -> Self {
        if is_oci_artifact_ref_path(&path) {
            Self::Oci(path.to_string_lossy().into_owned())
        } else {
            Self::Local(path)
        }
    }

    pub fn is_oci(&self) -> bool {
        matches!(self, Self::Oci(_))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataSeedPlan {
    pub input: DataSeedInput,
    pub resolved_target: Option<ResolvedDataTarget>,
    pub artifact_handoff: Option<ArtifactDataHandoff>,
    pub command: Option<DatabaseCommandPlan>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataDumpInput {
    pub database: String,
    pub target: Option<DataTargetRef>,
    pub destination: DataDumpDestination,
}

impl DataDumpInput {
    pub fn new(database: impl Into<String>, destination: DataDumpDestination) -> Self {
        Self {
            database: database.into(),
            target: None,
            destination,
        }
    }

    pub fn target(mut self, target: impl Into<DataTargetRef>) -> Self {
        self.target = Some(target.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataDumpDestination {
    Local(PathBuf),
    Oci(String),
}

impl DataDumpDestination {
    pub fn from_raw_path(path: PathBuf) -> Self {
        if is_oci_artifact_ref_path(&path) {
            Self::Oci(path.to_string_lossy().into_owned())
        } else {
            Self::Local(path)
        }
    }

    pub fn is_oci(&self) -> bool {
        matches!(self, Self::Oci(_))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataDumpPlan {
    pub input: DataDumpInput,
    pub resolved_target: ResolvedDataTarget,
    pub command: DatabaseCommandPlan,
    pub artifact_handoff: Option<ArtifactDataHandoff>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatabaseCommandPlan {
    pub service: String,
    pub kind: DatabaseServiceKind,
    pub database: String,
    pub argv: Vec<String>,
    pub stdin: Option<PathBuf>,
    pub stdout: Option<PathBuf>,
}

impl DatabaseCommandPlan {
    pub fn new(
        service: impl Into<String>,
        kind: DatabaseServiceKind,
        database: impl Into<String>,
        argv: Vec<String>,
    ) -> Self {
        Self {
            service: service.into(),
            kind,
            database: database.into(),
            argv,
            stdin: None,
            stdout: None,
        }
    }

    pub fn stdin(mut self, path: PathBuf) -> Self {
        self.stdin = Some(path);
        self
    }

    pub fn stdout(mut self, path: PathBuf) -> Self {
        self.stdout = Some(path);
        self
    }
}

pub fn database_dump_command(
    service: impl Into<String>,
    kind: DatabaseServiceKind,
    password: impl AsRef<str>,
    database: impl AsRef<str>,
) -> DatabaseCommandPlan {
    let password = password.as_ref();
    let database = database.as_ref();
    let argv = match kind {
        DatabaseServiceKind::Postgres => vec![
            "env".to_owned(),
            format!("PGPASSWORD={password}"),
            "pg_dump".to_owned(),
            "-U".to_owned(),
            "postgres".to_owned(),
            "-d".to_owned(),
            database.to_owned(),
            "--no-owner".to_owned(),
            "--no-privileges".to_owned(),
        ],
        DatabaseServiceKind::MariaDb => vec![
            "env".to_owned(),
            format!("MYSQL_PWD={password}"),
            "mysqldump".to_owned(),
            "-uroot".to_owned(),
            "--single-transaction".to_owned(),
            "--skip-comments".to_owned(),
            "--routines".to_owned(),
            "--triggers".to_owned(),
            database.to_owned(),
        ],
    };
    DatabaseCommandPlan::new(service, kind, database, argv)
}

pub fn database_seed_reset_command(
    service: impl Into<String>,
    kind: DatabaseServiceKind,
    password: impl AsRef<str>,
    database: impl AsRef<str>,
) -> DatabaseCommandPlan {
    let password = password.as_ref();
    let database = database.as_ref();
    let argv = match kind {
        DatabaseServiceKind::Postgres => vec![
            "env".to_owned(),
            format!("PGPASSWORD={password}"),
            "psql".to_owned(),
            "-v".to_owned(),
            "ON_ERROR_STOP=1".to_owned(),
            "-U".to_owned(),
            "postgres".to_owned(),
            "-d".to_owned(),
            database.to_owned(),
            "-c".to_owned(),
            "DROP SCHEMA public CASCADE; CREATE SCHEMA public;".to_owned(),
        ],
        DatabaseServiceKind::MariaDb => vec![
            "env".to_owned(),
            format!("MYSQL_PWD={password}"),
            "mysql".to_owned(),
            "-uroot".to_owned(),
            "-e".to_owned(),
            format!("DROP DATABASE IF EXISTS `{database}`; CREATE DATABASE `{database}`;"),
        ],
    };
    DatabaseCommandPlan::new(service, kind, database, argv)
}

pub fn database_seed_import_command(
    service: impl Into<String>,
    kind: DatabaseServiceKind,
    password: impl AsRef<str>,
    database: impl AsRef<str>,
) -> DatabaseCommandPlan {
    let password = password.as_ref();
    let database = database.as_ref();
    let argv = match kind {
        DatabaseServiceKind::Postgres => vec![
            "env".to_owned(),
            format!("PGPASSWORD={password}"),
            "psql".to_owned(),
            "-v".to_owned(),
            "ON_ERROR_STOP=1".to_owned(),
            "-U".to_owned(),
            "postgres".to_owned(),
            "-d".to_owned(),
            database.to_owned(),
        ],
        DatabaseServiceKind::MariaDb => vec![
            "env".to_owned(),
            format!("MYSQL_PWD={password}"),
            "mysql".to_owned(),
            "-uroot".to_owned(),
            database.to_owned(),
        ],
    };
    DatabaseCommandPlan::new(service, kind, database, argv)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactDataHandoff {
    StageSource {
        source: String,
        source_kind: DataArtifactRefKind,
        staged_path: Option<PathBuf>,
    },
    CaptureDestination {
        destination: String,
        destination_kind: DataArtifactRefKind,
        source_path: PathBuf,
        push: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataArtifactRefKind {
    Local,
    Oci,
}

impl DataArtifactRefKind {
    pub fn from_path(path: &std::path::Path) -> Self {
        if is_oci_artifact_ref_path(path) {
            Self::Oci
        } else {
            Self::Local
        }
    }
}

pub fn seed_artifact_handoff(source: &DataSeedSource) -> ArtifactDataHandoff {
    match source {
        DataSeedSource::Local(path) => ArtifactDataHandoff::StageSource {
            source: path.display().to_string(),
            source_kind: DataArtifactRefKind::Local,
            staged_path: None,
        },
        DataSeedSource::Oci(reference) => ArtifactDataHandoff::StageSource {
            source: reference.clone(),
            source_kind: DataArtifactRefKind::Oci,
            staged_path: None,
        },
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SeedArtifactStagingPlan {
    Local {
        source_path: PathBuf,
        artifact_root: PathBuf,
    },
    Oci {
        reference: String,
        artifact_root: PathBuf,
        pull_destination_root: PathBuf,
    },
}

pub fn seed_artifact_staging_plan(
    repo_root: &std::path::Path,
    handoff: &ArtifactDataHandoff,
) -> Option<SeedArtifactStagingPlan> {
    let artifact_root = default_data_artifact_root(repo_root);
    match handoff {
        ArtifactDataHandoff::StageSource {
            source,
            source_kind: DataArtifactRefKind::Local,
            ..
        } => {
            let source_path = PathBuf::from(source);
            let source_path = if source_path.is_absolute() {
                source_path
            } else {
                repo_root.join(source_path)
            };
            Some(SeedArtifactStagingPlan::Local {
                source_path,
                artifact_root,
            })
        }
        ArtifactDataHandoff::StageSource {
            source,
            source_kind: DataArtifactRefKind::Oci,
            ..
        } => Some(SeedArtifactStagingPlan::Oci {
            reference: source.clone(),
            pull_destination_root: artifact_root.join(".oci-pulls"),
            artifact_root,
        }),
        ArtifactDataHandoff::CaptureDestination { .. } => None,
    }
}

pub fn default_data_artifact_root(repo_root: &std::path::Path) -> PathBuf {
    repo_root.join(".effigy/local/artifacts")
}

pub fn dump_artifact_handoff(
    repo_root: &std::path::Path,
    target: Option<&DataTargetRef>,
    database: &str,
    destination: &DataDumpDestination,
    push: bool,
) -> Option<ArtifactDataHandoff> {
    match destination {
        DataDumpDestination::Local(_) => None,
        DataDumpDestination::Oci(reference) => Some(ArtifactDataHandoff::CaptureDestination {
            destination: reference.clone(),
            destination_kind: DataArtifactRefKind::Oci,
            source_path: planned_dump_capture_source_path(repo_root, target, database),
            push,
        }),
    }
}

pub fn planned_dump_capture_source_path(
    repo_root: &std::path::Path,
    target: Option<&DataTargetRef>,
    database: &str,
) -> PathBuf {
    let target = target
        .map(DataTargetRef::as_str)
        .unwrap_or(database)
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>();
    repo_root
        .join(".effigy/local/data-dumps")
        .join(format!("{target}.sql"))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataOperationReport {
    pub kind: DataOperationKind,
    pub target: Option<DataTargetRef>,
    pub database: Option<String>,
    pub result: DataOperationResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataOperationKind {
    Seed,
    Dump,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataOperationResult {
    Planned,
    Completed,
    Failed,
}

pub fn is_oci_artifact_ref_path(path: &std::path::Path) -> bool {
    path.as_os_str().to_string_lossy().starts_with("oci://")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_target_ref_preserves_name() {
        let target = DataTargetRef::from("legacy_mysql");

        assert_eq!(target.as_str(), "legacy_mysql");
    }

    #[test]
    fn resolved_target_keeps_service_and_database_identity() {
        let target = ResolvedDataTarget::new("legacy_mysql", "legacy")
            .service("mysql")
            .service_kind(DatabaseServiceKind::MariaDb);

        assert_eq!(target.name.as_str(), "legacy_mysql");
        assert_eq!(target.database, "legacy");
        assert_eq!(target.service.as_deref(), Some("mysql"));
        assert_eq!(target.service_kind, Some(DatabaseServiceKind::MariaDb));
    }

    #[test]
    fn collects_manifest_data_targets_from_bundle_and_explicit_targets() {
        let targets = collect_manifest_data_targets(
            &DataTargetManifestInput::new()
                .bundle_databases(vec!["acowtancy".to_owned(), "acowtancy_test".to_owned()])
                .data_targets(vec![DataTargetManifestEntry::new(
                    "legacy_mysql",
                    "mysql",
                    "acowtancy",
                )]),
        );

        assert_eq!(targets.len(), 3);
        assert_eq!(targets[0].name.as_str(), "acowtancy");
        assert_eq!(targets[0].database, "acowtancy");
        assert_eq!(targets[1].name.as_str(), "acowtancy_test");
        assert_eq!(targets[2].name.as_str(), "legacy_mysql");
        assert_eq!(targets[2].service.as_deref(), Some("mysql"));
        assert_eq!(targets[2].database, "acowtancy");
    }

    #[test]
    fn explicit_manifest_data_target_replaces_bundle_target() {
        let targets = collect_manifest_data_targets(
            &DataTargetManifestInput::new()
                .bundle_databases(vec!["app".to_owned()])
                .data_targets(vec![DataTargetManifestEntry::new(
                    "app", "postgres", "app_db",
                )]),
        );

        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].name.as_str(), "app");
        assert_eq!(targets[0].database, "app_db");
        assert_eq!(targets[0].service.as_deref(), Some("postgres"));
    }

    #[test]
    fn collect_manifest_data_targets_ignores_empty_material() {
        let targets = collect_manifest_data_targets(
            &DataTargetManifestInput::new()
                .bundle_databases(vec![" ".to_owned(), "app".to_owned()])
                .data_targets(vec![
                    DataTargetManifestEntry::new("missing_service", "", "app"),
                    DataTargetManifestEntry::new("missing_database", "postgres", ""),
                    DataTargetManifestEntry::new("", "postgres", "app"),
                ]),
        );

        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].name.as_str(), "app");
    }

    #[test]
    fn selects_requested_and_default_data_targets() {
        let declared = vec![
            ResolvedDataTarget::new("app", "app"),
            ResolvedDataTarget::new("legacy", "legacy"),
        ];
        let selected =
            select_data_targets(&declared, &[Some("legacy".to_owned())]).expect("selection");

        assert_eq!(selected, vec![Some(DataTargetRef::from("legacy"))]);

        let selected = select_data_targets(&declared[..1], &[None]).expect("default selection");
        assert_eq!(selected, vec![Some(DataTargetRef::from("app"))]);
    }

    #[test]
    fn target_selection_reports_unknown_missing_and_duplicate_targets() {
        let declared = vec![
            ResolvedDataTarget::new("app", "app"),
            ResolvedDataTarget::new("legacy", "legacy"),
        ];

        assert_eq!(
            select_data_targets(&declared, &[Some("missing".to_owned())]),
            Err(DataTargetSelectionError::UnknownTarget {
                index: 0,
                target: "missing".to_owned(),
                valid_targets: vec!["app".to_owned(), "legacy".to_owned()],
            })
        );
        assert_eq!(
            select_data_targets(&declared, &[None]),
            Err(DataTargetSelectionError::MissingTarget {
                index: 0,
                valid_targets: vec!["app".to_owned(), "legacy".to_owned()],
            })
        );
        assert_eq!(
            select_data_targets(&declared, &[Some("app".to_owned()), Some("app".to_owned())],),
            Err(DataTargetSelectionError::DuplicateTarget {
                index: 1,
                target: "app".to_owned(),
            })
        );
    }

    #[test]
    fn database_kind_accepts_current_catalog_names() {
        assert_eq!(
            DatabaseServiceKind::from_catalog("postgres"),
            Some(DatabaseServiceKind::Postgres)
        );
        assert_eq!(
            DatabaseServiceKind::from_catalog("mariadb"),
            Some(DatabaseServiceKind::MariaDb)
        );
        assert_eq!(
            DatabaseServiceKind::from_catalog("mysql"),
            Some(DatabaseServiceKind::MariaDb)
        );
        assert_eq!(DatabaseServiceKind::from_catalog("redis"), None);
    }

    #[test]
    fn database_kind_exposes_client_and_dump_tool_names() {
        assert_eq!(DatabaseServiceKind::Postgres.client_name(), "psql");
        assert_eq!(DatabaseServiceKind::Postgres.dump_tool_name(), "pg_dump");
        assert_eq!(DatabaseServiceKind::MariaDb.client_name(), "mysql");
        assert_eq!(DatabaseServiceKind::MariaDb.dump_tool_name(), "mysqldump");
    }

    #[test]
    fn seed_source_classifies_local_and_oci_paths() {
        let local = DataSeedSource::from_raw_path(PathBuf::from("backups/site.sql"));
        let oci = DataSeedSource::from_raw_path(PathBuf::from(
            "oci://ghcr.io/acme/uat-content:2026-05-06",
        ));

        assert_eq!(
            local,
            DataSeedSource::Local(PathBuf::from("backups/site.sql"))
        );
        assert_eq!(
            oci,
            DataSeedSource::Oci("oci://ghcr.io/acme/uat-content:2026-05-06".to_owned())
        );
        assert!(!local.is_oci());
        assert!(oci.is_oci());
    }

    #[test]
    fn normalizes_seed_source_paths() {
        let cwd = std::path::Path::new("/repo");

        assert_eq!(
            normalize_seed_source_path(cwd, PathBuf::from("seed.sql")),
            PathBuf::from("/repo/seed.sql")
        );
        assert_eq!(
            normalize_seed_source_path(cwd, PathBuf::from("/tmp/seed.sql")),
            PathBuf::from("/tmp/seed.sql")
        );
        assert_eq!(
            normalize_seed_source_path(cwd, PathBuf::from("oci://ghcr.io/acme/seed:latest")),
            PathBuf::from("oci://ghcr.io/acme/seed:latest")
        );
    }

    #[test]
    fn normalizes_dump_destination_paths() {
        let cwd = std::path::Path::new("/repo");
        let home = std::path::Path::new("/home/dev");

        assert_eq!(
            normalize_dump_destination_path(cwd, PathBuf::from("dump.sql"), Some(home)),
            PathBuf::from("/repo/dump.sql")
        );
        assert_eq!(
            normalize_dump_destination_path(cwd, PathBuf::from("/tmp/dump.sql"), Some(home)),
            PathBuf::from("/tmp/dump.sql")
        );
        assert_eq!(
            normalize_dump_destination_path(
                cwd,
                PathBuf::from("oci://ghcr.io/acme/dump:latest"),
                Some(home),
            ),
            PathBuf::from("oci://ghcr.io/acme/dump:latest")
        );
        assert_eq!(
            normalize_dump_destination_path(cwd, PathBuf::from("~/dump.sql"), Some(home)),
            PathBuf::from("/home/dev/dump.sql")
        );
        assert_eq!(
            normalize_dump_destination_path(cwd, PathBuf::from("~"), Some(home)),
            PathBuf::from("/home/dev")
        );
        assert_eq!(
            normalize_dump_destination_path(cwd, PathBuf::from("~user/dump.sql"), Some(home)),
            PathBuf::from("/repo/~user/dump.sql")
        );
    }

    #[test]
    fn detects_oci_artifact_ref_paths() {
        assert!(is_oci_artifact_ref_path(std::path::Path::new(
            "oci://ghcr.io/acme/uat-content:2026-05-06"
        )));
        assert!(!is_oci_artifact_ref_path(std::path::Path::new(
            "backups/site.sql"
        )));
    }

    #[test]
    fn dump_destination_classifies_local_and_oci_paths() {
        let local = DataDumpDestination::from_raw_path(PathBuf::from("backups/site.sql"));
        let oci = DataDumpDestination::from_raw_path(PathBuf::from(
            "oci://ghcr.io/acme/uat-content:2026-05-06",
        ));

        assert_eq!(
            local,
            DataDumpDestination::Local(PathBuf::from("backups/site.sql"))
        );
        assert_eq!(
            oci,
            DataDumpDestination::Oci("oci://ghcr.io/acme/uat-content:2026-05-06".to_owned())
        );
        assert!(!local.is_oci());
        assert!(oci.is_oci());
    }

    #[test]
    fn command_plan_preserves_io_paths() {
        let plan = DatabaseCommandPlan::new(
            "postgres",
            DatabaseServiceKind::Postgres,
            "app",
            vec!["pg_dump".to_owned(), "app".to_owned()],
        )
        .stdin(PathBuf::from("seed.sql"))
        .stdout(PathBuf::from("dump.sql"));

        assert_eq!(plan.service, "postgres");
        assert_eq!(plan.kind, DatabaseServiceKind::Postgres);
        assert_eq!(plan.stdin, Some(PathBuf::from("seed.sql")));
        assert_eq!(plan.stdout, Some(PathBuf::from("dump.sql")));
    }

    #[test]
    fn plans_local_and_oci_seed_artifact_handoffs() {
        let local = seed_artifact_handoff(&DataSeedSource::Local(PathBuf::from("seed.sql")));
        let oci = seed_artifact_handoff(&DataSeedSource::Oci(
            "oci://ghcr.io/acme/seed:latest".to_owned(),
        ));

        assert_eq!(
            local,
            ArtifactDataHandoff::StageSource {
                source: "seed.sql".to_owned(),
                source_kind: DataArtifactRefKind::Local,
                staged_path: None,
            }
        );
        assert_eq!(
            oci,
            ArtifactDataHandoff::StageSource {
                source: "oci://ghcr.io/acme/seed:latest".to_owned(),
                source_kind: DataArtifactRefKind::Oci,
                staged_path: None,
            }
        );
    }

    #[test]
    fn plans_seed_artifact_staging_roots() {
        let repo_root = std::path::Path::new("/repo");
        let local_handoff =
            seed_artifact_handoff(&DataSeedSource::Local(PathBuf::from("seed.sql")));
        let oci_handoff = seed_artifact_handoff(&DataSeedSource::Oci(
            "oci://ghcr.io/acme/seed:latest".to_owned(),
        ));

        assert_eq!(
            seed_artifact_staging_plan(repo_root, &local_handoff),
            Some(SeedArtifactStagingPlan::Local {
                source_path: PathBuf::from("/repo/seed.sql"),
                artifact_root: PathBuf::from("/repo/.effigy/local/artifacts"),
            })
        );
        assert_eq!(
            seed_artifact_staging_plan(repo_root, &oci_handoff),
            Some(SeedArtifactStagingPlan::Oci {
                reference: "oci://ghcr.io/acme/seed:latest".to_owned(),
                artifact_root: PathBuf::from("/repo/.effigy/local/artifacts"),
                pull_destination_root: PathBuf::from("/repo/.effigy/local/artifacts/.oci-pulls"),
            })
        );
    }

    #[test]
    fn plans_absolute_seed_artifact_staging_path_without_joining_repo() {
        let repo_root = std::path::Path::new("/repo");
        let handoff = seed_artifact_handoff(&DataSeedSource::Local(PathBuf::from("/tmp/seed.sql")));

        assert_eq!(
            seed_artifact_staging_plan(repo_root, &handoff),
            Some(SeedArtifactStagingPlan::Local {
                source_path: PathBuf::from("/tmp/seed.sql"),
                artifact_root: PathBuf::from("/repo/.effigy/local/artifacts"),
            })
        );
    }

    #[test]
    fn capture_handoff_has_no_seed_staging_plan() {
        let repo_root = std::path::Path::new("/repo");
        let handoff = dump_artifact_handoff(
            repo_root,
            None,
            "app",
            &DataDumpDestination::Oci("oci://ghcr.io/acme/app:latest".to_owned()),
            false,
        )
        .expect("dump handoff");

        assert_eq!(seed_artifact_staging_plan(repo_root, &handoff), None);
    }

    #[test]
    fn plans_local_and_oci_dump_artifact_handoffs() {
        let repo_root = std::path::Path::new("/repo");
        let target = DataTargetRef::from("legacy/mysql");

        assert_eq!(
            dump_artifact_handoff(
                repo_root,
                Some(&target),
                "legacy",
                &DataDumpDestination::Local(PathBuf::from("/tmp/dump.sql")),
                false,
            ),
            None
        );
        assert_eq!(
            dump_artifact_handoff(
                repo_root,
                Some(&target),
                "legacy",
                &DataDumpDestination::Oci("oci://ghcr.io/acme/dump:latest".to_owned()),
                false,
            ),
            Some(ArtifactDataHandoff::CaptureDestination {
                destination: "oci://ghcr.io/acme/dump:latest".to_owned(),
                destination_kind: DataArtifactRefKind::Oci,
                source_path: PathBuf::from("/repo/.effigy/local/data-dumps/legacy-mysql.sql"),
                push: false,
            })
        );
        assert_eq!(
            dump_artifact_handoff(
                repo_root,
                None,
                "app",
                &DataDumpDestination::Oci("oci://ghcr.io/acme/app:latest".to_owned()),
                true,
            ),
            Some(ArtifactDataHandoff::CaptureDestination {
                destination: "oci://ghcr.io/acme/app:latest".to_owned(),
                destination_kind: DataArtifactRefKind::Oci,
                source_path: PathBuf::from("/repo/.effigy/local/data-dumps/app.sql"),
                push: true,
            })
        );
    }

    #[test]
    fn renders_postgres_dump_command() {
        let plan =
            database_dump_command("postgres", DatabaseServiceKind::Postgres, "secret", "app");

        assert_eq!(plan.service, "postgres");
        assert_eq!(
            plan.argv,
            vec![
                "env",
                "PGPASSWORD=secret",
                "pg_dump",
                "-U",
                "postgres",
                "-d",
                "app",
                "--no-owner",
                "--no-privileges",
            ]
        );
    }

    #[test]
    fn selects_database_services_by_requested_declared_and_primary_database() {
        let services = vec![
            DatabaseService::new("postgres", DatabaseServiceKind::Postgres)
                .declared_databases(vec!["app".to_owned()]),
            DatabaseService::new("mysql", DatabaseServiceKind::MariaDb)
                .password("legacy-secret")
                .primary_database("legacy"),
        ];

        assert_eq!(
            select_database_service(&services, Some("mysql"), "ignored")
                .expect("requested service")
                .name,
            "mysql"
        );
        assert_eq!(
            select_database_service(&services, None, "app")
                .expect("declared database")
                .name,
            "postgres"
        );
        assert_eq!(
            select_database_service(&services, None, "legacy")
                .expect("primary database")
                .name,
            "mysql"
        );
    }

    #[test]
    fn database_service_selection_reports_unknown_ambiguous_and_missing_service() {
        let services = vec![
            DatabaseService::new("postgres_a", DatabaseServiceKind::Postgres)
                .declared_databases(vec!["app".to_owned()]),
            DatabaseService::new("postgres_b", DatabaseServiceKind::Postgres)
                .declared_databases(vec!["app".to_owned()]),
        ];

        assert_eq!(
            select_database_service(&services, Some("missing"), "app"),
            Err(DatabaseServiceSelectionError::UnknownService {
                service: "missing".to_owned(),
            })
        );
        assert_eq!(
            select_database_service(&services, None, "app"),
            Err(DatabaseServiceSelectionError::AmbiguousDeclaredDatabase {
                database: "app".to_owned(),
                services: vec![
                    "postgres_a (postgres)".to_owned(),
                    "postgres_b (postgres)".to_owned(),
                ],
            })
        );
        assert_eq!(
            select_database_service(&services, None, "missing"),
            Err(DatabaseServiceSelectionError::NoServiceForDatabase {
                database: "missing".to_owned(),
            })
        );
    }

    #[test]
    fn renders_mariadb_dump_command() {
        let plan = database_dump_command("mysql", DatabaseServiceKind::MariaDb, "secret", "legacy");

        assert_eq!(plan.service, "mysql");
        assert_eq!(
            plan.argv,
            vec![
                "env",
                "MYSQL_PWD=secret",
                "mysqldump",
                "-uroot",
                "--single-transaction",
                "--skip-comments",
                "--routines",
                "--triggers",
                "legacy",
            ]
        );
    }

    #[test]
    fn renders_builtin_seed_reset_commands() {
        let postgres =
            database_seed_reset_command("postgres", DatabaseServiceKind::Postgres, "secret", "app");
        let mariadb =
            database_seed_reset_command("mysql", DatabaseServiceKind::MariaDb, "secret", "legacy");

        assert_eq!(
            postgres.argv,
            vec![
                "env",
                "PGPASSWORD=secret",
                "psql",
                "-v",
                "ON_ERROR_STOP=1",
                "-U",
                "postgres",
                "-d",
                "app",
                "-c",
                "DROP SCHEMA public CASCADE; CREATE SCHEMA public;",
            ]
        );
        assert_eq!(
            mariadb.argv,
            vec![
                "env",
                "MYSQL_PWD=secret",
                "mysql",
                "-uroot",
                "-e",
                "DROP DATABASE IF EXISTS `legacy`; CREATE DATABASE `legacy`;",
            ]
        );
    }

    #[test]
    fn renders_builtin_seed_import_commands() {
        let postgres = database_seed_import_command(
            "postgres",
            DatabaseServiceKind::Postgres,
            "secret",
            "app",
        );
        let mariadb =
            database_seed_import_command("mysql", DatabaseServiceKind::MariaDb, "secret", "legacy");

        assert_eq!(
            postgres.argv,
            vec![
                "env",
                "PGPASSWORD=secret",
                "psql",
                "-v",
                "ON_ERROR_STOP=1",
                "-U",
                "postgres",
                "-d",
                "app",
            ]
        );
        assert_eq!(
            mariadb.argv,
            vec!["env", "MYSQL_PWD=secret", "mysql", "-uroot", "legacy",]
        );
    }
}
