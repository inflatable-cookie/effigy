# direnv

Status: Draft
Tool name: direnv
Category: environment management (directory-specific env vars)
Owner:
Last updated: 2026-03-07
Scope: direnv 2.x documentation, .envrc patterns, shell hook integration

## 1) Why this tool matters

direnv is a shell extension that loads environment variables when entering a directory. It's widely used for:
- Per-project environment configuration
- Managing secrets locally
- Switching between project contexts
- Avoiding global environment pollution

For Effigy, direnv represents:
- Directory-specific environment patterns
- .env file loading strategies
- Shell integration patterns
- Environment variable precedence

## 2) Product and era context

### Timeline

- **2011**: direnv created by zimbatm
- **2014-2018**: Adoption growth, shell integration improvements
- **2019-2024**: stdlib additions, stability focus

### Design Philosophy

From direnv documentation:

> "Unclutter your .profile"
> "Load environment variables depending on the current directory"
> "Clean environment when leaving directory"

### Target Audience

- Developers with multiple projects
- Teams needing project-specific configuration
- Users managing secrets locally
- DevOps engineers

### How It Works

1. User enters directory with `.envrc` file
2. direnv detects file and prompts for allow (security)
3. On allow, direnv loads variables into shell
4. When leaving directory, variables are unloaded

## 3) Defining architectural bets

### .envrc files

direnv uses `.envrc` (shell script):

```bash
# .envrc
export API_KEY="secret123"
export DATABASE_URL="postgres://localhost/myapp"
PATH_add ./bin
```

This is executable shell code, enabling:
- Variable exports
- PATH manipulation
- Conditional logic
- Calling external tools

### Shell hook integration

direnv integrates via shell hook:

```bash
# ~/.bashrc or ~/.zshrc
eval "$(direnv hook bash)"
```

The hook runs before every prompt, checking for `.envrc` changes.

### Security model

direnv requires explicit allow:
```bash
$ cd myproject
direnv: error .envrc is blocked. Run `direnv allow` to approve its content

$ direnv allow
direnv: loading .envrc
direnv: export +API_KEY +DATABASE_URL ~PATH
```

This prevents accidental execution of untrusted code.

### stdlib functions

direnv provides helper functions:
```bash
# Add to PATH
PATH_add ./node_modules/.bin

# Load .env file
dotenv

# Load .env with specific name
dotenv .env.local

# Set layout (e.g., for Python)
layout_python
```

## 4) Standout strengths

- **Automatic loading**: No manual sourcing
- **Automatic unloading**: Clean environment when leaving
- **Security**: Explicit allow required
- **Shell integration**: Works in bash, zsh, fish
- **Stdlib**: Helper functions for common patterns
- **Widely adopted**: Standard tool in many teams

## 5) Chronic weaknesses and recurring costs

### Shell hook overhead

direnv runs before every prompt:
- Checks for .envrc changes
- Minimal but non-zero overhead
- Can be noticeable in large directories

### Security prompts

New/ changed `.envrc` files require `direnv allow`:
- Good for security
- Can be annoying during active development
- Easy to accidentally ignore changes

### Not project-configurable

direnv is user-side:
- Each user must install direnv
- Each user must run `direnv allow`
- Can't enforce for team

### No Windows native support

direnv is Unix-focused:
- WSL support
- No native Windows
- PowerShell support limited

## 6) Between-release corrections

### Early versions (2011-2014)
- Basic .envrc loading
- bash/zsh support

### Modern direnv (2015-2024)
- stdlib additions
- Fish shell support
- Better performance
- Layout functions

The pattern: Maturing toward stability and broader shell support.

## 7) Effigy-relevant lessons

### Adopt carefully

- **Automatic .env loading**: Load .env files by default
- **Security**: Consider allow-listing or prompts
- **Shell integration**: Understand how shells load env
- **Precedence**: Define env loading order

### Reject early

- **Shell hook requirement**: Don't require shell modifications
- **External dependency**: Don't require direnv installation
- **Implicit execution**: Don't auto-execute scripts

### Prototype before deciding

- Effigy's env loading vs. direnv integration
- Security model for env files
- Precedence with process env

## 8) Comparison: direnv vs. Effigy env handling

| Aspect | direnv | Effigy |
|--------|--------|--------|
| Trigger | Directory change | Task execution |
| Scope | Shell-wide | Task-specific |
| .env support | Via stdlib | Native |
| Security | Explicit allow | Manifest-controlled |
| Dependency | Requires direnv | Built-in |

**Pattern**: direnv is shell-level, Effigy is task-level. Different use cases.

## 9) Effigy Integration Ideas

### Option 1: Learn from direnv

Implement similar patterns in Effigy:
```toml
[env]
DATABASE_URL = "postgres://localhost/myapp"

[tasks.dev.env]
DEBUG = "true"
```

### Option 2: direnv compatibility

Read `.envrc` if present:
```bash
# .envrc
export API_KEY="secret"
```

```toml
# effigy.toml
[env]
# Inherits from .envrc if available
# Or explicitly reference
env_file = ".envrc"
```

### Option 3: Complementary tools

Use both:
- direnv for shell-wide project settings
- Effigy for task-specific environment

## 10) Source inventory

| Source | Type | Version/Era | Confidence | Notes |
|--------|------|-------------|------------|-------|
| [direnv docs](https://direnv.net/) | official docs | current | high | Primary reference |
| [direnv stdlib](https://direnv.net/man/direnv-stdlib.1.html) | official docs | current | high | Helper functions |
| [direnv GitHub](https://github.com/direnv/direnv) | source | current | high | Implementation |
| Community guides | blog | various | medium | Usage patterns |

## 11) Open questions

- How often do users allow without reviewing .envrc changes?
- What's the performance impact of the shell hook?
- How do teams handle direnv adoption?

## Next Task

Compare against 1Password CLI and other tools in Track 10 synthesis on environment management.

