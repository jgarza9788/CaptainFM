#!/usr/bin/env bash
# Records the live demo scenes for trailer02 into a folder (default ~/Videos/cfm-demo02): one mp4 per scene, each
# with a .t file holding the seconds into the recording where the scene's actions begin.
# It runs a release build on workspace 9, fullscreen, against a synthetic home folder (scripts/demo-fixture.sh), and
# drives drags through /dev/uinput (scripts/uinput_pointer.py), so it MOVES THE REAL POINTER for about a minute.
# Needs Hyprland, gpu-screen-recorder, python3. Usage: record-demo02.sh [outdir] [scene-name ...]
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT=$PWD
BIN=$ROOT/target/release/captainfm
OUT=${1:-$HOME/Videos/cfm-demo02}; shift || true
ONLY=("$@")
MON=${MON:-HDMI-A-2}
mkdir -p "$HOME/.cache"; WORK=$(mktemp -d "$HOME/.cache/cfm-demo.XXXXXX")  # not /tmp: it is a small tmpfs
trap 'rm -rf "$WORK"' EXIT
PRISTINE=$WORK/pristine H=$WORK/home
mkdir -p "$OUT" "$PRISTINE"
REAL_WS=$(hyprctl activeworkspace -j | python3 -c 'import json,sys;print(json.load(sys.stdin)["id"])')

H=$PRISTINE
source scripts/demo-fixture.sh
mkdir -p "$H/.local/share/captainfm" "$H/.config/captainfm"
printf '{"bookmarks":["%s","%s/dotfiles"],"recents":[]}' "$P" "$H" > "$H/.local/share/captainfm/state.json"
PRISTINE_P=$P
H=$WORK/home

PID_APP=""; PID_REC=""
cleanup() { [ -n "$PID_APP" ] && kill "$PID_APP" 2>/dev/null; [ -n "$PID_REC" ] && kill -INT "$PID_REC" 2>/dev/null; hyprctl dispatch "hl.dsp.focus({ workspace = \"$REAL_WS\" })" >/dev/null 2>&1 || true; }
trap 'cleanup; rm -rf "$WORK"' EXIT

wanted() { [ ${#ONLY[@]} -eq 0 ] && return 0; local s; for s in "${ONLY[@]}"; do [ "$s" = "$1" ] && return 0; done; return 1; }
now() { date +%s.%N; }

# scene <name> <start-folder> <seconds> <settings-json> <theme|-> <keys|-> <pointer-script|->
# keys run inside the app (CFM_KEYS, after a 4 s lead); a pointer script runs once the window is up.
scene() {
  local name=$1 dir=$2 dur=$3 settings=$4 theme=$5 keys=$6 ptr=$7
  wanted "$name" || return 0
  echo "== $name"
  pkill -x captainfm || true; sleep 1
  rm -rf "$H" || { sleep 1; rm -rf "$H"; }
  cp -a --sparse=always "$PRISTINE" "$H"
  echo "$settings" > "$H/.config/captainfm/settings.json"
  [ "$theme" != - ] && cp "/usr/share/omarchy/themes/$theme/colors.toml" "$H/.local/state/omarchy/current/theme/colors.toml"
  dir=${dir//@HOME/$H}; dir=${dir//@P/$H/Projects/homelab}
  local lead="<F9> <F9> <F9> <F9> <F9> <F9> <F9> <F9> <F9> <F9> <F9> <F9> <F9> <F9> <F9> <F9>"
  local env="env HOME=$H XDG_CONFIG_HOME=$H/.config XDG_DATA_HOME=$H/.local/share XDG_CACHE_HOME=$H/.cache XDG_STATE_HOME=$H/.local/state CFM_NO_RESTART=1 QT_SCALE_FACTOR=2"
  [ "$keys" != - ] && env="$env CFM_KEYS='$lead $keys'"
  hyprctl dispatch 'hl.dsp.focus({ workspace = "9" })' >/dev/null
  gpu-screen-recorder -w "$MON" -f 30 -q very_high -o "$OUT/$name.mp4" >/dev/null 2>&1 & PID_REC=$!
  sleep 1.2
  local t0; t0=$(now)
  hyprctl dispatch "hl.dsp.exec_cmd(\"$env $BIN $dir\")" >/dev/null
  until hyprctl clients -j | grep -q '"class": "captainfm"'; do sleep 0.1; done
  sleep 0.5; hyprctl dispatch 'hl.dsp.window.fullscreen()' >/dev/null; sleep 1.2
  local ready; ready=$(awk "BEGIN{print $(now)-$t0+1.2}")
  echo "$ready" > "$OUT/$name.t"
  if [ "$ptr" != - ]; then python3 scripts/uinput_pointer.py < "$ROOT/scripts/demo02/$ptr" >"$OUT/$name.ptr.log" 2>&1 || echo "pointer script failed, see $OUT/$name.ptr.log"; fi
  sleep "$dur"
  pkill -x captainfm || true; PID_APP=""
  sleep 0.5; kill -INT "$PID_REC"; wait "$PID_REC" 2>/dev/null || true; PID_REC=""
}

N='{"dragOpenDelay":0}'
S='<Down> <Down> <Down> <Down> <Down> <Down> <Up> <Up> <Down> <Down> <Down> <Down>'
scene browse   @P 5  "$N" - "$S" -
scene tree     @P 6  "$N" - "@view.tree <Down> <Right> <Down> <Right> <Down> <Down> <Down> <Down>" -
scene miller   @P 6  "$N" - "@view.miller <Down> <Right> <Right> <Down> <Left> <Down> <Right>" -
scene grid     @HOME/Pictures/trip-2026-09 5 "$N" - "@view.grid <F9> <F9> <Down> <Right> <Right> <Down> <Right>" -
scene rename   @HOME/Pictures/trip-2026-09 6 "$N" - "<C-a> @file.batchRename !mode:template !clear =trip-{created:%Y-%m-%d}-{n:03}{ext} <F9> <F9> <F9> <F9>" -
scene search   @HOME 6 "$N" - "@search.folder =co =mpo =se =_ex =t:yml,yaml <F9> <F9> <F9> <F9> <F9>" -
scene palette  @P 5 "$N" - "@app.palette =s =p =l <F9> <F9> <F9> <F9>" -
scene t-gruvbox @P 3 "$N" gruvbox "<Down> <Down> <Down> <Down>" -
scene t-rose   @P 3 "$N" rose-pine "<Down> <Down> <Down> <Down>" -
scene t-latte  @P 3 "$N" catppuccin-latte "<Down> <Down> <Down> <Down>" -
scene drop     @P 1 "$N" - - drop.ptr
scene hover    @P 1 '{"dragOpenDelay":2}' - - hover.ptr
scene miller-drag @P 1 '{"dragOpenDelay":0,"defaultView":"miller"}' - - miller.ptr
echo "recorded to $OUT"
