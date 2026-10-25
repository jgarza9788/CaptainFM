#!/usr/bin/env bash
# Run CaptainFM's unit tests (every `#[test]` in src/, all built-in plugins included).
#
#   ./run-tests.sh                   all tests (about 1 minute to build, 1 second to run)
#   ./run-tests.sh trash             only tests whose name contains "trash"
#   ./run-tests.sh --ignored         also the tests marked #[ignore] (known failures waiting on a todo item)
#   ./run-tests.sh --list            list the tests and stop
#
# Runs headless and away from the real clipboard, so it is safe inside a desktop session.
set -euo pipefail
cd "$(dirname "$0")"

FILTER=()
EXTRA=()
while [[ $# -gt 0 ]]; do
  case "$1" in
    --ignored) EXTRA+=(--include-ignored) ;;
    --list) EXTRA+=(--list) ;;
    -h|--help) sed -n '2,9p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    -*) echo "run-tests.sh: unknown option '$1' (try --help)" >&2; exit 2 ;;
    *) FILTER+=("$1") ;;
  esac
  shift
done

export QT_QPA_PLATFORM=offscreen
unset WAYLAND_DISPLAY
# Same profile as `./build.sh --quick`, so the two share build files.
cargo test --release -- "${FILTER[@]}" "${EXTRA[@]}"
