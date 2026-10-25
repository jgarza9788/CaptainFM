#!/usr/bin/env bash
# Regenerate the README screenshots (docs/images/*.png) from a synthetic home folder.
# Runs CaptainFM headless (offscreen Qt), so it never touches your real files, config or desktop.
# Needs: a release build, ImageMagick (magick), ffmpeg, python3 with Pillow.
set -euo pipefail
cd "$(dirname "$0")/.."
BIN="${BIN:-target/release/captainfm}"
OUT="docs/images"
W="$(mktemp -d)"
trap 'rm -rf "$W"' EXIT
H="$W/home"
mkdir -p "$OUT"

# ---- fixture: a believable home folder -------------------------------------------------------
mkdir -p "$H"/{Documents,Downloads,Music,Videos,Desktop,dotfiles,.config,.local/state/omarchy/current/theme}
P="$H/Projects/homelab"
mkdir -p "$P"/{media-stack,monitoring,proxy,backups/2025} "$H/Pictures/trip-2026-09"
truncate -s 48G "$P/media-stack/library.img"; truncate -s 3G "$P/monitoring/prometheus.db"
truncate -s 412M "$P/proxy/certs.tar"; truncate -s 96M "$P/backups/2025/old.tar.zst"
printf 'services:\n  jellyfin:\n    image: jellyfin/jellyfin\n    restart: unless-stopped\n    ports:\n      - "8096:8096"\n    volumes:\n      - ./config:/config\n      - /mnt/media:/media\n  qbittorrent:\n    image: linuxserver/qbittorrent\n    environment:\n      - PUID=1000\n' > "$P/docker-compose.yml"
printf 'services:\n  jellyfin:\n    environment:\n      - TZ=Europe/Berlin\n' > "$P/docker-compose.override.yml"
printf '# homelab\n\nCompose files for the home server.\n\n- **media-stack**: jellyfin, qbittorrent\n- **monitoring**: prometheus, glance\n' > "$P/README.md"
printf 'PUID=1000\nTZ=Europe/Berlin\n' > "$P/.env"
printf 'import urllib.request\n\ndef healthy(url):\n    return urllib.request.urlopen(url).status == 200\n' > "$P/healthcheck.py"
printf '{"widgets":[{"type":"clock"},{"type":"weather"}]}\n' > "$P/glance.json"
printf '#!/bin/sh\nset -e\ndocker compose pull\ndocker compose up -d\n' > "$P/deploy.sh"; chmod +x "$P/deploy.sh"
head -c 18500000 /dev/zero > "$P/configs-2026-10.tar.zst"
magick -size 640x360 gradient:'#2b3a67-#f4b942' -gravity center -pointsize 40 -fill white -annotate 0 'topology' "$P/topology.png"
for f in "$P"/* "$P"/.env; do touch -d "2026-10-0$((RANDOM % 9 + 1)) 1$((RANDOM % 9)):$((RANDOM % 5))$((RANDOM % 9))" "$f"; done
touch -d "2026-10-21 07:12" "$P/media-stack"; touch -d "2026-10-20 18:30" "$P/monitoring" "$P/docker-compose.yml"
i=0; for c in '#e63946' '#f4a261' '#2a9d8f' '#457b9d' '#8e44ad' '#e9c46a' '#264653' '#d62828'; do
  i=$((i+1)); magick -size 640x480 gradient:"$c-#101820" -gravity south -pointsize 28 -fill white -annotate +0+20 "IMG $i" "$H/Pictures/trip-2026-09/IMG_20260912_10$((15+i))44.jpg"
done
ffmpeg -loglevel error -y -f lavfi -i "testsrc=duration=6:size=960x540:rate=24" -f lavfi -i "sine=frequency=440:duration=6" -shortest -c:v libx264 -pix_fmt yuv420p "$H/Videos/demo-reel.mp4"
cp /usr/share/omarchy/themes/catppuccin/colors.toml "$H/.local/state/omarchy/current/theme/colors.toml" 2>/dev/null || true

export HOME="$H" XDG_CONFIG_HOME="$H/.config" XDG_DATA_HOME="$H/.local/share" XDG_CACHE_HOME="$H/.cache" XDG_STATE_HOME="$H/.local/state"
export QT_QPA_PLATFORM=offscreen CFM_SIZE=1400x860
unset WAYLAND_DISPLAY
mkdir -p "$XDG_DATA_HOME/captainfm"
printf '{"bookmarks":["%s","%s/dotfiles"],"recents":[]}' "$P" "$H" > "$XDG_DATA_HOME/captainfm/state.json"

shot() { # name folder keys [keymap]
  rm -f "$XDG_CONFIG_HOME/captainfm/session.json"
  CFM_SHOT="$W/$1.png" CFM_KEYS="$3" CFM_KEYMAP="${4:-fp}" timeout 60 "$BIN" "$2" >/dev/null 2>&1
  cp "$W/$1.png" "$OUT/$1.png"
}
settle="<F9> <F9> <F9> <F9> <F9>"

shot main "$P" "<Down> <Down> <Down> <Down> <Down> <Insert> <Insert> $settle"
shot leader "$P" "<Down> <Down> <Down> <Down> , $settle" vim
shot palette "$P" "@app.palette =spl $settle"
shot search "$H" "@search.folder =compose_ext:yml,yaml $settle $settle"
shot batch-rename "$H/Pictures/trip-2026-09" "<C-a> @file.batchRename !mode:template !clear =trip-{created:%Y-%m-%d}-{n:03}{ext} $settle"
shot icons "$H/Pictures/trip-2026-09" "@view.grid <Down> <Right> $settle"
shot video "$H/Videos" "<F9> <F9> <F9> $settle"
shot tree "$P" "@view.tree <Down> <Right> $settle"
shot context-menu "$P" "<Down> <Down> <Down> <Down> <Down> <Down> @menu.context $settle"
shot group-by "$P" "@view.group $settle"
shot miller "$P" "@view.miller <Down> <Right> <Left> $settle"

# Themes: the same view under three Omarchy themes.
for t in gruvbox rose-pine catppuccin-latte; do
  cp "/usr/share/omarchy/themes/$t/colors.toml" "$H/.local/state/omarchy/current/theme/colors.toml"
  shot "theme-$t" "$P" "<Down> <Down> <Down> <Down> $settle"
done

python3 - "$OUT" <<'PY'
import os, sys
from PIL import Image
out = sys.argv[1]
names = ["theme-gruvbox", "theme-rose-pine", "theme-catppuccin-latte"]
ims = [Image.open(f"{out}/{n}.png") for n in names]
tw = 700
th = ims[0].height * tw // ims[0].width
sheet = Image.new("RGB", (tw * 3 + 20, th), (20, 20, 24))
for i, im in enumerate(ims):
    sheet.paste(im.resize((tw, th), Image.LANCZOS), (i * (tw + 10), 0))
sheet.save(f"{out}/themes.png")
for n in names:
    os.remove(f"{out}/{n}.png")
PY
echo "wrote $OUT/*.png"
