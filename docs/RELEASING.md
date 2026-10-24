# Making a release

The steps Claude (or anyone) follows to publish CaptainFM. Every package is built from the same optimised binary
(`./build.sh` profile `dist`, about 6 minutes), so build it once and wrap it five times.

Status: written down, not yet run. Nothing here has been tried end to end; the first release will fix the rough
edges, so correct this file as you go.

## 0. Before anything

- [ ] Work is committed on a clean branch and merged to `master`; `git status` is empty.
- [ ] `cargo test --release` passes.
- [ ] One version everywhere. Today `Cargo.toml` says `0.1.0` while Settings shows `CaptainFM v0.02 *Blackbeard*`
      (`qml/Settings.qml`). Pick the number, then update `Cargo.toml` (and `Cargo.lock` through a build), the Settings
      line, `README.md` and the packaging files below.
- [ ] Tag it: `git tag -a vX.Y.Z -m "CaptainFM vX.Y.Z <codename>"`, then push the tag.
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

1. Write `packaging/arch/PKGBUILD`: `pkgname=captainfm`, `source` is the tagged tarball from GitHub, `makedepends=(rust qt6-declarative cmake)`,
   `depends=(qt6-declarative qt6-multimedia fuse3)`, `optdepends=('rclone: cloud drives' 'gvfs: remote servers' ...)`.
2. `build()`: `cargo build --profile dist --locked`. `package()`: `install -Dm755` the binary, `ln -s captainfm "$pkgdir/usr/bin/cfm"`,
   `install -Dm644` the desktop entry, both icons and `LICENSE`.
3. In a clean directory: `makepkg -sf` (use `makepkg --printsrcinfo > .SRCINFO` for the AUR).
4. Check: `pacman -Qlp captainfm-*.pkg.tar.zst`, install in a clean container or VM, launch it.
5. Artifact: `captainfm-X.Y.Z-1-x86_64.pkg.tar.zst`. AUR: push `PKGBUILD` and `.SRCINFO` to the `captainfm` AUR repo.

Tool already here: `makepkg`.

## 3. Debian / Ubuntu `.deb`

1. Tool: `cargo install cargo-deb` (or build on a Debian or Ubuntu box or container: `distrobox` / `podman` with `ubuntu:24.04`).
2. Add a `[package.metadata.deb]` section to `Cargo.toml`: `depends = "libqt6qml6, libqt6gui6, libqt6network6, libqt6dbus6, qml6-module-qtquick, qml6-module-qtquick-controls, fuse3"`, `recommends = "gvfs, udisks2, ffmpeg, rclone"`,
   and `assets` for the binary, desktop entry, icons and licence.
3. Build inside the oldest Ubuntu LTS you want to support, not on Arch: a binary linked against a newer glibc or Qt
   will not start on older systems. `cargo deb --profile dist`.
4. Check: `dpkg-deb -I`, `dpkg-deb -c`, `lintian`, install in the container, launch.
5. Artifact: `captainfm_X.Y.Z-1_amd64.deb`.

## 4. Fedora / openSUSE `.rpm`

1. Tool: `cargo install cargo-generate-rpm`, or write `packaging/rpm/captainfm.spec` and use `rpmbuild -ba` in a Fedora container.
2. `Requires: qt6-qtbase qt6-qtdeclarative qt6-qtmultimedia fuse3`, `Recommends: gvfs udisks2 ffmpeg-free rclone`.
3. Build inside a current Fedora container so the Qt version matches what users have.
4. Check: `rpm -qpl`, `rpm -qpR`, `rpmlint`, install in the container, launch.
5. Artifact: `captainfm-X.Y.Z-1.x86_64.rpm`.

## 5. AppImage

1. Tools: `linuxdeploy` with the `linuxdeploy-plugin-qt` and `appimagetool` (download the AppImages from their GitHub releases and verify their checksums).
2. Build the binary on the oldest distro you support (same reason as the `.deb`); an AppImage runs everywhere only if it is built against old glibc.
3. Lay out `AppDir/` with the binary, desktop entry and icon, then
   `linuxdeploy --appdir AppDir --plugin qt --executable target/dist/captainfm --desktop-file ... --icon-file packaging/captainfm.svg --output appimage`.
   Set `QML_SOURCES_PATHS=qml` so the plugin bundles the QML modules the app imports.
4. FUSE mounts, gvfs, udisks and rclone are host tools and are not bundled; the app must still find them on `PATH`.
5. Check: run it on a different distro than the build box, with `--appimage-extract-and-run` if FUSE 2 is missing.
6. Artifact: `CaptainFM-X.Y.Z-x86_64.AppImage`.

## 6. Flatpak

1. Tool: `flatpak-builder` and the `org.kde.Platform` / `org.kde.Sdk` 6.x runtimes, plus `org.freedesktop.Sdk.Extension.rust-stable`.
2. Write `packaging/flatpak/io.github.jgarza9788.CaptainFM.yml` (app id matches the desktop entry's id; `cli.rs` has `DESKTOP_ID`, keep them the same).
   `runtime: org.kde.Platform`, `command: captainfm`, offline build with `cargo vendor` or `flatpak-cargo-generator.py` from `Cargo.lock`.
3. The hard part is the sandbox, because a file manager is useless inside one: `--filesystem=host`, `--socket=wayland`, `--socket=fallback-x11`,
   `--share=network`, `--talk-name=org.gtk.vfs.*`, `--talk-name=org.freedesktop.UDisks2`, and `--device=dri`. Expect
   FUSE mounts (rclone cloud drives) and running host tools to be limited or impossible; decide per feature and
   write the limits in the README.
4. Add an AppStream file (`packaging/flatpak/io.github.jgarza9788.CaptainFM.metainfo.xml`) with the version, release notes and screenshots from `docs/images`.
5. Check: `flatpak-builder --user --install --force-clean build-dir <manifest>`, run `flatpak run <id>`.
6. Artifact: a bundle (`flatpak build-bundle`) now; Flathub submission later.

## 7. Publish

1. `gh release create vX.Y.Z dist-release/vX.Y.Z/* --title "CaptainFM vX.Y.Z <codename>" --notes-file <notes>`.
   Notes: what is new (from `git log` since the last tag), known limits, install lines for each format, link to
   https://rclone.org/install/ for cloud drives.
2. Only publish after asking the user: a release is public and cannot be quietly taken back. Show the file list, the
   checksums and the notes first.
3. Update the README install section with the new formats and link the release.
4. Record in `todo.md` what shipped and what did not.

## Known gaps

- None of the packaging files exist yet (`packaging/` has only icons and plugins).
- None of the tools except `makepkg` are installed on this machine; use containers for the rest.
- Version numbers disagree (see step 0).
