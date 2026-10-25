#!/usr/bin/env bash
# Build the release files (Arch package, binary tarball, checksums) from `master`, never from the Dev branch.
# Nothing is committed or pushed, and nothing is published; see docs/RELEASING.md.
#
#   scripts/release.sh [--skip-tests]
#
# Needs: the master worktree (git worktree add ../CaptainFM-release master), makepkg, cargo. Takes about 10 minutes.
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
rel="$here/../CaptainFM-release"
[[ -d "$rel" ]] || { echo "release.sh: no $rel; run: git worktree add ../CaptainFM-release master" >&2; exit 1; }
[[ "$(git -C "$rel" branch --show-current)" == master ]] || { echo "release.sh: $rel is not on master" >&2; exit 1; }
[[ -z "$(git -C "$rel" status --porcelain)" ]] || { echo "release.sh: $rel has uncommitted changes" >&2; exit 1; }

ver="$(sed -n 's/^version = "\(.*\)"/\1/p' "$rel/Cargo.toml" | head -1)"
commit="$(git -C "$rel" rev-parse --short HEAD)"
out="$here/dist-release/v$ver"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
rm -rf "$out"; mkdir -p "$out"
echo "Releasing $ver from master $commit into $out"

git -C "$rel" archive --format=tar.gz --prefix="captainfm-$ver/" -o "$work/captainfm-$ver-src.tar.gz" HEAD
cp "$here/packaging/arch/PKGBUILD" "$here/packaging/arch/captainfm.desktop" "$work/"
sed -i "s/^pkgver=.*/pkgver=$ver/" "$work/PKGBUILD"
flags=(-f --noconfirm --nodeps)   # build deps are checked below, so a missing one is a clear error, not a pacman prompt
[[ "${1:-}" == --skip-tests ]] && flags+=(--nocheck)
for tool in cargo cmake; do command -v "$tool" >/dev/null || { echo "release.sh: $tool is missing" >&2; exit 1; }; done
(cd "$work" && makepkg "${flags[@]}")

pkg="$(ls "$work"/captainfm-"$ver"-1-x86_64.pkg.tar.zst)"
cp "$pkg" "$out/"
(cd "$work" && makepkg --printsrcinfo > "$out/.SRCINFO")
cp "$work/PKGBUILD" "$out/PKGBUILD"

# Binary tarball: the same files the package installs, under one folder, plus a small install script.
stage="$work/tar/captainfm-$ver-x86_64-linux"
mkdir -p "$stage"
bsdtar -xf "$pkg" -C "$stage" --exclude '.*'
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
tar -C "$work/tar" -czf "$out/captainfm-$ver-x86_64-linux.tar.gz" "captainfm-$ver-x86_64-linux"

(cd "$out" && sha256sum -- *.pkg.tar.zst *.tar.gz > SHA256SUMS)
echo; ls -la "$out"; cat "$out/SHA256SUMS"
