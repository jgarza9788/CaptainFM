-- Plain text of RTF files, the first page of DjVu scans, and the cover of e-books and comics.

local cp1252 = {
  [0x80] = "€", [0x91] = "‘", [0x92] = "’", [0x93] = "“", [0x94] = "”",
  [0x95] = "•", [0x96] = "–", [0x97] = "—", [0x99] = "™",
}

-- Text of an RTF document: groups that are not text (fonts, colours, pictures) are dropped,
-- paragraphs and tabs kept, \'hh and \uN escapes decoded.
function rtf_text(rtf)
  local out = {}
  local skip = { false }
  local function skipping() return skip[#skip] end
  local i, n = 1, #rtf
  while i <= n do
    local c = rtf:sub(i, i)
    if c == "{" then
      skip[#skip + 1] = skip[#skip]
      i = i + 1
    elseif c == "}" then
      if #skip > 1 then skip[#skip] = nil end
      i = i + 1
    elseif c == "\\" then
      i = i + 1
      if i > n then break end
      local nx = rtf:sub(i, i)
      if not nx:match("%a") then
        i = i + 1
        if (nx == "\\" or nx == "{" or nx == "}") and not skipping() then
          out[#out + 1] = nx
        elseif nx == "~" and not skipping() then
          out[#out + 1] = " "
        elseif nx == "*" then
          skip[#skip] = true
        elseif nx == "'" then
          local b = tonumber(rtf:sub(i, i + 1), 16)
          i = i + 2
          if b and not skipping() then out[#out + 1] = cp1252[b] or utf8.char(b) end
        end
      else
        local word = rtf:match("^%a+", i)
        i = i + #word
        local num = rtf:match("^%-?%d+", i)
        if num then i = i + #num end
        if rtf:sub(i, i) == " " then i = i + 1 end
        if word == "fonttbl" or word == "colortbl" or word == "stylesheet" or word == "info" or word == "pict"
          or word == "header" or word == "footer" or word == "object" or word == "themedata" then
          skip[#skip] = true
        elseif (word == "par" or word == "line" or word == "sect" or word == "page") and not skipping() then
          out[#out + 1] = "\n"
        elseif word == "tab" and not skipping() then
          out[#out + 1] = "\t"
        elseif word == "u" and not skipping() then
          local code = tonumber(num) or 0
          if code < 0 then code = code + 65536 end
          if code > 0 and code < 0x110000 and not (code >= 0xD800 and code < 0xE000) then out[#out + 1] = utf8.char(code) end
          -- The ANSI fallback that follows the escape is not text.
          i = i + 1
        end
      end
    elseif c == "\r" or c == "\n" then
      i = i + 1
    else
      if not skipping() then out[#out + 1] = c end
      i = i + 1
    end
  end
  local lines = {}
  for line in (table.concat(out) .. "\n"):gmatch("([^\n]*)\n") do lines[#lines + 1] = (line:gsub("%s+$", "")) end
  return (table.concat(lines, "\n"):gsub("^%s+", ""):gsub("%s+$", ""))
end

function rtf(ctx)
  local text = cfm.read_bytes(ctx.path, 2 * 1024 * 1024)
  if not text or not text:match("^%s*{\\rtf") then return nil end
  local plain = rtf_text(text)
  if plain == "" then return nil end
  return { text = plain, note = "extracted text" }
end

function djvu(ctx)
  local key = ctx.path .. ":" .. ctx.mtime .. ":djvu"
  local png = cfm.cache_file(key, "png")
  if not cfm.exists(png) then
    local ppm = cfm.cache_file(key, "ppm")
    local a = cfm.run("ddjvu", { "-format=ppm", "-page=1", "-size=1400x1400", ctx.path, ppm })
    local b = a.ok and cfm.run("magick", { ppm, png })
    cfm.run("rm", { "-f", ppm })
    if not (b and b.ok) then cfm.run("rm", { "-f", png }); return nil end
  end
  return { image = png, note = "DjVu · showing page 1" }
end

-- The cover image: a file called "cover" first, else the first image by name.
function pick_cover(members)
  local images = {}
  for _, m in ipairs(members) do
    local e = m:match("%.([^./]+)$")
    if e then
      e = e:lower()
      if e == "png" or e == "jpg" or e == "jpeg" or e == "gif" or e == "webp" then images[#images + 1] = m end
    end
  end
  table.sort(images)
  for _, m in ipairs(images) do
    if m:lower():find("cover", 1, true) then return m end
  end
  return images[1]
end

function cover(ctx)
  local list = cfm.run("bsdtar", { "-tf", ctx.path })
  if not list.ok then return nil end
  local members = {}
  for line in list.stdout:gmatch("[^\n]+") do members[#members + 1] = line end
  local member = pick_cover(members)
  if not member then return nil end
  local out = cfm.cache_file(ctx.path .. ":" .. ctx.mtime .. ":cover", (member:match("%.([^.]+)$") or "png"):lower())
  if not cfm.exists(out) then
    local r = cfm.run("sh", { "-c", 'bsdtar -xOf "$1" "$2" > "$3"', "sh", ctx.path, member, out })
    if not r.ok then cfm.run("rm", { "-f", out }); return nil end
  end
  return { image = out, note = ctx.ext == "epub" and "e-book cover" or "first page" }
end
