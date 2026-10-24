-- Tables with their row counts, then the schema, through the sqlite3 program.

function preview(ctx)
  local tables = cfm.run("sqlite3", { "-readonly", ctx.path, ".tables" })
  if not tables.ok then return nil end
  local out, n = {}, 0
  for name in tables.stdout:gmatch("%S+") do
    n = n + 1
    if n > 40 then break end
    local q = string.format('select count(*) from "%s"', name:gsub('"', ""))
    local count = cfm.run("sqlite3", { "-readonly", ctx.path, q })
    out[#out + 1] = string.format("-- %s: %s rows", name, count.ok and count.stdout:match("^%s*(.-)%s*$") or "?")
  end
  local schema = cfm.run("sqlite3", { "-readonly", ctx.path, ".schema" })
  local text = table.concat(out, "\n")
  if #out > 0 then text = text .. "\n" end
  if schema.ok then text = text .. "\n" .. schema.stdout:sub(1, 6000) end
  if text == "" then return nil end
  return { text = text, note = "SQLite database" }
end
