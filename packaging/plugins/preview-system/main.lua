-- What a program, certificate, SSH public key or software package is, from the system's own tools.

-- ELF programs and libraries (readelf and file).
function program(ctx)
  local h = cfm.run("readelf", { "-h", ctx.path })
  if not h.ok then return nil end
  local out = {}
  local wanted = { "Class:", "Data:", "OS/ABI:", "Type:", "Machine:", "Entry point address:" }
  for _, key in ipairs(wanted) do
    for line in h.stdout:gmatch("[^\n]+") do
      local k, v = line:match("^%s*([^:]+:)%s*(.*)$")
      if k == key then
        out[#out + 1] = string.format("%s: %s", k:sub(1, -2):lower(), v)
        break
      end
    end
  end
  local d = cfm.run("readelf", { "-d", ctx.path })
  local needed = {}
  if d.ok then
    for line in d.stdout:gmatch("[^\n]+") do
      if line:find("(NEEDED)", 1, true) then
        local lib = line:match("%[(.-)%]")
        if lib then needed[#needed + 1] = lib end
      end
    end
  end
  if #needed > 0 then
    out[#out + 1] = "needs:"
    for i = 1, math.min(#needed, 40) do out[#out + 1] = "  " .. needed[i] end
  end
  local p = cfm.run("readelf", { "-p", ".interp", ctx.path })
  if p.ok then
    for line in p.stdout:gmatch("[^\n]+") do
      local interp = line:match("%]%s*(/.-)%s*$")
      if interp then out[#out + 1] = "interpreter: " .. interp; break end
    end
  end
  if #out == 0 then return nil end
  local text = table.concat(out, "\n") .. "\n"
  local desc = cfm.run("file", { "-b", ctx.path })
  if desc.ok and desc.stdout:match("%S") then text = desc.stdout:match("^[^\n]*") .. "\n\n" .. text end
  return { text = text, note = "executable" }
end

-- X.509 certificates (openssl).
function certificate(ctx)
  local r = cfm.run("openssl", { "x509", "-in", ctx.path, "-noout", "-subject", "-issuer", "-dates", "-fingerprint", "-sha256" })
  if not r.ok then return nil end
  return { text = (r.stdout:gsub("=", " = ")), note = "certificate" }
end

-- SSH public keys: type, size and fingerprint (ssh-keygen).
function ssh_key(ctx)
  local head = cfm.read(ctx.path, 8192)
  if not head then return nil end
  local ok = false
  for _, prefix in ipairs({ "ssh-", "ecdsa-", "sk-" }) do
    if head:sub(1, #prefix) == prefix then ok = true end
  end
  if not ok then return nil end
  local r = cfm.run("ssh-keygen", { "-l", "-f", ctx.path })
  local text = r.ok and r.stdout:match("^%s*(.-)%s*$") or ""
  if text == "" then return nil end
  return { text = text, note = "SSH public key" }
end

-- Package metadata, from pacman, dpkg or rpm if installed.
function package(ctx)
  local name = ctx.name:lower()
  local r
  if name:find(".pkg.tar", 1, true) then
    r = cfm.run("pacman", { "-Qip", ctx.path })
  elseif name:sub(-4) == ".deb" then
    r = cfm.run("dpkg-deb", { "-I", ctx.path })
  elseif name:sub(-4) == ".rpm" then
    r = cfm.run("rpm", { "-qip", ctx.path })
  end
  local text = r and r.ok and r.stdout:match("^%s*(.-)%s*$") or ""
  if text == "" then return nil end
  return { text = text, note = "package" }
end
