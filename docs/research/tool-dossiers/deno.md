# Deno

Status: Draft
Tool name: Deno
Category: JavaScript/TypeScript runtime (cross-platform patterns)
Owner:
Last updated: 2026-03-07
Scope: Deno 1.x CLI patterns, cross-platform portability, modern runtime design

## 1) Why this tool matters

Deno is a modern JavaScript/TypeScript runtime created by Ryan Dahl (creator of Node.js). It's notable for:
- First-class TypeScript support
- Security-first design (permissions model)
- Single executable
- Cross-platform by design

For Effigy, Deno represents:
- Modern cross-platform CLI patterns
- Single-binary distribution
- Permission-based security model
- Tooling integration patterns

## 2) Product and era context

### Timeline

- **2018**: Deno announced
- **2020**: Deno 1.0 released
- **2021-2023**: Ecosystem growth, npm compatibility
- **2024**: Deno 2.0, major adoption push

### Design Philosophy

From Deno documentation:

> "Secure by default"
> "TypeScript out of the box"
> "Single executable"
> "Modern standard library"

### Target Audience

- JavaScript/TypeScript developers
- Teams wanting modern tooling
- Security-conscious projects
- Edge/serverless computing

### Positioning

Deno vs. Node.js:
- No node_modules (uses URL imports or npm: specifiers)
- Built-in TypeScript
- Permission-based security
- Standard library included

## 3) Defining architectural bets

### Single executable

Deno ships as a single binary:
```bash
# One file, no dependencies
deno --version
```

This simplifies:
- Installation (download and run)
- Distribution
- Version management

### Permission-based security

Deno requires explicit permissions:
```bash
# Allow network access
deno run --allow-net app.ts

# Allow file system read
deno run --allow-read app.ts

# Allow all (dangerous)
deno run -A app.ts
```

This is "secure by default" — without flags, scripts can't access network or filesystem.

### Cross-platform by design

Deno works identically on:
- macOS
- Linux
- Windows

Implementation uses Rust (same as Effigy), providing native performance on all platforms.

### First-class TypeScript

TypeScript works without configuration:
```typescript
// hello.ts
function greet(name: string): string {
    return `Hello, ${name}!`;
}

console.log(greet("Deno"));
```

```bash
deno run hello.ts  # Just works
```

No tsconfig.json, no build step.

### Standard library

Deno includes a standard library:
```typescript
import { serve } from "https://deno.land/std@0.200.0/http/server.ts";
```

This provides:
- Consistent APIs
- Versioned dependencies
- No external package manager needed

## 4) Standout strengths

- **Single binary**: Easy distribution
- **Cross-platform**: Works everywhere
- **Secure by default**: Permission model
- **TypeScript native**: No build step
- **Modern APIs**: Fetch, ES modules, top-level await
- **Built-in tooling**: Formatter, linter, test runner
- **Fast**: Rust-based, V8 engine

## 5) Chronic weaknesses and recurring costs

### Node.js compatibility

Deno initially ignored npm ecosystem:
- Required code changes to migrate
- Limited package availability

Deno 2.0 improved this with npm: specifiers, but still not 100% compatible.

### Permission friction

Security requires explicit flags:
```bash
# Without --allow-net, network calls fail
deno run app.ts  # Error: network access denied
```

Good for security, but can be annoying for development.

### Smaller ecosystem

Compared to Node.js:
- Fewer tutorials
- Smaller community
- Less Stack Overflow coverage

## 6) Between-release corrections

### Deno 1.0 → 2.0

- npm: compatibility added
- Node.js APIs polyfills
- Standard library stabilization

The pattern: Balancing innovation with ecosystem compatibility.

## 7) Effigy-relevant lessons

### Adopt carefully

- **Single binary**: Easy distribution
- **Cross-platform from day one**: Design for all platforms
- **Built-in tooling**: Reduce external dependencies
- **Permission model**: Consider for script execution

### Reject early

- **URL imports**: Security concerns for task runner
- **No package manager**: Different problem domain
- **Permission friction**: May be too strict for task runner

### Prototype before deciding

- Single-binary distribution for Effigy
- Permission model for task execution
- Cross-platform testing automation

## 8) Comparison: Deno vs. Node.js vs. Effigy

| Aspect | Deno | Node.js | Effigy |
|--------|------|---------|--------|
| Distribution | Single binary | Runtime + npm | Single binary |
| Cross-platform | Native | Native | Native (Rust) |
| TypeScript | Native | Requires build | Native (config) |
| Security | Permission-based | Unrestricted | User's shell |
| Ecosystem | Growing | Mature | Tooling integration |

**Pattern**: Single-binary, cross-platform tools provide best UX.

## 9) Cross-Platform Lessons for Effigy

### What Deno does well

1. **Single binary**: Download and run
2. **Consistent behavior**: Same on all platforms
3. **Native performance**: Rust-based
4. **Built-in help**: `deno --help` is comprehensive

### Effigy application

```bash
# Distribution
effigy --version  # Single binary

# Cross-platform consistency
effigy run  # Same on macOS, Linux, Windows

# Built-in help
effigy --help
effigy help <command>
```

## 10) Source inventory

| Source | Type | Version/Era | Confidence | Notes |
|--------|------|-------------|------------|-------|
| [Deno docs](https://docs.deno.com/) | official docs | current | high | Primary reference |
| [Deno manual](https://deno.land/manual) | official docs | current | high | Comprehensive |
| [Deno blog](https://deno.com/blog) | blog | ongoing | high | Updates |
| GitHub issues/discussions | community | ongoing | medium | Usage patterns |

## 11) Open questions

- How does Deno handle platform-specific APIs?
- What's the performance cost of cross-platform abstraction?
- How do users handle permission flags in practice?

## Next Task

Compare against Just and other tools in Track 09 synthesis on cross-platform patterns.

