use super::*;
use crate::{SecretValue, VaultSecretRecord};
use std::fs;

fn fixture(legacy: bool) -> (tempfile::TempDir, PathBuf) {
    let home = tempfile::tempdir().expect("fresh HOME");
    let path = home.path().join("vault");
    let mut payload = VaultPlaintextPayload::empty();
    payload.records.insert(
        "original".into(),
        VaultSecretRecord::new(SecretValue::new("kept")),
    );
    if legacy {
        let envelope = payload
            .encrypt_with_passphrase("old")
            .expect("encrypt legacy");
        fs::write(&path, envelope.to_json_pretty().expect("serialize")).expect("legacy");
    } else {
        VaultWrite::acquire_in(home.path(), &path)
            .expect("gate")
            .write_payload(&payload, "old")
            .expect("initial commit");
    }
    (home, path)
}
fn rotate(home: &Path, path: &Path) -> Result<(), VaultStorageError> {
    let write = VaultWrite::acquire_in(home, path)?;
    let mut payload = write.read()?.decrypt_with_passphrase("old")?;
    payload.records.insert(
        "added".into(),
        VaultSecretRecord::new(SecretValue::new("new-value")),
    );
    write.write_payload(&payload, "new")
}
fn assert_complete(path: &Path) {
    let envelope = read_envelope(path).expect("complete selected envelope");
    if let Ok(payload) = envelope.decrypt_with_passphrase("old") {
        assert_eq!(payload.records.len(), 1);
        assert_eq!(payload.records["original"].value.expose(), "kept");
    } else {
        let payload = envelope
            .decrypt_with_passphrase("new")
            .expect("new generation decrypts");
        assert_eq!(payload.records.len(), 2);
        assert_eq!(payload.records["original"].value.expose(), "kept");
        assert_eq!(payload.records["added"].value.expose(), "new-value");
    }
}

#[test]
fn vault_migrates_legacy_and_retains_previous_decryptable_generation() {
    let (home, path) = fixture(true);
    rotate(home.path(), &path).expect("rotation");
    assert_complete(&path);
    let previous = path.with_extension("prev");
    assert!(read_envelope(&previous)
        .expect("legacy previous")
        .decrypt_with_passphrase("old")
        .is_ok());
    let write = VaultWrite::acquire_in(home.path(), &path).expect("gate");
    let payload = write
        .read()
        .expect("read")
        .decrypt_with_passphrase("new")
        .expect("decrypt");
    write
        .write_payload(&payload, "third")
        .expect("next generation");
    assert!(read_envelope(&previous)
        .expect("previous pointer")
        .decrypt_with_passphrase("new")
        .is_ok());
    assert!(read_envelope(&path)
        .expect("selected")
        .decrypt_with_passphrase("third")
        .is_ok());
}

#[test]
fn vault_gate_requires_no_runtime_database() {
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("vault.json");
    let _write = VaultWrite::acquire_in(home.path(), &path).expect("private vault gate");
    assert!(!home.path().join(".effigy/state/kernel.db").exists());
    assert!(!home.path().join(".effigy/state/machine.db").exists());
}

#[test]
fn vault_refuses_unreadable_state_without_overwriting_it() {
    let (home, path) = fixture(false);
    let raw = fs::read(&path).expect("pointer");
    fs::write(&path, "broken").expect("damage");
    let write = VaultWrite::acquire_in(home.path(), &path).expect("gate");
    let envelope = VaultPlaintextPayload::empty()
        .encrypt_with_passphrase("new")
        .expect("encrypt");
    assert!(write.commit(&envelope).is_err());
    assert_eq!(fs::read(&path).expect("still damaged"), b"broken");
    fs::write(&path, raw).expect("restore pointer");
    assert!(write
        .read()
        .expect("selected generation")
        .decrypt_with_passphrase("old")
        .is_ok());
}

#[test]
fn vault_concurrent_process_writers_keep_both_updates() {
    const CHILD: &str = "EFFIGY_TEST_VAULT_WRITER";
    if let Ok(name) = std::env::var(CHILD) {
        let home = PathBuf::from(std::env::var_os("EFFIGY_TEST_VAULT_HOME").expect("HOME"));
        let path = home.join("vault");
        let write = VaultWrite::acquire_in(&home, &path).expect("gate");
        let mut payload = write
            .read()
            .expect("read")
            .decrypt_with_passphrase("old")
            .expect("decrypt");
        // Make an unlocked read/modify/write lose an update deterministically.
        std::thread::sleep(Duration::from_millis(100));
        payload
            .records
            .insert(name.clone(), VaultSecretRecord::new(SecretValue::new(name)));
        write.write_payload(&payload, "old").expect("commit");
        return;
    }
    let (home, path) = fixture(false);
    let spawn = |name| {
        std::process::Command::new(std::env::current_exe().expect("test exe"))
            .args([
                "--exact",
                "storage::tests::vault_concurrent_process_writers_keep_both_updates",
                "--nocapture",
            ])
            .env(CHILD, name)
            .env("EFFIGY_TEST_VAULT_HOME", home.path())
            .spawn()
            .expect("writer")
    };
    let mut first = spawn("cli");
    let mut second = spawn("rhai");
    assert!(first.wait().expect("first").success());
    assert!(second.wait().expect("second").success());
    let payload = read_envelope(&path)
        .expect("envelope")
        .decrypt_with_passphrase("old")
        .expect("decrypt");
    assert_eq!(payload.records.len(), 3);
    for name in ["cli", "rhai"] {
        assert_eq!(payload.records[name].value.expose(), name);
    }
}

#[cfg(feature = "fault-injection")]
#[test]
fn faults_engine_vault_rotation() {
    use effigy_kernel::fault_injection::with_hook;
    use std::cell::RefCell;
    use std::rc::Rc;
    const CHILD: &str = "EFFIGY_TEST_VAULT_FAULT";
    if let Ok(target) = std::env::var(CHILD) {
        let target: usize = target.parse().expect("crossing");
        let home = PathBuf::from(std::env::var_os("EFFIGY_TEST_VAULT_HOME").expect("HOME"));
        let mut count = 0;
        with_hook(
            move |_| {
                if count == target {
                    let _ = std::process::Command::new("/bin/kill")
                        .args(["-KILL", &std::process::id().to_string()])
                        .status();
                    panic!("SIGKILL did not terminate child");
                }
                count += 1;
                Ok(())
            },
            || rotate(&home, &home.join("vault")),
        )
        .expect("rotation");
        panic!("fault not delivered");
    }
    for legacy in [true, false] {
        let (home, path) = fixture(legacy);
        let points = Rc::new(RefCell::new(Vec::new()));
        let trace = points.clone();
        with_hook(
            move |point| {
                trace.borrow_mut().push(point);
                Ok(())
            },
            || rotate(home.path(), &path),
        )
        .expect("trace");
        let points = points.borrow();
        for required in [
            "GateAcquire",
            "ReplaceOpen",
            "ReplaceWrite",
            "ReplaceFsync",
            "ReplaceLinkPrev",
            "ReplaceRename",
            "ReplaceDirFsync",
        ] {
            assert!(points.contains(&required), "missing {required}");
        }
        for (target, point) in points.iter().enumerate() {
            for errno in [1, 28] {
                // EPERM and ENOSPC on both supported hosts.
                let (home, path) = fixture(legacy);
                let fired = Rc::new(std::cell::Cell::new(false));
                let delivered = fired.clone();
                let mut count = 0;
                let result = with_hook(
                    move |_| {
                        let here = count;
                        count += 1;
                        if here == target {
                            delivered.set(true);
                            Err(std::io::Error::from_raw_os_error(errno))
                        } else {
                            Ok(())
                        }
                    },
                    || rotate(home.path(), &path),
                );
                assert!(fired.get(), "undelivered {point} #{target}");
                assert!(result.is_err(), "ignored {point} #{target}");
                assert_complete(&path);
                // Recovery reacquires the released gate and commits another
                // complete generation; leftovers cannot shadow the selection.
                let write = VaultWrite::acquire_in(home.path(), &path).expect("recovery gate");
                let envelope = read_envelope(&path).expect("selected");
                write.commit(&envelope).expect("recovery write");
                assert_complete(&path);
            }
            let (home, path) = fixture(legacy);
            let status = std::process::Command::new(std::env::current_exe().expect("test exe"))
                .args([
                    "--exact",
                    "storage::tests::faults_engine_vault_rotation",
                    "--nocapture",
                ])
                .env(CHILD, target.to_string())
                .env("EFFIGY_TEST_VAULT_HOME", home.path())
                .status()
                .expect("kill fixture");
            use std::os::unix::process::ExitStatusExt;
            assert_eq!(status.signal(), Some(9), "{point} #{target}");
            assert_complete(&path);
            let write = VaultWrite::acquire_in(home.path(), &path).expect("gate after kill");
            write
                .commit(&read_envelope(&path).expect("selected"))
                .expect("recover after kill");
            assert_complete(&path);
        }
    }
}
