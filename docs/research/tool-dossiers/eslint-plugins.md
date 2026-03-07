# ESLint (Plugin System)

Status: Draft
Tool name: ESLint
Category: JavaScript linter (plugin architecture)
Owner:
Last updated: 2026-03-07
Scope: ESLint plugin system, rule API, config extension

## 1) Why this tool matters

ESLint has one of the most successful plugin ecosystems in JavaScript. It's notable for:
- Simple plugin API (rules as functions)
- Flat config system (modern) vs. eslintrc (legacy)
- Huge ecosystem (1000+ plugins)
- Configuration extension and composition

For Effigy, ESLint represents:
- Plugin API design patterns
- Configuration extension models
- Ecosystem growth strategies
- API versioning and migration

## 2) Product and era context

### Timeline

- **2013**: ESLint initial release
- **2015**: Plugin system introduced
- **2016-2022**: Shareable configs, parser services
- **2022**: Flat config announced (eslintrc deprecated)
- **2023-2024**: Flat config becomes default

### Design Philosophy

From ESLint documentation:

> "ESLint is designed to be completely configurable"
> "Each rule is essentially a plugin"

### Target Audience

- JavaScript developers
- Teams wanting custom linting rules
- Framework authors (React, Vue, etc.)
- Tooling companies

### Ecosystem

- **Plugins**: 1000+ npm packages (`eslint-plugin-*`)
- **Configs**: Shareable configurations (`eslint-config-*`)
- **Parsers**: Custom parsers (TypeScript, Vue, etc.)
- **Presets**: Airbnb, Standard, Prettier, etc.

## 3) Defining architectural bets

### Rule-based plugins

Plugins export rules as simple functions:

```javascript
// eslint-plugin-example
module.exports = {
  rules: {
    'no-console': {
      meta: {
        type: 'suggestion',
        docs: {
          description: 'Disallow console statements'
        },
        schema: [] // Configuration schema
      },
      create(context) {
        return {
          CallExpression(node) {
            if (node.callee.name === 'console') {
              context.report({
                node,
                message: 'Unexpected console statement'
              });
            }
          }
        };
      }
    }
  }
};
```

Benefits:
- Simple API
- Easy to write
- Well-documented

### Configuration extension

Configs extend other configs:

```javascript
// eslint.config.js (flat config)
import js from '@eslint/js';
import react from 'eslint-plugin-react';

export default [
  js.configs.recommended,
  react.configs.recommended,
  {
    rules: {
      'react/react-in-js-scope': 'off'
    }
  }
];
```

Composable, shareable configurations.

### Flat config migration

Legacy eslintrc → flat config:

```yaml
# .eslintrc.yml (deprecated)
extends: airbnb
plugins: react
rules:
  no-console: warn
```

```javascript
// eslint.config.js (new)
import airbnb from 'eslint-config-airbnb';
import react from 'eslint-plugin-react';

export default [
  ...airbnb,
  react.configs.recommended,
  { rules: { 'no-console': 'warn' } }
];
```

Breaking change required for:
- Async config loading
- Better ESM support
- Clearer configuration model

### Plugin discovery

Conventional naming:
- `eslint-plugin-react` → `react`
- `eslint-config-airbnb` → `airbnb`
- Scoped: `@scope/eslint-plugin` → `@scope`

Automatic resolution from npm.

## 4) Standout strengths

- **Simple API**: Rules are just functions
- **Ecosystem**: 1000+ plugins
- **Configuration**: Flexible, composable
- **Documentation**: Extensive rule docs
- **Tooling**: IDE integrations
- **Community**: Active, large

## 5) Chronic weaknesses and recurring costs

### Configuration complexity

"Config hell" in JavaScript:
```javascript
// Can become very complex
// Multiple configs, overrides, extends
// Hard to understand final config
```

### Breaking changes

Flat config migration:
- Years of deprecation
- Still painful for users
- Plugin ecosystem had to update

### Performance

Large rule sets:
- Slow linting
- Memory intensive
- AST traversal overhead

### Version conflicts

Plugins depend on ESLint versions:
```
eslint-plugin-react requires eslint ^8.0.0
eslint-plugin-vue requires eslint ^7.0.0
// Conflict!
```

## 6) Between-release corrections

### Early ESLint (2013-2015)
- No plugin system
- Built-in rules only

### Plugin era (2015-2022)
- eslintrc configuration
- Plugin ecosystem growth
- Parser services

### Flat config era (2022-)
- New configuration system
- ESM support
- Async configuration

The pattern: Simple → ecosystem growth → config complexity → simplification.

## 7) Effigy-relevant lessons

### Adopt carefully

- **Simple API**: Easy to write plugins
- **Configuration extension**: Compose and share
- **Conventional naming**: Easy discovery
- **Documentation**: Essential for adoption

### Reject early

- **Config hell**: Keep configuration simple
- **Breaking changes**: Plan API stability
- **Version conflicts**: Careful dependency management
- **Performance overhead**: Consider plugin costs

### Prototype before deciding

- Effigy plugin API
- Configuration extension model
- Plugin discovery mechanism

## 8: Effigy Plugin Architecture

### Option 1: Task plugins

```toml
# effigy.toml
[plugins]
node = "https://github.com/effigy/plugin-node@v1.0.0"
rust = { path = "./plugins/rust" }
```

Plugins provide tasks:
```rust
// Plugin API
pub trait Plugin {
  fn name(&self) -> &str;
  fn tasks(&self) -> Vec<Task>;
  fn execute(&self, task: &str, ctx: &Context) -> Result<()>;
}
```

### Option 2: Hook-based plugins

```toml
# effigy.toml
[plugins]
notify = { hook = "post-task", command = "notify-send Done" }
cache-s3 = { hook = "cache", provider = "s3" }
```

Plugins hook into lifecycle events.

### Option 3: Script plugins

```toml
# effigy.toml
[plugins]
my-tasks = { path = "./effigy-plugin.js" }
```

```javascript
// effigy-plugin.js
export function tasks() {
  return [{
    name: 'custom-build',
    run: async (ctx) => {
      await ctx.exec('make custom');
    }
  }];
}
```

## 9: Source inventory

| Source | Type | Version/Era | Confidence | Notes |
|--------|------|-------------|------------|-------|
| [ESLint docs](https://eslint.org/docs/latest/) | official docs | current | high | Primary reference |
| [Plugin API](https://eslint.org/docs/latest/extend/plugins) | API docs | current | high | Plugin development |
| [Flat config](https://eslint.org/docs/latest/use/configure/configuration-files) | docs | current | high | New config system |
| ESLint source | source | latest | high | Implementation |

## 10: Open questions

- How to balance plugin power with security?
- What's the right API stability guarantee?
- Should plugins be in-process or out-of-process?

## Next Task

Compare against Bazel rules and other plugin systems in Track 14 synthesis.

