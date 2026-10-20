//! `compose_file` is read as project catalog fragments for one release, and
//! `effigy container migrate` writes exactly those fragments out.
use effigy_test_support::Fixture as DirectoryFixture;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

struct Fixture {
    owner: DirectoryFixture,
    repo: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let owner = DirectoryFixture::workspace("effigy-migrate-").unwrap();
        let root = owner.path().canonicalize().unwrap();
        let repo = root.join("checkout");
        for directory in [repo.join("infra/dev"), root.join("home")] {
            fs::create_dir_all(directory).unwrap();
        }
        fs::write(
            repo.join("effigy.toml"),
            "[containers]\ndefault = \"web\"\n\n# The web stack.\n[containers.web]\ncompose_file = \"infra/dev/docker-compose.yml\"\nprimary_service = \"app\"\n\n[containers.web.host]\nmounts = [\"./:/srv\"]\n",
        )
        .unwrap();
        fs::write(
            repo.join("infra/dev/docker-compose.yml"),
            "services:\n  app:\n    build: ../..\n    volumes: [\"../..:/srv\", \"cache:/cache\"]\n    ports: [\"8080:80\"]\nvolumes:\n  cache:\n",
        )
        .unwrap();
        Self { owner, repo }
    }

    fn run(&self, args: &[&str]) -> Output {
        let root = self.owner.path().canonicalize().unwrap();
        Command::new(env!("CARGO_BIN_EXE_effigy"))
            .args(args)
            .current_dir(&self.repo)
            .env("HOME", root.join("home"))
            .env("EFFIGY_TEST_CONTAINERS_HOME", root.join("home/.effigy"))
            .env("EFFIGY_COMPOSE_BACKEND", "docker")
            .output()
            .unwrap()
    }

    fn json(&self, args: &[&str]) -> Value {
        let output = self.run(args);
        assert!(output.status.success(), "{args:?}: {output:?}");
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        value.get("result").cloned().unwrap_or(value)
    }

    /// Everything Compose receives; declaration inputs differ by construction.
    fn render(&self) -> BTreeMap<String, String> {
        self.json(&["container", "render", "web", "--json"])["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|file| {
                (
                    file["name"].as_str().unwrap().to_owned(),
                    file["content"].as_str().unwrap().to_owned(),
                )
            })
            .filter(|(name, _)| !name.starts_with("inputs/"))
            .collect()
    }
}

#[test]
fn migrate_writes_the_fragment_compose_file_is_read_as_and_keeps_the_render() {
    let f = Fixture::new();
    let warned = f.run(&["container", "render", "web"]);
    assert!(warned.status.success(), "{warned:?}");
    let stderr = String::from_utf8_lossy(&warned.stderr);
    assert!(
        stderr.contains("`[containers.web].compose_file` is deprecated")
            && stderr.contains("effigy container migrate web"),
        "{stderr}"
    );
    let before = f.render();

    let report = f.json(&["container", "migrate", "web", "--json"]);
    assert_eq!(report["schema"], "effigy.container.migrate.v1", "{report}");
    assert_eq!(report["render_unchanged"], true);
    assert_eq!(report["fragments"][0]["name"], "web-app");
    assert_eq!(report["fragments"][0]["path"], "infra/dev/catalog/web-app");

    let manifest = fs::read_to_string(f.repo.join("effigy.toml")).unwrap();
    assert!(!manifest.contains("compose_file"), "{manifest}");
    assert!(manifest.contains("# The web stack."), "{manifest}");
    assert!(
        manifest.contains("[containers.web.services.app]\ncatalog = \"web-app\""),
        "{manifest}"
    );
    // Ports follow catalog port policy; no `[host].ports` is invented.
    assert!(!manifest.contains("ports ="), "{manifest}");
    let fragment = f.repo.join("infra/dev/catalog/web-app");
    assert!(fragment.join("service.toml").is_file());
    let compose = fs::read_to_string(fragment.join("compose.fragment.yml")).unwrap();
    assert!(
        compose.contains("'{{ project_name }}-cache:/cache'"),
        "{compose}"
    );

    // The Compose file is no longer read.
    fs::remove_file(f.repo.join("infra/dev/docker-compose.yml")).unwrap();
    assert_eq!(f.render(), before);
    let quiet = f.run(&["container", "render", "web"]);
    assert!(!String::from_utf8_lossy(&quiet.stderr).contains("deprecated"));

    let again = f.run(&["container", "migrate", "web"]);
    assert!(!again.status.success());
    assert!(
        String::from_utf8_lossy(&again.stderr).contains("nothing to migrate"),
        "{again:?}"
    );
}

#[test]
fn eject_is_retired_and_names_the_render_command() {
    let f = Fixture::new();
    let output = f.run(&["container", "web", "eject"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("`effigy container eject` is retired"),
        "{stderr}"
    );
    assert!(
        stderr.contains("effigy container render <NAME>"),
        "{stderr}"
    );
    assert!(!f.repo.join("infra/dev/catalog").exists());
}
