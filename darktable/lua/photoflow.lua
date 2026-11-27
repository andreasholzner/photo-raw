--[[
  photoflow's darktable script (US-8): hands photos over to the photoflow program and carries out
  its plan in darktable.

  Which photos are handed over: the newly imported ones (can be switched off in darktable's
  preferences, "lua options"), or the ones the owner starts the tool on with the shortcut
  "photoflow: process photos" (the photos darktable acts on: hovered or selected).

  For each run the script makes darktable rewrite the photos' sidecars, writes a hand-over file
  (what darktable knows about each photo), runs `photoflow process` in the background and applies
  the plan it writes, so darktable stays the only writer of its library and of the
  sidecars (ADR-0001, ADR-0002): darktable applies the tool's edits (`apply_sidecar`) and attaches its
  tags. File formats: see the photoflow source, src/process.rs.
]]

local dt = require "darktable"

local MODULE = "photoflow"
local TOUCH_TAG = "photoflow|touch"
local CLEARED = { "photoflow|class|", "photoflow|open|", "photoflow|left alone|" }
local EDIT = "photoflow|edit|"

dt.preferences.register(MODULE, "binary", "string", "photoflow: program",
  "path of the photoflow program", "photoflow")
dt.preferences.register(MODULE, "config", "string", "photoflow: configuration file",
  "empty: photoflow's default (~/.config/photoflow/config.toml)", "")
dt.preferences.register(MODULE, "after_import", "bool", "photoflow: process imported photos",
  "hand newly imported photos over to photoflow", true)

-- the self-test drives the script through this table
local M = { busy = false, runs = 0, last_error = nil }
photoflow = M

local function read(path)
  local f = io.open(path, "r"); if not f then return nil end
  local s = f:read("a"); f:close(); return s
end

local function quote(s) return "'" .. tostring(s):gsub("'", "'\\''") .. "'" end

local function raw_path(img) return img.path .. "/" .. img.filename end

local function has_prefix(s, p) return s:sub(1, #p) == p end

-- Waits until every sidecar does (want == true) or doesn't contain the touch tag. Returns the
-- images whose sidecar didn't get there within 30 s.
local function wait_sidecars(images, want)
  local late = {}
  for _, img in ipairs(images) do late[#late + 1] = img end
  for _ = 1, 60 do
    local still = {}
    for _, img in ipairs(late) do
      local text = read(img.sidecar) or ""
      if (text:find(TOUCH_TAG, 1, true) ~= nil) ~= want then still[#still + 1] = img end
    end
    late = still
    if #late == 0 then return late end
    dt.control.sleep(500)
  end
  return late
end

-- darktable writes sidecars in the background, and changes made in the darkroom only when it
-- is left: a tag added and removed again proves two fresh writes (journal 2026-11-24/26).
local function refresh_sidecars(images)
  local tag = dt.tags.create(TOUCH_TAG)
  for _, img in ipairs(images) do img:attach_tag(tag) end
  local late1 = wait_sidecars(images, true)
  for _, img in ipairs(images) do img:detach_tag(tag) end
  local late2 = wait_sidecars(images, false)
  local bad = {}
  for _, img in ipairs(late1) do bad[img] = true end
  for _, img in ipairs(late2) do bad[img] = true end
  return bad
end

local function apply_plan(text, by_sidecar)
  for line in text:gmatch("[^\n]+") do
    local key, action, arg = line:match("^([^\t]*)\t([^\t]*)\t?(.*)$")
    local img = by_sidecar[key]
    if img and action == "clear" then
      for _, t in ipairs(img:get_tags()) do
        for _, p in ipairs(CLEARED) do
          if has_prefix(t.name, p) then img:detach_tag(t); break end
        end
      end
    elseif img and action == "apply" then
      -- the tool's new edit replaces its earlier one, and with it the earlier fingerprint
      for _, t in ipairs(img:get_tags()) do
        if has_prefix(t.name, EDIT) then img:detach_tag(t) end
      end
      img:apply_sidecar(arg)
    elseif img and action == "tag" then
      img:attach_tag(dt.tags.create(arg))
    elseif img and action == "untag" then
      for _, t in ipairs(img:get_tags()) do
        if has_prefix(t.name, arg) then img:detach_tag(t) end
      end
    elseif img and action == "duplicate" then
      -- a comparison duplicate (US-10): the tool's edit in the other sky colour
      local xmp, tags = arg:match("^([^\t]*)\t?(.*)$")
      local dup = dt.database.duplicate(img)
      dup:apply_sidecar(xmp)
      for t in tags:gmatch("[^\31]+") do dup:attach_tag(dt.tags.create(t)) end
    elseif img and action == "remove" then
      -- only the tool's own comparison duplicates, unchanged (US-10); never an original
      if img.duplicate_index > 0 then dt.database.delete(img) end
    end
  end
end

local function run(images, job)
  local by_sidecar = {}
  for _, img in ipairs(images) do by_sidecar[img.sidecar] = img end
  local bad = refresh_sidecars(images)
  if job.valid then job.percent = 0.3 end

  local handover, plan = os.tmpname(), os.tmpname()
  local lines = { "darkroom\t" .. (dt.gui.current_view().id == "darkroom" and "1" or "0") }
  for _, img in ipairs(images) do
    if bad[img] then
      img:attach_tag(dt.tags.create("photoflow|left alone|sidecar not up to date"))
    else
      local tags = {}
      for _, t in ipairs(img:get_tags()) do tags[#tags + 1] = t.name end
      lines[#lines + 1] = table.concat({ "photo", raw_path(img), img.sidecar,
        img.is_altered and "1" or "0", table.concat(tags, "\31") }, "\t")
    end
  end
  local f = assert(io.open(handover, "w")); f:write(table.concat(lines, "\n"), "\n"); f:close()

  local config = dt.preferences.read(MODULE, "config", "string")
  local cmd = quote(dt.preferences.read(MODULE, "binary", "string"))
    .. (config ~= "" and (" --config " .. quote(config)) or "")
    .. " process " .. quote(handover) .. " " .. quote(plan)
    .. " 2>>" .. quote(dt.configuration.tmp_dir .. "/photoflow.log")
  local rc = dt.control.execute(cmd)
  if job.valid then job.percent = 0.9 end
  local text = read(plan)
  os.remove(handover); os.remove(plan)
  if rc ~= 0 or not text then
    error("photoflow process failed (exit " .. tostring(rc) .. "), see " ..
      dt.configuration.tmp_dir .. "/photoflow.log")
  end
  apply_plan(text, by_sidecar)
end

local queue = {}

--- Hands `images` over to photoflow in the background; runs one after another.
function M.process(images)
  if #images == 0 then return end
  queue[#queue + 1] = images
  if M.busy then return end
  M.busy = true
  dt.control.dispatch(function()
    while #queue > 0 do
      local next_images = table.remove(queue, 1)
      local job = dt.gui.create_job("photoflow: " .. #next_images .. " photos", true,
        function(j) j.valid = false end)
      local ok, err = pcall(run, next_images, job)
      if job.valid then job.valid = false end
      if not ok then M.last_error = tostring(err); dt.print("photoflow: " .. tostring(err)) end
      M.runs = M.runs + 1
    end
    M.busy = false
  end)
end

-- Panorama sets (US-12): a set is a tag named after its first photo; proposals come from the tool.
local SET = "photoflow|panorama|"
local PROPOSAL = "photoflow|panorama proposal|"
local REJECTED = "photoflow|not a panorama"
local PANO_OPEN = "photoflow|open|panorama sets to confirm"

local function detach_prefix(img, prefix)
  for _, t in ipairs(img:get_tags()) do
    if has_prefix(t.name, prefix) then img:detach_tag(t) end
  end
end

local function proposal_of(img)
  for _, t in ipairs(img:get_tags()) do
    if has_prefix(t.name, PROPOSAL) then return t.name end
  end
end

local function first_name(images)
  local first
  for _, img in ipairs(images) do
    local key = (img.exif_datetime_taken or "") .. "\t" .. img.filename
    if not first or key < first.key then first = { key = key, img = img } end
  end
  return first and first.img.filename:gsub("%.[^.]*$", "")
end

local function retire_proposal(img)
  detach_prefix(img, PROPOSAL); detach_prefix(img, PANO_OPEN)
end

--- Marks `images` as one panorama set.
function M.mark_set(images)
  if #images == 0 then return end
  local tag = dt.tags.create(SET .. first_name(images))
  for _, img in ipairs(images) do
    retire_proposal(img); detach_prefix(img, REJECTED); img:attach_tag(tag)
  end
end

local function proposals()
  local groups, names = {}, {}
  for _, img in ipairs(dt.database) do
    local p = proposal_of(img)
    if p then
      if not groups[p] then groups[p] = {}; names[#names + 1] = p end
      table.insert(groups[p], img)
    end
  end
  table.sort(names)
  return groups, names
end

--- Shows the folder of `images` in the lighttable and selects them (darktable selects only photos
--- of the current collection).
function M.show_and_select(images)
  if #images == 0 then return end
  local rule = dt.gui.libs.collect.new_rule()
  rule.mode = "DT_LIB_COLLECT_MODE_AND"; rule.item = "DT_COLLECTION_PROP_FILMROLL"; rule.data = images[1].path
  dt.gui.libs.collect.filter({ rule })
  dt.control.sleep(500)
  dt.gui.selection(images)
end

--- Selects the photos of the proposed set `name` (its tag); returns whether there was one.
function M.select_proposal(name)
  local groups = proposals()
  if not groups[name] then return false end
  M.show_and_select(groups[name])
  return true
end

local last_proposal = nil
--- Selects the photos of the next proposed set; returns its tag, or nil when there is none.
function M.select_next_proposal()
  local _, names = proposals()
  if #names == 0 then return nil end
  local pick = names[1]
  for _, n in ipairs(names) do
    if last_proposal and n > last_proposal then pick = n; break end
  end
  last_proposal = pick
  M.select_proposal(pick)
  return pick
end

--- Confirms the proposal the selected photos come from: the selection becomes the set.
function M.confirm_set(images)
  if #images == 0 then return end
  local p
  for _, img in ipairs(images) do p = p or proposal_of(img) end
  if p then
    local groups = proposals()
    for _, img in ipairs(groups[p] or {}) do retire_proposal(img) end
  end
  M.mark_set(images)
end

--- Rejects the proposal the selected photos come from: none of its photos is proposed again.
function M.reject_set(images)
  local groups = proposals()
  for _, img in ipairs(images) do
    local p = proposal_of(img)
    for _, other in ipairs(p and groups[p] or { img }) do
      retire_proposal(other); other:attach_tag(dt.tags.create(REJECTED))
    end
  end
end

local function selected()
  local out = {}
  for _, img in ipairs(dt.gui.selection()) do out[#out + 1] = img end
  return out
end

dt.register_event(MODULE .. "_pano_mark", "shortcut", function() M.mark_set(selected()) end,
  "photoflow: mark selection as panorama set")
dt.register_event(MODULE .. "_pano_next", "shortcut", function() M.select_next_proposal() end,
  "photoflow: select next proposed panorama set")
dt.register_event(MODULE .. "_pano_confirm", "shortcut", function() M.confirm_set(selected()) end,
  "photoflow: confirm panorama set")
dt.register_event(MODULE .. "_pano_reject", "shortcut", function() M.reject_set(selected()) end,
  "photoflow: not a panorama")

local imported = {}
dt.register_event(MODULE, "post-import-image", function(_, img)
  if dt.preferences.read(MODULE, "after_import", "bool") then imported[#imported + 1] = img end
end)
dt.register_event(MODULE, "post-import-film", function()
  local images = imported; imported = {}
  M.process(images)
end)
dt.register_event(MODULE, "shortcut", function()
  local images = {}
  for _, img in ipairs(dt.gui.action_images) do images[#images + 1] = img end
  M.process(images)
end, "photoflow: process photos")
