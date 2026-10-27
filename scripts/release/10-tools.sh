#!/usr/bin/env bash
# Part 1 of the release: prepare the tools. Safe to run any number of times.
#
#   scripts/release/10-tools.sh [--tools DIR]
#
# Checks that the programs the other parts call are installed, and that the three AppImage tools (linuxdeploy,
# its Qt plugin, appimagetool) are in $TOOLS. A missing AppImage tool is downloaded and its checksum recorded in
# $TOOLS/SHA256SUMS; scripts/appimage.sh checks that file on every run, so it records what was downloaded here.
# It never installs system packages: it only tells you which one is missing.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/00-vars.sh"
parse_args "$@"; no_extra_args

# --- programs --------------------------------------------------------------------------------------------------------
# One per line: the command, then the Arch package that provides it (shown when it is missing).
missing=0
while read -r cmd pkg; do
  if command -v "$cmd" >/dev/null; then
    say "ok: $cmd"
  else
    warn "$cmd is missing (pacman -S $pkg)"; missing=1
  fi
done <<'LIST'
git git
cargo rust
cmake cmake
makepkg pacman
bsdtar libarchive
podman podman
gh github-cli
python3 python
qmake6 qt6-base
LIST

# --- jxrlib ----------------------------------------------------------------------------------------------------------
# The KDE image plugins that get bundled into the AppImage link against jxrlib; without it linuxdeploy stops.
if pacman -Q jxrlib >/dev/null 2>&1; then say "ok: jxrlib"; else warn "jxrlib is missing (pacman -S jxrlib)"; missing=1; fi

# --- gh must be logged in, or the publish part fails at the very end -------------------------------------------------
if command -v gh >/dev/null && ! gh auth status >/dev/null 2>&1; then warn "gh is not logged in (gh auth login)"; missing=1; fi

# --- AppImage tools --------------------------------------------------------------------------------------------------
# "continuous" builds: whatever is newest is downloaded, then pinned by checksum so later runs are repeatable.
mkdir -p "$TOOLS"
fetch() {   # fetch <file name> <url>: download only when the file is not there yet
  [[ -x "$TOOLS/$1" ]] && { say "ok: $1"; return; }
  say "downloading $1"
  curl -fL --progress-bar -o "$TOOLS/$1" "$2"
  chmod +x "$TOOLS/$1"
}
fetch linuxdeploy-x86_64.AppImage https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-x86_64.AppImage
fetch linuxdeploy-plugin-qt-x86_64.AppImage https://github.com/linuxdeploy/linuxdeploy-plugin-qt/releases/download/continuous/linuxdeploy-plugin-qt-x86_64.AppImage
fetch appimagetool-x86_64.AppImage https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage

# Record checksums only the first time. Rewriting them on every run would hide a tool that changed or was tampered with.
if [[ -f "$TOOLS/SHA256SUMS" ]]; then
  (cd "$TOOLS" && sha256sum -c SHA256SUMS >/dev/null) || die "$TOOLS/SHA256SUMS does not match the tools; delete the changed tool and run again"
  say "ok: tool checksums match"
else
  (cd "$TOOLS" && sha256sum -- *.AppImage > SHA256SUMS)
  say "wrote $TOOLS/SHA256SUMS"
fi

[[ "$missing" == 0 ]] || die "fix the warnings above, then run this again"
say "all tools are ready"
