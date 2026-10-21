# Sourced by record-demo02.sh: builds a believable synthetic home folder in $H (sets $P to the homelab folder).
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
