# Making a release

The steps Claude (or anyone) follows to publish CaptainFM. Every package (Arch, tarball, AppImage) is built from the same optimised binary
(`./build.sh` profile `dist`, about 6 minutes), so build it once and wrap it for each format.

Status: the Arch package and the tarball were built on 2026-10-24 with `scripts/release.sh` (version 0.1.0, from
`master` at the time). Nothing has been published yet. On 2026-10-25 the package was installed in a clean `archlinux`
container (podman): dependencies resolved, `captainfm --version` ran, and the app started headless and drew a folder,
but with every letter a box until a font was installed, so the PKGBUILD now depends on `ttf-font`.

Shortcut: `scripts/release.sh` does steps 1 and 2 and the tarball from `master`, and writes `dist-release/vX.Y.Z/`
(package, tarball, `PKGBUILD`, `.SRCINFO`, `SHA256SUMS`). It takes about 10 minutes (`--skip-tests` skips `cargo test`).
It does not build the AppImage: run `scripts/appimage.sh` (step 3) from the same worktree, then rewrite `SHA256SUMS`.
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

## 3. AppImage (part of every release, rolling distros only for now)

`scripts/appimage.sh` builds it from the `dist` binary in about a minute (`target/dist/captainfm`, so run `./build.sh`
or `cargo build --profile dist` from the master worktree first) and writes `CaptainFM-X.Y.Z-x86_64.AppImage` into
`dist-release/vX.Y.Z/`. It checks the tool checksums (`~/Work/CaptainFM-tools/SHA256SUMS`), that every `kimg_*` image
plugin finds its libraries (`jxrlib` must be installed), then runs `--version` and a headless screenshot, and refuses to
finish if nothing was drawn.

Tried 2026-10-26 (a trial build from the Dev1 tree, not a release artifact): 107 MB; works in a clean `archlinux`
container with no Qt installed (version, folder drawn, PNG preview); opens a native Wayland window on this machine (not
XWayland). What it took, all now in the script:
- the Qt plugin only adds `xcb`: `EXTRA_PLATFORM_PLUGINS="libqwayland.so;libqoffscreen.so"` plus the three
  `wayland-*` plugin folders copied by hand (without them there is no Wayland window, and no headless check);
- linuxdeploy's `AppRun` starts the binary through a symlink, so Qt looks for its platform plugins next to the symlink and
  aborts with "Could not find the Qt platform plugin"; the script writes its own `AppRun` that sets `QT_PLUGIN_PATH` and
  `QML_IMPORT_PATH` and starts the real binary (this was the "no screenshot" failure of the first try; the `kimg_jxr`
  problem was only `jxrlib` missing);
- the plugins are compiled into the binary, so there is nothing else to copy.

**Limits (say them in the release notes):**
- **glibc:** built on this Arch machine it needs glibc 2.44 or newer. In a Debian 13 container (glibc 2.41) it does not
  start (`GLIBC_2.44 not found`). So today it only helps rolling distros (Arch and derivatives, Fedora Rawhide or the
  newest release, openSUSE Tumbleweed). To run on older systems it must be built inside an older distro's container
  (Debian 12 or Ubuntu 22.04), which needs a Qt 6 there that the app builds against; not tried.
- **Host libraries are not bundled** (linuxdeploy's exclude list): `libGL`, `libEGL`, `libOpenGL` (`libopengl0` on
  Debian), `fontconfig`, `freetype`, `harfbuzz`, `libxkbcommon`, `libwayland-client`, `libdrm`, `libgbm`, and at least one
  font. A minimal container lacks them; a desktop has them.
- FUSE mounts, gvfs, udisks and rclone are host tools too; the app finds them on `PATH`.
- No update information is embedded.

Checks before publishing (the script does 1 and 2):
1. `./CaptainFM-X.Y.Z-x86_64.AppImage --version` prints the `Cargo.toml` version.
2. A headless run (`QT_QPA_PLATFORM=offscreen`, `CFM_SHOT=...`) draws a folder and saves a screenshot.
3. In a clean container of a distro whose glibc is at least as new as the build machine's (`archlinux`): install the
   host libraries above, run with `--appimage-extract-and-run`, folder drawn and an image previewed.
4. A live Wayland window on the build machine (`hyprctl clients -j` shows `"xwayland": false`).

Do not publish an AppImage that failed 1 or 2; say so in the release notes and ship the other formats instead.

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
- Formats: Arch package, a tarball and an AppImage (step 3; `scripts/appimage.sh`, built and checked 2026-10-26 as a trial; rolling distros only because of glibc; not yet called from `scripts/release.sh`). `.deb`, `.rpm` and Flatpak are skipped on purpose.
- Version numbers disagreed until 2026-10-25 (see step 0): the first packaging run named its files `0.1.0` while `master` says `v0.01 *Ahab*`. The scheme is now `0.0.N`, so Ahab is 0.0.1; `master` still says 0.1.0 in `Cargo.toml` until it gets this change.
