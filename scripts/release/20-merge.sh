#!/usr/bin/env bash
# Part 2 of the release: merge the Dev branch into master, in the second checkout ($REL).
#
#   scripts/release/20-merge.sh [--dev-branch Dev1] [--codename Blackbeard] [--yes]
#
# Before running: the version is set in Cargo.toml (scheme 0.0.N), the codename is edited by hand in qml/Settings.qml,
# and everything on the Dev branch is committed. This part:
#   1. refuses to go on if the Dev checkout has uncommitted changes;
#   2. makes the master checkout at $REL if it is not there (git worktree);
#   3. asks you to confirm (merging Dev into master is the owner's call), then makes a --no-ff merge commit.
# It pushes nothing. If the merge reports conflicts it stops, leaving the merge open for you to resolve.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/00-vars.sh"
parse_args "$@"; no_extra_args
need git

# --- 1. the Dev checkout must be clean --------------------------------------------------------------------------------
[[ -z "$(git -C "$DEV" status --porcelain)" ]] || die "$DEV has uncommitted changes; commit them first"
git -C "$DEV" rev-parse --verify --quiet "$DEV_BRANCH" >/dev/null || die "there is no branch called $DEV_BRANCH"

# --- 2. the master checkout ---------------------------------------------------------------------------------------------
# `worktree prune` forgets a checkout that was deleted by hand, so `worktree add` does not complain about it.
if [[ ! -d "$REL" ]]; then
  git -C "$DEV" worktree prune
  git -C "$DEV" worktree add "$REL" master
fi
require_master

# --- 3. merge -------------------------------------------------------------------------------------------------------
# The version and codename shown are what will be released, so a wrong one is visible before anything is merged.
dev_version=$(git -C "$DEV" show "$DEV_BRANCH:Cargo.toml" | sed -n 's/^version = "\(.*\)"/\1/p' | head -1)
say "merging $DEV_BRANCH (version $dev_version, codename ${CODENAME:-?}) into master at $(git -C "$REL" rev-parse --short HEAD)"
git -C "$REL" log --oneline master.."$DEV_BRANCH" | head -20
confirm "merge $DEV_BRANCH into master?" || die "not merged"
git -C "$REL" merge --no-ff "$DEV_BRANCH" -m "Merge $DEV_BRANCH: CaptainFM v$dev_version ${CODENAME}" \
  || die "merge conflicts: resolve them in $REL, commit, and go on from part 30"

load_version
say "done: master is at $(git -C "$REL" rev-parse --short HEAD), version $VERSION. Nothing was pushed."
git -C "$REL" log --oneline -3
