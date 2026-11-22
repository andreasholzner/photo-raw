# ADR-0004 — Per-photo values come from darktable's own rendering

**Status:** accepted (2026-11-22)

## Context

Per-photo values (exposure, aurora sky brightness and colour) are defined on how the final
edit looks. Predicting that requires the exact darktable pipeline.

## Decision

The tool determines per-photo values by rendering with darktable, measuring the result and
adjusting until the target is met. It does not reimplement darktable's image processing.

## Consequences

- Results match what darktable shows, also across darktable upgrades (checked by the
  self-test, US-2).
- Rendering several times per photo is slow; acceptable because runs are unattended.
