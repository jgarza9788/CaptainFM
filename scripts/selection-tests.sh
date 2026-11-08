#!/usr/bin/env bash
# Check the multi-selection protocol (todo.md section 1) headlessly: each case feeds keys (and mouse clicks, through the
# same pane functions the mouse uses) to a fresh folder of d1/ and f01..f11, presses Delete, and compares what went to
# the trash with what should have.
#
#   ./scripts/selection-tests.sh            all cases (about 1.5 s each)
#   ./scripts/selection-tests.sh grid       only cases whose keys contain "grid"
#
# Needs a release build (`./build.sh --quick` or `cargo build --release`). Runs offscreen with its own config, data
# and trash, so it never touches your files, settings or clipboard. Rows: 0 is d1/, 1..11 are f01..f11.
set -uo pipefail
cd "$(dirname "$0")/.."
BIN="${BIN:-target/release/captainfm}"
ONLY="${1:-}"
W="$(mktemp -d)"
trap 'rm -rf "$W"' EXIT
export QT_QPA_PLATFORM=offscreen
unset WAYLAND_DISPLAY

pass=0; fail=0
case_() { # keys expected [keymap]
  local keys="$1" want="$2" keymap="${3:-fp}"
  [[ -n "$ONLY" && "$keys" != *"$ONLY"* ]] && return
  rm -rf "$W"/*; mkdir -p "$W/fx/d1" "$W/home" "$W/cfg" "$W/data" "$W/cache" "$W/state"
  for i in 01 02 03 04 05 06 07 08 09 10 11; do echo "$i" > "$W/fx/f$i"; done
  HOME="$W/home" XDG_CONFIG_HOME="$W/cfg" XDG_DATA_HOME="$W/data" XDG_CACHE_HOME="$W/cache" XDG_STATE_HOME="$W/state" \
    CFM_SIZE=1200x700 CFM_KEYMAP="$keymap" CFM_KEYS="$keys <F9> <F9> <F9>" CFM_SHOT="$W/shot.png" \
    timeout 40 "$BIN" "$W/fx" >/dev/null 2>&1
  local got
  got="$(ls "$W/data/Trash/files" 2>/dev/null | sort | tr '\n' ' ' | sed 's/ $//')"
  if [[ "$got" == "$want" ]]; then pass=$((pass + 1)); printf 'ok    %s\n' "$keys"
  else fail=$((fail + 1)); printf 'FAIL  %s\n      want: %s\n      got:  %s\n' "$keys" "$want" "$got"; fi
}
ALL="d1 f01 f02 f03 f04 f05 f06 f07 f08 f09 f10 f11"
D="<Del> !accept"

# A plain click selects only that item.
case_ "!click:2 !click:3:C !click:4:C !click:6 $D" "f06"
# Ctrl+click after a plain click keeps both.
case_ "!click:2 !click:5:C $D" "f02 f05"
# A right-click on an unselected item selects only it (and a second right-click trashes it).
case_ "!click:2 !click:3:C !rclickrow:6 !rclick:row" "f06"
# A right-click on a selected item keeps the selection.
case_ "!click:2 !click:3:C !rclickrow:3 !rclick:row" "f02 f03"
# Plain keys drop Ctrl+marks and Shift ranges.
case_ "<C-a> <Down> $D" "f01"
case_ "<Down> <C-Space> <C-Down> <C-Space> <Down> $D" "f03"
case_ "<Down> <S-Down> <S-Down> <End> $D" "f11"
case_ "<Down> <S-Down> <S-Down> <Home> $D" "d1"
case_ "<Down> <S-Down> <PageDown> $D" "f11"
# Shift ranges: arrows shrink when reversed, Page keys, Home/End.
case_ "<Down> <S-Down> <S-Down> <S-Up> $D" "f01 f02"
case_ "<Down> <S-PageDown> $D" "f01 f02 f03 f04 f05 f06 f07 f08 f09 f10 f11"
case_ "<End> <S-PageUp> $D" "$ALL"
case_ "<Down> <Down> <S-Home> $D" "d1 f01 f02"
# Ctrl+arrows move the focus; Ctrl+Space toggles in place; Ctrl+Shift+arrows add.
case_ "<Down> <C-Down> <C-Down> <C-Space> $D" "f01 f03"
case_ "<Down> <C-Space> $D" "f01"
case_ "<Down> <C-Space> <C-Down> <C-Down> <C-S-Down> <C-S-Down> $D" "f01 f03 f04 f05"
# Select all, invert, pattern.
case_ "<C-a> $D" "$ALL"
case_ "<Down> <C-Space> <C-S-a> $D" "d1 f02 f03 f04 f05 f06 f07 f08 f09 f10 f11"
case_ "@sel.pattern =f0? !accept $D" "f01 f02 f03 f04 f05 f06 f07 f08 f09"
case_ "@sel.pattern =re:^f1 !accept $D" "f10 f11"
# Esc clears a Shift-selection at once.
case_ "<Down> <S-Down> <S-Down> <Esc> $D" "f03"
# Shift+click from a fixed anchor; Ctrl+Shift+click adds a range; Shift+click after Shift+arrows.
case_ "!click:2 !click:6:S !click:4:S $D" "f02 f03 f04"
case_ "!click:2 !click:3:S !click:8:C !click:10:CS $D" "f02 f03 f08 f09 f10"
case_ "<Down> <S-Down> !click:5:S $D" "f01 f02 f03 f04 f05"
# A click on empty space drops the selection.
case_ "<Down> <C-Space> <C-Down> <C-Space> !clickempty $D" "f02"
# After a delete the cursor takes the next item (the one before it at the end).
case_ "<Down> <Down> $D $D" "f02 f03"
case_ "<End> $D $D" "f10 f11"
case_ "<Down> <S-Down> $D $D" "f01 f02 f03"
# A filter hides part of a selection: only what is shown is counted and acted on.
case_ "<C-a> @view.filter =f05 !accept $D" "f05"
# Copy keeps the selection.
case_ "<Down> <S-Down> <C-c> $D" "f01 f02"
# Grid view: Shift+arrows move by row and column.
case_ "@view.grid <Right> <S-Right> <S-Right> $D" "f01 f02 f03"
case_ "@view.grid <Right> <S-Right> <S-Left> <S-Left> $D" "d1 f01"
# Gallery: Left/Right walk the strip, Shift+Right extends, Ctrl+Right moves the focus.
case_ "@view.gallery <Right> <Right> <S-Right> $D" "f02 f03"
case_ "@view.gallery <Right> <C-Right> <C-Right> <C-Space> $D" "f01 f03"
# Vim keymap: m marks and moves down, and the marks survive a move.
case_ "j m m j $D" "f01 f02" vim
case_ "j v j j <Esc> k $D" "f01 f02 f03" vim

echo "$pass passed, $fail failed"
[[ $fail -eq 0 ]]
