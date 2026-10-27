#!/usr/bin/env bash
# Part 7 of the release: PUBLISH. This is public and cannot be fully taken back, so it is never part of a chain
# unless asked for, and it always stops for a yes first.
#
#   scripts/release/70-publish.sh [--yes]
#
# Shows the file list, SHA256SUMS and notes.md, then asks. On yes: pushes master, makes the annotated tag vX.Y.Z,
# pushes the tag and creates the GitHub release with every file in $OUT. It refuses if the tag already exists (it
# never moves or deletes a published tag); to add files to a release that exists:
#   gh release upload vX.Y.Z <files> --clobber
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/00-vars.sh"
parse_args "$@"; no_extra_args
need git gh
require_master
load_version
notes="$OUT_ROOT/notes.md"
tag="v$VERSION"
[[ -s "$OUT/SHA256SUMS" ]] || die "$OUT has no SHA256SUMS (run 30-build.sh)"
[[ -s "$notes" ]] || die "$notes is missing (run 60-notes.sh and edit it)"
! grep -q 'EDIT:' "$notes" || die "$notes still has its EDIT marker: write the 'What is new' text first"
[[ -n "$CODENAME" ]] || die "no codename (pass --codename)"
git -C "$REL" rev-parse --verify --quiet "refs/tags/$tag" >/dev/null && die "tag $tag already exists; it is never moved"

# --- show what will go out ------------------------------------------------------------------------------------------
echo "== files ==";    ls -la "$OUT"
echo "== SHA256SUMS =="; cat "$OUT/SHA256SUMS"
echo "== notes ==";    cat "$notes"
echo "== will do ==";  echo "push master; tag $tag at $(git -C "$REL" rev-parse --short HEAD); push the tag; create the GitHub release"
confirm "publish CaptainFM $tag $CODENAME now?" || die "not published"

# --- publish --------------------------------------------------------------------------------------------------------
git -C "$REL" push origin master
git -C "$REL" tag -a "$tag" -m "CaptainFM $tag $CODENAME"
git -C "$REL" push origin "$tag"
# "$OUT"/* skips dotfiles on purpose: .SRCINFO is for the AUR, not for users.
(cd "$REL" && gh release create "$tag" "$OUT"/* --title "CaptainFM $tag $CODENAME" --notes-file "$notes")
say "published $tag"
