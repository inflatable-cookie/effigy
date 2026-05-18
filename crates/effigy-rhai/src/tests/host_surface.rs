use super::*;

#[test]
fn execute_rhai_script_exposes_task_effigy_and_container_helpers() {
    let root = temp_root("execute");
    let context = ScriptContext {
        cwd: root.clone(),
        repo_root: root,
        task_name: "demo".to_owned(),
        stop_requested: install_stop_requested_flag().expect("stop flag"),
    };
    let script = r#"
            let task = task::run("demo:task", ["a", "b"]);
            if task != "task:demo:task:a,b" { throw("task"); }
            let effigy = effigy::run(["demo", "list"]);
            if !effigy["success"] || effigy["output"] != "demo list" { throw("effigy"); }
            let active_version = effigy::active_version();
            if !str::contains(active_version, "__EFFIGY_VERSION__") { throw("active version"); }
            let json = effigy::run_json(["demo", "list"]);
            if json["json"] != true { throw("json"); }
            if container::up("web", true) != "up:web:true" { throw("up"); }
            if container::down("web") != "down:web:false" { throw("down"); }
            if container::shell("web", "echo hi") != "shell:web::echo hi" { throw("shell"); }
            let exec = container::exec("web", "postgres", ["psql", "-c", "select 1"]);
            if !exec["success"] || exec["stdout"] != "exec:web:postgres:psql,-c,select 1" { throw("exec"); }
            let default_service = container::exec("web", ["pwd"]);
            if default_service["stdout"] != "exec:web::pwd" { throw("exec default"); }
            let tasks = task::list();
            if tasks["feature"] != "tasks.list" { throw("tasks list"); }
            let resolved = task::resolve("api/test");
            if resolved["options"]["selector"] != "api/test" { throw("task resolve"); }
            let status = container::status("stack");
            if status["feature"] != "container.status" { throw("container status"); }
            let status_all = container::status(#{ "all": true });
            if status_all["feature"] != "container.status" || status_all["options"]["all"] != true { throw("container status all"); }
            let logs = container::logs("stack", #{ service: "postgres" });
            if logs["options"]["service"] != "postgres" { throw("container logs"); }
            let data = container::data("list", "stack");
            if data["feature"] != "container.data" || data["options"]["operation"] != "list" { throw("container data"); }
            let stats = container::stats();
            if stats["feature"] != "container.stats" { throw("container stats"); }
            let docs = docs::check_links(#{ paths: ["docs/README.md"] });
            if docs["feature"] != "docs.check_links" { throw("docs"); }
            let bundle = bundle::inspect();
            if bundle["feature"] != "bundle.inspect" { throw("bundle"); }
            let deploy = deploy::emit(#{ provider: "render", path: "tmp/render", plan: true });
            if deploy["feature"] != "deploy.emit" || deploy["options"]["provider"] != "render" { throw("deploy emit"); }
            let deploy_plan = deploy::plan(#{ env: "uat", write_report: true });
            if deploy_plan["feature"] != "deploy.plan" || deploy_plan["options"]["write_report"] != true { throw("deploy plan"); }
            let deploy_apply = deploy::apply(#{ env: "uat", yes: true });
            if deploy_apply["feature"] != "deploy.apply" || deploy_apply["options"]["yes"] != true { throw("deploy apply"); }
            let deploy_status = deploy::status(#{ env: "uat" });
            if deploy_status["feature"] != "deploy.status" || deploy_status["options"]["env"] != "uat" { throw("deploy status"); }
            let deploy_history = deploy::history(#{ env: "production", limit: 5 });
            if deploy_history["feature"] != "deploy.history" || deploy_history["options"]["limit"] != 5 { throw("deploy history"); }
            let deploy_redeploy = deploy::redeploy(#{ env: "production", deployment: "dep-123", yes: true });
            if deploy_redeploy["feature"] != "deploy.redeploy" || deploy_redeploy["options"]["deployment"] != "dep-123" { throw("deploy redeploy"); }
            let distribution_preflight = distribution::preflight(#{ tag: "v0.7.1", skip_docs: true, output: "tmp/preflight.env" });
            if distribution_preflight["feature"] != "distribution.preflight" || distribution_preflight["options"]["skip_docs"] != true { throw("distribution preflight"); }
            let distribution_glibc = distribution::check_glibc_floor(#{ binary: "./target/release/effigy", max_glibc: "2.35" });
            if distribution_glibc["feature"] != "distribution.check_glibc_floor" || distribution_glibc["options"]["max_glibc"] != "2.35" { throw("distribution glibc"); }
            let gateway = gateway::status();
            if gateway["feature"] != "gateway.status" { throw("gateway"); }
            let scan = scan::god_files(#{ threshold: 900 });
            if scan["feature"] != "scan.god_files" { throw("scan"); }
            let cache = cache::inspect(#{ selector: "build" });
            if cache["options"]["selector"] != "build" { throw("cache"); }
            let unlock = unlock::scopes(#{ "all": true });
            if unlock["feature"] != "unlock.scopes" || unlock["options"]["all"] != true { throw("unlock scopes"); }
            let config = config::effective();
            if config["feature"] != "config.effective" { throw("config effective"); }
            let raw = config::raw();
            if raw["feature"] != "config.raw" { throw("config raw"); }
            let value = config::get("systems.dev.container");
            if value != "stack" { throw("config get"); }
        "#;
    let script = script.replace("__EFFIGY_VERSION__", env!("CARGO_PKG_VERSION"));

    execute_rhai_script(&context, &script, &[], &callbacks()).expect("execute");
}

#[test]
fn execute_rhai_script_exposes_extended_rhai_feature_surface() {
    let root = temp_root("extended-surface");
    let context = ScriptContext {
        cwd: root.clone(),
        repo_root: root,
        task_name: "demo".to_owned(),
        stop_requested: install_stop_requested_flag().expect("stop flag"),
    };
    let script = r#"
            let user_path = config::user_path();
            if user_path["feature"] != "config.user_path" { throw("config user path"); }

            let user_get = config::user_get("containers.backend");
            if user_get["feature"] != "config.user_get" || user_get["options"]["key"] != "containers.backend" { throw("config user get"); }

            let user_set = config::user_set("containers.backend", "containerd");
            if user_set["feature"] != "config.user_set" || user_set["options"]["value"] != "containerd" { throw("config user set"); }

            let user_unset = config::user_unset("containers.profile");
            if user_unset["feature"] != "config.user_unset" || user_unset["options"]["key"] != "containers.profile" { throw("config user unset"); }

            let state_plan = state::plan("uat");
            if state_plan["feature"] != "state.plan" || state_plan["options"]["stack"] != "uat" { throw("state plan"); }

            let state_apply = state::apply(#{ stack: "uat", yes: true });
            if state_apply["feature"] != "state.apply" || state_apply["options"]["stack"] != "uat" { throw("state apply"); }

            let state_capture = state::capture("uat", "baseline");
            if state_capture["feature"] != "state.capture" || state_capture["options"]["profile"] != "baseline" { throw("state capture"); }

            let state_capture_set = state::capture_set(#{ stack: "uat", profiles: ["baseline", "media"], key: "uat-snapshot", yes: true, push: true });
            if state_capture_set["feature"] != "state.capture_set" { throw("state capture set"); }
            if state_capture_set["options"]["profiles"][1] != "media" || state_capture_set["options"]["key"] != "uat-snapshot" || state_capture_set["options"]["push"] != true { throw("state capture set options"); }

            let state_history = state::history(#{ stack: "uat", limit: 5 });
            if state_history["feature"] != "state.history" || state_history["options"]["limit"] != 5 { throw("state history"); }

            let artifact_inspect = artifact::inspect("oci://ghcr.io/acme/app:seed");
            if artifact_inspect["feature"] != "artifact.inspect" { throw("artifact inspect"); }

            let artifact_stage = artifact::stage("oci://ghcr.io/acme/app:seed", #{ farmyard_handoff: true });
            if artifact_stage["feature"] != "artifact.stage" || artifact_stage["options"]["farmyard_handoff"] != true { throw("artifact stage"); }

            let artifact_capture = artifact::capture(
                "tmp/seed.sql",
                "oci://ghcr.io/acme/app:seed",
                #{ kind: "database", environment_label: "uat", push: true },
            );
            if artifact_capture["feature"] != "artifact.capture" || artifact_capture["options"]["push"] != true { throw("artifact capture"); }

            let cache_list = container::cache_list(#{ global: true, project: "cbs-dev" });
            if cache_list["feature"] != "container.cache_list" || cache_list["options"]["project"] != "cbs-dev" { throw("cache list"); }

            let cache_prune = container::cache_prune(#{ global: true, yes: true, kind: "rust-target" });
            if cache_prune["feature"] != "container.cache_prune" || cache_prune["options"]["kind"] != "rust-target" { throw("cache prune"); }

            let volume_list = container::volume_list(#{ global: true, orphans: true });
            if volume_list["feature"] != "container.volume_list" || volume_list["options"]["orphans"] != true { throw("volume list"); }

            let volume_prune = container::volume_prune(#{ dormant: true, yes: true });
            if volume_prune["feature"] != "container.volume_prune" || volume_prune["options"]["dormant"] != true { throw("volume prune"); }

            let dump = container::data_dump(#{
                name: "web",
                db_dumps: ["main=tmp/main.sql", "tmp/other.sql"],
                push: true,
            });
            if dump["feature"] != "container.data_dump" || dump["options"]["push"] != true { throw("data dump"); }

            let seed = container::data_seed(#{
                db_seeds: ["main=tmp/main.sql"],
                yes: true,
            });
            if seed["feature"] != "container.data_seed" || seed["options"]["db_seeds"][0] != "main=tmp/main.sql" { throw("data seed"); }

            let pull = container::data_pull_production("web");
            if pull["feature"] != "container.data_pull_production" || pull["options"]["name"] != "web" { throw("data pull production"); }

            let validate_metadata = distribution::validate_metadata(#{ tag: "v0.7.1" });
            if validate_metadata["feature"] != "distribution.validate_metadata" || validate_metadata["options"]["tag"] != "v0.7.1" { throw("distribution validate metadata"); }

            let first_publish = distribution::first_publish(#{ tag: "v0.7.1", artifacts_dir: "artifacts/release", skip_homebrew: true });
            if first_publish["feature"] != "distribution.first_publish" || first_publish["options"]["skip_homebrew"] != true { throw("distribution first publish"); }

            let validate_artifacts = distribution::validate_artifacts(#{ artifacts_dir: "artifacts/release", expect_homebrew: true });
            if validate_artifacts["feature"] != "distribution.validate_artifacts" || validate_artifacts["options"]["expect_homebrew"] != true { throw("distribution validate artifacts"); }

            let generate_closeout = distribution::generate_closeout(#{
                tag: "v0.7.1",
                artifacts_dir: "artifacts/release",
                output: "tmp/closeout.md",
                owner: "Platform",
                expect_homebrew: true,
            });
            if generate_closeout["feature"] != "distribution.generate_closeout" || generate_closeout["options"]["owner"] != "Platform" { throw("distribution generate closeout"); }

            let write_summary = distribution::write_summary(#{
                tag: "v0.7.1",
                artifacts_dir: "artifacts/release",
                homebrew_executed: true,
                log_files: ["01.log", "02.log"],
            });
            if write_summary["feature"] != "distribution.write_summary" || write_summary["options"]["log_files"][1] != "02.log" { throw("distribution write summary"); }
        "#;

    execute_rhai_script(&context, script, &[], &callbacks()).expect("execute");
}
