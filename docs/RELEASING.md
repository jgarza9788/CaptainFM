# Making a release

The steps Claude (or anyone) follows to publish CaptainFM. Every package is built from the same optimised binary
(`./build.sh` profile `dist`, about 6 minutes), so build it once and wrap it for each format.

Status: the Arch package and the tarball were built on 2026-10-24 with `scripts/release.sh` (version 0.1.0, from
`master` at the time). Nothing has been published yet, and the package was not installed in a clean container.

Shortcut: `scripts/release.sh` does steps 1 and 2 and the tarball from `master`, and writes `dist-release/vX.Y.Z/`
(package, tarball, `PKGBUILD`, `.SRCINFO`, `SHA256SUMS`). It takes about 10 minutes (`--skip-tests` skips `cargo test`).
It never commits to or merges into `master`; the packaging files live on the Dev branch (`packaging/arch/`).

## 0. Before anything

- [ ] Release files are built from `master`, as it is. **Never merge a Dev branch into `master` for a release** (the
      user decides when that happens). Build in a separate worktree so the Dev branch you are working on is untouched:
      `git worktree add ../CaptainFM-release master` (then `git worktree remove ../CaptainFM-release` afterwards).
      Run every build, test and package command below from that folder. `git status` there must be empty, and
      nothing is committed to `master` by the release.
- [ ] `cargo test --release` passes.
- [ ] `scripts/perf.py` on the release binary (in the master worktree); commit the report in `docs/perf/` on the Dev branch and compare its change column with the last release: a clear, repeated slowdown (or more memory) is a reason to look before publishing.
- [ ] One version everywhere. Today `Cargo.toml` says `0.1.0` while Settings shows `CaptainFM v0.02 *Blackbeard*`
      (`qml/Settings.qml`). Pick the number, then update `Cargo.toml` (and `Cargo.lock` through a build), the Settings
      line, `README.md` and the packaging files below.
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

Runtime dependencies: `qt6-base`, `qt6-declarative`, `qt6-multimedia`, `fuse3`. Recommended or optional: `gvfs`
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
   `depends=(qt6-declarative qt6-multimedia fuse3)`, `optdepends=('rclone: cloud drives' 'gvfs: remote servers' ...)`.
2. `build()`: `cargo build --profile dist --locked`. `package()`: `install -Dm755` the binary, `ln -s captainfm "$pkgdir/usr/bin/cfm"`,
   `install -Dm644` the desktop entry, both icons and `LICENSE`.
3. In a clean directory: `makepkg -sf` (use `makepkg --printsrcinfo > .SRCINFO` for the AUR).
4. Check: `pacman -Qlp captainfm-*.pkg.tar.zst`, install in a clean container or VM, launch it.
5. Artifact: `captainfm-X.Y.Z-1-x86_64.pkg.tar.zst`. AUR: push `PKGBUILD` and `.SRCINFO` to the `captainfm` AUR repo.

Tool already here: `makepkg`.

## 3. AppImage (postponed, not part of releases yet)

First try on 2026-10-24, built on this Arch machine with the tools in `~/Work/CaptainFM-tools`: linuxdeploy's Qt plugin fails on the host's `kimg_*` image plugins (missing `libjpegxr`, `libjxrglue`). Worked around with stand-in libraries, then deleting `kimg_*` from the AppDir; `appimagetool` then made a 93 MB file. Its headless run produced no screenshot and was never debugged. Start from there, and build on an old distro.

1. Tools: `linuxdeploy` with the `linuxdeploy-plugin-qt` and `appimagetool` (download the AppImages from their GitHub releases and verify their checksums).
2. Build the binary on the oldest distro you support (a binary built against new glibc will not start on older systems); an AppImage runs everywhere only if it is built against old glibc.
3. Lay out `AppDir/` with the binary, desktop entry and icon, then
   `linuxdeploy --appdir AppDir --plugin qt --executable target/dist/captainfm --desktop-file ... --icon-file packaging/captainfm.svg --output appimage`.
   Set `QML_SOURCES_PATHS=qml` so the plugin bundles the QML modules the app imports.
4. FUSE mounts, gvfs, udisks and rclone are host tools and are not bundled; the app must still find them on `PATH`.
5. Check: run it on a different distro than the build box, with `--appimage-extract-and-run` if FUSE 2 is missing.
6. Artifact: `CaptainFM-X.Y.Z-x86_64.AppImage`.

## 4. Publish

1. `gh release create vX.Y.Z dist-release/vX.Y.Z/* --title "CaptainFM vX.Y.Z <codename>" --notes-file <notes>`.
   Notes: what is new (from `git log` since the last tag), known limits, install lines for each format, link to
   https://rclone.org/install/ for cloud drives.
2. Only publish after asking the user: a release is public and cannot be quietly taken back. Show the file list, the
   checksums and the notes first.
3. Update the README install section with the new formats and link the release.
4. Record in `todo.md` what shipped and what did not.

## Known gaps

- Not done: `cargo test` during the first packaging run (it was skipped), installing the package in a clean container, the AUR push, and a real `sha256sums` for the AUR.
- Binary tarball: `captainfm-X.Y.Z-x86_64-linux.tar.gz` holds the package files plus `install.sh` (to `/usr/local` or `PREFIX=`). It links against this machine's Qt 6.12, so it only suits distros with a similar Qt.
- Formats: Arch package and a tarball are done; AppImage is postponed (decided 2026-10-24). `.deb`, `.rpm` and Flatpak are skipped on purpose.
- Version numbers disagree (see step 0). The files are named `0.1.0` (from `Cargo.toml`); `master` says `v0.01 *Ahab*` in Settings and the git tag is `v0.01`. The user decides which number the release carries.
