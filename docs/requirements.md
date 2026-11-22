# photoflow — Requirements & User Stories

## Purpose

Tooling that takes the **repetitive work out of editing raw photos** in darktable and stitching
panoramas in hugin, while **darktable stays the editor**.

- **In scope:** classifying photos (day / night / aurora), automatic base edits with per-photo
  values (exposure, aurora sky brightness and colour), proposals (gradient filter, panorama
  sets), panorama stitching, keeping track of open steps per folder, darktable integration.
- **Out of scope:** replacing darktable or hugin, culling decisions, drawn masks, retouching.

Background, analyses and the reasoning behind these stories: [journal.md](journal.md).
Reference links: [links.md](links.md).

**Guiding requirements**

- **Seamless and hard to get wrong.** Editing happens only occasionally, so the tooling never
  relies on the owner remembering a step: every open step is shown, and it survives restarts
  and months-long breaks.
- **darktable is where the owner works.** Tooling is started from and reports into darktable;
  it can also be used from the terminal.
- **Ask instead of guessing; fail loudly.** Uncertain classifications become questions, broken
  preconditions stop a run with a clear message.
- **Never destructive, never overwrite manual work.**
- **Unattended runs are fine; speed is secondary.**
- **Print is the main output.** Looks are judged for print.

## Actors

- **The owner** — the single user: takes the photos, edits them in darktable on their own
  Manjaro machine, prints them.

## Scheduling

Tracks progress on work that spans several dependent stories: each step names the stories it
covers and why it comes where it does. Only what is not yet done is listed: a step leaves this
table when its stories ship. Each story's own state is recorded with the story and in the
[story index](#story-index) below.

| Step | Stories              | Why here                                                                                   |
|------|----------------------|--------------------------------------------------------------------------------------------|
| 2    | US-2                 | Reference set and self-test are needed to compare pipelines and to judge every later rule  |
| 3    | US-3, US-4           | New base pipeline and haze/clarity approach; the per-photo rules are fitted on top of them |
| 4    | US-5, US-6           | Print sharpening moves to the export; rules validated on more edited photos                |
| 5    | US-7 – US-10         | The tool's core: classify, safety, day and aurora edits                                    |
| 6    | US-14, US-17         | The to-do panel and the installation make it usable day to day                             |
| 7    | US-11 – US-13, US-15 | Proposals, panoramas, collection-wide status                                               |

## Story index

**State:** ✅ done · 🚧 in progress · 📋 planned · ⏳ backlog (not planned but not descoped) · 🚫 de-scoped

Stories are grouped by the part of the workflow they are about; a story's ID never changes.

| ID                                                  | State | Story                               | Area                                            |
|-----------------------------------------------------|-------|-------------------------------------|-------------------------------------------------|
| [US-1](#us-1--lua-spike-in-darktable)               | ✅     | Lua spike in darktable              | [Foundation](#foundation)                       |
| [US-2](#us-2--reference-set-and-self-test)          | 📋     | Reference set and self-test         | [Foundation](#foundation)                       |
| [US-3](#us-3--modern-base-pipeline)                 | 📋     | Modern base pipeline                | [Foundation](#foundation)                       |
| [US-4](#us-4--haze-and-clarity)                     | 📋     | Haze and clarity                    | [Foundation](#foundation)                       |
| [US-5](#us-5--print-sharpening-in-the-export)       | 📋     | Print sharpening in the export      | [Foundation](#foundation)                       |
| [US-6](#us-6--more-analysis-on-edited-photos)       | 📋     | More analysis on edited photos      | [Foundation](#foundation)                       |
| [US-7](#us-7--classify-day-night-and-aurora)        | 📋     | Classify day, night and aurora      | [Automatic editing](#automatic-editing)         |
| [US-8](#us-8--only-touch-what-was-handed-over)      | 📋     | Only touch what was handed over     | [Automatic editing](#automatic-editing)         |
| [US-9](#us-9--day-auto-edit)                        | 📋     | Day auto-edit                       | [Automatic editing](#automatic-editing)         |
| [US-10](#us-10--aurora-auto-edit)                   | 📋     | Aurora auto-edit                    | [Automatic editing](#automatic-editing)         |
| [US-11](#us-11--gradient-filter-proposal)           | 📋     | Gradient filter proposal            | [Automatic editing](#automatic-editing)         |
| [US-12](#us-12--panorama-sets)                      | 📋     | Panorama sets                       | [Panoramas](#panoramas)                         |
| [US-13](#us-13--stitch-panoramas)                   | 📋     | Stitch panoramas                    | [Panoramas](#panoramas)                         |
| [US-14](#us-14--to-do-panel-in-darktable)           | 📋     | To-do panel in darktable            | [darktable integration](#darktable-integration) |
| [US-15](#us-15--status-across-the-collection)       | 📋     | Status across the collection        | [darktable integration](#darktable-integration) |
| [US-16](#us-16--optional-export-step)               | ⏳     | Optional export step                | [darktable integration](#darktable-integration) |
| [US-17](#us-17--installation-on-the-owners-machine) | 📋     | Installation on the owner's machine | [darktable integration](#darktable-integration) |

### Maintaining this file

- A new story takes the next free ID — one above the highest in the table — and goes under the
  area it is about, in ID order, as `### US-N — Title`. It also gets a row in the table above.
- When a story's state changes, update the state icon both in the story and in the table.
- A story keeps its title once given: the table links to the story by its heading.
- A story whose criteria are not yet worked out carries **Notes** instead of **Acceptance
  criteria**; the label changes when the story is refined.

## Foundation

### US-1 — Lua spike in darktable

**Done ✅** — As the owner, I want proof that darktable's Lua API can carry the integration
before the tool is built around it.

**Acceptance criteria:** On the owner's darktable 5.6.1 a small script demonstrates, or documents
a limit or workaround for, each of: reacting to an import; a side panel with a list and buttons;
running an external command in the background with progress while the UI stays usable;
attaching and detaching tags; loading a sidecar into the library.

**Decisions:** US-1 → ADR-0001, ADR-0002 (proposed; accepted or revised by the outcome)

### US-2 — Reference set and self-test

**Planned 📋** — As the owner, I want a fixed set of my edited photos with reference renders, so
pipeline changes and darktable upgrades are checked instead of trusted.

**Acceptance criteria:** The set covers day, mountain/haze, aurora and panorama photos. One command
renders the set and reports the difference to the references per photo. After a darktable
upgrade automatic edits are blocked until the self-test has run and its result was accepted.

**Decisions:** US-2 → ADR-0004 (rendering is darktable's)

### US-3 — Modern base pipeline

**Planned 📋** — As the owner, I want my base edits on darktable's current scene-referred
approach instead of the display-referred stack I kept out of habit.

**Acceptance criteria:** One display transform (sigmoid, AgX or filmic), no additional contrast
curve on top. Candidates are compared side by side on the reference set (day and aurora) and the
owner picks. Results are a day and an aurora base style; the per-photo rules (US-9, US-10) are
re-fitted to the new pipeline.

### US-4 — Haze and clarity

**Planned 📋** — As the owner, I want the haze-removal look I like on mountain photos, and the
"punch" on other photos including aurora, without the instability haze removal showed on night
shots.

**Acceptance criteria:** Haze removal, "diffuse or sharpen" (dehaze/clarity presets) and local
contrast are compared on mountain, other day and aurora photos. The outcome is a choice per photo
type, applied automatically where possible (e.g. haze removal only where there is haze).

### US-5 — Print sharpening in the export

**Planned 📋** — As the owner, I want the strong print sharpening applied by the print export
instead of in every edit.

**Acceptance criteria:** The print export applies the print sharpening; edits keep only moderate
sharpening.

### US-6 — More analysis on edited photos

**Planned 📋** — As the owner, I want the rules checked against more of my edited photos before I
rely on them.

**Notes:** Day exposure rule (so far fitted on 37 photos), gradient filter usage (position,
density), haze usage, aurora targets. Results go to the journal.

## Automatic editing

### US-7 — Classify day, night and aurora

**Planned 📋** — As the owner, I want photos sorted into day, night and aurora automatically, and
to be asked only where it is unclear.

**Acceptance criteria:** Scene brightness from EXIF (EV100): ≤ 1 is night, ≥ 8 is day, in between
becomes an open question. Night photos are checked for aurora (colour in the sky on a small
render); night photos without aurora get a generic night edit or become a question.

### US-8 — Only touch what was handed over

**Planned 📋** — As the owner, I want the tool to leave alone everything it wasn't asked to
process and every edit I made by hand.

**Acceptance criteria:** Only newly imported or selected photos are processed. Photos with a
manual edit are skipped and reported. Running the tool again on the same photos changes nothing
that was already right.

**Decisions:** US-8 → ADR-0003

### US-9 — Day auto-edit

**Planned 📋** — As the owner, I want day photos to get my base edit with a per-photo exposure, so
I only correct outliers.

**Acceptance criteria:** Base style plus exposure from the highlight rule (brightest 1 % at a
target), validated by US-6. Photos where the rule is uncertain are flagged.

**Decisions:** US-9 → ADR-0004

### US-10 — Aurora auto-edit

**Planned 📋** — As the owner, I want aurora photos edited to show how the aurora felt — bright and
colourful, not neon — so I can cull after editing.

**Acceptance criteria:** Base style plus per-photo sky brightness and per-photo sky colour (the
aurora changes within minutes). A per-night preference (neutral blue to blue-violet) is the
default and can be overridden per photo. The brightness target depends on the scene (e.g. a lit
foreground gets a darker target). Variants for comparison are available.

**Decisions:** US-10 → ADR-0004

### US-11 — Gradient filter proposal

**Planned 📋** — As the owner, I want a graduated filter proposed where the sky is much brighter
than the land.

**Notes:** Applied as a separate, easily removed step and listed for confirmation. Density and
angle from my past usage (US-6).

## Panoramas

### US-12 — Panorama sets

**Planned 📋** — As the owner, I mark panorama sets in darktable, and the tool proposes sets I may
have missed.

**Acceptance criteria:** Sets are marked with a tag or label. Proposals (short time gaps, same
focal length) are listed for confirmation and never stitched unconfirmed.

### US-13 — Stitch panoramas

**Planned 📋** — As the owner, I want confirmed sets stitched and back in my folder without manual
steps.

**Acceptance criteria:** Confirmed sets get the panorama style, are exported, stitched with
hugin's automatic chain, and the normal (non-fused) result lands in the trip folder and the
darktable library with the usual post-stitch edit. A failed alignment becomes an open step.
Choosing a fused result instead is a later, optional open step. Deleting the source raws is an
open step for the owner.

**Decisions:** US-13 → ADR-0003

## darktable integration

### US-14 — To-do panel in darktable

**Planned 📋** — As the owner, I see in darktable what is still open for the current folder and
act on it there.

**Acceptance criteria:** A side panel lists the open steps (day/night unclear, sky colour,
panorama sets, gradient proposals, culling, deletions, export). Clicking a step shows the
affected photos; actions are buttons. A folder without open steps shows as done. Processing
starts automatically after an import (can be switched off).

**Decisions:** US-14 → ADR-0001, ADR-0002

### US-15 — Status across the collection

**Planned 📋** — As the owner, I get a list of all folders with open steps, so nothing is
forgotten between editing sessions.

**Acceptance criteria:** A terminal command lists every folder with open steps and what they are.

**Decisions:** US-15 → ADR-0002

### US-16 — Optional export step

**Backlog ⏳** — As the owner, I export a finished folder with my existing presets ("Fotobuch", …)
as an optional open step.

**Notes:** Low priority: exporting is already a single action with the existing preset.

### US-17 — Installation on the owner's machine

**Planned 📋** — As the owner, I install and update the tool and its darktable script simply on
Manjaro.

**Acceptance criteria:** One documented way to install and update the binary and the Lua script.
The tool runs next to an open darktable without touching its library or configuration.

**Decisions:** US-17 → ADR-0005
