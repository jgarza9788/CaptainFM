-- Events and to-dos of an .ics file, and the people in a .vcf file.

-- Lines with folded continuations joined and escapes decoded: { NAME, params, value }.
local function content_lines(text)
  local joined = {}
  for line in text:gmatch("[^\r\n]+") do
    local first = line:sub(1, 1)
    if (first == " " or first == "\t") and #joined > 0 then
      joined[#joined] = joined[#joined] .. line:sub(2)
    else
      joined[#joined + 1] = line
    end
  end
  local out = {}
  for _, l in ipairs(joined) do
    local head, value = l:match("^([^:]*):(.*)$")
    if head then
      local name, params = head:match("^([^;]*);?(.*)$")
      value = value:gsub("\\[nN]", "\n"):gsub("\\,", ","):gsub("\\;", ";"):gsub("\\\\", "\\")
      out[#out + 1] = { name:upper(), params, value }
    end
  end
  return out
end

-- 20261023T140000Z as "2026-10-23 14:00 UTC"; a plain date as "2026-10-23".
local function calendar_time(v)
  v = v:match("^%s*(.-)%s*$")
  local digits = v:gsub("%D", "")
  if #digits < 8 then return v end
  local date = digits:sub(1, 4) .. "-" .. digits:sub(5, 6) .. "-" .. digits:sub(7, 8)
  if #digits >= 12 and v:find("T", 1, true) then
    return date .. " " .. digits:sub(9, 10) .. ":" .. digits:sub(11, 12) .. (v:sub(-1) == "Z" and " UTC" or "")
  end
  return date
end

function summarize_calendar(text)
  local out, entries, current = {}, 0, nil
  for _, l in ipairs(content_lines(text)) do
    local name, value = l[1], l[3]
    if name == "BEGIN" and (value == "VEVENT" or value == "VTODO") then
      current = { kind = value, props = {} }
    elseif name == "END" and (value == "VEVENT" or value == "VTODO") and current then
      entries = entries + 1
      if entries <= 40 then
        local props = current.props
        local function get(k) if props[k] and props[k] ~= "" then return props[k] end end
        out[#out + 1] = (current.kind == "VTODO" and "☐ " or "• ") .. (get("SUMMARY") or "(no title)")
        local start = get("DTSTART") or get("DUE")
        start = start and calendar_time(start)
        local stop = get("DTEND")
        stop = stop and calendar_time(stop)
        if start and stop and start ~= stop then
          out[#out + 1] = "    when: " .. start .. " → " .. stop
        elseif start then
          out[#out + 1] = "    when: " .. start
        end
        if get("LOCATION") then out[#out + 1] = "    where: " .. get("LOCATION") end
        if get("RRULE") then out[#out + 1] = "    repeats: " .. get("RRULE") end
        if get("DESCRIPTION") then
          local first = get("DESCRIPTION"):match("^[^\n]*")
          out[#out + 1] = "    " .. first:sub(1, 120)
        end
      end
      current = nil
    elseif current and not current.props[name] then
      current.props[name] = value
    end
  end
  if entries == 0 then return nil end
  if entries > 40 then out[#out + 1] = string.format("… and %d more", entries - 40) end
  return string.format("%d %s\n\n%s\n", entries, entries == 1 and "entry" or "entries", table.concat(out, "\n"))
end

function summarize_contacts(text)
  local out, cards, open = {}, 0, false
  for _, l in ipairs(content_lines(text)) do
    local name, params, value = l[1], l[2], l[3]
    if name == "BEGIN" and value:upper() == "VCARD" then
      open = true
      cards = cards + 1
      if cards <= 40 then out[#out + 1] = "" end
    elseif name == "END" and value:upper() == "VCARD" then
      open = false
    elseif open and cards <= 40 and value:match("%S") then
      if name == "FN" then
        out[#out + 1] = value
      elseif name == "ORG" then
        local org = value:gsub(";", ", "):gsub("[, ]+$", "")
        out[#out + 1] = "    " .. org
      elseif name == "TEL" or name == "EMAIL" then
        local kind = ""
        for word in params:lower():gsub("type=", ""):gsub("[;,]", " "):gmatch("%S+") do
          if word ~= "pref" and word ~= "internet" and word ~= "voice" then kind = word; break end
        end
        out[#out + 1] = string.format("    %s%s: %s", name == "TEL" and "phone" or "email", kind ~= "" and (" (" .. kind .. ")") or "", value)
      elseif name == "BDAY" then
        out[#out + 1] = "    birthday: " .. value
      elseif name == "ADR" then
        local parts = {}
        for p in value:gmatch("[^;]+") do parts[#parts + 1] = p end
        if #parts > 0 then out[#out + 1] = "    address: " .. table.concat(parts, ", ") end
      end
    end
  end
  if cards == 0 then return nil end
  if cards > 40 then out[#out + 1] = ""; out[#out + 1] = string.format("… and %d more", cards - 40) end
  return string.format("%d %s\n%s\n", cards, cards == 1 and "contact" or "contacts", table.concat(out, "\n"))
end

function calendar(ctx)
  local text = cfm.read(ctx.path, 512 * 1024)
  local s = text and summarize_calendar(text)
  if s then return { text = s, note = "calendar" } end
end

function contacts(ctx)
  local text = cfm.read(ctx.path, 512 * 1024)
  local s = text and summarize_contacts(text)
  if s then return { text = s, note = "contacts" } end
end
