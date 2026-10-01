use super::*;
use crate::surface::MODULE_NAMES;
use crate::{rhai_surface_functions, rhai_surface_json};

fn retirement_context(name: &str) -> (PathBuf, ScriptContext) {
    let root = temp_root(name);
    let context = ScriptContext {
        cwd: root.clone(),
        repo_root: root.clone(),
        task_name: "storage-retirement".to_owned(),
        stop_requested: install_stop_requested_flag().expect("stop flag"),
    };
    (root, context)
}

#[test]
fn storage_module_is_absent_from_the_registered_runtime() {
    let (_root, context) = retirement_context("storage-absent-runtime");

    for call in [
        "storage::provider()",
        "storage::provider(#{ provider: \"s3\" })",
        "storage::status()",
        "storage::ls(#{ bucket: \"assets\" })",
        "storage::head(#{ bucket: \"assets\", key: \"a.txt\" })",
        "storage::get(#{ bucket: \"assets\", key: \"a.txt\" })",
        "storage::put(#{ bucket: \"assets\", key: \"a.txt\", body: \"x\" })",
        "storage::delete(#{ bucket: \"assets\", key: \"a.txt\" })",
    ] {
        let error = execute_rhai_script(&context, call, &[], &callbacks())
            .expect_err("removed storage call must fail honestly");
        let message = error.to_string().to_lowercase();
        assert!(
            message.contains("storage"),
            "error should name the removed module: {error}"
        );
        assert!(
            message.contains("not found"),
            "error should be an unknown module/function failure, not something else: {error}"
        );
    }
}

#[test]
fn removed_storage_calls_fail_without_network_or_external_effects() {
    let (_root, context) = retirement_context("storage-absent-no-network");
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback listener");
    let address = listener.local_addr().expect("local addr");
    listener
        .set_nonblocking(true)
        .expect("nonblocking listener");

    let script = format!(
        r#"
        storage::status(#{{ bucket: "assets", endpoint: "http://{address}", path_style: true }});
        storage::get(#{{ bucket: "assets", key: "a.txt", endpoint: "http://{address}" }});
        storage::put(#{{ bucket: "assets", key: "a.txt", body: "x", endpoint: "http://{address}" }});
        storage::delete(#{{ bucket: "assets", key: "a.txt", endpoint: "http://{address}" }});
        "#
    );

    let error = execute_rhai_script(&context, &script, &[], &callbacks())
        .expect_err("removed storage calls must fail before any request");
    let message = error.to_string().to_lowercase();
    assert!(
        message.contains("storage") && message.contains("not found"),
        "expected an unknown-module failure pointing at the script, got: {error}"
    );

    // Nothing dialled the loopback listener: the failure is local and total.
    match listener.accept() {
        Err(error) => {
            assert!(
                error.kind() == std::io::ErrorKind::WouldBlock,
                "unexpected inbound connection during storage retirement proof: {error}"
            );
        }
        Ok((_, peer)) => {
            panic!("removed storage call reached {peer}; removal must not keep a stub");
        }
    }
}

#[test]
fn storage_is_absent_from_the_listed_host_surface() {
    assert!(!MODULE_NAMES.contains(&"storage"));

    let functions = rhai_surface_functions();
    assert!(
        functions
            .iter()
            .all(|function| function.module != "storage"),
        "surface inventory still lists storage functions"
    );

    let surface = rhai_surface_json();
    let modules = surface["modules"].as_array().expect("modules array");
    assert!(
        !modules.iter().any(|module| module == "storage"),
        "surface JSON still lists the storage module"
    );
    let listed = surface["functions"].as_array().expect("functions array");
    assert!(
        listed
            .iter()
            .all(|function| function["module"] != "storage"),
        "surface JSON still lists storage functions"
    );
}

#[test]
fn representative_remaining_rhai_modules_still_work() {
    let (root, context) = retirement_context("storage-retirement-others-work");
    fs::write(root.join("note.txt"), "retired").expect("seed file");

    let script = r#"
        fs::write_file("out.txt", "body");
        if fs::read_file("note.txt") != "retired" { throw("fs"); }
        if str::starts_with("storage-retired", "storage") != true { throw("str"); }
        if semver::valid("1.2.3") != true { throw("semver"); }
        let parsed = json::parse("{\"ok\":true}");
        if parsed["ok"] != true { throw("json"); }
        if path::join("a", "b") != "a/b" { throw("path"); }
    "#;

    execute_rhai_script(&context, script, &[], &callbacks()).expect("execute");
}
