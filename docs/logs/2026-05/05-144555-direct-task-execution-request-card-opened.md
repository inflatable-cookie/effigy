# 2026-05-05 - Direct Task Execution Request Card Opened

## Summary

Opened and completed card `380` as the next `g03.032` execution request
migration.

## Scope

The card targets only direct `Command::Task` dispatch. It should build a
`TaskExecutionRequest` before entering the existing execution pipeline without
changing task behavior.

## Closeout

Direct task dispatch now builds through `TaskExecutionRequestBuilder`.
Card `381` is open for embedded task dispatch.

## Next

Implement card `381`.
