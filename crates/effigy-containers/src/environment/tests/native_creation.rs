//! Opt-in native disposable window. The selected runtime must already be up;
//! this proof never starts or stops a shared VM and never pulls an image.
use super::*;
use crate::environment::creation::{CreationPreflight, CreationRequest};
use crate::runtime::scope::{
    CHECKOUT_LABEL, COMPOSE_PROJECT_LABEL, MANAGED_LABEL, PROJECT_LABEL, SCOPE_LABEL,
};
use std::cell::Cell;
use std::ffi::OsString;
use std::time::{Duration, Instant};

#[derive(Debug)]
struct NativeWindow {
    selection: BackendSelection,
    workdir: PathBuf,
    project: String,
    scope: String,
    compose_file: PathBuf,
    applied: Cell<bool>,
    remote_directory: Option<PathBuf>,
    remote_removed: Cell<bool>,
}
impl NativeWindow {
    fn invoke(&self, args: &[OsString], phase: &str) -> Result<(), String> {
        let (program, args) = self
            .selection
            .compose_invocation(args)
            .map_err(|error| error.to_string())?;
        crate::exec::run_backend_invocation_until(
            &self.workdir,
            &program,
            &args,
            phase,
            &[],
            Instant::now() + Duration::from_secs(20),
        )
        .map(|_| ())
        .map_err(|error| error.to_string())
    }
    fn compose_args(&self, action: &[&str]) -> Vec<OsString> {
        let mut args = vec![
            "compose".into(),
            "--project-name".into(),
            self.project.clone().into(),
            "-f".into(),
            self.compose_file.as_os_str().to_owned(),
        ];
        args.extend(action.iter().map(OsString::from));
        args
    }
    fn owned(&self, row: &Workload) -> bool {
        row.labels.get(COMPOSE_PROJECT_LABEL) == Some(&self.project)
            && row.labels.get(PROJECT_LABEL) == Some(&self.project)
            && row.labels.get(SCOPE_LABEL) == Some(&self.scope)
            && row.labels.get(CHECKOUT_LABEL).map(String::as_str) == self.workdir.to_str()
            && row.labels.get(MANAGED_LABEL).map(String::as_str) == Some("true")
            && crate::environment::backend::full_container_id(&row.id)
    }
    fn inventory(&self) -> Result<Vec<Workload>, String> {
        match self.selection.adapter().inventory_until(
            &self.workdir,
            &self.project,
            Instant::now() + Duration::from_secs(10),
        ) {
            Observed::Verified(rows, _) => Ok(rows),
            Observed::Absent(proof) => Err(proof.to_string()),
            Observed::Unknown(reason) => Err(reason.to_string()),
        }
    }
    fn ssh(&self, args: &[OsString], input: &[u8]) -> Result<std::process::Output, String> {
        let mut argv = vec![
            "ssh".into(),
            "--profile".into(),
            self.selection.context.clone().into(),
            "--".into(),
        ];
        argv.extend_from_slice(args);
        crate::exec::run_input_capture_until(
            &self.workdir,
            "colima",
            &argv,
            input,
            Instant::now() + Duration::from_secs(20),
        )
        .map_err(|error| error.to_string())
        .and_then(|output| {
            if output.status.success() {
                Ok(output)
            } else {
                Err(String::from_utf8_lossy(&output.stderr).into_owned())
            }
        })
    }
    fn cleanup(&self) -> Result<(), String> {
        if self.applied.get() {
            let rows = self.inventory()?;
            if rows.iter().any(|row| !self.owned(row)) {
                return Err("native project contains an unowned row; refusing cleanup".into());
            }
            for row in rows {
                self.invoke(
                    &["rm".into(), "--force".into(), row.id.into()],
                    "remove exact-owned native container",
                )?;
            }
            self.invoke(
                &self.compose_args(&["down"]),
                "remove native fixture project",
            )?;
            if !self.inventory()?.is_empty() {
                return Err("native fixture still has containers".into());
            }
            self.applied.set(false);
        }
        if let Some(directory) = &self.remote_directory {
            if !self.remote_removed.get() {
                self.ssh(
                    &[
                        "rm".into(),
                        "-rf".into(),
                        "--".into(),
                        directory.as_os_str().to_owned(),
                    ],
                    &[],
                )?;
                self.remote_removed.set(true);
            }
        }
        Ok(())
    }
}
impl Drop for NativeWindow {
    fn drop(&mut self) {
        if let Err(error) = self.cleanup() {
            eprintln!("native cleanup failed for {}: {error}", self.project);
        }
    }
}

#[test]
#[ignore = "requires an explicitly selected running Colima profile and a cached alpine image"]
fn native_colima_complete_window_qualifies_or_refuses_without_weak_image_identity() {
    let profile = std::env::var("EFFIGY_510_NATIVE_PROFILE").expect("explicit native profile");
    let image = std::env::var("EFFIGY_510_NATIVE_IMAGE").expect("explicit cached image reference");
    let fixture = home();
    std::fs::create_dir(fixture.path().join(".git")).unwrap();
    std::fs::write(
        fixture.path().join(".git/config"),
        "[effigy]\n runtimeScope = ephemeral\n",
    )
    .unwrap();
    let token = effigy_core::worktree_scope::prepare_checkout_token(fixture.path()).unwrap();
    let mut bytes = [0; 16];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&token[index * 2..index * 2 + 2], 16).unwrap();
    }
    let checkout = CheckoutId::new(CheckoutKind::EphemeralClone, bytes);
    let stack = EnvRef::new(checkout, "native").unwrap();
    let project = format!("effigy-510-{}", token);
    let selection = BackendSelection {
        kind: BackendKind::Colima,
        engine: crate::BackendId::colima_nerdctl(),
        context: profile.clone(),
        reason: BackendReason::EnvironmentVariable("native proof".into()),
    };
    let labels = serde_json::json!({ "com.effigy.managed":"true", "com.effigy.project":project, "com.effigy.scope":token, "com.effigy.checkout":fixture.path() });
    let compose = serde_json::to_vec(&serde_json::json!({"services":{"workspace":{"image":image,"network_mode":"none","entrypoint":["/bin/sh","-c","sleep 120"],"labels":labels}}})).unwrap();
    let metadata = serde_json::to_vec(&serde_json::json!({"repo_root":fixture.path(),"project_name":project,"primary_service":"workspace","compose_files":["compose.yml"],"backend_id":selection.engine.as_str(),"backend_context":profile})).unwrap();
    let render = Closure::new(
        [
            ("compose.yml".into(), compose),
            ("inputs/environment.json".into(), metadata),
        ]
        .into_iter()
        .collect(),
    )
    .unwrap();
    let state = root(fixture.path());
    let ctx = EnvContext::new(&state, CancelToken::default()).unwrap();
    let mut window = NativeWindow {
        selection: selection.clone(),
        workdir: fixture.path().into(),
        project: project.clone(),
        scope: token,
        compose_file: fixture.path().join("not-published"),
        applied: Cell::new(false),
        remote_directory: None,
        remote_removed: Cell::new(false),
    };
    eprintln!(
        "native-creation-start {}",
        serde_json::json!({"profile":profile,"project":project,"fixture":fixture.path(),"image":image})
    );
    let result = (|| -> Result<serde_json::Value, String> {
        if !window.inventory()?.is_empty() {
            return Err("native project was not empty before the proof".into());
        }
        let deadline = Instant::now() + Duration::from_secs(30);
        let request = CreationRequest {
            workdir: fixture.path(),
            project: &project,
            primary_service: "workspace",
            render: Some(&render),
            compose_files: &[],
            deadline,
            preflight: None,
            required: true,
        };
        let mut adapter = selection.adapter();
        let preflight = CreationPreflight::observe(adapter.as_mut(), request)
            .map_err(|error| error.to_string())?;
        let seal =
            effigy_kernel::objects::Objects::seal_for_preflight(&ctx.cx, &state, &render, deadline)
                .map_err(|error| error.to_string())?;
        let mut run = EnvRun::begin(&ctx, &stack, Operation::Up, Wait::FailFast)
            .map_err(|error| error.to_string())?;
        run.publish_sealed_desired(&seal, &render)
            .map_err(|error| error.to_string())?;
        let object = match effigy_kernel::objects::Objects::resolve(
            &ctx.observe(),
            &state,
            &stack.resource,
            effigy_kernel::objects::Role::Desired,
        ) {
            Observed::Verified(object, _) => object,
            Observed::Absent(proof) => return Err(proof.to_string()),
            Observed::Unknown(reason) => return Err(reason.to_string()),
        };
        let files = [object.path.join("compose.yml")];
        run.verify_compose_inputs(&render, &files, fixture.path())
            .map_err(|error| error.to_string())?;
        let remote = window.ssh(&["mktemp".into(), "-d".into()], &[])?;
        let remote = String::from_utf8(remote.stdout).map_err(|error| error.to_string())?;
        let directory = PathBuf::from(remote.trim());
        if directory.parent() != Some(Path::new("/tmp"))
            || !directory
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    name.bytes().all(|byte| {
                        byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-')
                    })
                })
        {
            return Err("VM mktemp returned an unexpected directory".into());
        }
        window.remote_directory = Some(directory.clone());
        let bytes = render
            .files()
            .get("compose.yml")
            .ok_or("sealed compose file missing")?;
        window.ssh(
            &[
                "sh".into(),
                "-c".into(),
                "umask 077; cat > \"$1/compose.yml\"".into(),
                "sh".into(),
                directory.as_os_str().to_owned(),
            ],
            bytes,
        )?;
        window.compose_file = directory.join("compose.yml");
        if window
            .ssh(
                &["cat".into(), window.compose_file.as_os_str().to_owned()],
                &[],
            )?
            .stdout
            != *bytes
        {
            return Err("VM copy differs from the sealed Compose bytes".into());
        }
        eprintln!(
            "native-creation-stage {}",
            serde_json::json!({"project":project,"vm_directory":directory,"transport":"verified private VM copy of sealed bytes"})
        );
        let request = CreationRequest {
            compose_files: &files,
            preflight: Some(&preflight),
            ..request
        };
        let apply_ok = Cell::new(false);
        let outcome = run.create_with(adapter.as_mut(), &selection, request, || {
            window.applied.set(true);
            let result = window.invoke(
                &window.compose_args(&["up", "-d"]),
                "native sealed Compose apply",
            );
            apply_ok.set(result.is_ok());
            result.map_err(BackendError::Uncertain)
        });
        if !apply_ok.get() {
            return Err(format!(
                "native Compose apply did not complete successfully: {}",
                outcome
                    .as_ref()
                    .err()
                    .map(ToString::to_string)
                    .unwrap_or_default()
            ));
        }
        let rows = window.inventory()?;
        let [row] = rows.as_slice() else {
            return Err(format!(
                "native project answered {} rows, expected one",
                rows.len()
            ));
        };
        if !window.owned(row) {
            return Err("native apply answered an unowned container".into());
        }
        let immutable = crate::environment::backend::exact_image_id(&row.image).is_some();
        let refusal = outcome.as_ref().err().map(ToString::to_string);
        if immutable {
            let receipt = outcome
                .as_ref()
                .map_err(|error| error.to_string())?
                .receipt()
                .ok_or("native immutable container received no receipt")?;
            if receipt.created_ids().collect::<Vec<_>>() != [row.id.as_str()] {
                return Err("native receipt differs from the exact observed ID".into());
            }
        } else if outcome.is_ok() {
            return Err("native reference-only image received a successful receipt".into());
        }
        run.finish(outcome.is_ok())
            .map_err(|error| error.to_string())?;
        Ok(
            serde_json::json!({"profile":profile,"project":project,"container_id":row.id,"container_image":row.image,"image_reference":row.image_reference,"immutable_container_image":immutable,"qualified_receipt":outcome.is_ok(),"refusal":refusal,"apply_exit":0}),
        )
    })();
    let cleanup = window.cleanup();
    assert!(
        cleanup.is_ok(),
        "exact-owned native cleanup failed: {cleanup:?}"
    );
    let mut evidence = result.expect("native window must complete or name a real adapter blocker");
    evidence["cleanup_exit"] = 0.into();
    eprintln!("native-creation-result {evidence}");
}
