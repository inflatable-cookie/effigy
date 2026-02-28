# TUI Terminal Emulation Phase 6.1 Selection

Date: 2026-02-27
Owner: Platform
Related roadmap: 006 - TUI Terminal Emulation

## Scope
- Evaluate terminal emulation core options for Effigy TUI process panes.
- Compare integration cost and capability fit for:
  - `alacritty_terminal`
  - `wezterm_term` (or equivalent WezTerm core path)
  - `vt100`
- Select recommended path for the phase 6.1 spike implementation.

## Changes
- Evaluated candidate crates and availability:
  - `alacritty_terminal` exists on crates.io (`0.25.1`) and is actively used by Alacritty.
  - `wezterm_term` is not available as a first-party crates.io package under that name; public crates are forks/wrapper artifacts, which increases adoption risk.
  - `vt100` exists on crates.io (`0.16.2`) with direct parser/screen API and explicit scrollback support.
- Confirmed local compatibility signals:
  - `alacritty_terminal` currently declares `rust-version = 1.85.0`, which raises minimum toolchain requirements for Effigy.
  - `vt100` declares `rust-version = 1.70`, lowering toolchain friction.
- Selected recommendation:
  - Use `vt100` for phase 6.1 spike and first integration pass.

## Validation
- command: `cargo search alacritty_terminal --limit 1 && cargo search wezterm_term --limit 1 && cargo search vt100 --limit 3`
  - result: pass; confirms crates availability, including absence of first-party `wezterm_term`.
- command: `cargo info alacritty_terminal`
  - result: pass; confirms crate metadata and `rust-version = 1.85.0`.
- command: `cargo info vt100`
  - result: pass; confirms crate metadata and parser purpose.
- command: `rg`/`sed` inspection of downloaded crate sources in `~/.cargo/registry/src/...`
  - result: pass; confirms `vt100::Parser::new(rows, cols, scrollback_len)`, byte `process(&[u8])`, and screen/scrollback APIs required for the spike.

## Risks / Follow-ups
- `vt100` may not fully cover every terminal extension used by all tools; we should validate against `nextest`, `vite`, and one color-heavy process.
- Process manager currently emits line-based string events; emulator integration requires raw byte stream events from PTY readers.
- Rendering adapter work is still required to map emulator screen cells into ratatui lines/spans without performance regressions.

## Next
- Implement phase 6.1 spike:
  - add raw-byte PTY event path,
  - feed one process tab through `vt100::Parser`,
  - render emulator-backed viewport in TUI for `nextest` validation.
