--[[
  photoflow self-test driver (US-20). Loaded only into the self-test's private darktable
  configuration, never into the owner's.

  photoflow writes commands to $PHOTOFLOW_SELFTEST_IPC/cmd-<n> (tab-separated: name, arguments)
  and waits for reply-<n> ("ok<TAB>result" or "error<TAB>message"). Images are addressed by the
  full path of their raw file. What works and what doesn't was found in the US-20 spike
  (journal 2026-11-24).
]]

local dt = require "darktable"
local ipc = os.getenv("PHOTOFLOW_SELFTEST_IPC")
if not ipc then return end

local TOUCH_TAG = "photoflow|selftest|touch"
local loaded = nil -- raw path of the image darktable last loaded into the darkroom

dt.register_event("photoflow_selftest", "darkroom-image-loaded", function(_, _, img)
  loaded = img.path .. "/" .. img.filename
end)

local function read(path)
  local f = io.open(path, "r"); if not f then return nil end
  local s = f:read("a"); f:close(); return s
end

local function write(path, text)
  local f = assert(io.open(path .. ".tmp", "w")); f:write(text); f:close()
  assert(os.rename(path .. ".tmp", path))
end

local function image(path)
  for _, img in ipairs(dt.database) do
    if img.path .. "/" .. img.filename == path then return img end
  end
  error("not in the library: " .. path)
end

-- switching views happens in the background: wait until it's done
local function switch_to(view, name)
  if dt.gui.current_view().id == name then return end
  dt.gui.current_view(view)
  for _ = 1, 120 do
    if dt.gui.current_view().id == name then dt.control.sleep(1000); return end
    dt.control.sleep(500)
  end
  error("view didn't switch to " .. name .. " within 60 s")
end

local function lighttable() switch_to(dt.gui.views.lighttable, "lighttable") end

local commands = {}

-- import runs in the background: wait until the folder's images are in the library
function commands.import(dir)
  dt.database.import(dir)
  for _ = 1, 240 do
    for _, img in ipairs(dt.database) do
      if img.path == dir then dt.control.sleep(1000); return "" end
    end
    dt.control.sleep(500)
  end
  error("import of " .. dir .. " didn't finish within 120 s")
end

function commands.state(path)
  local img = image(path)
  local tags = {}
  for _, t in ipairs(img:get_tags()) do tags[#tags + 1] = t.name end
  return (img.is_altered and "1" or "0") .. "\t" .. dt.gui.current_view().id .. "\t" .. table.concat(tags, "\31")
end

function commands.apply_sidecar(path, xmp) return tostring(image(path):apply_sidecar(xmp)) end

-- a tag change makes darktable write the sidecar at once
function commands.touch(path)
  local img = image(path)
  local tag = dt.tags.create(TOUCH_TAG)
  img:attach_tag(tag); img:detach_tag(tag)
  return ""
end

function commands.tag(path, name) image(path):attach_tag(dt.tags.create(name)); return "" end

function commands.import_style(file) dt.styles.import(file); return "" end

function commands.apply_style(name, path)
  for _, st in ipairs(dt.styles) do
    if st.name == name then dt.styles.apply(st, image(path)); return "" end
  end
  error("no style " .. name)
end

-- The darkroom opens the image darktable acts on: it has to be in the lighttable's collection.
-- With the mouse pointer inside darktable's window that is the image darktable last had "under
-- the mouse" — after leaving the darkroom, the previous photo — not the selected one; switching
-- the darkroom to the right image afterwards didn't make the following changes land on it.
-- Acting before darktable has loaded the image crashed it in the spike: wait for the event.
function commands.darkroom(path)
  local img = image(path)
  lighttable()
  local rule = dt.gui.libs.collect.new_rule()
  rule.mode = "DT_LIB_COLLECT_MODE_AND"; rule.item = "DT_COLLECTION_PROP_FILMROLL"; rule.data = img.path
  dt.gui.libs.collect.filter({ rule })
  dt.gui.selection({ img })
  dt.control.sleep(1000)
  loaded = nil
  dt.gui.current_view(dt.gui.views.darkroom)
  for _ = 1, 240 do
    if loaded == path then dt.control.sleep(3000); return "" end
    if loaded then
      error("darktable opened " .. loaded .. " instead of " .. path ..
        " — keep the mouse pointer outside darktable's window during the self-test")
    end
    dt.control.sleep(500)
  end
  error("darkroom didn't load " .. path .. " within 120 s (view: " .. dt.gui.current_view().id .. ")")
end

function commands.lighttable() lighttable(); return "" end

function commands.action(path, element, effect, size)
  return tostring(dt.gui.action(path, element, effect, tonumber(size)))
end

local function split(s)
  local out = {}
  for field in (s .. "\t"):gmatch("([^\t]*)\t") do out[#out + 1] = field end
  return out
end

dt.control.dispatch(function()
  dt.control.sleep(2000)
  write(ipc .. "/ready", dt.configuration.version)
  local n = 1
  while true do
    local cmd = read(ipc .. "/cmd-" .. n)
    if cmd then
      local args = split(cmd)
      local name = table.remove(args, 1)
      local f = commands[name]
      local ok, result = pcall(function()
        if not f then error("unknown command " .. name) end
        return f(table.unpack(args))
      end)
      write(ipc .. "/reply-" .. n, (ok and "ok\t" or "error\t") .. tostring(result))
      n = n + 1
    else
      dt.control.sleep(200)
    end
  end
end)
