#!/usr/bin/env bash
# US-20 spike: runs darktable with a throwaway configuration and library, so your own
# library, configuration and photos are never touched.
# usage: run.sh <edited raw (its .xmp next to it)> <unedited raw>
# Results (log + sidecar snapshots) go to results/ next to this script.
set -euo pipefail
[ $# -eq 2 ] || { echo "usage: $0 <edited raw> <unedited raw>"; exit 1; }
here=$(cd "$(dirname "$0")" && pwd)
edited=$1; unedited=$2
[ -f "$edited.xmp" ] || { echo "$edited has no .xmp next to it"; exit 1; }
[ -f "$unedited.xmp" ] && echo "note: $unedited has a .xmp; it is not copied, so the copy is unedited"
own=${DARKTABLE_CONFIG:-$HOME/.config/darktable}
work=$(mktemp -d /tmp/photoflow-spike.XXXXXX)
mkdir -p "$work/config/lua" "$work/photos" "$work/edit" "$here/results"
rm -f "$here/results/"*
cp "$own/darktablerc" "$work/config/"
[ -f "$own/data.db" ] && cp "$own/data.db" "$work/config/"         # presets and styles
[ -d "$own/color" ] && cp -r "$own/color" "$work/config/"
cp "$here/spike.lua" "$work/config/lua/"
echo 'require "spike"' > "$work/config/luarc"
# both raws are imported without a sidecar; the edit is handed over later like the tool does it
cp "$edited" "$unedited" "$work/photos/"
cp "$edited.xmp" "$work/edit/"     # the edit, handed over later through apply_sidecar
echo "Starting darktable (throwaway config in $work). Don't touch it; it works by itself for"
echo "a few minutes. Close darktable when results/spike.log says 'done', or if nothing happens"
echo "for 5 minutes."
PHOTOFLOW_SPIKE_PHOTOS="$work/photos" PHOTOFLOW_SPIKE_EDIT="$work/edit/$(basename "$edited").xmp" PHOTOFLOW_SPIKE_RESULTS="$here/results" \
  darktable --configdir "$work/config" --library "$work/config/library.db" || true
echo "Finished. Results in $here/results; the throwaway folder $work can be deleted."
