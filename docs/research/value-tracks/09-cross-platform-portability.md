# Track 09: Cross-Platform Portability

Status: Draft
Track: Cross-Platform Portability
Owner:
Last updated: 2026-03-07
Primary Effigy tags: `PORTABILITY`, `UX`, `ARCH`

## 1) Problem statement

How should a tool work across platforms? What balances:
- Consistency (same behavior everywhere)
- Native feel (fits each platform)
- Implementation complexity
- Testing coverage

## 2) Why this track matters to Effigy

Effigy targets:
- macOS
- Linux
- Windows

Research validates:
- Cross-platform design patterns
- Shell/portability strategies
- Path handling
- Testing approaches

## 3) Cross-tool comparison

| Tool | Cross-Platform Strategy | Shell Handling | Windows Support |
|------|------------------------|----------------|-----------------|
| Make | Minimal | /bin/sh | Poor (requires MSYS) |
| Just | Excellent | sh/PowerShell abstraction | Native |
| Task | Good (Go-based) | Cross-platform | Native |
| Deno | Excellent (Rust-based) | Built-in | Native |
| Effigy | Good (Rust-based) | User's shell | Native |

### Portability Spectrum

**Minimal (Make)**
- Assumes Unix environment
- Poor Windows support
- Requires external tools on Windows

**Abstraction (Just, Deno)**
- Hides platform differences
- Consistent behavior
- May feel less "native"

**Native (Effigy current)**
- Uses platform's native shell
- Users write platform-specific commands
- Flexible but requires care

## 4) Repeated patterns

### Universal portability needs

1. **Path handling**
   - Forward slashes vs. backslashes
   - Absolute vs. relative paths
   - Home directory expansion

2. **Shell execution**
   - Unix: /bin/sh, bash, zsh
   - Windows: cmd.exe, PowerShell
   - Cross-platform: sh emulation

3. **Environment variables**
   - Setting/getting
   - PATH manipulation
   - Platform-specific variables

4. **Process spawning**
   - Different APIs on each platform
   - Argument escaping
   - Signal handling

### Tool-specific approaches

**Just: Shell abstraction**
- Uses `sh` on all platforms (via emulation on Windows)
- Provides `os()`, `arch()` functions
- Recipes work without modification

**Deno: Single binary**
- Rust-based, compiles to native
- No runtime dependencies
- Consistent behavior everywhere

**Task: Go-based**
- Go's cross-compilation
- Built-in shell handling
- Native performance

## 5) Frontier research signals

- **WASI**: WebAssembly system interface for portability
- **Container-based tools**: Docker for consistency
- **Web-based CLIs**: Terminal in browser
- **AI translation**: Convert shell commands cross-platform

## 6) Effigy implications

### Recommended direction

**Keep current approach with improvements:**

1. **Use native shell** (current)
   - Users write commands in their platform's shell
   - No abstraction layer
   - Maximum flexibility

2. **Path helpers**
   ```toml
   [tasks.build]
   # Effigy provides path normalization
   run = "build {project}/src"
   ```

3. **Cross-platform task examples**
   ```toml
   [tasks.build]
   run = [
       { if = "windows", run = "build.bat" },
       { if = "unix", run = "./build.sh" }
   ]
   ```

4. **Clear documentation**
   - Document platform differences
   - Provide cross-platform recipes
   - Test on all platforms

### Risks to avoid

1. **Shell abstraction complexity**: Don't try to be Just
2. **Assuming Unix**: Test on Windows
3. **Ignoring PowerShell**: Windows users expect it

### Evidence or prototype needed

- [ ] Windows CI testing
- [ ] Path handling edge cases
- [ ] Cross-platform recipe examples

## 7) Implementation suggestions

### Platform detection

```rust
pub enum Platform {
    MacOS,
    Linux,
    Windows,
}

impl Platform {
    pub fn current() -> Self {
        #[cfg(target_os = "macos")] return Platform::MacOS;
        #[cfg(target_os = "linux")] return Platform::Linux;
        #[cfg(target_os = "windows")] return Platform::Windows;
    }
}
```

### Conditional tasks

```toml
[tasks.build]
run = [
    { if = "windows", run = "cargo build --release" },
    { if = "unix", run = "cargo build" }
]
```

Or using platform-specific sections:
```toml
[tasks.build]
run = "cargo build"

[tasks.build.windows]
run = "cargo build --release"
```

### Path normalization

```rust
pub fn normalize_path(path: &str) -> PathBuf {
    // Handle both / and \
    // Expand ~ to home directory
    // Make relative to project root
}
```

## 8) Comparison: Approaches

| Approach | Pros | Cons | Effigy |
|----------|------|------|--------|
| Shell abstraction (Just) | Consistent | Less native feel | ❌ |
| Native shell (current) | Flexible | Platform-specific scripts | ✅ Keep |
| Single binary (Deno) | Easy distribution | | ✅ Already done |

## 9) Source inventory

| Source | Type | Confidence | Notes |
|--------|------|------------|-------|
| Just dossier | high | Shell abstraction |
| Deno dossier | high | Single binary, Rust |
| Task dossier | high | Go portability |
| Rust std::env docs | high | Cross-platform APIs |

## 10) Decision state

- [ ] `promote to concept work` — Document portability approach
- [ ] `continue research` — Current approach sufficient
- [ ] `prototype first` — Test Windows support

**Current leaning**: Continue with native shell approach. Add platform conditionals and path helpers. Ensure Windows CI testing.

## Next Task

1. Draft Translation Memo 009: Cross-Platform Strategy
2. Begin Track 10: Environment Management

