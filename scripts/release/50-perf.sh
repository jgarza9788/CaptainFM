#!/usr/bin/env bash
# Part 5 of the release: the standard performance test.
#
#   scripts/release/50-perf.sh
#
# Builds the optimised `dist` binary in the Dev checkout and runs scripts/perf.py, which writes a report to docs/perf/
# (commit it on the Dev branch). It measures and compares; it never stops the release by itself. YOU compare the
# report's "Change" column with the last one: a clear, repeated slowdown or more memory is a reason to look before
# publishing. Only compare runs from the same machine.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/00-vars.sh"
parse_args "$@"; no_extra_args
need cargo python3

cd "$DEV"
cargo build --profile dist
python3 scripts/perf.py
say "report saved in docs/perf/; read its Change column, then commit it on $DEV_BRANCH"
