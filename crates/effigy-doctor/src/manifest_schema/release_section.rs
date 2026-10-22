use toml::Value;

use super::diagnostics::SchemaContext;
use super::tables::{require_table, validate_allowed_keys};
use super::values::{
    validate_optional_boolean_field, validate_optional_non_empty_string_field,
    validate_optional_string_array_field,
};

pub(super) fn validate_release_section(context: &mut SchemaContext<'_, '_>, release: &Value) {
    let Some(release_table) = require_table(
        context,
        "release",
        release,
        "expected table with optional keys: version_file, version_path, changelog, pre-1-0, initial-tag-current-version, sync_files, gates, tag_format, hosted-evidence, identities, artifacts, targets, approve",
    ) else {
        return;
    };

    validate_allowed_keys(
        context,
        "release",
        release_table,
        &[
            "version_file",
            "version-file",
            "version_path",
            "version-path",
            "changelog",
            "pre-1-0",
            "initial_tag_current_version",
            "initial-tag-current-version",
            "sync_files",
            "sync-files",
            "gates",
            "tag_format",
            "tag-format",
            "hosted_evidence",
            "hosted-evidence",
            "identities",
            "artifacts",
            "targets",
            "approve",
        ],
    );

    validate_optional_non_empty_string_field(
        context,
        release_table
            .get("version_file")
            .or_else(|| release_table.get("version-file")),
        "release.version_file",
    );
    validate_optional_non_empty_string_field(
        context,
        release_table
            .get("version_path")
            .or_else(|| release_table.get("version-path")),
        "release.version_path",
    );
    validate_optional_non_empty_string_field(
        context,
        release_table.get("changelog"),
        "release.changelog",
    );
    validate_optional_boolean_field(context, release_table.get("pre-1-0"), "release.pre-1-0");
    validate_optional_boolean_field(
        context,
        release_table
            .get("initial_tag_current_version")
            .or_else(|| release_table.get("initial-tag-current-version")),
        "release.initial_tag_current_version",
    );
    validate_optional_string_array_field(
        context,
        release_table
            .get("sync_files")
            .or_else(|| release_table.get("sync-files")),
        "release.sync_files",
        "expected array of strings",
    );
    validate_optional_non_empty_string_field(
        context,
        release_table
            .get("tag_format")
            .or_else(|| release_table.get("tag-format")),
        "release.tag_format",
    );

    if let Some(hosted_evidence) = release_table
        .get("hosted-evidence")
        .or_else(|| release_table.get("hosted_evidence"))
    {
        validate_hosted_evidence(context, hosted_evidence);
    }

    if let Some(gates) = release_table.get("gates") {
        validate_release_gates(context, gates);
    }
    if let Some(approve) = release_table.get("approve") {
        validate_optional_string_array_field(
            context,
            Some(approve),
            "release.approve",
            "expected array of stage names",
        );
    }
    if let Some(identities) = release_table.get("identities") {
        validate_release_identities(context, identities);
    }
    if let Some(artifacts) = release_table.get("artifacts") {
        validate_release_artifacts(context, artifacts);
    }
    if let Some(targets) = release_table.get("targets") {
        validate_release_targets(context, targets);
    }
}

const RELEASE_IDENTITY_KINDS: [&str; 4] = ["semver", "digest", "source", "task"];
const RELEASE_TARGET_KINDS: [&str; 2] = ["tag", "task"];
const RELEASE_GATE_STAGES: [&str; 2] = ["pre", "certify"];

fn validate_release_identities(context: &mut SchemaContext<'_, '_>, identities: &Value) {
    let Some(table) = require_table(
        context,
        "release.identities",
        identities,
        "expected table of named identities",
    ) else {
        return;
    };

    for (name, value) in table {
        let path = format!("release.identities.{name}");
        let Some(identity) = require_table(
            context,
            &path,
            value,
            "expected table with `kind`, `version-file`, `version-path`, `tag-format`, `follows` and/or `task`",
        ) else {
            continue;
        };
        validate_allowed_keys(
            context,
            &path,
            identity,
            &[
                "kind",
                "version_file",
                "version-file",
                "version_path",
                "version-path",
                "tag_format",
                "tag-format",
                "follows",
                "task",
            ],
        );
        validate_optional_enum_field(
            context,
            identity.get("kind"),
            &format!("{path}.kind"),
            &RELEASE_IDENTITY_KINDS,
        );
        for (key, field) in [
            ("version_file", "version-file"),
            ("version_path", "version-path"),
            ("tag_format", "tag-format"),
        ] {
            validate_optional_non_empty_string_field(
                context,
                identity.get(key).or_else(|| identity.get(field)),
                &format!("{path}.{key}"),
            );
        }
        validate_optional_non_empty_string_field(
            context,
            identity.get("follows"),
            &format!("{path}.follows"),
        );
        validate_optional_non_empty_string_field(
            context,
            identity.get("task"),
            &format!("{path}.task"),
        );
    }
}

fn validate_release_artifacts(context: &mut SchemaContext<'_, '_>, artifacts: &Value) {
    let Some(table) = require_table(
        context,
        "release.artifacts",
        artifacts,
        "expected table of named artifacts",
    ) else {
        return;
    };

    for (name, value) in table {
        let path = format!("release.artifacts.{name}");
        let Some(artifact) = require_table(
            context,
            &path,
            value,
            "expected table with `task` and `path`",
        ) else {
            continue;
        };
        validate_allowed_keys(context, &path, artifact, &["task", "path"]);
        validate_optional_non_empty_string_field(
            context,
            artifact.get("task"),
            &format!("{path}.task"),
        );
        validate_optional_non_empty_string_field(
            context,
            artifact.get("path"),
            &format!("{path}.path"),
        );
    }
}

fn validate_release_targets(context: &mut SchemaContext<'_, '_>, targets: &Value) {
    let Some(table) = require_table(
        context,
        "release.targets",
        targets,
        "expected table of named targets",
    ) else {
        return;
    };

    for (name, value) in table {
        let path = format!("release.targets.{name}");
        let Some(target) = require_table(
            context,
            &path,
            value,
            "expected table with `kind` (`tag` or `task`) and optional `task`, `check`, `verify`",
        ) else {
            continue;
        };
        validate_allowed_keys(context, &path, target, &["kind", "task", "check", "verify"]);
        validate_optional_enum_field(
            context,
            target.get("kind"),
            &format!("{path}.kind"),
            &RELEASE_TARGET_KINDS,
        );
        for key in ["task", "check", "verify"] {
            validate_optional_non_empty_string_field(
                context,
                target.get(key),
                &format!("{path}.{key}"),
            );
        }
    }
}

fn validate_optional_enum_field(
    context: &mut SchemaContext<'_, '_>,
    value: Option<&Value>,
    path: &str,
    allowed: &[&str],
) {
    let Some(value) = value else {
        return;
    };
    match value.as_str() {
        Some(text) if allowed.contains(&text) => {}
        Some(text) => {
            context.unsupported_value(path, text, &format!("one of {}", allowed.join(", ")))
        }
        None => context.invalid_value_type(path, "string"),
    }
}

fn validate_hosted_evidence(context: &mut SchemaContext<'_, '_>, hosted_evidence: &Value) {
    let Some(hosted_table) = require_table(
        context,
        "release.hosted-evidence",
        hosted_evidence,
        "expected table with `workflow` and optional `event`/`branch`",
    ) else {
        return;
    };

    validate_allowed_keys(
        context,
        "release.hosted-evidence",
        hosted_table,
        &["workflow", "event", "branch"],
    );
    validate_optional_non_empty_string_field(
        context,
        hosted_table.get("workflow"),
        "release.hosted-evidence.workflow",
    );
    validate_optional_non_empty_string_field(
        context,
        hosted_table.get("event"),
        "release.hosted-evidence.event",
    );
    validate_optional_non_empty_string_field(
        context,
        hosted_table.get("branch"),
        "release.hosted-evidence.branch",
    );
}

fn validate_release_gates(context: &mut SchemaContext<'_, '_>, gates: &Value) {
    let Some(gates_table) = require_table(
        context,
        "release.gates",
        gates,
        "expected table of named gate commands",
    ) else {
        return;
    };

    for (name, gate_value) in gates_table {
        let gate_path = format!("release.gates.{name}");
        if gate_value.is_str() {
            validate_optional_non_empty_string_field(context, Some(gate_value), &gate_path);
            continue;
        }

        let Some(gate_table) = require_table(
            context,
            &gate_path,
            gate_value,
            "expected string command or table with `command`/`task`, `stage`, `description` and/or `reuse-hosted-evidence`",
        ) else {
            continue;
        };

        validate_allowed_keys(
            context,
            &gate_path,
            gate_table,
            &[
                "command",
                "task",
                "stage",
                "description",
                "reuse-hosted-evidence",
                "reuse_hosted_evidence",
            ],
        );
        validate_optional_non_empty_string_field(
            context,
            gate_table.get("task"),
            &format!("{gate_path}.task"),
        );
        validate_optional_enum_field(
            context,
            gate_table.get("stage"),
            &format!("{gate_path}.stage"),
            &RELEASE_GATE_STAGES,
        );
        validate_optional_non_empty_string_field(
            context,
            gate_table.get("command"),
            &format!("{gate_path}.command"),
        );
        validate_optional_non_empty_string_field(
            context,
            gate_table.get("description"),
            &format!("{gate_path}.description"),
        );
        validate_optional_boolean_field(
            context,
            gate_table
                .get("reuse-hosted-evidence")
                .or_else(|| gate_table.get("reuse_hosted_evidence")),
            &format!("{gate_path}.reuse-hosted-evidence"),
        );
    }
}
