# Effigy Agent Skills

This directory ships **agent skills** — portable prompts that teach AI coding
assistants how to use Effigy.

## What's here

- [`effigy/`](./effigy/) — main skill. Agent operating loop, graph assist,
  built-in surface lookup, JSON envelopes, config shapes, footguns, release
  protocol. Light front door ([`SKILL.md`](./effigy/SKILL.md)) routes to topic
  references in [`effigy/references/`](./effigy/references/). Task names such as
  **`dev`** are repo-defined unless the doc names a built-in (`test`, `init`, …).

## Install

The skill follows the open [Agent Skills](https://agentskills.io/specification)
standard. It works in **Claude Code, OpenAI Codex, Cursor**, and any other
agent that consumes `SKILL.md`.

### Recommended: `npx skills`

```bash
# Project-local install (from a checkout)
npx skills add /path/to/effigy/skills/effigy

# Published source (global)
npx skills add inflatable-cookie/effigy -g
```

`npx skills` auto-detects the calling agent and installs to the right
location:

- Claude Code: `.claude/skills/` or `~/.claude/skills/`
- Codex CLI: `.agents/skills/` or `~/.agents/skills/`
- Cursor: `.cursor/skills/` or `~/.cursor/skills/`

The CLI ships from [`vercel-labs/skills`](https://github.com/vercel-labs/skills)
and supports 50+ agents.

### Manual install

If your agent isn't covered by `npx skills`:

```bash
# Claude Code
mkdir -p ~/.claude/skills
cp -r skills/effigy ~/.claude/skills/effigy

# Codex CLI
mkdir -p ~/.agents/skills
cp -r skills/effigy ~/.agents/skills/effigy

# Cursor
mkdir -p ~/.cursor/skills
cp -r skills/effigy ~/.cursor/skills/effigy
```

Replace `~` with `.` for project-local installs.

## Activation

After install, the skill activates when the user mentions Effigy, runs
`effigy <command>`, edits `effigy.toml`, or invokes `/effigy` (in agents
that support slash-prefix skills).

## Layout

```
skills/
└── effigy/
    ├── SKILL.md                         # front door
    └── references/
        ├── agent-operating-loop.md      # default agent sequence
        ├── built-in-surfaces.md         # built-in command lookup
        ├── graph-assist.md              # code graph workflow
        ├── footguns.md
        ├── first-five-commands.md       # discovery loop detail
        ├── selector-routing.md
        ├── workflow-shortcuts.md
        ├── json-envelope.md
        ├── config-shapes.md
        └── release-protocol.md
```

The front door stays short. Topic references hold depth and are read on demand.

## Maintenance

Update the skill in the same PR as command or JSON contract changes. Validate
with `effigy qa:docs` (links, JSON examples, agent-defaults checks).

Reinstall after local edits:

```bash
npx skills add /path/to/effigy/skills/effigy -g
```
