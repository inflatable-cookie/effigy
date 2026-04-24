fn optional_config_string(path, fallback) {
    let value = config_get(path);
    if value.type_of() == "()" {
        return fallback;
    }
    if value.type_of() != "string" {
        throw `expected Effigy config string at ${path}, got ${value.type_of()}`;
    }
    return value.to_string().trim();
}

fn primary_database_name() {
    let value = config_get("bundle.databases");
    if value.type_of() == "array" && value.len() > 0 {
        let first = value[0];
        if first.type_of() == "string" {
            return first.to_string().trim();
        }
    }
    return optional_config_string("bundle.database", "");
}

let database = primary_database_name();
if database == "" {
    throw "decodelabs seed helper requires `[bundle].database` or `[bundle].databases`";
}

let container_name = optional_config_string("bundle.container_name", "web");
let db_root_password_path = "containers." + container_name + ".services.db.root_password";
let root_password = optional_config_string(db_root_password_path, "secret");
let dumps_dir = path_join(repo_root, ".effigy/local/db-seeds");
let dump_prefix = database + "-";

create_dir(dumps_dir);

let latest_host_path = "";
for entry in list_dir(dumps_dir) {
    let path = entry.to_string();
    let dump_name = path_file_name(path);

    if !is_file(path) {
        continue;
    }

    if !dump_name.starts_with(dump_prefix) || !dump_name.ends_with(".sql") {
        continue;
    }

    if latest_host_path == "" || path > latest_host_path {
        latest_host_path = path;
    }
}

if latest_host_path == "" {
    throw "No SQL dumps found in " + dumps_dir
        + "\nexpected files like " + database + "-20260422135323.sql";
}

let dump_name = path_file_name(latest_host_path);
let dump_container_path = path_join("/var/www/html/.effigy/local/db-seeds", dump_name);
let mysql_password_flag = if root_password == "" {
    ""
} else {
    " -p" + root_password
};

log("[check] resetting database " + database);

let reset_result = run_process("sh", [
    "-lc",
    "mysql --skip-ssl -h db -uroot" + mysql_password_flag
        + " -e \"DROP DATABASE IF EXISTS `" + database + "`; CREATE DATABASE `" + database + "`;\"",
]);

if reset_result["status"] != 0 {
    throw "[error] database reset failed"
        + "\nstdout:\n" + reset_result["stdout"].to_string()
        + "\nstderr:\n" + reset_result["stderr"].to_string();
}

let reset_output = reset_result["stdout"].to_string();
if reset_output.trim() != "" {
    log(reset_output);
}

log("[check] importing latest SQL dump " + dump_name);

let import_result = run_process("sh", [
    "-lc",
    "mysql --skip-ssl -h db -uroot" + mysql_password_flag
        + " `" + database + "` < " + dump_container_path,
]);

if import_result["status"] != 0 {
    throw "[error] SQL import failed"
        + "\nstdout:\n" + import_result["stdout"].to_string()
        + "\nstderr:\n" + import_result["stderr"].to_string();
}

let import_output = import_result["stdout"].to_string();
if import_output.trim() != "" {
    log(import_output);
}

log("[ok] reset and seeded database from " + dump_name);
