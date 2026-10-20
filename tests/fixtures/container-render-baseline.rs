//! Private qualification helper, compiled against the pre-cutover main tree.
//! Only `inspect` accepts real checkouts; pin/render require the private fixture.
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let root = Path::new(args.get(2).ok_or("consumer root missing")?);
    let manifest = root.join("effigy.toml");
    match args.get(1).map(String::as_str).ok_or("mode missing")? {
        "inspect" => {
            let inspected = effigy_manifest::load_task_manifest_with_inspection(&manifest)?;
            let spec: Option<effigy_manifest::ManifestBundleBase> = inspected
                .effective_value
                .get("bundle")
                .and_then(|v| v.get("base"))
                .cloned()
                .map(toml::Value::try_into)
                .transpose()?;
            let legacy = if let Some(effigy_manifest::ManifestBundleBase::Git { url, r#ref }) = spec
            {
                Some(
                    PathBuf::from(".effigy/cache/bundles/git")
                        .join(effigy_core::git_source::sha256_hex(
                            effigy_core::git_source::canonical_git_cache_identity(&url).as_bytes(),
                        ))
                        .join(effigy_core::git_source::sanitize_cache_segment(
                            r#ref.as_deref().unwrap_or("main"),
                        )),
                )
            } else {
                None
            };
            println!(
                "{}",
                serde_json::json!({"legacy":legacy,"bundle": inspected.bundle_root,
                "inputs": inspected.evaluation_order, "effective": inspected.effective_value})
            );
        }
        "pin" => {
            require_private(root)?;
            let home = PathBuf::from(std::env::var_os("HOME").ok_or("HOME missing")?);
            let preparation = effigy_manifest::SourcePreparation {
                home,
                executable: PathBuf::from(
                    std::env::var_os("EFFIGY_RENDER_PROOF_EXECUTABLE")
                        .ok_or("kernel-capable executable missing")?,
                ),
                cancel: Default::default(),
                budget: Duration::from_secs(30),
                next: effigy_kernel::outcome::Invocation::validated(
                    vec!["bundle".into(), "sync".into()],
                    |v| v == ["bundle", "sync"],
                )
                .expect("static recovery invocation is validated"),
            };
            effigy_manifest::prepare_bundle_sources(&manifest, &preparation)?;
        }
        "render" => {
            require_private(root)?;
            let policy = effigy_containers::load_container_policy(
                root,
                Some(args.get(3).ok_or("environment missing")?),
            )?;
            let mut files = BTreeMap::new();
            for path in policy.compose_files.iter().chain(
                policy
                    .shared_services
                    .iter()
                    .map(|binding| &binding.compose_file),
            ) {
                files.insert(path.clone(), std::fs::read_to_string(path)?);
            }
            println!(
                "{}",
                serde_json::json!({"compose_files": policy.compose_files,
                "files": files, "project": policy.project_name})
            );
        }
        _ => return Err("expected inspect, pin or render".into()),
    }
    Ok(())
}

fn require_private(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let owner =
        PathBuf::from(std::env::var_os("EFFIGY_RENDER_PROOF_ROOT").ok_or("proof root missing")?);
    if !root.canonicalize()?.starts_with(owner.canonicalize()?) {
        return Err("refusing to mutate outside the private proof root".into());
    }
    Ok(())
}
