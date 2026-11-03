//! Uninstalled qualification fixture utility. No product execution toggle.
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [command, path] if command == "prepare" => {
            let root = Path::new(path).canonicalize()?;
            let scratch = Path::new("/Volumes/Builds/scratch/effigy").canonicalize()?;
            if !root.starts_with(&scratch) || root == scratch || !root.join(".git").is_dir() || root.join(".git").is_symlink() {
                return Err("qualification fixture must be a fresh scratch checkout".into());
            }
            // The real checkout identity producer, never a manufactured token.
            let token = effigy_core::worktree_scope::prepare_checkout_token(&root)?;
            prepare_vault(&root)?;
            println!("{token}");
        }
        [command, pid] if command == "identity" => {
            match effigy_kernel::ident::process_identity(pid.parse()?) {
                effigy_kernel::observe::Observed::Verified(identity, _) => {
                    println!("{}", serde_json::to_string(&identity)?);
                }
                _ => return Err("process identity is unavailable".into()),
            }
        }
        [command, path, run] if command == "crash" => {
            crash_selected_supervisor(Path::new(path), run)?;
        }
        _ => return Err("expected prepare <scratch-checkout>, identity <pid>, or crash <scratch-checkout> <supervisor-run>".into()),
    }
    Ok(())
}

fn crash_selected_supervisor(path: &Path, id: &str) -> Result<(), Box<dyn std::error::Error>> {
    use effigy_kernel::ctx::*;
    use effigy_kernel::observe::Observed;
    use effigy_kernel::run;
    use std::time::{Duration, Instant};
    let path = path.canonicalize()?;
    let scratch = Path::new("/Volumes/Builds/scratch/effigy").canonicalize()?;
    if !path.starts_with(&scratch) || path == scratch {
        return Err("foreign fixture".into());
    }
    let observe = OperationContext::new(
        Deadline::At(Instant::now() + Duration::from_secs(2)),
        CancelToken::default(),
        Limits {
            capture_tail: 1024,
            log_rotate: 4096,
            log_keep: 2,
            line_max: 512,
            queue_bytes: 2048,
            probe_concurrency: 1,
        },
        EffectClass::Mutate,
        Placement::Host,
    )
    .observe()?;
    let root = match effigy_containers::environment::observe_root(&observe, &path) {
        Observed::Verified(root, _) => root,
        _ => return Err("fixture state unavailable".into()),
    };
    let reader = match effigy_kernel::store::Store::read(&observe, &root) {
        Observed::Verified(reader, _) => reader,
        _ => return Err("fixture journal unavailable".into()),
    };
    let run = id.parse()?;
    let summary = match run::summary(&reader, run) {
        Observed::Verified(summary, _) => summary,
        _ => return Err("fixture run unavailable".into()),
    };
    if !summary
        .invocation
        .as_ref()
        .is_some_and(|inv| inv.spec().task.task() == "workspace-serving")
    {
        return Err("fault target is not this fixture's serving supervisor".into());
    }
    let identity = match run::metadata(&observe, &root, run) {
        Observed::Verified(metadata, _) => metadata.owner.ok_or("unrecorded supervisor birth")?,
        _ => return Err("supervisor metadata unavailable".into()),
    };
    let delivered = effigy_kernel::custody::signal_verified_process(
        &identity,
        9,
        || matches!(run::summary(&reader, run), Observed::Verified(current, _) if current.phase != run::Phase::Closed && current.generation == summary.generation && current.invocation.as_ref().map(|inv| inv.digest()) == summary.invocation.as_ref().map(|inv| inv.digest())),
    );
    if !matches!(delivered, Observed::Verified(_, _)) {
        return Err("exact supervisor signal not verified".into());
    }
    println!("{{\"exact_supervisor_fault\":true}}");
    Ok(())
}

fn prepare_vault(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    use effigy_secrets::{
        local_dev_unlock_key_path, local_unlock_credential_path, seal_local_unlock_passphrase,
        LocalDevUnlockKey, SecretValue, VaultPlaintextPayload, VaultSecretRecord,
    };
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let state = root.join(".effigy");
    if state.is_symlink() {
        return Err("qualification state directory is a symlink".into());
    }
    if !state.exists() {
        std::fs::create_dir(&state)?;
    }
    let directory = state.join("secrets");
    // Setup owns only a newly created directory; it never repairs or replaces
    // a pre-existing vault, key, unlock credential or permission mode.
    std::fs::create_dir(&directory)?;
    std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))?;
    let vault = directory.join("local.vault");
    let key = LocalDevUnlockKey::generate()?;
    let mut payload = VaultPlaintextPayload::empty();
    payload.records.insert(
        "probe_token".into(),
        VaultSecretRecord::new(SecretValue::new("531-private-compose-canary")),
    );
    let passphrase = "531-isolated-fixture-passphrase";
    let envelope = payload.encrypt_with_passphrase_and_local_dev_key(passphrase, &key)?;
    let write_new = |path: &Path, bytes: &[u8]| -> Result<(), std::io::Error> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)?;
        file.write_all(bytes)?;
        file.sync_all()
    };
    write_new(&vault, envelope.to_json_pretty()?.as_bytes())?;
    write_new(&local_dev_unlock_key_path(&vault), key.expose())?;
    write_new(
        &local_unlock_credential_path(&vault),
        &seal_local_unlock_passphrase(&key, passphrase)?,
    )?;
    Ok(())
}
