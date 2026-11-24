# darktable setup

One-time steps in darktable that photoflow can't do itself. Once the Lua script exists, it
imports the styles automatically (US-17); the export preset stays a manual step because
darktable's Lua API can't create export presets.

## Print export with sharpening (US-5)

Edits made by photoflow contain no sharpening; the print sharpening is applied by the export.
Older edits already contain sharpening ("mehr schärfen") and must keep using the old preset —
exporting them with the new one sharpens them twice.

| Edit | Export preset |
|---|---|
| contains a sharpen step (all edits up to 2026) | **Fotobuch** (unchanged) |
| no sharpen step (new edits) | **Fotobuch (neu)** |

1. **Import the style** (only until the Lua script does it): lighttable → styles module →
   import → `darktable/styles/photoflow Druck v1.dtstyle` from this repository.
2. **Create the export preset:** in the export module, load the preset "Fotobuch", change only
   the style to "photoflow Druck v1" (mode: append), and save it as a new preset
   "Fotobuch (neu)". Everything else stays as in "Fotobuch": JPEG, full size, high quality,
   300 dpi, target `$(FILE_FOLDER)/fotobuch_exported/$(FILE_NAME)`.
3. Leave "Fotobuch" as it is.

"photoflow Druck v1" = the tone curve of "Aufhellung Druck" + sharpen "mehr schärfen"
(radius 3.0, amount 1.2, threshold 0.5). For an edit without sharpening it gives exactly the
same result as today's "Fotobuch" export of the same edit with sharpening.
