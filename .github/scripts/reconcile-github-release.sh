#!/usr/bin/env bash
set -euo pipefail

tag=${1:?release tag is required}
expected=${2:?expected asset directory is required}
script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
state=$(mktemp -d)
trap 'rm -rf "$state"' EXIT

if ! gh release view "$tag" --json tagName,isDraft,assets > "$state/release.json" 2>/dev/null; then
  gh release create "$tag" "$expected"/* \
    --verify-tag --draft --title "$tag" --generate-notes
  exit 0
fi

actual_tag=$(jq -r .tagName "$state/release.json")
if [[ "$actual_tag" != "$tag" ]]; then
  echo "release assets: requested tag and existing release tag disagree" >&2
  exit 1
fi

mkdir "$state/existing"
if [[ "$(jq '.assets | length' "$state/release.json")" -gt 0 ]]; then
  gh release download "$tag" --dir "$state/existing"
fi
"$script_dir/select-missing-release-assets.sh" \
  "$expected" "$state/existing" > "$state/missing"

draft=$(jq -r .isDraft "$state/release.json")
if [[ "$draft" == true ]]; then
  while IFS= read -r asset; do
    gh release upload "$tag" "$asset"
  done < "$state/missing"
elif [[ "$draft" == false ]]; then
  if [[ -s "$state/missing" ]]; then
    echo "release assets: public release is missing an expected asset" >&2
    exit 1
  fi
else
  echo "release assets: existing release has an invalid draft state" >&2
  exit 1
fi
