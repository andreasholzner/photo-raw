# ADR-0001 — darktable is the editor and holds the photo state

**Status:** accepted (2026-11-22)

## Context

The owner edits, culls and fine-tunes in darktable. A separate app that owns the photos and
calls darktable for rendering was considered: it would be independent of darktable's Lua API,
but photo state would live in two places and culling/browsing would have to be rebuilt.

## Decision

darktable stays the editor and the single home of photo state (library and sidecars). The
tooling integrates into darktable — started from it, reporting into it — and is also usable
from the terminal. It does not keep its own copy of photo state.

## Consequences

- The integration depends on darktable's Lua API; US-1 verifies it first.
- Results the tool produces must reach darktable's library, not only the sidecar files.
