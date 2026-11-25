# Executable scenarios

One file per story: the story's acceptance criteria as examples that are checked automatically
(ADR-0006). The story itself — why it exists, its state, decisions — stays in
[requirements.md](../docs/requirements.md).

**Tags** above a rule, scenario or table of examples say where it runs:

| Tag | Meaning |
|---|---|
| *(none)* | Checked with the tool's normal tests; needs no darktable. |
| `@darktable` | Depends on how darktable behaves. Runs with the normal tests against a simulated darktable, and in the self-test against the real darktable application — on the owner's machine and in the development container (US-20). |
| `@darktable-cli` | Depends on darktable's rendering. Runs in the self-test with darktable-cli (US-20). |
| `@manual` | Can't be checked unattended; the self-test lists it as a manual check. |
| `@wip` | Describes behaviour that isn't built yet; not run. |

Run the normal tests: `cargo test --test features`.
