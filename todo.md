# TODO

## 1. Blackbeard: still open after the review (2026-10-26)
- [ ] `Style.qml` logs `TypeError: Cannot read property 'toLowerCase' of undefined` (line 147, and `lastIndexOf` at 185) when files are created and deleted quickly in a folder shown in icons, tree or Miller view or sorted by size (a downloads or build folder): a row's name is read after the row is gone. Not seen in list view. It does no visible harm, but guard `nerdGlyph`, `glyphColor` and the other helpers that take a name (`name ?? ""`), and repeat the churn until the log is clean. To reproduce: one process making and deleting 100 files over and over in a 300-file folder, `CFM_KEYS="@view.grid <F9> <F9> <F9>"` headless, watch stderr.
- [ ] A refused or failed paste (copy a folder into itself) still shows "Transfers · finished: Done" with a full bar in the transfer panel, next to the error toast. A failed job should read "Failed" (and the reason).
- [ ] AppImage is still untried: `sudo pacman -S --needed jxrlib` (not installed yet), then follow `docs/RELEASING.md` section 3.
- [ ] Not covered, do by hand: mouse wheel scrolling (the test pointer has no wheel), a real remote or cloud drive, the Vim key style on a live window, other themes and fractional scales, copy and Shift-move drags in the final build, and an install of the final package in a clean container.

## 2. Features few people use: decide with the numbers
Counting began on 2026-10-24 (section 10), so the earliest sensible review is about 2026-11-21.
- [ ] After 4 weeks of counts, read "Show feature use…" and decide per feature: switch off by default, or compile out behind a cargo feature if it also costs build time. All plugins are on by default for now, so nothing seems to disappear.
- [ ] Likely under 5%: Lua plugins themselves (vendored Lua adds build time and size), batch rename templates and regex, the learned palette history. Not changed without numbers.
- Keep, even though not everyone uses them: the folder-handler D-Bus service and portal file chooser (they make it the default file manager), and trash and undo. Font previews stay built in because they need the font drawn, not just text.

## 3. Draggable tabs
- [x] Drop a file onto a tab to put it in that tab's folder (17.7).
- [ ] Drag a tab out to make a split pane.
