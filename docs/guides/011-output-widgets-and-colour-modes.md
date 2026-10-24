# 011 - Output Widgets and Colour Modes

This guide defines how command output should be produced in Effigy after roadmap 003.

## 1) Goals

- Keep normal CLI output consistent across built-in commands.
- Use semantic widgets instead of ad-hoc string formatting.
- Keep output readable with colour disabled and in CI/non-TTY environments.

## 2) Renderer Contract

Command surfaces should use the shared `Renderer` interface in
`crates/effigy-ui/src/renderer.rs` and the default `PlainRenderer` in
`crates/effigy-ui/src/plain_renderer.rs`.

Preferred widgets:

- `section(title)` for major output blocks
- `notice(level, body)` for high-signal status lines
- `error_block` / `warning_block` / `success_block` for contextual messages
- `key_values` for compact metadata
- `bullet_list` for evidence/risk/action lists
- `table` for list/report output
- `summary` for command-level totals
- `spinner` for a single long-running phase owned by a renderer call
- `effigy_ui::steps::step` for progress a person watches as work moves on; see
  section 6, which is the preferred API for new work

## 3) Colour and Mode Controls

Effigy uses the following mode/env behavior:

- `EFFIGY_COLOR=auto|always|never`
  - `auto`: enable colour only when the stream is a terminal
  - `always`: force ANSI colour output (except where disabled by `NO_COLOR`)
  - `never`: disable colour
- `NO_COLOR`
  - if present, colour is disabled regardless of other settings
- `CI`
  - animated spinner behavior is disabled in CI for stable logs

Precedence:
1. `NO_COLOR` disables styling.
2. Otherwise, `EFFIGY_COLOR` applies.
3. If unset, default is `auto`.

Default palette:
- Effigy uses a Gum-inspired default palette (soft neutral text with a vivid accent), while preserving semantic mappings (`info`, `success`, `warning`, `error`).

## 4) Authoring Rules

When adding/updating command output:

1. Do not add direct command-surface `println!`/`eprintln!` formatting for user-facing output.
2. Build output through widgets using renderer primitives.
3. Prefer semantic labels (`warning`, `error`) over decorative text.
4. Keep text deterministic when colour is disabled.
5. Add/adjust tests when output shape changes.

## 5) Testing Expectations

- Unit tests should cover renderer behavior with colour disabled.
- Command tests should validate representative success and failure flows.
- CLI integration tests should assert output remains ANSI-free when `NO_COLOR` is set.

## Related Guides

- [`017-json-output-contracts.md`](017-json-output-contracts.md)
- [`029-docs-qa-checklist-and-validation.md`](029-docs-qa-checklist-and-validation.md)

## Next Step

After changing renderer behavior, run docs and JSON contract checks from [`029-docs-qa-checklist-and-validation.md`](029-docs-qa-checklist-and-validation.md) before merging.

## 6) Step Progress and Error Style

Two shared pieces cover live progress and failures in every command plane.
They live in `effigy-ui` so the runner, the built-ins and the container code
all reach the same behaviour.

### Step progress

Start a step with `effigy_ui::steps::step(label)`; the handle offers
`set_label`, `succeed(summary)`, `fail(summary)`, `finish_clear` and nested
sub-steps via `step(label)`.

Labels are short, present tense and name their subject:

- `starting web`
- `waiting for web to report healthy (attempt 4, http://localhost:8080/healthz)`
- `fetching bundle decodelabs`
- `starting the Colima runtime`

Each step ends with one line: `✓ waiting for web to report healthy` or
`✗ web did not report healthy within 60s`. The label changes while the work
moves on, so a slow step never looks stuck.

Where the output goes:

- a terminal draws an asynchronous spinner on stderr;
- `--json`, piped output, CI and any non-terminal draw nothing, unless the
  command's verbose flag asks for plain one-line-per-step output (`StepMode::Lines`);
- every step draws through one process-wide sink, so concurrent steps stack
  instead of interleaving;
- while the managed-run TUI owns the terminal, `steps::suspend_terminal()`
  silences the sink so the two never draw over each other.

Never write a spinner by hand. A command that owns a richer event stream (the
bootstrap and workspace reporters) keeps its own lines and leaves step drawing
to the sink.

### Errors people can read

A failure rendered for a person carries three facts:

1. what failed (the service, file, command, route or checkout);
2. the evidence (exit code, the last relevant output, the health check's own
   message, the conflicting owner), bounded and with credential-looking values
   redacted;
3. the next action, as an exact command when one exists.

Build those with `effigy_ui::human::HumanError`: `render_text()` prints
`[error] <what failed>` with `cause:` lines and a `next:` line, and `to_json()`
is the same value for `--json` (`error.human` in the `effigy.command.v1`
envelope). The runner's top-level printer uses it, so a `RunnerError` that
renders through `Display` already supplies the sentence.

Rust `Debug` output, enum and variant names, and nested
`Unknown(StoreUnreadable(…))` chains must not reach a person. Render the value
through `Display` instead — the kernel's `StoreError`, `RunError`, `GateError`,
`Interrupt` and `UnknownReason` all do — and keep `Debug` for `--json` and
verbose diagnostics.

`effigy qa:docs:human-error-debug` is the ratchet that keeps it that way. It
counts Debug format specifiers inside string macros in files that define or
construct human-facing errors, and fails when the count rises above
`config/human-error-debug-ratchet.json`. When a change lowers a count, lower
the baseline in the same change:

```bash
python3 scripts/check-human-error-debug.py --write-baseline
```
