# Varlock

Status: Draft
Tool name: Varlock
Category: Environment variable management (schema-driven secrets)
Owner:
Last updated: 2026-03-07
Scope: Varlock .env.schema, validation, secret resolution, env-spec

## 1) Why this tool matters

Varlock is a modern approach to environment variable management that adds schema validation and secure resolution to traditional .env files. It's notable for:
- Schema-driven configuration with @decorator syntax
- Type safety and validation without custom logic
- Secure secret resolution from external providers
- Redaction of sensitive values in stdout/console

For Effigy, Varlock represents:
- A potential integration or alternative for environment management
- Schema validation patterns for configuration
- Secret provider abstraction
- Developer experience improvements over raw .env

## 2) Product and era context

### Timeline

- **2025**: Varlock launched by DMNO team (evolution of DMNO)
- **2025**: Active development, RFC for @env-spec open

### Design Philosophy

From varlock documentation:

> "Magic .env files built for sharing"
> "Human-first, AI-friendly"
> "Unlike .env.example, your .env.schema is a single source of truth"

### Target Audience

- JavaScript/TypeScript developers (primary)
- Teams managing multiple environment configurations
- Developers wanting validation without runtime overhead

### Ecosystem

- **@env-spec**: Open specification for environment variable schemas
- **Integrations**: 1Password, other secret providers
- **Coming soon**: Local encryption via biometrics, shared team vaults

## 3) Defining architectural bets

### Schema-in-comments approach

Varlock uses decorators in comments rather than separate config files:

```bash
# .env.schema
# @sensitive @required @type=string(startsWith=sk-)
OPENAI_API_KEY=

# @type=enum(development, preview, production, test)
APP_ENV=development

# use function calls to securely fetch data from external sources
XYZ_TOKEN=exec('op read "op://api-prod/xyz/auth-token"')
```

Benefits:
- Familiar .env syntax
- Schema co-located with values
- Human readable

### Type system

Built-in types with constraints:
- `@type=string(minLength=10, startsWith=sk-)`
- `@type=number(min=0, max=100)`
- `@type=enum(dev, staging, prod)`
- `@type=url(https)`

Validation happens at load time, not runtime.

### Secure resolution

Secrets fetched from external providers:
```bash
# Execute command to fetch secret
XYZ_TOKEN=exec('op read "op://vault/item/field"')

# Built-in providers coming soon
XYZ_TOKEN=1password("api-prod/xyz/auth-token")
```

### Redaction

Sensitive values automatically redacted:
- Console methods (console.log)
- stdout/stderr
- Bundled client code detection
- Outgoing server response detection

## 4) Standout strengths

- **Familiar syntax**: .env files with added benefits
- **Validation**: Catch misconfiguration early
- **Type generation**: Auto-generate TypeScript types
- **Security**: Redaction prevents accidental leaks
- **External providers**: Integrate with 1Password, etc.
- **Open spec**: @env-spec for ecosystem compatibility

## 5) Chronic weaknesses and recurring costs

### JavaScript-centric

Primary focus on JS/TS ecosystems:
- Node.js auto-load mechanism
- TypeScript type generation
- npm distribution

Limited support for other languages.

### New and evolving

Early stage project:
- API may change
- Features still being added
- Limited production usage history

### External dependency

For secure resolution:
- Requires 1Password CLI or similar
- Additional setup complexity
- Provider lock-in concerns

## 6) Between-release corrections

Varlock is new (2025), evolving from DMNO:
- DMNO required TypeScript for schemas
- Varlock uses simpler comment-based DSL
- Migration path from DMNO documented

## 7) Effigy-relevant lessons

### Adopt carefully

- **Schema validation**: Type constraints on environment variables
- **External provider integration**: 1Password, Bitwarden, etc.
- **Redaction**: Prevent accidental secret exposure
- **Open specification**: @env-spec for interoperability

### Reject early

- **JavaScript-specific features**: Keep language-agnostic
- **Comment-based schemas**: TOML is more structured
- **Tight coupling to external tools**: Maintain flexibility

### Prototype before deciding

- Varlock integration as optional feature
- Schema validation for effigy.toml
- Secret provider abstraction

## 8: Effigy Integration Options

### Option 1: Native varlock support

```toml
# effigy.toml
[env]
provider = "varlock"
schema = ".env.schema"
```

Effigy reads varlock schema and loads validated environment.

### Option 2: Built-in schema validation

```toml
# effigy.toml
[env]
[env.schema.OPENAI_API_KEY]
required = true
sensitive = true
type = "string"
pattern = "^sk-"

[env.schema.APP_ENV]
type = "enum"
values = ["dev", "staging", "prod"]
default = "dev"
```

Native Effigy implementation inspired by varlock.

### Option 3: Secret resolution

```toml
# effigy.toml
[env]
DATABASE_URL = { from = "1password", path = "prod/db/connection" }
API_KEY = { from = "bitwarden", path = "api/key" }
```

Effigy resolves secrets from providers at runtime.

## 9: Source inventory

| Source | Type | Version/Era | Confidence | Notes |
|--------|------|-------------|------------|-------|
| [varlock.dev](https://varlock.dev) | official docs | current | high | Primary reference |
| [GitHub dmno-dev/varlock](https://github.com/dmno-dev/varlock) | source | current | high | Implementation |
| [1Password Community](https://www.1password.community) | community | 2025 | medium | Announcement |
| [@env-spec RFC](https://github.com/dmno-dev/varlock/discussions/17) | RFC | current | medium | Specification |

## 10: Open questions

- How will varlock's encryption features (coming soon) work?
- What's the performance overhead of schema validation?
- How does varlock handle secret rotation?

## Next Task

Compare against SOPS, git-crypt, and other secrets management tools in Track 16 synthesis.

