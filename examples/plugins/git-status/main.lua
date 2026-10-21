-- Actions for the Git example plugin. `cfm` is the file manager API; `ctx` describes the
-- current state: ctx.path (folder), ctx.cursor (item under the cursor), ctx.selection (list).

function status(ctx)
  local branch = cfm.run("git", { "rev-parse", "--abbrev-ref", "HEAD" }, ctx.path)
  if not branch.ok then
    return { repo = false }
  end
  local short = cfm.run("git", { "status", "--short" }, ctx.path)
  local files = {}
  for line in short.stdout:gmatch("[^\n]+") do
    files[#files + 1] = { code = line:sub(1, 2), name = line:sub(4) }
  end
  return { repo = true, branch = branch.stdout:gsub("%s+$", ""), files = files }
end

function copy_branch(ctx)
  local r = cfm.run("git", { "rev-parse", "--abbrev-ref", "HEAD" }, ctx.path)
  if r.ok then
    cfm.copy_text((r.stdout:gsub("%s+$", "")))
    cfm.notify("Copied branch " .. r.stdout)
  else
    cfm.notify("Not a git repository", "warn")
  end
end
