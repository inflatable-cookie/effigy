container::up("builder", true);
let result = container::exec("builder", ["printf", "%s", "space and $literal; untouched"], #{"stdin_file": path::join(repo_root, "input.txt"), "cwd": repo_root, "env": #{"PAYLOAD": "a b;$raw"}});
if !result["success"] { throw("literal exec failed"); }
