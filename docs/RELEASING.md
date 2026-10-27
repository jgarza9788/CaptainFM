# Making a release

The steps Claude (or anyone) follows to publish CaptainFM. Every package (Arch, tarball, AppImage) is built from the same optimised binary
(`./build.sh` profile `dist`, about 6 minutes), so build it once and wrap it for each format.

Status: the Arch package and the tarball were built on 2026-10-24 with `scripts/release.sh` (version 0.1.0, from
`master` at the time). Nothing has been published yet. On 2026-10-25 the package was installed in a clean `archlinux`
container (podman): dependencies resolved, `captainfm --version` ran, and the app started headless and drew a folder,
but with every letter a box until a font was installed, so the PKGBUILD now depends on `ttf-font`.

Shortcut: `scripts/release.sh` does steps 1 and 2 and the tarball from `master`, and writes `dist-release/vX.Y.Z/`
(package, tarball, `PKGBUILD`, `.SRCINFO`, `SHA256SUMS`). It takes about 10 minutes (`--skip-tests` skips `cargo test`).
It does not build the AppImage yet: do step 3 by hand from the same worktree and binary, then rewrite `SHA256SUMS`.
It never commits to or merges into `master`; the packaging files live on the Dev branch (`packaging/arch/`).

## 0. Before anything

- [ ] Release files are built from `master`, as it is. **Never merge a Dev branch into `master` for a release** (the
      user decides when that happens). Build in a separate worktree so the Dev branch you are working on is untouched:
      `git worktree add ../CaptainFM-release master` (then `git worktree remove ../CaptainFM-release` afterwards).
      Run every build, test and package command below from that folder. `git status` there must be empty, and
      nothing is committed to `master` by the release.
- [ ] `cargo test --release` passes.
- [ ] `scripts/perf.py` on the release binary (in the master worktree); commit the report in `docs/perf/` on the Dev branch and compare its change column with the last release: a clear, repeated slowdown (or more memory) is a reason to look before publishing.
- [ ] One version everywhere: `Cargo.toml` is the only number. Settings shows it (`Backend.version()`), `captainfm
      --version` prints it, and `scripts/release.sh` names the files and sets the PKGBUILD's `pkgver` from it. Bump it in
      `Cargo.toml` (and `Cargo.lock` through a build) and change the codename in the Settings line (`qml/Settings.qml`).
      Scheme (decided 2026-10-25): `0.0.N` with a pirate codename; v0.01 *Ahab* is 0.0.1, v0.02 *Blackbeard* is 0.0.2.
- [ ] Tag: the existing release `v0.01` points at `5a722f6`. Attach files to an existing release with
      `gh release upload vX.Y.Z <files> --clobber`; a new version gets `git tag -a vX.Y.Z -m "CaptainFM vX.Y.Z <codename>" <master commit>`.
      Never move or delete a tag that is already published without the user saying so.
- [ ] Put every artifact in `dist-release/vX.Y.Z/` (ignored by git), with a `SHA256SUMS` file written last:
      `sha256sum * > SHA256SUMS`.

## 1. What goes in every package

| Piece | Where |
|---|---|
| Binary `captainfm` (QML is compiled in) | `/usr/bin/captainfm`, plus the `cfm` symlink |
| Desktop entry | `/usr/share/applications/captainfm.desktop` (`Exec=captainfm %U`, mime `inode/directory`) |
| Icons | `packaging/captainfm.svg` to `/usr/share/icons/hicolor/scalable/apps/`, `packaging/captainfm-symbolic.svg` to `/usr/share/icons/hicolor/symbolic/apps/` |
| Licence | `LICENSE` to `/usr/share/licenses/captainfm/` (or the distro's doc path) |
| Lua plugins | whatever `packaging/plugins/*` holds; check how `src/plugins.rs` finds bundled ones before packaging |

Do **not** run `captainfm --default`, `--install` or the portal setup from a package script. Making it the default
folder handler is the user's choice. `--install` writes into `$HOME`, which packages must not do; ship the desktop
entry and icons as files instead.

Runtime dependencies: `qt6-base`, `qt6-declarative`, `qt6-multimedia`, `fuse3`, and any font (`ttf-font` on Arch; a clean install with no font drew every letter as a box). Recommended or optional: `gvfs`
(remotes), `udisks2` (drives), `ffmpeg`, `imagemagick`, `poppler`, `libarchive`, `sqlite`, `wl-clipboard`,
`libsecret` (saved passwords), and `rclone` for cloud drives. rclone is a separate package; list it as optional and
point to https://rclone.org/install/ (never bundle it).

## 2. Arch package (also the one to test first, since this machine is Arch)

Done: `packaging/arch/PKGBUILD` and `packaging/arch/captainfm.desktop`. Lesson: it needs `options=('!lto')`, because
makepkg's `-flto` C++ objects cannot be read by rust-lld and the link fails with undefined `cxxbridge1$...` symbols.
The source is a `git archive` of `master` made by the script (not the GitHub tarball, which is why `sha256sums` is `SKIP`
here; set real sums for the AUR once a tag exists).

Original plan, for reference:
1. Write `packaging/arch/PKGBUILD`: `pkgname=captainfm`, `source` is the tagged tarball from GitHub, `makedepends=(rust qt6-declarative cmake)`,
   `depends=(qt6-declarative qt6-multimedia fuse3 ttf-font)`, `optdepends=('rclone: cloud drives' 'gvfs: remote servers' ...)`.
2. `build()`: `cargo build --profile dist --locked`. `package()`: `install -Dm755` the binary, `ln -s captainfm "$pkgdir/usr/bin/cfm"`,
   `install -Dm644` the desktop entry, both icons and `LICENSE`.
3. In a clean directory: `makepkg -sf` (use `makepkg --printsrcinfo > .SRCINFO` for the AUR).
4. Check: `pacman -Qlp captainfm-*.pkg.tar.zst`, install in a clean container or VM, launch it.
5. Artifact: `captainfm-X.Y.Z-1-x86_64.pkg.tar.zst`. AUR: push `PKGBUILD` and `.SRCINFO` to the `captainfm` AUR repo.

Tool already here: `makepkg`.

## 3. AppImage (part of every release)

The same `dist` binary, wrapped with the Qt libraries and plugins it needs. First try on 2026-10-24 made a 93 MB file,
but linuxdeploy's Qt plugin failed on the host's `kimg_*` image plugins (KDE's `kimageformats`) and the headless run gave
no screenshot. Cause of the failure: `kimg_jxr.so` needs `libjpegxr` and `libjxrglue`, which the `jxrlib` package provides
(`sudo pacman -S --needed jxrlib`), so the stand-in libraries and deleting `kimg_*` from the AppDir should no longer be
needed. Not yet repeated or debugged: do that first, and record the result here.

Tools live in `~/Work/CaptainFM-tools` (`linuxdeploy`, `linuxdeploy-plugin-qt`, `appimagetool`, with a `SHA256SUMS`).
Verify them with `sha256sum -c SHA256SUMS` before use; to refresh them, download the AppImages from their GitHub
releases and update the sums.

1. Prerequisites on the build machine: `jxrlib`, and every library the `kimg_*` plugins link to. Check with
   `for f in /usr/lib/qt6/plugins/imageformats/kimg_*.so; do ldd $f | grep 'not found'; done`, which must print nothing.
2. Build the binary from the `master` worktree (step 0) with `./build.sh` profile `dist`, the same binary as the other
   packages. Ideally build on the oldest distro you support (a binary built against new glibc will not start on older
   systems); until then the AppImage only promises to run on systems with a similar or newer glibc, and the release
   notes must say so.
3. Lay out `AppDir/` with the binary (and the `cfm` symlink), `packaging/arch/captainfm.desktop` and
   `packaging/captainfm.svg`, plus any bundled Lua plugins from `packaging/plugins/`, then
   `QML_SOURCES_PATHS=qml linuxdeploy --appdir AppDir --plugin qt --executable target/dist/captainfm --desktop-file packaging/arch/captainfm.desktop --icon-file packaging/captainfm.svg --output appimage`.
   `QML_SOURCES_PATHS` makes the plugin bundle the QML modules the app imports. Run these from the master worktree and
   put `AppDir/` outside the git tree or in an ignored folder.
4. FUSE mounts, gvfs, udisks and rclone are host tools and are not bundled; the app must still find them on `PATH`.
   Do not bundle fonts either; a system with no font shows boxes, as in the package check.
5. Check, in this order:
   - `./CaptainFM-X.Y.Z-x86_64.AppImage --version` prints the `Cargo.toml` version.
   - A headless run (`QT_QPA_PLATFORM=offscreen`, as in `scripts/screenshots.sh`) draws a folder and writes a screenshot.
     This is the check that failed on the first try.
   - On a different distro than the build box (a container is enough, `--appimage-extract-and-run` if FUSE 2 is
     missing): it starts, lists a folder, and previews an image and a video.
   - Image formats from the `kimg_*` plugins open, or the missing ones are named in the notes.
6. Artifact: `CaptainFM-X.Y.Z-x86_64.AppImage`, made executable (`chmod +x`), in `dist-release/vX.Y.Z/` before
   `SHA256SUMS` is written. No automatic update information is embedded yet.

Do not publish an AppImage that failed the headless check; say so in the release notes and ship the other formats
instead. Next step for the tooling: add this section to `scripts/release.sh` once one manual run has worked.

## 4. Publish

1. `gh release create vX.Y.Z dist-release/vX.Y.Z/* --title "CaptainFM vX.Y.Z <codename>" --notes-file <notes>`.
   Notes: what is new (from `git log` since the last tag), known limits, install lines for each format, link to
   https://rclone.org/install/ for cloud drives.
2. Only publish after asking the user: a release is public and cannot be quietly taken back. Show the file list, the
   checksums and the notes first.
3. Update the README install section with the new formats (AppImage: download, `chmod +x`, run) and link the release.
4. Record in `todo.md` what shipped and what did not.

## Known gaps

- Not done: `cargo test` during the first packaging run (it was skipped), installing the package in a clean container, the AUR push, and a real `sha256sums` for the AUR.
- Binary tarball: `captainfm-X.Y.Z-x86_64-linux.tar.gz` holds the package files plus `install.sh` (to `/usr/local` or `PREFIX=`). It links against this machine's Qt 6.12, so it only suits distros with a similar Qt.
- Formats: Arch package, a tarball and an AppImage (step 3; included in releases from 2026-10-26, not yet built end to end, and not yet in `scripts/release.sh`). `.deb`, `.rpm` and Flatpak are skipped on purpose.
- Version numbers disagreed until 2026-10-25 (see step 0): the first packaging run named its files `0.1.0` while `master` says `v0.01 *Ahab*`. The scheme is now `0.0.N`, so Ahab is 0.0.1; `master` still says 0.1.0 in `Cargo.toml` until it gets this change.
