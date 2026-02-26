# 011 - Output Widgets and Colour Modes

This guide defines how command output should be produced in Effigy after roadmap 003.

## 1) Goals

- Keep normal CLI output consistent across built-in commands.
- Use semantic widgets instead of ad-hoc string formatting.
- Keep output readable with colour disabled and in CI/non-TTY environments.

## 2) Renderer Contract

Command surfaces should use the shared `Renderer` interface in `src/ui/renderer.rs` and the default `PlainRenderer` in `src/ui/plain_renderer.rs`.

Preferred widgets:

- `section(title)` for major output blocks
- `notice(level, body)` for high-signal status lines
- `error_block` / `warning_block` / `success_block` for contextual messages
- `key_values` for compact metadata
- `bullet_list` for evidence/risk/action lists
- `table` for list/report output
- `summary` for command-level totals
- `spinner` for long-running progress (with fallback behavior)

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
