#!/usr/bin/env bash
# Part 8 of the release (after 70-publish.sh): make the two AUR packages' files, with real checksums, in
# $OUT/aur/captainfm/ and $OUT/aur/captainfm-bin/. It pushes nothing: copy the files into your AUR clones, test them in
# a clean Arch container, and push by hand (docs/RELEASING.md, "The AUR").
#
#   scripts/release/80-aur.sh              needs the release published (the source package builds from the tag tarball)
#   scripts/release/80-aur.sh --no-fetch   skip the download: captainfm gets SKIP and a warning, -bin is complete
#
# captainfm      is packaging/arch/PKGBUILD with its source changed to the tag tarball on GitHub.
# captainfm-bin  installs the prebuilt tarball from the release page; its checksum comes from $OUT/SHA256SUMS.
# The PKGBUILD is edited with exact-match replacements, so if it changes shape this stops instead of writing a wrong one.
source "$(dirname "$0")/00-vars.sh"
usage() { sed -n '2,/^# The PKGBUILD/p' "$0" | sed 's/^# \{0,1\}//'; }
parse_args "$@"
FETCH=1
for a in "${EXTRA_ARGS[@]}"; do [[ "$a" == --no-fetch ]] && FETCH=0 || die "unknown argument: $a (try --help)"; done
EXTRA_ARGS=()

need python3 sha256sum
src="$DEV/packaging/arch/PKGBUILD"
tarball="captainfm-$VERSION-x86_64-linux.tar.gz"
[[ -f "$OUT/SHA256SUMS" ]] || die "$OUT/SHA256SUMS is missing (run 30-build.sh first)"
bin_sum="$(awk -v f="$tarball" '$2 == f { print $1 }' "$OUT/SHA256SUMS")"
[[ -n "$bin_sum" ]] || die "$tarball is not listed in $OUT/SHA256SUMS"

src_sum=SKIP
if [[ "$FETCH" == 1 ]]; then
  need curl
  url="https://github.com/jgarza9788/CaptainFM/archive/refs/tags/v$VERSION.tar.gz"
  say "downloading $url for its checksum"
  src_sum="$(curl -fsSL "$url" | sha256sum | cut -d' ' -f1)" || die "could not download the tag tarball (is v$VERSION published?)"
  [[ "$src_sum" != e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855 ]] || die "the download was empty"
else
  warn "--no-fetch: captainfm gets sha256sums=('SKIP'), which the AUR does not accept; run updpkgsums there"
fi

mkdir -p "$OUT/aur/captainfm" "$OUT/aur/captainfm-bin"
SRC="$src" VERSION="$VERSION" OUT="$OUT" BIN_SUM="$bin_sum" SRC_SUM="$src_sum" python3 -I - <<'PY'
import os, re
text = open(os.environ["SRC"]).read()
v, out = os.environ["VERSION"], os.environ["OUT"]

def swap(s, old, new):
    if s.count(old) != 1:
        raise SystemExit(f"80-aur.sh: expected exactly one {old!r} in packaging/arch/PKGBUILD; update this script")
    return s.replace(old, new)

# --- captainfm: build from the tag tarball (its folder is CaptainFM-<version>) -------------------------------------
s = re.sub(r"^pkgver=.*$", f"pkgver={v}", text, flags=re.M)
s = swap(s, "source=(\"$pkgname-$pkgver-src.tar.gz\" 'captainfm.desktop')",
         "source=(\"$pkgname-$pkgver.tar.gz::https://github.com/jgarza9788/CaptainFM/archive/refs/tags/v$pkgver.tar.gz\")")
s = swap(s, "sha256sums=('SKIP' 'SKIP')", f"sha256sums=('{os.environ['SRC_SUM']}')")
s = s.replace('cd "$pkgname-$pkgver"', 'cd "CaptainFM-$pkgver"')
s = swap(s, '"$srcdir/captainfm.desktop"', 'packaging/arch/captainfm.desktop')
s = re.sub(r"^# Built by scripts/release.*\n", "", s, flags=re.M)
open(f"{out}/aur/captainfm/PKGBUILD", "w").write(s)

# --- captainfm-bin: the prebuilt tarball ---------------------------------------------------------------------------
deps = re.search(r"^# ttf-font:.*?^depends=.*?\n", text, flags=re.S | re.M).group(0)
opt = re.search(r"^optdepends=\(.*?\)\n", text, flags=re.S | re.M).group(0)
maint = re.search(r"^# Maintainer:.*\n", text, flags=re.M).group(0)
binpkg = f"""{maint}pkgname=captainfm-bin
pkgver={v}
pkgrel=1
pkgdesc='Fast, keyboard-first file manager for Omarchy (Qt6/QML, Rust), prebuilt'
arch=('x86_64')
url='https://github.com/jgarza9788/CaptainFM'
license=('MIT')
{deps}{opt}provides=('captainfm')
conflicts=('captainfm')
source=("captainfm-$pkgver-x86_64-linux.tar.gz::https://github.com/jgarza9788/CaptainFM/releases/download/v$pkgver/captainfm-$pkgver-x86_64-linux.tar.gz")
sha256sums=('{os.environ['BIN_SUM']}')

package() {{
  cd "captainfm-$pkgver-x86_64-linux"
  install -Dm755 usr/bin/captainfm "$pkgdir/usr/bin/captainfm"
  ln -s captainfm "$pkgdir/usr/bin/cfm"
  install -Dm644 usr/share/applications/captainfm.desktop "$pkgdir/usr/share/applications/captainfm.desktop"
  install -Dm644 usr/share/icons/hicolor/scalable/apps/captainfm.svg "$pkgdir/usr/share/icons/hicolor/scalable/apps/captainfm.svg"
  install -Dm644 usr/share/icons/hicolor/symbolic/apps/captainfm-symbolic.svg "$pkgdir/usr/share/icons/hicolor/symbolic/apps/captainfm-symbolic.svg"
  install -Dm644 usr/share/licenses/captainfm/LICENSE "$pkgdir/usr/share/licenses/captainfm-bin/LICENSE"
}}
"""
open(f"{out}/aur/captainfm-bin/PKGBUILD", "w").write(binpkg)
PY

if command -v makepkg >/dev/null; then
  for p in captainfm captainfm-bin; do (cd "$OUT/aur/$p" && makepkg --printsrcinfo > .SRCINFO); done
else
  warn "makepkg is not installed: no .SRCINFO written (run: makepkg --printsrcinfo > .SRCINFO in each folder)"
fi
say "wrote $OUT/aur/captainfm and $OUT/aur/captainfm-bin; nothing was pushed"
say "next: copy them into your AUR clones, test in a clean Arch container (makepkg -si), then git push"
