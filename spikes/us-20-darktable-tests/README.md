# US-20 spike — can Lua drive darktable for the `@darktable` scenarios?

`run.sh` starts darktable with a **throwaway** configuration (a copy of yours, for presets and
styles) and a throwaway library, with copies of two raws; your own library, configuration and
photos are not touched. `spike.lua` then works by itself: imports the copies, hands one of them
an edit through `apply_sidecar` (like the tool), tags it, and in separate darkroom visits
changes the exposure and undoes it, changes it, changes it twice, compresses the history.
After each visit it keeps a copy of the sidecar darktable writes (`results/`) and logs what
darktable reports.

```sh
spikes/us-20-darktable-tests/run.sh <edited raw (with .xmp)> <unedited raw>
# close darktable when results/spike.log says "done"
```

Results and pitfalls: journal, 2026-11-24 "US-20 spike". In the container it runs under a
virtual display (`xvfb-run`).
