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

### 2026-11-24 — What darktable-cli can verify for US-8 (testing strategy, ADR-0006)

Copies of one 2025 raw in throwaway folders, the owner's darktable configuration
(`write_sidecar_files=on import`), darktable 5.6.1, a library *file* instead of `:memory:`.
- **darktable-cli never writes sidecars**, also with a library file and "on import".
- It **does fill the library file**: without a sidecar the history holds only what darktable
  applies on import (workflow defaults, automatic presets), and darktable's history hashes say
  *not altered* (auto hash = current hash) — the basis of Lua's `is_altered`. With the owner's
  sidecar loaded, the library history is exactly its 15 steps.
- It keeps **no change time** in the library (−1 or empty) → the sidecar-lag rule can't be
  checked with darktable-cli.
- An exported JPEG embeds the full history, written by darktable's XMP code. Fingerprint
  (US-8) of the embedded history vs. the input sidecar:
  - **sidecar from an older darktable: fingerprint changes.** 5.6.1 converts modules to their
    current version while loading (demosaic 4→6, denoiseprofile 11→12, exposure 6→7) and writes
    the converted parameters.
  - **sidecar already in 5.6.1's module versions: fingerprint stable**, with a library file and
    in memory, also for a history with drawn masks (_DSC7382).
  → After a darktable upgrade that raises the version of a module in the tool's edit, the tool's
  earlier edits stop matching their fingerprint once darktable writes them again, and count as
  edited by hand (safe, but the tool can't redo them).
- Coverage of US-8's darktable assumptions: "unaltered after import" and "darktable writing the
  same edit in its own way" (same version, incl. masks) — darktable-cli; library change time,
  sidecar written after a change, undo and "compress history" — only the darktable application.

### 2026-11-24 — US-20 spike: driving darktable from Lua for the `@darktable` scenarios

darktable 5.6.1 (Lua API 9.7.0) under a virtual display (Xvfb) in the container, the owner's
configuration copied, throwaway library, copies of two 2025 raws, run fully unattended; script,
runner and notes in `spikes/us-20-darktable-tests/`. Fingerprints are the tool's (US-8).
- **Everything US-8 needs can be driven from Lua:** import, `apply_sidecar`, tags, opening the
  darkroom, changing a module (`dt.gui.action` on the exposure slider), undo
  (`views/darkroom/undo`) and compressing the history (`lib/history/compress history stack`).
  Pitfalls: the darkroom only opens an image that is in the lighttable's collection; acting
  before the darkroom has loaded the image crashed darktable; undo only works within the same
  darkroom visit; `apply_sidecar` silently ignores a file whose name doesn't end in `.xmp`;
  `display_image` of the image already shown crashed darktable; `global/quit` doesn't quit 5.6.1.
- **Fingerprints with an edit in 5.6.1 module versions:** after `apply_sidecar` and darktable's
  own sidecar write = the tool's file; change → different; change + undo → the same again;
  compress history → unchanged. With an edit from an older darktable, merely opening it in the
  darkroom converted its modules (US-19).
- **When darktable writes the sidecar:** `apply_sidecar` updates only the library (`is_altered`,
  change time), not the file; a tag change writes it at once. Changes in the lighttable (a
  style) are in the sidecar within 1 s. **Changes in the darkroom reach the sidecar only when the
  darkroom is left** (not within 5 minutes while the image stayed open; 1–16 s after leaving).
- **A sidecar can be stale and look current:** a tag change while the image was open in the
  darkroom wrote a sidecar with the library's *new* change time but the *old* history. The
  planned rule "sidecar older than the library → not trusted" would have passed it, and the
  tool would have taken the owner's unfinished darkroom work for its own edit.
- Lua reports the library change time as locale-formatted text in whole seconds (empty, or
  "01/01/1", when unset). The "darkroom-image-loaded" event came ~12 s after switching images.

### 2026-11-25 — US-6: day rules checked on all 706 edited day photos of 2022–2025

Method (`experiments/us6_*.py`): every day photo (EV100 ≥ 8) of 2022–2025 in the US-9 day edit
(the owner's non-tone steps, haze removal replaced by "clarity", one display transform: sigmoid
ACES + "vibrant colors" or AgX punchy), 500 px renders with darktable 5.6.1. Per photo the
exposure was solved to match the owner's edit — highlights (brightest 1 %) for sigmoid, midtones
for AgX, as in US-3 — and every probe kept, so fixed-target rules could be evaluated afterwards.
"Error" = rule exposure minus owner-matched exposure. Photos whose target the look can't reach
were left out (sigmoid 7, AgX 2: the owner's highlights clipped above 1.0).

**The owner's edits are less uniform than the 37-photo sample suggested:** brightest 1 % at
0.83–0.91 (middle half; the sample: 0.86–0.91), midtones 0.38–0.57; per year the midtones move a
lot (2024: 0.59, winter trips; 2023: 0.42).

| Rule | median error | within ±0.25 EV | within ±0.5 EV |
|---|---|---|---|
| sigmoid, same exposure for all | 0.64 EV | 21 % | 40 % |
| **sigmoid, brightest 1 % at 0.885** | **0.31 EV** | 42 % | 67 % |
| sigmoid, brightest 1 % — fitted 2022–23, checked on 2024–25 | 0.26 EV | 49 % | 70 % |
| sigmoid, brightest 1 % — fitted 2024–25, checked on 2022–23 | 0.36 EV | 39 % | 64 % |
| AgX, same exposure for all | 0.63 EV | 22 % | 41 % |
| AgX, midtones at 0.495 | 0.65 EV | 20 % | 38 % |
| **AgX, brightest 10 % at 0.675** | **0.28 EV** | 45 % | 72 % |
| AgX, sigmoid's highlight exposure +0.28 EV | 0.33 EV | 38 % | 71 % |

- The highlight rule holds for sigmoid (stable target across years: 0.88–0.89), but much
  looser than on the 37 photos it was fitted on (34/37 within ±0.5 EV then, 67 % now).
- **A fixed midtone target doesn't work for AgX**: the owner's midtones vary too much by scene
  (snow); a target on the brightest 10 % does better. Fitting the midtone target on one half of
  the years and checking on the other is off by ±0.4–0.7 EV.
- The "truth" is the owner's old edit translated into the new look; part of the spread may be
  the translation, not the owner's taste.
- Largest misses: photos with clipped highlights in the owner's edit (sigmoid), bright snow
  scenes of 2024 (AgX).

**Gradient filter** (213 of 706 photos; density 1.0 in 63 %, no rotation in 76 %, hardness ~47):
the sky-versus-land brightness in the sigmoid render without the filter separates used from not
used only weakly (AUC 0.70; median log2 ratio +0.68 vs +0.30). As a proposal: threshold 0.75 →
210 proposals, half of them where the owner used one, catching 47 % of the uses. Where used
without rotation, the filter's position follows the horizon (correlation 0.59); its offset value
is typically 8 higher than the horizon's position in % from the top (what darktable's offset
measures from is unverified).

**Look choice** on 50 random photos (`photo-edits/_us6/looks_01..10.png`; columns: owner's edit,
sigmoid ACES + vibrant, AgX punchy, both matched to the owner's edit; `index.tsv` maps sheet/row
to the file). The rule's predictions were stored before the owner picked.
- Owner's picks: **32 × AgX, 16 × sigmoid**, 2 × neither (_DSC7634, _DSC7632: bright 2024 snow
  scenes, also among the AgX exposure misses). In US-3 it was 9 sigmoid : 6 AgX on 15 photos.
- The US-3 rule ("snow share or strong-colour share > 0.14 → sigmoid, else AgX") is right on
  **17 of 48** — worse than "always AgX" (32 of 48). It even points the wrong way: photos picked
  sigmoid have less snow (median share 0.10 vs 0.18) and somewhat more colour (90th percentile of
  chroma 0.32 vs 0.27). The snow and colour measures were re-implemented here, so not identical
  to US-3's, but that doesn't explain a reversed direction.
→ No simple rule found for the look; AgX is the owner's more frequent choice.

### 2026-11-25 — US-20: the US-8 scenarios against the real darktable

`photoflow selftest scenarios` runs the 11 unattended `@darktable` scenarios of US-8 against
darktable 5.6.1 (private configuration and library, copies of one reference raw, virtual display
in the container): all pass, about 3 minutes plus ~20 s start. Every action was checked to have a
visible effect (otherwise a scenario that expects "edited by hand" fails; compress was checked
in the library: 12 → 11 entries). Learned on the way:
- Import and view switches run in the background: the driver waits for the image to be in the
  library and for the view to have changed; without that the photo was "not in the library"
  and darktable still showed the darkroom after "leaving" it.
- Applying the same style a second time through Lua changed nothing (no second instance, unlike
  darktable-cli's `--style` in US-5), so the stand-in for the tool's edit is an exposure change
  made in the darkroom, and the print style is the "style applied by hand".
- A sidecar darktable wrote while it was being ended in the darkroom didn't match its library
  (it had a style the library didn't have) → the driver returns to the lighttable before
  ending darktable. One more reason for the rule that the tool works only outside the darkroom.
- **A running darktable of the owner doesn't interfere:** with a second darktable running on the
  configuration folder the self-test copies from, in the same D-Bus session (as on a desktop),
  all scenarios passed and both instances kept running; darktable's locks are per library and
  configuration folder, and the self-test has its own.
- The test darktable runs in its own process group (so it can be ended completely); Ctrl+C
  therefore reached only photoflow and left darktable running → photoflow now ends it on
  Ctrl+C, and a leftover from a run that ended abruptly is ended at the next start.

### 2026-11-25 — US-18, round 1: denoise and local contrast for the aurora base

9 aurora photos (2026 favourites _DSC8257, 8287, 8296, 8300, 8305; reference set _DSC3492, 7371,
7305, 6085), darktable 5.6.1, full size. All new columns: scene-referred, the owner's 12mm lens
preset, sigmoid "ACES 100-nit like" + colour balance rgb "vibrant colors" (the day look), no haze
removal, tone curve, highpass or sharpening. One exposure and WB per photo, solved so that sky
median and sky-background colour match column A; only denoise and local contrast differ.
Sheets `photo-edits/_us18/sheet_<photo>.png` (row 1 full frame, rows 2–3 100 % crops of aurora
and foreground); full-size JPGs and sidecars per variant in `photo-edits/_us18/<folder>/`.

| | denoise | local contrast |
|---|---|---|
| A | owner-approved edit (2026: the recipe; older: the owner's own edit) | |
| B | profiled, non-local means default (research edit of 2026-11-23) | "clarity" once |
| C | profiled "wavelets: chroma only" | "clarity" twice |
| D | chroma only + the owner's "sanft" non-local means (strength 0.35) | "clarity" twice |
| E | as D | diffuse or sharpen "local contrast \| normal" |

- **Noise:** A is by far the noisiest; B, D and E are clean at 100 %, C keeps a fine luma grain
  (finer than B's coarse grain). Full-size JPG size as a rough noise measure on _DSC8287:
  A 5.4 MB, C 4.1, D 2.5, B 1.8, E 1.8.
- **Structure and stars:** "clarity" twice (C, D) keeps more of the aurora's rays than B; stars
  stay visible in C and D, fainter in B and E. E looks flatter, with lifted blacks.
- **Colour:** all new columns are less saturated than A ("vibrant colors" vs. the recipe's pushed
  colour balance) — not the subject of this round; colour and the display transform (sigmoid vs.
  AgX) come next.
- **Solver:** WB didn't reach the target where the owner's edit has an unusual sky background
  (_DSC6085 village lights, _DSC7371 yellowish, _DSC3492 strong blue): blue gain at its limit,
  so those columns differ in colour from A beyond the recipe.

### 2026-11-25 — US-18, round 2: display transform and colour, with D and E

Same 9 photos; round 1's D (chroma only + gentle non-local means + "clarity" twice) and E (same
denoise + diffuse "local contrast | normal"), each in three looks: **S** sigmoid "ACES 100-nit
like" + "vibrant colors" (round 1), **S+** the same with chroma 0.20 (the old recipe's chroma
push), **X** AgX "blender-like|punchy". Exposure and WB solved per display transform against
column A, shared by D/E and S/S+. Sheets `photo-edits/_us18/round2/sheet_<photo>.png`, columns
A, D-S, E-S, D-S+, E-S+, D-X, E-X; full-size JPGs and sidecars in `photo-edits/_us18/round2/<folder>/`.

Luma noise in a sky patch (std. of pixel minus 3×3 mean, ×1000; lower is cleaner):

| photo | A | D-S | E-S | D-S+ | E-S+ | D-X | E-X |
|---|---|---|---|---|---|---|---|
| _DSC8287 | 24.8 | 5.4 | 3.9 | 5.4 | 3.9 | 4.8 | 3.4 |
| _DSC8257 | 17.3 | 4.4 | 3.6 | 4.5 | 3.7 | 4.1 | 3.2 |
| _DSC8296 | 15.0 | 3.7 | 3.5 | 3.8 | 3.6 | 3.3 | 2.9 |
| _DSC8300 | 33.4 | 15.1 | 9.6 | 15.3 | 9.8 | 13.9 | 8.3 |
| _DSC8305 | 29.2 | 10.9 | 7.9 | 11.0 | 8.1 | 10.3 | 7.3 |
| _DSC3492 | 9.4 | 11.5 | 10.5 | 10.9 | 9.9 | 8.9 | 8.0 |
| _DSC7371 | 27.3 | 14.6 | 9.8 | 14.8 | 10.0 | 13.9 | 9.3 |
| _DSC7305 | 12.5 | 10.9 | 8.7 | 11.2 | 8.9 | 9.9 | 7.7 |
| _DSC6085 | 5.5 | 5.3 | 3.2 | 5.3 | 3.2 | 5.3 | 3.0 |

- **Noise:** D is at a third to a half of A's noise on the noisy 2024/2026 shots; E removes
  another 25–40 %. The hardest cases for D are the long dark exposures (_DSC8300, _DSC7371:
  ≈ 15 vs. E ≈ 10). _DSC3492 (2019) is the only photo where the owner's edit is cleaner than D.
  AgX comes out slightly less noisy than sigmoid at the same sky brightness.
- **Look (sheet size):** S+ is a little more colourful than S, still far from A's neon green;
  X is softer, with lifted shadows and more muted colour.
- **Owner's verdict on round 2:** S+ preferred. D preferred unless too noisy, then E; the measured
  sky noise matches what the owner sees as noisy. Too noisy in D: _DSC8300, _DSC7371, _DSC7305,
  _DSC8305 (D noise 11–15); fine: the four at ≤ 5.4; _DSC3492 (10.9) D ≈ E. → Switch to E when D's
  noise is above about 8–10.

### 2026-11-25 — US-18, round 3: foreground colour

8 aurora photos with a land/snow foreground (round 2's set without _DSC6085 and _DSC3492 — on their
dark foregrounds a masked correction made no measurable difference, so the solver failed — plus
_DSC7579). Base: S+ with D, or E where D's sky noise is above 9 (×1000) — E for _DSC8300, 8305,
7305, 7579, 7371. Sky exposure and WB as in round 2. Tools were chosen from current darktable
practice, not from the owner's old edits (owner, 2026-11-25); only the foreground colour target —
slightly blue snow, R/G 0.95, B/G 1.25 — is taken from the owner's edits without a cast.
Horizon positions read off the sheets by hand.

- **F0** no foreground step.
- **F1** channel mixer rgb as plain R/B gains (no chromatic adaptation) under a drawn gradient
  mask starting at the horizon, gains solved to the target.
- **F2** no mask: one global WB half-way between the sky-background and the foreground targets.
- **F3** color calibration (CAT16, custom illuminant) under the same mask — the darktable manual's
  way for mixed lighting; illuminant solved to the target.

Sheets `photo-edits/_us18/round3/sheet_<photo>.png` (columns A, F0, F1, F2, F3; rows full frame,
aurora crop, foreground crop); full-size JPGs and sidecars in `photo-edits/_us18/round3/<folder>/`.

Foreground colour (R/G / B/G of the middle luminance band; target 0.95 / 1.25):

| photo | A | F0 | F1 | F2 | F3 |
|---|---|---|---|---|---|
| _DSC8300 | 0.57 / 0.83 | 0.69 / 1.02 | 0.91 / 1.18 | 0.76 / 1.00 | 0.92 / 1.21 |
| _DSC8305 | 0.55 / 1.26 | 0.79 / 1.70 | 0.96 / 1.29 | 0.92 / 1.50 | 0.94 / 1.26 |
| _DSC7305 | 0.99 / 1.09 | 1.05 / 1.07 | 1.00 / 1.28 | 1.00 / 1.11 | 1.01 / 1.32 |
| _DSC7579 | 0.75 / 1.59 | 0.45 / 2.04 | 1.05 / 1.49 | 0.48 / 1.75 | 0.89 / 1.44 |
| _DSC7371 | 0.73 / 0.32 | 0.92 / 0.41 | 0.81 / 0.92 | 0.84 / 0.60 | 0.81 / 0.92 |
| _DSC8287 | 1.04 / 1.48 | 1.09 / 1.63 | 0.97 / 1.30 | 1.14 / 1.59 | 0.96 / 1.28 |
| _DSC8257 | 0.85 / 1.18 | 0.91 / 1.30 | 0.94 / 1.26 | 0.95 / 1.27 | 0.94 / 1.23 |
| _DSC8296 | 1.04 / 1.40 | 1.17 / 1.39 | 1.00 / 1.31 | 1.10 / 1.45 | 0.98 / 1.30 |

- With the sky-based WB alone (F0) the foreground is off in both directions: green on _DSC8300,
  cyan-blue on _DSC8305 and _DSC7579, orange from light pollution on _DSC7371; the rest are close.
- F1 and F3 reach the target on all but _DSC7371 (orange too strong; capped). At sheet size F3
  looks more neutral, F1 can tip into brown (_DSC7579). F2 only goes half-way and also shifts the
  sky — a single WB can't serve both.
- Needed for automation: a horizon line per photo (also for US-11).
- **Owner's verdict on round 3:** F1 and F3 look the same on most photos → F3 (color calibration,
  the documented tool).

### 2026-11-25 — US-7: what tells aurora from other night photos

All 130 night photos up to 2026 (EV100 ≤ 1, all ILCE-6000), rendered at 300 px with a fixed
neutral check edit: linear, fixed WB, no tone mapping, no denoise. Labels: the aurora photos
identified on 2026-11-21 plus the 2026 aurora nights, the rest by eye; the doubtful ones by the
owner. Weak, diffuse or underexposed aurora (_DSC8234–8239, 7302, 7305, 3437, 3721, 3904) may go
either way: the owner would rather treat a barely visible aurora as a night photo than make it
"pop" (noise, and not what was seen). → 75 aurora, 44 not, 11 either.

- **Green pixels in the sky** (share of sky pixels clearly greener than the sky's median colour):
  fails on weak aurora under light pollution or moonlight, where the whole sky is yellow — many
  clear aurora photos score 0.
- **Colour structure in the sky** works: chromaticity per 10×10-px block in the upper 55 % of the
  frame, minus a smooth quadratic fit across the sky, 90th percentile of the residual. Light
  pollution, twilight and moonlight change colour smoothly; aurora comes in bands and curtains.
  Green channel: non-aurora ≤ 0.0102 (moonlit peaks reaching into the sky, Rosengarten 2018),
  aurora ≥ 0.0103 except _DSC3896/3897 (small aurora under bright clouds) and _DSC7367 (light
  pollution). Red, or red and green together, separate worse (mountains and city lights add red
  structure); the upper 40 % instead of 55 % also worse.
- **Threshold ≈ 0.011** (a margin above the highest non-aurora): 6 of 75 aurora photos become
  the question "night photo without aurora", no non-aurora photo is taken for aurora — the safe
  direction, since a wrong aurora edit would go unnoticed. All "either way" photos fall below.
- _DSC0666, a long exposure of a small waterfall, counts as night by its exposure and has strong
  colour structure (foliage) → would be taken for aurora. Rare; the EXIF rule can't see it.
- Reference set: _DSC3735 (bright green aurora) 0.040, _DSC0906 (stars, summit cross) 0.002;
  _DSC7305 (weak aurora under a near-full moon) 0.004 → a question.

### 2026-11-25 — US-18, round 4: luma-denoise strength and the snow colour target

Same 8 photos as round 3, S+ look, F3 foreground step; sky exposure and WB as before.

**Luma denoise.** D's gentle non-local means had strength 0.35 only because the owner's preset
"sanft" uses it; tested 0.2, 0.35, 0.5, 0.7 (all with "clarity" twice), plus E (strength 0.35 with
diffuse "local contrast") for reference. Sheets `photo-edits/_us18/round4/denoise_<photo>.png`
(columns 0.2, 0.35, 0.5, 0.7, E). Sky noise (as in round 2, ×1000):

| photo | 0.2 | 0.35 | 0.5 | 0.7 | E |
|---|---|---|---|---|---|
| _DSC8300 | 20.3 | 15.3 | 12.4 | 10.1 | 9.8 |
| _DSC7371 | 19.5 | 14.8 | 11.7 | 9.3 | 10.0 |
| _DSC7579 | 17.6 | 14.0 | 11.7 | 9.8 | 10.0 |
| _DSC7305 | 15.9 | 11.2 | 9.0 | 7.6 | 8.9 |
| _DSC8305 | 15.5 | 11.0 | 8.8 | 7.2 | 8.1 |
| _DSC8287 | 8.1 | 5.4 | 4.2 | 3.4 | 3.9 |
| _DSC8257 | 6.2 | 4.5 | 3.8 | 3.3 | 3.7 |
| _DSC8296 | 5.0 | 3.8 | 3.4 | 3.1 | 3.6 |

- D at strength 0.7 is as clean as E on every photo while keeping "clarity" twice — so a stronger
  luma denoise could replace the D→E switch, if it doesn't cost the aurora's fine structure or the
  stars (to be judged on the crops).

**Snow colour target** for F3: N neutral (R/G 1.0, B/G 1.0), B slightly blue (0.95 / 1.25, rounds
3), B+ bluer (0.9 / 1.45). Sheets `photo-edits/_us18/round4/snow_<photo>.png` (columns N, B, B+;
full frame, foreground crop). Reached on all photos except _DSC7371 (orange light pollution, ≈ 0.1
short in B/G). At sheet size N looks olive-grey next to the night sky, B like snow at night, B+
violet towards the edges.

Full-size JPGs and sidecars for all columns in `photo-edits/_us18/round4/<folder>/`.
- **Owner's verdict on round 4:** D at strength 0.7 is fine and replaces E (no noise-dependent
  switch needed); snow target B looks best.

### 2026-11-26 — US-20: the self-test's darktable on the owner's desktop

On the owner's machine the `@darktable` scenarios failed in ways the container never showed:
- **Sidecar read too early:** right after `apply_sidecar` and the tag change that makes darktable
  write the sidecar, the driver sometimes still read the unedited sidecar, so the fingerprint
  tag named the wrong history and the tool's own edit counted as "edited by hand". In the
  container this failed every time once darktable's welcome dialog was switched off, which had
  slowed it down. → The driver waits until the sidecar has changed and stopped changing.
- **The mouse pointer decides which photo the darkroom opens:** with the pointer inside
  darktable's window, entering the darkroom opened the photo darktable last had "under the
  mouse" (after leaving the darkroom, the previous scenario's photo) instead of the selected one;
  the driver then waited in vain for its photo. The welcome dialog had hidden this by taking the
  pointer events. Switching the darkroom to the right photo afterwards (`display_image`) loaded
  it, but the following changes didn't land on it. darktable also opens full screen.
  → The self-test always runs darktable on a virtual screen (Xvfb), also on a desktop.

### 2026-11-26 — US-21, round 1: night base candidates

The owner's six favourite night edits (DSC00461, _DSC0905, _DSC0909, _DSC0914, _DSC0926,
_DSC4743) as column A — as a look reference only. Their look: a dark, almost black, neutral to
blue sky, many crisp stars, a structured Milky Way, a dark foreground. Candidates start from the
US-18 aurora base and change one thing each, chosen from current night-photo practice (links:
"Night (Milky Way, stars) editing"); exposure and WB solved per photo so that the sky median and
sky-background colour match column A (per display transform).

| | denoise | local contrast | look |
|---|---|---|---|
| N1 | US-18: chroma only + non-local means 0.7 | "clarity" twice | sigmoid ACES + vibrant + chroma 0.2 |
| N2 | as N1 | as N1 + diffuse "dehaze" (Milky Way detail) | as N1 |
| N3 | as N1 | as N1 | AgX punchy |
| N4 | astrophoto denoise (defaults) | as N1 | as N1 |

Sheets `photo-edits/_us21/round1/sheet_<photo>.png` (columns A, N1–N4; full frame, sky crop,
second crop); full-size JPGs and sidecars in `photo-edits/_us21/round1/<folder>/`. Sky noise as in
US-18 (×1000; in star fields the stars count too, so it reads high):

| photo | A | N1 | N2 | N3 | N4 |
|---|---|---|---|---|---|
| DSC00461 | 11.8 | 3.6 | 5.2 | 3.2 | 4.5 |
| _DSC0905 | 20.9 | 12.1 | 17.8 | 10.5 | 14.2 |
| _DSC0909 | 32.8 | 20.9 | 30.4 | 17.7 | 23.0 |
| _DSC0914 | 48.4 | 21.2 | 31.4 | 19.4 | 24.4 |
| _DSC0926 | 21.5 | 12.0 | 18.3 | 11.5 | 11.9 |
| _DSC4743 | 12.5 | 12.2 | 21.6 | 11.0 | 17.6 |

- All new columns are cleaner than the owner's edits; diffuse "dehaze" (N2) brings 40–75 % of the
  noise back for little visible gain at sheet size; astrophoto denoise (N4) leaves coloured
  blotches; AgX (N3) is the cleanest and a little more neutral.
- Matching only the sky goes wrong on the foreground: on _DSC4743 the moonlit rocks come out far
  too bright and cyan (the owner's edit keeps them dark); _DSC0914's sky turns greenish in the
  sigmoid columns. The foreground needs its own treatment, as for aurora (US-18).
- **Owner's verdict on round 1:** N3 (AgX) slightly preferred. In all candidates the stars lose a
  lot of their punch compared to the owner's edits, and they look too blue/green; a bit redder
  would be better.

### 2026-11-26 — US-21, round 2: what takes the stars' punch

On N3 (AgX): R0 = N3; R1 + raw chromatic aberrations (star fringes); R2 = R1 with luma denoise
0.35 instead of 0.7; R3 = R2 with a warmer WB (sky-background target R/G ×1.06, B/G ×0.9). Sheets
`photo-edits/_us21/round2/sheet_<photo>.png`. New measure for the stars (journal-only tool,
`experiments/us21_stars.py`): the 300 brightest local maxima in a sky patch; **punch** = peak
luminance / median of the 15×15 px around it; star colour = R/G, B/G of the 3×3 core.

| photo | punch A | R0 | R1 | R2 | R3 | star R/G, B/G A | R0 |
|---|---|---|---|---|---|---|---|
| DSC00461 | 7.9 | 3.2 | 3.2 | 3.2 | 3.2 | 0.89, 0.86 | 0.98, 0.70 |
| _DSC0905 | 7.3 | 4.9 | 4.8 | 4.7 | 4.7 | 0.96, 1.06 | 1.05, 0.86 |
| _DSC0909 | 9.5 | 5.0 | 5.0 | 4.8 | 4.8 | 0.93, 1.21 | 0.99, 1.05 |
| _DSC0914 | 6.6 | 4.2 | 4.2 | 4.1 | 4.1 | 0.97, 1.06 | 0.96, 1.06 |
| _DSC0926 | 8.8 | 6.5 | 6.5 | 6.4 | 6.4 | 1.01, 0.96 | 1.07, 0.63 |
| _DSC4743 | 3.6 | 3.5 | 3.5 | 3.5 | 3.4 | 0.95, 1.47 | 0.80, 1.04 |

- Neither CA correction nor weaker denoise changes the punch. The 100 % crops show why: the
  owner's edits have an almost black sky between the stars, so faint stars vanish and the bright
  ones stand out; the candidates lift the sky and the faint stars (grey, busy). A black-point /
  shadow-contrast difference, not detail.
- The WB solved on the sky background leaves the stars greenish (high G; _DSC4743 R/G 0.80).
  CA correction and the warmer WB move the stars only a little.

### 2026-11-26 — US-21, round 3: neutral stars and a darker black level

On R1 (AgX, raw CA, luma 0.7): T0 = R1 (sky-matched WB); T1 WB solved so that the stars average
neutral (R/G = B/G = 1, the usual star colour calibration in astro processing); T2 = T1 plus the
exposure module's black level correction, solved so that the star punch reaches the owner's edit,
WB re-solved for neutral stars afterwards; T3 = T2 + diffuse "local contrast | fine". Star
measures at 1500 px (`experiments/us21_stars.py`, patch 80 % wide, 5–50 % from the top). Sheets
`photo-edits/_us21/round3/sheet_<photo>.png` (columns A, T0–T3).

| photo | punch A | T0 | T1 | T2 | T3 | stars R/G, B/G: A | T0 | T2 | black |
|---|---|---|---|---|---|---|---|---|---|
| DSC00461 | 8.0 | 3.7 | 3.8 | 8.2 | 11.7 | 0.91, 0.85 | 1.00, 0.67 | 1.00, 1.03 | 0.0091 |
| _DSC0905 | 7.0 | 4.2 | 4.2 | 6.7 | 8.0 | 0.96, 1.08 | 0.94, 0.96 | 0.98, 0.98 | 0.0054 |
| _DSC0909 | 9.2 | 4.4 | 4.3 | 8.4 | 11.2 | 0.93, 1.30 | 0.89, 1.11 | 0.98, 1.00 | 0.0059 |
| _DSC0914 | 5.8 | 3.9 | 3.9 | 5.7 | 6.2 | 0.97, 1.11 | 0.92, 1.14 | 0.98, 0.99 | 0.0035 |
| _DSC0926 | 7.8 | 5.0 | 5.2 | 7.1 | 8.0 | 1.02, 0.93 | 1.00, 0.66 | 0.99, 0.99 | 0.0012 |
| _DSC4743 | 5.1 | 3.5 | 3.4 | 5.1 | 5.1 | 0.93, 1.44 | 0.68, 1.06 | 0.99, 1.00 | 0.0016 |

- The black level brings the punch to the owner's level on every photo (T2); fine local contrast
  (T3) adds more punch but also a busier, grainier sky.
- Neutral stars move the sky away from the owner's blue: brownish (T1) to violet (T2). The
  owner's stars themselves are slightly blue (B/G 0.85–1.44).
- The black level also darkens the frame's corners (lens vignetting) almost to black on
  _DSC0909; the moonlit foreground of _DSC4743 is still too bright — the foreground is next.
- **Owner's verdict on round 3:** T3 preferred in general, but _DSC4743's sky has a very
  different colour impression from the owner's edit.
- **Why (analysis, _DSC4743):** two causes. (1) One global WB can't give both: solved for
  neutral stars the dark sky turns violet (R 0.097, G 0.063, B 0.090); solved for the owner's sky
  colour the stars turn cyan-green (R/G 0.72). The owner's edit separates them with "color
  correction": shadows strongly towards blue (a −6.0, b −9.1), highlights slightly the other way
  (a +2.8, b −2.7) — split toning. (2) The exposure module's black level subtracts the same from
  every channel; green, the weakest in the dark sky, hits zero first (T3: R 0.033, G 0.000,
  B 0.062) → more violet; the near-black corners of _DSC0909 are the same clipping. rgb levels
  with "preserve colors: luminance" before the tone mapper clips as well (red first). The
  owner's sky is not darker than T0's at all (dark parts R 0.040, G 0.086, B 0.133): his punch
  comes from brighter stars (high pass, curves lifting the highlights), and his black point
  (levels, 0.004) sits on Lab lightness after the curves.

### 2026-11-26 — US-21, round 4: sky colour and star punch without clipping (_DSC4743 only)

Worked on _DSC4743 alone, step by step with the owner; sheets `photo-edits/_us21/round4/`.
Measures at 1500 px: dark sky = 10–30 % of the upper sky by brightness, Milky Way glow = 60–80 %
(below the stars' brightness), stars and punch as in round 2, star saturation = median of
(max − min) / max over the 300 brightest star cores.

| | punch | star sat. | stars R/G, B/G | dark sky R/G, B/G | glow R/G, B/G |
|---|---|---|---|---|---|
| owner's edit | 5.1 | 0.37 | 0.93, 1.44 | 0.47, 1.55 | 0.84, 1.42 |
| V0 = T3 | 5.1 | | 0.99, 0.98 | green clipped | |
| V1: no black level, shadows tint | 3.3 | | 0.83, 1.05 | 0.54, 1.60 | 0.64, 1.31 |
| W1: + highlights tint for the glow | 2.9 | | 1.49, 1.39 | 0.62, 1.64 | 0.88, 1.30 |
| X1: + midtones tint instead | 3.4 | 0.13 | 0.93, 1.09 | 0.96, 1.93 | 0.84, 1.43 |
| X2: X1 + tone equalizer + highlights chroma | 5.0 | 0.25 | 0.90, 1.09 | | 0.79, 1.61 |

- **Sky colour by split toning:** WB for neutral stars, then color balance rgb tints solved
  against the owner's edit: the shadows wheel for the dark sky (V1). The owner then clarified
  that the dark sky has no colour impression (too dark, which is good); what differs is the
  **Milky Way band, slightly red/purple in the owner's edit without the stars being affected**.
- **Tinting only the glow:** the highlights wheel (multiplicative) turned the stars and every
  bright rock purple (W1, "way too much"); the global offset clips green like the black level;
  the midtones wheel at a small strength (≈ 0.02) tints the glow and barely the stars (X1) — at
  0.1 it was unstable. "X1 is much better."
- **Punch without clipping:** the tone equalizer without detail preservation (per pixel), the
  bands −8 … −5 EV at −1.2 EV, lowers the sky and the faintest stars by a factor — no channel
  clips, hues stay (X2: punch 4.9–5.0). Faint stars merge into the background, which the owner
  accepts.
- **Star colour:** AgX bleaches bright points (AgX "base": saturation 0.07). Not the chroma
  denoise (0.13 → 0.14 without it). Color balance rgb highlights chroma 1.0 brings 0.25;
  sigmoid and AgX "unmodified base primaries" bleach more and lose punch.
- **Owner's verdict:** go on with X2, tints re-solved with the tone equalizer in place, on all
  six photos (round 5).

### 2026-11-26 — US-21, round 5: X2 on all six photos

Base: AgX punchy, raw CA, chroma-only + non-local-means 0.7 denoise, "clarity" twice, diffuse
"local contrast | fine", tone equalizer −1.2 EV on the darkest bands (no detail preservation),
color balance rgb with highlights chroma 1.0 and the two sky tints (shadows for the dark sky,
midtones for the Milky Way glow) solved per photo against the owner's edit; WB for neutral stars.
Sheets `photo-edits/_us21/round5/sheet_<photo>.png` (columns: owner's edit, Y).

| photo | punch A / Y | stars R/G, B/G A / Y | glow R/G, B/G A / Y |
|---|---|---|---|
| DSC00461 | 8.0 / 7.3 | 0.91, 0.85 / 1.00, 0.76 | 0.90, 0.87 / 0.91, 0.88 |
| _DSC0905 | 7.0 / 7.7 | 0.96, 1.08 / 0.96, 0.96 | 0.83, 1.17 / 0.91, 1.15 |
| _DSC0909 | 9.2 / 7.1 | 0.93, 1.30 / 0.90, 1.10 | 0.89, 1.32 / 0.85, 1.34 |
| _DSC0914 | 5.8 / 5.7 | 0.97, 1.11 / 0.94, 1.08 | 0.88, 1.21 / 0.90, 1.25 |
| _DSC0926 | 7.8 / 10.4 | 1.02, 0.93 / 1.01, 0.74 | 0.98, 0.82 / 0.97, 0.78 |
| _DSC4743 | 5.1 / 5.0 | 0.93, 1.44 / 0.88, 1.10 | 0.84, 1.42 / 0.85, 1.42 |

- Punch at the owner's level on four photos, above on _DSC0926, below on _DSC0909; the glow
  colour matches everywhere (the tints are small except on _DSC4743).
- The stars are consistently less blue than the owner's (B/G lower on every photo; yellowish
  in the _DSC0909 crops); the owner's _DSC0909 also shows a red Milky Way core near the horizon
  that Y renders grey. Y is overall a little darker.
- _DSC4743's dark sky ended up violet by the numbers (R/G 1.18, B/G 3.08) — the two tints pull on
  each other and the solver finished on the glow; at that darkness it shows as near-black.
- **Owner's verdict on round 5:** better; star colours still a little off; the photos look
  darker, especially the foregrounds (except _DSC4743); the dark sky background is better than in
  the owner's own edits.

### 2026-11-26 — US-21, round 6: star WB slightly blue, tone equalizer masked to the sky

Round 5 plus: star WB solved towards the owner's star colour (median R/G 0.95, B/G 1.12 over the six
edits) instead of neutral; the tone equalizer restricted to the sky by a drawn gradient mask above
the horizon (set by hand: DSC00461 0.65, _DSC0905 0.90, _DSC0909 0.97, _DSC0914 1.0, _DSC0926 0.70,
_DSC4743 0.73 from the top). Sheets `photo-edits/_us21/round6/sheet_<photo>.png` (columns:
owner's edit, Z).

- **Masks need a current sidecar version:** built on the 2018/2020 sidecars (`xmp_version` 2),
  darktable ignored the drawn mask entirely (the masked module acted on the whole frame); with
  `xmp_version` 5 the mask works. The version also shifts the render a little on its own
  (sky 0.067 → 0.060 on _DSC0926), so rounds 1–5 built those photos on their old sidecar
  version — as the aurora check showed (journal 2026-11-25), the tool has to write it explicitly.
- Foregrounds back at the owner's brightness (_DSC0926: grass and sleeping bags match).
- Stars still too warm on DSC00461 and _DSC0926 (B/G 0.82–0.88, target 1.12): the star WB was
  solved before the sky tints and the star chroma boost, which then shift the stars; the chroma
  boost amplifies a leftover yellow cast to orange.
- **Owner's verdict:** on _DSC0905 and _DSC0926 the stars look less bright and warmer; the
  foreground of _DSC0926 is good; _DSC4743's Z is liked very much. The star punch measure (a
  ratio to the surrounding sky) hides that the stars themselves got dimmer as the sky got darker.

### 2026-11-26 — US-21, round 7: star WB last, star brightness by the tone equalizer (3 photos)

_DSC0905, _DSC0926, _DSC4743. Round 6 plus: star WB solved last, with the sky tints and the star
chroma (lowered to 0.6) in place, alternated with the tints twice; the sky-masked tone equalizer's
bright bands (−2 … 0 EV) raised by k EV, solved so that the stars' peak brightness (median of the
300 brightest peaks) matches the owner's edit. Sheets `photo-edits/_us21/round7/`.

| photo | star peak A / Z2 | k | stars R/G, B/G A / Z (r6) / Z2 |
|---|---|---|---|
| _DSC0905 | 0.76 / 0.51 | 2.46 (limit) | 0.96, 1.08 / 0.97, 1.02 / 0.97, 1.13 |
| _DSC0926 | 0.55 / 0.45 | 2.46 (limit) | 1.02, 0.93 / 1.04, 0.88 / 1.00, 1.12 |
| _DSC4743 | 0.63 / 0.58 | 2.46 (limit) | 0.93, 1.44 / 0.89, 1.17 / 0.96, 1.14 |

- The median star colour reaches the target, but on _DSC0926 the individual stars split into
  orange and blue ones (the chroma boost spreads them apart around a neutral median), and the
  bluer WB turns the lit foreground cyan (white rocks, mat).
- The stars' brightness doesn't reach the owner's even at the solver's limit: AgX compresses
  the brightest values, so raising them before the tone mapper does little.
- _DSC0905's foreground just below the horizon got darker (the soft mask edge reaches into it).
- Not an improvement over round 6.

### 2026-11-26 — US-21, round 8: a star curve after AgX (_DSC0905, _DSC0926)

Round 6's Z plus a tone curve after AgX (in darktable's order the tone curve, levels and local
contrast all come after AgX), brightness only ("preserve colors: luminance"): identity up to a
display brightness of 0.2 (sky, Milky Way glow), lifted above; strength solved so that the stars'
peak brightness matches the owner's edit. Sheets `photo-edits/_us21/round8/` (columns A, Z, Z3).

| photo | star peak A / Z / Z3 | punch A / Z / Z3 | stars R/G, B/G A / Z3 |
|---|---|---|---|
| _DSC0905 | 0.76 / 0.44 / 0.53 | 7.1 / 7.7 / 8.3 | 0.96, 1.08 / 0.94, 1.04 |
| _DSC0926 | 0.55 / 0.40 / 0.47 | 7.8 / 12.3 / 12.7 | 1.02, 0.93 / 1.03, 0.88 |

- The curve hit the solver's limit on both and lifts the star peaks by only ~20 %: at 1500 px a
  star core is a pixel or two, mixed with the sky around it.
- The visible difference to the owner's edits is star **size**: the owner's stars are larger,
  white blobs (high pass and low pass in those edits, little luma denoise), the candidates'
  stars are small points. Current astro practice tends to shrink stars rather than enlarge them.
- **Owner's verdict on round 8:** Z3 is better than Z, but the owner's own edit is still
  preferred for _DSC0905 and _DSC0926; the stars as a whole look too dark. Strong star reduction
  (as in the r/ExposurePorn example) goes too far for the owner's taste; the Milky Way is visible
  in only a few of the owner's night photos.

### 2026-11-26 — US-21: a modern recipe from scratch, for comparison

Standard steps with defaults and no matching to the owner's edits: raw CA, profiled denoise
(default), lens, exposure, "clarity" once, color balance rgb "basic colorfulness | standard";
WB by background neutralization (dark sky slightly cool, R/G 0.95, B/G 1.10) and a fixed sky
brightness (median 0.12); M1 sigmoid "scene-referred default", M2 AgX punchy. _DSC0905, _DSC0926,
_DSC4743; sheets `photo-edits/_us21/modern/` (columns: owner's edit, Z of round 6, M1, M2).

| photo | star peak A / Z / M1 / M2 | punch A / Z / M1 / M2 | sky median A / Z / M1 |
|---|---|---|---|
| _DSC0905 | 0.76 / 0.44 / 0.45 / 0.46 | 7.1 / 7.7 / 3.8 / 4.0 | 0.11 / 0.06 / 0.12 |
| _DSC0926 | 0.55 / 0.40 / 0.48 / 0.49 | 7.8 / 12.3 / 4.1 / 4.1 | 0.07 / 0.03 / 0.12 |
| _DSC4743 | 0.63 / 0.52 / 0.51 / 0.53 | 5.1 / 4.7 / 3.5 / 3.4 | 0.10 / 0.06 / 0.12 |

- The plain modern recipe gives a grey, flat sky with dim stars: its stars are no brighter than
  Z's, and against the brighter sky they have half the punch.
- The owner's sky is about as bright as M1's, yet the owner's stars are far brighter (peak 0.76
  vs 0.45 on _DSC0905) and larger. Neither the modern defaults nor the tuned pipeline get there;
  the owner's star look comes from display-referred steps (high pass, low pass, curves) that
  current practice avoids.
- **Owner's verdict on the modern recipe:** M1 and M2 are worse than Z; for _DSC4743 round 6's Z is
  as good as the owner's own edit.

### 2026-11-26 — US-21: star size, star brightness and exposure (_DSC0905, _DSC0926)

Measured at full size in a sky patch (`experiments/us21_stars.py`, `star_size`: area of the 200
brightest stars above half of peak − sky). _DSC0905: owner's edit 12 px, peak 0.80, sky 0.095; Z3
14 px, peak 0.65, sky 0.054. **Z3's stars are not smaller than the owner's — they are dimmer, on a
much darker sky.** Weaker luma denoise (0.35) changed nothing measurable; diffuse "artistic effects |
bloom" made the cores smaller and dimmer (it spreads light away from them). Exposure on Z3:
+0.4 EV → peak 0.68, sky 0.074; +0.8 → 0.72, 0.109; +1.2 → 0.80, 0.144 — the sky brightens faster
than the stars (AgX compresses the highlights). A brightness-only white-point curve after AgX
(white point 0.8 … 0.5) barely moved the measured peaks (0.63–0.68); not understood.

Sheets with Z3 at +0.4 and +0.8 EV: `photo-edits/_us21/round9/`. At sheet size the stars in all Z3
variants look yellow-orange where the owner's are white (the core-average colour measure says
neutral, so it misses this), and _DSC0926's foreground is over-saturated (mat, grass): the star
chroma boost and the colour balance act on the whole frame.
- **Owner's decision:** +0.4 EV and no star chroma boost, on all six photos (round 10).

### 2026-11-26 — US-21, round 10: +0.4 EV, no star chroma boost (all six)

Round 6's Z + the star curve after AgX (round 8's strength, 3.0) + 0.4 EV, no star chroma boost,
sky tints re-solved. Sheets `photo-edits/_us21/round10/sheet_<photo>.png` (columns: owner's edit,
Z, F).

| photo | star peak A / Z / F | sky median A / Z / F | stars R/G, B/G A / F |
|---|---|---|---|
| DSC00461 | 0.79 / 0.36 / 0.56 | 0.101 / 0.046 / 0.078 | 0.91, 0.85 / 1.04, 0.76 |
| _DSC0905 | 0.76 / 0.44 / 0.58 | 0.108 / 0.055 / 0.086 | 0.96, 1.08 / 0.97, 1.09 |
| _DSC0909 | 1.00 / 0.49 / 0.62 | 0.104 / 0.061 / 0.092 | 0.93, 1.30 / 0.88, 1.25 |
| _DSC0914 | 0.79 / 0.54 / 0.69 | 0.135 / 0.080 / 0.129 | 0.97, 1.11 / 0.90, 1.21 |
| _DSC0926 | 0.54 / 0.40 / 0.51 | 0.073 / 0.029 / 0.044 | 1.02, 0.93 / 1.02, 0.91 |
| _DSC4743 | 0.63 / 0.52 / 0.78 | 0.100 / 0.057 / 0.107 | 0.93, 1.44 / 0.96, 1.30 |

- Stars brighter than in Z on all six and whiter (without the chroma boost they no longer turn
  orange); the sky stays darker than the owner's except on _DSC0914 and _DSC4743, where it reaches
  the owner's level.
- _DSC4743, which the owner liked in Z: F is clearly brighter, with a prominent purple Milky Way
  and over-bright, cold rocks in the foreground.

### 2026-11-26 — US-9: the flags on real photos

First run of US-9's scenarios against the self-test's darktable: 22 of 24 passed (default look
with the exposure on target, import kept, both switches both ways, contradictory tags). The two
flags failed on _DSC6299 (bright snow):
- **Snow**: "> 14 % of pixels brighter than 0.7 and colourless" can't be true in AgX — the exposure
  rule puts the brightest 10 % at 0.675. Relative to the image instead (colourless, brighter than
  60 % of the 90th percentile): _DSC6299 0.22, _DSC6560 0.003.
- **Clipped highlights**: AgX reaches its target on every photo (soft roll-off), so "target
  unreachable" never fires. Measuring clipped raw highlights instead didn't work: no plateau at
  the maximum, even at full size in a linear render — darktable's highlight reconstruction fills
  clipped areas in every render. _DSC6299 probably isn't clipped in the raw at all (its note in
  the reference set refers to the owner's edit).
- **Owner's verdict on US-21 round 10:** F is better than Z on DSC00461, _DSC0905, _DSC0909 and
  _DSC0926; on _DSC0914 F is a little too bright but not worse than Z; on _DSC4743 F is much worse
  and Z was good. Some foregrounds still need work. Reading: the preferred versions have a sky
  median of about 0.05–0.09; the fixed +0.4 EV pushes _DSC4743 to 0.107 and _DSC0914 to 0.129 →
  a per-photo sky-brightness target instead of a fixed exposure offset.
- **Clipped raw highlights, measured** (US-9): a check render with highlight reconstruction off
  (darktable's highlights module in "clip" mode, otherwise the aurora check edit, 300 px) keeps
  clipped areas as a flat plateau at the image's maximum. Share of the frame within 0.5 % of the
  maximum, over 882 of the 1925 day photos (the scan was stopped there): > 0.1 % on 25, > 0.3 % on
  16, > 1 % on 3 (max 1.5 %, _DSC0831). The top six are all backlit scenes with blown sky next to
  the sun. Threshold for the "check exposure" open step: 0.3 %.

### 2026-11-26 — US-21, round 11: a sky-brightness target instead of +0.4 EV

Round 10's F with the exposure solved per photo so that the sky median (upper 55 % of the frame,
300 px) reaches a target; two levels, G1 0.065 and G2 0.085. Sheets
`photo-edits/_us21/round11/sheet_<photo>.png` (columns: owner's edit, F, G1, G2).

| photo | sky median A / F / G1 / G2 | exposure EV F / G1 / G2 | star peak A / F / G1 / G2 |
|---|---|---|---|
| DSC00461 | 0.101 / 0.078 / 0.067 / 0.086 | 1.31 / 1.17 / 1.40 | 0.79 / 0.56 / 0.52 / 0.59 |
| _DSC0905 | 0.108 / 0.086 / 0.064 / 0.086 | 1.83 / 1.49 / 1.83 | 0.76 / 0.58 / 0.54 / 0.58 |
| _DSC0909 | 0.104 / 0.092 / 0.063 / 0.081 | 2.28 / 1.73 / 2.13 | 1.00 / 0.62 / 0.57 / 0.60 |
| _DSC0914 | 0.135 / 0.129 / 0.063 / 0.080 | 3.03 / 2.04 / 2.46 | 0.79 / 0.69 / 0.58 / 0.62 |
| _DSC0926 | 0.073 / 0.044 / 0.063 / 0.084 | 2.38 / 2.82 / 3.11 | 0.54 / 0.51 / 0.55 / 0.57 |
| _DSC4743 | 0.100 / 0.107 / 0.062 / 0.079 | 3.91 / 3.33 / 3.61 | 0.63 / 0.78 / 0.62 / 0.67 |

- The target pulls the outliers in: _DSC0914 and _DSC4743 get 0.6–1.0 EV less than F, _DSC0926
  0.4–0.7 EV more. Star brightness follows the exposure (G1 dimmest).
- _DSC4743's foreground (moonlit rocks) stays over-bright and cold even in G1: the sky target
  can't fix the foreground.
- **Owner's verdict on round 11:** G1 is preferred for more photos than G2, but not for all. The
  difference between the two is only the exposure module's value (≈ +0.3 EV), which the owner can
  adjust per photo in darktable (the photo then counts as edited by hand, US-8). → **Sky median
  0.065 is the default** for the night edit.

### 2026-11-26 — US-10: sky targets from the owner's aurora edits

The 75 aurora photos of the US-7 analysis, each in its owner-approved version: the owner's own edit
(33: 2019, 2022, 2024, 2026-09), or the 2026-01/03 recipe output accepted per night (42 — their
brightness was set by the recipe's fixed target, so they show the owner's colour choice only).
Measured at 300 px (`experiments/us10_targets.py`): sky median (upper 55 %), sky-background colour
(25–50 % band), and — independent of the edit — the scene's foreground-to-sky brightness in the
linear aurora-check render (lower 25 % vs upper 55 %).

| night (source) | n | sky median | background R/G, B/G |
|---|---|---|---|
| 2019 Nordnorwegen (own) | 20 | 0.238 (0.12–0.35) | 0.77, 1.37 |
| 2022-09-25 Tromsø (own) | 2 | 0.140 | 0.84, 1.18 |
| 2024-03-02 Dividalen (own) | 4 | 0.115 | 0.93, 0.55 |
| 2024-03-23 Dividalen (own) | 3 | 0.147 | 0.97, 1.11 |
| 2024-03-28 Finnland (own) | 1 | 0.076 | 0.72, 2.13 |
| 2026-01-11 (recipe, colour "B" chosen) | 24 | 0.231 | 0.96, 1.35 |
| 2026-01-23 (recipe, colour "C" chosen) | 16 | 0.233 | 1.06, 1.50 |
| 2026-03-10 Kvænangen (recipe, darker target chosen) | 2 | 0.144 | 0.99, 1.38 |
| 2026-09-05 Narvik-Alta (own) | 3 | 0.225 | 0.54, 0.93 |

- **Brightness:** one fixed target misses the owner's own edits by a median 0.44–0.47 EV (best at
  0.20–0.22); the owner's style changed between 2019 (≈ 0.24) and 2024 (≈ 0.13–0.16).
- **Scene dependence:** a bright foreground relative to the sky (scene ratio ≥ 0.6) goes with a
  darker sky — within 2019 0.243 → 0.195, within 2024 0.163 → 0.130, about −0.3 EV; the overall
  correlation (−0.48) is inflated by the year (7 of 8 bright-foreground scenes are from 2024). A
  two-level rule (0.24 / 0.14 at ratio 0.6) improves the median error only from 0.47 to 0.41 EV.
- **Colour:** the owner's own edits are spread widely (background B/G 0.55–2.13); the owner's
  explicit choices of 2026-11-22 are the clearest reference: neutral blue = R/G 0.95, B/G 1.35
  ("B"), blue-violet = 1.05, 1.50 ("C").

### 2026-11-26 — US-21, round 12: the foreground (brightness and colour, masked below the horizon)

Round 11's G1 (sky median 0.065) plus, masked below the hand-set horizon by a downward gradient:
H1 a foreground brightness step solved to the owner's foreground median; H2 also a color
calibration (CAT16) solved to the owner's foreground colour. Sheets
`photo-edits/_us21/round12/sheet_<photo>.png` (columns: owner's edit, G1, H1, H2; row 3 a
foreground crop); _DSC0914 has no foreground.

| photo | foreground L A / G1 / H1 / H2 | foreground R/G, B/G A / G1 / H2 | step |
|---|---|---|---|
| DSC00461 | 0.115 / 0.109 / 0.116 / 0.119 | 1.09, 0.55 / 1.20, 0.30 / 1.09, 0.54 | +0.10 EV |
| _DSC0905 | 0.099 / 0.007 / 0.101 / 0.101 | 0.99, 1.07 / 1.49, 3.18 / 0.99, 1.06 | +1.20 EV |
| _DSC0909 | 0.186 / 0.146 / 0.146 / 0.147 | 0.64, 0.83 / 0.87, 1.20 / 0.82, 1.17 | +2.00 (limit) |
| _DSC0926 | 0.097 / 0.126 / 0.098 / 0.100 | 0.97, 0.45 / 1.00, 0.34 / 0.97, 0.45 | −0.38 EV |
| _DSC4743 | 0.101 / 0.241 / 0.103 / 0.093 | 0.99, 1.07 / 0.73, 1.20 / 0.98, 1.08 | −1.25 EV |

- **A second exposure instance can't be used**: it turned the whole image white whatever its
  value, also without a mask. A second tone equalizer instance with the same gain on every band
  (no detail preservation) works as a per-pixel exposure and takes the mask.
- H2 reaches the owner's foreground brightness and colour on four photos: _DSC4743's rocks
  turn from bright cold-blue to neutral grey, _DSC0926's grass from garish yellow-green to the
  owner's darker tone, _DSC0905's almost black foreground gets visible.
- _DSC0909: only a thin strip below the horizon (0.97) — the solvers run into their limits.
- A brighter foreground lifts the sky near the horizon a little: the local-contrast steps work on
  neighbourhoods (an extreme +2 EV lifted the sky by up to 15 %).
- **Owner's verdict on round 12:** H2 is fine.

### 2026-11-26 — US-10: check round of the sky targets

11 aurora photos (reference set + 2026 favourites) in the US-18 base with the rule's targets — no
matching to the owner's edits; WB for the sky background (neutral blue; blue-violet for
2026-01-23); foreground colour correction (US-18) at hand-set horizons. The 0.6 threshold of the
rule as analysed gives all January 2026 photos (snowy ground, scene ratio 0.79–0.97) the dark
target, although the owner accepted the bright one for those nights → two thresholds rendered:
N 0.6, M 1.0 (only where the target differs). Sheets `photo-edits/_us10/check/sheet_<photo>.png`
(columns: owner-approved version, N, M).

| photo | scene ratio | N | M |
|---|---|---|---|
| _DSC3735 | 0.38 | 0.24 | 0.24 |
| _DSC6085 | 0.51 | 0.24 | 0.24 |
| _DSC7371 | 0.78 | 0.14 | 0.24 |
| _DSC8257 | 0.80 | 0.14 | 0.24 |
| _DSC8287 | 0.87 | 0.14 | 0.24 |
| _DSC8296 | 0.90 | 0.14 | 0.24 |
| _DSC7579 | 0.95 | 0.14 | 0.24 |
| _DSC8300 | 0.97 | 0.14 | 0.24 |
| _DSC8305 | 1.14 | 0.14 | 0.14 |
| _DSC7305 | 1.52 | 0.14 | 0.14 |

- January 2026 at 0.14 (N) is clearly darker than the accepted version, the aurora dim; at 0.24 (M)
  it is close to it and cleaner. The bright-foreground scenes (_DSC7305, _DSC8305) look right at
  0.14.
- _DSC3492 failed in the foreground solver (degenerate foreground, as in US-18 round 3).
- **Owner's decision:** threshold 1.0 (M). Some photos show more visible noise at the bright
  target; corrected by hand for now, a noise-aware rule is US-24 (backlog).

### 2026-11-26 — US-23: round 1, horizon from brightness edges

15 aurora and night photos with hand-set horizons (US-18, US-21), on the small linear renders of
US-7. Two simple detectors, both per vertical strip (12 strips) with a robust line through the
strip points (local experiment script): v1 the row with the largest brightness step, v2 the
first row that leaves a sky model (brightness and texture of the top 30 %). Sheet
`photo-edits/_us23/round1.png` (red hand line, yellow v1, green v2).

| detector | median error (frame height) | within 0.05 |
|---|---|---|
| v1 brightness step | 0.072 | 5 of 15 |
| v2 sky model | 0.059 | 5 of 15 |

- Brightness alone doesn't separate sky and land at night: the strongest step is often the edge of
  a snowfield or forest below the horizon (snow is as bright as an aurora sky), and aurora bands and
  the Milky Way leave the sky model. Clear-sky photos with a bright ground (_DSC0905) work.
- The hand lines mark where the edits' gradients should start, which is not always the land/sky
  edge (on _DSC8287 the treetops, not the snow edge) — a reference for a detector needs the true
  skyline. _DSC0909 has no horizon at all: the gradient in the owner's edit suppresses light
  pollution (owner).
- Hand-tuned rules on brightness won't reach a usable rate; next is a sky segmentation model.

### 2026-11-26 — US-23: round 2, sky segmentation model

SkyWater-Seg (SegFormer MiT-B2 fine-tuned on ADE20K for sky/water/person; MIT licence; 95 MB ONNX,
384×384 input) on the 130 night and aurora renders of US-7 (300×200), run from Rust with tract
(pure Rust, about 1 s per photo on the CPU; the model's stored intermediate shapes had to be
removed for tract). Input: the linear render brightened to its 99.5th percentile, sRGB. Skyline per
column: scanning up from the bottom, the first rows that are sky. Sheets in `photo-edits/_us23/`.

- The skyline follows peaks, ridges, treelines, tents and crosses closely — the shape a straight
  line can't give (DSC00461, the Rosengarten towers). Hand lines agree within 0.05 of the frame
  height on 10 of 14 photos; where they don't, the hand line is the gradient start, not the
  skyline. _DSC0909 comes out with no land in any column, which is right.
- Bright aurora is often not called sky (it looks like cloud to the model). Scanning from the bottom
  makes that harmless where the aurora doesn't reach the land; where it does (about 5 of 130,
  mostly 2019 Nordnorwegen), the skyline follows the aurora's edge.
- A greyscale input fixes the Milky Way frames the colour input calls non-sky, but loses much more
  aurora; taking a pixel as sky when any of colour, greyscale or a darker mapping (99.99th
  percentile) says so keeps the good of each. Left: the aurora-to-the-ground cases and one Milky Way
  frame with a partly non-sky mask.

### 2026-11-27 — US-23: round 3, higher resolution

The 130 renders again at 768 px wide. Model input variants, each as colour, greyscale and darker
mapping (sky where any of the three says so): (old) 300 px render at 384×384; (a) 768 px render
averaged down to 384×384; (b) 768 px render at 768×512, the photo's aspect kept. Sheets
`photo-edits/_us23/round3_*.png` (the 30 photos where any variant's skyline breaks up).

- (a) gets the aurora right where it reaches the land (2019 Nordnorwegen, the boulder scenes) —
  the averaging removes noise the 300 px render passes on. But clear Milky Way skies turn into
  "cloud": pure-sky frames get patches of non-sky and a broken skyline.
- (b) is worse than both: the model, trained at 384, sees more star detail than it knows.
- Sky where (old) or (a) says so takes the good of each: of 130, 3 left with a wrong skyline — a
  Milky Way frame without land (_DSC1176), the aurora reaching the land on _DSC7579 (middle third)
  and on _DSC8305 (beside the tent). Reference set: 8 of 14 within 0.05 of the hand lines, median
  0.043 (the hand lines are gradient starts).
- Cost: 2 small renders and 6 model runs per photo, about 10 s on the CPU.
