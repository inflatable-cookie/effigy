use std::path::Path;

const DOCKER_ENV_PATH: &str = "/.dockerenv";
const PODMAN_ENV_PATH: &str = "/run/.containerenv";
const CGROUP_PATH: &str = "/proc/1/cgroup";
const EFFIGY_WORKSPACE_ENTRYPOINT_PATH: &str = "/usr/local/bin/effigy-entrypoint";

pub fn process_is_inside_container() -> bool {
    let dockerenv = Path::new(DOCKER_ENV_PATH).exists();
    let containerenv = Path::new(PODMAN_ENV_PATH).exists();
    let effigy_workspace = process_is_inside_effigy_workspace_container();
    let cgroup = std::fs::read_to_string(CGROUP_PATH).unwrap_or_default();
    process_markers_indicate_container(dockerenv, containerenv, effigy_workspace, &cgroup)
}

pub fn process_is_inside_effigy_workspace_container() -> bool {
    Path::new(EFFIGY_WORKSPACE_ENTRYPOINT_PATH).exists()
}

pub fn process_markers_indicate_container(
    dockerenv: bool,
    containerenv: bool,
    effigy_workspace: bool,
    cgroup: &str,
) -> bool {
    dockerenv
        || containerenv
        || effigy_workspace
        || cgroup.contains("docker")
        || cgroup.contains("containerd")
        || cgroup.contains("kubepods")
}

#[cfg(test)]
mod tests {
    #[test]
    fn process_markers_detect_common_and_effigy_workspace_containers() {
        assert!(super::process_markers_indicate_container(
            true, false, false, ""
        ));
        assert!(super::process_markers_indicate_container(
            false, true, false, ""
        ));
        assert!(super::process_markers_indicate_container(
            false, false, true, "0::/"
        ));
        assert!(super::process_markers_indicate_container(
            false,
            false,
            false,
            "0::/docker/123"
        ));
        assert!(!super::process_markers_indicate_container(
            false, false, false, "0::/"
        ));
    }
}
