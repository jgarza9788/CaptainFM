# TODO

## 1. Blackbeard: still open (2026-10-26)
- [ ] The AppImage only starts on a glibc as new as this machine's (2.44), so it is for rolling distros. To reach older distros, build it inside an older distro's container (Debian 12 or Ubuntu 22.04) with a Qt 6 that the app builds against; first check which Qt 6 version the QML and `cxx-qt` need. Also call `scripts/appimage.sh` from `scripts/release.sh`.
- [ ] Rebuild and install (`./build.sh`) so `~/.local/bin/captainfm` has these fixes (the installed one is the 19:24 build, with the dead Vim keys).
- [ ] Not covered, do by hand: mouse wheel scrolling (the test pointer has no wheel), a real remote or cloud drive, fractional scales other than this machine's 1.5, copy and Shift-move drags in the final build, and an install of the final package in a clean container (the release from `master`).

## 2. Features few people use: decide with the numbers
Counting began on 2026-10-24 (section 10), so the earliest sensible review is about 2026-11-21.
- [ ] After 4 weeks of counts, read "Show feature use…" and decide per feature: switch off by default, or compile out behind a cargo feature if it also costs build time. All plugins are on by default for now, so nothing seems to disappear.
- [ ] Likely under 5%: Lua plugins themselves (vendored Lua adds build time and size), batch rename templates and regex, the learned palette history. Not changed without numbers.
- Keep, even though not everyone uses them: the folder-handler D-Bus service and portal file chooser (they make it the default file manager), and trash and undo. Font previews stay built in because they need the font drawn, not just text.

## 3. Draggable tabs
- [x] Drop a file onto a tab to put it in that tab's folder (17.7).
- [ ] Drag a tab out to make a split pane.
