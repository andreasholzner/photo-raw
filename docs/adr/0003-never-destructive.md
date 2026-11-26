# ADR-0003 — The tool is never destructive

**Status:** accepted (2026-11-22, amended 2026-11-26)

## Context

Raws, sidecars and manual edits are years of work. The tool runs unattended and on photos the
owner hasn't looked at yet.

## Decision

The tool never deletes the owner's photos or files and never overwrites an edit the owner made
by hand. It only changes photos it was handed, and only histories it created itself. It may
remove what it created itself — such as duplicates made for comparison — as long as nobody has
changed them since. Destructive steps on the owner's material (deleting culled photos or
panorama sources) are offered as open steps for the owner to do.

## Consequences

- The tool must be able to tell its own edits from manual ones.
- Every change it makes can be undone in darktable's history.
- Whatever the tool creates must be recognisable as its own (like its edits); once the owner
  changes it, it is the owner's and stays.
