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
- [x] A panel listing each copy or move, with speed and time left. It opens by itself, `, x x` shows or hides it (`src/transfer.rs`, `qml/TransferPanel.qml`).
- [x] Cancel, and pause. Both act between 8 MB chunks; a cancelled copy removes its half-written file. `, x p` pauses, `, x c` cancels, or click the panel.
- [x] Name conflicts: ask to replace, skip or keep both (one answer for the whole paste). Replace sends the old item to the trash, so undo brings it back. (`qml/ConflictDialog.qml`)

## 5. more previews
- [x] Built-in preview for more file types: calendars (.ics), contacts (.vcf), packages (.pkg.tar.*, .deb, .rpm, through the package manager's own tool) and SSH public keys (fingerprint).

## 6. Faster deep search index
Measured on this machine (`scripts/bench.py` (now `scripts/perf.py`)): `/home` is 322,755 entries. The index now builds in 0.22 s (was 1.94 s) and the whole app peaks at 159 MB (was 199 MB). Searching it takes about 6 ms.
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
- [x] `dist` profile (`lto = "fat"`, `codegen-units = 1`, `panic = "abort"`), which `build.sh` now uses: 19% smaller (12.7 to 10.2 MB) and the index builds a little faster (about 240 ms against 290 ms); startup is the same. A build takes 6 minutes instead of 1, so `cargo build --release` stays the development build, and `scripts/perf.py` measures it.
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

## 11. Double right-click: eject, disconnect, close, delete
- [x] Right-clicking twice within 3 seconds does the hard-to-undo thing under the pointer, and says so in a toast both times: unpin a pinned entry, unmount a drive (the system drive is refused), disconnect a remote or cloud drive, close a tab (never the last one), and move a file, folder or selection to the trash (`Ctrl-Z` undoes). The first press marks the target in red (sidebar rows and the tab) (`Main.armTwice`, `rowRightClicked`, `deviceRightClicked`, `tabRightClicked`).
- [x] On files and folders the first right-click still opens the context menu; the second, on the same item or on the menu, deletes (`Picker.rightClicked`).
- Deliberately left out: Miller columns, the breadcrumb bar and the places list (documents, downloads...) do nothing on a double right-click, since there is nothing there to remove.
- [x] Checked against real mounts through the double right-click (two `!rclick:side:<name>` presses in a headless run): a real gvfs mount (`gio mount localtest:///`) was disconnected and gone from `gio mount -l`, and a real udisks mount (a 24 MB ext4 image through `udisksctl loop-setup`, mounted under `/run/media`) was unmounted. Your three SMB shares were left alone (their passwords are not saved, so they could not be re-mounted).
- [x] A physical USB stick (a 116 GB Ventoy stick) was unmounted and powered off by the second right-click: it left `lsblk` and `udisksctl status` at once, and is safe to unplug (`devices::eject_disk`).

## 12. Remote drive capacity
Core, not a plugin: the sidebar already drew a usage bar, percentage and "N free" for any entry with numbers, and plugins cannot reach the sidebar. Remote and cloud entries just had none.
- [x] Looked up only when wanted, so people with many remotes are not slowed down: when the pointer rests on a remote's row (about 300 ms) or it is opened. Nothing is fetched while the sidebar lists (checked: no `gio` process while the sidebar is up; only the hovered or opened row gets a bar).
- [x] One remote at a time on its own thread with a 4 s limit: `gio info -f <mount>` for gvfs mounts (`filesystem::size` and `filesystem::free`), `statvfs` for rclone mounts. No numbers means no bar. Checked live on a real SMB share (49% used).
- [x] Each answer, including "nothing reported", is cached for 60 s and never asked twice at once; the `spaceReady` signal fills `used`, `total` and `usage` for that row, so the existing bar and colours show (`devices::remote_space`, `SpaceCache`, `Backend::request_space`, `Sidebar.askSpace`).
- [x] Tests: `gio info -f` parsing (size and free, missing keys, zero size, free above size), the 60 s cache, and giving up on a slow program.
- [x] rclone stays a separate package, not bundled. When it is missing the Connect dialog says so and points to https://rclone.org/install/ (also in README and HOWTO). Cloud capacity (`statvfs` on the mount) is the same code path as local drives.
- [x] A pulsing dot and "Loading…" show in an empty folder that is still being read, but only after 300 ms so local folders never flash it (`Pane.qml`, `slowTimer`). The small dot beside the item count already showed while file details load. Not seen mid-load: the QML loads and a Google Drive folder opens, but the headless screenshot fires after the listing is back.

## 12.5 **inserted**
- [x] The release steps are written down in `docs/RELEASING.md`: Arch package and tarball (AppImage is postponed; `.deb`, `.rpm` and Flatpak are skipped), built from one optimised binary, with what goes in each, the checks and the publish step (asks first).
- [x] Releases are built from `master` in a separate worktree (`git worktree add ../CaptainFM-release master`); a Dev branch is never merged into `master` for a release.
- [x] Packaging files made and the first build run: `packaging/arch/PKGBUILD` + `captainfm.desktop`, and `scripts/release.sh`, which builds from `master` in the `../CaptainFM-release` worktree (nothing committed to `master`). Output in `dist-release/v0.1.0/`: `captainfm-0.1.0-1-x86_64.pkg.tar.zst` (3.6 MB), `captainfm-0.1.0-x86_64-linux.tar.gz` (3.8 MB), `PKGBUILD`, `.SRCINFO`, `SHA256SUMS`. Checked: package file list and dependencies, tarball `install.sh` into a scratch prefix, the installed binary starts headless and shows the folder, `desktop-file-validate` passes. The first try failed to link until the PKGBUILD got `options=('!lto')`.
- [ ] Still to do for the release: `cargo test` in the packaging run (skipped), install in a clean container, pick the version number (files say 0.1.0, Settings says v0.01 Ahab), then publish (asks first) and the AUR push.
- [ ] AppImage (postponed): a first try on 2026-10-24 made a 93 MB file, but linuxdeploy's Qt plugin choked on the `kimg_*` image plugins and the headless run gave no screenshot. Needs a build on an old distro.

## 12.6 spacebar
- [x] The space bar toggles the preview pane (inspector) in both key styles, as in other file managers. To make room, marking an item is now `Insert` (File Pilot keys; Vim keeps `m`) and the Vim leader moved from `Space` to `,` (`, f f`, `, v i`, `, ,` for the palette, `, s` for Settings). Docs, examples and `scripts/screenshots.sh` follow.


## 13. Performance test
- [x] Create a performance test: `scripts/perf.py` (replaces `bench.py`). Cases: startup to "QML loaded", listing a 20,000-file folder, and indexing plus searching 70,000 files. Each reports CPU time, peak memory and the app's own `[timing]` figure.
- [x] Standardise it: the fixture is generated the same way every run (fixed names and counts) in a scratch folder that is also `$HOME`; headless; a warm-up run, then 5 timed runs with median, best and worst. `FIXTURE_VERSION` in the script is bumped whenever the cases change, and runs with different versions are never compared.
- [x] Write a small report for each run: `docs/perf/<date>_<commit>_<profile>.md` (and `.json`) with commit, build profile, binary size, CPU, cores, RAM, kernel, and a change column against the previous comparable run. Baseline: `docs/perf/2026-10-24_1737_0958bc4_dist.md` (fixture v2: startup 330 ms, 20,000-file listing 17 ms, 70,000-file index 11 ms, peak 144 MB). The earlier v1 report is kept but never compared.
- [x] It is a step in `docs/RELEASING.md` (run `scripts/perf.py` on the release binary and keep the report). Not yet done for a real release, since none is published; do it with the first one.
- Reading the numbers: Small timings are noisy (the report says so); trust CPU time and memory first.
- [x] Added copy throughput (fixture v2): 100 files of 2 MB copied into an empty folder through the real clipboard path, timed by a new `[timing] copied N items, B bytes: T` line that `CFM_DEBUG` prints when a copy or move ends (`ops::paste_tracked`). `--scratch DIR` picks the disk the fixture lives on. Baseline on the temp folder: about 140 ms, 1.4 GB/s. On this machine's home disk the same copy took 6 ms because the filesystem clones files instead of copying them, so that number says nothing about disk speed; use a different filesystem or a bigger fixture for that.
- Thumbnails need no case: `thumbnails.rs` only looks up the desktop's thumbnail cache (one file lookup per row); CaptainFM makes none itself.
- [x] Decided what is not measured: scrolling smoothness and frame times (the offscreen renderer is software, so its frame times say nothing about a real GPU) and a remote or cloud folder (needs a real server, and the time is the network's). Check these by hand before a release: open the 20,000-file folder from `scripts/perf.py`, hold `j` / Page Down in list and grid views, and look for stutter; open a remote folder and check the listing appears while loading. Add a case if a headless way appears.

## 14. Lag with remote connections
Mounting (`Backend::connect_job`) and reading a folder (`DirModel::refresh`) already run on their own threads. The freeze comes from remote file lookups that still run on the UI thread, and from background work that jams the single gvfs FUSE channel so those lookups wait even longer. A UI thread blocked for a few seconds is what makes Hyprland show "Application not responding".
- [x] Tell remote paths apart without touching them: read `/proc/self/mountinfo` (fstype `fuse.gvfsd-fuse`, `fuse.rclone`, `fuse.sshfs`, `nfs*`, `cifs`, `smb3`, `9p`, or anything under the gvfs folder), cached for about 2 s (`listing::remote_root`, `listing::is_remote`). Tests on a sample mount table.
- [x] No recursive folder sizes on remotes (stage 3 of `DirModel::refresh`): up to 6 threads walk the whole remote tree and every other lookup queues behind them. Folder rows show no size there, as in Miller columns. One stat per entry (stage 2) stays.
- [x] Do not set up the change watcher on a remote folder (`DirModel::rewatch`): notify's `watch()` looks the path up on the UI thread and can wait seconds on a fresh mount, and inotify never sees changes made on the server anyway. Our own pastes, renames and deletes already reload through `opDone`, and reload still works.
- [x] Make the lookups QML calls directly safe on remotes: `Backend.exists` and `isDir` (pinned remote folders in the sidebar, JumpBar, Main) wait at most about 150 ms on a remote and then assume yes. `sameDevice` (drag hover) compares mount points instead of stat'ing, and `state::mount_root` returns the remote's mount point without checking each parent folder.
- [x] A spinner while waiting for a remote: a turning arc instead of the pulsing dot in the empty-folder "Loading…" (still after 300 ms, so local folders never flash it). It says "Waiting for <server>…", counts seconds after 3 s, and shows "Connecting to <name>…" in the pane while a connection is being made (now only a toast) (`Pane.qml`, `Main.pendingMount`, a new `remote` property on `DirModel`).
- [x] Check: `cargo test`, headless screenshots of the connecting and loading states, `scripts/perf.py` (local folders still get sizes and are no slower), and a live `sftp://localhost` (if sshd runs) or an SMB share with `CFM_DEBUG=1`. The spinner should keep turning, keys should keep working, and no "not responding" should appear.
  - Done 2026-10-25. `cargo test` passes (two new mount-table tests). `scripts/perf.py`: no case slower (`docs/perf/2026-10-25_1217_5fbb4e9_dist.md`), and a local folder still shows folder sizes. Screenshots: "Waiting for <server>…  4 s" on an rclone FUSE mount slowed with `--tpslimit`, and "Connecting to 127.0.0.1:22222…  6 s" against a port that accepts and never answers.
  - Live `sftp://` through gvfs (sshd is disabled here, so `rclone serve sftp` on 127.0.0.1:22223 in front of a WebDAV server limited to 4 requests/s; 12 folders of 40 subfolders each). Opening a folder while its parent was loaded took 5.75 s and 91 server listings before (the size walk), 0.33 s and 11 after. Fed keys never waited more than one 250 ms tick in either build, so that mount never froze the UI; the watcher and lookup fixes matter on a fresh or hung mount, which this setup does not produce.

## 15. Middle Truncation
Long names are cut at the end today, so `Screenshot 2026-10-25 at 11.52.png` and `Screenshot 2026-10-25 at 11.58.png` both read `Screenshot 2026-10-2…` and the extension is lost. Finder cuts names in the middle (`Screenshot 2026…11.52.png`) so the start and the end both show. Only switch labels that are already truncated; names that never get cut stay as they are.
- [x] File and folder names in the list view (`Pane.qml` row name, now `ElideMiddle`). The trailing `/` on folders stays visible too.
- [x] Icons and Columns views (`Pane.qml` grid cells). Columns uses `ElideMiddle`; Icons wraps to two lines, where Qt can only cut at the end, so the name is cut in the middle by the helper to about two lines' width first.
- [x] Miller columns (`MillerColumn.qml` row name).
- [x] The inspector's folder contents list (`Inspector.qml`).
- [x] Sidebar labels: pinned entries, drives, remotes and cloud drives (`Sidebar.qml`). The armed text ("unpin <name>?") keeps its `?` visible as well.
- [x] Tab chips (`Main.qml`): the title is cut by the helper against the widest chip (220 px at zoom 1), not the chip's own width, which follows the text.
- [x] Picker rows that are names (`Picker.qml`): items with `elide: "middle"` (open tabs, devices, saved connections in the Connect and Edit pickers). Folder rows are cut at the start and commands at the end. Before, the picker's `ElideLeft` did nothing, since Qt does not elide rich text.
- [x] Deep search result names (`Pane.qml`), cut first and then highlighted; the folder column next to it stays `ElideLeft`.
- [x] The helper: `Style.elide(text, font, width, mode)` (default `Text.ElideMiddle`) uses `FontMetrics.elidedText`, and highlighting is applied to the cut text, so a match that falls in the removed middle is simply not shown. A shared `TextMetrics` was tried first and broke: every binding depended on its `elidedText`, so all icon cells showed the last name.
- Already middle: conflict dialog, batch rename preview, drag ghost, inspector title, transfer panel's current file. Already cut at the start, on purpose (the end of a path matters most): deep search folder column, jump bar, Settings paths, picker folder rows. Stay cut at the end (sentences, not names): transfer titles, pick bar title, Settings hints, Connect dialog notes, inspector details and media info, picker categories.
- [x] Checked 2026-10-25 with headless screenshots of a fixture (`report-final-version-for-the-board-v1/v2.pdf`, two long screenshot names, two long folders that differ at the end, a short name): list, icons, columns, Miller, tree, inspector (title and contents), deep search with highlighting, a pinned sidebar entry, the tab chip, the tab picker and the palette's folder row, at 560 to 900 px wide. Both ends show in each, and short names are unchanged. Plain labels use Qt's own elide, so scrolling does no new work; scroll smoothness was not measured (no headless way, see section 13).



## 16. Unit tests where they matter
Done 2026-10-25: 169 tests became 187 (plus 1 ignored on purpose; 190 after 16.1), `cargo test --release` still takes about 1 s, and five runs in a row all passed. Not more tests for the count: these cover the code that moves or removes your files, and the code section 17 is about to change. From now on, each section 17 fix lands with its own test.
- [x] Trash and undo (`ops.rs`), before 17.0 changes it: two files with the same name trashed at once (both kept, one undo puts each back in its own folder), a folder with its contents, a missing file that does not stop the rest (and leaves no `.trashinfo`), and a move across drives (copy then remove, checked against `/dev/shm`, a second filesystem; skips itself where there is none).
- [x] `a_remote_file_never_lands_in_the_local_trash` is written and marked `#[ignore]`: it fails today (checked with `cargo test -- --ignored`), so 17.0 is done when it passes and the `#[ignore]` comes off. It fakes a gvfs share by pointing `XDG_RUNTIME_DIR` into the sandbox (`Sandbox::set_env`, new).
- [x] Transfer queue (`transfer.rs`, 2 tests became 7): pause holds the copy until resumed, a paused job shows no speed or time left, progress counts items when sizes are unknown, an unknown conflict answer never means replace, and only the last 8 finished jobs stay listed. Cancel, the half-written file and replace / skip / keep-both were already covered in `ops.rs`.
- [x] `dir_model.rs` (no tests, now 9): the row lookups, range selection and tree collapse moved out of the Qt object into plain functions (`find_prefix_in`, `parent_row_in`, `range_paths`, `collapse_in`), and the display order (`compute_order`) is tested directly: dotfiles hidden, folders first, natural numbers, size order, extension filter that spares folders, filter text, grouping and tree nesting. Not covered: which background work is skipped on a remote; it is inline in `refresh`'s worker thread, and 17.2 will touch it anyway.
- [ ] With section 17, one test per fix in `listing.rs` / `devices.rs` / `openwith.rs`: a dead mount (`ENOTCONN`) is recognised, a remote that did not answer is remembered for about 10 s, a file type guessed from the extension on remotes.
- Not unit tests: the QML (`Style.elide` included). Headless screenshots catch more there than a QML test runner would.

### 16.1 Clean `cargo clippy` and `cargo fmt`
Done 2026-10-25. `cargo clippy --release --all-targets` gives 0 warnings (was 8: 3 in the app, 5 in tests), `cargo fmt --check` is clean, and 190 tests pass (1 ignored, the 17.0 one).
- [x] App code: a nested `if` in the word-form rule merged (`fuzzy::stem`), the first row of `fuzzy::edit_distance` filled with an iterator, and the palette's learned picks sorted with `sort_by_key` (`state::Usage::record`). Same behaviour.
- [x] Tests: five `&[x.clone()]` became `std::slice::from_ref(&x)` (`ops.rs`, `backend.rs`).
- [x] New tests for the code those fixes touched: the doubled-letter rule (`stopped` and `dropping` lose a letter; `falling`, `buzzer` and `pressed` keep theirs), edit distance at the edges (empty words, a swap, an accented letter counts once), and learned palette picks (a repeat counts up; past 500 the oldest go first).
- [x] `cargo fmt` on the section 14 code that was never formatted (`backend.rs`, `listing.rs`, `remotes.rs`); layout only.
- [x] Keep it clean: `./build.sh` now runs `cargo fmt --check`, `cargo clippy --release --all-targets -- -D warnings` and the unit tests (`./run-tests.sh`, new: headless, `./run-tests.sh <name>` for some, `--ignored` to include the known failures, `--list`) before it builds, and stops before installing if one fails. `--no-checks` skips them; `--dry-run` lists them. Not in `build.rs`: that is Cargo's own build script, which runs inside `cargo build` and `cargo clippy`.
- [ ] The same checks in `scripts/release.sh` (17.5).

## 17. More remote fixes, and other improvements
Found by reading what still touches a remote after section 14. Sub-sections are in priority order (17.0 first); items inside each are highest first.

### 17.0 Remote: data safety (do first)
- [ ] Trashing a remote file downloads it. `ops::trash_one` moves it into the local `~/.local/share/Trash`; across devices that is a full copy and then a delete on the server, with no progress shown and no way to cancel. On a remote, use the server's trash (`gio trash` on gvfs mounts, which some backends support). Where there is none, say "this server has no trash, delete permanently?" and ask twice, as with the double right-click.

### 17.1 Remote: UI freezes
The app stops responding, which is what Hyprland flags as "not responding".
- [ ] "Open with…" freezes on remote files. `openwith::content_type` runs `gio info` on the UI thread (`Backend.openWithList` from the context menu), and on a remote that sniffs the file over the network. Guess the type from the extension on remotes, or run it on a thread and fill the menu when it returns.
- [ ] The sidebar can stall for one wait per pinned remote. `Sidebar.qml` calls `Backend.exists` and then `isDir` for each pin, and each waits up to 150 ms on a hung remote, so 5 pins is up to 1.5 s. Remember for about 10 s that a remote did not answer, and skip it in later lookups. Or ask for all pins in one threaded call.

### 17.2 Remote: network load
Too much traffic at once jams the single gvfs channel, and every other lookup waits behind it.
- [ ] Whole-drive deep search on a remote walks the entire server. `Main.sidebarSearch` and `search.drive` start the index at `Backend.mountRoot`, which for a remote is the mount point. That means up to 8 threads listing every folder, plus the 30 s re-check of folder times, which is exactly what jams the gvfs channel. On a remote, limit it to the current folder (or a shallow depth), say so in the search bar, and skip the background re-check.
- [ ] Icons view loads every full-size image over the network (`Pane.qml` thumbnail `Image`, `file://` source). `sourceSize` only shrinks it after the whole file is read, so a folder of photos pulls all of them at once. On a remote, show the glyph and load images only for visible cells, one or two at a time. Or use the desktop thumbnail cache only, as for other files.
- [ ] Preview requests on a remote pile up. Holding `j` over remote files starts a thread per row (`Backend.request_preview`), and each one reads up to 24 KB through the same channel. Wait for the cursor to rest (about 150 ms) on remotes, and drop answers for rows no longer current.

### 17.3 Remote: connection lifecycle
- [ ] A dead mount (rclone exited, Wi-Fi dropped) gives "Transport endpoint is not connected" or a silent empty folder. Detect `ENOTCONN` in `DirModel::refresh` and show "Lost connection to <name> · Reconnect" in the pane, using the saved profile (`remotes`). Unmount the stale FUSE mount first.
- [ ] Offer to reconnect saved connections at start, and to remember the last folder per connection. Today a pinned remote folder only works if the share happens to be mounted already.

### 17.4 Remote: check (after 17.0 to 17.3)
- [ ] Check: a slowed rclone mount (`--tpslimit`, as in section 14) with photos, a deep folder tree and a few pins. Trash a file (check the local trash stays empty and it lands in the server's trash, or the delete is confirmed). Run a drive search, open "Open with…", scroll Icons view, kill rclone mid-browse, and start the app with the mount hung. Keys must keep working throughout.

### 17.5 Release and versioning
- [ ] Pick the version scheme. Settings says v0.02 Blackbeard, `Cargo.toml` and the PKGBUILD say 0.1.0, and the only tag is `v0.01`. Make one number the source and have Settings read it (`env!("CARGO_PKG_VERSION")`).
- [ ] Finish the release steps from 12.5: `cargo test` in the packaging run, and an install in a clean container.

### 17.6 Performance
- [ ] QML Profiler pass on interface build time (section 9, about 230 ms).

### 17.7 Features
- [ ] Draggable tabs (section 100): drop a file on a tab.

## 50. Features few people use: decide with the numbers
Counting began on 2026-10-24 (section 10), so the earliest sensible review is about 2026-11-21.
- [ ] After 4 weeks of counts, read "Show feature use…" and decide per feature: switch off by default, or compile out behind a cargo feature if it also costs build time. All plugins are on by default for now, so nothing seems to disappear.
- [ ] Likely under 5%: Lua plugins themselves (vendored Lua adds build time and size), batch rename templates and regex, the learned palette history. Not changed without numbers.
- Keep, even though not everyone uses them: the folder-handler D-Bus service and portal file chooser (they make it the default file manager), and trash and undo. Font previews stay built in because they need the font drawn, not just text.

## 100. Draggable tabs
- [ ] Drop a file onto a tab to put it in that tab's folder.
- [ ] Drag a tab out to make a split pane.
