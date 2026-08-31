let context = runtime::context();
if context["command_root"] != repo_root {
    throw("command root did not stay on the consumer");
}
if context["task_source_root"] != catalog_root {
    throw("task source root did not stay on the skill");
}
log(`rhai-repo=${repo_root}`);
log(`rhai-skill=${catalog_root}`);
log(`rhai-cwd=${cwd}`);
