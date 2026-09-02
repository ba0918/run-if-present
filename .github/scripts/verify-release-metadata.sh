#!/usr/bin/env bash
set -euo pipefail

tag=${1:?tag is required}
manifest=${2:?Cargo.toml path is required}
changelog=${3:?CHANGELOG.md path is required}
script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)

case "$tag" in
  v[0-9]*.[0-9]*.[0-9]*) version=${tag#v} ;;
  *) echo "release metadata: tag must be v<version>" >&2; exit 1 ;;
esac

if ! "$script_dir/verify-package-version.sh" "$version" "$manifest" >/dev/null 2>&1; then
  echo "release metadata: tag and Cargo version disagree" >&2
  exit 1
fi

heading="## [$version] - "
if ! awk -v heading="$heading" '
  function content(line) {
    if (line ~ /^[[:space:]]*$/) return 0
    if (line ~ /^###[#]*([[:space:]]|$)/) return 0
    if (line ~ /^\[[^]]+\]:[[:space:]]/) return 0
    return 1
  }
  /^## / {
    if ($0 == "## Unreleased") section = "unreleased"
    else if (index($0, heading) == 1 && substr($0, length(heading) + 1) ~ /^[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]$/) section = "target"
    else section = "other"
    next
  }
  section == "unreleased" && content($0) { unreleased_content = 1 }
  section == "target" && content($0) { target_content = 1 }
  END { exit unreleased_content || !target_content }
' "$changelog"; then
  echo "release metadata: changelog content was not promoted to a dated $version heading" >&2
  exit 1
fi

link_prefix="[$version]: https://"
if ! awk -v prefix="$link_prefix" '
  index($0, prefix) == 1 {
    url = substr($0, length(prefix) + 1)
    if (url ~ /^[^[:space:]]+$/) found = 1
  }
  END { exit !found }
' "$changelog"; then
  echo "release metadata: changelog has no https link for $version" >&2
  exit 1
fi

printf '%s\n' "$version"
