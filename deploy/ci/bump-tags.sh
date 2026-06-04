#!/bin/sh
# Pin the kustomize image newTags to $IMAGE_TAG, but ONLY for the components
# passed in $1 (space-separated, e.g. "gateway worker") — i.e. the images that
# actually rebuilt this pipeline. Images that were skipped keep their previous
# (already-published) tag, so we never pin a tag that was never pushed.
#
# Run from the repo root. Used by the CI `bump_manifests` job.
set -eu

BUILT="${1:-}"
TAG="${IMAGE_TAG:?IMAGE_TAG must be set}"
KF="deploy/k8s/kustomization.yaml"

if [ -z "$BUILT" ]; then
  echo "bump-tags: no components to bump"
  exit 0
fi

awk -v built="$BUILT" -v tag="$TAG" '
  BEGIN { n = split(built, a, " "); for (i = 1; i <= n; i++) want[a[i]] = 1 }
  # Track which image block we are in (component = last path segment of name).
  $1 == "-" && $2 == "name:" { comp = $3; sub(/.*\//, "", comp); hit = (comp in want) }
  # Rewrite this block s newTag only when its component rebuilt.
  $1 == "newTag:" && hit { print "    newTag: \"" tag "\""; hit = 0; next }
  { print }
' "$KF" > "$KF.tmp" && mv "$KF.tmp" "$KF"

echo "bump-tags: pinned [$BUILT ] -> $TAG"
