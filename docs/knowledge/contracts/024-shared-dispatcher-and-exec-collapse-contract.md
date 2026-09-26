# Shared Result Rendering and Routed Exec Boundary

These internal seams reduce duplicate control flow without changing public
commands, JSON schema IDs, success/error meaning, or release side effects.

## Result Rendering

`src/runner/render.rs` provides `render_command_result` for command surfaces
that already have a JSON value and a text rendering. The helper selects the
requested output mode and preserves the command's existing success/error
payload. It does not plan the command, prompt, execute side effects, or decide
its exit policy. Command-specific report types remain with their owners.

A new caller should use this seam when it has the same JSON/text result shape.
It should not coerce a special error into a generic success or invent a new
schema just to share the helper.

## Routed Container Exec

`src/runner/exec_command/mod.rs` keeps public run/capture and explicit/resolved
policy entrypoints, but they converge on one internal variant path. The
variant preserves:

- the host/container route and selected service;
- explicit policy overrides;
- capture versus inherited output;
- cwd mapping, workspace identity, environment, and TTY policy;
- the existing error classification and text.

`transport.rs` builds the backend invocation. A wrapper is acceptable when it
keeps an established caller name, but it must not reimplement route selection.

## Release Stages

`src/runner/release_command/mod.rs` shares bounded `run_release_stage` control
flow for `prepare` and `execute`. The two stages retain separate plans,
side-effect boundaries, confirmation behavior, and proof. Sharing the shell
of a stage never authorizes release execution or bypasses gates.

## Compatibility Rule

Changes to these seams require command-level parity checks for text, JSON,
exit status, and side effects. A release mutation remains human gated under
the [release procedure](release.md).
