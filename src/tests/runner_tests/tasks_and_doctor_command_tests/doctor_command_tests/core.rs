use super::*;

#[test]
fn run_doctor_default_never_executes_discovered_health_task() {
    let root = temp_workspace("doctor-health-delegation");
    let marker = root.join("health-ran");
    write_manifest(
        &root.join("effigy.toml"),
        &format!(
            "[tasks.health]\nrun = \"printf ran > {}\"\n",
            marker.display()
        ),
    );

    let out = run_doctor_task(root, &[]).expect("doctor run");

    assert_output_contains_all(&out, &["No findings."]);
    assert!(!marker.exists(), "fast doctor must not execute health");
    assert_output_excludes_all(
        &out,
        &[
            "health.task.discovery",
            "health.task.execute",
            "health task executed successfully",
            "workspace.root-resolution",
        ],
    );
}

#[test]
fn run_deep_doctor_executes_discovered_health_task() {
    let root = temp_workspace("doctor-deep-health-delegation");
    let marker = root.join("health-ran");
    write_manifest(
        &root.join("effigy.toml"),
        &format!(
            "[tasks.health]\nrun = \"printf ran > {}\"\n",
            marker.display()
        ),
    );

    let out = run_deep_doctor_task(root, &[]).expect("deep doctor run");

    assert_output_contains_all(&out, &["No findings."]);
    assert!(marker.exists(), "deep doctor must execute health");
}

#[test]
fn run_doctor_reports_error_when_health_task_fails() {
    let root = temp_workspace("doctor-health-failure");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.health]\nrun = \"sh -lc 'printf health-failed; exit 3'\"\n",
    );

    let err =
        run_deep_doctor_task(root, &[]).expect_err("doctor should fail when health task fails");
    assert_doctor_non_zero_contains(
        err,
        &["health.task.execute", "health task execution failed"],
    );
}

#[test]
fn run_deep_doctor_executes_health_task_that_reaches_qa() {
    let root = temp_workspace("doctor-heavy-health-task");
    let marker = root.join("heavy-health-ran");
    write_manifest(
        &root.join("effigy.toml"),
        &format!(
            r#"[tasks]
health = [{{ task = "baseline" }}]
baseline = [{{ task = "validate" }}]
validate = [{{ task = "qa" }}]
qa = "printf ran > {}"
"#,
            marker.display()
        ),
    );

    let out = run_deep_doctor_task(root, &[]).expect("deep doctor should execute health");

    assert_output_excludes_all(&out, &["health.task.posture"]);
    assert!(marker.exists(), "deep doctor must execute the health task");
}

#[test]
fn run_deep_doctor_catalog_selection_isolates_sibling_health() {
    let root = temp_workspace("doctor-selected-catalog-isolation");
    let catalog_a = root.join("catalog_a");
    let catalog_b = root.join("catalog_b");
    fs::create_dir_all(&catalog_a).expect("mkdir catalog_a");
    fs::create_dir_all(&catalog_b).expect("mkdir catalog_b");
    let marker_a = root.join("catalog-a-health-ran");
    let marker_b = root.join("catalog-b-health-ran");
    write_manifest(
        &root.join("effigy.toml"),
        "[catalog]\nalias = \"root\"\n[catalog.members]\ncatalog_a = \"catalog_a\"\ncatalog_b = \"catalog_b\"\n",
    );
    write_manifest(
        &catalog_a.join("effigy.toml"),
        &format!(
            "[catalog]\nalias = \"catalog_a\"\n[tasks.health]\nrun = \"printf ran > {}\"\n",
            marker_a.display()
        ),
    );
    write_manifest(
        &catalog_b.join("effigy.toml"),
        &format!(
            "[catalog]\nalias = \"catalog_b\"\n[tasks.health]\nrun = \"printf ran > {}\"\n",
            marker_b.display()
        ),
    );

    let out =
        run_deep_doctor_task(root, &["--catalog", "catalog_a"]).expect("selected catalog doctor");

    assert_output_contains_all(&out, &["catalog_a"]);
    assert!(marker_a.exists(), "selected health must run");
    assert!(!marker_b.exists(), "sibling health must not run");
}

#[test]
fn run_deep_doctor_reports_missing_child_bootstrap_and_skips_health() {
    let root = temp_workspace("doctor-child-bootstrap");
    let member = root.join("member");
    fs::create_dir_all(&member).expect("mkdir member");
    let marker = root.join("member-health-ran");
    fs::write(
        member.join("effigy.toml"),
        format!(
            "[catalog]\nalias = \"member\"\n[package_manager]\njs = \"bun\"\n[tasks.health]\nrun = \"printf ran > {}\"\n",
            marker.display()
        ),
    )
    .expect("write member manifest");
    fs::write(
        member.join("package.json"),
        r#"{"dependencies":{"left-pad":"1.3.0"}}"#,
    )
    .expect("write member package.json");
    fs::write(member.join("bun.lock"), "").expect("write member lock");
    // The parent workspace provides the same package.
    fs::create_dir_all(root.join("node_modules")).expect("parent install");
    write_manifest(
        &root.join("effigy.toml"),
        "[catalog]\nalias = \"root\"\n[catalog.members]\nmember = \"member\"\n",
    );

    let err = run_deep_doctor_task(root.clone(), &["--catalog", "member"])
        .expect_err("missing child bootstrap must fail deep doctor");
    assert_doctor_non_zero_contains(
        err,
        &["health.task.bootstrap", "effigy bootstrap deps sync member"],
    );
    assert!(
        !marker.exists(),
        "member health must not run before dependency bootstrap"
    );

    fs::create_dir_all(member.join("node_modules")).expect("member install");
    fs::create_dir_all(member.join("node_modules").join("left-pad")).expect("member dependency");
    fs::write(
        member.join("node_modules/left-pad/package.json"),
        r#"{"name":"left-pad","version":"1.3.0","main":"entry.cjs"}"#,
    )
    .expect("package metadata");
    fs::write(
        member.join("node_modules/left-pad/entry.cjs"),
        "module.exports = true;",
    )
    .expect("package entry");
    // Doctor may still report unrelated environment findings (for example a
    // missing JS tool on a CI runner), so accept either a clean exit or a
    // non-zero doctor report here. The marker proves the guarded health task
    // actually ran once the local install existed.
    let out = match run_deep_doctor_task(root, &["--catalog", "member"]) {
        Ok(out) => out,
        Err(crate::runner::error::RunnerError::DoctorNonZero { rendered, .. }) => rendered,
        Err(other) => panic!("unexpected deep doctor error after bootstrap: {other}"),
    };
    assert!(marker.exists(), "member health must run after bootstrap");
    assert_output_excludes_all(&out, &["health.task.bootstrap"]);
}

#[test]
fn run_deep_doctor_rejects_unverified_ancestor_install_for_child() {
    let root = temp_workspace("doctor-child-foreign-lock");
    let member = root.join("member");
    fs::create_dir_all(&member).expect("mkdir member");
    let marker = root.join("member-health-ran");
    fs::write(
        member.join("effigy.toml"),
        format!(
            "[catalog]\nalias = \"member\"\n[package_manager]\njs = \"bun\"\n[tasks.health]\nrun = \"printf ran > {}\"\n",
            marker.display()
        ),
    )
    .expect("write member manifest");
    fs::write(
        member.join("package.json"),
        r#"{"dependencies":{"left-pad":"1.3.0"}}"#,
    )
    .expect("write member package.json");
    // The parent has a lock and an install but never declares the member as a
    // workspace member, so it must not satisfy the child requirement.
    fs::write(root.join("bun.lock"), "").expect("write parent lock");
    fs::create_dir_all(root.join("node_modules")).expect("parent install");
    write_manifest(
        &root.join("effigy.toml"),
        "[catalog]\nalias = \"root\"\n[catalog.members]\nmember = \"member\"\n",
    );

    let err = run_deep_doctor_task(root.clone(), &["--catalog", "member"])
        .expect_err("unverified ancestor install must fail deep doctor");
    assert_doctor_non_zero_contains(err, &["health.task.bootstrap", "does not declare"]);
    assert!(
        !marker.exists(),
        "member health must not run against a foreign ancestor install"
    );
}

#[test]
fn run_deep_doctor_rejects_ancestor_install_for_standalone_child_without_lock() {
    let root = temp_workspace("doctor-standalone-child-no-lock");
    let member = root.join("member");
    fs::create_dir_all(&member).expect("mkdir member");
    // The child is its own repository, so an ancestor install must never
    // satisfy it even though the child has no lock of its own.
    fs::create_dir_all(member.join(".git")).expect("member git boundary");
    let marker = root.join("member-health-ran");
    fs::write(
        member.join("effigy.toml"),
        format!(
            "[catalog]\nalias = \"member\"\n[package_manager]\njs = \"bun\"\n[tasks.health]\nrun = \"printf ran > {}\"\n",
            marker.display()
        ),
    )
    .expect("write member manifest");
    fs::write(
        member.join("package.json"),
        r#"{"dependencies":{"left-pad":"1.3.0"}}"#,
    )
    .expect("write member package.json");
    // Ancestor lock and install without any member declaration.
    fs::write(root.join("bun.lock"), "").expect("write parent lock");
    fs::create_dir_all(root.join("node_modules")).expect("parent install");
    write_manifest(
        &root.join("effigy.toml"),
        "[catalog]\nalias = \"root\"\n[catalog.members]\nmember = \"member\"\n",
    );

    let err = run_deep_doctor_task(root, &["--catalog", "member"])
        .expect_err("standalone child without a lock must fail deep doctor");
    assert_doctor_non_zero_contains(err, &["health.task.bootstrap", "missing-local-lock"]);
    assert!(
        !marker.exists(),
        "standalone child health must not run against an ancestor install"
    );
}

#[cfg(unix)]
#[test]
fn run_deep_doctor_rejects_symlinked_ancestor_install_for_standalone_child() {
    let root = temp_workspace("doctor-standalone-child-symlinked-install");
    let member = root.join("member");
    fs::create_dir_all(&member).expect("mkdir member");
    fs::create_dir_all(member.join(".git")).expect("member git boundary");
    let marker = root.join("member-health-ran");
    fs::write(
        member.join("effigy.toml"),
        format!(
            "[catalog]\nalias = \"member\"\n[package_manager]\njs = \"bun\"\n[tasks.health]\nrun = \"printf ran > {}\"\n",
            marker.display()
        ),
    )
    .expect("write member manifest");
    fs::write(
        member.join("package.json"),
        r#"{"dependencies":{"left-pad":"1.3.0"}}"#,
    )
    .expect("write member package.json");
    // Ancestor lock and install, then the child points its own node_modules at
    // the ancestor install. A resolving symlink must not satisfy the guard.
    fs::write(root.join("bun.lock"), "").expect("write parent lock");
    fs::create_dir_all(root.join("node_modules")).expect("parent install");
    std::os::unix::fs::symlink(root.join("node_modules"), member.join("node_modules"))
        .expect("symlink member node_modules");
    write_manifest(
        &root.join("effigy.toml"),
        "[catalog]\nalias = \"root\"\n[catalog.members]\nmember = \"member\"\n",
    );

    let err = run_deep_doctor_task(root, &["--catalog", "member"])
        .expect_err("symlinked ancestor install must fail deep doctor");
    assert_doctor_non_zero_contains(err, &["health.task.bootstrap", "missing-local-lock"]);
    assert!(
        !marker.exists(),
        "member health must not run through a node_modules symlink into an ancestor install"
    );
}

#[test]
fn run_deep_doctor_rejects_empty_child_install_resolved_from_parent() {
    let root = temp_workspace("doctor-empty-child-install");
    let member = root.join("member");
    fs::create_dir_all(&member).expect("mkdir member");
    fs::create_dir_all(member.join(".git")).expect("member git boundary");
    let marker = root.join("member-health-ran");
    fs::write(
        member.join("effigy.toml"),
        format!(
            "[catalog]\nalias = \"member\"\n[package_manager]\njs = \"bun\"\n[tasks.health]\nrun = \"printf ran > {}\"\n",
            marker.display()
        ),
    )
    .expect("write member manifest");
    fs::write(
        member.join("package.json"),
        r#"{"dependencies":{"left-pad":"1.3.0"}}"#,
    )
    .expect("write member package.json");
    fs::write(member.join("bun.lock"), "").expect("write member lock");
    // The member has a directory named node_modules but never installed the
    // package it declares; the parent has it.
    fs::create_dir_all(member.join("node_modules/left-pad")).expect("empty dependency entry");
    fs::create_dir_all(root.join("node_modules").join("left-pad")).expect("parent package");
    fs::write(
        root.join("node_modules/left-pad/package.json"),
        r#"{"name":"left-pad","version":"1.3.0","main":"entry.cjs"}"#,
    )
    .expect("package metadata");
    fs::write(
        root.join("node_modules/left-pad/entry.cjs"),
        "module.exports = true;",
    )
    .expect("package entry");
    write_manifest(
        &root.join("effigy.toml"),
        "[catalog]\nalias = \"root\"\n[catalog.members]\nmember = \"member\"\n",
    );

    let err = run_deep_doctor_task(root.clone(), &["--catalog", "member"])
        .expect_err("empty child install must fail deep doctor");
    assert_doctor_non_zero_contains(
        err,
        &[
            "health.task.bootstrap",
            "missing-local-install",
            "missing_deps=left-pad",
        ],
    );
    assert!(
        !marker.exists(),
        "member health must not run while a declared dependency is only in the parent"
    );

    #[cfg(unix)]
    {
        fs::remove_dir(member.join("node_modules/left-pad")).expect("remove empty entry");
        std::os::unix::fs::symlink(
            root.join("node_modules/left-pad"),
            member.join("node_modules/left-pad"),
        )
        .expect("parent package link");
        let err = run_deep_doctor_task(root.clone(), &["--catalog", "member"])
            .expect_err("parent package link must fail deep doctor");
        assert_doctor_non_zero_contains(err, &["health.task.bootstrap", "missing_deps=left-pad"]);
        assert!(!marker.exists(), "parent link must not enable health");
        fs::remove_file(member.join("node_modules/left-pad")).expect("remove parent link");
    }

    // A real local install of the declared dependency clears the guard.
    fs::create_dir_all(member.join("node_modules").join("left-pad")).expect("member package");
    fs::write(
        member.join("node_modules/left-pad/package.json"),
        r#"{"name":"left-pad","version":"1.3.0","main":"entry.cjs"}"#,
    )
    .expect("package metadata");
    fs::write(
        member.join("node_modules/left-pad/entry.cjs"),
        "module.exports = true;",
    )
    .expect("package entry");
    let out = match run_deep_doctor_task(root, &["--catalog", "member"]) {
        Ok(out) => out,
        Err(crate::runner::error::RunnerError::DoctorNonZero { rendered, .. }) => rendered,
        Err(other) => panic!("unexpected deep doctor error after member install: {other}"),
    };
    assert!(
        marker.exists(),
        "member health must run after the declared dependency is installed"
    );
    assert_output_excludes_all(&out, &["health.task.bootstrap"]);
}

#[test]
fn run_deep_doctor_runs_health_for_declared_workspace_member() {
    let root = temp_workspace("doctor-declared-workspace-member");
    let member = root.join("member");
    fs::create_dir_all(&member).expect("mkdir member");
    let marker = root.join("member-health-ran");
    fs::write(
        member.join("effigy.toml"),
        format!(
            "[catalog]\nalias = \"member\"\n[package_manager]\njs = \"bun\"\n[tasks.health]\nrun = \"printf ran > {}\"\n",
            marker.display()
        ),
    )
    .expect("write member manifest");
    fs::write(
        member.join("package.json"),
        r#"{"dependencies":{"left-pad":"1.3.0"}}"#,
    )
    .expect("write member package.json");
    // The parent declares the member and owns the lock and install, so the
    // child legitimately shares it.
    fs::write(
        root.join("package.json"),
        r#"{"private":true,"workspaces":["member"]}"#,
    )
    .expect("write parent package.json");
    fs::write(root.join("bun.lock"), "").expect("write parent lock");
    fs::create_dir_all(root.join("node_modules")).expect("shared install");
    fs::create_dir_all(root.join("node_modules").join("left-pad")).expect("shared dependency");
    fs::write(
        root.join("node_modules/left-pad/package.json"),
        r#"{"name":"left-pad","version":"1.3.0","main":"entry.cjs"}"#,
    )
    .expect("package metadata");
    fs::write(
        root.join("node_modules/left-pad/entry.cjs"),
        "module.exports = true;",
    )
    .expect("package entry");
    write_manifest(
        &root.join("effigy.toml"),
        "[catalog]\nalias = \"root\"\n[catalog.members]\nmember = \"member\"\n",
    );

    let out = match run_deep_doctor_task(root, &["--catalog", "member"]) {
        Ok(out) => out,
        Err(crate::runner::error::RunnerError::DoctorNonZero { rendered, .. }) => rendered,
        Err(other) => panic!("unexpected deep doctor error: {other}"),
    };
    assert!(marker.exists(), "declared member health must run");
    assert_output_excludes_all(&out, &["health.task.bootstrap"]);
}

#[test]
fn run_deep_doctor_rejects_missing_install_for_bun_self_contained_member() {
    let root = temp_workspace("doctor-bun-self-contained-member");
    let member = root.join("member");
    fs::create_dir_all(&member).expect("mkdir member");
    let marker = root.join("member-health-ran");
    fs::write(
        member.join("effigy.toml"),
        format!(
            "[catalog]\nalias = \"member\"\n[package_manager]\njs = \"bun\"\n[tasks.health]\nrun = \"printf ran > {}\"\n",
            marker.display()
        ),
    )
    .expect("write member manifest");
    fs::write(
        member.join("package.json"),
        r#"{"dependencies":{"left-pad":"1.3.0"},"installConfig":{"hoistingLimits":"workspaces"}}"#,
    )
    .expect("write member package.json");
    // A self-contained Bun member keeps dependencies local, so the workspace
    // root install must not cover this member.
    fs::write(
        root.join("package.json"),
        r#"{"private":true,"workspaces":["member"]}"#,
    )
    .expect("write parent package.json");
    fs::write(root.join("bun.lock"), "").expect("write parent lock");
    fs::create_dir_all(root.join("node_modules")).expect("root install");
    write_manifest(
        &root.join("effigy.toml"),
        "[catalog]\nalias = \"root\"\n[catalog.members]\nmember = \"member\"\n",
    );

    let err = run_deep_doctor_task(root, &["--catalog", "member"])
        .expect_err("self-contained member without a local install must fail");
    assert_doctor_non_zero_contains(err, &["health.task.bootstrap", "missing-local-install"]);
    assert!(
        !marker.exists(),
        "self-contained member health must not run on the shared root install"
    );
}

#[test]
fn run_deep_doctor_rejects_missing_install_for_bun_isolated_workspace_member() {
    let root = temp_workspace("doctor-bun-isolated-member");
    let member = root.join("member");
    fs::create_dir_all(&member).expect("mkdir member");
    let marker = root.join("member-health-ran");
    fs::write(
        member.join("effigy.toml"),
        format!(
            "[catalog]\nalias = \"member\"\n[package_manager]\njs = \"bun\"\n[tasks.health]\nrun = \"printf ran > {}\"\n",
            marker.display()
        ),
    )
    .expect("write member manifest");
    fs::write(
        member.join("package.json"),
        r#"{"dependencies":{"left-pad":"1.3.0"}}"#,
    )
    .expect("write member package.json");
    // A configVersion = 1 workspace lock selects Bun's isolated linker, which
    // keeps member dependencies under the member's own node_modules.
    fs::write(
        root.join("package.json"),
        r#"{"private":true,"workspaces":["member"]}"#,
    )
    .expect("write parent package.json");
    fs::write(
        root.join("bun.lock"),
        "{\n  \"lockfileVersion\": 1,\n  \"configVersion\": 1,\n  \"workspaces\": {},\n  \"packages\": {}\n}\n",
    )
    .expect("write parent lock");
    fs::create_dir_all(root.join("node_modules")).expect("root install");
    write_manifest(
        &root.join("effigy.toml"),
        "[catalog]\nalias = \"root\"\n[catalog.members]\nmember = \"member\"\n",
    );

    let err = run_deep_doctor_task(root, &["--catalog", "member"])
        .expect_err("isolated member without a local install must fail");
    assert_doctor_non_zero_contains(err, &["health.task.bootstrap", "missing-local-install"]);
    assert!(
        !marker.exists(),
        "isolated Bun member health must not run on the shared root install"
    );
}

#[test]
fn run_deep_doctor_rejects_missing_install_for_npm_nested_member() {
    let root = temp_workspace("doctor-npm-nested-member");
    let member = root.join("member");
    fs::create_dir_all(&member).expect("mkdir member");
    let marker = root.join("member-health-ran");
    fs::write(
        member.join("effigy.toml"),
        format!(
            "[catalog]\nalias = \"member\"\n[package_manager]\njs = \"npm\"\n[tasks.health]\nrun = \"printf ran > {}\"\n",
            marker.display()
        ),
    )
    .expect("write member manifest");
    fs::write(
        member.join("package.json"),
        r#"{"dependencies":{"left-pad":"1.3.0"}}"#,
    )
    .expect("write member package.json");
    // npm's non-hoisted install strategies keep dependencies member-local.
    fs::write(
        root.join("package.json"),
        r#"{"private":true,"workspaces":["member"]}"#,
    )
    .expect("write parent package.json");
    fs::write(root.join("package-lock.json"), "").expect("write parent lock");
    fs::write(root.join(".npmrc"), "install-strategy=nested\n").expect("write npmrc");
    fs::create_dir_all(root.join("node_modules")).expect("root install");
    write_manifest(
        &root.join("effigy.toml"),
        "[catalog]\nalias = \"root\"\n[catalog.members]\nmember = \"member\"\n",
    );

    let err = run_deep_doctor_task(root, &["--catalog", "member"])
        .expect_err("nested npm member without a local install must fail");
    assert_doctor_non_zero_contains(err, &["health.task.bootstrap", "missing-local-install"]);
    assert!(
        !marker.exists(),
        "nested npm member health must not run on the shared root install"
    );
}

#[test]
fn run_deep_doctor_rejects_missing_install_for_pnpm_isolated_member() {
    let root = temp_workspace("doctor-pnpm-isolated-member");
    let member = root.join("member");
    fs::create_dir_all(&member).expect("mkdir member");
    let marker = root.join("member-health-ran");
    fs::write(
        member.join("effigy.toml"),
        format!(
            "[catalog]\nalias = \"member\"\n[package_manager]\njs = \"pnpm\"\n[tasks.health]\nrun = \"printf ran > {}\"\n",
            marker.display()
        ),
    )
    .expect("write member manifest");
    fs::write(
        member.join("package.json"),
        r#"{"dependencies":{"left-pad":"1.3.0"}}"#,
    )
    .expect("write member package.json");
    // pnpm's default isolated layout links each project's dependencies under
    // that project's own node_modules.
    fs::write(root.join("pnpm-lock.yaml"), "").expect("write parent lock");
    fs::write(
        root.join("pnpm-workspace.yaml"),
        "packages:\n  - 'member'\n",
    )
    .expect("write pnpm workspace");
    fs::create_dir_all(root.join("node_modules")).expect("root install");
    write_manifest(
        &root.join("effigy.toml"),
        "[catalog]\nalias = \"root\"\n[catalog.members]\nmember = \"member\"\n",
    );

    let err = run_deep_doctor_task(root, &["--catalog", "member"])
        .expect_err("isolated pnpm member without a local install must fail");
    assert_doctor_non_zero_contains(err, &["health.task.bootstrap", "missing-local-install"]);
    assert!(
        !marker.exists(),
        "isolated pnpm member health must not run on the workspace root install"
    );
}

#[test]
fn run_doctor_reports_stale_graph_index_with_refresh_remediation() {
    let root = temp_workspace("doctor-stale-graph-index");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.health]\nrun = \"printf health-ok\"\n",
    );
    fs::write(
        root.join("src/lib.rs"),
        "pub fn release_graph() { helper(); }\nfn helper() {}\n",
    )
    .expect("write rust");
    effigy_codegraph::run_index(&root).expect("graph index");
    fs::write(
        root.join("src/lib.rs"),
        "pub fn release_graph() { helper(); helper(); }\nfn helper() {}\n",
    )
    .expect("rewrite rust");

    let out = run_doctor_task(root, &[]).expect("doctor run");
    assert_output_contains_all(
        &out,
        &[
            "graph.index",
            "graph index is not current",
            "effigy graph status --refresh",
        ],
    );
}

#[test]
fn run_doctor_does_not_flag_missing_graph_index() {
    let root = temp_workspace("doctor-no-graph-index");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.health]\nrun = \"printf health-ok\"\n",
    );
    fs::write(root.join("src/lib.rs"), "pub fn release_graph() {}\n").expect("write rust");

    let out = run_doctor_task(root, &[]).expect("doctor run");
    assert_output_contains_all(&out, &["No findings."]);
    assert_output_excludes_all(&out, &["graph.index", "graph status --refresh"]);
}

#[test]
fn run_doctor_fix_scaffolds_health_task_when_missing() {
    let root = temp_workspace("doctor-fix-scaffold-health");
    write_manifest(
        &root.join("effigy.toml"),
        "[tasks.build]\nrun = \"printf ok\"\n",
    );

    let out = run_doctor_task(root.to_path_buf(), &["--fix"]).expect("doctor --fix");

    assert_file_text_contains_all(
        &root.join("effigy.toml"),
        &["health = \"printf health-check-placeholder\""],
    );
    assert_output_contains_all(
        &out,
        &["Fix Actions", "manifest.health_task_scaffold", "applied"],
    );
}

#[test]
fn run_doctor_fix_reports_skipped_when_manifest_invalid() {
    let root = temp_workspace("doctor-fix-invalid-manifest");
    fs::write(root.join("effigy.toml"), "[tasks\nbad = true\n").expect("write bad manifest");

    let err = run_doctor_err_from_cwd(&root, true);
    assert_doctor_non_zero_contains(
        err,
        &["Fix Actions", "manifest.health_task_scaffold", "skipped"],
    );
}

#[test]
fn run_doctor_reports_tasks_test_migration_remediation() {
    let root = temp_workspace("doctor-rejects-tasks-test");
    fs::write(
        root.join("effigy.toml"),
        "[tasks.test]\nrun = \"cargo test\"\n",
    )
    .expect("write legacy manifest");

    let err = run_doctor_err_from_cwd(&root, false);
    assert_doctor_non_zero_contains(
        err,
        &[
            "`tasks.test` was removed in v0.11",
            "move the command to a named `[test.suites]` entry",
        ],
    );
}

#[test]
fn run_doctor_removes_stale_scan_detail_report_when_scan_findings_clear() {
    let root = temp_workspace("doctor-removes-stale-scan-detail-report");
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_manifest(
        &root.join("effigy.toml"),
        r#"[scan.god_files]
warn = 10
high = 12
critical = 20
"#,
    );
    fs::write(root.join("src/app.ts"), "const a = 1;\n".repeat(14)).expect("write source");

    let _ = run_deep_doctor_task(root.clone(), &[]).expect_err("doctor should fail");
    let report_path = root.join(".effigy/reports/doctor/scan-god-files.md");
    assert!(report_path.exists(), "expected initial scan detail report");

    fs::write(root.join("src/app.ts"), "const a = 1;\n").expect("rewrite source");
    let out = run_deep_doctor_task(root, &[]).expect("doctor should succeed");

    assert_output_excludes_all(&out, &["scan.god-files"]);
    assert!(
        !report_path.exists(),
        "expected stale scan detail report to be removed"
    );
}
