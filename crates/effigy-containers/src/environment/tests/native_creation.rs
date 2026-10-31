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

fn native_render(window: &NativeWindow, image: &str, revision: usize) -> Closure {
    let labels = serde_json::json!({
        "com.effigy.managed":"true", "com.effigy.project":window.project,
        "com.effigy.scope":window.scope, "com.effigy.checkout":window.workdir,
    });
    let compose = serde_json::to_vec(&serde_json::json!({"services":{"workspace":{
        "image":image,"network_mode":"none","entrypoint":["/bin/sh","-c","sleep 120"],
        "environment":{"EFFIGY_510_CONFIG_REVISION":revision.to_string()},"labels":labels,
    }}}))
    .unwrap();
    let metadata = serde_json::to_vec(&serde_json::json!({
        "repo_root":window.workdir,"project_name":window.project,"primary_service":"workspace",
        "compose_files":["compose.yml"],"backend_id":window.selection.engine.as_str(),
        "backend_context":window.selection.context,
    }))
    .unwrap();
    Closure::new(
        [
            ("compose.yml".into(), compose),
            ("inputs/environment.json".into(), metadata),
        ]
        .into(),
    )
    .unwrap()
}

fn reserve_successor(
    ctx: &EnvContext<'_>,
    stack: &EnvRef,
    window: &NativeWindow,
    render: &Closure,
    image: &str,
) -> Result<effigy_kernel::workspace_session::WorkspaceActivationReservation, String> {
    use effigy_kernel::workspace_session::*;
    let inputs = PublishInputs {
        artifact: ArtifactEvidence::Unselected {
            image_reference: image.into(),
            backend: window.selection.engine.as_str().into(),
        },
        selection: effigy_kernel::id::Digest::of(b"native-creator-attribution"),
        inputs_digest: effigy_kernel::id::Digest::of(render.digest().as_bytes()),
        target: TargetEvidence::Absent {
            project: window.project.clone(),
            backend: window.selection.engine.as_str().into(),
        },
        platform: None,
        // These frozen inputs admit target binding only. No guest objects,
        // credentials, locator, session commit or serving proof is synthesized.
        guest: Some(GuestContextInputs {
            version: env!("CARGO_PKG_VERSION").into(),
            protocol: 1,
            host_user: "0".into(),
            checkout_id: stack.checkout.text(),
            environment_resource: stack.resource.name().to_string(),
            profile: window.selection.context.clone(),
            runtime: Some("containerd".into()),
            vm_type: None,
            routes: vec![],
            sources: vec![],
            paths: GuestPaths {
                host_checkout: window.workdir.to_string_lossy().into(),
                guest_checkout: "/".into(),
                workdir: "/".into(),
                user: "0".into(),
                home: "/".into(),
            },
        }),
    };
    match WorkspaceActivationReservation::decide(
        &ctx.cx,
        ctx.root,
        &stack.resource,
        &inputs,
        Wait::FailFast,
    )
    .map_err(|error| error.to_string())?
    {
        ActivationDecision::Activate(reservation) => Ok(*reservation),
        _ => Err("native successor unexpectedly attached to a live session".into()),
    }
}

fn native_phase(
    state: &StateRoot,
    stack: &EnvRef,
    window: &mut NativeWindow,
    render: &Closure,
    image: &str,
    predecessor: Option<&str>,
) -> Result<serde_json::Value, String> {
    use effigy_kernel::workspace_session::{ContainerIdentity, TargetCandidate};
    let ctx = EnvContext::new(state, CancelToken::default()).map_err(|error| error.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(45);
    let request = CreationRequest {
        workdir: &window.workdir,
        project: &window.project,
        primary_service: "workspace",
        render: Some(render),
        compose_files: &[],
        deadline,
        preflight: None,
        required: true,
    };
    let mut adapter = window.selection.adapter();
    let preflight =
        CreationPreflight::observe(adapter.as_mut(), request).map_err(|error| error.to_string())?;
    let seal =
        effigy_kernel::objects::Objects::seal_for_preflight(&ctx.cx, state, render, deadline)
            .map_err(|error| error.to_string())?;
    let reservation = reserve_successor(&ctx, stack, window, render, image)?;
    let mut run = EnvRun::begin_claimed(&ctx, stack, Operation::Up, Wait::FailFast, &reservation)
        .map_err(|error| error.to_string())?;
    let before = window.inventory()?;
    match (predecessor, before.as_slice()) {
        (None, []) => {}
        (Some(id), [row]) if row.id == id && window.owned(row) => {}
        _ => {
            return Err(
                "native pre-inventory differs from the expected non-live predecessor".into(),
            )
        }
    }
    run.publish_sealed_desired(&seal, render)
        .map_err(|error| error.to_string())?;
    let object = match effigy_kernel::objects::Objects::resolve(
        &ctx.observe(),
        state,
        &stack.resource,
        effigy_kernel::objects::Role::Desired,
    ) {
        Observed::Verified(object, _) => object,
        other => return Err(format!("native sealed object unavailable: {other:?}")),
    };
    let files = [object.path.join("compose.yml")];
    run.verify_compose_inputs(render, &files, &window.workdir)
        .map_err(|error| error.to_string())?;
    if window.remote_directory.is_none() {
        let output = window.ssh(&["mktemp".into(), "-d".into()], &[])?;
        let directory = PathBuf::from(
            String::from_utf8(output.stdout)
                .map_err(|error| error.to_string())?
                .trim(),
        );
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
        window.compose_file = directory.join("compose.yml");
        window.remote_directory = Some(directory);
    }
    let bytes = render
        .files()
        .get("compose.yml")
        .ok_or("sealed Compose bytes missing")?;
    window.ssh(
        &[
            "sh".into(),
            "-c".into(),
            "umask 077; cat > \"$1\"".into(),
            "sh".into(),
            window.compose_file.as_os_str().to_owned(),
        ],
        bytes,
    )?;
    if window
        .ssh(
            &["cat".into(), window.compose_file.as_os_str().to_owned()],
            &[],
        )?
        .stdout
        != *bytes
    {
        return Err("VM copy differs from sealed Compose bytes".into());
    }
    eprintln!(
        "native-creation-stage {}",
        serde_json::json!({"project":window.project,
        "vm_directory":window.remote_directory,"render_digest":render.digest(),
        "predecessor":predecessor,"transport":"verified private VM copy of sealed bytes"})
    );
    // Clone only request identity to leave the exact window's apply state mutable.
    let workdir = window.workdir.clone();
    let project = window.project.clone();
    let request = CreationRequest {
        workdir: &workdir,
        project: &project,
        compose_files: &files,
        preflight: Some(&preflight),
        ..request
    };
    let selection = window.selection.clone();
    let outcome = run
        .create_with(adapter.as_mut(), &selection, request, || {
            window.applied.set(true);
            window
                .invoke(
                    &window.compose_args(&["up", "-d"]),
                    "native sealed Compose apply",
                )
                .map_err(BackendError::Uncertain)
        })
        .map_err(|error| error.to_string())?;
    let receipt = outcome
        .receipt()
        .ok_or("native activation has no attributed receipt")?;
    let rows = window.inventory()?;
    let [row] = rows.as_slice() else {
        return Err("native inventory is not exactly one workspace".into());
    };
    if !window.owned(row)
        || receipt.created_ids().collect::<Vec<_>>() != [row.id.as_str()]
        || receipt.workspace_predecessor() != predecessor
    {
        return Err("native receipt does not attribute exactly the new workspace ID".into());
    }
    let boot = window.ssh(
        &["cat".into(), "/proc/sys/kernel/random/boot_id".into()],
        &[],
    )?;
    let identity = ContainerIdentity {
        container_id: row.id.clone(),
        vm_boot_id: String::from_utf8(boot.stdout)
            .map_err(|error| error.to_string())?
            .trim()
            .into(),
        backend: selection.engine.as_str().into(),
    };
    let evidence = Evidence::new(
        "complete native project inventory under activation gate".into(),
        None,
    )
    .map_err(|error| error.to_string())?;
    let bound = run
        .bind_target_observing(
            &reservation,
            Observed::Verified(
                vec![TargetCandidate {
                    identity: identity.clone(),
                    project: project.clone(),
                    created_by: None,
                }],
                evidence,
            ),
            adapter.as_mut(),
            deadline,
        )
        .map_err(|error| error.to_string())?;
    if bound != identity {
        return Err("native reserved target differs from actual new ID".into());
    }
    let refused = run
        .activate(&[])
        .err()
        .ok_or("reserved generation incorrectly allowed bare Active publication")?;
    if !refused
        .to_string()
        .contains("reserved by a workspace activation")
    {
        return Err(format!("unexpected bare-activate refusal: {refused}"));
    }
    let phase = serde_json::json!({"project":project,"container_id":row.id,
        "predecessor":predecessor,"creating_run":run.id().to_string(),
        "reserved_generation":reservation.generation().get(),"target_bound":true,
        "image_identities":receipt.image_identities(),"render_digest":render.digest(),
        "apply_exit":0,"reserved_bare_activate_refused":true,"environment_active_proved":false,
        "guest_workspace_session_commit_publication_proved":false});
    run.finish(false).map_err(|error| error.to_string())?;
    // The fixture never admits a guest workspace session. Close this completed
    // target-only reservation before testing its non-live successor generation.
    reservation
        .cancel(&ctx.cx, state, Wait::FailFast)
        .map_err(|error| error.to_string())?;
    eprintln!("native-creation-phase {phase}");
    Ok(phase)
}

#[test]
#[ignore = "requires owner authorization, a running Colima profile and a cached alpine image"]
fn native_colima_reserved_fresh_and_config_recreate() {
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
    let stack = EnvRef::new(
        CheckoutId::new(CheckoutKind::EphemeralClone, bytes),
        "native",
    )
    .unwrap();
    let selection = BackendSelection {
        kind: BackendKind::Colima,
        engine: crate::BackendId::colima_nerdctl(),
        context: profile.clone(),
        reason: BackendReason::EnvironmentVariable("native proof".into()),
    };
    let mut window = NativeWindow {
        selection,
        workdir: fixture.path().into(),
        project: format!("effigy-510-{token}"),
        scope: token,
        compose_file: fixture.path().join("not-published"),
        applied: Cell::new(false),
        remote_directory: None,
        remote_removed: Cell::new(false),
    };
    let state = root(fixture.path());
    eprintln!(
        "native-creation-start {}",
        serde_json::json!({"profile":profile,
        "project":window.project,"fixture":fixture.path(),"image":image})
    );
    let result = (|| -> Result<serde_json::Value, String> {
        let first_render = native_render(&window, &image, 1);
        let first = native_phase(&state, &stack, &mut window, &first_render, &image, None)?;
        let predecessor = first["container_id"].as_str().ok_or("fresh ID missing")?;
        let second_render = native_render(&window, &image, 2);
        let second = native_phase(
            &state,
            &stack,
            &mut window,
            &second_render,
            &image,
            Some(predecessor),
        )?;
        if first["container_id"] == second["container_id"] {
            return Err("config edit did not recreate workspace".into());
        }
        Ok(
            serde_json::json!({"profile":profile,"project":window.project,"phases":[first,second],
            "qualified_creator_attribution":true,"guest_workspace_session_commit_publication_proved":false,
            "proof_limit":"does NOT prove guest workspace-session commit/publication"}),
        )
    })();
    let cleanup = window.cleanup();
    assert!(
        cleanup.is_ok(),
        "exact-owned native cleanup failed: {cleanup:?}"
    );
    let mut evidence =
        result.expect("one native fresh/config window must succeed without fabricated evidence");
    evidence["cleanup_exit"] = 0.into();
    eprintln!("native-creation-result {evidence}");
}
