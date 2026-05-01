fn optional_config_string(path, fallback) {
    let value = config_get(path);
    if value.type_of() == "()" {
        return fallback;
    }
    if value.type_of() != "string" {
        throw `expected Effigy config string at ${path}, got ${value.type_of()}`;
    }
    return trim_string(value);
}

fn primary_database_name() {
    let value = config_get("bundle.databases");
    if value.type_of() == "array" && value.len() > 0 {
        let first = value[0];
        if first.type_of() == "string" {
            return trim_string(first);
        }
    }
    return optional_config_string("bundle.database", "");
}

fn latest_dump_path(dumps_dir) {
    let latest_path = "";
    let latest_name = "";
    for entry in list_dir(dumps_dir) {
        let entry_path = entry.to_string();
        let entry_name = path_file_name(entry_path);
        if entry_name == "" {
            continue;
        }
        if !string_ends_with(entry_name, ".sql") {
            continue;
        }
        if latest_name == "" || entry_name > latest_name {
            latest_name = entry_name;
            latest_path = entry_path;
        }
    }
    return latest_path;
}

let database = primary_database_name();
if database == "" {
    throw "decodelabs seed helper requires `[bundle].database` or `[bundle].databases`";
}

let container_name = optional_config_string("bundle.container_name", "web");
let db_password_path = "containers." + container_name + ".services.db.params.password";
let db_password = optional_config_string(db_password_path, "secret");
let dumps_dir = path_join(repo_root, ".effigy/local/db-seeds");

create_dir(dumps_dir);

let latest_host_path = latest_dump_path(dumps_dir);

if latest_host_path == "" {
    throw "No SQL dumps found in " + dumps_dir
        + "\nexpected one or more .sql files, e.g. legacy-export-20260422135323.sql";
}

let dump_name = path_file_name(latest_host_path);
let dump_container_path = path_join(dumps_dir, dump_name);
let mysql_args = ["--skip-ssl", "-h", "db", "-uroot"];
if db_password != "" {
    mysql_args.push("--password=" + db_password);
}
let reset_sql = "DROP DATABASE IF EXISTS `" + database + "`; CREATE DATABASE `" + database + "`;";

log("[check] resetting database " + database);

let reset_args = mysql_args + ["-e", reset_sql];
let reset_result = run_process_tee("mysql", reset_args);

if reset_result["status"] != 0 {
    throw "[error] database reset failed"
        + "\nstdout:\n" + reset_result["stdout"].to_string()
        + "\nstderr:\n" + reset_result["stderr"].to_string();
}

let reset_output = reset_result["stdout"].to_string();
if trim_string(reset_output) != "" {
    log(reset_output);
}

log("[check] importing latest SQL dump " + dump_name);

let import_args = mysql_args + [database];
let import_result = run_process_tee("mysql", import_args, #{ stdin_file: dump_container_path });

if import_result["status"] != 0 {
    throw "[error] SQL import failed"
        + "\nstdout:\n" + import_result["stdout"].to_string()
        + "\nstderr:\n" + import_result["stderr"].to_string();
}

let import_output = import_result["stdout"].to_string();
if trim_string(import_output) != "" {
    log(import_output);
}

log("[ok] reset and seeded database from " + dump_name);
