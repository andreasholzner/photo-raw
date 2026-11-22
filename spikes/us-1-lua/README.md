# US-1 — Lua spike

Checks whether darktable's Lua API can carry the photoflow integration (ADR-0001, ADR-0002).
`photoflow_spike.lua` adds a **"photoflow spike"** panel to the lighttable (right side). Every
result is shown in the panel and appended to `~/.config/darktable/photoflow_spike.log`.

## Already checked in the dev container

darktable 5.6.1 (Lua API 9.7.0) under a virtual display, throwaway config, copies of 3 raws:

| Check | Result |
|---|---|
| Panel in the lighttable | works |
| Import events (per image and per folder), EXIF for day/night | works |
| Tags attach/detach, collection filtered to a tag and restored | works |
| Edit state of an image (`is_altered`) | works — tells edited from unedited |
| External command in a background job with progress | works (10 s job) |
| `apply_sidecar` of a file written outside darktable | works — the full history arrives in the library |

Not checkable there: whether the **UI stays usable** during the background job, how things
**look**, and whether darktable **rewrites the `.xmp` file** after `apply_sidecar` (in the
container the file stayed unchanged; darktable was killed rather than quit, which may matter).

## Running it on your machine

**Safety:** the checks that change images only touch images whose folder path contains
`photoflow-spike`. Your real photos are not touched; your library gets one extra film roll
and two tags, which the cleanup removes.

1. Make a test folder with **copies** of three raws, e.g. `~/Pictures/photoflow-spike/`:
   one **edited** raw together with its `.xmp`, and two **unedited** raws (without `.xmp`).
2. Install the script:
   ```sh
   mkdir -p ~/.config/darktable/lua
   cp photoflow_spike.lua ~/.config/darktable/lua/
   echo 'require "photoflow_spike"' >> ~/.config/darktable/luarc
   ```
3. Start darktable, go to the lighttable. The panel "photoflow spike" appears on the right
   (expand it). Its log shows `PASS panel: darktable …`.
4. **Import** the test folder (add to library). Expect one `event post-import-image` line per
   raw and `PASS import events`.
5. Select all three test images, then:
   - **2a** tag as to-do → **2b** show to-do images: the lighttable shows only these three →
     **2c** restore collection → **2d** remove to-do tag.
   - **3** background job: for the 10 seconds, scroll, select, open an image in darkroom.
     Watch the progress bar at the bottom left; try cancelling it once (click the ✕ on the
     progress bar) and run it again to the end.
   - **4** apply sidecar: copies the edit of the edited test image to an unedited one through
     a sidecar file written outside darktable. The unedited one should now look like the
     edited one; open it in darkroom and check its history.
   - **5** show edit state: lists `is_altered` and the history length in each `.xmp` file.
6. Quit darktable, then check whether the `.xmp` of the image from step 4 now contains the
   copied history: `grep history_end ~/Pictures/photoflow-spike/*.xmp`.
7. Start darktable again, press **6 clean up spike tags**, remove the test film roll from the
   library (remove, not delete), delete the test folder.
8. Uninstall: delete the `require "photoflow_spike"` line from `~/.config/darktable/luarc` and
   `~/.config/darktable/lua/photoflow_spike.lua`.

## What to report back

- The log file `~/.config/darktable/photoflow_spike.log`.
- Was darktable usable during the background job? Did cancelling work?
- Did step 4 visibly change the image, and did its `.xmp` contain the history after quitting
  (step 6)?
- Anything that looked odd (panel placement, error popups).
