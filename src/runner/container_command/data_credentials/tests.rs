use super::{wrap_credential_bootstrap, DataCredentialFile};
use effigy_test_support::Fixture;
use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

struct Home {
    _fixture: Fixture,
    home: PathBuf,
}

fn fixture_home(prefix: &str) -> Home {
    let fixture = Fixture::workspace(prefix).expect("fixture");
    let home = fixture.path().join("home");
    fs::create_dir_all(&home).expect("mkdir home");
    Home {
        _fixture: fixture,
        home,
    }
}

fn env() -> BTreeMap<String, String> {
    BTreeMap::from([("MYSQL_PWD".to_owned(), "secret".to_owned())])
}

fn credential_entries(home: &Path) -> Vec<PathBuf> {
    fs::read_dir(home.join(".effigy/data-credentials"))
        .expect("credentials directory")
        .collect::<Result<Vec<_>, _>>()
        .expect("credential entries")
        .into_iter()
        .map(|entry| entry.path())
        .collect()
}

#[test]
fn a_credential_file_is_private_complete_and_removed() {
    let fixture = fixture_home("effigy-data-credential-private-");
    let credential = DataCredentialFile::write(&fixture.home, &env()).expect("write");

    let metadata = fs::symlink_metadata(credential.path()).expect("metadata");
    assert_eq!(
        metadata.permissions().mode() & 0o777,
        0o600,
        "0600 required"
    );
    assert_eq!(
        fs::read_to_string(credential.path()).expect("read"),
        "set +xv\nexport MYSQL_PWD='secret'\n"
    );
    assert_eq!(credential.names(), ["MYSQL_PWD".to_owned()]);
    let root = fixture.home.join(".effigy/data-credentials");
    assert_eq!(
        fs::symlink_metadata(&root)
            .expect("root metadata")
            .permissions()
            .mode()
            & 0o777,
        0o700,
        "0700 required"
    );

    credential.remove().expect("remove");
    assert!(!credential.path().exists(), "the file is removed");
    assert!(credential_entries(&fixture.home).is_empty());
}

#[test]
fn a_failed_credential_write_removes_its_partial_file() {
    let fixture = fixture_home("effigy-data-credential-partial-");
    let error = match DataCredentialFile::write_with(&fixture.home, &env(), |file, body| {
        use std::io::Write;
        file.write_all(&body[..8])?;
        Err(std::io::Error::other("injected partial write"))
    }) {
        Ok(_) => panic!("a partial write must fail the credential handoff"),
        Err(error) => error,
    };
    assert!(
        error
            .to_string()
            .contains("cannot write the private database credential file"),
        "{error}"
    );
    assert!(
        !error.to_string().contains("secret"),
        "no value in the error: {error}"
    );
    assert!(
        credential_entries(&fixture.home).is_empty(),
        "a partial credential file must not survive its failed write"
    );
}

#[test]
fn concurrent_credential_files_are_distinct_and_removed() {
    let fixture = fixture_home("effigy-data-credential-race-");
    let env = env();
    let mut handles = Vec::new();
    for _ in 0..8 {
        let home = fixture.home.clone();
        let env = env.clone();
        handles.push(std::thread::spawn(move || {
            let credential = DataCredentialFile::write(&home, &env).expect("write");
            let path = credential.path().to_path_buf();
            credential.remove().expect("remove");
            path
        }));
    }
    let paths = handles
        .into_iter()
        .map(|handle| handle.join().expect("thread"))
        .collect::<Vec<_>>();
    let unique = paths.iter().collect::<std::collections::BTreeSet<_>>();
    assert_eq!(unique.len(), paths.len(), "each exec owns its own path");
    assert!(credential_entries(&fixture.home).is_empty());
}

#[test]
fn an_invalid_credential_name_is_refused_without_its_value() {
    let fixture = fixture_home("effigy-data-credential-name-");
    let env = BTreeMap::from([("MYSQL PWD=1".to_owned(), "synthetic-487-name".to_owned())]);
    let error = DataCredentialFile::write(&fixture.home, &env)
        .expect_err("an unusable environment name must be refused");
    assert!(error
        .to_string()
        .contains("not a portable environment name"));
    assert!(!error.to_string().contains("synthetic-487-name"), "{error}");
    assert!(
        !fixture.home.join(".effigy/data-credentials").exists(),
        "a refused name creates no credential directory"
    );
}

#[test]
fn the_bootstrap_reads_the_file_and_carries_no_value() {
    let bootstrap = wrap_credential_bootstrap("exec sh -c 'true'");
    assert!(bootstrap.starts_with(". \"$1\""), "{bootstrap}");
    assert!(bootstrap.ends_with("exec sh -c 'true'"), "{bootstrap}");
    assert!(!bootstrap.contains("MYSQL_PWD"), "{bootstrap}");
    assert!(!bootstrap.contains("secret"), "{bootstrap}");
}
