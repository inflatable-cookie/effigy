# 038 - Plugin-Ready Container Manager Facade Strict Lane

Roadmap: [`g03.031`](../roadmaps/g03/031-plugin-ready-container-manager-facade.md)

Status: Complete
Owner: Platform
Created: 2026-05-05
Completed: 2026-05-05

## Purpose

Move container backend selection and operation shape behind one manager facade
before migrating runner container commands and execution transports.

## Hard Boundaries

- do not edit `.github/workflows/`
- do not initiate release commands
- do not add dynamic plugin loading in this lane
- do not change public CLI JSON schemas in the first manager slices
- keep Docker Compose and Colima/nerdctl as the only implemented backends in
  this round

## Current Ready Card

No active ready card. Lane `038` is complete.

## Exit Condition

This lane closes when container operations route through `ContainerManager`,
backend-specific code is behind `ContainerBackend`, and runner command code no
longer branches directly on Docker, Colima, or nerdctl.

## Next Task

Choose the next queued roadmap deliberately. The likely next roadmap is
`g03.033`.
