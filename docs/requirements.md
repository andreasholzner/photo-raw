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

The stories of a step are worked on in parallel: those that wait for long rendering runs
(experiments, analyses, darktable tests) alternate with code-only ones, which fill the waits.

| Step | Rendering-heavy     | Code-only            | Why here                                                                    |
|------|---------------------|----------------------|-----------------------------------------------------------------------------|
| 5    | US-23, US-10, US-21 | US-12, US-14         | The aurora and night edits need the horizon line; the panorama sets are used through the to-do panel |
| 6    | US-13, US-11        | US-17, US-16, US-15  | Stitching and the gradient filter (it needs the horizon line too); installation, export and the collection status make it usable day to day |
| 7    | US-19               |                      | Own edits after a darktable upgrade: needs a darktable release to test against |

## Story index

**State:** ✅ done · 🚧 in progress · 📋 planned · ⏳ backlog (not planned but not descoped) · 🚫 de-scoped

Stories are grouped by the part of the workflow they are about; a story's ID never changes.

| ID                                                              | State | Story                                          | Area                                            |
|-----------------------------------------------------------------|-------|------------------------------------------------|-------------------------------------------------|
| [US-1](#us-1--lua-spike-in-darktable)                           | ✅     | Lua spike in darktable                         | [Foundation](#foundation)                       |
| [US-2](#us-2--reference-set-and-self-test)                      | ✅     | Reference set and self-test                    | [Foundation](#foundation)                       |
| [US-3](#us-3--modern-base-pipeline)                             | ✅     | Modern base pipeline                           | [Foundation](#foundation)                       |
| [US-4](#us-4--haze-and-clarity)                                 | ✅     | Haze and clarity                               | [Foundation](#foundation)                       |
| [US-5](#us-5--print-sharpening-in-the-export)                   | ✅     | Print sharpening in the export                 | [Foundation](#foundation)                       |
| [US-6](#us-6--more-analysis-on-edited-photos)                   | ✅     | More analysis on edited photos                 | [Foundation](#foundation)                       |
| [US-7](#us-7--classify-day-night-and-aurora)                    | ✅     | Classify day, night and aurora                 | [Automatic editing](#automatic-editing)         |
| [US-8](#us-8--only-touch-what-was-handed-over)                  | ✅     | Only touch what was handed over                | [Automatic editing](#automatic-editing)         |
| [US-9](#us-9--day-auto-edit)                                    | ✅     | Day auto-edit                                  | [Automatic editing](#automatic-editing)         |
| [US-10](#us-10--aurora-auto-edit)                               | 🚧     | Aurora auto-edit                               | [Automatic editing](#automatic-editing)         |
| [US-11](#us-11--gradient-filter-proposal)                       | 📋     | Gradient filter proposal                       | [Automatic editing](#automatic-editing)         |
| [US-12](#us-12--panorama-sets)                                  | 🚧     | Panorama sets                                  | [Panoramas](#panoramas)                         |
| [US-13](#us-13--stitch-panoramas)                               | 📋     | Stitch panoramas                               | [Panoramas](#panoramas)                         |
| [US-14](#us-14--to-do-panel-in-darktable)                       | 📋     | To-do panel in darktable                       | [darktable integration](#darktable-integration) |
| [US-15](#us-15--status-across-the-collection)                   | 📋     | Status across the collection                   | [darktable integration](#darktable-integration) |
| [US-16](#us-16--optional-export-step)                           | 📋     | Optional export step                           | [darktable integration](#darktable-integration) |
| [US-17](#us-17--installation-on-the-owners-machine)             | 📋     | Installation on the owner's machine            | [darktable integration](#darktable-integration) |
| [US-18](#us-18--aurora-base-pipeline)                           | ✅     | Aurora base pipeline                           | [Foundation](#foundation)                       |
| [US-19](#us-19--own-edits-recognised-after-a-darktable-upgrade) | 📋     | Own edits recognised after a darktable upgrade | [Automatic editing](#automatic-editing)         |
| [US-20](#us-20--self-test-checks-darktable-behaviour)           | ✅     | Self-test checks darktable behaviour           | [Foundation](#foundation)                       |
| [US-21](#us-21--night-auto-edit)                                | 🚧     | Night auto-edit                                | [Automatic editing](#automatic-editing)         |
| [US-22](#us-22--dehaze-proposal-for-long-range-views)           | ⏳     | Dehaze proposal for long-range views           | [Automatic editing](#automatic-editing)         |
| [US-23](#us-23--horizon-line-per-photo)                         | 📋     | Horizon line per photo                         | [Automatic editing](#automatic-editing)         |

### Maintaining this file

- A new story takes the next free ID — one above the highest in the table — and goes under the
  area it is about, in ID order, as `### US-N — Title`. It also gets a row in the table above.
- When a story's state changes, update the state icon both in the story and in the table.
- A story keeps its title once given: the table links to the story by its heading.
- A story whose criteria are not yet worked out carries **Notes** instead of **Acceptance
  criteria**; the label changes when the story is refined.
- Testable acceptance criteria live in the story's feature file in [features/](../features/)
  (ADR-0006); the story links to it and keeps in prose only what can't be executed.

## Foundation

### US-1 — Lua spike in darktable

**Done ✅** — As the owner, I want proof that darktable's Lua API can carry the integration
before the tool is built around it.

**Acceptance criteria:** On the owner's darktable 5.6.1 a small script demonstrates, or documents
a limit or workaround for, each of: reacting to an import; a side panel with a list and buttons;
running an external command in the background with progress while the UI stays usable;
attaching and detaching tags; loading a sidecar into the library.

**Decisions:** US-1 → ADR-0001, ADR-0002 (accepted on its outcome)

### US-2 — Reference set and self-test

**Done ✅** — As the owner, I want a fixed set of my edited photos with reference renders,
so pipeline changes and darktable upgrades are checked instead of trusted.

**Acceptance criteria:** The set covers day, mountain/haze, aurora, night and panorama photos and
is stored by file name and folder year only — no folder names in the repository; the photo root
is configured. One command renders the set with the owner's edits and reports the difference to
the reference per photo; differences within a tolerance (rendering noise) pass. Reference renders
are made and kept on the owner's machine, outside the repository, in a configurable location.
Intended changes become the new reference with one command.

**Decisions:** US-2 → ADR-0004 (rendering is darktable's)

### US-3 — Modern base pipeline

**Done ✅** — As the owner, I want my base edits on darktable's current scene-referred
approach instead of the display-referred stack I kept out of habit.

**Acceptance criteria:** One display transform per photo, no additional contrast curve on top.
Candidates are compared side by side on the reference set and the owner picks. For day photos
there are two looks, chosen per photo: sigmoid "ACES 100-nit like" with colour balance rgb
"vibrant colors", and AgX "punchy" (exposure matched on the midtones). A rule proposes the look,
the owner can switch it, and the owner's choices are kept as data to improve the rule. The
aurora base is US-18. The per-photo exposure rules (US-9, US-10) are re-fitted to
the new looks.

### US-4 — Haze and clarity

**Done ✅** — As the owner, I want the haze-removal look I like on mountain photos, and the
"punch" on other photos, without the instability haze removal showed on night shots.

**Acceptance criteria:** Haze removal, "diffuse or sharpen: dehaze" and local contrast "clarity"
are compared on day photos in the new looks (US-3), including 100 % crops of long-range mountain
views; aurora belongs to US-18. Outcome: haze removal is no longer used on day photos (too much
on top of the new looks); local contrast "clarity" goes on every day photo; "dehaze" is added for
long-range views as a proposal the owner can switch off.

### US-5 — Print sharpening in the export

**Done ✅** — As the owner, I want the strong print sharpening applied by the print export
instead of in every edit.

**Acceptance criteria:** A print style ("photoflow Druck v1": the "Aufhellung Druck" curve plus the
"mehr schärfen" sharpening) is kept in the repository; for an edit without sharpening it reproduces
today's print export exactly. A new export preset "Fotobuch (neu)" applies it; the existing
"Fotobuch" stays for older edits, which already contain sharpening and would otherwise be sharpened
twice. A short setup description explains the one-time steps in darktable. New edits by the tool
contain no sharpening (US-9).

### US-6 — More analysis on edited photos

**Done ✅** — As the owner, I want the rules checked against more of my edited photos before I
rely on them.

**Acceptance criteria:** The rules of the day auto-edit are checked on all of the owner's edited day
photos from 2022–2025 (706, instead of the 15–37 they were fitted on), each in the day edit of US-9
(no haze removal, "clarity"):
- the exposure rule of each day look — a fixed highlight target for sigmoid, a fixed midtone target
  for AgX — fitted on some of the years and checked on the others, with the largest misses listed;
- the look-choice rule of US-3, on 50 random photos the owner labels; more only if the rule doesn't
  hold;
- the gradient filter: how often and with which density, angle and position it was used, and
  whether the sky-versus-land brightness in the new look predicts where (US-11).

Haze removal is no longer used (US-4) and is not analysed further. Aurora targets belong to US-10,
because they depend on the aurora base (US-18). Results go to the journal.

### US-18 — Aurora base pipeline

**Done ✅** — As the owner, I want my aurora photos on the new pipeline too, with less noise
but without losing the aurora's structure and colour.

**Acceptance criteria:** Candidates are compared side by side, including 100 % crops, on aurora
photos of the reference set and the 2026 aurora nights, and the owner picks. Outcome (journal
2026-11-25, rounds 1–4): scene-referred, chroma-only denoise plus a non-local-means luma denoise,
local contrast "clarity" twice, no haze removal, tone curve or sharpening; sigmoid "ACES 100-nit
like" with colour balance rgb "vibrant colors" and extra chroma. The land below the horizon gets
its own colour correction (a masked color calibration) towards slightly blue snow, since one
white balance can't serve both sky and foreground. Per-photo values are US-10's.

### US-20 — Self-test checks darktable behaviour

**Done ✅** — As the owner, I want the self-test to also check the darktable behaviour the tool
relies on, so that a darktable upgrade can't silently break it.

**Acceptance criteria:** The self-test also runs the executable scenarios that depend on darktable
(ADR-0006): those tagged `@darktable` against the darktable application with a private
configuration and library. They run on the owner's machine, with its darktable on a virtual screen, as part of the self-test,
and in the development container. Scenarios that can't be run unattended are listed as manual checks.

**Notes:** A spike showed that a Lua script can drive everything US-8's scenarios need
unattended, also under a virtual display (journal 2026-11-24, `spikes/us-20-darktable-tests/`).

## Automatic editing

### US-7 — Classify day, night and aurora

**Done ✅** — As the owner, I want photos sorted into day, night and aurora automatically, and
to be asked only where it is unclear.

**Acceptance criteria:** [us-07-classify-day-night-and-aurora.feature](../features/us-07-classify-day-night-and-aurora.feature)

### US-8 — Only touch what was handed over

**Done ✅** — As the owner, I want the tool to leave alone everything it wasn't asked to
process and every edit I made by hand.

**Acceptance criteria:** [us-08-only-touch-what-was-handed-over.feature](../features/us-08-only-touch-what-was-handed-over.feature)

**Decisions:** US-8 → ADR-0003

### US-9 — Day auto-edit

**Done ✅** — As the owner, I want day photos to get my base edit with a per-photo exposure, so
I only correct outliers.

**Acceptance criteria:** [us-09-day-auto-edit.feature](../features/us-09-day-auto-edit.feature).
Not executable: the self-test (US-2) also checks the tool's edits of the reference set's day photos
against the tool's previous output; intended changes become the new baseline.

**Notes:** The look's targets are from US-6 (AgX: brightest 10 % at 0.675; sigmoid: brightest 1 %
at 0.885). Dehaze is off by default; proposing it for long-range views is US-22.

**Decisions:** US-9 → ADR-0004

### US-10 — Aurora auto-edit

**In progress 🚧** — As the owner, I want aurora photos edited to show how the aurora felt — bright and
colourful, not neon — so I can cull after editing.

**Acceptance criteria:** Aurora base (US-18) plus per-photo sky brightness and per-photo sky colour
(the aurora changes within minutes). The brightness target depends on the scene (e.g. a lit
foreground gets a darker target). The sky targets are derived from the owner's past aurora edits and
checked on them. The foreground (land below the horizon, US-23) gets its own per-photo colour
correction towards slightly blue snow (US-18). The sky colour (neutral blue or blue-violet) is
chosen per night and can be overridden per photo: until it is chosen, a night's photos get a
default colour, an open step for the night, and duplicates in the other colour to compare; once
chosen, the tool redoes the edits and removes its comparison duplicates nobody changed (ADR-0003).
The self-test checks the tool's edits of the reference set's aurora photos like the day edits, and
the upgrade block of US-9 applies.

**Notes:** Sky targets (journal 2026-11-26): sky median 0.24, or 0.14 where the scene's foreground is
at least 0.6 × as bright as its sky (measured on the aurora check render); sky background
neutral blue R/G 0.95, B/G 1.35, blue-violet 1.05, 1.50. Against the owner's own edits the rule is
off by a median 0.4 EV (the owner's style changed between years); the owner adjusts single photos
with the exposure slider. On very dark foregrounds (village lights, mountains) the foreground
correction made no measurable difference (journal 2026-11-25, round 3).

**Decisions:** US-10 → ADR-0003, ADR-0004

### US-11 — Gradient filter proposal

**Planned 📋** — As the owner, I want a graduated filter proposed where the sky is much brighter
than the land.

**Notes:** Applied as a separate, easily removed step and listed for confirmation. Density and
angle from my past usage (US-6). The sky-versus-land brightness alone is a weak trigger: about
half of such proposals would match where the owner used a filter (US-6, journal 2026-11-25); a
better signal is still to be found. The filter's position follows the horizon.

### US-19 — Own edits recognised after a darktable upgrade

**Planned 📋** — As the owner, I want the tool to recognise its own edits also after a darktable
upgrade, so that only photos I really edited by hand are protected.

**Notes:** darktable converts modules to their current version when it loads an edit made with an
older version, and writes the converted settings back; the edit then no longer matches the
fingerprint the tool tagged it with (US-8, journal 2026-11-24). A photo whose state can't be told
for sure still counts as edited by hand (ADR-0003).

### US-21 — Night auto-edit

**In progress 🚧** — As the owner, I want night photos without aurora — stars, the Milky Way, moonlit
mountains, city lights below — edited automatically close to my favourite night edits, so I only
correct outliers.

**Acceptance criteria:** Night photos get the night edit, chosen side by side against the owner's
favourite night edits (journal 2026-11-26, rounds 1–12): AgX punchy, raw chromatic aberrations,
chroma-only plus a non-local-means luma denoise, local contrast "clarity" twice and diffuse
"local contrast | fine"; white balance for slightly blue stars; color balance rgb tints for the
dark sky and the Milky Way glow; a tone curve after AgX that lifts the stars; above the horizon a
tone equalizer that darkens the sky between the stars; below it a brightness step and a color
calibration for the foreground. Exposure per photo for a sky median of 0.065; the owner raises
single photos with the exposure slider. The self-test checks the tool's edits of the reference
set's night photos like the day edits, and the upgrade block of US-9 applies.

**Notes:** The look to aim for is the owner's edits of DSC00461 (2018), _DSC0905, _DSC0909,
_DSC0914, _DSC0926 (2018) and _DSC4743 (2020) — their result, not their editing steps. The
foreground targets came from those edits; a rule for photos without such an edit is still open.
The horizon line comes from US-23. A daytime long exposure with an ND filter counts as night by its
exposure (US-7); the owner edits those rare photos by hand.

### US-22 — Dehaze proposal for long-range views

**Backlog ⏳** — As the owner, I want diffuse or sharpen "dehaze" proposed where a photo shows
distant mountain ranges, so I don't have to switch it on by hand on every long-range view.

**Notes:** US-4 found that "dehaze" helps long-range views (separates distant layers, thins the
blue veil); no rule tells such views apart yet. Until then the owner switches dehaze on per photo
with a tag (US-9); those switches are the data to find a rule from.

### US-23 — Horizon line per photo

**Planned 📋** — As the owner, I want the tool to find where the land meets the sky in each photo,
so that sky and foreground can be treated separately without me drawing masks.

**Notes:** Needed by the aurora and night edits (foreground correction, sky-only steps: US-10,
US-21) and by the gradient filter proposal (US-11). In the experiments so far the horizon was set
by hand per photo, as a straight line; a straight line fits poorly where a peak rises into the sky
(DSC00461, journal 2026-11-26). Worked out by an analysis on the owner's photos first.

## Panoramas

### US-12 — Panorama sets

**In progress 🚧** — As the owner, I mark panorama sets in darktable, and the tool proposes sets I may
have missed.

**Acceptance criteria:** [us-12-panorama-sets.feature](../features/us-12-panorama-sets.feature)

**Notes:** Proposals are consecutive photos at most 10 s apart with the same focal length (compared
in whole millimetres, rounded); exposure
brackets belong to their panorama's set. The source photos of past panoramas no longer exist (the
owner deletes them after stitching), so the threshold is the owner's, not fitted. Used only
through the to-do panel (US-14).

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
affected photos; actions are buttons. For the selected photos, buttons switch the day look (AgX /
sigmoid) and dehaze (on / off) and start the tool on them again — the same tags the owner can
also set by hand (US-9). A folder without open steps shows as done. Processing starts
automatically after an import (can be switched off).

**Decisions:** US-14 → ADR-0001, ADR-0002

### US-15 — Status across the collection

**Planned 📋** — As the owner, I get a list of all folders with open steps, so nothing is
forgotten between editing sessions.

**Acceptance criteria:** A terminal command lists every folder with open steps and what they are.

**Decisions:** US-15 → ADR-0002

### US-16 — Optional export step

**Planned 📋** — As the owner, I export a finished folder with the right print preset without
having to know which edits already contain sharpening.

**Acceptance criteria:** Exporting is an open step in the panel (optional, like the rest of the
folder's steps). Its export action picks the preset per photo: "Fotobuch (neu)" for edits without
sharpening, the old "Fotobuch" for edits that contain sharpening. A manual export with the new print
style of a photo that already contains sharpening triggers a warning in darktable.

**Notes:** Whether the Lua layer can see the selected export style during a manual export is
still to be checked.

### US-17 — Installation on the owner's machine

**Planned 📋** — As the owner, I install and update the tool, its darktable script and its
darktable styles simply on Manjaro.

**Acceptance criteria:** One documented way to install and update the binary (e.g. a PKGBUILD) and
the Lua script (e.g. through darktable's script manager from the repository). The styles kept in the
repository are imported by the Lua script at darktable start when missing or outdated, under
versioned names; the owner's own styles stay untouched. Export presets, which the Lua API cannot
create, are a documented one-time manual step. The tool runs next to an open darktable and never
writes darktable's databases from outside.

**Decisions:** US-17 → ADR-0005
