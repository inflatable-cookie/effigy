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
    let error = table.resolve("nonexistent").expect_err("unknown alias");
    let ExecError::AliasNotFound { available, .. } = error else {
        panic!("an unknown alias must report AliasNotFound, got {error:?}");
    };
    assert!(available.contains(&"mysql".to_string()));
    assert!(available.contains(&"redis-cli".to_string()));
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
