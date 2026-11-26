--[[
  photoflow's darktable script (US-8): hands photos over to the photoflow program and carries out
  its plan in darktable.

  Which photos are handed over: the newly imported ones (can be switched off in darktable's
  preferences, "lua options"), or the ones the owner starts the tool on with the shortcut
  "photoflow: process photos" (the photos darktable acts on: hovered or selected).

  For each run the script makes darktable rewrite the photos' sidecars, writes a hand-over file
  (what darktable knows about each photo), runs `photoflow process` in the background and applies
  the plan it writes: tags only, so darktable stays the only writer of its library and of the
  sidecars (ADR-0001, ADR-0002). File formats: see the photoflow source, src/process.rs.
]]

local dt = require "darktable"

local MODULE = "photoflow"
local TOUCH_TAG = "photoflow|touch"
local CLEARED = { "photoflow|class|", "photoflow|open|", "photoflow|left alone|" }

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

local function apply_plan(text, by_path)
  for line in text:gmatch("[^\n]+") do
    local raw, action, arg = line:match("^([^\t]*)\t([^\t]*)\t?(.*)$")
    local img = by_path[raw]
    if img and action == "clear" then
      for _, t in ipairs(img:get_tags()) do
        for _, p in ipairs(CLEARED) do
          if has_prefix(t.name, p) then img:detach_tag(t); break end
        end
      end
    elseif img and action == "tag" then
      img:attach_tag(dt.tags.create(arg))
    end
  end
end

local function run(images, job)
  local by_path = {}
  for _, img in ipairs(images) do by_path[raw_path(img)] = img end
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
  apply_plan(text, by_path)
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
