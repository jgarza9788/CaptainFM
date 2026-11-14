#!/usr/bin/env bash
# Check what the dialogs and file commands do to the files, headlessly: each case feeds keys to a fresh folder, then
# compares the folder, the trash and one file's content with what should be there. Companion to selection-tests.sh
# (which covers selection); this one covers the prompt bar (new, rename), copy/paste, the name-clash dialog, undo and
# redo, sorting, and batch rename.
#
#   ./scripts/ui-tests.sh             all cases (about 1.5 s each)
#   ./scripts/ui-tests.sh clash       only cases whose name contains "clash"
#
# Needs a release build (`./build.sh --quick` or `cargo build --release`). Runs offscreen with its own config, data
# and trash, so it never touches your files, settings or clipboard. Rows: 0 is d1/, 1..5 are f01..f05; d1/ holds
# its own f01 (content "inner") so a paste into it clashes. Keys: see qml/Automation.qml
# (`@command`, `=text` typed into the focused field (`_` is a space), `!accept`, `!clear`, `!clash:<left|right|enter|esc>`,
# `!mode:<replace|template|regex|case>`, `!next`).
set -uo pipefail
cd "$(dirname "$0")/.."
BIN="${BIN:-target/release/captainfm}"
ONLY="${1:-}"
W="$(mktemp -d)"
trap 'rm -rf "$W"' EXIT
export QT_QPA_PLATFORM=offscreen
unset WAYLAND_DISPLAY

pass=0; fail=0
# case_ <name> <keys> <expected>   expected = "folder;trash;content of d1/f01" (what the case leaves behind)
case_() {
  local name="$1" keys="$2" want="$3"
  [[ -n "$ONLY" && "$name" != *"$ONLY"* ]] && return
  rm -rf "$W"/*; mkdir -p "$W/fx/d1" "$W/home" "$W/cfg" "$W/data" "$W/cache" "$W/state"
  for i in 01 02 03 04 05; do echo "$i" > "$W/fx/f$i"; done
  echo inner > "$W/fx/d1/f01"
  HOME="$W/home" XDG_CONFIG_HOME="$W/cfg" XDG_DATA_HOME="$W/data" XDG_CACHE_HOME="$W/cache" XDG_STATE_HOME="$W/state" \
    CFM_SIZE=1200x700 CFM_KEYMAP=fp CFM_KEYS="$keys <F9> <F9> <F9>" CFM_SHOT="$W/shot.png" \
    timeout 40 "$BIN" "$W/fx" >/dev/null 2>&1
  local got
  got="$(cd "$W/fx" && find . -mindepth 1 | sed 's|^\./||' | sort | tr '\n' ',' | sed 's/,$//');"
  got+="$(ls "$W/data/Trash/files" 2>/dev/null | sort | tr '\n' ',' | sed 's/,$//');"
  got+="$(cat "$W/fx/d1/f01" 2>/dev/null)"
  if [[ "$got" == "$want" ]]; then pass=$((pass + 1)); printf 'ok    %s\n' "$name"
  else fail=$((fail + 1)); printf 'FAIL  %s  (%s)\n      want: %s\n      got:  %s\n' "$name" "$keys" "$want" "$got"; fi
}

BASE="d1,d1/f01,f01,f02,f03,f04,f05"
COPY_INTO_D1="<Down> <C-c> <Up> <Right> <C-v>"   # copy f01, enter d1, paste: f01 is already there

# The prompt bar: new folder, new file, rename.
case_ "new folder" "@file.newDir =made !accept" "d1,d1/f01,f01,f02,f03,f04,f05,made;;inner"
case_ "new file" "@file.newFile =note.txt !accept" "d1,d1/f01,f01,f02,f03,f04,f05,note.txt;;inner"
case_ "new file inside a folder" "<Right> @file.newFile =x !accept" "d1,d1/f01,d1/x,f01,f02,f03,f04,f05;;inner"
case_ "rename" "<Down> @file.rename !clear =renamed !accept" "d1,d1/f01,f02,f03,f04,f05,renamed;;inner"
# (After Esc a fresh prompt is opened and answered: `!accept` acts on whatever has the focus, so it proves Esc closed
# the first one and that it did not apply.)
case_ "rename: Esc keeps the name" "<Down> @file.rename !clear =renamed <Esc> @file.newFile =y !accept" "d1,d1/f01,f01,f02,f03,f04,f05,y;;inner"

# Copy and paste, undo and redo.
case_ "copy into the same folder makes a (copy)" "<Down> <C-c> <C-v>" "d1,d1/f01,f01,f01 (copy),f02,f03,f04,f05;;inner"
case_ "copy then undo removes the copy" "<Down> <C-c> <C-v> @file.undo" "$BASE;f01 (copy);inner"
case_ "delete then undo" "<Down> <Del> !accept @file.undo" "$BASE;;inner"
case_ "delete, delete, undo, undo, redo" "<Down> <Del> !accept <Down> <Del> !accept @file.undo @file.undo @file.redo" "d1,d1/f01,f02,f03,f04,f05;f01;inner"

# Sorting: reversed, the cursor's first row is the last name.
case_ "sort reversed" "@view.sortReverse <Down> <Del> !accept" "d1,d1/f01,f01,f02,f03,f04;f05;inner"

# The name-clash dialog: it starts on Keep both; left moves to Skip, then Replace; Esc cancels.
case_ "clash: default keeps both" "$COPY_INTO_D1 !clash:enter" "d1,d1/f01,d1/f01 (copy),f01,f02,f03,f04,f05;;inner"
case_ "clash: skip" "$COPY_INTO_D1 !clash:left !clash:enter" "$BASE;;inner"
case_ "clash: replace trashes the old one" "$COPY_INTO_D1 !clash:left !clash:left !clash:enter" "$BASE;f01;01"
case_ "clash: replace then undo brings the old one back" "$COPY_INTO_D1 !clash:left !clash:left !clash:enter @file.undo" "$BASE;f01 (copy);inner"
case_ "clash: Esc cancels the paste" "$COPY_INTO_D1 !clash:esc" "$BASE;;inner"
case_ "clash: cut and replace moves the file" "<Down> <C-x> <Up> <Right> <C-v> !clash:left !clash:left !clash:enter" "d1,d1/f01,f02,f03,f04,f05;f01;01"

# Batch rename (Shift+End selects f01..f05): find and replace, regex, template.
BR="<Down> <S-End> @file.batchRename"
case_ "batch rename: find and replace" "$BR =f !next =g !accept" "d1,d1/f01,g01,g02,g03,g04,g05;;inner"
case_ "batch rename: regex" "$BR !mode:regex =^f(\\d+)\$ !next =n\$1 !accept" "d1,d1/f01,n01,n02,n03,n04,n05;;inner"
case_ "batch rename: template" "$BR !mode:template !clear =x-{name}{ext} !accept" "d1,d1/f01,x-f01,x-f02,x-f03,x-f04,x-f05;;inner"
case_ "batch rename: Esc changes nothing" "$BR =f !next =g <Esc> @file.newFile =y !accept" "d1,d1/f01,f01,f02,f03,f04,f05,y;;inner"

echo
echo "passed $pass, failed $fail"
[[ "$fail" -eq 0 ]]
