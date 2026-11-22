# ADR-0003 — The tool is never destructive

**Status:** accepted (2026-11-22)

## Context

Raws, sidecars and manual edits are years of work. The tool runs unattended and on photos the
owner hasn't looked at yet.

## Decision

The tool never deletes files and never overwrites an edit the owner made by hand. It only
changes photos it was handed, and only histories it created itself. Destructive steps
(deleting culled photos or panorama sources) are offered as open steps for the owner to do.

## Consequences

- The tool must be able to tell its own edits from manual ones.
- Every change it makes can be undone in darktable's history.
