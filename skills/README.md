# Effigy Agent Skills

This directory ships **agent skills** — portable prompts that teach AI coding
assistants how to use Effigy.

## What's here

- [`effigy/`](./effigy/) — main skill. Discovery, code-understanding with
  graph, common workflows, JSON envelopes, config shapes, footguns, release
  protocol. Light front door ([`SKILL.md`](./effigy/SKILL.md)) routes to topic references in
  [`effigy/references/`](./effigy/references/). Task names such as **`dev`** are
  repo-defined unless the doc names a built-in (`test`, `init`, …).

## Install

The skill follows the open [Agent Skills](https://agentskills.io/specification)
standard. It works in **Claude Code, OpenAI Codex, Cursor**, and any other
agent that consumes `SKILL.md`.

### Recommended: `npx skills`

```bash
# Project-local install
npx skills add inflatable-cookie/effigy

# Global install (available in every repo)
npx skills add inflatable-cookie/effigy -g
```

`npx skills` auto-detects the calling agent and installs to the right
location:

- Claude Code: `.claude/skills/` or `~/.claude/skills/`
- Codex CLI: `.agents/skills/` or `~/.agents/skills/`
- Cursor: `.cursor/skills/` or `~/.cursor/skills/`

The CLI ships from [`vercel-labs/skills`](https://github.com/vercel-labs/skills)
and supports 50+ agents.

When both a project-local and global Effigy skill are installed, treat the
project-local copy as authoritative for that repo. The global install is the
fallback for repos that do not vendor Effigy locally.

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
    ├── SKILL.md                    # front door (~150 lines)
    └── references/
        ├── footguns.md
        ├── first-five-commands.md
        ├── selector-routing.md
        ├── workflow-shortcuts.md
        ├── json-envelope.md
        ├── config-shapes.md
        └── release-protocol.md
```

The front door is intentionally short. Topic references hold depth and are
read on demand by the agent.

## Maintenance

The skill version-locks to the Effigy release that ships it. When command
names or flags change, the skill content updates in the same PR.

The skill is part of `effigy qa:docs` — docs QA flags stale references and
broken links.
