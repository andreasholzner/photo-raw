# Raw-photo workflow automation (darktable + hugin)

Planning notes from a brainstorm (2026-11-11 to 2026-11-19), continued as a journal during
the exploration (2026-11-21/22). Since 2026-11-22 this is a real project: features live in
[requirements.md](requirements.md), decisions in [adr/](adr/), links in [links.md](links.md).
The sections above "Journal" are the state at the switch and are not maintained further. The
journal only records what isn't tracked elsewhere and has future value — experiments,
analyses, brainstorming — newest entry last. Story progress, decisions and usage live in
requirements.md, the ADRs, the code and the README.

## Goal

- Spend less time on **repetitive raw editing** — the area with the **biggest expected
  benefit** among the brainstormed ideas.
- Owning the data is a nice plus, not the driver; existing tools are fine, build only where
  nothing fits.

## Current workflow

- Camera RAWs edited in **darktable**, after a standardized editing workflow.
- Panoramas stitched with **hugin**.
- About **5 editing steps applied to almost every photo**, plus a few extra steps when a photo
  needs them (e.g. a gradient filter).
- **Night photos** are edited differently.
- Most steps are judged by the looks **and the histogram** → some automation seems possible.
- **Exposure target** (stated 2026-11-21): good overall brightness, no or almost no clipped
  highlights, dark parts bright enough for printing; if needed a gradient filter brings the
  sky down.
- **All kept RAWs and their darktable XMP sidecars are still there.** Photos not good enough
  to keep are deleted, and so are the source RAWs of a panorama after stitching.
- **Panorama workflow:** identify the sets by eye (same focal length, a few seconds apart) →
  apply style "PanoramaV2 (Tageslicht)" → export with preset "Panorama Teil" (TIFF to
  `panorama_exported/`) → hugin's automatic alignment (if it fails, manual alignment doesn't
  succeed either) → move the result into the trip folder, add it to the darktable collection
  → usually only sharpening, rarely a gradient filter. Assumption: once the sets are
  identified, the rest is easy to script.
- darktable config is available at `~/.config/darktable` (styles and presets in `data.db`);
  darktable 5.4.1 with `darktable-cli` is installed in the container.

## Context and preferences

- Prefers Rust; no Python fan, doesn't want to maintain a local Python environment (stated for
  the finance project, likely applies here too).

## Approach: deduce the workflow from the sidecars

Each XMP sidecar holds the full history stack: per step the module (`operation`), `enabled`,
`modversion`, `params` (hex, or `gz…` = base64 of compressed data), blend parameters, the
instance name; plus EXIF (exposure time, ISO, lens, focal length, maybe GPS). Across years of
photos that's an exact record of what is actually done — better than describing it from memory.

1. **Module usage (easy):** which modules, how often, in which order; per period and per photo
   type (day vs. night via exposure time / ISO). Probably already reveals the ~5 standard steps.
2. **Chosen values (harder):** decode `params` per module and version — layouts from the
   parameter structs in darktable's `src/iop/*.c` → value distributions (e.g. exposure EV,
   sigmoid/filmic contrast).
3. **Histogram rules (hardest):** relate chosen values to statistics of the unedited rendering
   (e.g. histogram percentiles) → candidate rules like "exposure so the brightest 1% lands just
   below clipping". This is what would automate the histogram-based decisions.

**Caveats**
- The history contains undone experiments and modules darktable applied automatically
  (compare against module defaults).
- darktable changed modules and parameter layouts over the years → start with recent years,
  which also reflect today's editing.

## Automation options (once the workflow is known)

- **Auto-applied presets:** darktable module presets can apply automatically on EXIF
  conditions (ISO, exposure time, lens, focal length) → a base preset for every photo, a night
  preset for long exposures / high ISO.
- **Histogram-driven values:** either a tool computes the histogram and writes module
  parameters into the XMP sidecar (binary layout per module version, fiddly), or — cleaner — a
  **Lua plugin** does it inside darktable.
- Other darktable tools: styles, `darktable-cli` (batch export with a style / XMP), Lua API
  (hooks on import/export, auto-apply styles, tagging).
- **hugin:** fully scriptable CLI chain (`pto_gen` → `cpfind` → `cpclean` → `linefind` →
  `autooptimiser` → `pano_modify` → `hugin_executor`); panorama sets detectable by short time
  gaps + same exposure and focal length.

## Next step

See [requirements.md](requirements.md).

## Open questions

- Which base pipeline from now on: back to display-referred (as in 2023–2025, the basis of
  the exposure rule), or try scene-referred with sigmoid? Since the 2026 darktable upgrade
  `sigmoid` sits in nearly every history next to the contrast tonecurve (see journal).
- Aurora: should weak-aurora shots get a lower brightness target than strong ones? Weak
  ones are culled anyway (01-11), but the aurora trip 3 shots (aurora over a lit foreground)
  were too bright at 0.24 → maybe the target should depend on the scene, not one value.
- Aurora foreground: snow picks up a green cast (_DSC8300) that a sky-based WB can't fix.
  Needs sky/foreground separation — parametric mask (automatable) or drawn mask (manual,
  as done in older edits).

## Journal

### 2026-11-21 — Analysis 1 (module usage) on a sample

**Data setup**
- Photos mounted at `/home/ubuntu/photos`, one folder per trip, named `YYYY-MM-DD <name>`
  (two 2019 folders without day). Folders contain only the kept photos (`.ARW` + `.ARW.xmp`),
  plus hugin outputs (`<first> - <last>.tif` + `.tif.xmp`) and, since 2024/2025,
  `fotobuch_exported` / `web_exported` subfolders with JPGs.
- 2026 folders skipped (not all fully processed).
- Volume: 2022: 151 raw sidecars, 2023: 276, 2024: 208, 2025: 103.
- The sidecars **don't contain exposure time / ISO / lens** (only `DateTimeOriginal`) → read
  from the ARW EXIF.
- All 587 raws of 2023–2025: Sony **ILCE-6000**; lens **E PZ 16-50mm** except 12 with lens
  `----` = a **manual 12mm f/2.0** used for night shots.
- No exiftool, darktable or Rust toolchain in the container → throwaway stdlib-only Python
  scripts (not kept).

**Sample:** 42 day photos (2 random per 2023–2025 folder) + 8 night photos (exposure ≥ 1 s or
manual lens). Only the active history (entries below `history_end`, last entry per module
instance) was counted.

**Day photos — the 5 standard steps** (plus darktable's automatic pipeline modules, and
lens correction + denoiseprofile on nearly all):

| Step | Module | Used | Values |
|---|---|---|---|
| 1 | white balance (`temperature`) | 42/42 | nearly constant R≈2.8 B≈1.7; 2025 exactly 2.80/1.67 |
| 2 | `exposure` | 38/42 | always **positive**, +0.5 … +3.4 EV, median ≈ +1.5 — the only real per-photo value |
| 3 | contrast curve, built-in "Kontrast – mittel/hoch" preset | 42/42 | `tonecurve` 2023, `rgbcurve` 2024, `tonecurve` 2025 |
| 4 | `hazeremoval` | 33/42 | only two settings: default 0.20/0.20 or 0.40/0.25 |
| 5 | `sharpen` preset "mehr schärfen" | 41/42 | always radius 3.0, amount 1.2, threshold 0.5 |

- Extra steps: `graduatednd` on ~⅓ (density ≈ 1.0, range 0.8–1.8, almost always rotation 0 →
  sky darkening); `retouch` (spots) on ~⅓; occasional crop / `ashift`; `colorbalancergb`
  mostly in 2023; a short scene-referred try (filmicrgb / channelmixerrgb) in July 2023,
  dropped again.

**Existing automation:** style **"Anfangsautomatik"** appears in 2024 (17 photos) and is on
**all 103 photos of 2025**; also a "PanoramaV2 (Tageslicht)" style. In 2025 the stack is
identical everywhere (lens/denoise → exposure → WB → tonecurve → haze → sharpen), and
**exposure is adjusted on every photo** (+1.2 … +1.7). *(Corrected in the next entry: the
style only covers lens + denoise; the fixed WB/haze/sharpen values come from hand-clicked
presets.)*

**Night photos** (manual 12mm f/2.0, 1–30 s, ISO 100–400):
- Lens correction set by hand to "12mm, f/2.0".
- WB picked per photo, varies widely (R 2.0–3.4).
- Extra modules: `rawdenoise` "minimal", `denoiseprofile` "Wavelets: nur Chrominanz", local
  contrast `bilat` "Klarheit" (often twice), `colorbalancergb`, `highpass` sharpening.
- Exposure always positive (+0.4 … +2.2), sometimes in 2–3 instances.
- Long histories with many tried-and-disabled modules (nlmeans, lowlight, bilateral,
  cacorrectrgb) → no settled recipe yet.

**Conclusions**
- Day: everything but **exposure** uses fixed values → the remaining histogram-driven
  decision and the main target for analysis 3 (consistent underexposure suggests a
  highlight-based rule).
- **Graduated ND** is the next candidate: fixed density and angle, only its position varies.
- Night: an auto-applied preset (lens `----` or long exposure) could set lens correction and
  the denoise / local-contrast stack; WB and exposure stay manual for now.

### 2026-11-21 — Styles/presets, exposure rule (analysis 3), panoramas

**Styles and presets** (from `data.db`; user-defined only)
- **"Anfangsautomatik"** = only **lens correction + denoiseprofile** (description
  "Objektivkorrektur + Entrauschen"). Not WB, exposure, curve, haze or sharpen.
- So the fixed values of the 2025 stack are **presets applied by hand** each time:
  - WB: user preset **"Tageslicht"** (R 2.80, B 1.67).
  - Haze removal: darktable's built-in default 0.20/0.20, or user preset **"Default"**
    (0.40/0.25).
  - Sharpen: **"mehr schärfen"** (3.0 / 1.2 / 0.5); "noch mehr schärfen" (4.5 / 1.6 / 0.5)
    also exists.
  - Contrast: darktable's built-in "Kontrast – mittel/hoch" tonecurve/rgbcurve presets.
- Only `demosaic` "amaze" and `colorout` "adobe" are auto-applied.
- Night presets exist: lens "12mm, f/2.0" and "12mm, f/16", rawdenoise "minimal",
  highpass "schärfen".
- **"PanoramaV2 (Tageslicht)"** = Tageslicht WB + lens + denoise + sharpen "schärfen"
  (2.0 / 0.5 / 0.5) + base pipeline. Older variants: "Panorama", "Panorama (5300K/5500K/
  Tageslicht)".
- Export presets: **"Fotobuch"** (JPEG to `fotobuch_exported/`, applies style
  **"Aufhellung Druck"** = a brightening tonecurve), **"Panorama Teil"** (TIFF to
  `panorama_exported/`), "web groß", "FullHD", "Jpeg Edit", "Kopie".
- darktablerc: default workflow "scene-referred (sigmoid)", chromatic adaptation "legacy".

**Exposure rule (analysis 3)**
- Method: rendered the 50 sample photos with `darktable-cli` (isolated config copy,
  in-memory library, 800 px, PFM float output, Adobe RGB). For each photo the full final edit
  plus the main exposure instance shifted by −1, −0.5, +0.5, +1 EV (226 renders, about 4.5 s
  each). Measured output luminance percentiles and clipping. Solved per photo for the EV that
  hits a target value of a statistic, and compared it with the chosen EV.
- 37 day photos had a sweepable exposure instance (5 had no exposure module, 0 failed).
- Final day edits: luminance **p99 ≈ 0.86–0.91** (IQR), mean ≈ 0.42–0.53, median clipping
  0.2 % of pixels. A few outliers with lots of clipping (up to 21 %) are snow or white sky,
  apparently accepted.

| Rule (EV so that … in the final render) | Mean abs. error | within ±0.25 EV | within ±0.5 EV |
|---|---|---|---|
| baseline: same EV (+1.57) for all | 0.57 EV | 11/37 | 20/37 |
| mean luminance = 0.50 | 0.42 | 13/37 | 23/37 |
| **99th percentile luminance = 0.90** | **0.23** | 23/37 | **34/37** |
| 99.5th percentile of max channel = 0.95 | 0.34 | 26/37 | 31/37 |
| min(highlight target, brightness target), best pair | 0.23 | 21/37 | 34/37 |

- **Result:** a single highlight rule, "**brightest 1 % at 0.90**", reproduces the chosen
  exposure to within ±0.5 EV for 34 of 37 photos. Adding a shadow or brightness floor doesn't
  improve it: in practice the highlights limit the exposure. Largest misses: _DSC6756
  (−1.19, from the July 2023 scene-referred try), _DSC6776 (+0.59), _DSC6560 (−0.56).
- Caveats: fitted and evaluated on the same 37 photos (a single parameter, so low overfitting
  risk, but unconfirmed). The statistic is measured *after* the whole pipeline (curve, haze,
  gradient), so applying it needs an iterate-render-measure loop, or a calibrated shortcut on
  the pre-curve data.
- Night (8 photos): no common target. p99 ranges 0.24–1.10, mean 0.10–0.54; exposure stays
  manual.

**Panoramas**
- 133 panorama TIFFs in 2023–2025, named `<first> - <last>.tif`; 5 are `_fused` (exposure
  fusion?).
- Only 3 of their 494 source RAWs remain (deleted after stitching) → set detection
  (time gap + same focal length) **can't be validated on old folders**.
- Time gaps alone won't separate the sets: 196 neighbouring kept non-panorama photos are
  ≤ 30 s apart, many with the same focal length → detection should be a suggestion that is
  confirmed by hand (e.g. a darktable tag or group) before scripted stitching.

**Assessment: automating the non-panorama edits**
- Fixed steps (WB, lens, denoise, contrast curve, haze, sharpen) → one style or auto-applied
  presets; no code needed.
- Exposure → realistic via the p99 rule; correct only the outliers.
- Graduated ND → a preset with the usual density/angle; the horizon position stays manual.
- Night → preset for lens + denoise/local-contrast stack; WB and exposure manual.
- Remaining manual work: exposure check, gradient where needed, retouch/crop, culling.

### 2026-11-21 — Answers on fusion, panorama sets, workflow default, night photos

- **Exposure fusion:** done in hugin when it offers the option (depends on the overlap);
  afterwards the user picks which stitched result to keep → `_fused` TIFFs are regular
  panorama output, not a separate workflow.
- **No unstitched panorama sets left** → set detection can't be validated on existing data;
  only on future trips.
- **darktable default workflow:** "scene-referred (sigmoid)" came with a recent darktable
  upgrade; never experimented with deliberately.
  - Quick check (module counts only, 2026 folders otherwise skipped): `sigmoid` is in 11/12
    (2026-03, aurora trip 3), 19/20 (2026-05) and 9/9 (2026-09) histories,
    together with `exposure` and the contrast `tonecurve`; none in the Jan 2026 folders.
    So since about the upgrade, new imports probably get sigmoid applied automatically on
    top of the usual display-referred stack.
  - Relevance: the p99 exposure rule was fitted on the 2023–2025 pipeline (no sigmoid). A
    different base pipeline changes the tone mapping, so the rule's target would need to be
    re-fitted.
- **Night photos are mostly aurora.** Goal: recreate on print how the aurora felt, so
  **more saturation and brightness** than "accurate". Fits the night stack seen so far
  (colorbalancergb on 7/8, local contrast "Klarheit", exposure up to +2.2), but the output
  levels vary widely → a look preset seems possible, exposure probably stays manual.

### 2026-11-21 — Aurora: old edits analyzed, recipe applied to the 2026 aurora photos

**Old aurora edits** (34 aurora photos found among the 43 manual-lens long exposures up to
2025, identified on a rendered contact sheet):
- **2019/2022 (24):** basecurve + tonecurve + `colisa` (saturation +0.14 … +0.42, often
  brightness/contrast too) + per-photo WB (mostly R 3.3–3.7 / B 1.35–1.6) + rawdenoise +
  denoise + lens + sharpen. Result: **bright, vivid**, blue sky. Sky median luminance
  0.20–0.33, sky chroma (95th pct.) 0.3–0.5.
- **2024 (10):** exposure (+0.35 … +1.4) + haze "Default" + local contrast (`bilat`, often
  "Klarheit" twice) + `colorbalancergb` (mostly brilliance, saturation ≤ +0.23) + tonecurve
  "Kontrast – hoch" + chroma-only wavelet denoise + highpass. Result: **about half the
  brightness and a third of the chroma** of 2019 (sky median ≈ 0.14, chroma ≈ 0.1).
- The stated goal ("brighter and more saturated than it looked") matches the 2019 look.

**2026 aurora photos:** 30 in aurora night 1 (2026-01-11) (strong red + green aurora, first 6
weak with city lights), 16 in aurora night 2 (2026-01-23) (green arcs, moon), 2 in
aurora trip 3 (2026-03-10). Sidecars are empty (42), WB only (4), or a minimal scene-referred
start (1). The 2026-09 trip (3, kit lens) is already edited → left out.

**Recipe** ("2024 modules, 2019 levels"):
- History of _DSC7371 (2024-03-02, no drawn masks) as template: lens "12mm, f/2.0",
  rawdenoise "minimal", denoise "Wavelets: nur Chrominanz", exposure, haze "Default",
  tonecurve "Kontrast – hoch (Gamma 2.2)", 2× local contrast ("Klarheit"), color balance rgb,
  highpass.
- Color balance rgb set to chroma +0.20, saturation +0.12, vibrance +0.15.
- **Per photo, solved iteratively with darktable-cli renders (300 px):**
  - **exposure** so that the **sky median luminance = 0.24** (sky = upper 55 % of the
    frame);
  - **WB** so that the **sky background** (25–50 % luminance band of the sky) has
    R/G = 0.85, B/G = 1.25 in the output, i.e. a dark, slightly blue night sky. A fixed WB
    (R 3.3 / B 1.55, the old median) gave an olive/brown sky from light pollution; the first
    solver version (darkest pixels, linearized) diverged.
- New sidecar = the photo's own sidecar (rating/tags/labels kept) with the history replaced
  and the iop order taken from the template.

**Output:** `/home/ubuntu/photo-edits/<trip folder>/` holds `<name>.ARW.xmp` + full-size
`<name>.jpg` for 48 photos; contact sheet `/home/ubuntu/photo-edits/_overview_aurora_2026.png`.
To use a result in darktable: copy the `.xmp` next to the raw (replacing the empty sidecar)
or load it via history stack → "load sidecar file". The scripts were throwaway
Python (local `experiments/`, not part of the repo).
- Results: solved EV −2.0 … +2.2, WB R 2.0–2.9 / B 1.8–2.2. Consistent series look.
- Known weak spots:
  - The 6 weak-aurora/city shots are lifted to the same sky brightness → grey and noisy.
  - _DSC8293/_DSC8294 (moon) hit the −2 EV limit.
  - In 3 photos filled with bright aurora (_DSC8285/8288/8297) the background band is
    aurora-tinted, so the sky ends up a bit greener than targeted.
  - Judged only at thumbnail size: noise and halos from local contrast + highpass still need
    checking at 100 %.

**Takeaway for automation:** night/aurora editing reduces to a fixed module stack plus
**two per-photo rules (sky brightness, sky-background WB)**. Both are histogram-type rules
of the same kind as the day exposure rule.

### 2026-11-22 — Aurora review feedback, less-green variants

**Feedback on the first run:**
- Overall good in both aurora nights, little or no visible noise, but **all a bit too
  green**. Favourites: _DSC8257 (01-11), _DSC8287/8292/8295/8296/8297 (01-23).
- _DSC8234–_DSC8239 (weak aurora, city lights) discarded → removed from `photo-edits`.
- aurora trip 3 _DSC8300 and _DSC8305: **too bright, obvious noise**. 8305 colours right; 8300
  snow greenish (but the stars came out well).
- Workflow facts: for night/aurora shots the user culls *after* a first processing pass
  (unlike day shots), so batch-processing everything first is useful. Varying shutter time
  within a series was intentional. Older edits used **drawn masks** to treat sky and
  foreground differently where parametric masks weren't enough.

**Variants** (favourites + aurora trip 3, same recipe, only the sky-background target changed;
aurora trip 3 also sky median 0.15 instead of 0.24):

| Variant | background R/G | background B/G | look |
|---|---|---|---|
| A (first run) | 0.85 | 1.25 | blue-cyan, greenish |
| B | 0.95 | 1.35 | neutral blue |
| C | 1.05 | 1.50 | blue-violet |

Output: `/home/ubuntu/photo-edits/_variants/{B,C}/<trip folder>/` (JPG + XMP), side by side
in `_variants/_compare_A_B_C.png` (columns A, B, C). Measured: share of clearly green pixels
drops e.g. _DSC8287 8 % → 5 % → 2 %, _DSC8296 15 % → 10 % → 6 %; aurora trip 3 frames stay
greenish overall (whole-image R/G ≈ 0.7) because the foreground snow carries the cast — the
sky-based WB doesn't reach it.

**Chosen** (user, after viewing full size; criterion mostly snow discoloration in the
foreground): **B for 2026-01-11, C for 2026-01-23**, aurora trip 3 B ≈ C, both good with the
darker target 0.15. → The sky-colour target is a per-session (per-night) setting, not a
constant. All 42 remaining aurora photos re-rendered accordingly into
`/home/ubuntu/photo-edits/<trip folder>/` (first-run A results replaced); overview sheet
regenerated. `_variants/` kept for reference.

### 2026-11-22 — Tooling direction (proposal, not decided)

How the findings turn into tooling:
- **Fixed steps → darktable styles** ("Tag", "Aurora"), incl. parametric sky/foreground
  masks where they work. Useful without any code.
- **Per-photo values → Rust CLI** (`photoflow`), rewriting the throwaway Python
  prototypes: render with darktable-cli → measure → adjust → write sidecar.
  Subcommands roughly: `day <folder>` (exposure p99 = 0.90), `aurora <folder> --sky B|C`
  (sky brightness + sky-background WB), `pano <folder>` (sets marked by tag/label in
  darktable, optional suggestions from time gaps + focal length; style export → hugin chain →
  result into trip folder), `sheet <folder>` (contact sheets / variants for culling).
- **darktable integration pitfall:** the library DB is authoritative, externally written
  sidecars aren't picked up. Options: small Lua script ("run photoflow on selection", then
  `image:apply_sidecar`) or run while darktable is closed + "look for updated XMP files on
  startup".
- Stays manual: culling, drawn masks, retouch, gradient filter decisions.
- Suggested order: settle base pipeline → styles → aurora in Rust → day exposure → Lua
  button → panoramas.

### 2026-11-22 — Can the sky-colour choice be automated? (first check: not yet)

Measured the foreground snow (bottom 20 % of the frame, brighter half) in the A/B/C renders
(throwaway script):
- Lowest snow colourfulness picks **C for _DSC8257 and B for most of 01-23**, i.e. the
  **opposite** of the user's choice (B for 01-11, C for 01-23). Neutral snow is not the
  criterion as such.
- Chosen snow colours: 01-11 (_DSC8257, B) R/G 0.83 / B/G 1.24; 01-23 (C) R/G 1.00–1.15 /
  B/G 1.25–1.43. Not one fixed target either. Plausible reasons: aurora/moon light really
  tints the snow differently per night; only 1 compared photo for 01-11.
- Too little data to derive a rule (7 photos, 2 nights). Practical approach for now: the
  tool renders A/B/C for a few representative photos per night and asks **one choice per
  night**; the choices are recorded and become the data for a rule later. Possible
  experiment: WB solved on the snow instead of the sky background.

### 2026-11-22 — Day vs. night from EXIF

Scene brightness from EXIF, EV100 = log2(f²/t) − log2(ISO/100), for all 2103 raws (manual
12mm lens reports no aperture → f/2.0 assumed, so ±1–2 EV uncertain there):
- **Clearly bimodal:** 130 photos at EV ≤ 1 (night), 1924 at EV ≥ 8 (day), only **49 (2.3 %)
  in between**.
- The in-between ones are real edge cases: blue hour/dawn (05:30 in 2018, 20:20 in 2020),
  fireworks (New Year 2024, 00:59), **daytime ND long exposures** (New Year 2024, 10:30, 30 s f/16 →
  EV 3, would look like night), dim interiors (ISO 3200 at 11:39).
- Rule proposal: EV ≤ 1 → night, EV ≥ 8 → day, between → ask (or extra signals). Extra
  signals: time of day / sun elevation (needs a location per trip; camera has no GPS),
  lens (manual 12mm = night lens, but also used by day), a quick render (dark sky?).
- Night ≠ aurora: of the 43 older long exposures only 34 were aurora. If the night recipe
  should only hit aurora, a second check (green/red hue in the sky on a small render) is
  needed.

### 2026-11-22 — Requirement: seamless and hard to get wrong

**User requirement:** edits photos only occasionally → will forget manual extra steps. The
tooling must *tell* what is still open; a few external tools controlled from darktable
seem the right shape.

**Design take (proposal):**
- **The tool keeps the to-do list, not the user's memory.** Open decisions are stored as
  darktable tags on the images (e.g. `photoflow|todo|sky-colour`,
  `photoflow|todo|pano-confirm`, `photoflow|todo|day-or-night`), so the state survives
  restarts and months-long breaks.
- **Thin Lua layer in darktable, logic in one Rust CLI.** Lua: on-import hook, a side panel
  "photoflow" listing the open decisions of the current film roll (click → collection of the
  affected images), buttons that call the CLI, then `apply_sidecar` so the library stays in
  sync. Rust: classify, edits, panorama chain, status.
- **Flow:** import → automatically classify + base edits + proposals → panel lists open
  decisions → user decides/culls/fine-tunes → "stitch confirmed panoramas" → results
  imported → panel says "folder done". Plus `photoflow status <photos dir>` listing all
  folders with open steps.
- **Safety rules:** never overwrite a manually changed history (marker tag + history check);
  everything reversible via darktable history; idempotent re-runs; fail loudly instead of
  guessing.
- **Risks to verify first:** Lua API coverage (import event, side panel, `apply_sidecar`,
  background jobs without blocking the UI); darktable upgrades changing module param layouts
  → build edits from darktable's own styles/presets and check module versions, plus a
  self-test after each upgrade.

### 2026-11-22 — Is the darktable-centric design realistic? Lua API check

- Installed darktable 5.4.1 is built with **Lua (API 9.6.0, Lua 5.4)**. The library contains
  the needed entry points: `post-import-film` / `post-import-image` events,
  `register_lib` + `new_widget` (side panel), `create_job` (background job with progress
  bar), tag attach/detach, `apply_sidecar`. Existence only — behaviour (e.g. UI staying
  responsive while an external command runs) still needs a GUI prototype.
- The user's config has no Lua scripts in use yet (`lua/_scripts_install/*` keys only).
- **Version note:** the config dir contains `data.db-pre-5.6.1`, i.e. the user's own
  darktable was probably upgraded to 5.6.x, while this container has 5.4.1. Develop and test
  against the version the user actually runs.
- Separate-app alternative (app owns all state, calls darktable-cli): rejected for now —
  culling and fine-tuning still happen in darktable, so there would be two places holding
  photo state (the thing the user wants to avoid), plus a browsing/culling UI to rebuild.
  Its one advantage (no dependency on the Lua API) is kept by making the Rust CLI usable on
  its own (`photoflow status`, etc.); Lua stays a thin layer.

### 2026-11-22 — darktable 5.6.1 installed; does the minor version matter?

- User runs **darktable 5.6.1 on Manjaro**. Installed the official 5.6.1 AppImage, extracted
  to `/opt/darktable-5.6.1`; `/usr/local/bin/darktable-cli` → 5.6.1 (system 5.4.1 package
  still at `/usr/bin`). Config copy for 5.6.1: scratchpad `dtcfg56/`.
- **Render comparison 5.4.1 vs 5.6.1**, same sidecars, 600 px:
  - 6 day photos: 5 bit-identical in 8-bit terms, 1 at mean 0.8 / p99 5 levels.
  - 2 older night photos: mean ≤ 0.9 levels.
  - 3 new aurora edits: _DSC8257 0.4 levels, _DSC8305 2.2, **_DSC8292 5.2 (p99 23) —
    visibly brighter/redder in 5.6.1**.
- **Cause of the _DSC8292 difference** (disable one module at a time, compare versions):
  the difference disappears when *any* of lens / exposure / WB / **haze removal** is
  disabled, but not with denoise disabled. So a tiny upstream change (most likely lens
  correction) gets **amplified by haze removal**, which estimates its effect from image
  statistics (here: moon + city lights). The darktable manual itself says haze removal is
  meant for real haze and fails on images without it.
- **Takeaways:** minor versions barely matter for the day stack. Unstable, statistics-based
  modules (haze removal on night shots) are the risk → drop haze from the aurora recipe, keep
  the tool's self-test (render a reference set, compare) for upgrades. Module param formats:
  darktable upgrades old module versions in sidecars itself, so sidecars written with the
  template's module versions stay readable.

### 2026-11-22 — Feedback on the editing steps (vs. common practice) and plan review

**Day stack vs. common practice / darktable docs:**
- Consistent, simple display-referred stack — fine. darktable itself recommends
  scene-referred (exposure + one display transform: sigmoid / filmic / AgX).
- **2026 histories have sigmoid *and* the "Kontrast – hoch/mittel" tonecurve** → two layers
  of tone mapping/contrast. Docs: use only one display transform. → part of the base-pipeline
  decision.
- Fixed "Tageslicht" WB: a legitimate choice (keeps golden hour warm, consistent series);
  shade/overcast will look cool.
- Exposure always +1.5 EV: normal (camera protects highlights; scene-referred default adds
  +0.7 EV automatically). The p99 rule = "expose to the right" in post.
- **Haze removal on most photos** is unusual: the manual describes it for real haze and warns
  it fails otherwise; local contrast is the suggested alternative for general punch.
- Sharpen 3.0/1.2/0.5 on every photo is fairly strong; many sharpen per output (print vs web).

**Aurora vs. common practice:**
- "How it felt" over accuracy is common; the usual warning is over-saturation (neon green),
  consistent with the "too green" feedback. Sky colour varying per night matches common
  advice (aurora strength changes sky colour).
- Noise: the usual extra techniques are stacking several frames and a separate (longer)
  foreground exposure blended in. Capture-time choices; optional later.
- 2× local contrast + highpass + haze on night shots amplify noise; haze is also the unstable
  module (see 5.6.1 entry).

**Plan review — gaps found:**
1. **Export/print is missing from the flow** ("Fotobuch" with "Aufhellung Druck", "web groß",
   …). Print is the goal → export (and a print check) should be part of "folder done".
2. **Destructive steps** (deleting panorama sources and culled photos) must stay manual /
   confirmed; the tool only lists them as to-dos.
3. **Existing and half-done folders:** the tool must only touch photos it was asked to, and
   detect existing manual edits (2026 folders are mixed).
4. **Regression test set:** the old edited photos are ground truth → reference renders +
   self-test after darktable upgrades.
5. **Day culling happens before editing, night culling after** → the flow differs by type.
6. **Gradient ND** (on ~⅓ of day photos) dropped out of the plan → later candidate
   (proposal + confirm).
7. **Panorama details:** hugin's fused vs. non-fused choice becomes a to-do decision; result
   import + sharpening as part of the chain.
8. **Runs on the user's Manjaro machine** (GPU/OpenCL there; performance unknown), packaging
   of the Rust binary + Lua script, darktable-cli next to the running GUI (isolated
   configdir).
9. **Day exposure rule** fitted on 37 photos only → validate on a fresh, finished folder.
10. **Project setup:** repo layout, where this journal goes (docs), first decisions as ADRs
    (proposed first, then written).

### 2026-11-22 — Switch to a real project

User answers on the plan review:
- **Base pipeline:** interested in switching to the current scene-referred approach; the old
  stack was kept only because it worked.
- **Haze removal:** works really well on the many mountain photos; on aurora it also gave good
  results; a habit rather than a deliberate choice. Open to a better way.
- **Sharpening** is aimed at print (main output) → can live in the print export.
- **Sky colour per photo** rather than per night: aurora changes substantially within minutes.
- Export: optional step, low priority (one action with the existing preset).
- With more automation, day photos can also be culled after editing.
- Gradient filter proposals: wanted. Fused vs. normal hugin result: start with normal, fused
  choice as a later to-do. Speed: unimportant if unattended. Many edited photos available
  for more analysis.
- Project layout: this workspace (empty git repo) is the project. `docs/requirements.md`
  (features), `docs/adr/` (decisions), links documented (`docs/links.md`); this file moved to
  `docs/journal.md`.

### 2026-11-22 — Experiments not versioned

The throwaway Python scripts stay local in `experiments/` (git-ignored): they point to the
session scratchpad and aren't part of the tool (ADR-0005). Findings and numbers live in this
journal.

### 2026-11-22 — US-1: Lua spike, first results in the container

Spike script and guide: `spikes/us-1-lua/`. Tested in the container with darktable 5.6.1
(Lua API 9.7.0) under a virtual display (Xvfb), throwaway config and library, copies of 3 raws
(one edited, two unedited), driven by an unattended self-check mode of the script.

- **Works:** panel in the lighttable; `post-import-image` / `post-import-film` events; EXIF
  exposure/aperture/ISO (EV100 14.0 for the day raw, −2.0 for the aurora raws); creating,
  attaching, detaching, deleting tags; filtering the collection to a tag and restoring it;
  `is_altered` tells edited from unedited images; `dt.control.dispatch` + `dt.control.execute`
  run a 10 s external command with a progress job; **`apply_sidecar` of a file written
  outside darktable puts the full history into the library** (verified in library.db: 16
  history items).
- **API facts learned:** no Lua call to write sidecars (darktable writes them itself, 5.6
  asynchronously); no Lua call to quit darktable; the bundled lua-scripts in 5.6.1 (official,
  contrib, examples) are good references for current API usage.
- **Open, needs the owner's machine:** UI responsiveness during the background job and
  cancelling; looks of the panel; whether darktable rewrites the target's `.xmp` after
  `apply_sidecar` (in the container it didn't, but darktable was killed, not quit). If it
  doesn't, sidecar files can lag behind the library — acceptable under ADR-0001 (the library
  is the home of the state) but to be kept in mind.

### 2026-11-22 — US-1: spike on the owner's machine (darktable 5.6.1, Manjaro)

Log from the owner's run, plus their observations:
- **Confirmed:** panel; import events for both a non-test folder (correctly not tagged —
  the safety check on the folder name works) and the test folder; EXIF/EV100; tags as
  to-dos, collection by tag and restore, detach; background job (10 s, finished);
  `apply_sidecar` — the image changed visibly, history in darkroom; `is_altered`.
- **Sidecar file lags the library:** the target's `.xmp` still had `history_end=0` 5 s after
  `apply_sidecar`, and 16 about 2½ minutes later. darktable writes sidecars in the background
  → the tool must not expect a sidecar to reflect a change it just applied; the library is
  the source of truth (ADR-0001).
- **Locale:** Lua's `string.format("%.1f")` printed `8,0` (German locale) → numbers exchanged
  between the Lua layer and the CLI need locale-independent formatting.
- **Panel UI** is not suited for long log lines; the real panel shows a short list of open
  steps and buttons, details go to a log file.
- **Open:** the second background job (23:11:22) logged no end — if it was cancelled, the
  cancel path probably raised an error. Asked the owner whether they cancelled it and
  whether the UI stayed usable during the jobs.

**Answers (same day):** the owner cancelled the second job and its progress bar disappeared;
darktable stayed usable during both jobs. Cause of the missing "cancelled" log, reproduced in
the container: after a cancel, reading `job.valid` works, but setting `job.percent` raises
"Accessing an invalid job" — the cancel arrived during a step, the progress update threw and
ended the function before the log line. → The real tool checks `job.valid` before every
progress update.

**US-1 result: passed.** Every capability the darktable-centric design needs works on the
owner's darktable 5.6.1. ADR-0001 and ADR-0002 proposed for acceptance.

### 2026-11-22 — US-2: reference set and the first Rust code

**Repository cleaned of trip folder names** (owner's wish): earlier journal entries rewritten
with neutral terms (aurora night 1/2, aurora trip 3, years instead of places) in all commits.
File names stay; they are harmless.

**Reference set:** 30 photos chosen from a scan of all 2663 edited sidecars and checked on a
contact sheet: 6 day photos of the current stack (incl. the two worst exposure-rule outliers),
6 mountain/haze, 7 aurora, 5 night/twilight/edge cases, 3 panoramas, 4 scene-referred (filmic
2023, sigmoid 2026). The owner dropped the kit-lens aurora (the kit lens won't be used for night
shots again) and replaced a family photo with a dim interior (ISO 1000). Stored as file name +
folder year in `data/reference-set.toml`.
- **Correction:** the 30 s exposure at 10:30 on 1 January (listed as "daytime ND long exposure"
  in the day/night entry) is polar-night twilight in Tromsø, not an ND shot — the EV rule
  classified it correctly.
- **File names are unique today** (2645 raws/panoramas, no duplicates), but the camera counter
  (now ≈ _DSC8370) wraps after _DSC9999 and will repeat 2018 names → photos are identified by
  file name + folder year; the tool must report ambiguities instead of guessing.

**`photoflow selftest` (init / run / accept):** renders the set with darktable-cli (private copy
of the owner's darktable configuration with sidecar writing disabled, in-memory library, sources
via symlinks) to 1000 px 8-bit PNGs in `~/.local/share/photoflow/selftest/`, compares per photo
(mean / 99th percentile / max difference in 8-bit levels; tolerance mean 1.0, p99 8 —
configurable). First run in the container: 30 renders in 2:50 min, 28 MB.
- Same darktable twice: all pass, but **renders are not bit-identical** — two photos show small
  noise (mean 0.08–0.13, max up to 14 levels), so a tolerance is necessary.
- With darktable 5.4.1 against 5.6.1 references: 5 of 30 flagged — the 2023 panorama TIFF
  (mean 7.5 levels), fireworks (p99 41), dim interior, stars, one aurora — and the version change
  is reported.
- `darktable-cli` with an in-memory library doesn't write sidecars even with "write sidecar
  files: on import" (163 renders, none written); the tool disables it anyway.

### 2026-11-23 — Experiment: research-based aurora edit vs. the current recipe

Two aurora-trip-3 photos (_DSC8300, _DSC8305), darktable 5.6.1, full size. Same per-photo
targets for both edits (sky median 0.15, sky background B), so only the recipe differs.
- **Current recipe** (from the owner's old aurora edits): display-referred, "Kontrast – hoch"
  curve, raw denoise minimal + chroma-only denoise, haze removal, local contrast twice,
  highpass, colour balance rgb pushed (chroma +0.20, saturation +0.12, vibrance +0.15).
- **Research recipe:** modules of a finished 2026 scene-referred edit (exposure, sigmoid
  "scene-referred default", profiled denoise luma + chroma), the owner's 12mm lens preset,
  built-in "basic colorfulness | standard", local contrast "clarity" once; no haze removal,
  no tone curve, no sharpening (print sharpening belongs in the export, US-5).
- **Variant with foreground filter:** built-in tinted purple graduated filter, density 0.6 EV,
  rotated to the bottom, as a crude stand-in for a drawn foreground mask.

**Result** (side by side incl. 100 % crops in `photo-edits/_research/`):
- **Noise:** by far the biggest difference. The current recipe is heavily noisy at full size
  (chroma-only denoise, then haze + 2× local contrast + highpass amplify it); the research
  recipe is clean at 100 %.
- **Colour:** research recipe is more natural — aurora softer green, snow neutral to blue
  instead of green — but also visibly less vivid; for "how it felt" it probably needs more
  colour (e.g. the "vibrant colors" preset).
- **Stars:** fainter with luminance denoise — the owner liked the stars in _DSC8300.
- **Foreground filter:** helps the green snow in _DSC8300, overdone (purple) in _DSC8305 →
  a fixed filter is too crude; per-photo strength or a proper mask needed.
- 8300 without filter hit the WB blue clamp (3.0): background B/G 1.24 instead of 1.35.
- **Owner's verdict:** noise better, but the remaining noise is coarser-grained, and the aurora
  loses much of its structure compared to the current recipe. Interesting, but no quick win;
  parked.

### 2026-11-23 — US-3, round 1: one display transform instead of the display-referred stack

16 day/mountain photos of the reference set (incl. the 4 already scene-referred ones),
darktable 5.6.1. Each owner edit with its tone mapping removed (base curve, tone/rgb curve,
filmic, sigmoid, colisa) and exactly one display transform added, iop order v4; exposure
solved so that the brightest 1 % lands where it does in the owner's edit (the exposure rule).
Candidates: sigmoid "scene-referred default", sigmoid "ACES 100-nit like", AgX
"blender-like|base", AgX "blender-like|punchy". Sheets: `photo-edits/_us3/sheet_1..4.png`
(columns: owner's edit, then the four candidates).
- **Sigmoid (both presets): close to the owner's look**, typically +0.3 … +1.8 EV more
  exposure for the same highlights and slightly brighter midtones; ACES-like a bit more
  contrast. Old photos lose the colisa saturation boost (tulips visibly duller).
- **AgX at the same highlight placement: flat and bright** (median luminance +0.1 … +0.25,
  EV +1.2 … +3.8) — its strong highlight roll-off means the p99 rule pushes it too far; base
  washed out, punchy better. Matching AgX on midtones instead would be the fairer test.
- In the 2026 edits sigmoid sits together with a "Kontrast – mittel" tone curve; sigmoid
  alone (ACES-like) already reaches a similar contrast.
- **Owner's verdict on round 1:** AgX flat and very bright in general, but in backlit scenes
  ("Gegenlicht") better than any alternative; for normal light sigmoid ACES-like is the best
  new option. The owner's edits have more punch (saturation), wanted for print because paper
  dulls colours.
- **Bug in round 1:** a second instance of a module (e.g. a second gradient filter or exposure)
  is ignored by darktable unless the sidecar carries an explicit `iop_order_list` containing it
  → _DSC5028 and _DSC8343 lost instances in round 1. Fixed in round 2 by always writing the
  complete order list (from a 2026 edit) with extra instances inserted after their module.

### 2026-11-23 — US-3, round 2: saturation for sigmoid, AgX matched on midtones

Same 16 photos; sheets `photo-edits/_us3/round2_sheet_1..4.png`, columns: owner's edit,
sigmoid ACES, + colour balance rgb "basic colorfulness | vibrant colors", + the same with
chroma +0.25, AgX base and AgX punchy with exposure matched on the owner's median luminance.
Colourfulness relative to the owner's edits (mean over photos; median / 90th percentile of
per-pixel max−min, midtones = median luminance):

| | colour (median) | colour (90th pct.) | midtones |
|---|---|---|---|
| sigmoid ACES | ×1.08 | ×0.92 | ×1.08 |
| + vibrant colors | ×1.40 | ×1.11 | ×1.07 |
| + vibrant + chroma 0.25 | ×1.76 | ×1.41 | ×1.06 |
| AgX base, midtones matched | ×0.86 | ×0.68 | ×1.00 |
| AgX punchy, midtones matched | ×1.35 | ×1.06 | ×0.99 |

- Sigmoid ACES + "vibrant colors" is closest to the owner's punch; the extra chroma is clearly
  too much (deep blue skies, neon greens).
- Matched on midtones, AgX is no longer too bright; base stays flat and dull, punchy comes
  close to the owner's colourfulness with a softer contrast — the candidate for backlit scenes.
- **Owner's verdict on round 2:** extra chroma too much; AgX base always too dull. The two good
  candidates — sigmoid ACES + "vibrant colors" and AgX punchy (midtones matched) — are very
  near the owner's edits, and which is better depends on the scene:
  - darker scenes → AgX punchy (dark parts brighter);
  - winter/snow → sigmoid (AgX punchy too dark there);
  - the few strongly backlit examples → sigmoid ACES + vibrant (contrary to the earlier
    impression that AgX wins in backlight);
  - _DSC6560 (green mountain lake) and _DSC6494 (stream, green foliage) → AgX punchy;
    sigmoid + vibrant comes out too colourful there;
  - tulips: no candidate comes close to the owner's edit — accepted as an exception.

### 2026-11-23 — US-3: can a rule predict the owner's look choice?

Owner's picks (round-2 sheets, row by row): 1: A S S S, 2: S S S A, 3: A S A –, 4: A S A S
(A = AgX punchy, S = sigmoid ACES + vibrant colors, – = tulips, none fits) → 9 × S, 6 × A.

Measured per photo: snow share (bright, low-chroma pixels), dark share, green share, luminance
quartiles of the owner's edit; share of strongly coloured pixels in the sigmoid + vibrant render.
- **No single feature separates the picks.** The expected "too colourful → AgX" doesn't hold as
  measured: the photos with the most strongly coloured pixels in the sigmoid version (mostly
  blue sky) were picked S.
- Best simple candidate: **S if snow share > 0.14 or strong-colour share > 0.14, else A** →
  12 of 15 right (misses: _DSC5677, _DSC5719 picked S; _DSC4805, bright rock, picked A).
  Reading: blue-sky and snow scenes → sigmoid; scenes dominated by foliage, rock or shade →
  AgX. Thresholds fitted on the same 15 photos → not validated; needs more labelled photos.

### 2026-11-24 — US-4: haze removal vs. alternatives on the new looks

Same 16 photos, each in the look the owner picked in US-3 round 2 (tulips: sigmoid). Exposure
solved once per photo without any haze step and used for all variants, so each column shows
only the module's own effect. Sheets `photo-edits/_us4/haze_sheet_1..4.png`; columns: owner's
edit, owner's haze removal kept (0.2/0.2 or 0.4/0.25), none, "diffuse or sharpen: dehaze |
default", local contrast "clarity".

| | midtones vs. owner | colour vs. owner |
|---|---|---|
| owner's haze removal kept | ×1.03 | ×1.42 |
| none | ×1.12 | ×1.28 |
| diffuse "dehaze" | ×1.13 | ×1.33 |
| local contrast "clarity" | ×1.13 | ×1.27 |

- Haze removal has by far the strongest effect: darker midtones, more saturation, deep blue
  distant ranges — the look the owner is used to. Diffuse "dehaze" and "clarity" are subtle
  at sheet size; their difference is in fine detail and needs 100 % crops.
- Experiment notes: a second module instance needs an explicit `iop_order_list` (see US-3);
  local contrast placed after the display transform shifts the brightest pixels, so solving
  exposure on highlights per variant is misleading — one exposure per photo is the fair way;
  a shared sqlite connection across threads lost a query (round 2's sidecars checked: unaffected).
- **Owner's verdict:** the original haze removal is now too much on top of the new looks.
  "dehaze" and "clarity" both look good, "clarity" preferred in general; undecided for
  long-range mountain views → 100 % crops of distant ranges (_DSC8328, _DSC8333, _DSC4805,
  `photo-edits/_us4/crops4_*.png`; columns none / dehaze / clarity / clarity + dehaze):
  dehaze separates distant layers from each other and the sky and thins the blue veil;
  clarity crisps texture (rock, foliage) but leaves the distant haze; combined, the effects
  add up without halos or visible noise at 100 %.

### 2026-11-24 — US-5: print sharpening in the export style? Double sharpening

The "Fotobuch" export preset: JPEG, full size, high quality, 300 dpi, style "Aufhellung Druck"
(one tone curve instance) applied in append mode. Test on _DSC8015 (edited with sharpen
"mehr schärfen"), sharpness = mean absolute Laplacian of luminance at 2000 px:
- today (edit + "Aufhellung Druck"): 0.057; edit without its sharpening: 0.032;
- edit without sharpening + "Aufhellung Druck" extended by "mehr schärfen": **0.057, identical
  to today** (0.000 levels difference);
- the owner's existing edit (already sharpened) + the extended style: **0.111 — sharpened
  twice**, also when the style's sharpen instance carries the same name ("mehr schärfen").
  darktable appends a second instance instead of merging.
→ Moving the sharpening into the export only works for new edits; every older edit would be
oversharpened on re-export.

darktable-cli facts learned:
- `--apply-custom-presets false` also skips `data.db`, so **styles are unavailable**
  ("cannot find the style"); with `--library :memory:` styles still work once presets are on.
- With `data.db` in use, **only one darktable-cli process per config dir** at a time (parallel
  runs fail).
- `--style` must come before `--core`.
