use super::*;

pub(in crate::runner) fn run_demo(args: DemoArgs) -> Result<String, RunnerError> {
    let resolved = resolve_active_repo_root(args.repo_override.clone())?;
    let repo_root = resolved.resolved_root;
    let manifest_path = repo_root.join(TASK_MANIFEST_FILE);
    let loaded = load_task_manifest_with_inspection(&manifest_path)?;

    match args.subcommand {
        DemoSubcommand::Browser { group_by } => {
            if args.output_json {
                return demo_error(
                    true,
                    "effigy.demo.browser.v1",
                    "demo browser does not support json mode".to_owned(),
                    json!({ "repo_root": repo_root.display().to_string() }),
                );
            }
            run_demo_browser_tui(repo_root, group_by)?;
            Ok(String::new())
        }
        DemoSubcommand::List { query } => {
            render_demo_list(&repo_root, &loaded, &query, args.output_json)
        }
        DemoSubcommand::Inspect { demo_id } => {
            render_demo_inspect(&repo_root, &loaded, &demo_id, args.output_json)
        }
        DemoSubcommand::History {
            demo_id,
            limit,
            outcome,
            attempt_id,
            attempt_ordinal,
        } => render_demo_history(
            &repo_root,
            &loaded,
            &demo_id,
            &super::query::demo_history_request(
                limit,
                outcome,
                attempt_id.as_deref(),
                attempt_ordinal,
            ),
            args.output_json,
        ),
        DemoSubcommand::Run { demo_id } => render_demo_execute(
            &repo_root,
            &loaded,
            &demo_id,
            args.output_json,
            DemoInvocationKind::Run,
        ),
        DemoSubcommand::Rerun { demo_id } => render_demo_execute(
            &repo_root,
            &loaded,
            &demo_id,
            args.output_json,
            DemoInvocationKind::Rerun,
        ),
        DemoSubcommand::Stop { demo_id } => {
            render_demo_stop(&repo_root, &loaded, &demo_id, args.output_json)
        }
        DemoSubcommand::Input {
            demo_id,
            text,
            append_newline,
        } => render_demo_input(
            &repo_root,
            &loaded,
            &demo_id,
            &text,
            append_newline,
            args.output_json,
        ),
        DemoSubcommand::Resize {
            demo_id,
            cols,
            rows,
        } => render_demo_resize(&repo_root, &loaded, &demo_id, cols, rows, args.output_json),
    }
}

pub(in crate::runner) fn demo_error(
    output_json: bool,
    schema: &str,
    message: String,
    extra: JsonValue,
) -> Result<String, RunnerError> {
    if output_json {
        let payload = build_demo_error_payload(schema, &message, extra);
        let rendered = encode_json(&payload, true)?;
        return Err(RunnerError::CommandJsonFailure { rendered });
    }
    Err(RunnerError::task_invocation(message))
}
