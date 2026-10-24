# TODO

## 1. Connections and palette loose ends (from the code review, highest first)
- [x] Two saved cloud drives with the same name share one mount folder, so one can hide or unmount the other. Make the folder unique per connection (add the id to the slug) and refuse a duplicate name in the edit form. (`remotes::mount_point`)
- [x] A gvfs mount that shows under a different host spelling (name versus IP) is listed twice in the sidebar, and the second entry has no folder, so clicking it opens the connect picker. Match the two by share and user, or hide the folderless one when a browsable twin exists. (`devices::list`)
- [x] Renaming a cloud drive while it is mounted leaves it in the sidebar under the old name until it is unmounted and reconnected. Unmount first, or move the mount point on rename. (`remotes::update`)
- [x] Two running copies can overwrite each other's learned palette picks. Write `palette_usage.json` to a temp file and rename it, and merge with what is on disk before saving. (`state::Usage::save`)
- [x] The palette stores what you typed, in plain text. Add a setting to turn learning off and a "Forget palette history" command. (`state::Usage`)

## 2. Unpin friction
- [x] Unpinning takes two presses in a row, so a stray key press can't drop a pin: Ctrl+D (or Ctrl+B) twice, or right-click the pinned sidebar entry twice. The first press turns the entry red ("unpin name?") and shows a toast; the second, within 3 seconds, removes it.

## 3. Finish pro dragging
- [x] Refresh the drop label when a modifier key is pressed or released, without moving the pointer (Ctrl copy, Shift move, Alt link). Qt's own key cache goes stale during a Wayland drag, so the held keys are asked of the window system (`queryKeyboardModifiers`, `src/cpp/modifiers.cpp`). Checked in a live drag: the label went Copy, then Move while Shift was held with the pointer still, then Copy again on release.
- [x] Let Shift force a move across drives. A plain move and a Shift move look the same to the app today, so read the held keys directly. Done and checked in a live drag across /tmp and /home: plain copies, Shift moves, Ctrl copies.
- [x] Drop targets in grid view: drop onto a folder, and hover a drag over one to open it. Grid and column cells can now also start a drag.

## 4. Transfer queue with progress
- [x] A panel listing each copy or move, with speed and time left. It opens by itself, `Space x x` shows or hides it (`src/transfer.rs`, `qml/TransferPanel.qml`).
- [x] Cancel, and pause. Both act between 8 MB chunks; a cancelled copy removes its half-written file. `Space x p` pauses, `Space x c` cancels, or click the panel.
- [x] Name conflicts: ask to replace, skip or keep both (one answer for the whole paste). Replace sends the old item to the trash, so undo brings it back. (`qml/ConflictDialog.qml`)

## 5. more previews
- [x] Built-in preview for more file types: calendars (.ics), contacts (.vcf), packages (.pkg.tar.*, .deb, .rpm, through the package manager's own tool) and SSH public keys (fingerprint).

## 6. Faster deep search index
Measured on this machine (`scripts/bench.py`): `/home` is 322,755 entries. The index now builds in 0.22 s (was 1.94 s) and the whole app peaks at 159 MB (was 199 MB). Searching it takes about 6 ms.
- [x] Stop reading size and modified time for every entry while building. They are read for the 500 results shown, and for the entries that match the name when the query uses `size>` or `newer:` (`index::search`).
- [x] Walk folders in parallel, on our own small thread pool (up to 8 threads), because `jwalk` is unmaintained (`index::scan`).
- [x] Use less memory: names share one text buffer and each folder is stored once, 12 bytes per entry plus the name (was about 133 bytes of path text per entry).
- [x] Do not rebuild the whole index every 2 minutes. After 30 s it only checks the folders' modified times, and rebuilds in the background, still serving the old index, only if something changed.
- Equal search scores now come out in path order, since the parallel walk order differs from run to run.

## 7. Faster code previews
Not an editor, so the minimum: a preview is a glance.
- [x] Preview at most 200 lines or 24 KB of text (was 600 lines or 96 KB). A source file preview went from 137 to 198 ms to about 35 to 45 ms.
- [x] Load the syntax set in the background at start, so the first text preview does not wait for it. The first file of each language still pays about 80 ms to compile its patterns.
- Skipped on purpose: highlighting as you scroll, a preview cache, and the `regex-onig` backend. They add code for little gain at this size.

## 8. Idle wake-ups and polling
Idle went from about 2.7 wake-ups a second to 0.15 (3 in 20 s, measured on an open window with nothing happening).
- [x] The transfer panel polls only while a transfer is active or just finished; a `transferStarted` signal starts it (`qml/TransferPanel.qml`).
- [x] The status bar index count updates only while a deep search is open, and once when it closes (`Main.qml`).
- [x] The sidebar no longer polls every 30 s. The kernel wakes us when `/proc/self/mountinfo` changes, the gvfs folder is watched, and coming back to the window refreshes the free-space bars. Only one device listing runs at a time, so a hung network mount cannot pile up threads (`devices::watch_changes`).

## 9. Build and startup
Startup with an empty folder (headless, `CFM_DEBUG=1` prints it): Qt itself is up at 87 ms, the interface is built at about 300 ms, the first frame at about 345 ms (was about 375 ms).
- [x] `dist` profile (`lto = "fat"`, `codegen-units = 1`, `panic = "abort"`), which `build.sh` now uses: 19% smaller (12.7 to 10.2 MB) and the index builds a little faster (about 240 ms against 290 ms); startup is the same. A build takes 6 minutes instead of 1, so `cargo build --release` stays the development build, and `CFM_PROFILE=dist scripts/bench.py` measures it.
- [x] Settings, Connect and Batch rename are built the first time they open, not at startup (about 30 ms).
- [x] `CFM_DEBUG=1` prints "Qt application created", "interface built", "QML loaded" and "first frame" times.
- [x] `cargo bloat`: code is 7.1 MB; the biggest parts are `std` 1.0 MB, `zbus` 1.0 MB, `cxx_qt_lib` 0.7 MB and the regex crates 0.9 MB. Nothing cheap to remove for speed; `zbus` is the portal and folder-handler service.
- [ ] Building the interface still takes about 230 ms. The QML is already compiled ahead of time, so the next step is the QML Profiler to see which components cost most, then `pragma ComponentBehavior: Bound` and `required property` in the delegates it points at ([Qt docs](https://doc.qt.io/qt-6/qtquick-bestpractices.html), [KDAB](https://www.kdab.com/10-tips-to-make-your-qml-code-faster-and-more-maintainable/)). Not done blind: it touches every delegate and the gain is unproven.
- Already fast, leave alone: listing 100,000 files takes 66 to 77 ms, rows are reused (`reuseItems`, `cacheBuffer`), and copies use `copy_file_range`.

## 10. Features few people use
There is no usage data to go on: a web search found no real survey of file manager feature use, only feature lists. So the list below is a judgment about the average user, and the app now counts use on this computer to check it.
- [x] Count feature use on this computer only: command ids, preview kinds and plugins, by day (`feature_usage.json`; ids and day numbers, never typed text or paths). Settings > "Count feature use" turns it off, "Show feature use…" in the palette lists what was used on how many of the last 28 days (least used first), and "Forget feature use" clears it (`state::FeatureUse`).
- [x] Plugins can provide previews (`"previews"` in `plugin.json`, with `cfm.read_bytes` and `cfm.cache_file`), and plugins can ship inside the app (`packaging/plugins/`) and be switched off in Settings > Plugins.
- [x] The rarely used previews are now five built-in plugins, and their Rust code is gone: `preview-torrent`, `preview-calendar-contacts`, `preview-sqlite`, `preview-system` (programs, certificates, SSH keys, packages) and `preview-documents` (RTF, DjVu, e-book and comic covers). Their tests run the Lua.
- [x] The Connect dialog shows SFTP, SMB, FTP, WebDAV, NFS and Google Drive; Dropbox, OneDrive, Box, Google Cloud Storage and S3 come after "More cloud services…" (still reachable from the palette).
- [x] Plugins can be switched with build flags (`./build.sh --no-plugins`, `--without torrent,sqlite`, `--only sqlite,system`, or cargo features `plugin-torrent` and so on), and at run time from the palette ("Plugins: turn off …", "turn all on/off"), from the command line (`captainfm --plugins`, `captainfm --plugin torrent off`) and in Settings.

## 11. right click (double) eject remove drives
- [ ] a right click (twice) should eject remove drives

## 12. remote drive capacity (plugin)
- [ ] show an indicatator showing the capacity of remove drives 

## 13. performance test 
- [ ] create a performance test
- [ ] standarddrize the performance test
- [ ] make a small report for each performance test

## 14. Features few people user
- [ ] After 4 weeks of counts, read "Show feature use…" and decide per feature: switch off by default, or compile out behind a cargo feature if it also costs build time. All plugins are on by default for now, so nothing seems to disappear.
- [ ] Likely under 5%: Lua plugins themselves (vendored Lua adds build time and size), batch rename templates and regex, the learned palette history. Not changed without numbers.
- Keep, even though not everyone uses them: the folder-handler D-Bus service and portal file chooser (they make it the default file manager), and trash and undo. Font previews stay built in because they need the font drawn, not just text.

## 100. Draggable tabs
- [ ] Drop a file onto a tab to put it in that tab's folder.
- [ ] Drag a tab out to make a split pane.
