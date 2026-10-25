#!/usr/bin/env bash
# Build CaptainFM and install it to ~/.local/bin, with desktop entry and icon.
#
#   ./build.sh                       everything, optimised (about 6 minutes)
#   ./build.sh --quick               the faster development build (about 1 minute)
#   ./build.sh --no-plugins          without any of the built-in plugins
#   ./build.sh --without torrent,sqlite   leave some built-in plugins out
#   ./build.sh --only sqlite,system       only these built-in plugins
#   ./build.sh --dry-run ...         show the cargo command and stop
#   ./build.sh --no-checks ...       skip the checks that run first (fmt, clippy, tests)
#
# Before building it checks the code: `cargo fmt --check`, `cargo clippy` with warnings as errors, and
# the unit tests (`./run-tests.sh`). A failed check stops the build, so nothing broken is installed.
#
# Built-in plugins: torrent, calendar-contacts, sqlite, system, documents. Whatever is built in can also be
# switched off later (Settings > Plugins, the palette, or `captainfm --plugin <name> off`).
set -euo pipefail
cd "$(dirname "$0")"

ALL=(torrent calendar-contacts sqlite system documents)
PROFILE=dist
DRY=0
CHECKS=1
FEATURES=("${ALL[@]}")

has() { local x; for x in "${ALL[@]}"; do [[ "$x" == "$1" ]] && return 0; done; return 1; }
list() { local i; IFS=',' read -ra items <<< "$1"; for i in "${items[@]}"; do has "$i" || { echo "build.sh: unknown plugin '$i' (have: ${ALL[*]})" >&2; exit 2; }; echo "$i"; done; }

while [[ $# -gt 0 ]]; do
  case "$1" in
    --quick) PROFILE=release ;;
    --dry-run) DRY=1 ;;
    --no-checks) CHECKS=0 ;;
    --no-plugins) FEATURES=() ;;
    --only) shift; picked=$(list "${1:?--only needs a list}") || exit 2; mapfile -t FEATURES <<< "$picked" ;;
    --without)
      shift; picked=$(list "${1:?--without needs a list}") || exit 2; mapfile -t skip <<< "$picked"
      keep=(); for p in "${ALL[@]}"; do [[ " ${skip[*]} " == *" $p "* ]] || keep+=("$p"); done
      FEATURES=("${keep[@]}") ;;
    -h|--help) sed -n '2,16p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "build.sh: unknown option '$1' (try --help)" >&2; exit 2 ;;
  esac
  shift
done

args=(--no-default-features)
if [[ ${#FEATURES[@]} -gt 0 ]]; then
  joined=""; for p in "${FEATURES[@]}"; do joined+="plugin-$p,"; done
  args+=(--features "${joined%,}")
fi
if [[ "$PROFILE" == release ]]; then args+=(--release); else args+=(--profile "$PROFILE"); fi

echo "Building with plugins: ${FEATURES[*]:-none} ($PROFILE)"
if [[ "$DRY" == 1 ]]; then
  [[ "$CHECKS" == 1 ]] && echo "cargo fmt --check; cargo clippy --release --all-targets -- -D warnings; ./run-tests.sh"
  echo "cargo build ${args[*]}"; exit 0
fi
if [[ "$CHECKS" == 1 ]]; then
  echo "Checking: fmt, clippy, tests (skip with --no-checks)"
  cargo fmt --check || { echo "build.sh: code is not formatted; run: cargo fmt" >&2; exit 1; }
  cargo clippy --release --all-targets -- -D warnings
  ./run-tests.sh
fi
cargo build "${args[@]}"
install -Dm755 "target/$PROFILE/captainfm" "$HOME/.local/bin/captainfm"
ln -sf captainfm "$HOME/.local/bin/cfm"
"$HOME/.local/bin/captainfm" --install
echo "Installed: $HOME/.local/bin/captainfm (alias: cfm)"
echo "Make it your default folder handler with: captainfm --default   (undo: captainfm --default off)"
