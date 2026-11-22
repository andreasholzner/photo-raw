--[[
  photoflow spike (US-1): can darktable's Lua API carry the photoflow integration?

  Adds a "photoflow spike" panel to the lighttable (right side) with one button per check.
  Every result is shown in the panel and appended to <darktable config dir>/photoflow_spike.log.

  Safety: the checks that change images (tags, sidecar) only touch images whose folder path
  contains "photoflow-spike". Use a test folder with copies of a few raws.
  See README.md next to this file.
]]

local dt = require "darktable"

local MODULE = "photoflow_spike"
local SAFE_MARK = "photoflow-spike"
local TODO_TAG = "photoflow|todo|spike"
local IMPORT_TAG = "photoflow|spike|imported"
local LOGFILE = dt.configuration.config_dir .. "/photoflow_spike.log"

-- ---------------------------------------------------------------------------------------
-- logging: panel, darktable log, file
-- ---------------------------------------------------------------------------------------

local lines = {}
local log_label = dt.new_widget("label") { label = "", halign = "start", selectable = true }

local function log(msg)
  local line = os.date("%H:%M:%S ") .. msg
  table.insert(lines, line)
  while #lines > 14 do table.remove(lines, 1) end
  log_label.label = table.concat(lines, "\n")
  dt.print_log("[photoflow_spike] " .. msg)
  local f = io.open(LOGFILE, "a")
  if f then f:write(os.date("%Y-%m-%d ") .. line .. "\n"); f:close() end
end

local function result(check, ok, detail)
  log(string.format("%s %s%s", ok and "PASS" or "FAIL", check, detail and (": " .. detail) or ""))
end

local function safe(image)
  return string.find(image.path, SAFE_MARK, 1, true) ~= nil
end

local function safe_selection(min)
  local sel = dt.gui.selection()
  local out = {}
  for _, img in ipairs(sel) do
    if safe(img) then table.insert(out, img) end
  end
  if #out < min then
    dt.print(string.format("select at least %d image(s) from a folder containing '%s'", min, SAFE_MARK))
    return nil
  end
  return out
end

local function ev100(img)
  local t, f, iso = img.exif_exposure, img.exif_aperture, img.exif_iso
  if not t or t <= 0 or not iso or iso <= 0 then return nil end
  if not f or f <= 0 then f = 2.0 end -- manual lens: no aperture in EXIF
  return math.log(f * f / t, 2) - math.log(iso / 100, 2)
end

-- ---------------------------------------------------------------------------------------
-- check 1: import events
-- ---------------------------------------------------------------------------------------

dt.register_event(MODULE, "post-import-image", function(event, image)
  local ev = ev100(image)
  log(string.format("event post-import-image: %s (EV100 %s, altered %s)", image.filename,
    ev and string.format("%.1f", ev) or "?", tostring(image.is_altered)))
  if safe(image) then image:attach_tag(dt.tags.create(IMPORT_TAG)) end
end)

dt.register_event(MODULE, "post-import-film", function(event, film)
  result("import events", true, string.format("post-import-film: %s (%d images)", film.path, #film))
end)

-- ---------------------------------------------------------------------------------------
-- check 2: tags as to-do markers + showing them as a collection
-- ---------------------------------------------------------------------------------------

local saved_rules = nil

local function tag_selection()
  local imgs = safe_selection(1); if not imgs then return end
  local tag = dt.tags.create(TODO_TAG)
  for _, img in ipairs(imgs) do img:attach_tag(tag) end
  local found = dt.tags.find(TODO_TAG)
  result("attach tag", found ~= nil and #found >= #imgs, string.format("%d image(s) carry %s", found and #found or 0, TODO_TAG))
end

local function show_tagged()
  local rule = dt.gui.libs.collect.new_rule()
  rule.mode = "DT_LIB_COLLECT_MODE_AND"
  rule.item = "DT_COLLECTION_PROP_TAG"
  rule.data = TODO_TAG
  local prev = dt.gui.libs.collect.filter({ rule })
  if not saved_rules then saved_rules = prev end
  log("collection set to tag " .. TODO_TAG .. " - check the lighttable shows exactly the tagged images")
end

local function restore_collection()
  if saved_rules then dt.gui.libs.collect.filter(saved_rules); saved_rules = nil end
  log("collection restored")
end

local function untag_all()
  local tag = dt.tags.find(TODO_TAG)
  if not tag then log("no images carry " .. TODO_TAG); return end
  local imgs = {}
  for i = 1, #tag do table.insert(imgs, tag[i]) end
  for _, img in ipairs(imgs) do img:detach_tag(tag) end
  local left = #tag
  tag:delete()
  result("detach tag", left == 0, string.format("removed from %d image(s)", #imgs))
end

-- ---------------------------------------------------------------------------------------
-- check 3: long external command in the background with progress and cancel
-- ---------------------------------------------------------------------------------------

local function background_job()
  local steps = 10
  local job = dt.gui.create_job("photoflow spike: 10 s external command", true, function(j) j.valid = false end)
  log("background job started - keep using darktable (scroll, select, open darkroom) and watch the progress bar")
  local function work()
    local t0 = os.time()
    for i = 1, steps do
      if not job.valid then
        result("background job", true, string.format("cancelled after %d s", os.time() - t0))
        return
      end
      dt.control.execute("sleep 1")
      job.percent = i / steps
    end
    job.valid = false
    result("background job", true, string.format("finished in %d s - was the UI usable meanwhile? note it in the log", os.time() - t0))
  end
  if dt.control.dispatch then
    dt.control.dispatch(work)
  else
    log("dt.control.dispatch not available - running inline")
    work()
  end
end

-- ---------------------------------------------------------------------------------------
-- check 4: a sidecar written outside darktable gets into the library (apply_sidecar)
-- ---------------------------------------------------------------------------------------

local function read_file(p)
  local f = io.open(p, "r"); if not f then return nil end
  local s = f:read("a"); f:close(); return s
end

-- the Lua API has no "write sidecar" call: darktable writes sidecars itself (5.6 does it
-- asynchronously), so the file is only read here
local function history_count(img)
  local s = read_file(img.sidecar)
  if not s then return nil end
  local n = string.match(s, 'history_end="(%d+)"')
  return tonumber(n) or 0
end

local function sidecar_roundtrip()
  local imgs = safe_selection(2); if not imgs then return end
  local src, dst
  for _, img in ipairs(imgs) do
    if img.is_altered and not src then src = img elseif not img.is_altered and not dst then dst = img end
  end
  if not src or not dst then
    dt.print("select one edited and one unedited test image")
    return
  end
  local altered_before, file_before = dst.is_altered, history_count(dst)
  -- simulate the external tool: it writes a sidecar file somewhere outside darktable
  local tmp = os.tmpname() .. ".xmp"
  local rc = dt.control.execute(string.format("cp %q %q", src.sidecar, tmp))
  if rc ~= 0 then result("sidecar", false, "external copy failed (rc " .. tostring(rc) .. ")"); return end
  local ok, err = pcall(function() dst:apply_sidecar(tmp) end)
  os.remove(tmp)
  if not ok then result("apply_sidecar", false, tostring(err)); return end
  dst:drop_cache()
  result("apply_sidecar (library)", dst.is_altered and not altered_before,
    string.format("%s altered %s -> %s (edit copied from %s); open it in darkroom to see the history",
      dst.filename, tostring(altered_before), tostring(dst.is_altered), src.filename))
  dt.control.sleep(5000)
  log(string.format("%s sidecar file history_end: %s before, %s 5 s after", dst.filename,
    tostring(file_before), tostring(history_count(dst))))
end

-- ---------------------------------------------------------------------------------------
-- check 5: telling manual edits apart
-- ---------------------------------------------------------------------------------------

local function show_altered()
  for _, img in ipairs(dt.gui.selection()) do
    log(string.format("%s: is_altered=%s, sidecar history_end=%s", img.filename, tostring(img.is_altered),
      tostring(history_count(img))))
  end
end

-- ---------------------------------------------------------------------------------------
-- cleanup
-- ---------------------------------------------------------------------------------------

local function cleanup()
  for _, name in ipairs({ TODO_TAG, IMPORT_TAG }) do
    local tag = dt.tags.find(name)
    if tag then
      local imgs = {}
      for i = 1, #tag do table.insert(imgs, tag[i]) end
      for _, img in ipairs(imgs) do img:detach_tag(tag) end
      tag:delete()
      log("deleted tag " .. name)
    end
  end
end

-- ---------------------------------------------------------------------------------------
-- panel
-- ---------------------------------------------------------------------------------------

local function button(label, fn)
  return dt.new_widget("button") { label = label, clicked_callback = function() fn() end }
end

local panel = dt.new_widget("box") {
  orientation = "vertical",
  dt.new_widget("label") { label = "test images: folder must contain '" .. SAFE_MARK .. "'", halign = "start" },
  button("2a tag selection as to-do", tag_selection),
  button("2b show to-do images", show_tagged),
  button("2c restore collection", restore_collection),
  button("2d remove to-do tag", untag_all),
  button("3 background job (10 s)", background_job),
  button("4 apply sidecar: edited -> unedited", sidecar_roundtrip),
  button("5 show edit state of selection", show_altered),
  button("6 clean up spike tags", cleanup),
  dt.new_widget("separator") {},
  log_label,
}

local installed = false
local function install()
  if installed then return end
  dt.register_lib(MODULE, "photoflow spike", true, false,
    { [dt.gui.views.lighttable] = { "DT_UI_CONTAINER_PANEL_RIGHT_CENTER", 100 } }, panel, nil, nil)
  installed = true
  result("panel", true, string.format("darktable %s, Lua API %s", dt.configuration.version, dt.configuration.api_version_string))
end

if dt.gui.current_view().id == "lighttable" then
  install()
else
  dt.register_event(MODULE, "view-changed", function(event, old_view, new_view)
    if new_view.id == "lighttable" then install() end
  end)
end

-- ---------------------------------------------------------------------------------------
-- unattended self-check, only used to test this script in a headless container:
-- PHOTOFLOW_SPIKE_AUTOTEST=<dir with test raws> imports the dir, runs the checks, quits.
-- ---------------------------------------------------------------------------------------

local autodir = os.getenv("PHOTOFLOW_SPIKE_AUTOTEST")
if autodir then
  dt.control.dispatch(function()
    dt.control.sleep(3000)
    dt.database.import(autodir)
    dt.control.sleep(2000)
    local imgs = {}
    for _, img in ipairs(dt.database) do if safe(img) then table.insert(imgs, img) end end
    table.sort(imgs, function(a, b) return a.filename < b.filename end)
    dt.gui.selection(imgs)
    tag_selection(); show_tagged(); dt.control.sleep(500); restore_collection(); untag_all()
    sidecar_roundtrip(); show_altered()
    background_job(); dt.control.sleep(12000)
    cleanup()
    log("autotest done") -- no Lua call quits darktable; the test harness kills it
  end)
end
