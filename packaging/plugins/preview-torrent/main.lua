-- Name, size, pieces, trackers and files of a .torrent file (bencoded metainfo).

-- Parse one bencoded value at position i: integers are numbers, strings are strings,
-- lists are { t = "l", v = {...} } and dictionaries { t = "d", v = { key = value } }.
local function parse(s, i, depth)
  if depth > 16 then return nil end
  local c = s:sub(i, i)
  if c == "i" then
    local e = s:find("e", i, true)
    if not e then return nil end
    local n = tonumber(s:sub(i + 1, e - 1))
    if not n then return nil end
    return n, e + 1
  elseif c == "l" or c == "d" then
    i = i + 1
    local items, map = {}, {}
    while s:sub(i, i) ~= "e" do
      if i > #s then return nil end
      local v
      if c == "d" then
        local key
        key, i = parse(s, i, depth + 1)
        if type(key) ~= "string" then return nil end
        v, i = parse(s, i, depth + 1)
        if v == nil then return nil end
        map[key] = v
      else
        v, i = parse(s, i, depth + 1)
        if v == nil then return nil end
        items[#items + 1] = v
      end
    end
    return { t = c, v = (c == "d") and map or items }, i + 1
  elseif c:match("%d") then
    local colon = s:find(":", i, true)
    if not colon then return nil end
    local len = tonumber(s:sub(i, colon - 1))
    if not len or colon + len > #s then return nil end
    return s:sub(colon + 1, colon + len), colon + len + 1
  end
  return nil
end

local function get(d, key)
  if type(d) == "table" and d.t == "d" then return d.v[key] end
end

local function short_size(n)
  local v, unit = math.max(n, 0) + 0.0, 1
  while v >= 1024 and unit < 5 do v = v / 1024; unit = unit + 1 end
  if unit == 1 then return string.format("%d B", n) end
  return string.format("%.1f %s", v, ({ "B", "KB", "MB", "GB", "TB" })[unit])
end

-- The summary text for torrent file contents, or nil when it is not a torrent.
function summary(bytes)
  local root = parse(bytes, 1, 0)
  local info = get(root, "info")
  if not info then return nil end
  local name = get(info, "name")
  name = type(name) == "string" and name or ""
  local files = {}
  local list = get(info, "files")
  if type(list) == "table" and list.t == "l" then
    for _, f in ipairs(list.v) do
      local parts = get(f, "path")
      if type(parts) == "table" and parts.t == "l" then
        local names = {}
        for _, p in ipairs(parts.v) do
          if type(p) == "string" then names[#names + 1] = p end
        end
        local len = get(f, "length")
        files[#files + 1] = { table.concat(names, "/"), type(len) == "number" and len or 0 }
      end
    end
  elseif type(get(info, "length")) == "number" then
    files[1] = { name, get(info, "length") }
  end
  local total = 0
  for _, f in ipairs(files) do total = total + f[2] end
  local out = {}
  out[#out + 1] = string.format("name: %s", name)
  out[#out + 1] = string.format("size: %s (%d %s)", short_size(total), #files, #files == 1 and "file" or "files")
  local plen = get(info, "piece length")
  if type(plen) == "number" then
    local pieces = get(info, "pieces")
    out[#out + 1] = string.format("pieces: %d × %s", type(pieces) == "string" and #pieces // 20 or 0, short_size(plen))
  end
  if get(info, "private") == 1 then out[#out + 1] = "private: yes" end
  for _, key in ipairs({ "comment", "created by" }) do
    local t = get(root, key)
    if type(t) == "string" and t ~= "" then out[#out + 1] = key .. ": " .. t end
  end
  local trackers = {}
  local seen = {}
  local function add(u)
    if type(u) == "string" and not seen[u] then seen[u] = true; trackers[#trackers + 1] = u end
  end
  add(get(root, "announce"))
  local tiers = get(root, "announce-list")
  if type(tiers) == "table" and tiers.t == "l" then
    for _, tier in ipairs(tiers.v) do
      if type(tier) == "table" and tier.t == "l" then
        for _, u in ipairs(tier.v) do add(u) end
      end
    end
  end
  if #trackers > 0 then
    out[#out + 1] = "trackers:"
    for i = 1, math.min(#trackers, 10) do out[#out + 1] = "  " .. trackers[i] end
  end
  if #files > 1 then
    out[#out + 1] = "files:"
    for i = 1, math.min(#files, 60) do
      out[#out + 1] = string.format("  %s  (%s)", files[i][1], short_size(files[i][2]))
    end
    if #files > 60 then out[#out + 1] = string.format("  … and %d more", #files - 60) end
  end
  return table.concat(out, "\n") .. "\n"
end

function preview(ctx)
  if ctx.size > 16 * 1024 * 1024 then return nil end
  local bytes = cfm.read_bytes(ctx.path, ctx.size)
  local text = bytes and summary(bytes)
  if text then return { text = text, note = "torrent" } end
end
