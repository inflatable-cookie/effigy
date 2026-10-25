//! The built-in Rust test route: Oxide's leased slot, and its plain-Cargo
//! escape. Which route a suite takes is a property of the host, so every
//! fixture here owns its `oxide` and `cargo-nextest` stubs and states its
//! PATH explicitly.

use crate::runner::tests::prelude::{
    assert_builtin_test_non_zero, assert_output_contains_all, fs, lock_test,
    parse_json_output_with_schema, read_file_text, run_builtin_err, run_builtin_ok, temp_workspace,
    write_executable, EnvGuard,
};

const POOL_SCHEMA_OK: &str = "if [ \"$1 $2\" = \"pool describe\" ]; then\n  printf '%s\\n' '{\"schema\":\"oxide.pool.describe/1\"}'\n  exit 0\nfi";

/// A Cargo package root plus fixture binaries whose invocations are recorded.
struct Fixture {
    root: std::path::PathBuf,
    bin: std::path::PathBuf,
    nextest_log: std::path::PathBuf,
    oxide_log: std::path::PathBuf,
}

impl Fixture {
    fn new(name: &str, oxide_body: &str, with_nextest: bool) -> Self {
        let root = temp_workspace(name);
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
        )
        .expect("write cargo toml");
        // A real Cargo passes its manifest check before it runs the stub
        // `cargo-nextest`; the routed stubs never reach Cargo at all.
        fs::create_dir_all(root.join("src")).expect("mkdir src");
        fs::write(root.join("src/lib.rs"), "pub fn ok() -> bool { true }\n").expect("write lib");
        let bin = root.join("bin");
        fs::create_dir_all(&bin).expect("mkdir bin");
        let nextest_log = root.join("nextest-args.log");
        let oxide_log = root.join("oxide-args.log");
        if with_nextest {
            write_executable(
                &bin.join("cargo-nextest"),
                &format!(
                    "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\nexit 0\n",
                    nextest_log.display()
                ),
            );
        }
        write_executable(
            &bin.join("oxide"),
            &format!(
                "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\n{oxide_body}\nexit 0\n",
                oxide_log.display()
            ),
        );
        Self {
            root,
            bin,
            nextest_log,
            oxide_log,
        }
    }

    fn path_with(&self, include_system: bool) -> String {
        if include_system {
            let prior = std::env::var("PATH").unwrap_or_default();
            format!("{}:{prior}", self.bin.display())
        } else {
            format!("{}:/usr/bin:/bin", self.bin.display())
        }
    }

    /// A PATH that reaches Cargo but never the host's own `oxide`.
    fn path_without_oxide(&self) -> String {
        let cargo_dir = std::env::var("CARGO")
            .ok()
            .and_then(|cargo| {
                std::path::Path::new(&cargo)
                    .parent()
                    .map(|dir| dir.display().to_string())
            })
            .unwrap_or_else(|| "/usr/bin".to_owned());
        format!("{}:{cargo_dir}:/usr/bin:/bin", self.bin.display())
    }

    fn oxide_invocations(&self) -> Vec<String> {
        recorded_invocations(&self.oxide_log)
    }

    fn nextest_invocations(&self) -> Vec<String> {
        recorded_invocations(&self.nextest_log)
    }
}

/// Every argument line a fixture binary recorded; a binary that never ran
/// leaves no file at all.
fn recorded_invocations(log: &std::path::Path) -> Vec<String> {
    if !log.exists() {
        return Vec::new();
    }
    read_file_text(log).lines().map(str::to_owned).collect()
}

#[test]
fn builtin_test_plan_and_execution_lease_nextest_through_oxide() {
    let _guard = lock_test();
    let fixture = Fixture::new("builtin-test-oxide-nextest", POOL_SCHEMA_OK, true);
    let _env = EnvGuard::set_many(&[
        ("PATH", Some(fixture.path_with(true))),
        ("EFFIGY_OXIDE_ESCAPE", None),
    ]);

    let plan = run_builtin_ok(fixture.root.clone(), "test", &["--plan", "--json"]);
    let parsed = parse_json_output_with_schema(&plan, "effigy.test.plan.v1");
    assert_eq!(parsed["targets"][0]["commands"][0], "oxide nextest run");
    assert!(
        plan.contains("oxide.pool.describe/1"),
        "the plan must say which route and why: {plan}"
    );

    let out = run_builtin_ok(fixture.root.clone(), "test", &[]);
    assert_output_contains_all(&out, &["Test Results", "targets:", "root"]);
    assert_eq!(
        fixture.oxide_invocations(),
        vec!["pool describe --json".to_owned(), "nextest run".to_owned()],
        "the suite must run as `oxide nextest run`, not as plain Cargo"
    );
    assert!(
        fixture.nextest_invocations().is_empty(),
        "cargo-nextest must not be invoked directly: {:?}",
        fixture.nextest_invocations()
    );
}

#[test]
fn builtin_test_routes_the_cargo_test_fallback_as_oxide_test() {
    let _guard = lock_test();
    let fixture = Fixture::new("builtin-test-oxide-cargo-test", POOL_SCHEMA_OK, false);
    let _env = EnvGuard::set_many(&[
        ("PATH", Some(fixture.path_with(false))),
        ("EFFIGY_OXIDE_ESCAPE", None),
    ]);

    let plan = run_builtin_ok(fixture.root.clone(), "test", &["--plan", "--json"]);
    let parsed = parse_json_output_with_schema(&plan, "effigy.test.plan.v1");
    assert_eq!(parsed["targets"][0]["commands"][0], "oxide test");

    let out = run_builtin_ok(fixture.root.clone(), "test", &[]);
    assert_output_contains_all(&out, &["Test Results", "root"]);
    assert_eq!(
        fixture.oxide_invocations(),
        vec!["pool describe --json".to_owned(), "test".to_owned()]
    );
}

#[test]
fn builtin_test_escape_keeps_the_plain_cargo_route() {
    let _guard = lock_test();
    let fixture = Fixture::new("builtin-test-oxide-escape", POOL_SCHEMA_OK, true);
    // The oxide stub records any call, so an empty log proves it was not used.
    let _env = EnvGuard::set_many(&[
        ("PATH", Some(fixture.path_with(true))),
        ("EFFIGY_OXIDE_ESCAPE", Some("cargo".to_owned())),
    ]);

    let plan = run_builtin_ok(fixture.root.clone(), "test", &["--plan", "--json"]);
    let parsed = parse_json_output_with_schema(&plan, "effigy.test.plan.v1");
    assert_eq!(parsed["targets"][0]["commands"][0], "cargo nextest run");
    assert!(
        plan.contains("EFFIGY_OXIDE_ESCAPE=cargo"),
        "the plan must name the escape it took: {plan}"
    );

    let out = run_builtin_ok(fixture.root.clone(), "test", &[]);
    assert_output_contains_all(&out, &["Test Results", "root"]);
    assert!(
        fixture.oxide_invocations().is_empty(),
        "the escape must not consult Oxide: {:?}",
        fixture.oxide_invocations()
    );
    assert_eq!(
        fixture.nextest_invocations(),
        vec!["nextest run".to_owned()]
    );
}

#[test]
fn builtin_test_refuses_an_oxide_that_cannot_report_its_pool() {
    let _guard = lock_test();
    let fixture = Fixture::new(
        "builtin-test-oxide-unusable",
        "if [ \"$1 $2\" = \"pool describe\" ]; then\n  printf '%s\\n' 'oxide: unknown_accounting: the pool ledger could not be read' >&2\n  exit 1\nfi",
        true,
    );
    let _env = EnvGuard::set_many(&[
        ("PATH", Some(fixture.path_with(true))),
        ("EFFIGY_OXIDE_ESCAPE", None),
    ]);

    let err = run_builtin_err(fixture.root.clone(), "test", &["--plan"]);

    let message = err.to_string();
    assert!(message.contains("unknown_accounting"), "{message}");
    assert!(message.contains("EFFIGY_OXIDE_ESCAPE=cargo"), "{message}");
    assert!(
        fixture.nextest_invocations().is_empty(),
        "an unusable oxide must not fall back to plain Cargo"
    );
}

#[test]
fn builtin_test_reports_an_oxide_refused_suite_and_names_the_escape() {
    let _guard = lock_test();
    let fixture = Fixture::new(
        "builtin-test-oxide-refusal",
        "if [ \"$1 $2\" = \"pool describe\" ]; then\n  printf '%s\\n' '{\"schema\":\"oxide.pool.describe/1\"}'\n  exit 0\nfi\nprintf '%s\\n' 'oxide: unknown_accounting: the pool ledger could not be read' >&2\nexit 1",
        true,
    );
    let _env = EnvGuard::set_many(&[
        ("PATH", Some(fixture.path_with(true))),
        ("EFFIGY_OXIDE_ESCAPE", None),
    ]);

    let err = run_builtin_err(fixture.root.clone(), "test", &[]);

    // Oxide's own diagnostic goes to the suite's stderr, which Effigy inherits
    // rather than swallows; its failure surfaces as the suite's exit code and
    // the escape is named for the operator.
    assert_builtin_test_non_zero(
        err,
        None,
        &["Test Results", "exit=1", "EFFIGY_OXIDE_ESCAPE=cargo"],
        &[],
    );
    assert_eq!(
        fixture.oxide_invocations(),
        vec!["pool describe --json".to_owned(), "nextest run".to_owned()]
    );
}

#[test]
fn builtin_test_plain_route_is_unchanged_when_oxide_is_absent() {
    let _guard = lock_test();
    let fixture = Fixture::new("builtin-test-no-oxide", POOL_SCHEMA_OK, true);
    // A PATH that reaches Cargo but never the host's own `oxide`.
    let _env = EnvGuard::set_many(&[
        ("PATH", Some(fixture.path_without_oxide())),
        ("EFFIGY_OXIDE_ESCAPE", None),
    ]);
    fs::remove_file(fixture.bin.join("oxide")).expect("remove oxide stub");

    let plan = run_builtin_ok(fixture.root.clone(), "test", &["--plan", "--json"]);
    let parsed = parse_json_output_with_schema(&plan, "effigy.test.plan.v1");
    assert_eq!(parsed["targets"][0]["commands"][0], "cargo nextest run");
    assert!(
        plan.contains("no `oxide` on PATH"),
        "the plan must say why the route stayed on Cargo: {plan}"
    );
    assert!(
        fixture.oxide_invocations().is_empty(),
        "a host without Oxide has nothing to consult: {:?}",
        fixture.oxide_invocations()
    );
}
