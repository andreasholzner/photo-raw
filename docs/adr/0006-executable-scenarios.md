# ADR-0006 — Acceptance criteria are executable scenarios

**Status:** accepted (2026-11-24)

## Context

The tool is built once and used for years with little further development. The owner reads the
specification as a non-programmer, often after long breaks. The main risk is darktable on the
owner's machine changing under the tool. Prose acceptance criteria can't be checked, and
separate tests would duplicate them.

## Decision

Testable acceptance criteria are written as Gherkin scenarios, one feature file per story, and
replace the story's prose criteria. requirements.md keeps the story, its state and the criteria
that can't be executed. Scenarios that depend on darktable run against the owner's installed
darktable as part of the self-test; the others run with the normal tests. Unit tests remain for
details below the level of a story.

## Consequences

- The scenarios are the specification: a change in behaviour starts with changing its scenario.
- The step code is maintained together with the tool's code.
- After a darktable upgrade, the self-test also checks the darktable-dependent scenarios.
- Scenarios in the darktable UI stay manual checks unless the Lua layer can be driven
  unattended.
