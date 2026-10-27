#!/usr/bin/env bash
# Part 3 of the release: build the release files from `master`, never from the Dev branch.
#
#   scripts/release/30-build.sh [--skip-tests]
#
# About 12 minutes. Run one at a time: it deletes $OUT when it starts. It commits and publishes nothing. In order:
#   1. checks that $REL is on master with no changes, and warns if `cargo fmt` or `clippy` are not clean;
#   2. builds the Arch package with makepkg (the optimised `dist` build) and runs the unit tests inside it
#      (headless, away from your clipboard); a failing test stops the release;
#   3. makes the tarball from the package's files, with an install.sh;
#   4. makes the AppImage from the very binary in the package (scripts/appimage.sh), so all three files are the same
#      build; if that fails the release goes on without it and says so;
#   5. writes SHA256SUMS for whatever was made.
# Result in $OUT: captainfm-X.Y.Z-1-x86_64.pkg.tar.zst, captainfm-X.Y.Z-x86_64-linux.tar.gz,
# CaptainFM-X.Y.Z-x86_64.AppImage, PKGBUILD, .SRCINFO, SHA256SUMS.
# --skip-tests skips the lint and the tests (only for a rebuild of something already tested).
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/00-vars.sh"
parse_args "$@"; no_extra_args
need cargo cmake makepkg bsdtar
require_master
load_version

commit="$(git -C "$REL" rev-parse --short HEAD)"
# Scratch folder for everything makepkg and the tarball need; removed on exit, so nothing is left in the git tree.
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
rm -rf "$OUT"; mkdir -p "$OUT"
say "releasing $VERSION from master $commit into $OUT"

# --- source and PKGBUILD ----------------------------------------------------------------------------------------------
# The package is built from an archive of master (git archive: tracked files only), not from the working folder.
git -C "$REL" archive --format=tar.gz --prefix="captainfm-$VERSION/" -o "$work/captainfm-$VERSION-src.tar.gz" HEAD
cp "$DEV/packaging/arch/PKGBUILD" "$DEV/packaging/arch/captainfm.desktop" "$work/"
sed -i "s/^pkgver=.*/pkgver=$VERSION/" "$work/PKGBUILD"
# Build dependencies are checked by `need` above, so a missing one is a clear error and not a pacman prompt.
flags=(-f --noconfirm --nodeps)
[[ "$SKIP_TESTS" == 1 ]] && flags+=(--nocheck)

# --- lint report ----------------------------------------------------------------------------------------------------
# It warns and does not stop: master can lag behind the clean-up the Dev branch got. The tests run in the
# package's check() and do stop the build.
if [[ "$SKIP_TESTS" != 1 ]]; then
  (cd "$REL" && cargo fmt --check >/dev/null 2>&1) || warn "master is not cargo-fmt clean"
  warnings=$(cd "$REL" && cargo clippy --release --all-targets 2>&1 | grep -c '^warning: ' || true)
  [[ "$warnings" == 0 ]] || warn "cargo clippy reports $warnings warning lines on master"
fi

# --- the Arch package -----------------------------------------------------------------------------------------------
# Headless, and no Wayland socket, so the tests cannot touch the real clipboard.
export QT_QPA_PLATFORM=offscreen
unset WAYLAND_DISPLAY
(cd "$work" && makepkg "${flags[@]}")

pkg="$(ls "$work"/captainfm-"$VERSION"-1-x86_64.pkg.tar.zst)"
cp "$pkg" "$OUT/"
(cd "$work" && makepkg --printsrcinfo > "$OUT/.SRCINFO")
cp "$work/PKGBUILD" "$OUT/PKGBUILD"

# --- the tarball ----------------------------------------------------------------------------------------------------
# The same files the package installs, under one folder, plus a small install script.
stage="$work/tar/captainfm-$VERSION-x86_64-linux"
mkdir -p "$stage"
bsdtar -xf "$pkg" -C "$stage" --exclude '.*'      # --exclude '.*' drops the package's own metadata files
cat > "$stage/install.sh" <<'I'
#!/usr/bin/env sh
# Copy CaptainFM into /usr/local (or PREFIX=... ./install.sh). It does not make CaptainFM the default file manager.
set -eu
p="${PREFIX:-/usr/local}"
here="$(cd "$(dirname "$0")" && pwd)"
install -Dm755 "$here/usr/bin/captainfm" "$p/bin/captainfm"
ln -sf captainfm "$p/bin/cfm"
install -Dm644 "$here/usr/share/applications/captainfm.desktop" "$p/share/applications/captainfm.desktop"
install -Dm644 "$here/usr/share/icons/hicolor/scalable/apps/captainfm.svg" "$p/share/icons/hicolor/scalable/apps/captainfm.svg"
install -Dm644 "$here/usr/share/icons/hicolor/symbolic/apps/captainfm-symbolic.svg" "$p/share/icons/hicolor/symbolic/apps/captainfm-symbolic.svg"
echo "Installed to $p. Needs Qt 6 (declarative, multimedia) and fuse3; see the README for optional tools."
I
chmod +x "$stage/install.sh"
tar -C "$work/tar" -czf "$OUT/captainfm-$VERSION-x86_64-linux.tar.gz" "captainfm-$VERSION-x86_64-linux"

# --- the AppImage ---------------------------------------------------------------------------------------------------
# Wraps the binary that is in the package (so all three formats are the same build) with Qt. It needs jxrlib and the
# tools from 10-tools.sh. appimage.sh checks --version and a headless screenshot itself.
mkdir -p "$work/bin"
bsdtar -xf "$pkg" -C "$work/bin" usr/bin/captainfm
if ! TOOLS="$TOOLS" CFM_BIN="$work/bin/usr/bin/captainfm" "$REL/scripts/appimage.sh" "$OUT"; then
  warn "the AppImage was not made; do not list it in the release notes"
fi

# --- checksums ------------------------------------------------------------------------------------------------------
# Without the AppImage the glob fails, so the second sha256sum covers the other two files.
(cd "$OUT" && sha256sum -- *.pkg.tar.zst *.tar.gz *.AppImage > SHA256SUMS 2>/dev/null || sha256sum -- *.pkg.tar.zst *.tar.gz > SHA256SUMS)
echo; ls -la "$OUT"; cat "$OUT/SHA256SUMS"
