# 003 - Coloured Output and Widget Layer

Status: In Progress
Owner: Platform
Created: 2026-02-26
Depends on: 001

## 1) Problem

Effigy currently emits mostly plain-text output. As commands expand, output shape and tone can drift by feature owner, making failures harder to scan and success states less obvious. We need a consistent, reusable output layer for normal CLI mode that supports colour, structured blocks, and progress feedback without forcing a full-screen TUI.

## 2) Goals

- [x] Lock crate stack for normal-mode widgets.
- [x] Define a stable internal widget contract (`ui::widgets`) that command logic can reuse.
- [x] Implement a plain renderer with consistent success/error/notice/section/summary output.
- [x] Implement progress and spinner integration for long-running tasks.
- [x] Implement table rendering for list/report commands.
- [x] Add colour-mode controls (`auto`, `always`, `never`) and environment handling.
- [x] Migrate existing built-in commands to the new renderer.
- [ ] Document authoring guidance for new command output.

## 3) Non-Goals

- [ ] No requirement for full-screen interaction in phase 003.
- [ ] No ratatui event loop or alternate-screen mode in phase 003.
- [ ] No redesign of resolver/runner core behavior.
- [ ] No JSON schema stabilization beyond current machine-output needs.

## 4) Decision

Selected stack (Option A):

- `anstream`: output stream + ANSI adaptation.
- `indicatif`: spinners and progress bars.
- `tabled`: table widgets for normal CLI output.
- internal `ui::widgets` facade: project-defined semantic widgets for consistency.

Rationale:
- crates are active and broadly used;
- each crate is focused and composable;
- internal facade prevents command code from depending directly on third-party rendering APIs.

## 5) Widget Contract

All command-facing output goes through a renderer interface and semantic widgets.

### 5.1 Module layout

```text
src/ui/
  mod.rs
  theme.rs
  widgets.rs
  renderer.rs
  plain_renderer.rs
  progress.rs
  table.rs
```

### 5.2 Core output types

```rust
pub enum NoticeLevel {
    Info,
    Success,
    Warning,
    Error,
}

pub enum StepState {
    Pending,
    Running,
    Done,
    Failed,
}

pub struct MessageBlock<'a> {
    pub title: &'a str,
    pub body: &'a str,
    pub hint: Option<&'a str>,
}

pub struct KeyValue<'a> {
    pub key: &'a str,
    pub value: &'a str,
}

pub struct SummaryCounts {
    pub ok: usize,
    pub warn: usize,
    pub err: usize,
}
```

### 5.3 Renderer trait

```rust
pub trait Renderer {
    fn section(&mut self, title: &str) -> anyhow::Result<()>;
    fn notice(&mut self, level: NoticeLevel, body: &str) -> anyhow::Result<()>;

    fn success_block(&mut self, block: &MessageBlock<'_>) -> anyhow::Result<()>;
    fn error_block(&mut self, block: &MessageBlock<'_>) -> anyhow::Result<()>;
    fn warning_block(&mut self, block: &MessageBlock<'_>) -> anyhow::Result<()>;

    fn key_values(&mut self, items: &[KeyValue<'_>]) -> anyhow::Result<()>;
    fn step(&mut self, label: &str, state: StepState) -> anyhow::Result<()>;
    fn summary(&mut self, counts: SummaryCounts) -> anyhow::Result<()>;

    fn table(&mut self, spec: &TableSpec<'_>) -> anyhow::Result<()>;
    fn spinner(&mut self, label: &str) -> anyhow::Result<Box<dyn SpinnerHandle>>;
}
```

### 5.4 Progress handle

```rust
pub trait SpinnerHandle {
    fn set_message(&self, message: &str);
    fn finish_success(&self, message: &str);
    fn finish_error(&self, message: &str);
}
```

## 6) Theme and Output Modes

`ui::theme` defines style tokens rather than ad-hoc colours:

- `accent`
- `muted`
- `success`
- `warning`
- `error`
- `label`
- `value`

Output mode controls:

- `auto` (default): enabled only when output supports styling.
- `always`: force styled output.
- `never`: plain, no colour/styling escapes.

Environment handling:

- `NO_COLOR` disables styling.
- CI environments may default to simplified progress display (no animated spinners when not TTY).

## 7) Command Migration Targets

Phase 003 migration scope:

- `tasks` built-in:
  - render catalog/task listing with table widget;
  - use section + summary blocks.
- `repo-pulse` built-in:
  - render health/warning/success blocks consistently;
  - use progress spinner for filesystem scan phases where useful.
- runner-level failures:
  - unify parse/resolution/execution error presentation via `error_block`.

## 8) Execution Plan

### Phase 3.1 - Foundation
- [x] Add `src/ui` modules and core types.
- [x] Add renderer trait + `PlainRenderer`.
- [x] Add theme tokens and colour-mode plumbing.
- [x] Add unit tests for deterministic text rendering (colour disabled snapshots).

### Phase 3.2 - Widget expansion
- [x] Add table adapter using `tabled`.
- [x] Add spinner/progress adapter using `indicatif`.
- [x] Add handling for non-TTY and CI fallback behavior.
- [x] Add tests for spinner no-op fallback when output is non-interactive.

### Phase 3.3 - Command adoption
- [x] Migrate `tasks` and `repo-pulse` to renderer calls.
- [ ] Replace direct `println!` paths in command surfaces.
- [ ] Add integration tests for representative success/failure flows.
- [ ] Update README and docs with output conventions.

## 9) Acceptance Criteria

- [x] Commands use semantic widgets through `Renderer`, not ad-hoc formatting.
- [ ] Success/error/notice block styling is consistent across built-ins.
- [ ] Tables and progress feedback are available in normal mode.
- [ ] Output remains readable and deterministic with colour disabled.
- [ ] Core command behavior remains unchanged except output presentation.

## 10) Risks and Mitigations

- [ ] Risk: output regressions while migrating commands.
  - Mitigation: snapshot tests with colour disabled and stable width assumptions.
- [ ] Risk: spinner output clobbers logs in non-TTY environments.
  - Mitigation: strict TTY detection and fallback to line-based status.
- [ ] Risk: too much direct dependency on third-party APIs in command code.
  - Mitigation: keep all crate-specific logic inside `ui` module adapters.

## 11) Deliverables

- [x] `src/ui` module with renderer contract and plain implementation.
- [x] Table and spinner integrations.
- [x] Migrated built-ins using widget API.
- [ ] Documentation for output authoring conventions and mode flags.
