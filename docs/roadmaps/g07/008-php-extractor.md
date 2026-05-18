# g07.008 - PHP Extractor

Status: Complete
Depends on: `g07.004`

## Goal

Index PHP repos well enough for Decodelabs and legacy application work.

The extractor should provide useful class, method, function, namespace, include,
and route/front-controller context without hard-coding Decodelabs concepts into
core product behavior.

## Scope

- parse `.php` and `.phtml`
- extract:
  - namespaces
  - classes
  - interfaces
  - traits
  - methods
  - functions
  - constants where easy
  - `use` imports
  - `include`, `include_once`, `require`, `require_once`
- emit class/member containment edges
- emit namespace/import edges
- emit include/require edges when paths are static
- emit heuristic call-like references where syntactically obvious

## Legacy/Framework Guidance

Legacy PHP apps often use front controllers and runtime routing.

Do not hard-code product names into the extractor. Use generic concepts:

- front-controller file
- static route declaration
- include graph
- class autoload hints where deterministic

Any framework-specific heuristic must be named, optional, deterministic, and
marked as heuristic.

## Non-Goals

- no PHP runtime execution
- no Composer autoloader evaluation
- no framework boot
- no dynamic route discovery
- no DB-backed route inference

## Tests

- namespace/class/method fixtures
- trait/interface fixtures
- static include/require fixtures
- front-controller fixture
- legacy-style global function fixture
- parse-error diagnostic fixture

## Acceptance Criteria

- agents can navigate legacy PHP codebases by class/function/file ownership
- front-controller and include-heavy code does not collapse into useless search
- dynamic behavior is not represented as exact fact
- Decodelabs-style repos benefit without reintroducing Decodelabs into core

## Next Task

Execute `g07.009`.
