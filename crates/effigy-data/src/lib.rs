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
        staged_path: Option<PathBuf>,
    },
    CaptureDestination {
        destination: String,
        source_path: PathBuf,
        push: bool,
    },
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
