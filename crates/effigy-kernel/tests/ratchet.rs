#[path = "../qa/ratchet.rs"]
mod ratchet;
use ratchet::*;
fn counts(source: &str) -> std::collections::BTreeMap<String, usize> {
    scan_source(source, false).unwrap().0
}

/// Merged proof: ratchet_negative_added_command_alias_and_macro_fail, ratchet_counts_aliases_and_all_boundary_families, ratchet_test_exclusion_is_syntax_aware_and_not_cfg_any_loophole.
#[test]
fn ratchet_counts_every_construct_and_loophole() {
    // ratchet_negative_added_command_alias_and_macro_fail

    for source in [
        r#"fn added() { std::process::Command::new("true").status(); }"#,
        r#"use std::process::Command as Cmd; fn added() { let mut cmd=Cmd::new("true");cmd.spawn(); }"#,
        r#"fn added() { assert!(std::process::Command::new("true").output().is_ok()); }"#,
    ] {
        let production =
            std::collections::BTreeMap::from([("src/added.rs".into(), counts(source))]);
        assert!(!production["src/added.rs"].is_empty());
        assert!(Inventory {
            schema: 1,
            production,
            tests: Default::default()
        }
        .compare(&Inventory {
            schema: 1,
            ..Default::default()
        })
        .unwrap_err()
        .contains("rose"));
    }

    // ratchet_counts_aliases_and_all_boundary_families

    let source = r#"
        use std::{process::Command as Cmd, fs::{self as disk, File as F, OpenOptions as O}, env as environment};
        use fs2::FileExt;
        fn work() {
            let state=".effigy/state";
            let mut child=Cmd::new("true"); child.output();
            libc::kill(1,0); libc::killpg(1,0); libc::flock(1,0);
            nix::sys::signal::kill(1,0); nix::sys::signal::killpg(1,0);
            environment::set_var("a","b"); environment::remove_var("a");
            disk::write(state,"a"); disk::rename(state,state); disk::remove_file(state);
            F::create(state); let f=F::open(state).unwrap(); f.lock_exclusive();
            O::new().write(true).create(true).open(state);
            signal_hook::flag::register(1,flag);
            let shell="pkill -f thing; killall thing";
        }
    "#;
    let found = counts(source);
    for construct in [
        "std.process.Command.new",
        "std.process.Command.output",
        "libc::kill",
        "libc::killpg",
        "libc::flock",
        "nix::sys::signal::kill",
        "nix::sys::signal::killpg",
        "std::env::set_var",
        "std::env::remove_var",
        "std::fs::write",
        "std::fs::rename",
        "std::fs::remove_file",
        "std::fs::File::create",
        "fs2.FileExt.lock",
        "std.fs.OpenOptions.write",
        "signal_hook.registration",
        "literal.pkill",
        "literal.killall",
    ] {
        assert!(
            found.contains_key(construct),
            "missing {construct}: {found:?}"
        );
    }
    assert_eq!(found["std.fs.OpenOptions.write"], 2);

    // ratchet_test_exclusion_is_syntax_aware_and_not_cfg_any_loophole

    let (production, tests) = scan_source(
        r#"
      // std::process::Command::new("comment");
      const DOC:&str="std::process::Command::new(fake)";
      #[cfg(test)] mod tests { fn test() { std::process::Command::new("true"); } }
      #[cfg(all(unix,test))] fn test2() { std::process::Command::new("true"); }
      #[cfg(any(test,unix))] fn product() { std::process::Command::new("true"); }
    "#,
        false,
    )
    .unwrap();
    assert_eq!(production["std.process.Command.new"], 1);
    assert_eq!(tests["std.process.Command.new"], 2);
    let (production, tests) =
        scan_source("fn test(){std::process::Command::new(\"true\");}", true).unwrap();
    assert!(production.is_empty());
    assert_eq!(tests["std.process.Command.new"], 1);
    assert!(
        counts("use std::fs; fn ordinary_file(){fs::write(\"user.txt\",\"data\");}").is_empty()
    );
    assert!(scan_source("this is not rust", false).is_err());

}
/// Merged proof: ratchet_baseline_rejects_unrecorded_fall_and_new_pair_at_same_total, ratchet_type_aliases_function_references_and_typed_parameters_are_counted, ratchet_child_file_inherits_parent_state_path_without_gating_ordinary_files.
#[test]
fn ratchet_compares_baselines_and_semantics() {
    // ratchet_baseline_rejects_unrecorded_fall_and_new_pair_at_same_total

    let old = Inventory {
        schema: 1,
        production: std::collections::BTreeMap::from([(
            "src/a.rs".into(),
            counts("fn f(){std::process::Command::new(\"true\");}"),
        )]),
        tests: Default::default(),
    };
    assert!(old.compare(&old).is_ok());
    let empty = Inventory {
        schema: 1,
        ..Default::default()
    };
    assert!(empty.compare(&old).unwrap_err().contains("fell"));
    let moved = Inventory {
        schema: 1,
        production: std::collections::BTreeMap::from([(
            "src/b.rs".into(),
            old.production["src/a.rs"].clone(),
        )]),
        tests: Default::default(),
    };
    let error = moved.compare(&old).unwrap_err();
    assert!(error.contains("rose"));
    assert!(error.contains("fell"));
    assert!(empty.compare(&empty).is_ok());

    // ratchet_type_aliases_function_references_and_typed_parameters_are_counted

    let found = counts(
        r#"
        type Cmd=std::process::Command;
        fn run(cmd: &mut Cmd) { cmd.spawn(); }
        fn other(){ let constructor=<std::process::Command>::new; let spawn=Cmd::spawn; }
    "#,
    );
    assert_eq!(found["std.process.Command.new"], 1);
    assert_eq!(found["std.process.Command.spawn"], 2);

    // ratchet_child_file_inherits_parent_state_path_without_gating_ordinary_files

    // tempfile uses a fresh mkdtemp directory; every fixture write stays in it.
    let tree = tempfile::tempdir().unwrap();
    let root = tree.path();
    for dir in ["src/cache", "src/ordinary", "crates"] {
        std::fs::create_dir_all(root.join(dir)).unwrap();
    }
    std::fs::write(
        root.join("src/cache.rs"),
        r#"const ROOT:&str=".effigy/cache"; mod store;"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src/cache/store.rs"),
        r#"use std::fs; fn write(){fs::write(super::ROOT,"data");}"#,
    )
    .unwrap();
    std::fs::write(root.join("src/ordinary.rs"), "mod store;").unwrap();
    std::fs::write(
        root.join("src/ordinary/store.rs"),
        r#"use std::fs; fn write(){fs::write("user.txt","data");}"#,
    )
    .unwrap();
    let found = inventory(root).unwrap();
    assert_eq!(found.production["src/cache/store.rs"]["std::fs::write"], 1);
    assert!(!found.production.contains_key("src/ordinary/store.rs"));
    assert!(counts("/// pkill and .effigy are documentation, not code.\nfn f(){std::fs::write(\"user.txt\",\"data\");}").is_empty());

}
/// Merged proof: ratchet_vault_parameter_paths_cannot_hide_raw_writes, ratchet_accounts_for_external_boundaries_by_name_and_ceiling, ratchet_inventory_refuses_an_external_boundary_above_its_ceiling.
#[test]
fn ratchet_reports_each_command_shape() {
    // ratchet_vault_parameter_paths_cannot_hide_raw_writes

    let root = tempfile::tempdir().expect("fresh checkout");
    for relative in [
        "src/runner/secret_vault.rs",
        "crates/effigy-rhai/src/rhai_secrets.rs",
        "crates/effigy-secrets/src/storage.rs",
    ] {
        let path = root.path().join(relative);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("directory");
        std::fs::write(
            path,
            "fn write(vault_path: &std::path::Path) { std::fs::write(vault_path, b\"torn\"); }",
        )
        .expect("source");
    }
    let found = inventory(root.path()).expect("inventory");
    assert_eq!(found.production.len(), 3);
    for counts in found.production.values() {
        assert_eq!(counts["std::fs::write"], 1);
    }
    assert!(found
        .compare(&Inventory {
            schema: 1,
            ..Default::default()
        })
        .is_err());

    // ratchet_accounts_for_external_boundaries_by_name_and_ceiling

    let one = counts(
        "fn write(d: &std::path::Path) { let _ = std::fs::File::create(d.join(\".effigy-marker\")); }",
    );
    assert_eq!(one["std::fs::File::create"], 1);
    assert_eq!(
        enforce_external_ceilings("src/runner/scratch.rs", &one),
        Ok(())
    );

    let two = counts(
        "fn write(d: &std::path::Path) { let _ = std::fs::File::create(d.join(\".effigy-a\")); let _ = std::fs::File::create(d.join(\".effigy-b\")); }",
    );
    assert_eq!(
        enforce_external_ceilings("src/runner/scratch.rs", &two),
        Err(
            "src/runner/scratch.rs: std::fs::File::create exceeds its accounted ceiling (2 > 1)"
                .into()
        )
    );

    // A construct the boundary does not account for fails by name, so a new
    // raw write cannot ride in on the existing allowance.
    let unlisted =
        counts("fn w(d: &std::path::Path) { std::fs::write(d.join(\".effigy-x\"), b\"x\"); }");
    assert!(
        enforce_external_ceilings("src/runner/scratch.rs", &unlisted)
            .unwrap_err()
            .contains("not an accounted external boundary")
    );

    // Files outside the table are unaffected.
    assert_eq!(
        enforce_external_ceilings("src/runner/other.rs", &two),
        Ok(())
    );

    // ratchet_inventory_refuses_an_external_boundary_above_its_ceiling

    let root = tempfile::tempdir().expect("fresh checkout");
    std::fs::create_dir_all(root.path().join("crates")).expect("workspace root");
    let path = root.path().join("src/runner/scratch.rs");
    std::fs::create_dir_all(path.parent().expect("parent")).expect("directory");
    std::fs::write(
        &path,
        "fn write(d: &std::path::Path) { let _ = std::fs::File::create(d.join(\".effigy-a\")); let _ = std::fs::File::create(d.join(\".effigy-b\")); }",
    )
    .expect("source");
    let error = inventory(root.path()).expect_err("two raw writes exceed the ceiling");
    assert!(error.contains("exceeds its accounted ceiling"), "{error}");

}
