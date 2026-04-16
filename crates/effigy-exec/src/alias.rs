//! Exec alias resolution.
//!
//! Exec aliases are named shortcuts for running specific commands in
//! specific container services. For example:
//!
//! ```toml
//! [containers.web.exec.aliases]
//! mysql = { service = "db", command = "mysql" }
//! redis-cli = { service = "cache", command = "redis-cli" }
//! ```
//!
//! When the user runs `effigy mysql`, the alias resolver translates this
//! to `docker compose exec db mysql` (with the right compose project and
//! file arguments).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::error::ExecError;

/// A single exec alias definition.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecAlias {
    /// The container service to exec into (e.g., "db", "cache").
    pub service: String,

    /// The command to run (e.g., "mysql", "redis-cli").
    pub command: String,
}

/// A collection of exec aliases for a container.
#[derive(Debug, Clone, Default)]
pub struct ExecAliasTable {
    /// Aliases keyed by name.
    aliases: HashMap<String, ExecAlias>,
}

impl ExecAliasTable {
    /// Create an empty alias table.
    pub fn new() -> Self {
        Self {
            aliases: HashMap::new(),
        }
    }

    /// Create an alias table from a map.
    pub fn from_map(aliases: HashMap<String, ExecAlias>) -> Self {
        Self { aliases }
    }

    /// Register an alias.
    pub fn register(&mut self, name: String, alias: ExecAlias) {
        self.aliases.insert(name, alias);
    }

    /// Resolve an alias by name.
    ///
    /// Returns the alias definition, or an error with available aliases.
    pub fn resolve(&self, name: &str) -> Result<&ExecAlias, ExecError> {
        self.aliases.get(name).ok_or_else(|| {
            let mut available: Vec<String> = self.aliases.keys().cloned().collect();
            available.sort();
            ExecError::AliasNotFound {
                name: name.to_string(),
                available,
            }
        })
    }

    /// Check if an alias exists.
    pub fn contains(&self, name: &str) -> bool {
        self.aliases.contains_key(name)
    }

    /// All alias names, sorted.
    pub fn names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.aliases.keys().map(|s| s.as_str()).collect();
        names.sort();
        names
    }

    /// Number of registered aliases.
    pub fn len(&self) -> usize {
        self.aliases.len()
    }

    /// Whether the table is empty.
    pub fn is_empty(&self) -> bool {
        self.aliases.is_empty()
    }

    /// Build the full command line for an alias invocation.
    ///
    /// Given the alias name and any extra arguments from the user, returns
    /// the service and full command string to exec.
    ///
    /// For example, `resolve_command("mysql", &["-u", "root", "mydb"])`
    /// returns `("db", ["mysql", "-u", "root", "mydb"])`.
    pub fn resolve_command(
        &self,
        name: &str,
        extra_args: &[String],
    ) -> Result<ResolvedExec, ExecError> {
        let alias = self.resolve(name)?;

        // Split the alias command into parts (it might be multi-word,
        // e.g., "php artisan").
        let mut command_parts: Vec<String> = alias
            .command
            .split_whitespace()
            .map(|s| s.to_string())
            .collect();

        // Append extra arguments from the user.
        command_parts.extend_from_slice(extra_args);

        Ok(ResolvedExec {
            service: alias.service.clone(),
            command: command_parts,
        })
    }
}

/// The result of resolving an alias with arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedExec {
    /// The service to exec into.
    pub service: String,

    /// The full command to run (base command + user args).
    pub command: Vec<String>,
}

impl ResolvedExec {
    /// The command as a single string (for display purposes).
    pub fn command_string(&self) -> String {
        self.command.join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_table() -> ExecAliasTable {
        let mut table = ExecAliasTable::new();
        table.register(
            "mysql".to_string(),
            ExecAlias {
                service: "db".to_string(),
                command: "mysql".to_string(),
            },
        );
        table.register(
            "redis-cli".to_string(),
            ExecAlias {
                service: "cache".to_string(),
                command: "redis-cli".to_string(),
            },
        );
        table.register(
            "artisan".to_string(),
            ExecAlias {
                service: "app".to_string(),
                command: "php artisan".to_string(),
            },
        );
        table.register(
            "tinker".to_string(),
            ExecAlias {
                service: "app".to_string(),
                command: "php artisan tinker".to_string(),
            },
        );
        table
    }

    #[test]
    fn resolve_existing_alias() {
        let table = test_table();
        let alias = table.resolve("mysql").unwrap();
        assert_eq!(alias.service, "db");
        assert_eq!(alias.command, "mysql");
    }

    #[test]
    fn resolve_nonexistent_alias() {
        let table = test_table();
        let result = table.resolve("nonexistent");
        assert!(result.is_err());
        if let ExecError::AliasNotFound { available, .. } = result.unwrap_err() {
            assert!(available.contains(&"mysql".to_string()));
            assert!(available.contains(&"redis-cli".to_string()));
        }
    }

    #[test]
    fn contains_check() {
        let table = test_table();
        assert!(table.contains("mysql"));
        assert!(!table.contains("nonexistent"));
    }

    #[test]
    fn names_are_sorted() {
        let table = test_table();
        let names = table.names();
        assert_eq!(names, vec!["artisan", "mysql", "redis-cli", "tinker"]);
    }

    #[test]
    fn resolve_command_simple() {
        let table = test_table();
        let resolved = table
            .resolve_command("mysql", &["-u".to_string(), "root".to_string()])
            .unwrap();
        assert_eq!(resolved.service, "db");
        assert_eq!(resolved.command, vec!["mysql", "-u", "root"]);
    }

    #[test]
    fn resolve_command_multi_word_base() {
        let table = test_table();
        let resolved = table
            .resolve_command(
                "artisan",
                &["migrate:fresh".to_string(), "--seed".to_string()],
            )
            .unwrap();
        assert_eq!(resolved.service, "app");
        assert_eq!(
            resolved.command,
            vec!["php", "artisan", "migrate:fresh", "--seed"]
        );
    }

    #[test]
    fn resolve_command_no_extra_args() {
        let table = test_table();
        let resolved = table.resolve_command("tinker", &[]).unwrap();
        assert_eq!(resolved.command, vec!["php", "artisan", "tinker"]);
    }

    #[test]
    fn command_string_formatting() {
        let resolved = ResolvedExec {
            service: "app".to_string(),
            command: vec![
                "php".to_string(),
                "artisan".to_string(),
                "migrate".to_string(),
            ],
        };
        assert_eq!(resolved.command_string(), "php artisan migrate");
    }

    #[test]
    fn empty_table() {
        let table = ExecAliasTable::new();
        assert!(table.is_empty());
        assert_eq!(table.len(), 0);
        assert!(table.names().is_empty());
    }

    #[test]
    fn from_map() {
        let mut map = HashMap::new();
        map.insert(
            "psql".to_string(),
            ExecAlias {
                service: "db".to_string(),
                command: "psql -U postgres".to_string(),
            },
        );
        let table = ExecAliasTable::from_map(map);
        assert_eq!(table.len(), 1);
        let resolved = table
            .resolve_command("psql", &["mydb".to_string()])
            .unwrap();
        assert_eq!(resolved.command, vec!["psql", "-U", "postgres", "mydb"]);
    }
}
