# Translation Memo 016c: Varlock Integration Strategy

**Status:** Draft  
**Track:** 16 - Secure Secrets Management (Final Refinement)  
**Focus:** Varlock as External Dependency  
**Date:** 2026-03-07  
**Related:** Translation Memo 016b (External Provider Secrets)

## Refined Stance

After deeper research into Varlock's architecture, the recommendation is refined:

> **Effigy should integrate Varlock as an external dependency rather than reimplementing its functionality.**

Varlock has already solved the hard problems:
- Schema validation with @env-spec DSL
- External provider resolution (1Password, etc.)
- Secret redaction
- Multi-environment composition
- Type coercion and validation

## Why Not Build Our Own?

| Feature | Effigy Building | Varlock Already Has |
|---------|----------------|---------------------|
| Schema parser | New DSL to design | @env-spec mature DSL |
| Validation engine | Complex type system | Built-in validators |
| Provider resolution | Integration work | `exec()` function support |
| Secret redaction | Hook into logging | Console method patching |
| Multi-env composition | Design + implement | `.env`, `.env.local`, etc. |
| IDE support | LSP to build | VSCode extension exists |

**Key insight:** Varlock's `.env.schema` is designed to live *alongside* code, not in a central config file. This is actually better - schema is colocated with the code that uses it.

## Varlock Architecture

### File Structure

```
project/
├── .env.schema          # Schema + defaults (committed)
├── .env                 # Local values (gitignored)
├── .env.production      # Environment overrides (optional)
└── .env.local           # Local overrides (gitignored)
```

### Schema Example (.env.schema)

```bash
# @defaultSensitive=true @defaultRequired=infer
# ---

# API Configuration
# @type=port @default=3000
PORT=3000

# @type=url @required
DATABASE_URL=

# External secret from 1Password
# @type=string @sensitive
STRIPE_KEY=exec('op read "op://Production/Stripe/live-key"')

# Enum validation
# @type=enum(development, production, test)
NODE_ENV=development
```

### CLI Interface

```bash
# Validate schema
varlock load

# Run with resolved env
varlock run -- npm start

# Check specific env
varlock load --env=production
```

### Key Features

1. **Validation**: Type checking, required fields, custom validators
2. **Resolution**: `exec()` function runs commands to fetch secrets
3. **Redaction**: Sensitive values masked in output
4. **Composition**: Multiple .env files merged
5. **IntelliSense**: VSCode extension for autocomplete

## Effigy Integration Options

### Option 1: Thin Wrapper (Recommended)

Effigy spawns varlock as child process:

```rust
// Effigy task execution
pub fn run_task_with_varlock(cmd: &str, env_file: Option<&str>) -> Result<()> {
    // Check if .env.schema exists
    if Path::new(".env.schema").exists() {
        // Use varlock to run
        let mut child = Command::new("varlock")
            .args(["run", "--"])
            .arg(cmd)
            .spawn()?;
        
        child.wait()?;
    } else {
        // Fall back to regular execution
        run_task_directly(cmd)?;
    }
}
```

```toml
# effigy.toml - minimal varlock config
[env]
provider = "varlock"  # Enable varlock integration
schema = ".env.schema"  # Optional: specify path
```

**Pros:**
- Zero reimplementation
- Varlock handles all complexity
- Gets all varlock features for free

**Cons:**
- Requires varlock CLI installed
- Spawn overhead (minimal)

### Option 2: Library Integration (If Available)

If Varlock exposes a Rust library:

```rust
use varlock_rs::{load_env, run_with_env};

fn run_task(cmd: &str) -> Result<()> {
    let env = load_env(".env.schema")?;
    run_with_env(cmd, &env)?;
}
```

**Status:** Unclear if Varlock has a Rust library - research needed.

### Option 3: @env-spec Parser Only

Use @env-spec parser, implement resolution ourselves:

```rust
use env_spec::parse_schema;

fn load_env_schema(path: &str) -> Result<EnvConfig> {
    let schema = parse_schema(path)?;
    // Implement resolution, validation, redaction
}
```

**Pros:**
- Embedded in Effigy binary
- No external dependency

**Cons:**
- Reimplementing varlock features
- Maintenance burden

## Recommended: Option 1 (Thin Wrapper)

### User Experience

```bash
# 1. Install varlock (one time)
curl -sSfL https://varlock.dev/install.sh | sh

# 2. Create schema
cat > .env.schema << 'EOF'
# @type=port @default=3000
PORT=3000

# @type=url @required
DATABASE_URL=

# @sensitive @type=string
API_KEY=exec('op read "op://Dev/API/key"')
EOF

# 3. Run effigy task
effigy run dev
# [effigy] Detected .env.schema, using varlock
# [varlock] ✓ Schema validated
# [varlock] ✓ 3 secrets resolved
# [dev] Starting server on port 3000...
```

### Implementation

```rust
// In Effigy task runner

pub struct VarlockIntegration {
    schema_path: PathBuf,
}

impl VarlockIntegration {
    pub fn detect() -> Option<Self> {
        // Look for .env.schema in project root
        let schema = find_upward(".env.schema")?;
        Some(Self { schema_path: schema })
    }
    
    pub fn run(&self, cmd: &[&str]) -> Result<()> {
        // Validate first
        let validate = Command::new("varlock")
            .arg("load")
            .arg("--quiet")
            .status()?;
        
        if !validate.success() {
            return Err("Varlock validation failed".into());
        }
        
        // Run with varlock
        let mut child = Command::new("varlock")
            .arg("run")
            .arg("--")
            .args(cmd)
            .spawn()?;
        
        child.wait()?;
        Ok(())
    }
}

// In task execution
fn execute_task(task: &Task) -> Result<()> {
    // Check if varlock schema exists
    if let Some(varlock) = VarlockIntegration::detect() {
        println!("[effigy] Using varlock for environment management");
        return varlock.run(&task.command);
    }
    
    // Fall back to direct execution
    execute_directly(task)
}
```

### Configuration in effigy.toml

```toml
# effigy.toml
[env]
# Enable varlock integration (auto-detected by default)
varlock = true

# Optional: specify schema path if not .env.schema
# schema = "config/.env.schema"

# Optional: specify environment
# environment = "production"
```

## Integration Points

### 1. Task Execution

```rust
// Before running task, check for varlock
if varlock_detected() {
    wrap_with_varlock(cmd)
} else {
    run_directly(cmd)
}
```

### 2. Validation Command

```bash
# Add effigy command to validate env
effigy env validate
# Runs: varlock load
```

### 3. Secret Resolution

Varlock handles this via `exec()` in schema:

```bash
# .env.schema
STRIPE_KEY=exec('op read "op://Prod/Stripe/key"')
```

No effigy config needed!

### 4. CI/CD Integration

```yaml
# GitHub Actions
- name: Install varlock
  run: curl -sSfL https://varlock.dev/install.sh | sh

- name: Run tests
  env:
    OP_SERVICE_ACCOUNT_TOKEN: ${{ secrets.OP_TOKEN }}
  run: effigy run test
```

## Migration Path

### From .env files

```bash
# 1. Generate schema from existing .env
varlock init

# 2. Add annotations to .env.schema
# Edit file, add @type, @required, etc.

# 3. Move secrets to 1Password
# Update values to use exec('op read ...')

# 4. Test
effigy run dev
```

### From effigy.toml env

```toml
# Before: env in effigy.toml
[env]
DATABASE_URL = { from = "1password", path = "..." }

# After: env in .env.schema
# DATABASE_URL=exec('op read "op://..."')
```

## Comparison: Integration vs. Native

| Aspect | Varlock Integration | Native Implementation |
|--------|---------------------|----------------------|
| Implementation effort | ~1 week | ~2 months |
| Feature completeness | 100% (all varlock) | Partial |
| Maintenance burden | Low (varlock team) | High (effigy team) |
| Binary size | No change | Larger |
| External dependency | varlock CLI | None |
| Schema format | @env-spec | TOML-based? |
| IDE support | VSCode extension | None |

## Open Questions

1. **Varlock availability:** What if varlock isn't installed?
   - Option: Bundle varlock binary with effigy
   - Option: Prompt user to install
   - Option: Graceful fallback

2. **Version compatibility:** Lock to specific varlock version?
   - Check `varlock --version` matches supported range

3. **Error handling:** Map varlock errors to effigy format?
   - Parse varlock output for structured errors

4. **Cross-platform:** Varlock supports Windows/Mac/Linux?
   - Verify platform support

## Recommended Implementation

### Phase 1: Basic Integration (1 week)

1. Detect `.env.schema` in project root
2. Spawn `varlock run -- <task>` when detected
3. Pass through exit codes and output
4. Add `--no-varlock` flag to disable

### Phase 2: Validation (3 days)

1. Add `effigy env validate` command
2. Run `varlock load --quiet` before tasks
3. Show helpful error on validation failure

### Phase 3: Configuration (3 days)

1. Add `[env]` section to effigy.toml
2. Allow disabling varlock
3. Allow custom schema path

### Phase 4: Documentation (2 days)

1. Document varlock integration
2. Provide migration guide
3. Add examples

## Summary

**Don't build what Varlock already built.**

Instead:
1. Detect when projects use Varlock (`.env.schema` exists)
2. Automatically use `varlock run` for task execution
3. Provide validation command that wraps varlock
4. Keep effigy.toml config minimal (just enable/disable)

This gives Effigy users:
- Schema validation
- External provider resolution
- Secret redaction
- Multi-environment support
- IDE integration

Without Effigy team maintaining any of it.

---

**Final recommendation: Thin wrapper around Varlock CLI.**
