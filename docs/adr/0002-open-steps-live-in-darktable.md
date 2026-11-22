# ADR-0002 — Open steps live in darktable

**Status:** accepted (2026-11-22, confirmed by US-1)

## Context

The owner edits photos only occasionally and would forget manual steps between sessions. The
tooling has to tell what is still open, across restarts and long breaks.

## Decision

Every open step (a question, a proposal to confirm, a decision, a manual task) is recorded in
darktable as a tag on the affected photos. Open steps are shown to the owner in darktable and
can be listed for the whole collection; a step disappears only when it is resolved.

## Consequences

- No separate state store; the tool reads the open steps back from darktable.
- Tag names form a small, stable vocabulary that both the tool and the owner see.
