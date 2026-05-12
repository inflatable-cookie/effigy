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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatabaseServiceManifestEntry {
    pub name: String,
    pub catalog: String,
    pub password: Option<String>,
    pub declared_databases: Vec<String>,
    pub primary_database: Option<String>,
}

impl DatabaseServiceManifestEntry {
    pub fn new(name: impl Into<String>, catalog: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            catalog: catalog.into(),
            password: None,
            declared_databases: Vec::new(),
            primary_database: None,
        }
    }

    pub fn password(mut self, password: Option<String>) -> Self {
        self.password = password;
        self
    }

    pub fn declared_databases(mut self, databases: Vec<String>) -> Self {
        self.declared_databases = databases;
        self
    }

    pub fn primary_database(mut self, database: Option<String>) -> Self {
        self.primary_database = database;
        self
    }
}

pub fn collect_database_services_from_manifest_entries(
    entries: &[DatabaseServiceManifestEntry],
) -> Vec<DatabaseService> {
    entries
        .iter()
        .filter_map(|entry| {
            let name = entry.name.trim();
            if name.is_empty() {
                return None;
            }
            let kind = DatabaseServiceKind::from_catalog(entry.catalog.trim())?;
            Some(
                DatabaseService::new(name.to_owned(), kind)
                    .password(trimmed_or_default(entry.password.as_deref(), "secret"))
                    .declared_databases(trimmed_non_empty_strings(&entry.declared_databases))
                    .primary_database_opt(trimmed_optional_string(
                        entry.primary_database.as_deref(),
                    )),
            )
        })
        .collect()
}

fn trimmed_or_default(value: Option<&str>, default: &str) -> String {
    trimmed_optional_string(value).unwrap_or_else(|| default.to_owned())
}

fn trimmed_optional_string(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn trimmed_non_empty_strings(values: &[String]) -> Vec<String> {
    values
        .iter()
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect()
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
    pub reset_command: Option<DatabaseCommandPlan>,
    pub command: Option<DatabaseCommandPlan>,
}

impl DataSeedPlan {
    pub fn new(input: DataSeedInput) -> Self {
        let artifact_handoff = Some(seed_artifact_handoff(&input.source));
        Self {
            input,
            resolved_target: None,
            artifact_handoff,
            reset_command: None,
            command: None,
        }
    }

    pub fn resolved_target(mut self, target: ResolvedDataTarget) -> Self {
        self.resolved_target = Some(target);
        self
    }

    pub fn reset_command(mut self, command: DatabaseCommandPlan) -> Self {
        self.reset_command = Some(command);
        self
    }

    pub fn command(mut self, command: DatabaseCommandPlan) -> Self {
        self.command = Some(command);
        self
    }
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
mod tests;
