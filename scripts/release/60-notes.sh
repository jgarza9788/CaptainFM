#!/usr/bin/env bash
# Part 6 of the release: draft the release notes.
#
#   scripts/release/60-notes.sh
#
# Writes $OUT_ROOT/notes.md: the commits since the last tag (as a starting point, not the final text), the limits
# of each file, an install line for each file that exists in $OUT, and the rclone link. It is a DRAFT: edit it by
# hand (what is new, in plain words) before publishing. It overwrites an earlier draft only after asking.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/00-vars.sh"
parse_args "$@"; no_extra_args
require_master
load_version
notes="$OUT_ROOT/notes.md"
[[ ! -e "$notes" ]] || confirm "$notes exists; overwrite it?" || die "kept the old $notes"
mkdir -p "$OUT_ROOT"

# The newest tag, or nothing on a first release. `sort -V` so v0.0.10 comes after v0.0.9.
last="$(git -C "$REL" tag --list 'v*' | sort -V | tail -1)"
range="${last:+$last..}master"

{
  echo "# CaptainFM v$VERSION ${CODENAME}"
  echo
  echo "## What is new"
  echo "<!-- EDIT: write this in plain words. The commits since ${last:-the start} are listed at the bottom. -->"
  echo
  echo "## Install"
  # Only list files that were really made (the AppImage may be missing).
  ls "$OUT"/*.pkg.tar.zst >/dev/null 2>&1 && echo "- Arch Linux: \`sudo pacman -U captainfm-$VERSION-1-x86_64.pkg.tar.zst\`"
  ls "$OUT"/*.tar.gz >/dev/null 2>&1 && echo "- Tarball: unpack, then \`./install.sh\` (into /usr/local, or \`PREFIX=~/.local ./install.sh\`). It links against this machine's Qt, so it only suits a similar Qt."
  ls "$OUT"/*.AppImage >/dev/null 2>&1 && echo "- AppImage: \`chmod +x CaptainFM-$VERSION-x86_64.AppImage && ./CaptainFM-$VERSION-x86_64.AppImage\`"
  echo "- Cloud drives (Google Drive, Dropbox, ...) need rclone: https://rclone.org/install/"
  echo "- \`SHA256SUMS\` has the checksums of all files."
  echo
  echo "## Limits"
  echo "- Made for Arch Linux with Omarchy and Hyprland (Wayland, Qt 6.12). X11, other desktops and other CPUs are untested."
  echo "- The AppImage was built on a rolling distro and needs glibc 2.44 or newer; it does not start on Debian 13."
  echo "- The AppImage does not bundle libGL, libEGL, fontconfig, freetype, harfbuzz, libxkbcommon, libwayland-client, libdrm, libgbm or a font."
  echo
  echo "<!-- Commits since ${last:-the start} (delete this block when the text above is done):"
  git -C "$REL" log "$range" --oneline --no-merges
  echo "-->"
} > "$notes"
say "wrote $notes; edit it before publishing"
