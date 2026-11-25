--[[
  US-20 spike: can a Lua script drive the darktable behaviour that US-8's scenarios rely on,
  unattended? Started by run.sh with a throwaway configuration and library — never the owner's.

  Environment: PHOTOFLOW_SPIKE_PHOTOS = folder with two raws without sidecars;
  PHOTOFLOW_SPIKE_EDIT = the edit for one of them (<raw>.xmp, elsewhere; a name not ending in
  .xmp is silently ignored by apply_sidecar); PHOTOFLOW_SPIKE_RESULTS = folder for the log and the sidecar snapshots.
  Steps: import → hand over the edit with apply_sidecar → tag → in separate darkroom visits:
  change + undo, a change, two changes, compress the history. After every visit darktable writes
  the sidecar; each version is copied and the time it took is logged.
]]

local dt = require "darktable"
local photos = os.getenv("PHOTOFLOW_SPIKE_PHOTOS")
local edit = os.getenv("PHOTOFLOW_SPIKE_EDIT")
local results = os.getenv("PHOTOFLOW_SPIKE_RESULTS")
if not photos or not edit or not results then return end
local LOG = results .. "/spike.log"
local T0 = os.time()

local function log(fmt, ...)
  local f = io.open(LOG, "a")
  f:write(string.format("%4ds ", os.time() - T0) .. string.format(fmt, ...) .. "\n"); f:close()
end

local function read(path)
  local f = io.open(path, "r"); if not f then return nil end
  local s = f:read("a"); f:close(); return s
end

local function sidecar_info(img)
  local s = read(img.sidecar)
  if not s then return nil, "no sidecar" end
  local hist = s:match("<darktable:history>.-</darktable:history>") or ""
  local n = select(2, string.gsub(hist, "<rdf:li", ""))
  local ts = s:match('change_timestamp="([-%d]+)"')
  return ts, string.format("history_end=%s entries=%d change_timestamp=%s",
    s:match('history_end="(%d+)"') or "?", n, ts or "none")
end

local function state(label, img)
  local ok, ts = pcall(function() return img.change_timestamp end)
  local tags = {}
  for _, t in ipairs(img:get_tags()) do tags[#tags + 1] = t.name end
  local _, info = sidecar_info(img)
  log("[%s] %s altered=%s lua_change_timestamp=%s(%s) tags=%s | sidecar %s", label, img.filename,
    tostring(img.is_altered), tostring(ts), ok and type(ts) or "error", table.concat(tags, ","), info)
end

local function snapshot(img, name)
  local s = read(img.sidecar)
  if s then local f = io.open(results .. "/" .. name .. ".xmp", "w"); f:write(s); f:close() end
end

-- Waits until darktable has written a sidecar with a change time other than `before`.
local function wait_for_sidecar(img, before, label)
  local t = os.time()
  for _ = 1, 120 do
    local ts = sidecar_info(img)
    if ts and ts ~= before and ts ~= "-1" then
      log("%s: sidecar written after %d s", label, os.time() - t); return ts
    end
    dt.control.sleep(500)
  end
  log("%s: sidecar NOT written within 60 s", label); return before
end

local function try(label, f)
  local ok, r = pcall(f)
  log("%s -> %s %s", label, ok and "ok" or "ERROR", tostring(r))
  return ok, r
end

local function exposure()
  return dt.gui.action("iop/exposure/exposure", "value", "", 0)
end

dt.control.dispatch(function()
  dt.control.sleep(3000)
  log("start: darktable %s, Lua API %s, locale %s, os.date %s", dt.configuration.version,
    dt.configuration.api_version_string, tostring(os.setlocale()), os.date("%c"))
  dt.database.import(photos)
  dt.control.sleep(3000)
  local A, B
  local n = 0
  for _, img in ipairs(dt.database) do
    n = n + 1
    if edit:sub(-#img.filename - 4) == img.filename .. ".xmp" then B = img else A = img end
  end
  if not A or not B then log("need one edited and one unedited raw, found %d images", n); return end
  state("1 after import, unedited", A); state("1 after import, to be edited", B)

  -- 2: the edited photo's edit stands for the tool's edit
  -- loaded through apply_sidecar, the way the tool hands over its edit; darktable then writes it
  local before = sidecar_info(B)
  try("2 apply_sidecar", function() return B:apply_sidecar(edit) end)
  dt.control.sleep(1000); state("2 applied", B)
  -- does apply_sidecar alone make darktable write the sidecar?
  local ts = before
  for _ = 1, 40 do
    dt.control.sleep(500); ts = sidecar_info(B)
    if ts ~= before then break end
  end
  log("2 sidecar after apply_sidecar alone: %s", ts ~= before and "written" or "NOT written within 20 s")
  try("2 attach tag", function() B:attach_tag(dt.tags.create("photoflow|edit|spike")) end)
  ts = wait_for_sidecar(B, before, "2 baseline (after tagging)")
  snapshot(B, "1-baseline")
  state("2 tagged", B)

  -- 3: darkroom on B (it must be in the lighttable's collection to be opened)
  local rule = dt.gui.libs.collect.new_rule()
  rule.mode = "DT_LIB_COLLECT_MODE_AND"; rule.item = "DT_COLLECTION_PROP_FILMROLL"; rule.data = B.film.path
  dt.gui.libs.collect.filter({ rule })
  dt.gui.selection({ B })
  dt.control.sleep(2000)
  try("3 enter darkroom", function() dt.gui.current_view(dt.gui.views.darkroom) end)
  dt.control.sleep(8000)
  try("3 view", function() return dt.gui.current_view().id end)
  try("3 exposure (slider position)", exposure)

  -- Each step is one visit to the darkroom: darktable writes the sidecar only after the darkroom is
  -- left (not within 5 minutes while the image stayed open), and its undo list lives only as long
  -- as the visit.
  local function set(v) try("set exposure " .. v, function() return dt.gui.action("iop/exposure/exposure", "value", "set", v) end) end
  local function leave(label, name)
    try(label .. " leave darkroom", function() dt.gui.current_view(dt.gui.views.lighttable) end)
    ts = wait_for_sidecar(B, ts, label)
    snapshot(B, name); state(label, B)
    dt.control.sleep(2000)
  end
  local function enter(label)
    dt.gui.selection({ B })
    try(label .. " enter darkroom", function() dt.gui.current_view(dt.gui.views.darkroom) end)
    dt.control.sleep(8000)
  end

  -- 4: change and undo in the same visit: expected to equal the baseline
  set(2.0); dt.control.sleep(2000); try("4 exposure after change (slider position)", exposure)
  try("4 undo", function() return dt.gui.action("views/darkroom/undo", "", "", 1.0) end)
  dt.control.sleep(2000); try("4 exposure after undo (slider position)", exposure)
  leave("4 changed and undone", "2-undone")

  -- 5: a change by hand
  enter("5"); set(2.0); dt.control.sleep(2000)
  leave("5 changed", "3-changed")

  -- 6: two more changes, then compress the history in a separate visit
  enter("6"); set(1.0); dt.control.sleep(3000); set(1.5); dt.control.sleep(2000)
  leave("6 two changes", "4-two-changes")
  enter("7")
  try("7 compress history", function() return dt.gui.action("lib/history/compress history stack", "", "", 1.0) end)
  dt.control.sleep(3000)
  leave("7 compressed", "5-compressed")

  try("8 back to lighttable", function() dt.gui.current_view(dt.gui.views.lighttable) end)
  state("8 end, unedited", A)
  log("done — you can close darktable now")
end)
