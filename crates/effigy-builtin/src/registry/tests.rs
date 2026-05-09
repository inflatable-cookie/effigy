use super::builtin_registry_entry;

#[test]
fn builtin_registry_contract_is_stable() {
    let names = [
        "doctor", "tasks", "config", "help", "watch", "init", "scan", "test",
    ];

    for name in names {
        let entry = builtin_registry_entry(name).expect("registry entry should exist");
        assert_eq!(entry.name, name);
    }
    assert!(builtin_registry_entry("not-a-builtin").is_none());
}
