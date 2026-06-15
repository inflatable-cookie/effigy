use crate::runner::tests::prelude::{
    assert_output_contains_all, run_builtin_ok, temp_workspace, write_root_manifest,
};
use effigy_manifest::{with_test_user_config_home, USER_CONFIG_FILE};

#[test]
fn run_manifest_task_builtin_config_can_manage_user_global_container_preferences() {
    let root = temp_workspace("builtin-config-user-global");
    write_root_manifest(&root, "");
    let user_home = temp_workspace("builtin-config-user-global-home");

    with_test_user_config_home(&user_home, || {
        let path_out = run_builtin_ok(root.clone(), "config", &["path"]);
        let config_path = user_home.join(USER_CONFIG_FILE);
        assert_eq!(path_out.trim(), config_path.display().to_string());

        let out = run_builtin_ok(
            root.clone(),
            "config",
            &["set", "containers.backend", "containerd"],
        );
        let set_profile = run_builtin_ok(
            root.clone(),
            "config",
            &["set", "containers.profile", "effigy"],
        );
        let set_disk = run_builtin_ok(
            root.clone(),
            "config",
            &["set", "containers.profile_disk_gib", "180"],
        );
        let rendered = std::fs::read_to_string(&config_path).expect("user config written");

        assert_output_contains_all(&out, &["User Config", "backend: containerd"]);
        assert_output_contains_all(&set_profile, &["User Config", "profile: effigy"]);
        assert_output_contains_all(&set_disk, &["User Config", "profile_disk_gib: 180"]);
        assert!(rendered.contains("[containers]"));
        assert!(rendered.contains("backend = \"containerd\""));
        assert!(rendered.contains("profile = \"effigy\""));
        assert!(rendered.contains("profile_disk_gib = 180"));

        let inspect = run_builtin_ok(root.clone(), "config", &["path"]);
        assert_eq!(inspect.trim(), config_path.display().to_string());

        let inspect = run_builtin_ok(root.clone(), "config", &["--user-inspect"]);
        assert_output_contains_all(
            &inspect,
            &[
                "User Config",
                "Status: present",
                "backend: containerd",
                "profile: effigy",
                "profile_disk_gib: 180",
            ],
        );

        let get_backend = run_builtin_ok(root.clone(), "config", &["get", "containers.backend"]);
        assert_eq!(get_backend.trim(), "containerd");
        let get_profile = run_builtin_ok(root.clone(), "config", &["get", "containers.profile"]);
        assert_eq!(get_profile.trim(), "effigy");
        let get_disk = run_builtin_ok(
            root.clone(),
            "config",
            &["get", "containers.profile_disk_gib"],
        );
        assert_eq!(get_disk.trim(), "180");

        let unset = run_builtin_ok(root.clone(), "config", &["unset", "containers.backend"]);
        assert_output_contains_all(&unset, &["backend: (unset)"]);
        let unset_profile =
            run_builtin_ok(root.clone(), "config", &["unset", "containers.profile"]);
        assert_output_contains_all(&unset_profile, &["profile: (unset)"]);
        let unset_disk = run_builtin_ok(
            root.clone(),
            "config",
            &["unset", "containers.profile_disk_gib"],
        );
        assert_output_contains_all(&unset_disk, &["profile_disk_gib: (unset)"]);
        let inspect_after = run_builtin_ok(root.clone(), "config", &["--user-inspect"]);
        assert_output_contains_all(
            &inspect_after,
            &[
                "User Config",
                "Status: not yet created",
                "backend: (unset)",
                "profile: (unset)",
                "profile_disk_gib: (unset)",
            ],
        );
        assert!(
            !config_path.exists(),
            "expected empty user config file to be removed"
        );
    });
}
