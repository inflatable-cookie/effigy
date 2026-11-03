use std::collections::HashMap;

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
fn resolve_nonexistent_alias() {
    let table = test_table();
    let error = table.resolve("nonexistent").expect_err("unknown alias");
    let ExecError::AliasNotFound { available, .. } = error else {
        panic!("an unknown alias must report AliasNotFound, got {error:?}");
    };
    let expected = ["artisan", "mysql", "redis-cli", "tinker"];
    assert_eq!(available, expected);
    assert_eq!(table.names(), expected);
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
fn from_map() {
    let mut map = HashMap::new();
    map.insert(
        "psql".to_string(),
        ExecAlias {
            service: "db".to_string(),
            command: "psql -U postgres".to_string(),
        },
    );
    let mut table = ExecAliasTable::from_map(map);
    assert_eq!(table.len(), 1);
    let resolved = table
        .resolve_command("psql", &["mydb".to_string()])
        .unwrap();
    assert_eq!(resolved.command, vec!["psql", "-U", "postgres", "mydb"]);
    table.register(
        "psql".to_string(),
        ExecAlias {
            service: "other".to_string(),
            command: "psql".to_string(),
        },
    );
    assert_eq!(table.resolve("psql").unwrap().service, "other");
}
