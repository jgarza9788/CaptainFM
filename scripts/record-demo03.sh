#!/usr/bin/env bash
# Records the live scenes for trailer03 into a folder (default ~/Videos/cfm-demo03): one mp4 per scene, plus
#   <scene>.log   what the app printed with CFM_DEBUG=1, including "[keys] <ms since start> <key>" for every key it was fed
#   <scene>.meta  "exec <seconds from recording start to launching the app>"
# It runs the release build on workspace 9 against a synthetic home folder (scripts/demo-fixture.sh, demo-fixture03.sh and
# scripts/make-big-archive.py), so none of your files show. Fullscreen scenes use QT_SCALE_FACTOR=1.6 so the interface
# reads well at 4K; the window-manager and speed scenes run tiled at the normal size.
# It MOVES THE REAL POINTER (drag scene, through /dev/uinput) and switches to workspace 9 for a few minutes, then back.
# Needs Hyprland, gpu-screen-recorder, python3, ffmpeg, imagemagick. Usage: record-demo03.sh [outdir] [scene ...]
# Scenes: launch search keys split cargo spyglass dnd wm race
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT=$PWD
BIN=$ROOT/target/release/captainfm
OUT=${1:-$HOME/Videos/cfm-demo03}; shift || true
ONLY=("$@")
MON=${MON:-HDMI-A-2}
BASE=$HOME/.cache/cfm-demo03; mkdir -p "$BASE"
PRISTINE=$BASE/pristine H=$BASE/home
BIG=$BASE/big
mkdir -p "$OUT"
REAL_WS=$(hyprctl activeworkspace -j | python3 -c 'import json,sys;print(json.load(sys.stdin)["id"])')

[ -d "$BIG" ] || python3 scripts/make-big-archive.py "$BIG"
if [ ! -d "$PRISTINE/Showcase" ]; then
  rm -rf "$PRISTINE"; mkdir -p "$PRISTINE"; H=$PRISTINE
  source scripts/demo-fixture.sh; source scripts/demo-fixture03.sh
  mkdir -p "$H/.local/share/captainfm" "$H/.config/captainfm"
  printf '{"bookmarks":["%s","%s/dotfiles"],"recents":[]}' "$P" "$H" > "$H/.local/share/captainfm/state.json"
  H=$BASE/home
fi

PID_APP=""; PID_REC=""
cleanup() {
  pkill -x captainfm 2>/dev/null || true; pkill -x nautilus 2>/dev/null || true
  [ -n "$PID_REC" ] && kill -INT "$PID_REC" 2>/dev/null || true
  for c in $(hyprctl clients -j | python3 -c 'import json,sys;[print(c["address"]) for c in json.load(sys.stdin) if c["workspace"]["id"]==9]'); do
    hyprctl dispatch "hl.dsp.window.close({ window = \"address:$c\" })" >/dev/null 2>&1 || true
  done
  hyprctl dispatch "hl.dsp.focus({ workspace = \"$REAL_WS\" })" >/dev/null 2>&1 || true
}
trap 'cleanup' EXIT

# Runs a command on the compositor side. Lua strings use backslash as an escape, so double each one (keys like <C-\>).
exec9() { local c=${1//\\/\\\\}; hyprctl dispatch "hl.dsp.exec_cmd(\"$c\")" >/dev/null; }

wanted() { [ ${#ONLY[@]} -eq 0 ] && return 0; local s; for s in "${ONLY[@]}"; do [ "$s" = "$1" ] && return 0; done; return 1; }
now() { date +%s.%N; }
n9() { hyprctl clients -j | python3 -c 'import json,sys;print(sum(1 for c in json.load(sys.stdin) if c["workspace"]["id"]==9))'; }

# A fresh copy of the demo home (with the big archive when asked), settings and theme.
fresh_home() {
  local settings=$1 archive=$2
  rm -rf "$H" || { sleep 1; rm -rf "$H"; }
  cp -a --sparse=always "$PRISTINE" "$H"
  [ "$archive" = yes ] && cp -al "$BIG" "$H/Archive"
  echo "$settings" > "$H/.config/captainfm/settings.json"
}

app_env() {
  echo "env HOME=$H XDG_CONFIG_HOME=$H/.config XDG_DATA_HOME=$H/.local/share XDG_CACHE_HOME=$H/.cache XDG_STATE_HOME=$H/.local/state CFM_NO_RESTART=1 CFM_DEBUG=1 QT_FORCE_STDERR_LOGGING=1 $1"
}

record_start() { # <name> <cursor yes|no>
  gpu-screen-recorder -w "$MON" -f 30 -q very_high -cursor "$2" -o "$OUT/$1.mp4" >/dev/null 2>&1 & PID_REC=$!
  sleep 1.5; REC_T0=$(now)
}
record_stop() { sleep "$1"; kill -INT "$PID_REC"; wait "$PID_REC" 2>/dev/null || true; PID_REC=""; }

# scene <name> <start-folder> <seconds after the keys start> <keys|-> <pointer-script|-> <cursor yes|no> [archive yes|no]
scene() {
  local name=$1 dir=$2 dur=$3 keys=$4 ptr=$5 cursor=$6 archive=${7:-no}
  wanted "$name" || return 0
  echo "== $name"
  pkill -x captainfm || true; sleep 1
  fresh_home '{"dragOpenDelay":0}' "$archive"
  dir=${dir//@HOME/$H}; dir=${dir//@P/$H/Projects/homelab}
  local env; env=$(app_env "QT_SCALE_FACTOR=1.6")
  [ "$keys" != - ] && env="$env CFM_KEYS='$keys'"
  hyprctl dispatch 'hl.dsp.focus({ workspace = "9" })' >/dev/null
  record_start "$name" "$cursor"
  local t0; t0=$(now)
  exec9 "$env $BIN $dir 2> $OUT/$name.log"
  echo "exec $(awk "BEGIN{print $t0-$REC_T0}")" > "$OUT/$name.meta"
  until hyprctl clients -j | grep -q '"class": "captainfm"'; do sleep 0.05; done
  hyprctl dispatch 'hl.dsp.window.fullscreen()' >/dev/null
  if [ "$ptr" != - ]; then sleep 1.5; echo "ptr $(awk "BEGIN{print $(now)-$REC_T0}")" >> "$OUT/$name.meta"; python3 scripts/uinput_pointer.py < "$ROOT/scripts/demo03/$ptr" >"$OUT/$name.ptr.log" 2>&1 || echo "pointer script failed, see $OUT/$name.ptr.log"; fi
  # The keys run at 4 a second after the window is up; "dur" is the quiet tail after the last one.
  local nk=0; [ "$keys" != - ] && nk=$(wc -w <<< "$keys")
  record_stop "$(awk "BEGIN{print 0.6 + $nk * 0.25 + $dur}")"
  pkill -x captainfm || true; sleep 0.5
}

L8='<F9> <F9> <F9> <F9> <F9> <F9> <F9> <F9>'
scene launch   @P 1.5 '<F9> <Right> <Right> <Left> <Left> <Down> <Down> <Down> <Down> <Down> <Down> <Down> <Down> <Down> <Down> <Down>' - no
scene search   @HOME 3.5 '<F9> <F9> <F9> <F9> <C-f> <F9> =i =n =v =o =i =c =e =_ =2 =0 =2 =4 <F9> <F9> <Down> <Down>' - no yes
scene keys     @P 2 '<F9> <F9> <Down> <Down> <Right> <Down> <Left> <C-p> <F9> =t =r =e =e <F9> !accept <F9> <Down> <Down> <Down>' - no
scene split    @P 2 "<F9> <F9> <C-\\> <F9> <F9> <Tab> <C-l> =$H/Pictures/trip-2026-09 !accept <F9> <F9> <F9> <Tab> <F9> <C-t> <F9> <C-l> =$H/Showcase !accept <F9> <F9> <C-Tab> <F9> <F9> <C-Tab> <F9>" - no
scene cargo    @HOME/Pictures/trip-2026-09 2 '<F9> <F9> <C-\> <F9> <Tab> <A-Up> <F9> <Tab> <F9> <S-Down> <S-Down> <S-Down> <S-Down> <F9> <F9> <F8> <F9> <F9> <F9>' - no
scene spyglass @HOME/Showcase 1.2 '<F9> <F9> <F9> <F9> <Down> <F9> <F9> <F9> <Down> <F9> <F9> <F9> <Down> <F9> <C-m> <F9> <F9> <F9> <F9> <F9> <F9> <F9> <F9>' - no
scene dnd      @P 1.5 - dnd.ptr yes

# window-manager scene: CaptainFM, a terminal and a browser tiled side by side.
if wanted wm; then
  echo "== wm"
  pkill -x captainfm || true; sleep 1
  fresh_home '{"dragOpenDelay":0}' no
  printf '<!doctype html><meta charset=utf-8><title>homelab docs</title><body style="margin:0;background:#0f1722;color:#d7e3f0;font:28px/1.5 sans-serif;padding:70px"><h1 style="color:#6fd3cc">Homelab runbook</h1><p>Compose files live in <code>Projects/homelab</code>.</p><ul><li>media-stack: jellyfin, qbittorrent</li><li>monitoring: prometheus, glance</li><li>backups: nightly, 14 days</li></ul></body>' > "$BASE/page.html"
  rm -rf "$BASE/chrome"
  hyprctl dispatch 'hl.dsp.focus({ workspace = "9" })' >/dev/null
  record_start wm no
  t0=$(now)
  env=$(app_env "CFM_KEYS='<F9> <F9> <F9> <F9> <F9> <F9> <F9> <F9> <F9> <F9> <F9> <F9> <F9> <F9> <F9> <F9> <Down> <Down> <Down> <Right> <Down> <Down> <Left> <Down> <Down> <Down> <Down>'")
  exec9 "$env $BIN $H/Projects/homelab 2> $OUT/wm.log"
  echo "exec $(awk "BEGIN{print $t0-$REC_T0}")" > "$OUT/wm.meta"
  until [ "$(n9)" -ge 1 ]; do sleep 0.05; done
  exec9 "alacritty --working-directory $H/Projects/homelab -e bash -c 'clear; ls --color=always -lh; exec bash'"
  until [ "$(n9)" -ge 2 ]; do sleep 0.05; done
  exec9 "chromium --user-data-dir=$BASE/chrome --no-first-run --no-default-browser-check --app=file://$BASE/page.html"
  until [ "$(n9)" -ge 3 ]; do sleep 0.05; done
  # The scrolling layout gives every window its own column: stack the browser under the terminal on the right, CaptainFM wide on the left.
  sleep 1.2
  hyprctl dispatch 'hl.dsp.layout("consume_or_expel prev")' >/dev/null; sleep 0.4
  hyprctl dispatch 'hl.dsp.layout("colresize 0.4")' >/dev/null; sleep 0.4
  hyprctl dispatch 'hl.dsp.layout("focus l")' >/dev/null; sleep 0.4
  hyprctl dispatch 'hl.dsp.layout("colresize 0.6")' >/dev/null
  sleep 2.5; hyprctl clients -j | python3 -c 'import json,sys;[print(c["class"],c["at"],c["size"],c["floating"],c["workspace"]["id"]) for c in json.load(sys.stdin) if c["workspace"]["id"]==9]' > "$OUT/wm.clients"
  record_stop 5
  cleanup
fi

# speed scene: CaptainFM and Nautilus open the same 100,000-file folder at the same moment.
if wanted race; then
  echo "== race"
  pkill -x captainfm || true; pkill -x nautilus || true; sleep 1
  fresh_home '{"dragOpenDelay":0}' yes
  hyprctl dispatch 'hl.dsp.focus({ workspace = "9" })' >/dev/null
  record_start race no
  t0=$(now)
  exec9 "$(app_env '') $BIN $H/Archive/everything 2> $OUT/race.log"
  exec9 "env HOME=$H XDG_CONFIG_HOME=$H/.config XDG_DATA_HOME=$H/.local/share XDG_CACHE_HOME=$H/.cache nautilus --new-window $H/Archive/everything"
  echo "exec $(awk "BEGIN{print $t0-$REC_T0}")" > "$OUT/race.meta"
  record_stop 9
  cleanup
fi
echo "recorded to $OUT"
