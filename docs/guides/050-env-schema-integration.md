# 050 - Environment Schema Integration

This guide explains the `@env-spec` integration that lets projects declare
environment variable schemas in `.env.schema` files. Effigy parses, resolves,
validates, and injects these variables at task execution time.

This is native Effigy env-schema support, not a live Varlock backend. Varlock
inspired the original env-spec work, but Varlock is deferred for `g05`. The
supported Effigy local secret path is `[secrets]` plus the built-in vault.

Use it when your project needs:
- declarative required/optional environment variables
- type-checked values (port, URL, enum, string length)
- compatibility secret handling for existing task env that never leaks to `ps`
  output
- computed defaults via shell commands or variable references

Do not use `.env.schema` as the long-term source of truth for true secrets in
new Effigy-managed projects. Declare true secrets under `[secrets.keys]` and
store local values with `effigy admin secrets`.

## Vision Alignment

- Primary tags: `OPERATE`, `MAINT`
- Target movement: environment configuration moves from implicit runtime
  assumptions toward declarative, validated, schema-driven contracts.

## 1) Schema File Format

Place a `.env.schema` file in your catalog root (next to `effigy.toml`).

### Basic syntax

```
# Comment lines describe the variable below.
# @type=port @required
PORT=3000

# @sensitive
DATABASE_PASSWORD=

# @type=url
API_ENDPOINT=https://api.example.com

# @type=enum(development,staging,production)
NODE_ENV=development
```

Each entry is a `KEY=VALUE` line. Comments immediately above a key are parsed
for annotations. Blank lines reset the annotation buffer.

### Annotations

| Annotation | Effect |
|---|---|
| `@required` | Error if no value is available from any source |
| `@optional` | Explicitly mark as optional (default behavior) |
| `@sensitive` | Value is treated as a secret: zeroized on drop, redacted in logs, injected via `Command::env()` instead of shell wrapping |
| `@type=port` | Validate value is an integer in 1-65535 |
| `@type=url` | Validate value contains `://` |
| `@type=string` | String type |
| `@min=3` | Minimum length for string values (implies `string` if no type is set) |
| `@max=64` | Maximum length for string values (implies `string` if no type is set) |
| `@pattern=^https://` | Regex the resolved value must match |
| `@type=enum(a,b,c)` | Validate value is one of the listed variants |

Annotations appear in comment lines prefixed with `#`. Multiple annotations
can appear on one line:

```
# Database connection @type=url @required @sensitive
DATABASE_URL=
```

Text before the first `@` becomes the variable description.

### Value expressions

The right side of `=` supports several expression forms:

| Form | Example | Behavior |
|---|---|---|
| Literal | `PORT=3000` | Static default value |
| Empty | `API_KEY=` | No default; must come from process env or `.env` |
| `exec('cmd')` | `HOST=exec('hostname')` | Run shell command, use trimmed stdout |
| `env('VAR')` | `PORT=env('BASE_PORT')` | Reference another schema variable |
| `${VAR}` template | `URL=https://${HOST}:${PORT}` | String interpolation |

`exec()` commands run with `sh -c` in the project root, with a configurable
timeout (default 30 seconds).

## 2) Resolution Priority

When resolving a variable, Effigy checks sources in this order (highest wins):

1. **Process environment** (`std::env::var`) -- always takes priority
2. **`.env` file overrides** -- parsed from `.env` in the catalog root
3. **Schema defaults** -- the value expression from `.env.schema`

This means a `PORT=3000` in the schema is the fallback; if `PORT=8080` is in
`.env` or the process environment, that value is used instead.

### Dependency resolution

Variables that reference other schema variables via `env('VAR')` or `${VAR}`
are topologically sorted before resolution. Circular dependencies are detected
and produce a clear error.

## 3) Secret Handling

Variables annotated with `@sensitive` receive special treatment:

- Values are stored in a `SecretString` type that zeroizes memory on drop
- `Display` prints `[REDACTED]`; `Debug` prints `SecretString(****)`
- Secrets are injected via `Command::env(key, value)` at process spawn time,
  so they never appear in the shell command string or `ps` output
- Non-sensitive values continue to use the existing `env KEY=VALUE sh -c "..."`
  wrapping

This dual-injection strategy ensures secrets are available to the task process
but invisible to system monitoring tools.

For new local secret workflows, prefer the dedicated secrets surface:

```toml
[secrets]
backend = "effigy-vault"

[secrets.keys.database_url]
required = true
targets = ["tasks", "containers"]
```

`.env.schema @sensitive` remains supported for compatibility, validation, and
existing task environments. It is not connected to a Varlock adapter in `g05`.

## 4) Configuration

Add an `[env_schema]` section to `effigy.toml` to customize behavior:

```toml
[env_schema]
enabled = true              # true/false/omit for auto-detect
schema = ".env.schema"      # custom path (relative to catalog root)
exec_timeout = 30           # seconds for exec() commands
```

| Field | Default | Behavior |
|---|---|---|
| `enabled` | auto-detect | `true`: error if schema file missing. `false`: skip entirely. Omit: use schema if file exists. |
| `schema` | `.env.schema` | Path to the schema file relative to catalog root |
| `exec_timeout` | `30` | Timeout in seconds for `exec()` value expressions; must be at least `1` |

When `[env_schema]` is omitted entirely, Effigy auto-detects: if
`.env.schema` exists in the catalog root, it is loaded and resolved. If not,
task execution proceeds without env-schema.

Configuration guardrails:
- `schema` cannot be empty or whitespace-only
- `exec_timeout` must be at least `1`

### One-off schema override

For ad hoc runs, override the schema path at invocation time:

```bash
effigy serve --env-schema config/staging.env.schema
```

`--env-schema <PATH>` is resolved relative to the selected catalog root unless
you pass an absolute path. This runtime override takes precedence over
`[env_schema].schema` in `effigy.toml`.

## 5) Type Validation

After resolution, values are validated against their declared types:

- **`@type=port`** -- must parse as an integer in 1-65535
- **`@type=url`** -- must contain `://`
- **`@type=enum(a,b,c)`** -- must be one of the listed variants
- **`@type=string`** -- may also use `@min=<N>` / `@max=<N>` length constraints
- **`@pattern=...`** -- regex that the resolved value must match

Validation errors include the variable name, expected type, actual value, and
the line number from the schema file.

For variables marked `@sensitive`, validation output redacts the actual value
as `[REDACTED]` instead of echoing the secret back in errors.
That same redaction carries through Effigy's JSON command envelopes and
resolved-env debug formatting, so secret values do not leak through normal
task failure reporting paths.

## 6) Interaction with Task Env

Env-schema values merge with task-level `env` declarations from `effigy.toml`.
Task-level declarations take priority:

```toml
[tasks.serve]
run = "node server.js"
env = { PORT = "9090" }  # overrides PORT from .env.schema
```

Resolution order for the final task environment:

1. Task-level `env` from `effigy.toml` (highest priority)
2. Env-schema plain values
3. Process environment variables passed through

Sensitive values from env-schema are never merged into the shell command --
they are injected separately via `Command::env()`.

Run-array `env = "NAME"` directives, task-ref expansions, and configured
built-in test suite env resolution also consult resolved env-schema values
before falling back to raw dotenv-only lookup.

## 7) Example Workflow

Given this `.env.schema`:

```
# Application port @type=port
PORT=3000

# Database connection @type=url @required @sensitive
DATABASE_URL=

# Runtime mode @type=enum(development,staging,production)
NODE_ENV=development

# Computed hostname
HOSTNAME=exec('hostname')

# Full API URL
API_BASE=https://${HOSTNAME}:${PORT}/api
```

And this `.env`:

```
DATABASE_URL=postgres://localhost:5432/mydb
```

Running `effigy run serve` will:

1. Parse the schema (5 entries, 1 sensitive)
2. Resolve values: `PORT=3000` (schema default), `DATABASE_URL` (from `.env`,
   marked secret), `NODE_ENV=development` (schema default),
   `HOSTNAME` (exec), `API_BASE` (template interpolation)
3. Validate types (port range, URL format, enum membership)
4. Inject `PORT`, `NODE_ENV`, `HOSTNAME`, `API_BASE` via shell wrapping
5. Inject `DATABASE_URL` via `Command::env()` (secret)

## Expected Outcome

After reading this guide you can:
- Write `.env.schema` files with annotations and value expressions
- Configure `[env_schema]` in `effigy.toml`
- Understand the resolution priority and secret injection model
- Debug validation errors using the schema line numbers in error messages

## Library API

If you use Effigy as a Rust library, the env-schema module exposes a cohesive
public surface:

```rust
use effigy::env_schema;

let schema = env_schema::load_env_schema(path)?;
let resolved = env_schema::resolve_env(&schema, &context)?;
let errors = env_schema::validate_env(&schema, &resolved);
let exported = resolved.env_values(); // HashMap<String, env_schema::EnvValue>
```

`load_env_schema_if_present(project_root)` auto-detects `.env.schema` in a
project root and returns `Ok(None)` when the default file is absent.

## Related Guides

- [`022-manifest-cookbook.md`](./022-manifest-cookbook.md) -- task env patterns
- [`048-built-in-test-suite-lifecycle-and-env.md`](./048-built-in-test-suite-lifecycle-and-env.md) -- test suite env
- [`025-command-reference-matrix.md`](./025-command-reference-matrix.md) -- `effigy admin secrets` command surface
- [`075-secrets-and-vault-guide.md`](./075-secrets-and-vault-guide.md) -- dedicated secret management and vault workflow

## Next Step

Create a `.env.schema` file in your project root and run a task. Effigy
auto-detects the schema file -- no configuration needed to get started.

For true secrets, declare them under `[secrets.keys]` and store values with
`effigy admin secrets set` instead of `.env.schema`.
