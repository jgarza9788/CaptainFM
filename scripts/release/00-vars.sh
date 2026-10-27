#!/usr/bin/env bash
# Part 0 of the release: the variables every other part uses. This file is SOURCED, never run:
#
#   source "$(dirname "$0")/00-vars.sh"; parse_args "$@"
#
# Every variable has a default and can be changed two ways, the later one winning:
#   1. in the environment:   DEV_BRANCH=Dev2 scripts/release/30-build.sh
#   2. with a flag:          scripts/release/30-build.sh --dev-branch Dev2
# When the parts are chained (all.sh), all.sh parses the flags once and exports the result, so the parts it starts
# see the same values.
#
#   DEV         the Dev checkout (this repo).                             default: the repo this file is in
#   REL         the second checkout of `master` that releases build from. default: $DEV/../CaptainFM-release
#   TOOLS       folder with the AppImage tools (linuxdeploy, ...).        default: ~/Work/CaptainFM-tools
#   DEV_BRANCH  the Dev branch that gets merged into master.              default: the branch checked out in $DEV
#   VERSION     X.Y.Z, read from Cargo.toml (the only place it is set).   default: master's, else Dev's
#   CODENAME    the pirate codename, read from qml/Settings.qml.          default: the one in Dev
#   OUT_ROOT    where release files go.                                   default: $DEV/dist-release
#   OUT         this release's folder.                                    default: $OUT_ROOT/v$VERSION
#   SKIP_TESTS  1 = skip lint and unit tests when building.               default: 0
#   ASSUME_YES  1 = do not stop to ask (merge and publish questions).     default: 0
#
# Nothing here changes anything on disk; it only decides names and values.
set -euo pipefail

# --- where things are ---------------------------------------------------------------------------------------------
# This file lives in scripts/release/, so the repo is two folders up.
: "${DEV:=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
: "${REL:=$DEV/../CaptainFM-release}"
: "${TOOLS:=$HOME/Work/CaptainFM-tools}"
: "${OUT_ROOT:=$DEV/dist-release}"

# --- what is being released ---------------------------------------------------------------------------------------
: "${DEV_BRANCH:=$(git -C "$DEV" branch --show-current)}"
: "${SKIP_TESTS:=0}"
: "${ASSUME_YES:=0}"

# The version is read from Cargo.toml in `master` (that is what gets built); before the first merge or the
# first worktree there is no master checkout, so fall back to the Dev one. load_version is a function so that
# 20-merge.sh can call it again once the merge has changed Cargo.toml.
load_version() {
  local src="$REL"
  [[ -f "$src/Cargo.toml" ]] || src="$DEV"
  VERSION="${VERSION_OVERRIDE:-$(sed -n 's/^version = "\(.*\)"/\1/p' "$src/Cargo.toml" | head -1)}"
  OUT="${OUT_OVERRIDE:-$OUT_ROOT/v$VERSION}"
}
# An explicit VERSION (env or --version) wins over Cargo.toml; keep it in a separate variable so a reload does not lose it.
# (Exported below, so parts started by all.sh see it.)
: "${VERSION_OVERRIDE:=${VERSION:-}}"
: "${OUT_OVERRIDE:=${OUT:-}}"
load_version

# The codename is edited by hand in qml/Settings.qml, in a line like:  text: "CaptainFM v" + ... + " *Blackbeard (...) *"
# Take the first word after the asterisk. Empty if the line is not found (then pass --codename).
: "${CODENAME:=$(sed -n 's/.*Backend\.version() + " \*\([^ ]*\).*/\1/p' "$DEV/qml/Settings.qml" | head -1)}"

# --- small helpers every part uses --------------------------------------------------------------------------------
# Name of the part that is running, for messages: "30-build.sh: ..."
PART="$(basename "${0:-release}")"
say()  { echo "[$PART] $*"; }
warn() { echo "[$PART] warning: $*" >&2; }
die()  { echo "[$PART] error: $*" >&2; exit 1; }
# need <command>...: stop with a clear message if a tool is not installed.
need() { local c; for c in "$@"; do command -v "$c" >/dev/null || die "$c is not installed (run 10-tools.sh)"; done; }
# confirm <question>: ask a yes/no question; ASSUME_YES=1 answers yes. Anything but "y"/"yes" is a no.
confirm() {
  [[ "$ASSUME_YES" == 1 ]] && return 0
  local a; read -r -p "[$PART] $1 [y/N] " a
  [[ "$a" == y || "$a" == yes ]]
}
# require_master: the checkout in $REL must exist, be on master, and have no changes. Releases build from it only.
require_master() {
  [[ -d "$REL" ]] || die "$REL does not exist (run 20-merge.sh, it makes the master checkout)"
  [[ "$(git -C "$REL" branch --show-current)" == master ]] || die "$REL is not on master"
  [[ -z "$(git -C "$REL" status --porcelain)" ]] || die "$REL has uncommitted changes"
}

# --- flags ---------------------------------------------------------------------------------------------------------
# parse_args "$@": the flags every part accepts. Flags a part does not know are left in the array EXTRA_ARGS, so
# a part with its own flags (all.sh) can read them. `usage` is defined by the part (a plain text block).
EXTRA_ARGS=()
parse_args() {
  EXTRA_ARGS=()
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --dev-branch) DEV_BRANCH="$2"; shift 2 ;;
      --codename)   CODENAME="$2"; shift 2 ;;
      --version)    VERSION_OVERRIDE="$2"; load_version; shift 2 ;;
      --out)        OUT_OVERRIDE="$2"; load_version; shift 2 ;;
      --tools)      TOOLS="$2"; shift 2 ;;
      --skip-tests) SKIP_TESTS=1; shift ;;
      --yes|-y)     ASSUME_YES=1; shift ;;
      -h|--help)    if declare -F usage >/dev/null; then usage; else sed -n '2,/^set -/p' "$0" | sed '$d'; fi; exit 0 ;;
      *)            EXTRA_ARGS+=("$1"); shift ;;
    esac
  done
}
# no_extra_args: a part with no flags of its own calls this after parse_args, so a typo is an error, not ignored.
no_extra_args() { [[ ${#EXTRA_ARGS[@]} -eq 0 ]] || die "unknown argument: ${EXTRA_ARGS[0]} (try --help)"; }

# Hand the values to any part started from here (all.sh starts the others as separate processes).
# VERSION itself is not exported on purpose: a part started after the merge must read the new Cargo.toml.
export DEV REL TOOLS OUT_ROOT DEV_BRANCH CODENAME SKIP_TESTS ASSUME_YES VERSION_OVERRIDE OUT_OVERRIDE

# --- run directly: show the values ---------------------------------------------------------------------------------
# `scripts/release/00-vars.sh [flags]` changes nothing; it prints what the other parts would use, to check them.
if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  parse_args "$@"; no_extra_args
  printf '%-11s %s\n' DEV "$DEV" REL "$REL" TOOLS "$TOOLS" DEV_BRANCH "$DEV_BRANCH" VERSION "$VERSION" \
    CODENAME "${CODENAME:-(not found)}" OUT_ROOT "$OUT_ROOT" OUT "$OUT" SKIP_TESTS "$SKIP_TESTS" ASSUME_YES "$ASSUME_YES"
fi
