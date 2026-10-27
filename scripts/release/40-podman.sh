#!/usr/bin/env bash
# Part 4 of the release: try the built files in a clean Arch container, away from this machine's own libraries.
#
#   scripts/release/40-podman.sh
#
# Two checks, each prints the version and then lists a screenshot drawn by a headless run (offscreen Qt,
# <F9> <F9> = open a folder, CFM_SHOT saves the window to a picture):
#   1. installs the .pkg.tar.zst with pacman -U (so a missing dependency in the PKGBUILD shows up here);
#   2. runs the AppImage after installing only the libraries the AppImage does not bundle.
# $OUT is mounted at /r (:Z relabels it for SELinux); nothing in it is changed. The first run pulls the
# archlinux image, which takes a while. Needs part 30 to have run.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/00-vars.sh"
parse_args "$@"; no_extra_args
need podman
[[ -d "$OUT" ]] || die "$OUT does not exist (run 30-build.sh first)"

# The headless run both checks use: draw /usr/share and save a picture. HOME is a scratch folder.
shot='QT_QPA_PLATFORM=offscreen CFM_SHOT=/tmp/s.png CFM_KEYS="<F9> <F9>" HOME=/tmp/h timeout 60'

# --- 1. the package -------------------------------------------------------------------------------------------------
say "package in a clean container"
podman run --rm -v "$OUT":/r:Z docker.io/library/archlinux bash -c "
  pacman -Sy --noconfirm >/dev/null &&
  pacman -U --noconfirm /r/captainfm-*.pkg.tar.zst >/dev/null &&
  captainfm --version &&
  $shot captainfm /usr/share; ls -la /tmp/s.png"

# --- 2. the AppImage ------------------------------------------------------------------------------------------------
# The libraries listed are the host ones the AppImage does not bundle (see the AppImage limits in RELEASING.md).
if ls "$OUT"/CaptainFM-*-x86_64.AppImage >/dev/null 2>&1; then
  say "AppImage in a clean container"
  podman run --rm -v "$OUT":/r:Z docker.io/library/archlinux bash -c "
    pacman -Sy --noconfirm --needed libglvnd fontconfig ttf-dejavu harfbuzz freetype2 libxkbcommon wayland libdrm mesa >/dev/null 2>&1
    /r/CaptainFM-*-x86_64.AppImage --appimage-extract-and-run --version &&
    $shot /r/CaptainFM-*-x86_64.AppImage --appimage-extract-and-run /usr/share; ls -la /tmp/s.png"
else
  warn "no AppImage in $OUT, skipped"
fi
say "both should have printed the version and listed /tmp/s.png"
