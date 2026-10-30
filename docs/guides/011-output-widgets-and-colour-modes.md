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

Start a live wait with `ProgressSource::waiting_for(probe)` and
`effigy_ui::steps::live_step(source)`. The source supplies the current
sub-step, known count, last event and elapsed wait; when no further event
exists it names that exact probe. Finish with `succeed(summary)`,
`fail(summary)` or `finish_clear`. The legacy `step(label)` constructor
remains only for sites still on the feedback-spinner inventory.

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

Never write a spinner by hand. A command that owns a richer event stream keeps
its own lines and leaves step drawing to the sink: an engine whose work is in
another crate (bundle and catalog acquisition, release gates) reports
structured events through a hook (`effigy_manifest::SourceProgress`,
`effigy_release::GateObserver`) and the command plane turns them into steps, so
`--json` and piped output stay silent without the engine knowing where it is
running.

### Wording shared across the planes

The labels below are the agreed vocabulary for the command planes. A plane that
adds a step uses the same shape: present tense, the verb first, then the thing
it acts on, with the count or mode in parentheses only when it adds something.

| Plane | Label | One line at the end |
| --- | --- | --- |
| containers | `starting web`, `waiting for web to report healthy (attempt 4)` | `✓ web is healthy` |
| kernel and gateway | `waiting for the environment to settle`, `releasing the gateway route` | `✓ environment settled` |
| bootstrap and setup | `pulling <url> into <path>`, `running setup <selector>`, `starting <selector>` | `✓ setup complete (<selector>)` |
| bundle and catalog sources | `fetching bundle <name> <rev>`, `fetching catalog <name> <rev>` | `✓ fetched bundle <name>` / `✗ could not fetch bundle <name>: <why>` |
| doctor | `scanning for god files (1/2); last: shared scan inventory; 4s elapsed`, `running health task; waiting for health --json; 3s elapsed` | `✓ god files: no problems found` / `✗ health task: stopped at the time budget` |
| secrets | `waiting for machine vault write gate; 12s elapsed`, `decrypting vault; waiting for vault decrypt; 2s elapsed`, `removing local unlock files (1/2); last: removed local-dev key; 1s elapsed`, `running generate hook; last: task \`secrets generate\`; 4s elapsed` | `✓ vault unlocked for commands and tasks` / `✓ vault locked; the next secret-backed task requires a passphrase` / `✓ generated the missing vault secrets` |
| cache and volumes | `inspecting cache volumes; waiting for runtime volume inspect; 3s elapsed`, `removing cache volume \`web-target\` (1/3); last: listed 3 cache volumes; 5s elapsed`, `removing volume \`orphaned-data\` (2/3); last: removed \`web-target\`; 8s elapsed` | the command report; `✗ runtime volume inspect timed out` |
| workspace | `verifying guest artifact; waiting for workspace guest publication; 2s elapsed`, `probing workspace container identity; waiting for container identity probe; 8s elapsed`, `preparing workspace permissions (0/3); waiting for workspace path inspect; 3s elapsed`, `stopping workspace system \`app\`; waiting for container down; 5s elapsed` | `✓ workspace system app stopped` / `✗ workspace identity changed during publication; run \`effigy container up\` on the host` |
| release | `running gate <name> (2/5)`, `reusing hosted evidence for gate <name> (1/1)` | `✓ gate <name> passed after 12s (2/5)` |
| deploy | `resolving deploy provider <name> (offline)` | `✓ resolved deploy provider <name> from cached checkout at commit <sha>` |
| task runs | ``waiting for heavy admission (task `dev`, 2 ahead)``, `running dev`, `running dev step 2/4: build` | ``✓ admitted heavy task `dev` after 12s`` |
| state apply and capture | ``running task layer `structure` ``, ``importing SQL layer `baseline` ``, ``staging artifact layer `legacy` ``, ``running apply hook `legacy` ``, ``running capture task `capture:uat` `` | ``✓ applied layer `structure` `` / ``✗ apply hook `legacy` failed: …`` |

A single-step task omits the step count and generated ordinal name; it uses a
task description when present. Multi-step tasks include the position and a
meaningful step subject.

A failure that wraps a child command never reports only that something failed.
It names the child and its exit code, shows the bounded tail of the child's own
output (redacted), and gives the next action:

```text
[error] bootstrap setup task `fails` failed: command `fails` failed
  cause: exit code 7
         stderr: the fixture refuses to start
  next: run `effigy fails` on its own to see it fail
```

`RunnerError::TaskCommandFailure` renders that way for every plane, and
`child_failure_text` in `src/runner/error.rs` wraps it without losing the
child's evidence. A new wrapper lifts the child's `HumanError`; it never
repeats `{error:?}` or quotes the whole streams. A hook line the engine streams
(a heavy-admission position, a gate's own progress) becomes the step's label,
not a second line under it.

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
renders through `Display` already supplies the sentence; a refusal that also
carries structured details lifts their `cause`, phase and `stop`/`next` keys,
so the text and `--json` tell the same story.

Rust `Debug` output, enum and variant names, and nested
`Unknown(StoreUnreadable(…))` chains must not reach a person. Render the value
through `Display` instead — the kernel's `StoreError`, `RunError`, `GateError`,
`Interrupt` and `UnknownReason` all do — and keep `Debug` for `--json` and
verbose diagnostics.

### Argument errors

An argument error names what is wrong, then the nearest command's own usage: one
or two `Usage` lines of that command, the words it accepts where the typed word
failed, and a "did you mean" when the typed word is a near miss or a command
that lives somewhere else. It never prints the whole help panel;
`effigy <command> --help` is the pointer for the rest.

```text
[error] Invalid command arguments
  unknown argument: app
  hint: Run `effigy container --help` for every form

`effigy container exec` does not exist: `exec` is its own command — `effigy exec [--repo <PATH>] [--service <NAME>] [--json] <COMMAND> [ARGS...]`
```

When there is no such supported form, the same slot lists the words the command
accepts instead:

```text
effigy container accepts here: cache, data, down, hosts, logs, migrate, ports, profile, render, retire, scope, stats, status, up, volume
```

`effigy_cli::usage_hint` derives that answer from the typed argv and the
command's own help panel, so a usage line that changes changes the answer. The
`--json` details carry the same facts as `hint`, `usage`, `supported` and
`did_you_mean`.

Two rules keep the answer honest:

- a message only suggests a command this checkout actually reaches. A
  repository task may own a built-in word (contract `043`), so a lock conflict
  in a repository that declares a `tasks` task says the built-in inventory is
  unavailable there instead of suggesting `effigy tasks status …`;
- `tests/suggested_commands.rs` collects every backtick-quoted `effigy …` form in
  `src/` and `crates/`, substitutes its placeholders and parses it with this
  checkout's grammar. A suggestion that stops parsing fails
  `effigy test:feedback:suggestions`.

`effigy qa:docs:human-error-debug` is the ratchet that keeps it that way. It
counts Debug format specifiers inside string macros in files that define or
construct human-facing errors, and fails when the count rises above
`config/human-error-debug-ratchet.json`. The runner, built-in, managed, doctor,
kernel and human-error vocabularies are counted, and so are the container
policy and manifest errors (`ContainerPolicyError`, `ManifestError`) and the
managed-run `NotReady` refusal that effigy#402 found outside the ratchet. When a
change lowers a count, lower the baseline in the same change:

```bash
python3 scripts/check-human-error-debug.py --write-baseline
```

The baseline is a per-file count, so a file the ratchet newly covers records its
current count. One such line is deliberately internal: `inline_workspace.rs`
formats a `Debug` of the inline container config into the render closure's
`inputs/manifest.toml`, which is the provenance of the generated Compose file and
never part of a message a person reads (`container migrate` filters `inputs/` out
of everything it writes). Keep such an entry only with the same evidence: say in
a comment what the string is, and why no command prints it.
