#!/usr/bin/env bash
# Chain the release parts, in order, stopping at the first one that fails.
#
#   scripts/release/all.sh [--from N] [--to N] [--skip NAME]... [--publish] [common flags]
#
#   --from N / --to N   run only the parts numbered N..M (10, 20, ... 70). Default: 10 to 60.
#   --skip NAME         leave a part out; NAME is its word: tools merge build podman perf notes publish. Repeatable.
#   --publish           also run part 70 (extends --to to 70). Without it the chain stops after the notes, so
#                       you can edit them and look at the files before anything goes public.
#   common flags        --dev-branch --codename --version --out --tools --skip-tests --yes (see 00-vars.sh);
#                       they are set once here and exported to every part.
#
# Examples:
#   scripts/release/all.sh                              # tools, merge, build, podman, perf, notes
#   scripts/release/all.sh --from 30 --to 40            # rebuild and re-check only
#   scripts/release/all.sh --skip merge --skip perf     # build from a master that is already merged
#   scripts/release/70-publish.sh                       # later, by itself, after reading the notes
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$here/00-vars.sh"
parse_args "$@"

from=10; to=60; skip=()
set -- "${EXTRA_ARGS[@]}"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --from)    from="$2"; shift 2 ;;
    --to)      to="$2"; shift 2 ;;
    --skip)    skip+=("$2"); shift 2 ;;
    --publish) to=70; shift ;;
    *)         die "unknown argument: $1 (try --help)" ;;
  esac
done

# The order of the chain. Each entry is "number:word:file".
parts=(10:tools:10-tools.sh 20:merge:20-merge.sh 30:build:30-build.sh 40:podman:40-podman.sh
       50:perf:50-perf.sh 60:notes:60-notes.sh 70:publish:70-publish.sh)

for entry in "${parts[@]}"; do
  IFS=: read -r num word file <<<"$entry"
  (( num >= from && num <= to )) || continue
  if [[ " ${skip[*]:-} " == *" $word "* ]]; then say "skipping $word"; continue; fi
  say "=== $num $word ==="
  # Run as a separate process so a part's `exit` or `die` stops the chain, and its variables stay its own.
  # The common values (branch, codename, tools folder, --skip-tests, --yes, ...) travel in the environment,
  # which 00-vars.sh exported after the flags were parsed above, so nothing needs repeating here.
  "$here/$file"
done
say "chain finished (parts $from..$to)"
