-- Counts lines in the selected files (or the file under the cursor).
function count(ctx)
  local files = ctx.selection
  if #files == 0 then files = { ctx.cursor } end
  local total = 0
  for _, path in ipairs(files) do
    if not cfm.is_dir(path) then
      local text = cfm.read(path, 8 * 1024 * 1024)
      if text then
        for _ in text:gmatch("\n") do total = total + 1 end
      end
    end
  end
  cfm.notify(total .. " lines in " .. #files .. " file(s)")
end
