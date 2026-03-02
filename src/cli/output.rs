use crate::{Command, HelpTopic};
use serde_json::json;

pub fn help_topic_label(topic: HelpTopic) -> &'static str {
    match topic {
        HelpTopic::General => "general",
        HelpTopic::Doctor => "doctor",
        HelpTopic::Tasks => "tasks",
        HelpTopic::Test => "test",
        HelpTopic::Watch => "watch",
        HelpTopic::Init => "init",
        HelpTopic::Migrate => "migrate",
    }
}

pub fn command_kind_and_name(cmd: &Command) -> (&'static str, String) {
    match cmd {
        Command::Help(topic) => ("help", help_topic_label(*topic).to_owned()),
        Command::Doctor(_) => ("doctor", "doctor".to_owned()),
        Command::Tasks(_) => ("tasks", "tasks".to_owned()),
        Command::Task(task) => ("task", task.name.clone()),
    }
}

pub fn parse_json_or_string(raw: &str) -> serde_json::Value {
    serde_json::from_str::<serde_json::Value>(raw).unwrap_or_else(|_| json!({ "text": raw }))
}

pub fn emit_json_envelope_success(kind: &str, name: &str, output: &str) {
    let result = parse_json_or_string(output);
    emit_json_envelope_success_value(kind, name, result);
}

pub fn emit_json_envelope_success_value(kind: &str, name: &str, result: serde_json::Value) {
    let payload = json!({
        "schema": "effigy.command.v1",
        "schema_version": 1,
        "ok": true,
        "command": {
            "kind": kind,
            "name": name,
        },
        "result": result,
        "error": serde_json::Value::Null,
    });
    print_json_payload(&payload);
}

pub fn emit_json_envelope_error(
    exit_code: i32,
    kind: &str,
    name: &str,
    error_kind: &str,
    message: &str,
    details: Option<serde_json::Value>,
) -> ! {
    let payload = json!({
        "schema": "effigy.command.v1",
        "schema_version": 1,
        "ok": false,
        "command": {
            "kind": kind,
            "name": name,
        },
        "result": serde_json::Value::Null,
        "error": {
            "kind": error_kind,
            "message": message,
            "details": details.unwrap_or(serde_json::Value::Null),
        }
    });
    print_json_payload(&payload);
    std::process::exit(exit_code);
}

fn print_json_payload(payload: &serde_json::Value) {
    println!(
        "{}",
        serde_json::to_string_pretty(payload).unwrap_or_else(|_| {
            "{\"ok\":false,\"error\":{\"kind\":\"JsonEncodeError\"}}".to_owned()
        })
    );
}
