fn optional_config_string(path, fallback) {
    let value = config::get(path);
    if value.type_of() == "()" {
        return fallback;
    }
    if value.type_of() != "string" {
        throw `expected Effigy config string at ${path}, got ${value.type_of()}`;
    }
    return str::trim(value);
}

fn optional_env_string(name, fallback) {
    let value = env(name);
    if value == "" {
        return fallback;
    }
    return str::trim(value);
}

fn primary_database_name() {
    let value = config::get("bundle.databases");
    if value.type_of() == "array" && value.len() > 0 {
        let first = value[0];
        if first.type_of() == "string" {
            return str::trim(first);
        }
    }
    return optional_config_string("bundle.database", "");
}

fn declared_databases() {
    let value = config::get("bundle.databases");
    if value.type_of() == "array" && value.len() > 0 {
        return value;
    }
    let database = optional_config_string("bundle.database", "");
    if database != "" {
        return [database];
    }
    return [];
}

fn latest_dump_path(dumps_dir) {
    let latest_path = "";
    let latest_name = "";
    for entry in fs::list(dumps_dir) {
        let entry_path = entry.to_string();
        let entry_name = path::file_name(entry_path);
        if entry_name == "" {
            continue;
        }
        if !str::ends_with(entry_name, ".sql") {
            continue;
        }
        if latest_name == "" || entry_name > latest_name {
            latest_name = entry_name;
            latest_path = entry_path;
        }
    }
    return latest_path;
}

fn seed_specs_from_env_or_file(dumps_dir) {
    let metadata_path = path::join(dumps_dir, "_effigy-bootstrap-db-seeds.json");
    let raw = if fs::exists(metadata_path) {
        str::trim(fs::read_file(metadata_path))
    } else {
        optional_env_string("EFFIGY_BOOTSTRAP_DB_SEEDS_JSON", "")
    };
    if raw == "" {
        return [];
    }
    let parsed = json::parse(raw);
    if parsed.type_of() != "array" {
        throw "EFFIGY_BOOTSTRAP_DB_SEEDS_JSON must decode to a JSON array";
    }
    return parsed;
}

fn host_dump_path(dump_repo_path) {
    if str::starts_with(dump_repo_path, "/") {
        return dump_repo_path;
    }
    return path::join(repo_root, dump_repo_path);
}

fn mysql_cli_args(db_password) {
    let args = ["mysql", "--skip-ssl", "-h", "db", "-uroot"];
    if db_password != "" {
        args.push("--password=" + db_password);
    }
    return args;
}

fn import_dump(container_name, db_password, database, dump_repo_path, dump_name) {
    let mysql_args = mysql_cli_args(db_password);
    let import_path = host_dump_path(dump_repo_path);
    let reset_sql = "DROP DATABASE IF EXISTS `" + database + "`; CREATE DATABASE `" + database + "`;";
    let exec_options = #{ run_in: "container", container: container_name, service: "db" };

    log("[check] resetting database " + database);
    let reset_result = exec::run(mysql_args + ["-e", reset_sql], exec_options);
    if reset_result["status"] != 0 {
        throw "[error] database reset failed"
            + "\nstdout:\n" + reset_result["stdout"].to_string()
            + "\nstderr:\n" + reset_result["stderr"].to_string();
    }

    log("[check] importing SQL dump " + dump_name + " into " + database);
    let import_result = exec::run(
        mysql_args + [database],
        #{ run_in: "container", container: container_name, service: "db", stdin_file: import_path },
    );

    if import_result["status"] != 0 {
        throw "[error] SQL import failed"
            + "\nstdout:\n" + import_result["stdout"].to_string()
            + "\nstderr:\n" + import_result["stderr"].to_string();
    }

    let import_output = import_result["stdout"].to_string();
    if str::trim(import_output) != "" {
        log(import_output);
    }

    log("[ok] reset and seeded database " + database + " from " + dump_name);
}

let database = primary_database_name();
if database == "" {
    throw "decodelabs seed helper requires `[bundle].database` or `[bundle].databases`";
}

let container_name = optional_config_string("bundle.container_name", "web");
let db_password_path = "containers." + container_name + ".services.db.params.password";
let db_password = optional_config_string(db_password_path, "secret");
let dumps_dir = optional_env_string(
    "EFFIGY_BOOTSTRAP_DB_SEEDS_DIR",
    ".effigy/local/db-seeds",
);
let databases = declared_databases();
let seed_specs = seed_specs_from_env_or_file(dumps_dir);

fs::create_dir(dumps_dir);

if seed_specs.len() > 0 {
    for spec in seed_specs {
        let target = "";
        if spec["target"].type_of() == "string" {
            target = str::trim(spec["target"]);
        }
        let effective_database = target;
        if effective_database == "" {
            if databases.len() == 1 {
                effective_database = databases[0].to_string();
            } else {
                throw "bootstrap db seed entry is missing a target for a multi-database bundle";
            }
        }
        let staged_path = "";
        if spec["staged_path"].type_of() == "string" {
            staged_path = str::trim(spec["staged_path"]);
        }
        if staged_path == "" {
            throw "bootstrap db seed entry is missing staged_path";
        }
        let dump_name = path::file_name(staged_path);
        if dump_name == "" {
            throw "bootstrap db seed entry has an invalid staged_path";
        }
        import_dump(container_name, db_password, effective_database, staged_path, dump_name);
    }
    return;
}

let latest_host_path = latest_dump_path(dumps_dir);

if latest_host_path == "" {
    throw "No SQL dumps found in " + dumps_dir
        + "\nexpected one or more .sql files, e.g. legacy-export-20260422135323.sql";
}

let dump_name = path::file_name(latest_host_path);
let dump_repo_path = path::join(".effigy/local/db-seeds", dump_name);
import_dump(container_name, db_password, database, dump_repo_path, dump_name);
