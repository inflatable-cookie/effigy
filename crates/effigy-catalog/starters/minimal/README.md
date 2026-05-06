# Minimal starter

Baseline scaffold for **`effigy init`** with no name or **`effigy init minimal`**.
Emits a small root **`effigy.toml`**: a working **`ping`** task plus commented
examples for managed **`dev`** and DAG-style **`validate`**, and this **`README.md`**
when the repo does not already have one at the root (existing files are skipped
unless you pass **`--force`**).

## First commands

```sh
effigy tasks
effigy ping
effigy doctor --verbose
```

**`ping`** (and **`dev`** if you uncomment it) are **manifest tasks** — names you
define under **`[tasks]`**. Built-ins such as **`test`**, **`init`**, and
**`doctor`** ship with Effigy; see **`effigy --help`** for the full list.

## Learn more

Links target the Effigy repo so they stay valid after **`effigy init`** copies
this file into your project:

- Quick start: [`021-quick-start-and-command-cookbook.md`](https://github.com/inflatable-cookie/effigy/blob/main/docs/guides/021-quick-start-and-command-cookbook.md)
- Manifest cookbook: [`022-manifest-cookbook.md`](https://github.com/inflatable-cookie/effigy/blob/main/docs/guides/022-manifest-cookbook.md)
