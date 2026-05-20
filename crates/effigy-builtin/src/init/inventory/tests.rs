use super::{
    build_setup_inventory, render_follow_up_jobs_excluding, SetupApplicability, SetupCategory,
};
use crate::init::agent::{collect_agent_checks, load_agent_init_assets};
use crate::init::request::AgentInitMode;
use crate::init::scaffold;
use std::fs;
use std::path::PathBuf;

fn temp_root(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "effigy-init-inventory-{name}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("create temp root");
    path
}

#[test]
fn inventory_detects_contextual_setup_surfaces() {
    let root = temp_root("context");
    fs::write(
        root.join("package.json"),
        "{ \"scripts\": { \"dev\": \"vite\" } }\n",
    )
    .expect("package");
    fs::write(
        root.join("effigy.toml"),
        "[bundle]\nbase = { type = \"path\", dir = \"bundle\" }\n\n[secrets]\nbackend = \"effigy-vault\"\n\n[containers]\nbackend = \"docker\"\n\n[state]\n\n[deploy]\n\n[distribution]\n\n[release]\n\n[tasks.qa]\nrun = \"printf qa\"\n",
    )
    .expect("manifest");
    let assets = load_agent_init_assets(|| scaffold::load_starter("minimal")).expect("assets");
    let checks = collect_agent_checks(&root, &assets, AgentInitMode::Check, None).expect("checks");
    let jobs = build_setup_inventory(&root, &checks);

    assert!(jobs
        .iter()
        .any(|job| job.id == "task_migration.package_json"
            && job.applicability == SetupApplicability::Applicable));
    assert!(jobs
        .iter()
        .any(|job| job.id == "bundle_sync.run" && job.category == SetupCategory::Bundles));
    assert!(jobs.iter().any(|job| job.id == "secrets_vault.init"));
    assert!(jobs.iter().any(|job| job.id == "release_surface.inspect"));
}

#[test]
fn follow_up_renderer_surfaces_real_commands() {
    let root = temp_root("followup");
    fs::write(
        root.join("package.json"),
        "{ \"scripts\": { \"build\": \"vite build\" } }\n",
    )
    .expect("package");
    fs::write(
        root.join("effigy.toml"),
        "[bundle]\nbase = { type = \"path\", dir = \"bundle\" }\n",
    )
    .expect("manifest");
    let assets = load_agent_init_assets(|| scaffold::load_starter("minimal")).expect("assets");
    let checks = collect_agent_checks(&root, &assets, AgentInitMode::Check, None).expect("checks");
    let jobs = build_setup_inventory(&root, &checks);
    let rendered = render_follow_up_jobs_excluding(&jobs, &std::collections::BTreeSet::new());
    assert!(rendered.contains("effigy tasks migrate"));
    assert!(rendered.contains("effigy bundle inspect"));
    assert!(rendered.contains("effigy graph status --json"));
}
