# VS Code Tasks

Status: Draft
Tool name: VS Code Tasks
Category: IDE task integration (editor extension)
Owner:
Last updated: 2026-03-07
Scope: VS Code tasks.json, problem matchers, task providers, extensions

## 1) Why this tool matters

VS Code Tasks provide generic task integration for editors. It's notable for:
- tasks.json configuration for custom tasks
- Problem matchers for error parsing
- Task providers for dynamic task discovery
- Extensions for Make, Just, npm, etc.

For Effigy, VS Code Tasks represents:
- IDE integration patterns
- Task discovery mechanisms
- Output parsing conventions
- Editor extension models

## 2) Product and era context

### Timeline

- **2015**: VS Code initial release
- **2016**: Tasks system introduced
- **2017**: Problem matchers added
- **2019**: Task providers API
- **2020+**: Extensions for task runners (Make, Just, etc.)

### Design Philosophy

From VS Code documentation:

> "Tasks in VS Code can be configured to run scripts and start processes"
> "Many tools that are already used today can be used from within VS Code"

### Target Audience

- Developers using VS Code
- Teams wanting IDE-integrated workflows
- Projects with custom build scripts

### Ecosystem

- **Built-in support**: npm, TypeScript, gulp, grunt, jake
- **Extensions**: Make, Just, Task, cargo
- **Custom tasks**: Any shell command

## 3) Defining architectural bets

### tasks.json configuration

Tasks defined in `.vscode/tasks.json`:

```json
{
  "version": "2.0.0",
  "tasks": [
    {
      "label": "Build",
      "type": "shell",
      "command": "cargo build",
      "group": "build",
      "problemMatcher": ["$rustc"]
    },
    {
      "label": "Test",
      "type": "shell",
      "command": "cargo test",
      "group": "test"
    }
  ]
}
```

Benefits:
- Version controlled
- Team-shared
- Flexible

### Problem matchers

Parse tool output for error detection:

```json
{
  "problemMatcher": {
    "pattern": {
      "regexp": "^(.*):(\\d+):(\\d+):\\s+(error|warning):\\s+(.*)$",
      "file": 1,
      "line": 2,
      "column": 3,
      "severity": 4,
      "message": 5
    }
  }
}
```

Integrates with VS Code problems panel.

### Task providers

Extensions contribute tasks dynamically:

```typescript
// Extension API
vscode.tasks.registerTaskProvider('cargo', {
  provideTasks: () => {
    return [
      new vscode.Task(
        { type: 'cargo', task: 'build' },
        vscode.TaskScope.Workspace,
        'build',
        'cargo',
        new vscode.ShellExecution('cargo build')
      )
    ];
  }
});
```

### Auto-detection

VS Code auto-detects tasks from:
- package.json (npm scripts)
- Makefile
- gulpfile.js
- Gruntfile.js
- tsconfig.json

No configuration needed for common cases.

## 4) Standout strengths

- **Flexibility**: Any shell command as task
- **Problem matchers**: Parse errors for IDE integration
- **Auto-detection**: Works without configuration
- **Keybindings**: Run tasks via keyboard shortcuts
- **Extensions**: Ecosystem of task providers
- **Debugging**: Tasks can start debugging sessions

## 5) Chronic weaknesses and recurring costs

### Configuration complexity

Complex projects need verbose tasks.json:
```json
// Can become hundreds of lines
// Multiple configurations per task
// Platform-specific variants
```

### Duplication

Tasks often duplicate what's in other config files:
- Makefile targets
- npm scripts
- Taskfile.yml

### Limited introspection

No way to:
- List available tasks without opening VS Code
- Run tasks from command line
- Share tasks between editors

## 6) Between-release corrections

### Early VS Code (2015-2017)
- Basic task running
- Manual configuration required

### Modern VS Code (2018-2024)
- Task providers API
- Auto-detection
- Better problem matchers
- More extensions

The pattern: From manual configuration to intelligent auto-detection.

## 7) Effigy-relevant lessons

### Adopt carefully

- **Auto-detection**: Discover tasks without configuration
- **Problem matchers**: Parse output for IDE consumption
- **Task providers**: Dynamic task contribution
- **Shell execution**: Keep it simple

### Reject early

- **VS Code-specific formats**: Keep generic
- **tasks.json duplication**: Single source of truth
- **Editor lock-in**: Support multiple editors

### Prototype before deciding

- Effigy VS Code extension
- Task provider implementation
- Problem matcher patterns

## 8: Effigy VS Code Integration

### Option 1: tasks.json generation

```bash
effigy ide vscode init  # Generates .vscode/tasks.json
```

```json
{
  "version": "2.0.0",
  "tasks": [
    {
      "label": "effigy: build",
      "type": "shell",
      "command": "effigy build",
      "group": "build",
      "problemMatcher": ["$effigy"]
    }
  ]
}
```

### Option 2: Task provider extension

```typescript
// VS Code extension
vscode.tasks.registerTaskProvider('effigy', {
  provideTasks: async () => {
    const tasks = await exec('effigy --list --format json');
    return tasks.map(t => new vscode.Task(
      { type: 'effigy', task: t.name },
      vscode.TaskScope.Workspace,
      t.name,
      'effigy',
      new vscode.ShellExecution(`effigy ${t.name}`)
    ));
  }
});
```

### Option 3: Auto-detection

VS Code detects effigy.toml and suggests tasks.

## 9: Source inventory

| Source | Type | Version/Era | Confidence | Notes |
|--------|------|-------------|------------|-------|
| [VS Code docs](https://code.visualstudio.com/docs/editor/tasks) | official docs | current | high | Primary reference |
| [Tasks API](https://code.visualstudio.com/api/extension-guides/task-provider) | API docs | current | high | Extension development |
| [Problem matchers](https://code.visualstudio.com/docs/editor/tasks#_processing-task-output-with-problem-matchers) | docs | current | high | Error parsing |
| VS Code source | source | latest | high | Implementation |

## 10: Open questions

- How effective is auto-detection vs. explicit configuration?
- What problem matcher patterns are most reliable?
- How to handle task dependencies in IDE context?

## Next Task

Compare against Language Server Protocol and other IDE tools in Track 13 synthesis.

